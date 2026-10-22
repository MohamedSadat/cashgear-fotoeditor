//! Immutable tile identities independent of CPU residency. Storage I/O belongs to
//! the worker; processing crates see only validated backing extents and samples.
use crate::{Pixel, Residency, Tile, TILE_PIXELS};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, Weak};

/// A worker-owned immutable extent. Acknowledgement is allowed only after the
/// storage adapter has flushed and validated the exact generation's payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileBacking {
    pub store: u128,
    pub offset: u64,
    pub length: u32,
    pub checksum: u32,
}
struct Generation<P: Pixel> {
    id: u64,
    state: RwLock<State<P>>,
}
struct State<P: Pixel> {
    resident: Option<Arc<Tile<P>>>,
    backing: Option<TileBacking>,
}
/// Documents, history and pending candidates clone this handle, not the buffer.
/// A successful pin keeps a buffer resident until the returned Arc is dropped.
#[derive(Clone)]
pub struct TileHandle<P: Pixel>(Arc<Generation<P>>);
impl<P: Pixel> TileHandle<P> {
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn pin(&self) -> Result<Arc<Tile<P>>, String> {
        self.0
            .state
            .read()
            .map_err(|_| "Tile residency lock failed")?
            .resident
            .clone()
            .ok_or_else(|| "Tile is paged out; hydrate it before processing".into())
    }
    pub fn backing(&self) -> Option<TileBacking> {
        self.0.state.read().expect("tile lock").backing
    }
    pub fn resident_bytes(&self) -> u64 {
        self.0
            .state
            .read()
            .expect("tile lock")
            .resident
            .as_ref()
            .map_or(0, |tile| tile.bytes())
    }
}
struct Entry<P: Pixel> {
    generation: Weak<Generation<P>>,
    used: u64,
}
/// Weak indexing never extends a document/history generation's lifetime. Buffer
/// capacities are counted once regardless of the number of handle references.
pub struct TilePager<P: Pixel> {
    entries: HashMap<u64, Entry<P>>,
    clock: u64,
}
impl<P: Pixel> Default for TilePager<P> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
        }
    }
}
impl<P: Pixel> TilePager<P> {
    /// Callers reserve buffers before constructing them. Registering an existing
    /// Arc transfers residency ownership; external Arc clones behave as pins.
    pub fn register(&mut self, tile: Arc<Tile<P>>) -> TileHandle<P> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(&tile.id) {
            if let Some(generation) = entry.generation.upgrade() {
                entry.used = self.clock;
                return TileHandle(generation);
            }
        }
        let handle = TileHandle(Arc::new(Generation {
            id: tile.id,
            state: RwLock::new(State {
                resident: Some(tile),
                backing: None,
            }),
        }));
        self.entries.insert(
            handle.id(),
            Entry {
                generation: Arc::downgrade(&handle.0),
                used: self.clock,
            },
        );
        handle
    }
    pub fn touch(&mut self, handle: &TileHandle<P>) {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(&handle.id()) {
            entry.used = self.clock;
        }
    }
    /// No eviction is possible until storage has confirmed this exact payload.
    pub fn acknowledge_backing(
        &self,
        handle: &TileHandle<P>,
        backing: TileBacking,
    ) -> Result<(), String> {
        let expected = (TILE_PIXELS * size_of::<P>()) as u64;
        if u64::from(backing.length) != expected || backing.offset.checked_add(expected).is_none() {
            return Err("Invalid tile backing extent".into());
        }
        let mut state = handle.0.state.write().map_err(|_| "Tile lock failed")?;
        if let Some(old) = state.backing {
            if old != backing {
                return Err("An immutable tile backing cannot be replaced".into());
            }
        } else {
            if state.resident.is_none() {
                return Err("Cannot acknowledge an absent tile generation".into());
            }
            state.backing = Some(backing);
        }
        Ok(())
    }
    /// A pin, unbacked edit, or storage failure must never lose samples.
    pub fn evict(&self, handle: &TileHandle<P>) -> Result<u64, String> {
        let mut state = handle.0.state.write().map_err(|_| "Tile lock failed")?;
        if state.backing.is_none() {
            return Err("Tile has no validated durable backing".into());
        }
        if state
            .resident
            .as_ref()
            .is_some_and(|tile| Arc::strong_count(tile) > 1)
        {
            return Err("Tile is pinned by an active operation".into());
        }
        Ok(state.resident.take().map_or(0, |tile| tile.bytes()))
    }
    /// The I/O adapter must verify the backing checksum before passing samples.
    /// The supplied residency includes any live I/O/bridge staging buffers.
    pub fn hydrate(
        &mut self,
        handle: &TileHandle<P>,
        backing: TileBacking,
        pixels: Vec<P>,
        residency: Residency,
    ) -> Result<(), String> {
        if pixels.len() != TILE_PIXELS || handle.backing() != Some(backing) {
            return Err("Hydration does not match the tile generation".into());
        }
        let mut state = handle.0.state.write().map_err(|_| "Tile lock failed")?;
        if let Some(resident) = &state.resident {
            if resident.pixels != pixels {
                return Err("Hydration cannot replace immutable tile samples".into());
            }
            return Ok(());
        }
        residency.check_cpu((pixels.capacity() * size_of::<P>()) as u64)?;
        let mut tile = Tile::new(pixels);
        tile.id = handle.id();
        state.resident = Some(Arc::new(tile));
        drop(state);
        self.touch(handle);
        Ok(())
    }
    pub fn resident_bytes(&self) -> u64 {
        self.entries
            .values()
            .filter_map(|entry| entry.generation.upgrade())
            .map(|generation| TileHandle(generation).resident_bytes())
            .sum()
    }
    /// Evict only validated, unpinned generations in least-recently-used order.
    /// Returns released logical bytes; callers still check total admission.
    pub fn evict_oldest(&self, needed: u64, protected: &HashSet<u64>) -> u64 {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_unstable_by_key(|(id, entry)| (entry.used, **id));
        let mut released = 0;
        for (id, entry) in entries {
            if released >= needed {
                break;
            }
            if protected.contains(id) {
                continue;
            }
            if let Some(generation) = entry.generation.upgrade() {
                released += self.evict(&TileHandle(generation)).unwrap_or(0);
            }
        }
        released
    }
    pub fn collect_unused(&mut self) {
        self.entries
            .retain(|_, entry| entry.generation.strong_count() != 0);
    }
}

