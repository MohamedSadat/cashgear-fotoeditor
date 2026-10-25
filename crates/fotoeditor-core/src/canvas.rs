//! Visible canvas metadata over retained, pageable document samples.
use crate::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl CanvasRect {
    pub fn validate(self, width: u32, height: u32) -> Result<Self, String> {
        if self.width == 0
            || self.height == 0
            || self.x.checked_add(self.width).is_none_or(|end| end > width)
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|end| end > height)
        {
            return Err("Canvas rectangle must fit the retained document extent".into());
        }
        Ok(self)
    }
    pub fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.width && y - self.y < self.height
    }
    pub fn geometry(self) -> TransformRect {
        TransformRect {
            x: self.x as f64,
            y: self.y as f64,
            width: self.width as f64,
            height: self.height as f64,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guide {
    pub axis: GuideAxis,
    pub position: f64,
}
pub const MAX_GUIDES: usize = 256;
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum GeometryMetadata {
    Crop { rect: Option<CanvasRect> },
    Guides { guides: Vec<Guide> },
}
impl Document {
    pub fn canvas_rect(&self) -> CanvasRect {
        self.canvas.unwrap_or(CanvasRect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        })
    }
    pub fn validate_geometry(&self) -> Result<(), String> {
        validate_size(self.width, self.height, self.dpi)?;
        self.canvas_rect().validate(self.width, self.height)?;
        if self.guides.len() > MAX_GUIDES
            || self.guides.iter().any(|g| {
                let limit = match g.axis {
                    GuideAxis::Horizontal => self.height,
                    GuideAxis::Vertical => self.width,
                };
                !g.position.is_finite() || g.position < 0.0 || g.position > limit as f64
            })
        {
            return Err("Use at most 256 finite guides within the retained document".into());
        }
        Ok(())
    }
    pub fn crop_command(&self, rect: CanvasRect) -> Result<DocumentCommand, String> {
        rect.validate(self.width, self.height)?;
        let mut after = self.clone();
        after.canvas = (rect
            != CanvasRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            })
        .then_some(rect);
        DocumentCommand::between(self, after, "Crop canvas")
    }
    pub fn guide_command(&self, guides: Vec<Guide>) -> Result<DocumentCommand, String> {
        let mut after = self.clone();
        after.guides = guides;
        DocumentCommand::between(self, after, "Edit guides")
    }
}

/// Exact COW document replacement for geometry jobs. No candidate is published
/// before validation/history admission. Shared before/after generations count once.
pub struct DocumentCommand {
    before: Document,
    after: Document,
    label: &'static str,
    empty: bool,
}
impl DocumentCommand {
    pub fn between(
        before: &Document,
        after: Document,
        label: &'static str,
    ) -> Result<Self, String> {
        after.validate_geometry()?;
        validate_layers(&after.layers, after.width, after.height)?;
        if after
            .selection
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (after.width, after.height))
        {
            return Err("Selection dimensions must match the retained document".into());
        }
        let empty = before.width == after.width
            && before.height == after.height
            && before.dpi.to_bits() == after.dpi.to_bits()
            && before.canvas == after.canvas
            && before.guides == after.guides
            && crate::layers::equivalent(&before.layers, &after.layers)
            && match (&before.selection, &after.selection) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            };
        Ok(Self {
            before: before.clone(),
            after,
            label,
            empty,
        })
    }
}
impl Command for DocumentCommand {
    fn label(&self) -> &str {
        self.label
    }
    fn apply(&self, d: &mut Document) {
        *d = self.after.clone();
    }
    fn revert(&self, d: &mut Document) {
        *d = self.before.clone();
    }
    fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        self.before.visit_tiles(visitor);
        self.after.visit_tiles(visitor);
    }
    fn is_empty(&self) -> bool {
        self.empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_retains_exact_samples_without_allocating_and_history_restores_metadata() {
        let mut d = Document::blank(513, 259, 72.0).unwrap();
        for (x, y, p) in [
            (0, 0, [87, 33, 19, 0]),
            (256, 258, [201, 3, 7, 1]),
            (512, 258, [5, 6, 7, 255]),
        ] {
            d.raster_mut().write(x, y, Rgba8(p));
        }
        let before = d.clone();
        let bytes = d.bytes();
        let crop = CanvasRect {
            x: 255,
            y: 1,
            width: 257,
            height: 258,
        };
        let mut h = History::default();
        h.execute(&mut d, Box::new(before.crop_command(crop).unwrap()), 0)
            .unwrap();
        assert_eq!(d.canvas_rect(), crop);
        assert_eq!(d.bytes(), bytes);
        assert_eq!(h.bytes(&d), 0);
        for (c, t) in before.raster().tiles() {
            assert_eq!(d.raster().tiles()[c].id(), t.id());
        }
        assert_eq!(d.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
        let guides = vec![Guide {
            axis: GuideAxis::Vertical,
            position: 256.5,
        }];
        let command = d.guide_command(guides.clone()).unwrap();
        h.execute(&mut d, Box::new(command), 0).unwrap();
        assert!(h.undo(&mut d));
        assert!(d.guides.is_empty());
        assert!(h.undo(&mut d));
        assert!(d.canvas.is_none());
        assert!(h.redo(&mut d));
        assert_eq!(d.canvas_rect(), crop);
        assert!(h.redo(&mut d));
        assert_eq!(d.guides, guides);
        let full = CanvasRect {
            x: 0,
            y: 0,
            width: 513,
            height: 259,
        };
        let command = d.crop_command(full).unwrap();
        h.execute(&mut d, Box::new(command), 0).unwrap();
        assert!(d.canvas.is_none());
        assert_eq!(d.raster().sample(512, 258), Rgba8([5, 6, 7, 255]));
    }
    #[test]
    fn invalid_metadata_and_noops_preserve_redo() {
        let mut d = Document::blank(10, 10, 72.0).unwrap();
        let mut h = History::default();
        let command = d
            .crop_command(CanvasRect {
                x: 1,
                y: 1,
                width: 8,
                height: 8,
            })
            .unwrap();
        h.execute(&mut d, Box::new(command), 0).unwrap();
        h.undo(&mut d);
        assert!(d
            .crop_command(CanvasRect {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1
            })
            .is_err());
        assert!(d
            .guide_command(vec![Guide {
                axis: GuideAxis::Horizontal,
                position: f64::NAN
            }])
            .is_err());
        let command = d.crop_command(d.canvas_rect()).unwrap();
        assert!(!h.execute(&mut d, Box::new(command), 0).unwrap());
        assert_eq!(h.depths(), (0, 1));
        assert_eq!(Interpolation::default(), Interpolation::Lanczos3);
    }
}
