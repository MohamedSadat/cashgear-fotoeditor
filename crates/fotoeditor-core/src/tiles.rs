use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
pub const TILE_SIZE: u32 = 256;
pub const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;
pub type TileCoord = (u32, u32);
pub type DirtySet = HashSet<TileCoord>;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub trait Pixel: Copy + Default + PartialEq + Send + Sync + 'static {}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Rgba8(pub [u8; 4]);
impl Pixel for Rgba8 {}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gray8(pub u8);
impl Pixel for Gray8 {}
/// Stable identity distinguishes COW generations for GPU/history residency.
pub struct Tile<P: Pixel> {
    pub id: u64,
    pub pixels: Vec<P>,
}
impl<P: Pixel> Clone for Tile<P> {
    fn clone(&self) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            pixels: self.pixels.clone(),
        }
    }
}
impl<P: Pixel> Tile<P> {
    pub fn new(pixels: Vec<P>) -> Self {
        assert_eq!(pixels.len(), TILE_PIXELS);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
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
    tiles: HashMap<TileCoord, Arc<Tile<P>>>,
    dirty: DirtySet,
}
impl<P: Pixel> TileSurface<P> {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            tiles: HashMap::new(),
            dirty: DirtySet::new(),
        }
    }
    pub fn tiles(&self) -> &HashMap<TileCoord, Arc<Tile<P>>> {
        &self.tiles
    }
    /// Scratch/command primitive; one COW check per tile row/dab.
    pub fn edit_tile(&mut self, coord: TileCoord) -> &mut [P] {
        assert!(coord.0 * TILE_SIZE < self.width && coord.1 * TILE_SIZE < self.height);
        self.dirty.insert(coord);
        let tile = self
            .tiles
            .entry(coord)
            .or_insert_with(|| Arc::new(Tile::new(vec![P::default(); TILE_PIXELS])));
        &mut Arc::make_mut(tile).pixels
    }
    pub fn compact_dirty(&mut self) {
        for coord in &self.dirty {
            if self
                .tiles
                .get(coord)
                .is_some_and(|t| t.pixels.iter().all(|p| *p == P::default()))
            {
                self.tiles.remove(coord);
            }
        }
    }
    pub fn bytes(&self) -> u64 {
        self.tiles.values().map(|t| t.bytes()).sum()
    }
    pub fn drain_dirty(&mut self) -> DirtySet {
        std::mem::take(&mut self.dirty)
    }
    pub(crate) fn restore(&mut self, coord: TileCoord, tile: Option<Arc<Tile<P>>>) {
        if let Some(tile) = tile {
            self.tiles.insert(coord, tile);
        } else {
            self.tiles.remove(&coord);
        }
        self.dirty.insert(coord);
    }
    pub fn sample(&self, x: u32, y: u32) -> P {
        if x >= self.width || y >= self.height {
            return P::default();
        }
        self.tiles
            .get(&(x / TILE_SIZE, y / TILE_SIZE))
            .map_or(P::default(), |t| {
                t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
            })
    }
    /// Builder/command primitive. Hidden nonzero RGB must remain allocated.
    pub fn insert(&mut self, coord: TileCoord, pixels: Vec<P>) {
        assert!(coord.0 * TILE_SIZE < self.width && coord.1 * TILE_SIZE < self.height);
        if pixels.iter().all(|p| *p == P::default()) {
            self.tiles.remove(&coord);
        } else {
            self.tiles.insert(coord, Arc::new(Tile::new(pixels)));
        }
        self.dirty.insert(coord);
    }
    pub fn write(&mut self, x: u32, y: u32, pixel: P) {
        assert!(x < self.width && y < self.height);
        let coord = (x / TILE_SIZE, y / TILE_SIZE);
        if pixel == P::default() && !self.tiles.contains_key(&coord) {
            return;
        }
        let tile = self
            .tiles
            .entry(coord)
            .or_insert_with(|| Arc::new(Tile::new(vec![P::default(); TILE_PIXELS])));
        Arc::make_mut(tile).pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize] = pixel;
        self.dirty.insert(coord);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
        let old_id = old.tiles()[&(1, 1)].id;
        s.write(256, 256, Rgba8([8, 9, 10, 2]));
        assert_eq!(old.sample(256, 256), Rgba8([201, 3, 7, 1]));
        assert_ne!(s.tiles()[&(1, 1)].id, old_id);
        assert_eq!(s.bytes(), 2 * 256 * 256 * 4);
        assert_eq!(s.drain_dirty().len(), 2);
        assert!(s.drain_dirty().is_empty());
        assert_eq!(s.sample(255, 256), Rgba8([87, 33, 19, 0]));
    }
}
