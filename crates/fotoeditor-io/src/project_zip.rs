//! Bounded ZIP/LZ4 project output. Empty packets mean CPU-only progress.
use crate::{
    encode_lz4_tile, encode_thumbnail, ExportJob, LayerProjectWriter, ZipStreamWriter,
    LAYER_PROJECT_SCRATCH, MAX_FILE_BYTES, PROJECT_CHUNK_BYTES, ZIP_SCRATCH,
};
use fotoeditor_core::{Document, Residency, TileRef, TILE_SIZE};

struct RasterThumbnail {
    source: Document,
    width: u32,
    height: u32,
    row: u32,
    pixels: Vec<u8>,
}
impl RasterThumbnail {
    fn new(source: &Document) -> Result<Self, String> {
        source.export_raster()?;
        let rect = source.canvas_rect();
        let scale = 64.0 / f64::from(rect.width.max(rect.height));
        let width = (f64::from(rect.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(rect.height) * scale).round().max(1.0) as u32;
        Ok(Self {
            source: source.clone(),
            width,
            height,
            row: 0,
            pixels: vec![0; (width * height * 4) as usize],
        })
    }
    fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if self.row >= self.height {
            return;
        }
        let rect = self.source.canvas_rect();
        let y = rect.y + self.row * rect.height / self.height;
        for x in 0..self.width {
            let sx = rect.x + x * rect.width / self.width;
            if let Some(tile) = self
                .source
                .raster()
                .tiles()
                .get(&(sx / TILE_SIZE, y / TILE_SIZE))
            {
                visitor(TileRef::Rgba(tile));
            }
        }
    }
    fn step(&mut self) -> Result<bool, String> {
        if self.row >= self.height {
            return Ok(true);
        }
        let rect = self.source.canvas_rect();
        let y = rect.y + self.row * rect.height / self.height;
        for x in 0..self.width {
            let sx = rect.x + x * rect.width / self.width;
            let pixel = self
                .source
                .raster()
                .tiles()
                .get(&(sx / TILE_SIZE, y / TILE_SIZE))
                .map(|tile| {
                    tile.pin()
                        .map(|p| p.pixels[((y % TILE_SIZE) * TILE_SIZE + sx % TILE_SIZE) as usize])
                })
                .transpose()?
                .unwrap_or(self.source.raster().default);
            self.pixels[((self.row * self.width + x) * 4) as usize..][..4]
                .copy_from_slice(&pixel.0);
        }
        self.row += 1;
        Ok(self.row == self.height)
    }
}

