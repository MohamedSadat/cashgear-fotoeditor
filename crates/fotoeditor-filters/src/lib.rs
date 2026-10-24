//! Browser-independent, straight-alpha processing. No UI, browser or GPU types.
mod jobs;
pub use fotoeditor_core::{Adjustment, AdjustmentKernel};
pub use jobs::*;
