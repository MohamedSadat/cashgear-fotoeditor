//! Browser-independent, straight-alpha processing. No UI, browser or GPU types.
mod jobs;
mod spatial;
mod texture;
pub use fotoeditor_core::{Adjustment, AdjustmentKernel};
pub use jobs::*;
pub use spatial::*;