/// Classify a shared generation once, excluding document IDs from history.
pub fn paged_resident_bytes<P: Pixel>(
    handles: impl IntoIterator<Item = TileHandle<P>>,
    excluding: &HashSet<u64>,
) -> u64 {
    let mut seen = excluding.clone();
    handles
        .into_iter()
        .filter(|handle| seen.insert(handle.id()))
        .map(|handle| handle.resident_bytes())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Gray8, Rgba8, CPU_BUDGET};
    fn backing<P: Pixel>() -> TileBacking {
        TileBacking {
            store: 77,
            offset: 128,
            length: (TILE_PIXELS * size_of::<P>()) as u32,
            checksum: 19,
        }
    }
    #[test]
    fn eviction_requires_validated_backing_and_cannot_remove_a_pin() {
        let mut pager = TilePager::default();
        let handle = pager.register(Arc::new(Tile::new(vec![
            Rgba8([87, 33, 19, 0]);
            TILE_PIXELS
        ])));
        let history = handle.clone();
        assert!(pager.evict(&handle).is_err());
        pager
            .acknowledge_backing(&handle, backing::<Rgba8>())
            .unwrap();
        let pin = history.pin().unwrap();
        assert!(pager.evict(&handle).is_err());
        drop(pin);
        let id = handle.id();
        assert_eq!(pager.evict(&handle).unwrap(), 262144);
        assert_eq!(pager.resident_bytes(), 0);
        assert!(history.pin().is_err());
        pager
            .hydrate(
                &handle,
                backing::<Rgba8>(),
                vec![Rgba8([87, 33, 19, 0]); TILE_PIXELS],
                Residency::default(),
            )
            .unwrap();
        assert_eq!(history.pin().unwrap().id, id);
        assert_eq!(history.pin().unwrap().pixels[0], Rgba8([87, 33, 19, 0]));
        assert_eq!(
            paged_resident_bytes([handle.clone(), history], &HashSet::new()),
            262144
        );
        assert_eq!(paged_resident_bytes([handle], &HashSet::from([id])), 0);
    }
    #[test]
    fn copy_on_write_generation_does_not_change_paged_history() {
        let mut pager = TilePager::default();
        let old = pager.register(Arc::new(Tile::new(vec![
            Rgba8([201, 3, 7, 1]);
            TILE_PIXELS
        ])));
        let mut edited = old.pin().unwrap();
        Arc::make_mut(&mut edited).pixels[0] = Rgba8([9, 8, 7, 2]);
        let new = pager.register(edited);
        assert_ne!(old.id(), new.id());
        assert_eq!(old.pin().unwrap().pixels[0], Rgba8([201, 3, 7, 1]));
        assert_eq!(new.pin().unwrap().pixels[0], Rgba8([9, 8, 7, 2]));
    }
    #[test]
    fn lru_skips_protected_pinned_and_unbacked_then_releases_unused_handles() {
        let mut pager = TilePager::default();
        let handles: Vec<_> = (0..4)
            .map(|n| pager.register(Arc::new(Tile::new(vec![Gray8(n); TILE_PIXELS]))))
            .collect();
        for handle in &handles[..3] {
            pager
                .acknowledge_backing(handle, backing::<Gray8>())
                .unwrap();
        }
        let pin = handles[1].pin().unwrap();
        assert_eq!(
            pager.evict_oldest(u64::MAX, &HashSet::from([handles[0].id()])),
            65536
        );
        assert_eq!(pager.resident_bytes(), 3 * 65536);
        assert!(handles[2].pin().is_err());
        drop(pin);
        drop(handles);
        pager.collect_unused();
        assert!(pager.entries.is_empty());
    }
    #[test]
    fn hydration_and_backing_failures_preserve_the_generation() {
        let mut pager = TilePager::default();
        let handle = pager.register(Arc::new(Tile::new(vec![Gray8(19); TILE_PIXELS])));
        let valid = backing::<Gray8>();
        assert!(pager
            .acknowledge_backing(&handle, TileBacking { length: 1, ..valid })
            .is_err());
        pager.acknowledge_backing(&handle, valid).unwrap();
        assert!(pager
            .acknowledge_backing(
                &handle,
                TileBacking {
                    checksum: 20,
                    ..valid
                }
            )
            .is_err());
        pager.evict(&handle).unwrap();
        let full = Residency {
            temporary: CPU_BUDGET,
            ..Residency::default()
        };
        assert!(pager
            .hydrate(&handle, valid, vec![Gray8(19); TILE_PIXELS], full)
            .is_err());
        assert_eq!(handle.resident_bytes(), 0);
        assert!(pager
            .hydrate(&handle, valid, vec![Gray8(19)], Residency::default())
            .is_err());
        assert_eq!(handle.backing(), Some(valid));
    }
}
