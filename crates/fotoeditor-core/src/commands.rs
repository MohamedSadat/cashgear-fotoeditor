//! Reversible document commands. No browser, renderer or UI state.
use crate::{Document, LayerId, Rgba8, TileCoord, TileHandle, TileRef, CPU_BUDGET};
use std::collections::HashSet;

/// Commands must apply/revert exactly, including transparent RGB and sparse tiles.
/// Retained allocations are enumerated so shared COW generations count once.
pub trait Command: std::any::Any {
    fn label(&self) -> &str {
        "Brush stroke"
    }
    fn merged(&self, _next: &dyn Command) -> Option<Box<dyn Command>> {
        None
    }
    fn apply(&self, document: &mut Document);
    fn revert(&self, document: &mut Document);
    fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>));
    fn is_empty(&self) -> bool;
}

struct TileChange {
    coord: TileCoord,
    before: Option<TileHandle<Rgba8>>,
    after: Option<TileHandle<Rgba8>>,
}

/// An atomic tile delta, also used by the brush's single-command stroke commit.
pub struct TileCommand {
    layer: LayerId,
    changes: Vec<TileChange>,
}
impl TileCommand {
    pub fn between(before: &Document, after: &Document) -> Result<Self, String> {
        let layer = before.layers.first().ok_or("No raster layer")?.id;
        Self::for_layer(before, after, layer)
    }
    pub fn for_layer(before: &Document, after: &Document, layer: LayerId) -> Result<Self, String> {
        if (before.width, before.height, before.dpi) != (after.width, after.height, after.dpi) {
            return Err("Tile commands cannot change document dimensions or resolution".into());
        }
        let before_samples = before.layer(layer)?.raster_samples()?;
        let after_samples = after.layer(layer)?.raster_samples()?;
        let coordinates: HashSet<_> = before_samples
            .tiles()
            .keys()
            .chain(after_samples.tiles().keys())
            .copied()
            .collect();
        let changes = coordinates
            .into_iter()
            .filter_map(|coord| {
                let old = before_samples.tiles().get(&coord);
                let new = after_samples.tiles().get(&coord);
                if old.map(|t| t.id()) == new.map(|t| t.id())
                    || old.zip(new).is_some_and(|(a, b)| match (a.pin(), b.pin()) {
                        (Ok(a), Ok(b)) => a.pixels == b.pixels,
                        _ => false,
                    })
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
        Ok(Self { layer, changes })
    }
}
impl Command for TileCommand {
    fn apply(&self, document: &mut Document) {
        for change in &self.changes {
            document
                .layer_mut(self.layer)
                .expect("command layer")
                .raster_samples_mut()
                .expect("raster command")
                .restore(change.coord, change.after.clone());
        }
    }
    fn revert(&self, document: &mut Document) {
        for change in &self.changes {
            document
                .layer_mut(self.layer)
                .expect("command layer")
                .raster_samples_mut()
                .expect("raster command")
                .restore(change.coord, change.before.clone());
        }
    }
    fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for change in &self.changes {
            for tile in [&change.before, &change.after].into_iter().flatten() {
                visitor(TileRef::Rgba(tile));
            }
        }
    }
    fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// A document property command. Gesture IDs prevent merges across focus changes.
pub struct ResolutionCommand {
    pub before: f64,
    pub after: f64,
    pub gesture: u32,
}
impl Command for ResolutionCommand {
    fn label(&self) -> &str {
        "Resolution"
    }
    fn apply(&self, document: &mut Document) {
        document.dpi = self.after;
    }
    fn revert(&self, document: &mut Document) {
        document.dpi = self.before;
    }
    fn visit_tiles(&self, _: &mut dyn FnMut(TileRef<'_>)) {}
    fn is_empty(&self) -> bool {
        self.before == self.after
    }
    fn merged(&self, next: &dyn Command) -> Option<Box<dyn Command>> {
        let next = (next as &dyn std::any::Any).downcast_ref::<Self>()?;
        (self.gesture == next.gesture && self.after == next.before).then(|| {
            Box::new(Self {
                before: self.before,
                after: next.after,
                gesture: self.gesture,
            }) as Box<dyn Command>
        })
    }
}
#[derive(Clone, Copy)]
pub struct HistoryLimits {
    pub bytes: u64,
    pub steps: usize,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            bytes: 256 * 1024 * 1024,
            steps: 200,
        }
    }
}
/// Chronological commands with a cursor; dropped prefixes become the new baseline.
#[derive(Default)]
pub struct History {
    entries: Vec<Box<dyn Command>>,
    cursor: usize,
    pub limits: HistoryLimits,
    pub evicted: usize,
    merge_allowed: bool,
}
impl History {
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for command in &self.entries {
            command.visit_tiles(visitor);
        }
    }
    pub fn depths(&self) -> (usize, usize) {
        (self.cursor, self.entries.len() - self.cursor)
    }
    pub fn labels(&self) -> Vec<&str> {
        self.entries.iter().map(|c| c.label()).collect()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cursor = 0;
        self.evicted = 0;
        self.merge_allowed = false;
    }
    pub fn bytes(&self, document: &Document) -> u64 {
        retained_bytes(document, self.entries.iter().map(|c| c.as_ref()))
    }
    pub fn configure(&mut self, document: &Document, limits: HistoryLimits) -> Result<(), String> {
        if limits.bytes > CPU_BUDGET || limits.steps > 2000 {
            return Err("History limits must be 0–768 MiB and 0–2000 steps".into());
        }
        self.limits = limits;
        self.trim(document, 0);
        self.merge_allowed = false;
        Ok(())
    }
    pub fn reserve(&mut self, document: &Document, temporary: u64) -> Result<(), String> {
        if document.bytes().saturating_add(temporary) > CPU_BUDGET {
            return Err("The document and this operation exceed the 768 MiB CPU budget".into());
        }
        self.trim(document, temporary);
        self.merge_allowed = false;
        Ok(())
    }
    fn trim(&mut self, document: &Document, temporary: u64) {
        while !self.entries.is_empty()
            && (self.limits.bytes == 0
                || self.entries.len() > self.limits.steps
                || self.bytes(document) > self.limits.bytes
                || document
                    .bytes()
                    .saturating_add(self.bytes(document))
                    .saturating_add(temporary)
                    > CPU_BUDGET)
        {
            if self.cursor > 0 {
                self.entries.remove(0);
                self.cursor -= 1;
            } else {
                self.entries.pop();
            }
            self.evicted += 1;
        }
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
        if candidate.bytes().saturating_add(temporary) > CPU_BUDGET {
            return Err("This edit and temporary buffers exceed the 768 MiB CPU budget".into());
        }
        let merged = if self.merge_allowed && self.cursor == self.entries.len() {
            self.entries.last().and_then(|c| c.merged(command.as_ref()))
        } else {
            None
        };
        self.entries.truncate(self.cursor);
        if let Some(merged) = merged {
            self.entries.pop();
            self.cursor -= 1;
            if !merged.is_empty() {
                self.entries.push(merged);
                self.cursor += 1;
            }
        } else {
            self.entries.push(command);
            self.cursor += 1;
        }
        *document = candidate;
        self.trim(document, temporary);
        self.merge_allowed = true;
        Ok(true)
    }
    pub fn jump(&mut self, document: &mut Document, target: usize) -> Result<bool, String> {
        if target > self.entries.len() {
            return Err("That history state is no longer available".into());
        }
        let changed = target != self.cursor;
        while self.cursor > target {
            self.cursor -= 1;
            self.entries[self.cursor].revert(document);
        }
        while self.cursor < target {
            self.entries[self.cursor].apply(document);
            self.cursor += 1;
        }
        self.merge_allowed = false;
        self.trim(document, 0);
        Ok(changed)
    }
    pub fn undo(&mut self, document: &mut Document) -> bool {
        self.cursor > 0 && self.jump(document, self.cursor - 1).unwrap_or(false)
    }
    pub fn redo(&mut self, document: &mut Document) -> bool {
        self.cursor < self.entries.len() && self.jump(document, self.cursor + 1).unwrap_or(false)
    }
}
fn retained_bytes<'a>(document: &Document, commands: impl Iterator<Item = &'a dyn Command>) -> u64 {
    let mut seen = HashSet::new();
    document.visit_tiles(&mut |tile| {
        seen.insert(tile.id());
    });
    let mut bytes = 0;
    for command in commands {
        command.visit_tiles(&mut |tile| {
            if seen.insert(tile.id()) {
                bytes += tile.bytes();
            }
        });
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::hash::{Hash, Hasher};
    #[test]
    fn paged_undo_keeps_identity_and_exact_samples_without_pin_retention() {
        use crate::{TileBacking, TilePager};
        let mut document = Document::blank(256, 1, 72.0).unwrap();
        document.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        let old = document.raster().tiles()[&(0, 0)].clone();
        let mut history = History::default();
        let command = edit(&document, 0, [201, 3, 7, 1]);
        history.execute(&mut document, command, 0).unwrap();
        let mut handles = vec![old.clone(), document.raster().tiles()[&(0, 0)].clone()];
        for command in &history.entries {
            command.visit_tiles(&mut |tile| {
                if let TileRef::Rgba(tile) = tile {
                    handles.push(tile.clone());
                }
            });
        }
        let mut pager = TilePager::default();
        let mut disk = std::collections::HashMap::new();
        for handle in &handles {
            pager.track(handle);
            disk.insert(handle.id(), handle.pin().unwrap().pixels.clone());
            pager
                .acknowledge_backing(
                    handle,
                    TileBacking {
                        store: 1,
                        offset: handle.id() * 262144,
                        length: 262144,
                        checksum: 0,
                    },
                )
                .unwrap();
        }
        assert_eq!(pager.evict_oldest(u64::MAX, &HashSet::new()), 524288);
        assert_eq!(document.bytes() + history.bytes(&document), 0);
        assert!(history.undo(&mut document));
        let restored = &document.raster().tiles()[&(0, 0)];
        assert_eq!(restored.id(), old.id());
        pager
            .hydrate(
                restored,
                restored.backing().unwrap(),
                disk[&restored.id()].clone(),
                crate::Residency::default(),
            )
            .unwrap();
        assert_eq!(document.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
        assert!(history.redo(&mut document));
        let restored = &document.raster().tiles()[&(0, 0)];
        pager
            .hydrate(
                restored,
                restored.backing().unwrap(),
                disk[&restored.id()].clone(),
                crate::Residency::default(),
            )
            .unwrap();
        assert_eq!(document.raster().sample(0, 0), Rgba8([201, 3, 7, 1]));
    }
    fn hash(d: &Document) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        d.dpi.to_bits().hash(&mut hash);
        for x in 0..d.width {
            d.raster().sample(x, 0).0.hash(&mut hash);
        }
        hash.finish()
    }
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]
        #[test]
        fn mixed_sequences_restore_exact_hash(actions in prop::collection::vec((0u8..6,0u32..513,any::<[u8;4]>()),1..80)) {
            let mut doc = Document::blank(513,1,72.0).unwrap();
            doc.raster_mut().write(255,0,Rgba8([87,33,19,0]));
            let initial = hash(&doc);
            let mut history = History::default();
            for (kind,x,p) in actions {
                match kind {
                    0 => { history.undo(&mut doc); }
                    1 => { history.redo(&mut doc); }
                    2 => {
                        let c = ResolutionCommand { before: doc.dpi, after: x as f64 + 1.0, gesture: x };
                        history.execute(&mut doc,Box::new(c),0).unwrap();
                    }
                    _ => { let c = edit(&doc,x,p); history.execute(&mut doc,c,0).unwrap(); }
                }
            }
            let last = hash(&doc);
            let cursor = history.depths().0;
            while history.undo(&mut doc) {}
            prop_assert_eq!(hash(&doc),initial);
            history.jump(&mut doc,cursor).unwrap();
            prop_assert_eq!(hash(&doc),last);
        }
    }
    #[test]
    fn limits_evict_applied_prefix_and_farthest_redo_and_preserve_branch() {
        let mut doc = Document::blank(513, 1, 72.0).unwrap();
        let mut h = History::default();
        h.configure(
            &doc,
            HistoryLimits {
                bytes: 262144,
                steps: 2,
            },
        )
        .unwrap();
        for n in 1..=6 {
            let c = edit(&doc, 0, [n, 0, 0, 255]);
            h.execute(&mut doc, c, 0).unwrap();
        }
        assert!(h.bytes(&doc) <= 262144);
        assert_eq!(h.depths(), (1, 0));
        assert_eq!(h.evicted, 5);
        h.undo(&mut doc);
        assert_eq!(doc.raster().sample(0, 0).0[0], 5);
        h.configure(&doc, HistoryLimits { bytes: 0, steps: 0 })
            .unwrap();
        assert_eq!(h.depths(), (0, 0));
        assert_eq!(h.bytes(&doc), 0);
        let c = edit(&doc, 256, [9, 8, 7, 1]);
        h.execute(&mut doc, c, 0).unwrap();
        assert_eq!(doc.raster().sample(256, 0), Rgba8([9, 8, 7, 1]));
        assert_eq!(h.depths(), (0, 0));
    }
    #[test]
    fn resolution_merges_only_within_a_gesture_and_never_after_navigation() {
        let mut doc = Document::blank(1, 1, 72.0).unwrap();
        let mut h = History::default();
        for (dpi, gesture) in [(100., 1), (200., 1), (300., 1), (400., 2)] {
            let c = ResolutionCommand {
                before: doc.dpi,
                after: dpi,
                gesture,
            };
            h.execute(&mut doc, Box::new(c), 0).unwrap();
        }
        assert_eq!(h.depths(), (2, 0));
        h.jump(&mut doc, 0).unwrap();
        assert_eq!(doc.dpi, 72.);
        h.redo(&mut doc);
        assert_eq!(doc.dpi, 300.);
        h.redo(&mut doc);
        let c = ResolutionCommand {
            before: doc.dpi,
            after: 500.,
            gesture: 2,
        };
        h.execute(&mut doc, Box::new(c), 0).unwrap();
        assert_eq!(h.depths(), (3, 0));
    }
    #[test]
    fn operation_reservation_evicts_before_allocation_and_rejection_preserves_history() {
        let mut doc = Document::blank(513, 1, 72.0).unwrap();
        let mut h = History::default();
        for n in 1..=4 {
            let c = edit(&doc, 0, [n, 33, 19, 0]);
            h.execute(&mut doc, c, 0).unwrap();
        }
        let before = hash(&doc);
        let depth = h.depths();
        assert!(h.reserve(&doc, CPU_BUDGET).is_err());
        assert_eq!(h.depths(), depth);
        h.reserve(&doc, CPU_BUDGET - doc.raster().bytes()).unwrap();
        assert_eq!(h.bytes(&doc), 0);
        assert_eq!(hash(&doc), before);
        assert!(h.evicted > 0);
    }
    fn edit(doc: &Document, x: u32, pixel: [u8; 4]) -> Box<dyn Command> {
        let mut after = doc.clone();
        after.raster_mut().write(x, 0, Rgba8(pixel));
        Box::new(TileCommand::between(doc, &after).unwrap())
    }
    #[test]
    fn exact_undo_redo_branch_and_unique_residency() {
        let mut doc = Document::blank(600, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        let original_id = doc.raster().tiles()[&(0, 0)].id();
        let mut h = History::default();
        let first = edit(&doc, 0, [201, 3, 7, 1]);
        h.execute(&mut doc, first, 0).unwrap();
        let second = edit(&doc, 256, [0, 0, 255, 255]);
        h.execute(&mut doc, second, 0).unwrap();
        assert_eq!(h.bytes(&doc), 262144);
        assert!(h.undo(&mut doc));
        assert_eq!(doc.raster().tiles().len(), 1);
        assert_eq!(h.bytes(&doc), 2 * 262144);
        assert!(h.undo(&mut doc));
        assert_eq!(doc.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
        assert_eq!(doc.raster().tiles()[&(0, 0)].id(), original_id);
        assert!(h.redo(&mut doc));
        let branch = edit(&doc, 1, [1, 2, 3, 255]);
        assert!(h.execute(&mut doc, branch, 0).unwrap());
        assert_eq!(h.depths(), (2, 0));
        assert!(!h.redo(&mut doc));
        let refused = edit(&doc, 511, [4, 5, 6, 255]);
        assert!(h.execute(&mut doc, refused, CPU_BUDGET).is_err());
        assert_eq!(doc.raster().sample(511, 0), Rgba8::default());
        assert_eq!(h.depths(), (2, 0));
    }
    #[test]
    fn empty_or_rejected_command_preserves_redo_and_dirty_state() {
        let mut doc = Document::blank(513, 1, 72.0).unwrap();
        let mut history = History::default();
        let command = edit(&doc, 256, [1, 2, 3, 1]);
        history.execute(&mut doc, command, 0).unwrap();
        assert!(history.undo(&mut doc));
        doc.raster_mut().drain_dirty();
        let empty = Box::new(TileCommand::between(&doc, &doc).unwrap());
        assert!(!history.execute(&mut doc, empty, 0).unwrap());
        let rejected = edit(&doc, 0, [4, 5, 6, 0]);
        assert!(history.execute(&mut doc, rejected, CPU_BUDGET).is_err());
        assert_eq!(history.depths(), (0, 1));
        assert!(doc.raster_mut().drain_dirty().is_empty());
        assert_eq!(doc.raster().bytes(), 0);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.raster().sample(256, 0), Rgba8([1, 2, 3, 1]));
        assert!(doc.raster_mut().drain_dirty().contains(&(1, 0)));
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
            let final_pixels: Vec<_> = (0..513).map(|x| doc.raster().sample(x, 0)).collect();
            while h.undo(&mut doc) {}
            assert_eq!(doc.raster().tiles().len(), 0);
            while h.redo(&mut doc) {}
            assert_eq!(
                (0..513)
                    .map(|x| doc.raster().sample(x, 0))
                    .collect::<Vec<_>>(),
                final_pixels
            );
        }
    }
}