pub struct ZipProjectWriter {
    legacy: LayerProjectWriter,
    zip: Option<ZipStreamWriter>,
    thumbnail: Option<RasterThumbnail>,
    export: Option<ExportJob>,
    previews: Vec<(&'static str, Vec<u8>)>,
    stage: u8,
    preview: usize,
    offset: usize,
    directory: Option<Vec<u8>>,
    done: bool,
    written: u64,
}
impl ZipProjectWriter {
    pub fn new(
        document: &Document,
        composite: &Document,
        residency: Residency,
    ) -> Result<Self, String> {
        residency.check_cpu(
            LAYER_PROJECT_SCRATCH + ZIP_SCRATCH + 2 * MAX_FILE_BYTES + 8 * 1024 * 1024,
        )?;
        Ok(Self {
            legacy: LayerProjectWriter::new(document, residency)?,
            zip: Some(ZipStreamWriter::new(residency)?),
            thumbnail: Some(RasterThumbnail::new(composite)?),
            export: Some(ExportJob::new(composite, residency)?),
            previews: Vec::new(),
            stage: 0,
            preview: 0,
            offset: 0,
            directory: None,
            done: false,
            written: 0,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        LAYER_PROJECT_SCRATCH
            + ZIP_SCRATCH
            + 1024 * 1024
            + self.export.as_ref().map_or(0, |e| e.temporary_bytes())
            + self
                .previews
                .iter()
                .map(|(_, p)| p.capacity() as u64)
                .sum::<u64>()
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        match self.stage {
            0 => {
                if let Some(t) = &self.thumbnail {
                    t.needed(visitor);
                }
            }
            1 => {
                if let Some(e) = &self.export {
                    e.needed(visitor);
                }
            }
            2 => {
                if let Some(t) = self.legacy.needed_tile() {
                    match t {
                        crate::StoredProjectTile::Rgba(t) => visitor(TileRef::Rgba(t)),
                        crate::StoredProjectTile::Gray(t) => visitor(TileRef::Gray(t)),
                    }
                }
            }
            _ => {}
        }
    }
    pub fn written_bytes(&self) -> u64 {
        self.written
    }
    pub fn next_packet(&mut self, residency: Residency) -> Result<Option<Vec<u8>>, String> {
        residency.check_cpu(self.temporary_bytes())?;
        if self.done {
            return Ok(None);
        }
        let bytes = match self.stage {
            0 => {
                let t = self.thumbnail.as_mut().unwrap();
                if t.step()? {
                    let t = self.thumbnail.take().unwrap();
                    self.previews.push((
                        "thumbnail.png",
                        encode_thumbnail(t.width, t.height, &t.pixels)?,
                    ));
                    self.stage = 1;
                }
                Vec::new()
            }
            1 => {
                if self.export.as_mut().unwrap().step()? {
                    self.previews
                        .push(("composite.png", self.export.take().unwrap().finish()?));
                    self.stage = 2;
                }
                Vec::new()
            }
            2 => {
                if let Some(packet) = self.legacy.next_packet(residency)? {
                    if packet.len() == 12 {
                        Vec::new()
                    } else {
                        let kind = u32::from_le_bytes(packet[..4].try_into().unwrap());
                        let id = u64::from_le_bytes(packet[4..12].try_into().unwrap());
                        match kind {
                            1 => self
                                .zip
                                .as_mut()
                                .unwrap()
                                .record("manifest.json", &packet[20..])?,
                            2 | 3 => {
                                let name = format!(
                                    "tiles/{id}.{}.lz4",
                                    if kind == 2 { "rgba" } else { "gray" }
                                );
                                self.zip
                                    .as_mut()
                                    .unwrap()
                                    .record(&name, &encode_lz4_tile(&packet[20..])?)?
                            }
                            255 => {
                                self.stage = 3;
                                Vec::new()
                            }
                            _ => return Err("Unexpected source project packet".into()),
                        }
                    }
                } else {
                    return Err("Source project ended before its footer".into());
                }
            }
            3 => {
                let (name, pixels) = &self.previews[self.preview];
                self.stage = 4;
                self.offset = 0;
                self.zip.as_mut().unwrap().start(
                    name,
                    pixels.len() as u32,
                    crc32fast::hash(pixels),
                )?
            }
            4 => {
                let pixels = &self.previews[self.preview].1;
                let end = (self.offset + PROJECT_CHUNK_BYTES).min(pixels.len());
                let packet = pixels[self.offset..end].to_vec();
                self.zip.as_mut().unwrap().data(&packet)?;
                self.offset = end;
                if end == pixels.len() {
                    self.zip.as_mut().unwrap().end()?;
                    self.preview += 1;
                    self.stage = if self.preview == self.previews.len() {
                        5
                    } else {
                        3
                    };
                }
                packet
            }
            5 => {
                if self.directory.is_none() {
                    self.directory = Some(self.zip.take().unwrap().finish()?);
                    self.offset = 0;
                }
                let directory = self.directory.as_ref().unwrap();
                let end = (self.offset + PROJECT_CHUNK_BYTES).min(directory.len());
                let packet = directory[self.offset..end].to_vec();
                self.offset = end;
                if end == directory.len() {
                    self.done = true;
                }
                packet
            }
            _ => return Err("Invalid ZIP project writer stage".into()),
        };
        self.written += bytes.len() as u64;
        Ok(Some(bytes))
    }
}
