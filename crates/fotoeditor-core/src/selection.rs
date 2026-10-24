//! Exact selection commands and the coverage contract shared by all pixel tools.
use crate::{Command, Document, Gray8, TileRef, TileSurface};
use std::sync::Arc;

pub type Selection = Option<Arc<TileSurface<Gray8>>>;

/// None permits editing the entire canvas; Some(empty) permits no editing.
pub fn selection_coverage(selection: &Selection, x: u32, y: u32) -> u8 {
    selection.as_ref().map_or(255, |s| {
        if x >= s.width || y >= s.height {
            0
        } else {
            s.sample(x, y).0
        }
    })
}

pub struct SelectionCommand {
    before: Selection,
    after: Selection,
    label: &'static str,
    empty: bool,
}
impl SelectionCommand {
    pub fn new(document: &Document, after: Selection, label: &'static str) -> Result<Self, String> {
        if after
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (document.width, document.height))
        {
            return Err("Selection dimensions differ from the document".into());
        }
        let same = match (&document.selection, &after) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                Arc::ptr_eq(a, b)
                    || (a.default == b.default
                        && a.tiles().len() == b.tiles().len()
                        && a.tiles().iter().all(|(coord, t)| {
                            b.tiles()
                                .get(coord)
                                .is_some_and(|other| t.id() == other.id())
                        }))
            }
            _ => false,
        };
        Ok(Self {
            before: document.selection.clone(),
            after,
            label,
            empty: same,
        })
    }
}
impl Command for SelectionCommand {
    fn label(&self) -> &str {
        self.label
    }
    fn apply(&self, document: &mut Document) {
        document.selection = self.after.clone();
    }
    fn revert(&self, document: &mut Document) {
        document.selection = self.before.clone();
    }
    fn is_empty(&self) -> bool {
        self.empty
    }
    fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for surface in [&self.before, &self.after].into_iter().flatten() {
            for tile in surface.tiles().values() {
                visitor(TileRef::Gray(tile));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{History, Rgba8};
    #[test]
    fn selection_commands_share_unique_buffers_and_restore_unrestricted_empty_and_partial() {
        let mut d = Document::blank(257, 2, 72.0).unwrap();
        d.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        let mut h = History::default();
        let empty = Arc::new(TileSurface::new(257, 2));
        let c = SelectionCommand::new(&d, Some(empty), "Select none").unwrap();
        h.execute(&mut d, Box::new(c), 0).unwrap();
        assert_eq!(selection_coverage(&d.selection, 0, 0), 0);
        let mut s = TileSurface::new(257, 2);
        s.write(256, 1, Gray8(128));
        let c = SelectionCommand::new(&d, Some(Arc::new(s)), "Select").unwrap();
        h.execute(&mut d, Box::new(c), 0).unwrap();
        assert_eq!(d.bytes(), 262144 + 65536);
        assert_eq!(h.bytes(&d), 0);
        assert_eq!(selection_coverage(&d.selection, 256, 1), 128);
        assert_eq!(selection_coverage(&d.selection, 257, 1), 0);
        assert!(h.undo(&mut d));
        assert_eq!(h.bytes(&d), 65536);
        assert_eq!(selection_coverage(&d.selection, 256, 1), 0);
        assert!(h.undo(&mut d));
        assert_eq!(selection_coverage(&d.selection, 0, 0), 255);
        assert_eq!(d.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
        assert!(h.redo(&mut d));
        assert!(h.redo(&mut d));
        let c = SelectionCommand::new(&d, d.selection.clone(), "Select").unwrap();
        assert!(!h.execute(&mut d, Box::new(c), 0).unwrap());
    }
}
