use crate::{CanvasRect, Rgba8, TileCoord, TileRect, TileSurface, TILE_SIZE};
// A mip gutter can touch only the eight neighboring source tiles. Pin them once
// for the kernel instead of taking an Arc and residency lock for every sample.
struct Neighborhood<'a> {
    surface: &'a TileSurface<Rgba8>,
    coord: TileCoord,
    tiles: [Option<std::sync::Arc<crate::Tile<Rgba8>>>; 9],
}
impl<'a> Neighborhood<'a> {
    fn new(surface: &'a TileSurface<Rgba8>, coord: TileCoord) -> Self {
        let tiles = std::array::from_fn(|i| {
            let x = i as i64 % 3 + i64::from(coord.0) - 1;
            let y = i as i64 / 3 + i64::from(coord.1) - 1;
            if x < 0 || y < 0 {
                return None;
            }
            surface
                .tiles()
                .get(&(x as u32, y as u32))
                .map(|t| t.pin().expect("hydrate display neighborhood"))
        });
        Self {
            surface,
            coord,
            tiles,
        }
    }
    #[inline(always)]
    fn sample(&self, x: i64, y: i64) -> Rgba8 {
        let x = x.clamp(0, i64::from(self.surface.width) - 1) as u32;
        let y = y.clamp(0, i64::from(self.surface.height) - 1) as u32;
        let index = ((i64::from(y / TILE_SIZE) - i64::from(self.coord.1) + 1) * 3
            + i64::from(x / TILE_SIZE)
            - i64::from(self.coord.0)
            + 1) as usize;
        self.tiles[index]
            .as_ref()
            .map_or(self.surface.default, |t| {
                t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
            })
    }
}
#[inline(never)]
pub fn display_region(surface: &TileSurface<Rgba8>, coord: TileCoord, rect: TileRect) -> Vec<u8> {
    let width = rect.x1 - rect.x0;
    let height = rect.y1 - rect.y0;
    let mut out = vec![0; (width * height * 4) as usize];
    let tile = surface.tiles()[&coord].pin().expect("hydrate display tile");
    let valid_w = (surface.width - coord.0 * TILE_SIZE).min(TILE_SIZE);
    let valid_h = (surface.height - coord.1 * TILE_SIZE).min(TILE_SIZE);
    for y in 0..height {
        for x in 0..width {
            let p = tile.pixels[(((rect.y0 + y).min(valid_h - 1)) * TILE_SIZE
                + (rect.x0 + x).min(valid_w - 1)) as usize];
            out[((y * width + x) * 4) as usize..][..4].copy_from_slice(&premultiplied(p));
        }
    }
    out
}
/// Box-filter straight samples into a temporary premultiplied display mip.
/// One texel gutter samples adjacent tiles, preventing atlas/filter seams.
#[inline(never)]
pub fn display_tile(surface: &TileSurface<Rgba8>, coord: TileCoord, lod: u32) -> Vec<u8> {
    if lod == 0 {
        return display_base_tile(surface, coord);
    }
    let scale = 1u32 << lod;
    let side = TILE_SIZE / scale;
    let padded = side + 2;
    let mut out = vec![0; (padded * padded * 4) as usize];
    let tile = surface.tiles()[&coord].pin().expect("hydrate display tile");
    let neighborhood = Neighborhood::new(surface, coord);
    let valid_w = (surface.width - coord.0 * TILE_SIZE).min(TILE_SIZE);
    let valid_h = (surface.height - coord.1 * TILE_SIZE).min(TILE_SIZE);
    for y in 0..padded {
        for x in 0..padded {
            let mut sum = [0u32; 4];
            for by in 0..scale {
                for bx in 0..scale {
                    let p = if x > 0 && x <= side && y > 0 && y <= side {
                        tile.pixels[(((y - 1) * scale + by).min(valid_h - 1) * TILE_SIZE
                            + ((x - 1) * scale + bx).min(valid_w - 1))
                            as usize]
                    } else {
                        let sx = coord.0 as i64 * TILE_SIZE as i64
                            + (x as i64 - 1) * scale as i64
                            + bx as i64;
                        let sy = coord.1 as i64 * TILE_SIZE as i64
                            + (y as i64 - 1) * scale as i64
                            + by as i64;
                        neighborhood.sample(sx, sy)
                    };
                    let a = p.0[3] as u32;
                    for (s, c) in sum[..3].iter_mut().zip(p.0) {
                        *s += c as u32 * a;
                    }
                    sum[3] += a;
                }
            }
            let n = scale * scale;
            let i = ((y * padded + x) * 4) as usize;
            for c in 0..3 {
                out[i + c] = ((sum[c] + n * 127) / (n * 255)) as u8;
            }
            out[i + 3] = ((sum[3] + n / 2) / n) as u8;
        }
    }
    out
}
#[inline(always)]
fn premultiplied(p: Rgba8) -> [u8; 4] {
    let [r, g, b, a] = p.0;
    if a == 255 {
        return p.0;
    }
    let a32 = a as u32;
    [
        ((r as u32 * a32 + 127) / 255) as u8,
        ((g as u32 * a32 + 127) / 255) as u8,
        ((b as u32 * a32 + 127) / 255) as u8,
        a,
    ]
}
/// Crop-aware temporary display samples. Hidden pixels never contribute to a
/// visible window's edge mip or interpolation gutter; authoritative tiles stay
/// unchanged. An intersecting tile's clamp can touch only its existing neighbors.
#[inline(never)]
pub fn display_tile_window(
    surface: &TileSurface<Rgba8>,
    coord: TileCoord,
    lod: u32,
    window: Option<CanvasRect>,
) -> Vec<u8> {
    let Some(c) = window else {
        return display_tile(surface, coord, lod);
    };
    let scale = 1u32 << lod;
    let side = TILE_SIZE / scale;
    let padded = side + 2;
    let sx = coord.0 * TILE_SIZE;
    let sy = coord.1 * TILE_SIZE;
    if sx >= c.x + c.width || sy >= c.y + c.height || sx + TILE_SIZE <= c.x || sy + TILE_SIZE <= c.y
    {
        return vec![0; (padded * padded * 4) as usize];
    }
    if sx as i64 - scale as i64 >= c.x as i64
        && sy as i64 - scale as i64 >= c.y as i64
        && sx + TILE_SIZE + scale <= c.x + c.width
        && sy + TILE_SIZE + scale <= c.y + c.height
    {
        return display_tile(surface, coord, lod);
    }
    let neighborhood = Neighborhood::new(surface, coord);
    let mut out = vec![0; (padded * padded * 4) as usize];
    for y in 0..padded {
        for x in 0..padded {
            let mut sum = [0u32; 4];
            for by in 0..scale {
                for bx in 0..scale {
                    let px = (sx as i64 + (x as i64 - 1) * scale as i64 + bx as i64)
                        .clamp(c.x as i64, (c.x + c.width - 1) as i64);
                    let py = (sy as i64 + (y as i64 - 1) * scale as i64 + by as i64)
                        .clamp(c.y as i64, (c.y + c.height - 1) as i64);
                    let p = neighborhood.sample(px, py);
                    let a = p.0[3] as u32;
                    for (s, v) in sum[..3].iter_mut().zip(p.0) {
                        *s += v as u32 * a;
                    }
                    sum[3] += a;
                }
            }
            let n = scale * scale;
            let i = ((y * padded + x) * 4) as usize;
            for k in 0..3 {
                out[i + k] = ((sum[k] + n * 127) / (n * 255)) as u8;
            }
            out[i + 3] = ((sum[3] + n / 2) / n) as u8;
        }
    }
    out
}
#[inline(never)]
fn display_base_tile(surface: &TileSurface<Rgba8>, coord: TileCoord) -> Vec<u8> {
    const PAD: usize = TILE_SIZE as usize + 2;
    let mut out = vec![0; PAD * PAD * 4];
    let tile = surface.tiles()[&coord].pin().expect("hydrate display tile");
    let w = (surface.width - coord.0 * TILE_SIZE).min(TILE_SIZE) as usize;
    let h = (surface.height - coord.1 * TILE_SIZE).min(TILE_SIZE) as usize;
    for y in 0..TILE_SIZE as usize {
        let source = &tile.pixels[y.min(h - 1) * TILE_SIZE as usize..][..TILE_SIZE as usize];
        let row = &mut out[((y + 1) * PAD + 1) * 4..][..TILE_SIZE as usize * 4];
        if tile.known_opaque && w == TILE_SIZE as usize {
            for (output, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(source) {
                *output = pixel.0;
            }
        } else {
            for (x, output) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                *output = premultiplied(source[x.min(w - 1)]);
            }
        }
    }
    let sx = coord.0 as i64 * TILE_SIZE as i64;
    let sy = coord.1 as i64 * TILE_SIZE as i64;
    let neighborhood = Neighborhood::new(surface, coord);
    let sample = |x: i64, y: i64| premultiplied(neighborhood.sample(x, y));
    for i in 0..PAD {
        let x = sx + i as i64 - 1;
        out[i * 4..][..4].copy_from_slice(&sample(x, sy - 1));
        out[((PAD - 1) * PAD + i) * 4..][..4].copy_from_slice(&sample(x, sy + TILE_SIZE as i64));
    }
    for y in 1..PAD - 1 {
        let py = sy + y as i64 - 1;
        out[y * PAD * 4..][..4].copy_from_slice(&sample(sx - 1, py));
        out[(y * PAD + PAD - 1) * 4..][..4].copy_from_slice(&sample(sx + TILE_SIZE as i64, py));
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_display_matches_full_tile_including_clamped_edges() {
        let mut surface = TileSurface::new(257, 1);
        for x in 0..257 {
            surface.write(x, 0, Rgba8([217, 33, 97, (x % 256) as u8]));
        }
        for coord in [(0, 0), (1, 0)] {
            let rect = TileRect {
                x0: 0,
                y0: 0,
                x1: 17,
                y1: 13,
            };
            let partial = display_region(&surface, coord, rect);
            let full = display_tile(&surface, coord, 0);
            for y in 0..13 {
                assert_eq!(
                    &partial[y * 17 * 4..][..17 * 4],
                    &full[((y + 1) * 258 + 1) * 4..][..17 * 4]
                );
            }
        }
    }
    #[test]
    fn base_fast_path_matches_reference_for_alpha_gutters_and_odd_edges() {
        let mut surface = TileSurface::new(513, 259);
        for coord in [(0, 0), (1, 0), (2, 1)] {
            let pixels = (0..crate::TILE_PIXELS)
                .map(|i| {
                    Rgba8([
                        (i % 256) as u8,
                        ((i * 37) % 256) as u8,
                        ((i * 71) % 256) as u8,
                        ((i * 19) % 256) as u8,
                    ])
                })
                .collect();
            surface.insert(coord, pixels);
        }
        for coord in [(0, 0), (1, 0), (2, 1)] {
            let actual = display_tile(&surface, coord, 0);
            for y in 0..258 {
                for x in 0..258 {
                    let interior = x > 0 && x < 257 && y > 0 && y < 257;
                    let sx = if interior {
                        (coord.0 * 256 + (x - 1).min(surface.width - coord.0 * 256 - 1)) as i64
                    } else {
                        coord.0 as i64 * 256 + x as i64 - 1
                    };
                    let sy = if interior {
                        (coord.1 * 256 + (y - 1).min(surface.height - coord.1 * 256 - 1)) as i64
                    } else {
                        coord.1 as i64 * 256 + y as i64 - 1
                    };
                    let Rgba8(p) = surface.sample(sx.clamp(0, 512) as u32, sy.clamp(0, 258) as u32);
                    let a = p[3] as u32;
                    let expected = [
                        ((p[0] as u32 * a + 127) / 255) as u8,
                        ((p[1] as u32 * a + 127) / 255) as u8,
                        ((p[2] as u32 * a + 127) / 255) as u8,
                        p[3],
                    ];
                    assert_eq!(&actual[((y * 258 + x) * 4) as usize..][..4], expected);
                }
            }
        }
    }
    #[test]
    fn mips_filter_premultiplied_and_do_not_change_samples() {
        let mut s = TileSurface::new(512, 256);
        s.write(0, 0, Rgba8([255, 0, 0, 0]));
        s.write(1, 0, Rgba8([0, 0, 255, 255]));
        s.write(256, 0, Rgba8([0, 255, 0, 255]));
        let mip = display_tile(&s, (0, 0), 1);
        let i = (130 + 1) * 4;
        assert_eq!(&mip[i..i + 4], &[0, 0, 64, 64]);
        assert_eq!(s.sample(0, 0), Rgba8([255, 0, 0, 0]));
        let base = display_tile(&s, (0, 0), 0);
        let gutter = (258 + 257) * 4;
        assert_eq!(&base[gutter..gutter + 4], &[0, 255, 0, 255]);
    }
    #[test]
    fn odd_document_edges_do_not_fade_into_padding() {
        let mut s = TileSurface::new(257, 1);
        for x in 0..257 {
            s.write(x, 0, Rgba8([255, 255, 255, 255]));
        }
        for lod in [0, 1, 4, 8] {
            assert!(display_tile(&s, (1, 0), lod).iter().all(|c| *c == 255));
        }
    }
    #[test]
    fn cropped_mips_and_gutters_exclude_hidden_neighbors_at_every_lod() {
        let mut s = TileSurface::new(770, 515);
        for y in 0..515 {
            for x in 0..770 {
                s.write(x, y, Rgba8([255, 0, 0, 255]));
            }
        }
        let window = CanvasRect {
            x: 253,
            y: 251,
            width: 9,
            height: 11,
        };
        for y in window.y..window.y + window.height {
            for x in window.x..window.x + window.width {
                s.write(x, y, Rgba8([0, 255, 0, 255]));
            }
        }
        let original = s.tiles()[&(0, 0)].id();
        for lod in [0, 1, 3, 8] {
            for c in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let bytes = display_tile_window(&s, c, lod, Some(window));
                assert!(bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [0, 255, 0, 255]));
            }
        }
        assert_eq!(s.sample(252, 251), Rgba8([255, 0, 0, 255]));
        assert_eq!(s.tiles()[&(0, 0)].id(), original);
        assert!(display_tile_window(&s, (2, 1), 4, Some(window))
            .iter()
            .all(|v| *v == 0));
    }
}
