use crate::{TileEdit, TileHandle};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
pub const TILE_SIZE: u32 = 256;
pub const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;
pub type TileCoord = (u32, u32);
pub type DirtySet = HashSet<TileCoord>;
pub type DirtyRegions = HashMap<TileCoord, TileRect>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}
impl TileRect {
    pub const FULL: Self = Self {
        x0: 0,
        y0: 0,
        x1: TILE_SIZE,
        y1: TILE_SIZE,
    };
    pub fn union(&mut self, other: Self) {
        self.x0 = self.x0.min(other.x0);
        self.y0 = self.y0.min(other.y0);
        self.x1 = self.x1.max(other.x1);
        self.y1 = self.y1.max(other.y1);
    }
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) fn next_tile_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
pub trait Pixel: Copy + Default + PartialEq + Send + Sync + 'static {
    fn is_opaque(self) -> bool {
        false
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Rgba8(pub [u8; 4]);
impl Pixel for Rgba8 {
    fn is_opaque(self) -> bool {
        self.0[3] == 255
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gray8(pub u8);
impl Pixel for Gray8 {}
/// Stable identity distinguishes COW generations for GPU/history residency.
pub struct Tile<P: Pixel> {
    pub id: u64,
    pub pixels: Vec<P>,
    pub known_opaque: bool,
}
impl<P: Pixel> Clone for Tile<P> {
    fn clone(&self) -> Self {
        Self {
            id: next_tile_id(),
            pixels: self.pixels.clone(),
            known_opaque: self.known_opaque,
        }
    }
}
impl<P: Pixel> Tile<P> {
    pub fn new(pixels: Vec<P>) -> Self {
        assert_eq!(pixels.len(), TILE_PIXELS);
        Self {
            id: next_tile_id(),
            known_opaque: pixels.iter().all(|p| p.is_opaque()),
            pixels,
        }
    }
    pub fn bytes(&self) -> u64 {
        (self.pixels.capacity() * size_of::<P>()) as u64
    }
}
#[derive(Clone)]
pub struct TileSurface<P: Pixel> {
    pub width: u32,
    pub height: u32,
    pub default: P,
    tiles: HashMap<TileCoord, TileHandle<P>>,
    dirty: DirtySet,
    regions: DirtyRegions,
}
impl<P: Pixel> TileSurface<P> {
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_default(width, height, P::default())
    }
    pub fn with_default(width: u32, height: u32, default: P) -> Self {
        Self {
            width,
            height,
            default,
            tiles: HashMap::new(),
            dirty: DirtySet::new(),
            regions: DirtyRegions::new(),
        }
    }
    pub fn tiles(&self) -> &HashMap<TileCoord, TileHandle<P>> {
        &self.tiles
    }
    pub fn insert_handle(&mut self, coord: TileCoord, tile: TileHandle<P>) -> Result<(), String> {
        if coord.0 >= self.width.div_ceil(TILE_SIZE) || coord.1 >= self.height.div_ceil(TILE_SIZE) {
            return Err("Tile coordinate lies outside the surface".into());
        }
        self.tiles.insert(coord, tile);
        self.mark_dirty(coord, TileRect::FULL);
        Ok(())
    }
    /// Scratch/command primitive; one COW check per tile row/dab.
    pub fn edit_tile(&mut self, coord: TileCoord) -> TileEdit<'_, P> {
        self.edit_region(coord, TileRect::FULL)
    }
    pub fn edit_region(&mut self, coord: TileCoord, region: TileRect) -> TileEdit<'_, P> {
        assert!(coord.0 * TILE_SIZE < self.width && coord.1 * TILE_SIZE < self.height);
        assert!(
            region.x0 < region.x1
                && region.y0 < region.y1
                && region.x1 <= TILE_SIZE
                && region.y1 <= TILE_SIZE
        );
        self.mark_dirty(coord, region);
        let tile = self.tiles.entry(coord).or_insert_with(|| {
            TileHandle::new(Arc::new(Tile::new(vec![self.default; TILE_PIXELS])))
        });
        tile.edit().expect("hydrate before editing")
    }
    pub fn compact_dirty(&mut self) {
        for coord in &self.dirty {
            if self.tiles.get(coord).is_some_and(|t| {
                t.pin()
                    .expect("hydrate before compacting")
                    .pixels
                    .iter()
                    .all(|p| *p == self.default)
            }) {
                self.tiles.remove(coord);
            } else if let Some(tile) = self.tiles.get_mut(coord) {
                tile.refresh_opacity();
            }
        }
    }
    /// Bounded job primitive: compact one resident tile without scanning a surface.
    pub fn compact_tile(&mut self, coord: TileCoord) -> Result<(), String> {
        let empty = match self.tiles.get(&coord) {
            Some(t) => t.pin()?.pixels.iter().all(|p| *p == self.default),
            None => return Ok(()),
        };
        if empty {
            self.tiles.remove(&coord);
        } else if let Some(t) = self.tiles.get_mut(&coord) {
            t.refresh_opacity();
        }
        Ok(())
    }
    pub fn bytes(&self) -> u64 {
        let mut seen = HashSet::new();
        self.tiles
            .values()
            .filter(|t| seen.insert(t.id()))
            .map(|t| t.resident_bytes())
            .sum()
    }
    pub fn drain_dirty(&mut self) -> DirtySet {
        self.regions.clear();
        std::mem::take(&mut self.dirty)
    }
    pub fn drain_regions(&mut self) -> DirtyRegions {
        self.dirty.clear();
        std::mem::take(&mut self.regions)
    }
    pub fn has_dirty(&self) -> bool {
        !self.regions.is_empty()
    }
    fn mark_dirty(&mut self, coord: TileCoord, region: TileRect) {
        self.dirty.insert(coord);
        self.regions
            .entry(coord)
            .and_modify(|r| r.union(region))
            .or_insert(region);
    }
    pub(crate) fn restore(&mut self, coord: TileCoord, tile: Option<TileHandle<P>>) {
        if let Some(tile) = tile {
            self.tiles.insert(coord, tile);
        } else {
            self.tiles.remove(&coord);
        }
        self.mark_dirty(coord, TileRect::FULL);
    }
    pub fn sample(&self, x: u32, y: u32) -> P {
        if x >= self.width || y >= self.height {
            return self.default;
        }
        self.tiles
            .get(&(x / TILE_SIZE, y / TILE_SIZE))
            .map_or(self.default, |t| {
                t.pin().expect("hydrate before sampling").pixels
                    [((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
            })
    }
    /// Builder/command primitive. Hidden nonzero RGB must remain allocated.
    pub fn insert(&mut self, coord: TileCoord, pixels: Vec<P>) {
        assert!(coord.0 * TILE_SIZE < self.width && coord.1 * TILE_SIZE < self.height);
        if pixels.iter().all(|p| *p == self.default) {
            self.tiles.remove(&coord);
        } else {
            self.tiles
                .insert(coord, TileHandle::new(Arc::new(Tile::new(pixels))));
        }
        self.mark_dirty(coord, TileRect::FULL);
    }
    pub fn write(&mut self, x: u32, y: u32, pixel: P) {
        assert!(x < self.width && y < self.height);
        let coord = (x / TILE_SIZE, y / TILE_SIZE);
        if pixel == self.default && !self.tiles.contains_key(&coord) {
            return;
        }
        {
            let tile = self.tiles.entry(coord).or_insert_with(|| {
                TileHandle::new(Arc::new(Tile::new(vec![self.default; TILE_PIXELS])))
            });
            tile.edit().expect("hydrate before writing")
                [((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize] = pixel;
        }
        self.mark_dirty(
            coord,
            TileRect {
                x0: x % TILE_SIZE,
                y0: y % TILE_SIZE,
                x1: x % TILE_SIZE + 1,
                y1: y % TILE_SIZE + 1,
            },
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dirty_regions_union_and_opacity_metadata_survive_cow() {
        let mut surface = TileSurface::new(256, 256);
        surface.insert((0, 0), vec![Rgba8([1, 2, 3, 255]); TILE_PIXELS]);
        assert!(surface.tiles()[&(0, 0)].pin().unwrap().known_opaque);
        surface.drain_regions();
        let before = surface.clone();
        surface.write(10, 20, Rgba8([99, 77, 33, 0]));
        surface.write(30, 40, Rgba8([3, 4, 5, 1]));
        assert_eq!(
            surface.drain_regions()[&(0, 0)],
            TileRect {
                x0: 10,
                y0: 20,
                x1: 31,
                y1: 41
            }
        );
        assert!(surface.drain_dirty().is_empty());
        assert!(before.tiles()[&(0, 0)].pin().unwrap().known_opaque);
        assert!(!surface.tiles()[&(0, 0)].pin().unwrap().known_opaque);
        surface.restore((0, 0), Some(before.tiles()[&(0, 0)].clone()));
        assert_eq!(surface.drain_regions()[&(0, 0)], TileRect::FULL);
        assert_eq!(surface.sample(10, 20), Rgba8([1, 2, 3, 255]));
    }
    #[test]
    fn sparse_exact_alpha_cow_and_dirty() {
        let mut s = TileSurface::<Rgba8>::new(6000, 4000);
        assert_eq!(s.bytes(), 0);
        for (x, p) in [
            Rgba8([87, 33, 19, 0]),
            Rgba8([201, 3, 7, 1]),
            Rgba8([3, 4, 5, 255]),
        ]
        .into_iter()
        .enumerate()
        {
            s.write(x as u32 + 255, 256, p);
        }
        let old = s.clone();
        let old_id = old.tiles()[&(1, 1)].id();
        s.write(256, 256, Rgba8([8, 9, 10, 2]));
        assert_eq!(old.sample(256, 256), Rgba8([201, 3, 7, 1]));
        assert_ne!(s.tiles()[&(1, 1)].id(), old_id);
        assert_eq!(s.bytes(), 2 * 256 * 256 * 4);
        assert_eq!(s.drain_dirty().len(), 2);
        assert!(s.drain_dirty().is_empty());
        assert_eq!(s.sample(255, 256), Rgba8([87, 33, 19, 0]));
    }
}
