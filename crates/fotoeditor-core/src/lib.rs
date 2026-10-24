//! Straight-alpha CPU document samples. No browser or GPU types.
mod adjustment;
#[cfg(test)]
mod adjustment_layers;
mod blend;
mod commands;
mod composite;
mod display;
mod layer_jobs;
mod layers;
mod paging;
mod selection;
mod selection_display;
mod tiles;
mod view;
pub use adjustment::*;
pub use blend::*;
pub use commands::*;
pub use composite::*;
pub use display::*;
pub use layer_jobs::*;
pub use layers::*;
pub use paging::*;
pub use selection::*;
pub use selection_display::*;
pub use tiles::*;
pub use view::*;
pub const MAX_SIDE: u32 = 16_384;
pub const CPU_BUDGET: u64 = 768 * 1024 * 1024;
pub const GPU_BUDGET: u64 = 128 * 1024 * 1024;
pub fn validate_size(width: u32, height: u32, dpi: f64) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!(
            "Canvas sides must be between 1 and {MAX_SIDE} pixels"
        ));
    }
    if !dpi.is_finite() || !(1.0..=2400.0).contains(&dpi) {
        return Err("DPI must be between 1 and 2400".into());
    }
    Ok(())
}
/// Bottom-to-top layer tree. Session mutations go through commands.
#[derive(Clone)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub layers: Vec<Layer>,
    /// Session selection: None is unrestricted; an active zero mask blocks edits.
    pub selection: Option<std::sync::Arc<TileSurface<Gray8>>>,
}
impl Document {
    pub fn blank(width: u32, height: u32, dpi: f64) -> Result<Self, String> {
        validate_size(width, height, dpi)?;
        Ok(Self {
            width,
            height,
            dpi,
            layers: vec![Layer::raster(
                "Layer 1".into(),
                TileSurface::new(width, height),
            )],
            selection: None,
        })
    }
    /// Single-raster compatibility for v1 codecs and the existing brush adapter.
    pub fn raster(&self) -> &TileSurface<Rgba8> {
        self.layers[0]
            .raster_samples()
            .expect("single-raster adapter")
    }
    pub fn raster_mut(&mut self) -> &mut TileSurface<Rgba8> {
        self.layers[0]
            .raster_samples_mut()
            .expect("single-raster adapter")
    }
    pub fn single_raster(&self) -> Result<&TileSurface<Rgba8>, String> {
        if self.layers.len() != 1
            || self.layers[0].mask.is_some()
            || self.layers[0].properties != LayerProperties::default()
        {
            return Err("Composite the layer tree before using a single-raster adapter".into());
        }
        self.layers[0].raster_samples()
    }
    pub fn export_raster(&self) -> Result<&TileSurface<Rgba8>, String> {
        if self.layers.len() != 1 || self.layers[0].mask.is_some() {
            return Err("Composite layers before image export".into());
        }
        let mut properties = self.layers[0].properties.clone();
        properties.locks = LayerLocks::default();
        if properties != LayerProperties::default() {
            return Err("Composite layer effects before image export".into());
        }
        self.layers[0].raster_samples()
    }
    pub fn layer(&self, id: LayerId) -> Result<&Layer, String> {
        find_layer(&self.layers, id).ok_or_else(|| "Layer no longer exists".into())
    }
    pub fn layer_mut(&mut self, id: LayerId) -> Result<&mut Layer, String> {
        find_layer_mut(&mut self.layers, id).ok_or_else(|| "Layer no longer exists".into())
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(selection) = &self.selection {
            for tile in selection.tiles().values() {
                visitor(TileRef::Gray(tile));
            }
        }
        for layer in &self.layers {
            layer.visit_tiles(visitor);
        }
    }
    pub fn bytes(&self) -> u64 {
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0;
        self.visit_tiles(&mut |tile| {
            if seen.insert(tile.id()) {
                bytes += tile.bytes();
            }
        });
        bytes
    }
    pub fn tile_count(&self) -> usize {
        fn count(nodes: &[Layer]) -> usize {
            nodes
                .iter()
                .map(|l| match &l.content {
                    LayerContent::Raster(s) => s.tiles().len(),
                    LayerContent::Group(c) => count(c),
                    LayerContent::Adjustment(_) => 0,
                })
                .sum()
        }
        count(&self.layers)
    }
}
/// Live buffer allocations, excluding allocator high-water retention and driver overhead.
/// History must count unique retained IDs excluding live document IDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct Residency {
    pub document: u64,
    pub history: u64,
    pub temporary: u64,
    pub gpu: u64,
}
impl Residency {
    pub fn check_cpu(&self, additional: u64) -> Result<(), String> {
        if self
            .document
            .saturating_add(self.history)
            .saturating_add(self.temporary)
            .saturating_add(additional)
            > CPU_BUDGET
        {
            return Err(
                "This operation exceeds the 768 MiB document and temporary memory budget".into(),
            );
        }
        Ok(())
    }
}
