//! Reversible document commands. No browser, renderer or UI state.
use crate::{Document, Rgba8, Tile, TileCoord, CPU_BUDGET};
use std::collections::HashSet;
use std::sync::Arc;

/// Commands must apply/revert exactly, including transparent RGB and sparse tiles.
/// Retained allocations are enumerated so shared COW generations count once.
pub trait Command {
    fn apply(&self, document: &mut Document);
    fn revert(&self, document: &mut Document);
    fn visit_tiles(&self, visitor: &mut dyn FnMut(&Arc<Tile<Rgba8>>));
    fn is_empty(&self) -> bool;
}

struct TileChange {
    coord: TileCoord,
    before: Option<Arc<Tile<Rgba8>>>,
    after: Option<Arc<Tile<Rgba8>>>,
}

/// An atomic tile delta, also used by the brush's single-command stroke commit.
pub struct TileCommand {
    changes: Vec<TileChange>,
}
impl TileCommand {
    pub fn between(before: &Document, after: &Document) -> Result<Self, String> {
        if (before.width, before.height, before.dpi) != (after.width, after.height, after.dpi) {
            return Err("Tile commands cannot change document dimensions or resolution".into());
        }
        let coordinates: HashSet<_> = before
            .raster
            .tiles()
            .keys()
            .chain(after.raster.tiles().keys())
            .copied()
            .collect();
        let changes = coordinates
            .into_iter()
            .filter_map(|coord| {
                let old = before.raster.tiles().get(&coord);
                let new = after.raster.tiles().get(&coord);
                if old.map(|t| t.id) == new.map(|t| t.id)
                    || old.map(|t| &t.pixels) == new.map(|t| &t.pixels)
                {
                    return None;
                }
                Some(TileChange {
                    coord,
                    before: old.cloned(),
                    after: new.cloned(),
                })
            })
            .collect();
        Ok(Self { changes })
    }
}
impl Command for TileCommand {
    fn apply(&self, document: &mut Document) {
        for change in &self.changes {
            document.raster.restore(change.coord, change.after.clone());
        }
    }
    fn revert(&self, document: &mut Document) {
        for change in &self.changes {
            document.raster.restore(change.coord, change.before.clone());
        }
    }
    fn visit_tiles(&self, visitor: &mut dyn FnMut(&Arc<Tile<Rgba8>>)) {
        for change in &self.changes {
            for tile in [&change.before, &change.after].into_iter().flatten() {
                visitor(tile);
            }
        }
    }
    fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// P2 never silently evicts history. P3 adds user budgets/eviction and a panel.
#[derive(Default)]
pub struct History {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}
impl History {
    pub fn depths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    pub fn bytes(&self, document: &Document) -> u64 {
        retained_bytes(
            document,
            self.undo.iter().chain(&self.redo).map(|c| c.as_ref()),
        )
    }
    pub fn execute(
        &mut self,
        document: &mut Document,
        command: Box<dyn Command>,
        temporary: u64,
    ) -> Result<bool, String> {
        if command.is_empty() {
            return Ok(false);
        }
        let mut candidate = document.clone();
        command.apply(&mut candidate);
        let history = retained_bytes(
            &candidate,
            self.undo
                .iter()
                .map(|c| c.as_ref())
                .chain(std::iter::once(command.as_ref())),
        );
        if candidate
            .raster
            .bytes()
            .saturating_add(history)
            .saturating_add(temporary)
            > CPU_BUDGET
        {
            return Err("Undo history and this edit exceed the 768 MiB CPU budget; undo an edit or create/open a document".into());
        }
        *document = candidate;
        self.redo.clear();
        self.undo.push(command);
        Ok(true)
    }
    pub fn undo(&mut self, document: &mut Document) -> bool {
        let Some(command) = self.undo.pop() else {
            return false;
        };
        command.revert(document);
        self.redo.push(command);
        true
    }
    pub fn redo(&mut self, document: &mut Document) -> bool {
        let Some(command) = self.redo.pop() else {
            return false;
        };
        command.apply(document);
        self.undo.push(command);
        true
    }
}
fn retained_bytes<'a>(document: &Document, commands: impl Iterator<Item = &'a dyn Command>) -> u64 {
    let mut seen: HashSet<_> = document.raster.tiles().values().map(|t| t.id).collect();
    let mut bytes = 0;
    for command in commands {
        command.visit_tiles(&mut |tile| {
            if seen.insert(tile.id) {
                bytes += tile.bytes();
            }
        });
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edit(doc: &Document, x: u32, pixel: [u8; 4]) -> Box<dyn Command> {
        let mut after = doc.clone();
        after.raster.write(x, 0, Rgba8(pixel));
        Box::new(TileCommand::between(doc, &after).unwrap())
    }
    #[test]
    fn exact_undo_redo_branch_and_unique_residency() {
        let mut doc = Document::blank(600, 1, 72.0).unwrap();
        doc.raster.write(0, 0, Rgba8([87, 33, 19, 0]));
        let original_id = doc.raster.tiles()[&(0, 0)].id;
        let mut h = History::default();
        let first = edit(&doc, 0, [201, 3, 7, 1]);
        h.execute(&mut doc, first, 0).unwrap();
        let second = edit(&doc, 256, [0, 0, 255, 255]);
        h.execute(&mut doc, second, 0).unwrap();
        assert_eq!(h.bytes(&doc), 262144);
        assert!(h.undo(&mut doc));
        assert_eq!(doc.raster.tiles().len(), 1);
        assert_eq!(h.bytes(&doc), 2 * 262144);
        assert!(h.undo(&mut doc));
        assert_eq!(doc.raster.sample(0, 0), Rgba8([87, 33, 19, 0]));
        assert_eq!(doc.raster.tiles()[&(0, 0)].id, original_id);
        assert!(h.redo(&mut doc));
        let branch = edit(&doc, 1, [1, 2, 3, 255]);
        assert!(h.execute(&mut doc, branch, 0).unwrap());
        assert_eq!(h.depths(), (2, 0));
        assert!(!h.redo(&mut doc));
        let refused = edit(&doc, 511, [4, 5, 6, 255]);
        assert!(h.execute(&mut doc, refused, CPU_BUDGET).is_err());
        assert_eq!(doc.raster.sample(511, 0), Rgba8::default());
        assert_eq!(h.depths(), (2, 0));
    }
    #[test]
    fn empty_or_rejected_command_preserves_redo_and_dirty_state() {
        let mut doc = Document::blank(513, 1, 72.0).unwrap();
        let mut history = History::default();
        let command = edit(&doc, 256, [1, 2, 3, 1]);
        history.execute(&mut doc, command, 0).unwrap();
        assert!(history.undo(&mut doc));
        doc.raster.drain_dirty();
        let empty = Box::new(TileCommand::between(&doc, &doc).unwrap());
        assert!(!history.execute(&mut doc, empty, 0).unwrap());
        let rejected = edit(&doc, 0, [4, 5, 6, 0]);
        assert!(history.execute(&mut doc, rejected, CPU_BUDGET).is_err());
        assert_eq!(history.depths(), (0, 1));
        assert!(doc.raster.drain_dirty().is_empty());
        assert_eq!(doc.raster.bytes(), 0);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.raster.sample(256, 0), Rgba8([1, 2, 3, 1]));
        assert!(doc.raster.drain_dirty().contains(&(1, 0)));
        history.clear();
        assert_eq!(history.bytes(&doc), 0);
    }
    #[test]
    fn randomized_commands_undo_all_and_redo_all() {
        for seed in 1..=500_u64 {
            let mut random = seed;
            let mut doc = Document::blank(513, 1, 72.0).unwrap();
            let mut h = History::default();
            for _ in 0..20 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                let c = edit(
                    &doc,
                    (random >> 32) as u32 % 513,
                    random.to_le_bytes()[..4].try_into().unwrap(),
                );
                h.execute(&mut doc, c, 0).unwrap();
            }
            let final_pixels: Vec<_> = (0..513).map(|x| doc.raster.sample(x, 0)).collect();
            while h.undo(&mut doc) {}
            assert_eq!(doc.raster.tiles().len(), 0);
            while h.redo(&mut doc) {}
            assert_eq!(
                (0..513)
                    .map(|x| doc.raster.sample(x, 0))
                    .collect::<Vec<_>>(),
                final_pixels
            );
        }
    }
}
