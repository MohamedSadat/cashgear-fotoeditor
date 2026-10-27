//! Read-only image export sources over the visible canvas window.
use fotoeditor_core::{
    CanvasRect, Document, Gray8, LayerContent, LayerId, Rgba8, TileRef, TileSurface, TILE_SIZE,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ImageExportSource {
    Full,
    Layer,
    Selection,
}

#[derive(Clone)]
pub(crate) struct RasterSource {
    samples: Arc<TileSurface<Rgba8>>,
    mask: Option<Arc<TileSurface<Gray8>>>,
    opacity: f64,
    pub rect: CanvasRect,
    pub dpi: f64,
    pub selection: bool,
}
impl RasterSource {
    pub fn new(
        document: &Document,
        composite: &Document,
        kind: ImageExportSource,
        active: Option<LayerId>,
    ) -> Result<Self, String> {
        document.validate_geometry()?;
        let rect = document.canvas_rect();
        let (samples, mask, opacity) = if kind == ImageExportSource::Layer {
            let id = active.ok_or("Choose a raster layer to export")?;
            let layer = document.layer(id)?;
            let LayerContent::Raster(samples) = &layer.content else {
                return Err("Choose a raster layer to export; export the document to include groups and adjustments".into());
            };
            (
                samples.clone(),
                layer
                    .mask
                    .as_ref()
                    .filter(|m| m.enabled)
                    .map(|m| m.samples.clone()),
                layer.properties.opacity * layer.properties.fill,
            )
        } else {
            if document.width != composite.width
                || document.height != composite.height
                || rect != composite.canvas_rect()
            {
                return Err("The current document composite is unavailable".into());
            }
            composite.export_raster()?;
            let LayerContent::Raster(samples) = &composite.layers[0].content else {
                unreachable!()
            };
            let selection = if kind == ImageExportSource::Selection {
                Some(
                    document
                        .selection
                        .clone()
                        .ok_or("Make a selection before exporting it")?,
                )
            } else {
                None
            };
            (samples.clone(), selection, 1.0)
        };
        if (samples.width, samples.height) != (document.width, document.height)
            || mask
                .as_ref()
                .is_some_and(|m| (m.width, m.height) != (document.width, document.height))
        {
            return Err("Export source dimensions do not match the document".into());
        }
        Ok(Self {
            samples,
            mask,
            opacity,
            rect,
            dpi: document.dpi,
            selection: kind == ImageExportSource::Selection,
        })
    }
    pub fn needed(&self, row: u32, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if row >= self.rect.height {
            return;
        }
        let y1 = self.rect.y + (row + 32).min(self.rect.height);
        for ty in (self.rect.y + row) / TILE_SIZE..y1.div_ceil(TILE_SIZE) {
            for tx in self.rect.x / TILE_SIZE..(self.rect.x + self.rect.width).div_ceil(TILE_SIZE) {
                if let Some(tile) = self.samples.tiles().get(&(tx, ty)) {
                    visitor(TileRef::Rgba(tile));
                }
                if let Some(tile) = self.mask.as_ref().and_then(|m| m.tiles().get(&(tx, ty))) {
                    visitor(TileRef::Gray(tile));
                }
            }
        }
    }
    pub fn read_row(&self, row: u32, output: &mut [u8]) -> Result<bool, String> {
        if row >= self.rect.height || output.len() != self.rect.width as usize * 4 {
            return Err("Invalid export scanline".into());
        }
        let y = self.rect.y + row;
        let mut selected = false;
        for tx in self.rect.x / TILE_SIZE..(self.rect.x + self.rect.width).div_ceil(TILE_SIZE) {
            let coord = (tx, y / TILE_SIZE);
            let samples = self
                .samples
                .tiles()
                .get(&coord)
                .map(|t| t.pin())
                .transpose()?;
            let mask = self
                .mask
                .as_ref()
                .and_then(|m| m.tiles().get(&coord))
                .map(|t| t.pin())
                .transpose()?;
            let x0 = (tx * TILE_SIZE).max(self.rect.x);
            let x1 = ((tx + 1) * TILE_SIZE).min(self.rect.x + self.rect.width);
            for x in x0..x1 {
                let index = ((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize;
                let mut pixel = samples
                    .as_ref()
                    .map_or(self.samples.default, |t| t.pixels[index])
                    .0;
                let coverage = mask.as_ref().map_or_else(
                    || self.mask.as_ref().map_or(255, |m| m.default.0),
                    |m| m.pixels[index].0,
                );
                selected |= coverage != 0;
                if coverage != 255 || self.opacity != 1.0 {
                    pixel[3] = (f64::from(pixel[3]) * f64::from(coverage) / 255.0 * self.opacity)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
                // Straight RGB survives zero coverage/alpha, including hidden samples.
                let at = (x - self.rect.x) as usize * 4;
                output[at..at + 4].copy_from_slice(&pixel);
            }
        }
        Ok(selected)
    }
}
