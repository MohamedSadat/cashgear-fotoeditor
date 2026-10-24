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
                                .is_some_and(|other| t.id() == other.id() || matches!((t.pin(),other.pin()),(Ok(a),Ok(b)) if a.pixels==b.pixels))
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
    use proptest::prelude::*;
    fn fingerprint(d: &Document) -> (bool, Vec<u8>) {
        (
            d.selection.is_some(),
            (0..d.height)
                .flat_map(|y| (0..d.width).map(move |x| selection_coverage(&d.selection, x, y)))
                .collect(),
        )
    }
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]
        #[test]
        fn selection_sequences_undo_all_and_redo_exactly(steps in prop::collection::vec((0u8..5,any::<u8>(),0u32..513),0..35)) {
            let mut d=Document::blank(513,2,72.0).unwrap();d.raster_mut().write(255,1,Rgba8([87,33,19,0]));
            let initial=fingerprint(&d);let mut history=History::default();
            for (kind,value,x) in steps {
                let after=match kind {
                    0=>None,
                    1=>Some(Arc::new(TileSurface::new(513,2))),
                    2=>Some(Arc::new(TileSurface::with_default(513,2,Gray8(255)))),
                    _=>{let mut s=d.selection.as_ref().map_or_else(||TileSurface::new(513,2),|s|(**s).clone());s.write(x,u32::from(kind%2),Gray8(value));Some(Arc::new(s))}
                };
                let command=SelectionCommand::new(&d,after,"Selection").unwrap();history.execute(&mut d,Box::new(command),0).unwrap();
            }
            let final_state=fingerprint(&d);while history.undo(&mut d) {}
            prop_assert_eq!(fingerprint(&d),initial);prop_assert_eq!(d.raster().sample(255,1),Rgba8([87,33,19,0]));
            while history.redo(&mut d) {}prop_assert_eq!(fingerprint(&d),final_state);
        }
    }
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
