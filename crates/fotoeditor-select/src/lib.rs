//! Browser-independent tiled selections, fills and clipboard processing.
mod clipboard;
mod content_aware;
mod fill;
mod mask;
mod shapes;
mod wand;
pub use clipboard::*;
pub use content_aware::*;
pub use fill::*;
pub use mask::*;
pub use shapes::*;
pub use wand::*;
#[cfg(test)]
mod canvas_tests;
