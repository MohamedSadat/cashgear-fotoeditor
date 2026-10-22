use crate::{Rgba8, TileCoord, TileSurface, TILE_SIZE};
/// Box-filter straight samples into a temporary premultiplied display mip.
/// One texel gutter samples adjacent tiles, preventing atlas/filter seams.
pub fn display_tile(surface: &TileSurface<Rgba8>, coord: TileCoord, lod: u32) -> Vec<u8> {
    let scale = 1u32 << lod;
    let side = TILE_SIZE / scale;
    let padded = side + 2;
    let mut out = vec![0; (padded * padded * 4) as usize];
    let tile = &surface.tiles()[&coord];
    for y in 0..padded {
        for x in 0..padded {
            let mut sum = [0u32; 4];
            for by in 0..scale {
                for bx in 0..scale {
                    let p = if x > 0 && x <= side && y > 0 && y <= side {
                        tile.pixels
                            [(((y - 1) * scale + by) * TILE_SIZE + (x - 1) * scale + bx) as usize]
                    } else {
                        let sx = coord.0 as i64 * TILE_SIZE as i64
                            + (x as i64 - 1) * scale as i64
                            + bx as i64;
                        let sy = coord.1 as i64 * TILE_SIZE as i64
                            + (y as i64 - 1) * scale as i64
                            + by as i64;
                        if sx < 0 || sy < 0 {
                            Rgba8::default()
                        } else {
                            surface.sample(sx as u32, sy as u32)
                        }
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
#[cfg(test)]
mod tests {
    use super::*;
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
}
