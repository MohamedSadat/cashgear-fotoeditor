use super::*;
use fotoeditor_core::*;
use fotoeditor_io::{LayerProjectReader, LayerProjectWriter, StoredProjectTile};
use std::collections::{HashMap, HashSet};
static NEXT_IO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

#[derive(Default)]
pub(super) struct LayerRuntime {
    pub display: Option<Document>,
    pub active: Option<LayerId>,
    pub mask_target: bool,
    pub mask_view: bool,
    force_composite: bool,
    io_sessions: HashSet<u32>,
    pub mask_base_bytes: u64,
    pub dirty: Option<Vec<TileCoord>>,
    display_revision: Option<u32>,
    render: Option<LayerRenderJob>,
    incremental: bool,
    merge: Option<MergeRecipe>,
    read: Option<LayerProjectReader>,
    write: Option<LayerProjectWriter>,
    pages: HashMap<u64, StoredProjectTile>,
    gpu_coord: Option<TileCoord>,
    golden: Option<Vec<Rgba8>>,
    thumbnail: Option<ThumbnailJob>,
    memory: Option<Residency>,
}
impl LayerRuntime {
    pub fn selection_revision(&mut self, before: u32, after: u32) {
        if self.display_revision == Some(before) {
            self.display_revision = Some(after);
        }
    }
    pub fn invalidate_memory(&mut self) {
        self.memory = None;
    }
    pub fn invalidate_display(&mut self) {
        self.display_revision = None;
        self.dirty = None;
    }
    pub fn extra_source_bytes(
        &self,
        document: Option<&Document>,
        history: &History,
        preview: Option<&Document>,
        selection: &super::selection::SelectionRuntime,
        adjustment: &super::adjustments::AdjustmentRuntime,
    ) -> u64 {
        if self.render.is_none()
            && self.thumbnail.is_none()
            && self.display.is_none()
            && !selection.has_tiles()
            && adjustment.apply.is_none()
            && adjustment.histogram.is_none()
        {
            return 0;
        }
        let mut seen = HashSet::new();
        if let Some(doc) = document {
            doc.visit_tiles(&mut |t| {
                seen.insert(t.id());
            });
        }
        history.visit_tiles(&mut |t| {
            seen.insert(t.id());
        });
        if let Some(preview) = preview {
            preview.visit_tiles(&mut |t| {
                seen.insert(t.id());
            });
        }
        let mut bytes = 0;
        let mut count = |t: TileRef<'_>| {
            if seen.insert(t.id()) {
                bytes += t.bytes();
            }
        };
        if let Some(job) = &self.render {
            job.source.visit_tiles(&mut count);
            job.output.visit_tiles(&mut count);
        }
        if let Some(job) = &self.thumbnail {
            job.source.visit_tiles(&mut count);
        }
        if let Some(display) = &self.display {
            display.visit_tiles(&mut count);
        }
        selection.visit_tiles(&mut count);
        adjustment.visit_tiles(&mut count);
        bytes
    }
    pub fn cancel(&mut self) {
        self.read = None;
        self.write = None;
        self.render = None;
        self.merge = None;
        self.pages.clear();
        self.gpu_coord = None;
        self.golden = None;
        self.thumbnail = None;
        self.memory = None;
        self.io_sessions.clear();
    }
    pub fn requires_composite(&self, doc: Option<&Document>) -> bool {
        self.force_composite || self.mask_view || doc.is_some_and(|d| d.single_raster().is_err())
    }
    pub fn bytes(&self) -> u64 {
        self.mask_base_bytes
            + self.io_sessions.len() as u64 * 2 * 1024 * 1024
            + self.read.as_ref().map_or(0, |r| r.temporary_bytes())
            + self.write.as_ref().map_or(0, |r| r.temporary_bytes())
            + self.golden.as_ref().map_or(0, |p| p.capacity() as u64 * 4)
            + if self.gpu_coord.is_some() {
                4 * 1024 * 1024
            } else {
                0
            }
            + if self.thumbnail.is_some() {
                1024 * 1024
            } else {
                0
            }
    }
}
fn js(error: String) -> JsValue {
    JsValue::from_str(&error)
}
fn decode_composite(bytes: &[u8]) -> Result<Vec<Rgba8>, String> {
    if bytes.len() != TILE_PIXELS * 16 {
        return Err("Invalid GPU composite readback length".into());
    }
    bytes
        .as_chunks::<16>()
        .0
        .iter()
        .map(|pixel| {
            let channels = std::array::from_fn(|i| {
                f64::from(f32::from_le_bytes(
                    pixel[i * 4..i * 4 + 4].try_into().unwrap(),
                ))
            });
            if channels
                .iter()
                .any(|v| !v.is_finite() || *v < -0.00001 || *v > 1.00001)
            {
                return Err("GPU composite contains an invalid sample".into());
            }
            Ok(Composite(channels.map(|v| v.clamp(0.0, 1.0))).straight())
        })
        .collect()
}
fn store_id(value: &str) -> Result<u128, JsValue> {
    u128::from_str_radix(value, 16).map_err(|_| JsValue::from_str("Invalid backing store ID"))
}
fn id(value: &str) -> Result<u64, JsValue> {
    value
        .parse()
        .map_err(|_| JsValue::from_str("Invalid generation ID"))
}
fn description(tile: &StoredProjectTile) -> serde_json::Value {
    let backing = tile.backing();
    serde_json::json!({"id":tile.id().to_string(), "store":backing.map(|b|format!("{:032x}",b.store)),
        "offset":backing.map(|b|b.offset), "length":backing.map(|b|b.length)})
}
impl Engine {
    fn begin_page_io(&mut self) -> Result<u32, JsValue> {
        let mut memory = self.paging_residency();
        memory.check_cpu(2 * 1024 * 1024).map_err(js)?;
        let ticket = NEXT_IO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.layered.io_sessions.insert(ticket);
        memory.temporary += 2 * 1024 * 1024;
        self.layered.memory = Some(memory);
        Ok(ticket)
    }
    fn paging_residency(&mut self) -> Residency {
        if let Some(memory) = self.layered.memory {
            return memory;
        }
        let memory = self.residency();
        self.layered.memory = Some(memory);
        memory
    }
    pub(super) fn normalize_layer_target(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        if self.layered.active.is_some_and(|id| doc.layer(id).is_err()) {
            self.layered.active = doc.layers.last().map(|l| l.id);
            self.layered.display_revision = None;
        }
        if self.layered.active.is_none_or(|id| {
            doc.layer(id).is_err() || doc.layer(id).is_ok_and(|l| l.mask.is_none())
        }) {
            self.layered.mask_target = false;
            self.layered.mask_view = false;
        }
        self.refresh_pages();
    }
    pub(super) fn mask_preview(&self, preview: &Document) -> Result<Layer, String> {
        let target = preview.layers[0].id;
        let mut layer = self
            .document
            .as_ref()
            .ok_or("No document")?
            .layer(target)?
            .clone();
        let mask = layer.mask.as_mut().ok_or("Layer has no mask")?;
        let mut samples =
            TileSurface::with_default(preview.width, preview.height, mask.samples.default);
        for (coord, tile) in preview.raster().tiles() {
            samples.insert(
                *coord,
                tile.pin()?.pixels.iter().map(|p| Gray8(p.0[0])).collect(),
            );
        }
        let unchanged = samples.tiles().len() == mask.samples.tiles().len()
            && samples.tiles().iter().all(|(coord, tile)| {
                mask.samples
                    .tiles()
                    .get(coord)
                    .is_some_and(|old| match (tile.pin(), old.pin()) {
                        (Ok(a), Ok(b)) => a.pixels == b.pixels,
                        _ => false,
                    })
            });
        if !unchanged {
            mask.samples = std::sync::Arc::new(samples);
        }
        Ok(layer)
    }
    pub(super) fn mask_document(&self, target: LayerId) -> Result<Document, String> {
        let doc = self.document.as_ref().ok_or("No document")?;
        check_layer_edit(&doc.layers, target, false)?;
        let layer = doc.layer(target)?;
        let mask = layer.mask.as_ref().ok_or("Layer has no mask")?;
        self.residency().check_cpu(mask.samples.bytes() * 4)?;
        let value = mask.samples.default.0;
        let mut samples =
            TileSurface::with_default(doc.width, doc.height, Rgba8([value, value, value, 255]));
        for (coord, tile) in mask.samples.tiles() {
            samples.insert(
                *coord,
                tile.pin()?
                    .pixels
                    .iter()
                    .map(|p| Rgba8([p.0, p.0, p.0, 255]))
                    .collect(),
            );
        }
        let mut layer = Layer::raster("Mask stroke".into(), samples);
        layer.id = target;
        Ok(Document {
            selection: doc.selection.clone(),
            width: doc.width,
            height: doc.height,
            dpi: doc.dpi,
            layers: vec![layer],
        })
    }
    pub(super) fn quick_or_layer_mask_document(&self, target: LayerId) -> Result<Document, String> {
        if self.selected.quick_mask {
            self.quick_document()
                .map_err(|e| e.as_string().unwrap_or_else(|| "Quick Mask failed".into()))
        } else {
            self.mask_document(target)
        }
    }
    pub(super) fn refresh_pages(&mut self) {
        self.layered.memory = None;
        let mut pages = HashMap::new();
        let mut add = |t: TileRef<'_>| {
            pages
                .entry(t.id())
                .or_insert_with(|| StoredProjectTile::from(t));
        };
        if let Some(doc) = &self.document {
            doc.visit_tiles(&mut add);
        }
        self.history.visit_tiles(&mut add);
        if let Some(job) = &self.layered.render {
            job.source.visit_tiles(&mut add);
            job.output.visit_tiles(&mut add);
        }
        if let Some(stroke) = &self.stroke {
            stroke.preview.visit_tiles(&mut add);
        }
        if let Some(job) = &self.layered.thumbnail {
            job.source.visit_tiles(&mut add);
        }
        self.selected.visit_tiles(&mut add);
        self.adjusted.visit_tiles(&mut add);
        self.layered.pages = pages;
    }
    pub(super) fn layer_json(&self) -> serde_json::Value {
        fn nodes(layers: &[Layer]) -> Vec<serde_json::Value> {
            layers
                .iter()
                .map(|l| {
                    serde_json::json!({"id":l.id,"name":l.name,"properties":l.properties,
                "mask":l.mask.as_ref().map(|m|serde_json::json!({"enabled":m.enabled})),
                "children":match &l.content { LayerContent::Group(c)=>Some(nodes(c)),_=>None },
                "adjustment":match &l.content { LayerContent::Adjustment(a)=>Some(a),_=>None }})
                })
                .collect()
        }
        serde_json::json!({"nodes":self.document.as_ref().map(|d|nodes(&d.layers)).unwrap_or_default(),
            "active":self.layered.active,"maskTarget":self.layered.mask_target,"maskView":self.layered.mask_view,"compositing":self.layered.render.as_ref().map_or(0, |r|r.remaining())})
    }
}
#[wasm_bindgen]
impl Engine {
    pub fn select_layer(&mut self, target: u32) -> Result<(), JsValue> {
        self.document
            .as_ref()
            .ok_or("No document")?
            .layer(target)
            .map_err(js)?;
        self.layered.active = Some(target);
        if self.layered.mask_view {
            self.layered.display_revision = None;
        }
        self.layered.mask_target = false;
        self.layered.mask_view = false;
        self.refresh_pages();
        Ok(())
    }
    pub fn select_mask(&mut self, target: u32, view: bool) -> Result<(), JsValue> {
        self.document
            .as_ref()
            .ok_or("No document")?
            .layer(target)
            .map_err(js)?
            .mask
            .as_ref()
            .ok_or("Layer has no mask")?;
        self.layered.active = Some(target);
        self.layered.mask_target = true;
        self.layered.mask_view = view;
        self.layered.display_revision = None;
        Ok(())
    }
    pub fn mask_view_enabled(&self) -> bool {
        self.layered.mask_view
    }
    pub fn preview_mask(&mut self, enabled: bool) {
        self.layered.mask_view = enabled;
        self.layered.display_revision = None;
    }
    pub fn layer_action(&mut self, action: &str) -> Result<(), JsValue> {
        if self.stroke.is_some() {
            return Err(JsValue::from_str("Finish the stroke first"));
        }
        let action = serde_json::from_str::<LayerAction>(action).map_err(|e| js(e.to_string()))?;
        let doc = self.document.as_mut().ok_or("No document")?;
        let select_new = matches!(
            action,
            LayerAction::Add { .. }
                | LayerAction::AddAdjustment { .. }
                | LayerAction::Duplicate { .. }
                | LayerAction::Group { .. }
        );
        fn layer_ids(nodes: &[Layer], ids: &mut HashSet<LayerId>) {
            for l in nodes {
                ids.insert(l.id);
                if let LayerContent::Group(children) = &l.content {
                    layer_ids(children, ids);
                }
            }
        }
        let mut existing = HashSet::new();
        layer_ids(&doc.layers, &mut existing);
        let command = LayerCommand::prepare(doc, action).map_err(js)?;
        let changed = self
            .history
            .execute(doc, Box::new(command), self.layered.bytes())
            .map_err(js)?;
        self.revision = self.revision.wrapping_add(u32::from(changed));
        self.content_revision = self.content_revision.wrapping_add(u32::from(changed));
        self.layered.force_composite = true;
        if select_new {
            fn new_node(nodes: &[Layer], existing: &HashSet<LayerId>) -> Option<LayerId> {
                nodes.iter().rev().find_map(|l| {
                    if !existing.contains(&l.id) {
                        Some(l.id)
                    } else if let LayerContent::Group(children) = &l.content {
                        new_node(children, existing)
                    } else {
                        None
                    }
                })
            }
            self.layered.active = new_node(&doc.layers, &existing);
            self.layered.mask_target = false;
            self.layered.mask_view = false;
        }
        if self.layered.active.is_none_or(|id| doc.layer(id).is_err()) {
            self.layered.active = doc.layers.last().map(|l| l.id);
        }
        if self
            .layered
            .active
            .is_none_or(|id| doc.layer(id).is_ok_and(|l| l.mask.is_none()))
        {
            self.layered.mask_target = false;
            self.layered.mask_view = false;
        }
        self.refresh_pages();
        Ok(())
    }
    pub fn apply_mask(&mut self, target: u32) -> Result<(), JsValue> {
        let command = apply_raster_mask(
            self.document.as_ref().ok_or("No document")?,
            target,
            self.residency(),
        )
        .map_err(js)?;
        let changed = self
            .history
            .execute(
                self.document.as_mut().unwrap(),
                Box::new(command),
                self.layered.bytes(),
            )
            .map_err(js)?;
        self.revision = self.revision.wrapping_add(u32::from(changed));
        self.content_revision = self.content_revision.wrapping_add(u32::from(changed));
        self.refresh_pages();
        Ok(())
    }
    pub fn layer_is_group(&self, target: u32) -> Result<bool, JsValue> {
        Ok(matches!(
            self.document
                .as_ref()
                .ok_or("No document")?
                .layer(target)
                .map_err(js)?
                .content,
            LayerContent::Group(_)
        ))
    }
    pub fn merge_begin(&mut self, kind: &str, target: u32) -> Result<(), JsValue> {
        self.layered.incremental = false;
        let recipe = MergeRecipe::new(self.document.as_ref().ok_or("No document")?, kind, target)
            .map_err(js)?;
        self.layered.render = Some(LayerRenderJob::new(recipe.source.clone()).map_err(js)?);
        self.layered.merge = Some(recipe);
        self.refresh_pages();
        Ok(())
    }
    pub fn composite_needed(&self) -> bool {
        if !self.layered.requires_composite(self.document.as_ref()) {
            return self.layered.display.is_some();
        }
        self.layered.display_revision != Some(self.revision)
            || self
                .stroke
                .as_ref()
                .is_some_and(|s| !self.selected.quick_mask && s.preview.raster().has_dirty())
    }
    pub fn composite_begin(&mut self) -> Result<bool, JsValue> {
        if !self.layered.requires_composite(self.document.as_ref()) {
            if self.layered.display.take().is_some() {
                self.renderer.clear_tiles();
            }
            self.layered.display_revision = Some(self.revision);
            return Ok(false);
        }
        let mut doc = self.document.as_ref().ok_or("No document")?.clone();
        let mut regions = self.layered.dirty.take();
        if let Some(stroke) = self.stroke.as_ref().filter(|_| !self.selected.quick_mask) {
            let preview = if self.layered.mask_target {
                self.mask_preview(&stroke.preview).map_err(js)?
            } else {
                stroke.preview.layers[0].clone()
            };
            let target = preview.id;
            *doc.layer_mut(target).map_err(js)? = preview;
        }
        if let Some(stroke) = self.stroke.as_mut().filter(|_| !self.selected.quick_mask) {
            regions = Some(
                stroke
                    .preview
                    .raster_mut()
                    .drain_dirty()
                    .into_iter()
                    .collect(),
            );
        }
        if self.layered.mask_view {
            let layer = doc
                .layer(self.layered.active.ok_or("Choose a mask")?)
                .map_err(js)?;
            let mask = &layer.mask.as_ref().ok_or("Layer has no mask")?.samples;
            let count = u64::from(doc.width.div_ceil(TILE_SIZE))
                * u64::from(doc.height.div_ceil(TILE_SIZE));
            self.residency()
                .check_cpu(count * TILE_PIXELS as u64 * 4)
                .map_err(js)?;
            let mut samples = TileSurface::new(doc.width, doc.height);
            for y in 0..doc.height.div_ceil(TILE_SIZE) {
                for x in 0..doc.width.div_ceil(TILE_SIZE) {
                    let pixels = if let Some(tile) = mask.tiles().get(&(x, y)) {
                        tile.pin()
                            .map_err(js)?
                            .pixels
                            .iter()
                            .map(|p| Rgba8([p.0, p.0, p.0, 255]))
                            .collect()
                    } else {
                        let v = mask.default.0;
                        vec![Rgba8([v, v, v, 255]); TILE_PIXELS]
                    };
                    samples.insert((x, y), pixels);
                }
            }
            doc.layers = vec![Layer::raster("Mask view".into(), samples)];
        }
        self.layered.incremental =
            !self.layered.mask_view && self.layered.display.is_some() && regions.is_some();
        self.layered.render = Some(if !self.layered.mask_view {
            if let (Some(output), Some(regions)) = (&self.layered.display, regions) {
                LayerRenderJob::updating(doc, output.clone(), regions).map_err(js)?
            } else {
                LayerRenderJob::new(doc).map_err(js)?
            }
        } else {
            LayerRenderJob::new(doc).map_err(js)?
        });
        self.refresh_pages();
        Ok(true)
    }
    pub fn composite_step(&mut self) -> Result<bool, JsValue> {
        let r = self.residency();
        let done = self
            .layered
            .render
            .as_mut()
            .ok_or("No composite job")?
            .step(r)
            .map_err(js)?;
        if done {
            let output = self.layered.render.take().unwrap().output;
            if let Some(recipe) = self.layered.merge.take() {
                let command = recipe
                    .finish(self.document.as_ref().unwrap(), output)
                    .map_err(js)?;
                self.history
                    .execute(
                        self.document.as_mut().unwrap(),
                        Box::new(command),
                        self.layered.bytes(),
                    )
                    .map_err(js)?;
                self.revision = self.revision.wrapping_add(1);
                self.content_revision = self.content_revision.wrapping_add(1);
                self.layered.display_revision = None;
                self.layered.mask_target = false;
                self.layered.mask_view = false;
                self.layered.active = self.document.as_ref().unwrap().layers.last().map(|l| l.id);
            } else {
                self.layered.display = Some(output);
                self.layered.display_revision = Some(self.revision);
            }
            if !self.layered.incremental {
                self.renderer.clear_tiles();
            }
            self.refresh_pages();
        }
        Ok(done)
    }
    pub fn composite_gpu_begin(&mut self) -> Result<Option<js_sys::Promise>, JsValue> {
        let Some((coord, plan)) = self
            .layered
            .render
            .as_ref()
            .ok_or("No composite job")?
            .plan()
            .map_err(js)?
        else {
            return Ok(None);
        };
        self.residency().check_cpu(4 * 1024 * 1024).map_err(js)?;
        self.layered.gpu_coord = Some(coord);
        self.renderer.composite(&plan).map(Some).map_err(js)
    }
    pub fn composite_gpu_accept(&mut self, bytes: &[u8]) -> Result<bool, JsValue> {
        self.layered.memory = None;
        let coord = self.layered.gpu_coord.take().ok_or("No pending GPU tile")?;
        let pixels = decode_composite(bytes).map_err(js)?;
        let r = self.residency();
        let job = self
            .layered
            .render
            .as_mut()
            .ok_or("Composite job cancelled")?;
        job.accept(coord, pixels, r).map_err(js)?;
        if job.remaining() == 0 {
            self.composite_step()
        } else {
            Ok(false)
        }
    }
    pub fn golden_begin(
        &mut self,
        mode: u32,
        scenario: u32,
        adjustment: Option<String>,
    ) -> Result<js_sys::Promise, JsValue> {
        let mode = BlendMode::from_index(mode).map_err(js)?;
        if scenario > 3 {
            return Err(JsValue::from_str("Unknown golden scene"));
        }
        fn raster(seed: u32) -> Layer {
            let mut surface = TileSurface::new(TILE_SIZE, TILE_SIZE);
            let pixels = (0..TILE_PIXELS)
                .map(|i| {
                    let v = (i as u32)
                        .wrapping_mul(1664525)
                        .wrapping_add(seed.wrapping_mul(1013904223));
                    Rgba8([(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8])
                })
                .collect();
            surface.insert((0, 0), pixels);
            Layer::raster("Golden samples".into(), surface)
        }
        let bottom = raster(1);
        let is_adjustment = adjustment.is_some();
        let mut top = if let Some(settings) = adjustment {
            let settings: Adjustment =
                serde_json::from_str(&settings).map_err(|e| js(e.to_string()))?;
            settings.validate().map_err(js)?;
            Layer::adjustment("Adjustment golden".into(), settings)
        } else {
            raster(7)
        };
        top.properties.blend = mode;
        top.properties.opacity = 0.73;
        top.properties.fill = 0.81;
        let mut mask = TileSurface::with_default(TILE_SIZE, TILE_SIZE, Gray8(255));
        mask.insert(
            (0, 0),
            (0..TILE_PIXELS).map(|i| Gray8((i % 256) as u8)).collect(),
        );
        if scenario > 0 {
            top.mask = Some(LayerMask {
                enabled: true,
                samples: std::sync::Arc::new(mask.clone()),
            });
        }
        let layers = if scenario >= 2 {
            let children = if is_adjustment {
                vec![raster(11), top]
            } else {
                vec![top]
            };
            let mut group = Layer::group("Golden group".into(), children);
            group.properties.pass_through = scenario == 2;
            group.properties.opacity = 0.67;
            group.mask = Some(LayerMask {
                enabled: true,
                samples: std::sync::Arc::new(mask),
            });
            vec![bottom, group]
        } else {
            if scenario == 1 {
                top.properties.clipped = true;
            }
            vec![bottom, top]
        };
        let doc = Document {
            selection: None,
            width: TILE_SIZE,
            height: TILE_SIZE,
            dpi: 72.0,
            layers,
        };
        let plan = CompositePlan::tile(&doc, (0, 0), None).map_err(js)?;
        self.layered.golden = Some(
            plan.reference(self.residency())
                .map_err(js)?
                .into_iter()
                .map(Composite::straight)
                .collect(),
        );
        self.renderer.composite(&plan).map_err(js)
    }
    pub fn golden_compare(&mut self, bytes: &[u8]) -> Result<u32, JsValue> {
        let actual = decode_composite(bytes).map_err(js)?;
        let expected = self.layered.golden.take().ok_or("No golden reference")?;
        Ok(actual
            .iter()
            .zip(expected)
            .flat_map(|(a, b)| {
                a.0.into_iter()
                    .zip(b.0)
                    .map(|(a, b)| u32::from(a.abs_diff(b)))
            })
            .max()
            .unwrap_or(0))
    }
    pub fn thumbnail_begin(&mut self, target: u32) -> Result<(), JsValue> {
        self.residency().check_cpu(1024 * 1024).map_err(js)?;
        self.layered.thumbnail = Some(
            ThumbnailJob::new(self.document.as_ref().ok_or("No document")?, target).map_err(js)?,
        );
        self.refresh_pages();
        Ok(())
    }
    pub fn thumbnail_step(&mut self) -> Result<bool, JsValue> {
        self.layered
            .thumbnail
            .as_mut()
            .ok_or("No thumbnail job")?
            .step()
            .map_err(js)
    }
    pub fn thumbnail_cancel(&mut self) {
        self.layered.thumbnail = None;
        self.refresh_pages();
    }
    pub fn thumbnail_finish(&mut self) -> Result<Vec<u8>, JsValue> {
        let job = self.layered.thumbnail.take().ok_or("No thumbnail job")?;
        fotoeditor_io::encode_thumbnail(job.width, job.height, &job.pixels).map_err(js)
    }
    /// Returns only cold generations needed by the next bounded operation.
    pub fn page_requests(&mut self, scope: &str) -> Result<String, JsValue> {
        let mut needed = Vec::new();
        let mut add = |t: TileRef<'_>| {
            let tile = StoredProjectTile::from(t);
            if !tile.is_resident() {
                needed.push(tile);
            }
        };
        match scope {
            "adjustment" => {
                if let Some(j) = &self.adjusted.apply {
                    j.needed(&mut add);
                }
            }
            "histogram" => {
                if let Some(j) = &self.adjusted.histogram {
                    j.needed(&mut add);
                }
            }
            "export" => {
                if let Some(j) = &self.export {
                    j.needed(&mut add);
                }
            }
            "selection" => self.selected.needed(&mut add),
            "selection-seed" => self.selected.seed_needed(&mut add),
            "selection-mask" => {
                if let Some(selection) = self.document.as_ref().and_then(|d| d.selection.as_ref()) {
                    for t in selection.tiles().values() {
                        add(TileRef::Gray(t));
                    }
                }
            }
            "thumbnail" => {
                if let Some(job) = &self.layered.thumbnail {
                    if let Some((_, plan)) = job.plan().map_err(js)? {
                        plan.visit_tiles(&mut add);
                    }
                }
            }
            "render" => {
                if let Some(job) = &self.layered.render {
                    if let Some((_, plan)) = job.plan().map_err(js)? {
                        plan.visit_tiles(&mut add);
                    }
                }
            }
            "write" => {
                if let Some(tile) = self.layered.write.as_ref().and_then(|w| w.needed_tile()) {
                    if !tile.is_resident() {
                        needed.push(tile.clone());
                    }
                }
            }
            "active" => {
                let doc = self.document.as_ref().ok_or("No document")?;
                let target = self
                    .layered
                    .active
                    .or_else(|| doc.layers.first().map(|l| l.id))
                    .ok_or("Choose a layer")?;
                let layer = doc.layer(target).map_err(js)?;
                if self.layered.mask_target {
                    if let Some(mask) = &layer.mask {
                        for tile in mask.samples.tiles().values() {
                            add(TileRef::Gray(tile));
                        }
                    }
                } else {
                    layer.visit_tiles(&mut add);
                }
                if let Some(s) = &doc.selection {
                    for t in s.tiles().values() {
                        add(TileRef::Gray(t));
                    }
                }
            }
            _ => return Err(JsValue::from_str("Unknown page request scope")),
        }
        let mut seen = HashSet::new();
        needed.retain(|t| seen.insert(t.id()));
        for tile in &needed {
            self.layered.pages.insert(tile.id(), tile.clone());
        }
        let ticket = if needed.is_empty() {
            0
        } else {
            self.begin_page_io()?
        };
        Ok(serde_json::json!({"requests":needed.iter().map(description).collect::<Vec<_>>(),"ticket":ticket}).to_string())
    }
    pub fn page_io_complete(&mut self, ticket: u32) {
        if self.layered.io_sessions.remove(&ticket) {
            if let Some(memory) = &mut self.layered.memory {
                memory.temporary = memory.temporary.saturating_sub(2 * 1024 * 1024);
            }
        }
    }
    pub fn active_cold(&self) -> bool {
        let Some(doc) = &self.document else {
            return false;
        };
        let Some(target) = self
            .layered
            .active
            .or_else(|| doc.layers.first().map(|l| l.id))
        else {
            return false;
        };
        let mut cold = false;
        if let Some(s) = &doc.selection {
            cold |= s.tiles().values().any(|t| t.resident_bytes() == 0);
        }
        if let Ok(layer) = doc.layer(target) {
            let mut check = |t: TileRef<'_>| {
                if t.bytes() == 0 {
                    cold = true;
                }
            };
            if self.layered.mask_target {
                if let Some(mask) = &layer.mask {
                    for tile in mask.samples.tiles().values() {
                        check(TileRef::Gray(tile));
                    }
                }
            } else if matches!(layer.content, LayerContent::Raster(_)) {
                layer.visit_tiles(&mut check);
            }
        }
        cold
    }
    pub fn export_original(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|d| d.export_raster().is_ok())
    }
    pub fn page_hydrate(&mut self, generation: &str, bytes: &[u8]) -> Result<(), JsValue> {
        let tile = self
            .layered
            .pages
            .get(&id(generation)?)
            .ok_or("Generation expired")?
            .clone();
        let cold = !tile.is_resident();
        let mut memory = self.paging_residency();
        tile.hydrate(bytes, memory).map_err(js)?;
        if cold {
            memory.document += bytes.len() as u64;
            self.layered.memory = Some(memory);
        }
        Ok(())
    }
    pub fn page_unbacked(&mut self) -> Result<String, JsValue> {
        self.refresh_pages();
        let mut ids: Vec<_> = self
            .layered
            .pages
            .values()
            .filter(|t| t.backing().is_none())
            .map(|t| t.id().to_string())
            .collect();
        ids.sort();
        let ticket = self.begin_page_io()?;
        Ok(serde_json::json!({"ids":ids,"ticket":ticket}).to_string())
    }
    pub fn page_stores(&mut self) -> String {
        self.refresh_pages();
        let stores: HashSet<_> = self
            .layered
            .pages
            .values()
            .filter_map(|t| t.backing().map(|b| format!("{:032x}", b.store)))
            .collect();
        serde_json::json!(stores).to_string()
    }
    pub fn page_encode(&self, generation: &str) -> Result<Vec<u8>, JsValue> {
        self.layered
            .pages
            .get(&id(generation)?)
            .ok_or("Generation expired")?
            .encode(self.residency())
            .map_err(js)
    }
    pub fn page_acknowledge(
        &self,
        generation: &str,
        store: &str,
        offset: f64,
        bytes: &[u8],
    ) -> Result<(), JsValue> {
        if !offset.is_finite() || offset < 0.0 || offset.fract() != 0.0 {
            return Err(JsValue::from_str("Invalid backing offset"));
        }
        self.layered
            .pages
            .get(&id(generation)?)
            .ok_or("Generation expired")?
            .acknowledge(store_id(store)?, offset as u64, bytes, self.residency())
            .map_err(js)
    }
    pub fn page_evict(&mut self, target: f64, protected: &str) -> Result<(), JsValue> {
        let mut protected: HashSet<String> =
            serde_json::from_str(protected).map_err(|e| js(e.to_string()))?;
        if let Some(stroke) = &self.stroke {
            stroke.preview.visit_tiles(&mut |t| {
                protected.insert(t.id().to_string());
            });
            if let Some(doc) = &self.document {
                if let Ok(layer) = doc.layer(stroke.preview.layers[0].id) {
                    if self.layered.mask_target {
                        if let Some(mask) = &layer.mask {
                            for tile in mask.samples.tiles().values() {
                                protected.insert(tile.id().to_string());
                            }
                        }
                    } else {
                        layer.visit_tiles(&mut |t| {
                            protected.insert(t.id().to_string());
                        });
                    }
                }
            }
        }
        let r = self.residency();
        let mut used = r.document + r.history + r.temporary;
        for tile in self.layered.pages.values() {
            if used <= target.max(0.0) as u64 {
                break;
            }
            if tile.backing().is_some() && !protected.contains(&tile.id().to_string()) {
                let bytes = match tile {
                    StoredProjectTile::Rgba(t) => t.resident_bytes(),
                    StoredProjectTile::Gray(t) => t.resident_bytes(),
                };
                if tile.evict().is_ok() {
                    used = used.saturating_sub(bytes);
                }
            }
        }
        self.layered.memory = None;
        Ok(())
    }
    pub fn stream_read_begin(&mut self, length: f64, store: &str) -> Result<(), JsValue> {
        if !length.is_finite()
            || length.fract() != 0.0
            || !(0.0..=fotoeditor_io::MAX_PROJECT_BYTES as f64).contains(&length)
        {
            return Err(JsValue::from_str(
                "Project exceeds the 4 GiB streamed file limit",
            ));
        }
        self.layered.read = Some(
            LayerProjectReader::new(length as u64, store_id(store)?, self.residency())
                .map_err(js)?,
        );
        Ok(())
    }
    pub fn stream_read_feed(&mut self, bytes: &[u8]) -> Result<u32, JsValue> {
        self.layered
            .read
            .as_mut()
            .ok_or("No streamed project read")?
            .feed(bytes)
            .map(|n| n as u32)
            .map_err(js)
    }
    pub fn stream_read_finish(&mut self, commit: bool) -> Result<(), JsValue> {
        let doc = self
            .layered
            .read
            .take()
            .ok_or("No streamed project read")?
            .finish()
            .map_err(js)?;
        if commit {
            let cache_bytes = raster_coordinates(&doc.layers).len() as u64 * TILE_PIXELS as u64 * 4;
            // Reserve a bounded source working set and bridge/readback scratch
            // before replacing the old session. Unsupported dense canvas sizes
            // must not replace it and then fail halfway through composition.
            Residency::default()
                .check_cpu(cache_bytes + 192 * 1024 * 1024)
                .map_err(js)?;
            self.execute(SessionCommand::ReplaceDocument(doc));
            self.layered.force_composite = true;
        }
        Ok(())
    }
    pub fn stream_write_begin(&mut self) -> Result<f64, JsValue> {
        self.layered.write = Some(
            LayerProjectWriter::new(
                self.document.as_ref().ok_or("No document")?,
                self.residency(),
            )
            .map_err(js)?,
        );
        self.refresh_pages();
        Ok(self.layered.write.as_ref().unwrap().file_bytes as f64)
    }
    pub fn stream_write_next(&mut self) -> Result<Option<Vec<u8>>, JsValue> {
        let mut r = self.paging_residency();
        r.temporary = r.temporary.saturating_sub(
            self.layered
                .write
                .as_ref()
                .map_or(0, |w| w.temporary_bytes()),
        );
        self.layered
            .write
            .as_mut()
            .ok_or("No streamed project save")?
            .next_packet(r)
            .map_err(js)
    }
    pub fn legacy_project(&self) -> bool {
        self.document.as_ref().is_some_and(|d| {
            d.single_raster().is_ok()
                && d.layers[0].name == "Layer 1"
                && d.raster().tiles().len() as u64 * 262152 + 40 <= fotoeditor_io::MAX_FILE_BYTES
        })
    }
}
