//! Straight-alpha CPU document samples. No browser or GPU types.
mod blend;
mod commands;
mod display;
mod paging;
mod tiles;
mod view;
pub use blend::*;
pub use commands::*;
pub use display::*;
pub use paging::*;
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
/// One straight-alpha raster layer. Session mutations go through commands.
#[derive(Clone)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub raster: TileSurface<Rgba8>,
}
impl Document {
    pub fn blank(width: u32, height: u32, dpi: f64) -> Result<Self, String> {
        validate_size(width, height, dpi)?;
        Ok(Self {
            width,
            height,
            dpi,
            raster: TileSurface::new(width, height),
        })
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
