//! Browser-independent, straight-alpha processing. No UI, browser or GPU types.
mod adjustment;
mod jobs;
pub use adjustment::*;
pub use jobs::*;
