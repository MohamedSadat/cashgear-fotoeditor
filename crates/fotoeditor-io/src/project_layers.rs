//! Layer-aware v2 adapters. The worker streams to an immutable backing file,
//! flushes and validates it, then atomically commits the returned candidate.
//! Reading never materializes tile samples; hydration checks each backing CRC.
use crate::{
    ProjectRecordKind as Kind, ProjectStreamReader, ProjectStreamWriter, MAX_PROJECT_BYTES,
    PROJECT_CHUNK_BYTES,
};
use fotoeditor_core::{
    validate_layers, validate_size, Document, Gray8, Layer, LayerContent, LayerMask,
    LayerProperties, Pixel, Residency, Rgba8, TileBacking, TileHandle, TilePager, TileRef,
    TileSurface, MAX_GROUP_DEPTH, MAX_LAYERS, TILE_PIXELS,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
/// Includes bounded parsed manifest/directories plus framing and bridge buffers.
pub const LAYER_PROJECT_SCRATCH: u64 = 16 * 1024 * 1024;
const MAX_MANIFEST_REFERENCES: usize = PROJECT_CHUNK_BYTES / 20;
struct BoundedManifest(Vec<u8>);
impl Write for BoundedManifest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > PROJECT_CHUNK_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other(
                "Layer manifest exceeds its 1 MiB bound",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default = "legacy_schema")]
    schema: u32,
    width: u32,
    height: u32,
    dpi: f64,
    blend_space: String,
    layers: Vec<Node>,
}
fn legacy_schema() -> u32 {
    2
}
fn full_mask() -> u8 {
    255
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    id: u32,
    name: String,
    properties: LayerProperties,
    mask: Option<Mask>,
    content: Content,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Content {
    Raster {
        tiles: Vec<TileEntry>,
    },
    Group {
        children: Vec<Node>,
    },
    Adjustment {
        settings: fotoeditor_core::Adjustment,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mask {
    #[serde(default = "full_mask")]
    default: u8,
    enabled: bool,
    tiles: Vec<TileEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TileEntry {
    x: u32,
    y: u32,
    generation: u64,
}

#[derive(Clone)]
pub enum StoredProjectTile {
    Rgba(TileHandle<Rgba8>),
    Gray(TileHandle<Gray8>),
}
impl StoredProjectTile {
    pub fn evict(&self) -> Result<(), String> {
        match self {
            Self::Rgba(t) => TilePager::default().evict(t).map(|_| ()),
            Self::Gray(t) => TilePager::default().evict(t).map(|_| ()),
        }
    }
    /// Called only with readback bytes from a flushed immutable file. Verify
    /// against the resident generation before permitting eviction.
    pub fn acknowledge(
        &self,
        store: u128,
        offset: u64,
        bytes: &[u8],
        residency: Residency,
    ) -> Result<(), String> {
        let expected = self.encode(residency)?;
        if expected != bytes {
            return Err("Tile backing readback differs from its generation".into());
        }
        let backing = TileBacking {
            store,
            offset,
            length: bytes.len() as u32,
            checksum: crc32fast::hash(bytes),
        };
        match self {
            Self::Rgba(t) => TilePager::default().acknowledge_backing(t, backing),
            Self::Gray(t) => TilePager::default().acknowledge_backing(t, backing),
        }
    }
    pub fn kind(&self) -> Kind {
        match self {
            Self::Rgba(_) => Kind::RgbaTile,
            Self::Gray(_) => Kind::GrayTile,
        }
    }
    pub fn id(&self) -> u64 {
        match self {
            Self::Rgba(t) => t.id(),
            Self::Gray(t) => t.id(),
        }
    }
    pub fn backing(&self) -> Option<TileBacking> {
        match self {
            Self::Rgba(t) => t.backing(),
            Self::Gray(t) => t.backing(),
        }
    }
    pub fn is_resident(&self) -> bool {
        match self {
            Self::Rgba(t) => t.pin().is_ok(),
            Self::Gray(t) => t.pin().is_ok(),
        }
    }
    pub fn encode(&self, residency: Residency) -> Result<Vec<u8>, String> {
        let length = match self {
            Self::Rgba(_) => TILE_PIXELS * 4,
            Self::Gray(_) => TILE_PIXELS,
        };
        residency.check_cpu(3 * length as u64)?;
        Ok(match self {
            Self::Rgba(t) => t.pin()?.pixels.iter().flat_map(|p| p.0).collect(),
            Self::Gray(t) => t.pin()?.pixels.iter().map(|p| p.0).collect(),
        })
    }
    /// Admit I/O/bridge bytes before fetching them; this check admits decoded
    /// samples too. No failed checksum or wrong-sized read can replace samples.
    pub fn hydrate(&self, bytes: &[u8], residency: Residency) -> Result<(), String> {
        let backing = self.backing().ok_or("Tile has no backing record")?;
        if bytes.len() != backing.length as usize || crc32fast::hash(bytes) != backing.checksum {
            return Err("Paged tile checksum or length failed".into());
        }
        residency.check_cpu(bytes.len() as u64)?;
        match self {
            Self::Rgba(t) => TilePager::default().hydrate(
                t,
                backing,
                bytes.as_chunks::<4>().0.iter().map(|p| Rgba8(*p)).collect(),
                residency,
            ),
            Self::Gray(t) => TilePager::default().hydrate(
                t,
                backing,
                bytes.iter().copied().map(Gray8).collect(),
                residency,
            ),
        }
    }
}
impl From<TileRef<'_>> for StoredProjectTile {
    fn from(tile: TileRef<'_>) -> Self {
        match tile {
            TileRef::Rgba(t) => Self::Rgba(t.clone()),
            TileRef::Gray(t) => Self::Gray(t.clone()),
        }
    }
}

struct ManifestBuilder {
    ids: HashMap<u64, u64>,
    tiles: Vec<StoredProjectTile>,
    node: u32,
}
impl ManifestBuilder {
    fn tiles<P: Pixel>(
        &mut self,
        surface: &TileSurface<P>,
        convert: impl Fn(&TileHandle<P>) -> StoredProjectTile,
    ) -> Vec<TileEntry> {
        let mut coordinates: Vec<_> = surface.tiles().keys().copied().collect();
        coordinates.sort_unstable();
        coordinates
            .into_iter()
            .map(|(x, y)| {
                let tile = convert(&surface.tiles()[&(x, y)]);
                let id = *self.ids.entry(tile.id()).or_insert_with(|| {
                    self.tiles.push(tile);
                    self.tiles.len() as u64
                });
                TileEntry {
                    x,
                    y,
                    generation: id,
                }
            })
            .collect()
    }
    fn nodes(&mut self, layers: &[Layer]) -> Vec<Node> {
        layers
            .iter()
            .map(|layer| {
                self.node += 1;
                let id = self.node;
                let mask = layer.mask.as_ref().map(|mask| Mask {
                    default: mask.samples.default.0,
                    enabled: mask.enabled,
                    tiles: self.tiles(&mask.samples, |t| StoredProjectTile::Gray(t.clone())),
                });
                let content = match &layer.content {
                    LayerContent::Raster(samples) => Content::Raster {
                        tiles: self.tiles(samples, |t| StoredProjectTile::Rgba(t.clone())),
                    },
                    LayerContent::Group(children) => Content::Group {
                        children: self.nodes(children),
                    },
                    LayerContent::Adjustment(settings) => Content::Adjustment {
                        settings: settings.clone(),
                    },
                };
                Node {
                    id,
                    name: layer.name.clone(),
                    properties: layer.properties.clone(),
                    mask,
                    content,
                }
            })
            .collect()
    }
}
/// Bounded signature/manifest/tile/end packets. The caller writes and releases
/// each returned packet before advancing. Hydrate `needed_tile` when cold.
pub struct LayerProjectWriter {
    framing: Option<ProjectStreamWriter>,
    manifest: Vec<u8>,
    tiles: Vec<StoredProjectTile>,
    next: usize,
    stage: u8,
    pub file_bytes: u64,
}
impl LayerProjectWriter {
    pub fn new(document: &Document, residency: Residency) -> Result<Self, String> {
        validate_size(document.width, document.height, document.dpi)?;
        validate_layers(&document.layers, document.width, document.height)?;
        // Bounded manifest plus framed output/input/bridge reservation.
        residency.check_cpu(LAYER_PROJECT_SCRATCH)?;
        let mut references = 0;
        for layer in &document.layers {
            layer.visit_tiles(&mut |_| references += 1);
        }
        if references > MAX_MANIFEST_REFERENCES {
            return Err("Layer tile directory exceeds the bounded manifest reference limit".into());
        }
        let mut builder = ManifestBuilder {
            ids: HashMap::new(),
            tiles: Vec::new(),
            node: 0,
        };
        let manifest = Manifest {
            schema: 3,
            width: document.width,
            height: document.height,
            dpi: document.dpi,
            blend_space: "srgb".into(),
            layers: builder.nodes(&document.layers),
        };
        let mut output = BoundedManifest(Vec::with_capacity(PROJECT_CHUNK_BYTES));
        serde_json::to_writer(&mut output, &manifest).map_err(|e| e.to_string())?;
        let manifest = output.0;
        let file_bytes = 12
            + 20
            + manifest.len() as u64
            + 36
            + builder
                .tiles
                .iter()
                .map(|t| {
                    20 + match t {
                        StoredProjectTile::Rgba(_) => 262144,
                        StoredProjectTile::Gray(_) => 65536,
                    }
                })
                .sum::<u64>();
        if file_bytes > MAX_PROJECT_BYTES {
            return Err("Project exceeds the 4 GiB streamed file limit".into());
        }
        Ok(Self {
            framing: Some(ProjectStreamWriter::new(residency)?),
            manifest,
            tiles: builder.tiles,
            next: 0,
            stage: 0,
            file_bytes,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        LAYER_PROJECT_SCRATCH
    }
    pub fn needed_tile(&self) -> Option<&StoredProjectTile> {
        if self.stage == 2 {
            self.tiles.get(self.next)
        } else {
            None
        }
    }
    pub fn next_packet(&mut self, residency: Residency) -> Result<Option<Vec<u8>>, String> {
        // This argument excludes this job's own reported scratch reservation.
        // Packet/payload allocations below remain inside that reservation.
        residency.check_cpu(LAYER_PROJECT_SCRATCH)?;
        let framing = self.framing.as_mut();
        match self.stage {
            0 => {
                self.stage = 1;
                Ok(Some(framing.unwrap().signature().to_vec()))
            }
            1 => {
                let bytes = framing.unwrap().record(Kind::Manifest, 0, &self.manifest)?;
                self.stage = 2;
                Ok(Some(bytes))
            }
            2 => {
                if let Some(tile) = self.tiles.get(self.next) {
                    let payload = tile.encode(residency)?;
                    let bytes =
                        framing
                            .unwrap()
                            .record(tile.kind(), self.next as u64 + 1, &payload)?;
                    self.next += 1;
                    Ok(Some(bytes))
                } else {
                    self.stage = 3;
                    self.next_packet(residency)
                }
            }
            3 => {
                self.stage = 4;
                Ok(Some(self.framing.take().unwrap().finish()?))
            }
            _ => Ok(None),
        }
    }
}

/// A parser over an immutable candidate file identified by the worker's store
/// key. The worker must finish flush/readback validation before publishing it.
pub struct LayerProjectReader {
    framing: ProjectStreamReader,
    store: u128,
    manifest: Option<Manifest>,
    tiles: HashMap<u64, StoredProjectTile>,
    failed: bool,
}
impl LayerProjectReader {
    pub fn new(file_bytes: u64, store: u128, residency: Residency) -> Result<Self, String> {
        residency.check_cpu(LAYER_PROJECT_SCRATCH)?;
        Ok(Self {
            framing: ProjectStreamReader::new(file_bytes, residency)?,
            store,
            manifest: None,
            tiles: HashMap::new(),
            failed: false,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        // Parsed JSON, names and directories reserve the bounded manifest too.
        LAYER_PROJECT_SCRATCH
    }
    pub fn feed(&mut self, input: &[u8]) -> Result<usize, String> {
        if self.failed {
            return Err("Project reader failed; discard the candidate".into());
        }
        let result = self.feed_inner(input);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn feed_inner(&mut self, input: &[u8]) -> Result<usize, String> {
        let (used, record) = self.framing.feed(input)?;
        if let Some(record) = record {
            match record.kind {
                Kind::Manifest => {
                    let manifest: Manifest = serde_json::from_slice(&record.payload)
                        .map_err(|e| format!("Invalid layer manifest: {e}"))?;
                    validate_size(manifest.width, manifest.height, manifest.dpi)?;
                    if !(2..=3).contains(&manifest.schema) {
                        return Err("Unsupported layer manifest schema".into());
                    }
                    fn compatible(nodes: &[Node], schema: u32) -> Result<(), String> {
                        for node in nodes {
                            if schema == 2 && node.mask.as_ref().is_some_and(|m| m.default != 255) {
                                return Err("Sparse mask defaults require manifest schema 3".into());
                            }
                            match &node.content {
                                Content::Adjustment { settings } => {
                                    if schema < 3 {
                                        return Err(
                                            "Adjustment layers require manifest schema 3".into()
                                        );
                                    }
                                    settings.validate()?;
                                }
                                Content::Group { children } => compatible(children, schema)?,
                                _ => {}
                            }
                        }
                        Ok(())
                    }
                    compatible(&manifest.layers, manifest.schema)?;
                    if manifest.blend_space != "srgb" {
                        return Err("Unsupported project blend space".into());
                    }
                    self.manifest = Some(manifest);
                }
                kind => {
                    let backing = TileBacking {
                        store: self.store,
                        offset: record.file_offset,
                        length: record.payload.len() as u32,
                        checksum: crc32fast::hash(&record.payload),
                    };
                    let tile = match kind {
                        Kind::RgbaTile => StoredProjectTile::Rgba(TileHandle::paged(backing)?),
                        Kind::GrayTile => StoredProjectTile::Gray(TileHandle::paged(backing)?),
                        _ => unreachable!(),
                    };
                    self.tiles.insert(record.id, tile);
                }
            }
        }
        Ok(used)
    }
    /// Returns only a validated candidate; replacing a live document is a command
    /// in the engine adapter, after the immutable backing file is durable.
    pub fn finish(self) -> Result<Document, String> {
        if self.failed {
            return Err("Layer project is corrupt".into());
        }
        self.framing.finish()?;
        let manifest = self.manifest.ok_or("Project has no layer manifest")?;
        struct Builder {
            width: u32,
            height: u32,
            tiles: HashMap<u64, StoredProjectTile>,
            used: HashSet<u64>,
            node: u32,
        }
        impl Builder {
            fn surface<P: Pixel>(
                &mut self,
                entries: Vec<TileEntry>,
                default: P,
                get: impl Fn(&StoredProjectTile) -> Option<TileHandle<P>>,
            ) -> Result<TileSurface<P>, String> {
                let mut surface = TileSurface::with_default(self.width, self.height, default);
                let mut seen = HashSet::new();
                for entry in entries {
                    if !seen.insert((entry.x, entry.y)) {
                        return Err("Duplicate layer tile coordinate".into());
                    }
                    let tile = self
                        .tiles
                        .get(&entry.generation)
                        .and_then(&get)
                        .ok_or("Missing project tile or wrong sample type")?;
                    self.used.insert(entry.generation);
                    surface.insert_handle((entry.x, entry.y), tile)?;
                }
                Ok(surface)
            }
            fn nodes(&mut self, nodes: Vec<Node>, depth: usize) -> Result<Vec<Layer>, String> {
                if depth > MAX_GROUP_DEPTH {
                    return Err("Project group nesting exceeds 32 levels".into());
                }
                let mut layers = Vec::new();
                for node in nodes {
                    self.node += 1;
                    if self.node as usize > MAX_LAYERS || node.id != self.node {
                        return Err("Invalid canonical layer IDs or layer count".into());
                    }
                    let mask = node
                        .mask
                        .map(|mask| {
                            Ok::<_, String>(LayerMask {
                                enabled: mask.enabled,
                                samples: Arc::new(self.surface(
                                    mask.tiles,
                                    Gray8(mask.default),
                                    |t| {
                                        if let StoredProjectTile::Gray(t) = t {
                                            Some(t.clone())
                                        } else {
                                            None
                                        }
                                    },
                                )?),
                            })
                        })
                        .transpose()?;
                    let mut layer = match node.content {
                        Content::Raster { tiles } => Layer::raster(
                            node.name,
                            self.surface(tiles, Rgba8::default(), |t| {
                                if let StoredProjectTile::Rgba(t) = t {
                                    Some(t.clone())
                                } else {
                                    None
                                }
                            })?,
                        ),
                        Content::Group { children } => {
                            Layer::group(node.name, self.nodes(children, depth + 1)?)
                        }
                        Content::Adjustment { settings } => Layer::adjustment(node.name, settings),
                    };
                    layer.mask = mask;
                    layer.properties = node.properties;
                    layers.push(layer);
                }
                Ok(layers)
            }
        }
        let mut builder = Builder {
            width: manifest.width,
            height: manifest.height,
            tiles: self.tiles,
            used: HashSet::new(),
            node: 0,
        };
        let document = Document {
            selection: None,
            width: manifest.width,
            height: manifest.height,
            dpi: manifest.dpi,
            layers: builder.nodes(manifest.layers, 0)?,
        };
        if builder.used.len() != builder.tiles.len() {
            return Err("Project contains unreferenced tile records".into());
        }
        validate_layers(&document.layers, document.width, document.height)?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remaining_adjustment_settings_roundtrip_with_cold_masks_and_hidden_rgb() {
        use fotoeditor_core::Adjustment;
        let controls = [
            Adjustment::Vibrance { amount: -0.4 },
            Adjustment::ColorBalance {
                shadows: [-0.2, 0.1, 0.4],
                midtones: [0.1, -0.1, 0.0],
                highlights: [0.3, 0.2, -0.1],
                preserve_luminosity: true,
            },
            Adjustment::BlackWhite {
                weights: [0.3, 0.89, 0.59, 0.7, 0.11, 0.41],
            },
            Adjustment::Invert,
            Adjustment::Threshold { level: 128 },
            Adjustment::Posterize { levels: 7 },
            Adjustment::GradientMap {
                stops: vec![
                    [0.0, 0.2, 0.0, 0.7],
                    [0.37, 1.0, 0.2, 0.0],
                    [1.0, 0.7, 1.0, 0.9],
                ],
            },
        ];
        for settings in controls {
            let mut doc = Document::blank(513, 3, 72.0).unwrap();
            doc.raster_mut().write(256, 1, Rgba8([87, 33, 19, 0]));
            let mut mask = TileSurface::new(513, 3);
            mask.write(256, 1, Gray8(128));
            let mut layer = Layer::adjustment(settings.label().into(), settings.clone());
            layer.mask = Some(LayerMask {
                enabled: true,
                samples: Arc::new(mask),
            });
            doc.layers.push(Layer::group("Group".into(), vec![layer]));
            let bytes = encode(&doc);
            let decoded = read(&bytes).unwrap();
            hydrate(&decoded, &bytes);
            assert_eq!(decoded.raster().sample(256, 1), Rgba8([87, 33, 19, 0]));
            let LayerContent::Group(children) = &decoded.layers[1].content else {
                panic!()
            };
            let LayerContent::Adjustment(actual) = &children[0].content else {
                panic!()
            };
            assert_eq!(actual, &settings);
            assert_eq!(children[0].mask.as_ref().unwrap().samples.default, Gray8(0));
            assert_eq!(
                children[0].mask.as_ref().unwrap().samples.sample(256, 1),
                Gray8(128)
            );
            assert_eq!(encode(&decoded), bytes);
        }
    }
    use fotoeditor_core::BlendMode;
    #[test]
    fn adjustment_settings_and_sparse_mask_defaults_roundtrip_without_baking_pixels() {
        let mut document = Document::blank(513, 3, 72.0).unwrap();
        document.raster_mut().write(256, 1, Rgba8([87, 33, 19, 0]));
        let settings = fotoeditor_core::Adjustment::Curves {
            points: vec![[0.0, 1.0], [0.3, 0.2], [1.0, 0.0]],
        };
        let mut layer = Layer::adjustment("Editable curves".into(), settings.clone());
        let mut mask = TileSurface::new(513, 3);
        mask.write(256, 1, Gray8(128));
        layer.mask = Some(LayerMask {
            enabled: true,
            samples: Arc::new(mask),
        });
        document.layers.push(layer);
        let bytes = encode(&document);
        let decoded = read(&bytes).unwrap();
        assert_eq!(decoded.tile_count(), 1);
        let LayerContent::Adjustment(actual) = &decoded.layers[1].content else {
            panic!()
        };
        assert_eq!(actual, &settings);
        assert_eq!(
            decoded.layers[1].mask.as_ref().unwrap().samples.default,
            Gray8(0)
        );
        assert_eq!(decoded.bytes(), 0);
        hydrate(&decoded, &bytes);
        assert_eq!(decoded.raster().sample(256, 1), Rgba8([87, 33, 19, 0]));
        assert_eq!(
            decoded.layers[1]
                .mask
                .as_ref()
                .unwrap()
                .samples
                .sample(256, 1),
            Gray8(128)
        );
        assert_eq!(encode(&decoded), bytes);
    }
    #[test]
    fn schema_two_remains_readable_and_new_or_invalid_adjustment_schemas_are_rejected() {
        fn frame(json: &str) -> Vec<u8> {
            let mut writer = ProjectStreamWriter::new(Residency::default()).unwrap();
            let mut bytes = writer.signature().to_vec();
            bytes.extend(writer.record(Kind::Manifest, 0, json.as_bytes()).unwrap());
            bytes.extend(writer.finish().unwrap());
            bytes
        }
        let legacy = r#"{"width":1,"height":1,"dpi":72,"blend_space":"srgb","layers":[{"id":1,"name":"Legacy","properties":{},"mask":{"enabled":true,"tiles":[]},"content":{"kind":"raster","tiles":[]}}]}"#;
        let doc = read(&frame(legacy)).unwrap();
        assert_eq!(
            doc.layers[0].mask.as_ref().unwrap().samples.default,
            Gray8(255)
        );
        let adjustment = legacy.replace(
            r#""kind":"raster","tiles":[]"#,
            r#""kind":"adjustment","settings":{"kind":"curves","points":[[0,1],[1,0]]}"#,
        );
        assert!(read(&frame(&adjustment)).is_err());
        let valid = adjustment.replacen('{', r#"{"schema":3,"#, 1);
        assert!(read(&frame(&valid)).is_ok());
        for invalid in [
            valid.replace("[[0,1],[1,0]]", "[[0,1],[0,0]]"),
            valid.replace("\"schema\":3", "\"schema\":4"),
            legacy.replace("\"enabled\":true", "\"enabled\":true,\"default\":0"),
        ] {
            assert!(read(&frame(&invalid)).is_err());
        }
    }
    #[test]
    fn active_session_selection_is_not_a_project_channel_or_tile_record() {
        let mut document = Document::blank(257, 2, 72.0).unwrap();
        document.raster_mut().write(256, 1, Rgba8([87, 33, 19, 0]));
        let before = encode(&document);
        let mut selection = TileSurface::new(257, 2);
        selection.write(256, 1, Gray8(128));
        document.selection = Some(Arc::new(selection));
        let after = encode(&document);
        assert_eq!(before, after);
        let decoded = read(&after).unwrap();
        assert!(decoded.selection.is_none());
    }
    #[test]
    fn arbitrary_nearby_float_metadata_roundtrips_exact_bits() {
        for n in 0..256u64 {
            let mut document =
                Document::blank(1, 1, f64::from_bits(72.0f64.to_bits() + n * 137)).unwrap();
            document.layers[0].properties.opacity = f64::from_bits(0.37f64.to_bits() + n * 147);
            document.layers[0].properties.fill = f64::from_bits(0.73f64.to_bits() + n * 173);
            let bytes = encode(&document);
            let decoded = read(&bytes).unwrap();
            assert_eq!(decoded.dpi.to_bits(), document.dpi.to_bits());
            assert_eq!(
                decoded.layers[0].properties.opacity.to_bits(),
                document.layers[0].properties.opacity.to_bits()
            );
            assert_eq!(
                decoded.layers[0].properties.fill.to_bits(),
                document.layers[0].properties.fill.to_bits()
            );
            assert_eq!(bytes, encode(&decoded));
        }
    }
    #[test]
    fn manifest_and_admission_limits_apply_before_unbounded_serialization() {
        let mut out = BoundedManifest(Vec::with_capacity(PROJECT_CHUNK_BYTES));
        out.write_all(&vec![0; PROJECT_CHUNK_BYTES]).unwrap();
        assert!(out.write_all(&[1]).is_err());
        assert_eq!(out.0.len(), PROJECT_CHUNK_BYTES);
        let full = Residency {
            temporary: fotoeditor_core::CPU_BUDGET,
            ..Residency::default()
        };
        assert!(LayerProjectWriter::new(&Document::blank(1, 1, 72.0).unwrap(), full).is_err());
        assert!(LayerProjectReader::new(1024, 1, full).is_err());
        // Directory-bound fixture only; repeated shared handles are deliberately
        // not the distinct dense 50-layer performance acceptance dataset.
        let mut document = Document::blank(16384, 16384, 72.0).unwrap();
        document.raster_mut().write(0, 0, Rgba8([1, 2, 3, 4]));
        let tile = document.raster().tiles()[&(0, 0)].clone();
        for y in 0..64 {
            for x in 0..64 {
                document
                    .raster_mut()
                    .insert_handle((x, y), tile.clone())
                    .unwrap();
            }
        }
        let samples = document.raster().clone();
        document.layers = (0..14)
            .map(|i| Layer::raster(format!("Shared {i}"), samples.clone()))
            .collect();
        assert!(LayerProjectWriter::new(&document, Residency::default()).is_err());
    }
    fn encode(document: &Document) -> Vec<u8> {
        let mut writer = LayerProjectWriter::new(document, Residency::default()).unwrap();
        let mut bytes = Vec::new();
        while let Some(packet) = writer.next_packet(Residency::default()).unwrap() {
            bytes.extend(packet);
        }
        assert_eq!(writer.file_bytes, bytes.len() as u64);
        bytes
    }
    fn read(bytes: &[u8]) -> Result<Document, String> {
        let mut reader = LayerProjectReader::new(bytes.len() as u64, 77, Residency::default())?;
        for chunk in bytes.chunks(173) {
            let mut offset = 0;
            while offset < chunk.len() {
                let used = reader.feed(&chunk[offset..])?;
                assert!(used > 0);
                offset += used;
            }
            assert_eq!(reader.temporary_bytes(), LAYER_PROJECT_SCRATCH);
        }
        reader.finish()
    }
    fn hydrate(document: &Document, bytes: &[u8]) {
        document.visit_tiles(&mut |tile| {
            let tile = StoredProjectTile::from(tile);
            let backing = tile.backing().unwrap();
            let range = backing.offset as usize..backing.offset as usize + backing.length as usize;
            tile.hydrate(&bytes[range], Residency::default()).unwrap();
        });
    }
    #[test]
    fn layers_groups_masks_properties_and_hidden_rgb_roundtrip_with_cold_samples() {
        let mut document = Document::blank(257, 1, 299.123456789).unwrap();
        document.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        document.raster_mut().write(256, 0, Rgba8([201, 3, 7, 1]));
        let mut layer = document.layers[0].clone();
        layer.id += 10000;
        layer.name = "Shared copy".into();
        layer.properties.blend = BlendMode::SoftLight;
        layer.properties.opacity = 0.37;
        layer.properties.fill = 0.73;
        layer.properties.locks.alpha = true;
        layer.properties.clipped = true;
        let mut mask = TileSurface::with_default(257, 1, Gray8(255));
        mask.write(256, 0, Gray8(127));
        layer.mask = Some(LayerMask {
            enabled: false,
            samples: Arc::new(mask),
        });
        let original = document.layers.remove(0);
        document.layers = vec![Layer::group("Group — اختبار".into(), vec![original, layer])];
        let bytes = encode(&document);
        let decoded = read(&bytes).unwrap();
        assert_eq!(decoded.bytes(), 0); // Metadata + cold handles, no sample allocation.
        assert_eq!(decoded.dpi.to_bits(), document.dpi.to_bits());
        hydrate(&decoded, &bytes);
        assert_eq!(decoded.bytes(), 2 * 262144 + 65536); // Shared duplicate counted once.
        assert_eq!(bytes, encode(&decoded));
        let LayerContent::Group(children) = &decoded.layers[0].content else {
            panic!("group");
        };
        assert_eq!(
            children[0].raster_samples().unwrap().sample(0, 0),
            Rgba8([87, 33, 19, 0])
        );
        assert_eq!(
            children[1].raster_samples().unwrap().sample(256, 0),
            Rgba8([201, 3, 7, 1])
        );
        assert_eq!(
            children[1].mask.as_ref().unwrap().samples.sample(256, 0),
            Gray8(127)
        );
    }
    #[test]
    fn corrupt_backing_cannot_hydrate_and_failed_projects_cannot_commit() {
        let mut document = Document::blank(1, 1, 72.0).unwrap();
        document.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        let bytes = encode(&document);
        let decoded = read(&bytes).unwrap();
        let tile = StoredProjectTile::Rgba(decoded.raster().tiles()[&(0, 0)].clone());
        assert!(tile
            .hydrate(&vec![0; 262144], Residency::default())
            .is_err());
        assert_eq!(decoded.bytes(), 0);
        let mut corrupt = bytes.clone();
        corrupt[bytes.len() - 1] ^= 1;
        assert!(read(&corrupt).is_err());
        assert!(read(&bytes[..bytes.len() - 1]).is_err());
        assert_eq!(document.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
    }
    #[test]
    fn semantic_manifest_validation_rejects_unknown_fields_coordinates_and_references() {
        let manifest = |tiles: &str| {
            format!(
                r#"{{"width":1,"height":1,"dpi":72,"blend_space":"srgb","layers":[{{"id":1,"name":"Layer","properties":{{}},"mask":null,"content":{{"kind":"raster","tiles":{tiles}}}}}]}}"#
            )
        };
        for json in [
            manifest(r#"[{"x":1,"y":0,"generation":1}]"#),
            manifest(r#"[{"x":0,"y":0,"generation":99}]"#),
            manifest(r#"[{"x":0,"y":0,"generation":1},{"x":0,"y":0,"generation":1}]"#),
            manifest("[]").replace("\"width\":1", "\"width\":1,\"extra\":true"),
        ] {
            let mut framing = ProjectStreamWriter::new(Residency::default()).unwrap();
            let mut bytes = framing.signature().to_vec();
            bytes.extend(framing.record(Kind::Manifest, 0, json.as_bytes()).unwrap());
            bytes.extend(framing.record(Kind::RgbaTile, 1, &vec![1; 262144]).unwrap());
            bytes.extend(framing.finish().unwrap());
            assert!(read(&bytes).is_err());
        }
    }
}
