use crate::SelectionMode;
use fotoeditor_core::{
    CanvasRect, Gray8, Rgba8, Selection, TileCoord, TileRef, TileSurface, TILE_PIXELS, TILE_SIZE,
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
#[inline(always)]
fn visible(p: Rgba8) -> [u16; 4] {
    if p.0[3] == 255 {
        return p.0.map(u16::from);
    }
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
    canvas: Option<CanvasRect>,
    pub output: TileSurface<Gray8>,
    pub source: Arc<TileSurface<Rgba8>>,
    before: Selection,
    domain: Selection,
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
    pub fn phase(&self) -> u32 {
        u32::from(self.stage)
    }
    pub fn finish(self) -> Result<(TileSurface<Gray8>, u64), String> {
        if !self.done() {
            return Err("Wand is incomplete".into());
        }
        Ok((self.output, self.selected_pixels))
    }
    pub fn new(
        source: Arc<TileSurface<Rgba8>>,
        before: Selection,
        options: WandOptions,
        available: u64,
    ) -> Result<Self, String> {
        Self::in_domain(source, before, options, None, available)
    }
    pub fn in_domain(
        source: Arc<TileSurface<Rgba8>>,
        before: Selection,
        options: WandOptions,
        domain: Selection,
        available: u64,
    ) -> Result<Self, String> {
        Self::in_canvas(source, before, options, domain, available, None)
    }
    pub fn in_canvas(
        source: Arc<TileSurface<Rgba8>>,
        before: Selection,
        options: WandOptions,
        domain: Selection,
        available: u64,
        canvas: Option<CanvasRect>,
    ) -> Result<Self, String> {
        if let Some(rect) = canvas {
            rect.validate(source.width, source.height)?;
            if (options.contiguous || options.color.is_none())
                && !rect.contains(options.seed[0], options.seed[1])
            {
                return Err("Choose a pixel inside the visible canvas".into());
            }
        }
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
        if domain
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (source.width, source.height))
        {
            return Err("Fill domain dimensions differ".into());
        }
        if tolerance == 255 && domain.is_none() && canvas.is_none() {
            let output = match mode {
                SelectionMode::New | SelectionMode::Add => {
                    TileSurface::with_default(source.width, source.height, Gray8(255))
                }
                SelectionMode::Subtract => TileSurface::new(source.width, source.height),
                SelectionMode::Intersect => before.as_ref().map_or_else(
                    || TileSurface::new(source.width, source.height),
                    |s| (**s).clone(),
                ),
            };
            let count = u64::from(source.width) * u64::from(source.height);
            return Ok(Self {
                canvas,
                output,
                source,
                before,
                domain,
                mode,
                reference: [0; 4],
                tolerance: 255,
                contiguous,
                antialias,
                matched: Vec::new(),
                connected: Vec::new(),
                frontier: Vec::new(),
                stage: 3,
                next: 0,
                available,
                selected_pixels: count,
            });
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
        let seed_sample = if tolerance == 255 {
            Rgba8::default()
        } else if let Some(color) = color {
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
        let job = Self {
            canvas,
            output: TileSurface::with_default(source.width, source.height, Gray8(default)),
            source,
            before,
            domain,
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
        };
        #[cfg(target_arch = "wasm32")]
        {
            // Grow the WASM allocator's free extent once, instead of requesting
            // many small linear-memory expansions while retaining tiled history.
            // This transient capacity is admitted alongside all live job buffers.
            let reserve = available
                .saturating_sub(job.temporary_bytes())
                .min(32 * 1024 * 1024) as usize;
            std::hint::black_box(Vec::<u8>::with_capacity(reserve));
        }
        Ok(job)
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
            let coord = self.coord();
            let empty = self
                .domain
                .as_ref()
                .is_some_and(|s| s.default.0 == 0 && !s.tiles().contains_key(&coord));
            if let Some(t) = self.domain.as_ref().and_then(|s| s.tiles().get(&coord)) {
                visitor(TileRef::Gray(t));
            }
            if let Some(t) = self
                .source
                .tiles()
                .get(&coord)
                .filter(|_| !empty && self.tolerance < 255)
            {
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
    pub fn visit_sources(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for t in self.source.tiles().values() {
            visitor(TileRef::Rgba(t));
        }
        for s in [&self.before, &self.domain].into_iter().flatten() {
            for t in s.tiles().values() {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn temporary_bytes(&self) -> u64 {
        ((self.matched.capacity() + self.connected.capacity()) * 8
            + self.frontier.capacity() * size_of::<(u32, u32)>()) as u64
            + self.output.tiles().len() as u64 * TILE_PIXELS as u64
            + TILE_PIXELS as u64
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
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
        let domain = self
            .domain
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let empty = self.domain.as_ref().is_some_and(|s| s.default.0 == 0) && domain.is_none();
        let tile = if !empty && self.tolerance < 255 {
            self.source.tiles().get(&c).map(|t| t.pin()).transpose()?
        } else {
            None
        };
        if !empty {
            for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
                let row = (c.1 * TILE_SIZE + y) as usize * self.output.width as usize
                    + c.0 as usize * TILE_SIZE as usize;
                if let Some(t) = tile.as_ref().filter(|t| t.known_opaque) {
                    let start = (y * TILE_SIZE) as usize;
                    let count = TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) as usize;
                    opaque_matches(
                        &mut self.matched,
                        row,
                        &t.pixels[start..start + count],
                        self.reference,
                        self.tolerance,
                    );
                    continue;
                }
                for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                    let p = visible(tile.as_ref().map_or(self.source.default, |t| {
                        t.pixels[(y * TILE_SIZE + x) as usize]
                    }));
                    if p[0].abs_diff(self.reference[0]) <= self.tolerance
                        && p[1].abs_diff(self.reference[1]) <= self.tolerance
                        && p[2].abs_diff(self.reference[2]) <= self.tolerance
                        && p[3].abs_diff(self.reference[3]) <= self.tolerance
                    {
                        set(&mut self.matched, row + x as usize);
                    }
                }
            }
        }
        if let Some(surface) = self.domain.as_ref().filter(|_| !empty) {
            let default = surface.default.0;
            for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                    if domain
                        .as_ref()
                        .map_or(default, |t| t.pixels[(y * TILE_SIZE + x) as usize].0)
                        == 0
                    {
                        let i = (c.1 * TILE_SIZE + y) as usize * self.output.width as usize
                            + (c.0 * TILE_SIZE + x) as usize;
                        self.matched[i / 64] &= !(1u64 << (i % 64));
                    }
                }
            }
        }
        if let Some(rect) = self.canvas {
            for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                    if !rect.contains(c.0 * TILE_SIZE + x, c.1 * TILE_SIZE + y) {
                        let i = ((c.1 * TILE_SIZE + y) * self.output.width + c.0 * TILE_SIZE + x)
                            as usize;
                        self.matched[i / 64] &= !(1u64 << (i % 64));
                    }
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
            set_span(&mut self.connected, row + l as usize, row + r as usize + 1);
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
        let mut pixels = vec![self.output.default.0; TILE_PIXELS];
        let w = self.output.width;
        let h = self.output.height;
        for y in 0..TILE_SIZE.min(h - c.1 * TILE_SIZE) {
            for x in (0..TILE_SIZE.min(w - c.0 * TILE_SIZE)).step_by(64) {
                let xx = c.0 * TILE_SIZE + x;
                let yy = c.1 * TILE_SIZE + y;
                let n = 64.min(w - xx);
                let center = row_word(bits, w, xx, yy)[1];
                self.selected_pixels += u64::from((center & valid_bits(n)).count_ones());
                let planes = if self.antialias {
                    blur_planes(bits, w, h, xx, yy)
                } else {
                    [center, 0, 0, 0, 0]
                };
                if matches!(self.mode, SelectionMode::New) && planes.iter().all(|v| *v == 0) {
                    continue;
                }
                for b in (0..n).step_by(8) {
                    let word = if self.antialias {
                        gray_word(planes, b)
                    } else {
                        SPREAD[((center >> b) & 255) as usize] * 255
                    };
                    let i = (y * TILE_SIZE + x + b) as usize;
                    let count = (n - b).min(8) as usize;
                    pixels[i..i + count].copy_from_slice(&word.to_le_bytes()[..count]);
                }
            }
        }
        if let Some(rect) = self.canvas {
            for y in 0..TILE_SIZE.min(h - c.1 * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(w - c.0 * TILE_SIZE) {
                    if !rect.contains(c.0 * TILE_SIZE + x, c.1 * TILE_SIZE + y) {
                        pixels[(y * TILE_SIZE + x) as usize] = 0;
                    }
                }
            }
        }
        if !matches!(self.mode, SelectionMode::New) {
            let default = self.before.as_ref().map_or(0, |s| s.default.0);
            for y in 0..TILE_SIZE.min(h - c.1 * TILE_SIZE) {
                let start = (y * TILE_SIZE) as usize;
                let length = TILE_SIZE.min(w - c.0 * TILE_SIZE) as usize;
                for (i, value) in pixels[start..start + length].iter_mut().enumerate() {
                    *value = self.mode.combine(
                        old.as_ref().map_or(default, |t| t.pixels[start + i].0),
                        *value,
                    );
                }
            }
        }
        if old
            .as_ref()
            .is_some_and(|t| t.pixels.iter().map(|p| p.0).eq(pixels.iter().copied()))
            && self
                .before
                .as_ref()
                .is_some_and(|s| s.default == self.output.default)
        {
            self.output
                .insert_handle(c, self.before.as_ref().unwrap().tiles()[&c].clone())?;
        } else {
            self.output
                .insert(c, pixels.into_iter().map(Gray8).collect());
        }
        self.next += 1;
        if self.next == self.tiles() {
            self.stage = 3;
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn opaque_matches(
    bits: &mut [u64],
    row: usize,
    pixels: &[Rgba8],
    reference: [u16; 4],
    tolerance: u16,
) {
    if 255u16.abs_diff(reference[3]) > tolerance {
        return;
    }
    for (x, p) in pixels.iter().enumerate() {
        if u16::from(p.0[0]).abs_diff(reference[0]) <= tolerance
            && u16::from(p.0[1]).abs_diff(reference[1]) <= tolerance
            && u16::from(p.0[2]).abs_diff(reference[2]) <= tolerance
        {
            set(bits, row + x);
        }
    }
}
#[cfg(target_arch = "wasm32")]
fn opaque_matches(
    bits: &mut [u64],
    row: usize,
    pixels: &[Rgba8],
    reference: [u16; 4],
    tolerance: u16,
) {
    use std::arch::wasm32::*;
    if 255u16.abs_diff(reference[3]) > tolerance {
        return;
    }
    let lower = reference.map(|v| v.saturating_sub(tolerance) as u8);
    let upper = reference.map(|v| (v + tolerance).min(255) as u8);
    let low = i32x4_splat(i32::from_le_bytes(lower));
    let high = i32x4_splat(i32::from_le_bytes(upper));
    let mut x = 0;
    while x + 4 <= pixels.len() {
        // Rgba8 is repr(C) over [u8;4]. Four live samples supply all 16 bytes;
        // v128_load permits an unaligned address. SIMD128 is a required WASM feature.
        let value = unsafe { v128_load(pixels.as_ptr().add(x).cast::<v128>()) };
        let mask = i8x16_bitmask(v128_and(u8x16_ge(value, low), u8x16_le(value, high)));
        let group = (mask & (mask >> 1) & (mask >> 2) & (mask >> 3)) & 0x1111;
        let packed = (group & 1) | ((group >> 3) & 2) | ((group >> 6) & 4) | ((group >> 9) & 8);
        let i = row + x;
        let shift = i % 64;
        bits[i / 64] |= u64::from(packed) << shift;
        if shift > 60 {
            bits[i / 64 + 1] |= u64::from(packed) >> (64 - shift);
        }
        x += 4;
    }
    for (offset, p) in pixels[x..].iter().enumerate() {
        if u16::from(p.0[0]).abs_diff(reference[0]) <= tolerance
            && u16::from(p.0[1]).abs_diff(reference[1]) <= tolerance
            && u16::from(p.0[2]).abs_diff(reference[2]) <= tolerance
        {
            set(bits, row + x + offset);
        }
    }
}

fn set_span(bits: &mut [u64], start: usize, end: usize) {
    if start >= end {
        return;
    }
    let first = start / 64;
    let last = (end - 1) / 64;
    let a = u64::MAX << (start % 64);
    let b = u64::MAX >> (63 - (end - 1) % 64);
    if first == last {
        bits[first] |= a & b;
    } else {
        bits[first] |= a;
        bits[first + 1..last].fill(u64::MAX);
        bits[last] |= b;
    }
}
fn valid_bits(n: u32) -> u64 {
    if n == 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}
/// Three horizontally shifted bit vectors with replicated document boundaries.
fn row_word(bits: &[u64], width: u32, x: u32, y: u32) -> [u64; 3] {
    let i = y as usize * width as usize + x as usize;
    let shift = i % 64;
    let mut mid = bits[i / 64] >> shift;
    if shift > 0 {
        mid |= bits.get(i / 64 + 1).copied().unwrap_or(0) << (64 - shift);
    }
    let last = bit(bits, y as usize * width as usize + width as usize - 1);
    let n = 64.min(width - x);
    let valid = valid_bits(n);
    mid = (mid & valid) | if last { !valid } else { 0 };
    let previous = if x == 0 {
        mid & 1
    } else {
        u64::from(bit(bits, i - 1))
    };
    let next = if x + 64 < width {
        u64::from(bit(bits, i + 64))
    } else {
        u64::from(last)
    };
    [(mid << 1) | previous, mid, (mid >> 1) | (next << 63)]
}
fn add_plane(planes: &mut [u64; 5], value: u64, weight: usize) {
    let mut carry = value;
    for plane in &mut planes[weight..] {
        let next = *plane & carry;
        *plane ^= carry;
        carry = next;
    }
}
fn blur_planes(bits: &[u64], w: u32, h: u32, x: u32, y: u32) -> [u64; 5] {
    let top = row_word(bits, w, x, y.saturating_sub(1));
    let mid = row_word(bits, w, x, y);
    let bottom = row_word(bits, w, x, (y + 1).min(h - 1));
    let mut planes = [0; 5];
    for value in [top[0], top[2], bottom[0], bottom[2]] {
        add_plane(&mut planes, value, 0);
    }
    for value in [top[1], bottom[1], mid[0], mid[2]] {
        add_plane(&mut planes, value, 1);
    }
    add_plane(&mut planes, mid[1], 2);
    planes
}
const fn spread() -> [u64; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut b = 0;
        while b < 8 {
            table[i] |= (((i >> b) & 1) as u64) << (b * 8);
            b += 1;
        }
        i += 1;
    }
    table
}
const SPREAD: [u64; 256] = spread();
fn gray_word(p: [u64; 5], shift: u32) -> u64 {
    let expand = |v: u64| SPREAD[((v >> shift) & 255) as usize];
    let base =
        (expand(p[0]) << 4) | (expand(p[1]) << 5) | (expand(p[2]) << 6) | (expand(p[3]) << 7);
    // round(255*sum/16): 16*sum for 0..8, 16*sum-1 for 9..16.
    (base - expand(p[3] & (p[0] | p[1] | p[2]))) | (expand(p[4]) * 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bucket_domain_is_a_connectivity_boundary_even_at_full_tolerance() {
        let source = Arc::new(TileSurface::with_default(7, 1, Rgba8([10, 20, 30, 255])));
        let mut domain = TileSurface::new(7, 1);
        domain.write(1, 0, Gray8(255));
        domain.write(5, 0, Gray8(128));
        let domain = Arc::new(domain);
        for contiguous in [true, false] {
            let mut job = WandJob::in_domain(
                source.clone(),
                None,
                WandOptions {
                    seed: [1, 0],
                    tolerance: 255,
                    contiguous,
                    antialias: false,
                    ..WandOptions::default()
                },
                Some(domain.clone()),
                32 * 1024 * 1024,
            )
            .unwrap();
            while !job.step().unwrap() {}
            assert_eq!(job.selected_pixels, if contiguous { 1 } else { 2 });
            assert_eq!(job.output.sample(1, 0).0, 255);
            assert_eq!(job.output.sample(5, 0).0, if contiguous { 0 } else { 255 });
        }
    }
    #[test]
    fn packed_antialias_matches_scalar_gaussian_at_every_word_tile_and_canvas_boundary() {
        for w in [1u32, 2, 31, 63, 64, 65, 257, 513] {
            let h = 17u32;
            let mut bits = vec![0; (w as usize * h as usize).div_ceil(64)];
            let mut rng = 17u32;
            for i in 0..w as usize * h as usize {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                if rng >> 31 != 0 {
                    set(&mut bits, i);
                }
            }
            for y in 0..h {
                for start in (0..w).step_by(64) {
                    let planes = blur_planes(&bits, w, h, start, y);
                    for x in 0..64.min(w - start) {
                        let actual = gray_word(planes, (x / 8) * 8).to_le_bytes()[(x % 8) as usize];
                        let mut sum = 0u32;
                        for dy in -1i32..=1 {
                            for dx in -1i32..=1 {
                                let xx =
                                    (start as i32 + x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                                let yy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                                sum += u32::from(bit(&bits, yy * w as usize + xx))
                                    * if dx == 0 { 2 } else { 1 }
                                    * if dy == 0 { 2 } else { 1 };
                            }
                        }
                        assert_eq!(actual, ((sum * 255 + 8) / 16) as u8, "{w} {start} {x} {y}");
                    }
                }
            }
        }
    }
    #[test]
    fn full_tolerance_is_sparse_and_preserves_coverage_algebra_without_source_hydration() {
        let source = Arc::new(TileSurface::new(6000, 4000));
        let mut j = WandJob::new(
            source,
            None,
            WandOptions {
                tolerance: 255,
                ..WandOptions::default()
            },
            1 << 20,
        )
        .unwrap();
        assert!(j.step().unwrap());
        assert_eq!(j.selected_pixels, 24_000_000);
        assert_eq!(j.output.bytes(), 0);
        assert_eq!(j.output.sample(3000, 2000), Gray8(255));
    }
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
