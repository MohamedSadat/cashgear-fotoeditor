use crate::SelectionMode;
use fotoeditor_core::{
    Gray8, Rgba8, Selection, TileCoord, TileRef, TileSurface, TILE_PIXELS, TILE_SIZE,
};
use serde::Deserialize;
use std::sync::Arc;
const FRONTIER: usize = 1_048_576;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WandOptions {
    pub seed: [u32; 2],
    pub tolerance: u8,
    pub contiguous: bool,
    pub antialias: bool,
    pub mode: SelectionMode,
    pub color: Option<[u8; 4]>,
}
impl Default for WandOptions {
    fn default() -> Self {
        Self {
            seed: [0, 0],
            tolerance: 32,
            contiguous: true,
            antialias: true,
            mode: SelectionMode::New,
            color: None,
        }
    }
}

/// Matching compares visible sRGB and alpha; hidden RGB never changes a visual match.
fn visible(p: Rgba8) -> [u16; 4] {
    [
        ((u16::from(p.0[0]) * u16::from(p.0[3]) + 127) / 255),
        ((u16::from(p.0[1]) * u16::from(p.0[3]) + 127) / 255),
        ((u16::from(p.0[2]) * u16::from(p.0[3]) + 127) / 255),
        u16::from(p.0[3]),
    ]
}
fn bit(bits: &[u64], i: usize) -> bool {
    bits[i / 64] & (1u64 << (i % 64)) != 0
}
fn set(bits: &mut [u64], i: usize) {
    bits[i / 64] |= 1u64 << (i % 64);
}

