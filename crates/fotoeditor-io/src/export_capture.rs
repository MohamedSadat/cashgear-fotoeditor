//! Bounded scanline capture for synchronous image codecs in an isolated job.
use crate::{export_source::RasterSource, ImageExportSource};
use fotoeditor_core::{Document, LayerId, Residency, TileRef};

pub struct ExportPixelsJob {
    source: RasterSource,
    pixels: Vec<u8>,
    row: Vec<u8>,
    next_y: u32,
    selected: bool,
}
impl ExportPixelsJob {
    pub fn new(
        document: &Document,
        composite: &Document,
        source: ImageExportSource,
        active: Option<LayerId>,
        residency: Residency,
    ) -> Result<Self, String> {
        let source = RasterSource::new(document, composite, source, active)?;
        let bytes = u64::from(source.rect.width) * u64::from(source.rect.height) * 4;
        residency.check_cpu(2 * bytes + u64::from(source.rect.width) * 4 + 8 * 1024 * 1024)?;
        Ok(Self {
            pixels: Vec::with_capacity(bytes as usize),
            row: vec![0; source.rect.width as usize * 4],
            source,
            next_y: 0,
            selected: false,
        })
    }
    pub fn width(&self) -> u32 {
        self.source.rect.width
    }
    pub fn height(&self) -> u32 {
        self.source.rect.height
    }
    pub fn dpi(&self) -> f64 {
        self.source.dpi
    }
    pub fn temporary_bytes(&self) -> u64 {
        2 * self.pixels.capacity() as u64 + self.row.capacity() as u64 + 8 * 1024 * 1024
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        self.source.needed(self.next_y, visitor);
    }
    pub fn step(&mut self) -> Result<bool, String> {
        for y in self.next_y..(self.next_y + 32).min(self.source.rect.height) {
            self.selected |= self.source.read_row(y, &mut self.row)?;
            self.pixels.extend_from_slice(&self.row);
        }
        self.next_y = (self.next_y + 32).min(self.source.rect.height);
        Ok(self.next_y == self.source.rect.height)
    }
    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.next_y != self.source.rect.height {
            return Err("Export capture is incomplete".into());
        }
        if self.source.selection && !self.selected {
            return Err("The selection is empty".into());
        }
        Ok(self.pixels)
    }
}