/// Membership is processed tilewise, then a bounded four-connected scanline flood,
/// then a tilewise Gray8 conversion. No source-wide RGBA allocation is made.
pub struct WandJob {
    pub output: TileSurface<Gray8>,
    pub source: Arc<TileSurface<Rgba8>>,
    before: Selection,
    mode: SelectionMode,
    reference: [u16; 4],
    tolerance: u16,
    contiguous: bool,
    antialias: bool,
    matched: Vec<u64>,
    connected: Vec<u64>,
    frontier: Vec<(u32, u32)>,
    stage: u8,
    next: u32,
    available: u64,
    pub selected_pixels: u64,
}
impl WandJob {
    pub fn new(
        source: Arc<TileSurface<Rgba8>>,
        before: Selection,
        options: WandOptions,
        available: u64,
    ) -> Result<Self, String> {
        let WandOptions {
            seed,
            tolerance,
            contiguous,
            antialias,
            mode,
            color,
        } = options;
        fotoeditor_core::validate_size(source.width, source.height, 72.0)?;
        if seed[0] >= source.width || seed[1] >= source.height {
            return Err("Choose a pixel inside the canvas".into());
        }
        if before
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (source.width, source.height))
        {
            return Err("Selection dimensions differ".into());
        }
        let words = (source.width as usize * source.height as usize).div_ceil(64);
        let scratch = words as u64 * 8 * (1 + u64::from(contiguous))
            + if contiguous {
                (FRONTIER * size_of::<(u32, u32)>()) as u64
            } else {
                0
            }
            + 2 * TILE_PIXELS as u64;
        if scratch > available {
            return Err("Not enough memory for the bounded wand job".into());
        }
        // The bridge hydrates the seed tile before constructing the job.
        let seed_sample = if let Some(color) = color {
            Rgba8(color)
        } else {
            let tile = source
                .tiles()
                .get(&(seed[0] / TILE_SIZE, seed[1] / TILE_SIZE))
                .map(|t| t.pin())
                .transpose()?;
            tile.as_ref().map_or(source.default, |t| {
                t.pixels[((seed[1] % TILE_SIZE) * TILE_SIZE + seed[0] % TILE_SIZE) as usize]
            })
        };
        let reference = visible(seed_sample);
        let mut frontier = if contiguous {
            Vec::with_capacity(FRONTIER)
        } else {
            Vec::new()
        };
        if contiguous {
            frontier.push((seed[0], seed[1]));
        }
        let default = mode.combine(before.as_ref().map_or(0, |s| s.default.0), 0);
        Ok(Self {
            output: TileSurface::with_default(source.width, source.height, Gray8(default)),
            source,
            before,
            mode,
            reference,
            tolerance: u16::from(tolerance),
            contiguous,
            antialias,
            matched: vec![0; words],
            connected: if contiguous {
                vec![0; words]
            } else {
                Vec::new()
            },
            frontier,
            stage: 0,
            next: 0,
            available,
            selected_pixels: 0,
        })
    }
    fn tiles(&self) -> u32 {
        self.output.width.div_ceil(TILE_SIZE) * self.output.height.div_ceil(TILE_SIZE)
    }
    fn coord(&self) -> TileCoord {
        (
            self.next % self.output.width.div_ceil(TILE_SIZE),
            self.next / self.output.width.div_ceil(TILE_SIZE),
        )
    }
    pub fn done(&self) -> bool {
        self.stage == 3
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if self.stage == 0 {
            if let Some(t) = self.source.tiles().get(&self.coord()) {
                visitor(TileRef::Rgba(t));
            }
        }
        if self.stage == 2 {
            if let Some(t) = self
                .before
                .as_ref()
                .and_then(|s| s.tiles().get(&self.coord()))
            {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn temporary_bytes(&self) -> u64 {
        ((self.matched.capacity() + self.connected.capacity()) * 8
            + self.frontier.capacity() * size_of::<(u32, u32)>()) as u64
            + self.output.bytes()
            + TILE_PIXELS as u64
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.temporary_bytes() + TILE_PIXELS as u64 > self.available {
            return Err("Wand exceeds the shared CPU budget".into());
        }
        match self.stage {
            0 => self.membership()?,
            1 => self.flood()?,
            2 => self.convert()?,
            _ => {}
        }
        Ok(self.done())
    }
    #[inline(never)]
    fn membership(&mut self) -> Result<(), String> {
        let c = self.coord();
        let tile = self.source.tiles().get(&c).map(|t| t.pin()).transpose()?;
        for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
            let row = (c.1 * TILE_SIZE + y) as usize * self.output.width as usize
                + c.0 as usize * TILE_SIZE as usize;
            for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                let p = visible(tile.as_ref().map_or(self.source.default, |t| {
                    t.pixels[(y * TILE_SIZE + x) as usize]
                }));
                let distance = (0..4)
                    .map(|i| p[i].abs_diff(self.reference[i]))
                    .max()
                    .unwrap();
                if distance <= self.tolerance {
                    set(&mut self.matched, row + x as usize);
                }
            }
        }
        self.next += 1;
        if self.next == self.tiles() {
            self.next = 0;
            self.stage = if self.contiguous { 1 } else { 2 };
        }
        Ok(())
    }
    fn flood(&mut self) -> Result<(), String> {
        let w = self.output.width;
        let h = self.output.height;
        let mut work = 0;
        while work < 65536 {
            let Some((x, y)) = self.frontier.pop() else {
                self.stage = 2;
                return Ok(());
            };
            let row = y as usize * w as usize;
            work += 1;
            if !bit(&self.matched, row + x as usize) || bit(&self.connected, row + x as usize) {
                continue;
            }
            let mut l = x;
            let mut r = x;
            while l > 0
                && bit(&self.matched, row + l as usize - 1)
                && !bit(&self.connected, row + l as usize - 1)
            {
                l -= 1;
            }
            while r + 1 < w
                && bit(&self.matched, row + r as usize + 1)
                && !bit(&self.connected, row + r as usize + 1)
            {
                r += 1;
            }
            for xx in l..=r {
                set(&mut self.connected, row + xx as usize);
            }
            work += r - l + 1;
            for yy in [y.checked_sub(1), if y + 1 < h { Some(y + 1) } else { None }]
                .into_iter()
                .flatten()
            {
                let neighbor = yy as usize * w as usize;
                let mut in_run = false;
                for xx in l..=r {
                    let matched = bit(&self.matched, neighbor + xx as usize)
                        && !bit(&self.connected, neighbor + xx as usize);
                    if matched && !in_run {
                        if self.frontier.len() == FRONTIER {
                            return Err(
                                "Wand frontier exceeds its bounded capacity; selection unchanged"
                                    .into(),
                            );
                        }
                        self.frontier.push((xx, yy));
                    }
                    in_run = matched;
                }
            }
        }
        Ok(())
    }
    #[inline(never)]
    fn convert(&mut self) -> Result<(), String> {
        let c = self.coord();
        let old = self
            .before
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let bits = if self.contiguous {
            &self.connected
        } else {
            &self.matched
        };
        let mut pixels = vec![self.output.default; TILE_PIXELS];
        let w = self.output.width;
        let h = self.output.height;
        for y in 0..TILE_SIZE.min(h - c.1 * TILE_SIZE) {
            for x in 0..TILE_SIZE.min(w - c.0 * TILE_SIZE) {
                let xx = c.0 * TILE_SIZE + x;
                let yy = c.1 * TILE_SIZE + y;
                let i = (y * TILE_SIZE + x) as usize;
                let center = bit(bits, yy as usize * w as usize + xx as usize);
                self.selected_pixels += u64::from(center);
                let coverage = if self.antialias {
                    let mut sum = 0u32;
                    for (dy, wy) in [(-1, 1), (0, 2), (1, 1)] {
                        for (dx, wx) in [(-1, 1), (0, 2), (1, 1)] {
                            let nx = (xx as i32 + dx).clamp(0, w as i32 - 1) as usize;
                            let ny = (yy as i32 + dy).clamp(0, h as i32 - 1) as usize;
                            sum += u32::from(bit(bits, ny * w as usize + nx)) * wx * wy;
                        }
                    }
                    ((sum * 255 + 8) / 16) as u8
                } else {
                    if center {
                        255
                    } else {
                        0
                    }
                };
                let before = old.as_ref().map_or_else(
                    || self.before.as_ref().map_or(0, |s| s.default.0),
                    |t| t.pixels[i].0,
                );
                pixels[i] = Gray8(self.mode.combine(before, coverage));
            }
        }
        self.output.insert(c, pixels);
        self.next += 1;
        if self.next == self.tiles() {
            self.stage = 3;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: TileSurface<Rgba8>, seed: [u32; 2], contiguous: bool, tolerance: u8) -> WandJob {
        let mut j = WandJob::new(
            Arc::new(s),
            None,
            WandOptions {
                seed,
                tolerance,
                contiguous,
                antialias: false,
                ..WandOptions::default()
            },
            64 * 1024 * 1024,
        )
        .unwrap();
        while !j.step().unwrap() {}
        j
    }
    #[test]
    fn connectivity_crosses_tile_seams_but_not_diagonals_or_a_barrier() {
        let mut s = TileSurface::with_default(513, 3, Rgba8([10, 20, 30, 255]));
        for y in 0..3 {
            s.write(256, y, Rgba8([255; 4]));
        }
        s.write(256, 1, Rgba8([10, 20, 30, 255]));
        let j = run(s.clone(), [0, 0], true, 0);
        assert_eq!(j.selected_pixels, 1537);
        assert_eq!(j.output.sample(512, 0), Gray8(255));
        s.write(256, 1, Rgba8([255; 4]));
        let j = run(s.clone(), [0, 0], true, 0);
        assert_eq!(j.selected_pixels, 768);
        assert_eq!(j.output.sample(257, 1), Gray8(0));
        let j = run(s, [0, 0], false, 0);
        assert_eq!(j.selected_pixels, 1536);
    }
    #[test]
    fn visual_tolerance_ignores_hidden_rgb_and_distinguishes_alpha() {
        let mut s = TileSurface::new(3, 1);
        s.write(0, 0, Rgba8([87, 33, 19, 0]));
        s.write(1, 0, Rgba8([255, 255, 255, 0]));
        s.write(2, 0, Rgba8([0, 0, 0, 1]));
        assert_eq!(run(s.clone(), [0, 0], true, 0).selected_pixels, 2);
        assert_eq!(run(s, [0, 0], true, 1).selected_pixels, 3);
    }
}
