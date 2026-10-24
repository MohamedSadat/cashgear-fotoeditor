//! Finite, separable sRGB convolution. Halo samples always come from the
//! immutable source, including outside the selection; canvas edges clamp.
use fotoeditor_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

pub const FILTER_SCRATCH: u64 = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Filter {
    Gaussian {
        radius: u32,
    },
    BoxBlur {
        radius: u32,
    },
    UnsharpMask {
        radius: u32,
        amount: f32,
        threshold: u8,
    },
    Sharpen {
        amount: f32,
    },
    Median {
        radius: u32,
    },
}
impl Filter {
    pub fn radius(&self) -> u32 {
        match *self {
            Self::Gaussian { radius }
            | Self::BoxBlur { radius }
            | Self::UnsharpMask { radius, .. }
            | Self::Median { radius } => radius,
            Self::Sharpen { .. } => 1,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let limit = if matches!(self, Self::Median { .. }) {
            5
        } else {
            64
        };
        if self.radius() > limit {
            return Err(format!("Filter radius must be 0–{limit}"));
        }
        if let Self::UnsharpMask { amount, .. } | Self::Sharpen { amount } = *self {
            if !amount.is_finite() || !(0.0..=5.0).contains(&amount) {
                return Err("Sharpen amount must be finite and 0–5".into());
            }
        }
        Ok(())
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Gaussian { .. } => "Gaussian blur",
            Self::BoxBlur { .. } => "Box blur",
            Self::UnsharpMask { .. } => "Unsharp mask",
            Self::Sharpen { .. } => "Sharpen",
            Self::Median { .. } => "Median",
        }
    }
    pub fn weights(&self) -> Vec<f32> {
        let r = self.radius() as usize;
        if r == 0 {
            return vec![1.0];
        }
        if matches!(self, Self::BoxBlur { .. } | Self::Sharpen { .. }) {
            return vec![1.0 / (2 * r + 1) as f32; 2 * r + 1];
        }
        let sigma = r as f64 / 3.0;
        let mut w: Vec<_> = (0..=2 * r)
            .map(|i| {
                let x = i as f64 - r as f64;
                (-x * x / (2.0 * sigma * sigma)).exp()
            })
            .collect();
        let sum: f64 = w.iter().sum();
        w.iter_mut().for_each(|v| *v /= sum);
        w.into_iter().map(|v| v as f32).collect()
    }
}
fn premul(p: Rgba8) -> [f32; 4] {
    let a = f32::from(p.0[3]);
    [
        f32::from(p.0[0]) * a / 255.0,
        f32::from(p.0[1]) * a / 255.0,
        f32::from(p.0[2]) * a / 255.0,
        a,
    ]
}
fn quantize(v: f32) -> u8 {
    // Rust's float-to-byte conversion saturates, including the endpoints.
    (v + 0.5) as u8
}
pub fn filtered_sample(
    original: Rgba8,
    filtered: [f32; 4],
    coverage: u8,
    alpha_lock: bool,
) -> Rgba8 {
    if coverage == 0 {
        return original;
    }
    let mut result = filtered;
    if alpha_lock {
        let a = result[3];
        for c in &mut result[..3] {
            *c = if a > 0.0 {
                *c * f32::from(original.0[3]) / a
            } else {
                0.0
            };
        }
        result[3] = f32::from(original.0[3]);
    }
    let before = premul(original);
    let f = f32::from(coverage) / 255.0;
    let p: [f32; 4] = std::array::from_fn(|i| before[i] + (result[i] - before[i]) * f);
    let alpha = quantize(p[3]);
    if alpha == 0 {
        return Rgba8([original.0[0], original.0[1], original.0[2], 0]);
    }
    let scale = 255.0 / p[3];
    Rgba8([
        quantize(p[0] * scale),
        quantize(p[1] * scale),
        quantize(p[2] * scale),
        alpha,
    ])
}

pub struct FilterJob {
    pub before: Document,
    pub candidate: Document,
    pub full: Document,
    pub layer: LayerId,
    pub filter: Filter,
    coordinates: Vec<TileCoord>,
    next: usize,
    available: u64,
    allocated: u64,
    weights: Vec<f32>,
}
impl FilterJob {
    pub fn new(
        document: &Document,
        layer: LayerId,
        filter: Filter,
        available: u64,
    ) -> Result<Self, String> {
        filter.validate()?;
        check_layer_edit(&document.layers, layer, false)?;
        let surface = document.layer(layer)?.raster_samples()?;
        if available < FILTER_SCRATCH {
            return Err("Not enough shared CPU budget for filter".into());
        }
        let mut coords = BTreeSet::new();
        let halo = u32::from(filter.radius() > 0);
        for &(x, y) in surface.tiles().keys() {
            for cy in
                y.saturating_sub(halo)..=(y + halo).min(document.height.div_ceil(TILE_SIZE) - 1)
            {
                for cx in
                    x.saturating_sub(halo)..=(x + halo).min(document.width.div_ceil(TILE_SIZE) - 1)
                {
                    coords.insert((cx, cy));
                }
            }
        }
        Ok(Self {
            before: document.clone(),
            candidate: document.clone(),
            full: document.clone(),
            layer,
            weights: filter.weights(),
            filter,
            coordinates: coords.into_iter().collect(),
            next: 0,
            available,
            allocated: 0,
        })
    }
    pub fn done(&self) -> bool {
        self.next == self.coordinates.len()
    }
    /// Conservative cumulative new-generation capacity. Source generations are
    /// counted separately; paging may reduce actual residency below this bound.
    pub fn allocated_bytes(&self) -> u64 {
        self.allocated
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        self.before.visit_tiles(visitor);
        self.candidate.visit_tiles(visitor);
        self.full.visit_tiles(visitor);
    }
    fn bounds(&self, c: TileCoord) -> (u32, u32, u32, u32) {
        let r = self.filter.radius();
        (
            (c.0 * TILE_SIZE).saturating_sub(r),
            (c.1 * TILE_SIZE).saturating_sub(r),
            ((c.0 + 1) * TILE_SIZE + r).min(self.before.width) - 1,
            ((c.1 + 1) * TILE_SIZE + r).min(self.before.height) - 1,
        )
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        let Some(&c) = self.coordinates.get(self.next) else {
            return;
        };
        let (x0, y0, x1, y1) = self.bounds(c);
        let s = self
            .before
            .layer(self.layer)
            .unwrap()
            .raster_samples()
            .unwrap();
        for y in y0 / TILE_SIZE..=y1 / TILE_SIZE {
            for x in x0 / TILE_SIZE..=x1 / TILE_SIZE {
                if let Some(t) = s.tiles().get(&(x, y)) {
                    visitor(TileRef::Rgba(t));
                }
            }
        }
        if let Some(t) = self
            .before
            .selection
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
        {
            visitor(TileRef::Gray(t));
        }
    }
    #[inline(never)]
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if FILTER_SCRATCH + self.allocated + (TILE_PIXELS * 8) as u64 > self.available {
            return Err("Filter exceeds shared CPU budget; document unchanged".into());
        }
        let c = self.coordinates[self.next];
        let surface = self.before.layer(self.layer)?.raster_samples()?;
        let mut pinned = HashMap::new();
        let (x0, y0, x1, y1) = self.bounds(c);
        for y in y0 / TILE_SIZE..=y1 / TILE_SIZE {
            for x in x0 / TILE_SIZE..=x1 / TILE_SIZE {
                if let Some(t) = surface.tiles().get(&(x, y)) {
                    pinned.insert((x, y), t.pin()?);
                }
            }
        }
        let r = self.filter.radius() as usize;
        let width = TILE_SIZE.min(self.before.width - c.0 * TILE_SIZE) as usize;
        let height = TILE_SIZE.min(self.before.height - c.1 * TILE_SIZE) as usize;
        let stride = width + 2 * r;
        let rows = height + 2 * r;
        let mut input = vec![[0.0; 4]; stride * rows];
        for y in 0..rows {
            let sy = (c.1 as i64 * 256 + y as i64 - r as i64)
                .clamp(0, i64::from(self.before.height) - 1) as u32;
            let mut x = 0;
            while x < stride {
                let raw = c.0 as i64 * 256 + x as i64 - r as i64;
                let sx = raw.clamp(0, i64::from(self.before.width) - 1) as u32;
                let repeated = raw < 0 || raw >= i64::from(self.before.width);
                let count = if raw < 0 {
                    (-raw) as usize
                } else if repeated {
                    stride - x
                } else {
                    (256 - sx % 256).min(self.before.width - sx) as usize
                }
                .min(stride - x);
                let row = &mut input[y * stride + x..y * stride + x + count];
                if let Some(tile) = pinned.get(&(sx / 256, sy / 256)) {
                    let start = ((sy % 256) * 256 + sx % 256) as usize;
                    if repeated {
                        row.fill(premul(tile.pixels[start]));
                    } else {
                        for (out, &p) in row.iter_mut().zip(&tile.pixels[start..start + count]) {
                            *out = premul(p);
                        }
                    }
                } else {
                    row.fill(premul(surface.default));
                }
                x += count;
            }
        }
        let opaque = input.iter().all(|p| p[3] == 255.0);
        let output = convolve(&input, stride, width, height, &self.filter, &self.weights);
        let selection = self.before.selection.as_ref();
        let mask = selection
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let mut pixels = pinned
            .get(&c)
            .map_or_else(|| vec![surface.default; TILE_PIXELS], |t| t.pixels.clone());
        let alpha_lock = self.before.layer(self.layer)?.properties.locks.alpha;
        let mut full = selection.map(|_| pixels.clone());
        let mut changed = false;
        if opaque && selection.is_none() {
            changed = apply_opaque_full(&mut pixels, &output, width, height);
        } else {
            for y in 0..height {
                for x in 0..width {
                    let i = y * 256 + x;
                    let coverage = mask
                        .as_ref()
                        .map_or_else(|| selection.map_or(255, |s| s.default.0), |s| s.pixels[i].0);
                    let result =
                        filtered_sample(pixels[i], output[y * width + x], coverage, alpha_lock);
                    if coverage > 0 {
                        if let Some(full) = &mut full {
                            full[i] =
                                filtered_sample(pixels[i], output[y * width + x], 255, alpha_lock);
                        }
                    }
                    changed |= result != pixels[i];
                    pixels[i] = result;
                }
            }
        }
        let shared = full.as_ref().is_none_or(|f| f == &pixels);
        if changed {
            self.candidate
                .layer_mut(self.layer)?
                .raster_samples_mut()?
                .insert(c, pixels);
            self.allocated += (TILE_PIXELS * 4) as u64;
        }
        if shared {
            let tile = self
                .candidate
                .layer(self.layer)?
                .raster_samples()?
                .tiles()
                .get(&c)
                .cloned();
            let surface = self.full.layer_mut(self.layer)?.raster_samples_mut()?;
            if let Some(tile) = tile {
                surface.insert_handle(c, tile)?;
            } else {
                surface.insert(c, vec![surface.default; TILE_PIXELS]);
            }
        } else if let Some(full) = full {
            self.full
                .layer_mut(self.layer)?
                .raster_samples_mut()?
                .insert(c, full);
            self.allocated += (TILE_PIXELS * 4) as u64;
        }
        self.next += 1;
        Ok(self.done())
    }
    pub fn finish(&self) -> Result<TileCommand, String> {
        if !self.done() {
            return Err("Filter is incomplete".into());
        }
        Ok(
            TileCommand::for_layer(&self.before, &self.candidate, self.layer)?
                .named(self.filter.label()),
        )
    }
}

#[inline(never)]
fn apply_opaque_full(
    pixels: &mut [Rgba8],
    output: &[[f32; 4]],
    width: usize,
    height: usize,
) -> bool {
    let mut changed = false;
    for y in 0..height {
        for (old, p) in pixels[y * 256..y * 256 + width]
            .iter_mut()
            .zip(&output[y * width..(y + 1) * width])
        {
            let after = Rgba8([quantize(p[0]), quantize(p[1]), quantize(p[2]), 255]);
            changed |= *old != after;
            *old = after;
        }
    }
    changed
}

#[inline(never)]
fn convolve(
    input: &[[f32; 4]],
    stride: usize,
    width: usize,
    height: usize,
    filter: &Filter,
    weights: &[f32],
) -> Vec<[f32; 4]> {
    let r = filter.radius() as usize;
    let mut out = vec![[0.0; 4]; width * height];
    if matches!(filter, Filter::Median { .. }) {
        let count = (2 * r + 1).pow(2);
        let mut values = vec![0.0f32; count];
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    for dy in 0..=2 * r {
                        for dx in 0..=2 * r {
                            values[dy * (2 * r + 1) + dx] =
                                input[(y + dy) * stride + x + dx][channel];
                        }
                    }
                    let (_, median, _) =
                        values.select_nth_unstable_by(count / 2, |a, b| a.total_cmp(b));
                    out[y * width + x][channel] = *median;
                }
            }
        }
    } else {
        // Process narrow columns so all vertical tap rows fit in the per-core
        // cache. This is the same finite kernel, with no radius approximation.
        let opaque = input.iter().all(|p| p[3] == 255.0);
        let packed: Vec<f32> = if opaque {
            input.iter().flat_map(|p| [p[0], p[1], p[2]]).collect()
        } else {
            Vec::new()
        };
        let channels = if opaque { 3 } else { 4 };
        let samples = if opaque {
            packed.as_slice()
        } else {
            input.as_flattened()
        };
        let result = separable(samples, channels, stride, width, height, weights);
        for (p, values) in out.iter_mut().zip(result.chunks_exact(channels)) {
            *p = [
                values[0],
                values[1],
                values[2],
                if opaque { 255.0 } else { values[3] },
            ];
        }
    }

    let (amount, threshold) = match *filter {
        Filter::UnsharpMask {
            amount, threshold, ..
        } => (amount, f32::from(threshold)),
        Filter::Sharpen { amount } => (amount, 0.0),
        _ => return out,
    };
    for y in 0..height {
        for x in 0..width {
            let original = input[(y + r) * stride + x + r];
            let blurred = out[y * width + x];
            let mut p = original;
            if original[3] > 0.0 {
                for i in 0..3 {
                    let b = if blurred[3] > 0.0 {
                        blurred[i] * 255.0 / blurred[3]
                    } else {
                        0.0
                    };
                    let s = original[i] * 255.0 / original[3];
                    let delta = s - b;
                    if delta.abs() >= threshold {
                        p[i] = (s + amount * delta).clamp(0.0, 255.0) * original[3] / 255.0;
                    }
                }
            }
            out[y * width + x] = p;
        }
    }
    out
}
#[inline(never)]
fn separable(
    input: &[f32],
    channels: usize,
    stride: usize,
    width: usize,
    height: usize,
    weights: &[f32],
) -> Vec<f32> {
    let r = weights.len() / 2;
    let full_pitch = width * channels;
    let stride = stride * channels;
    let mut result = vec![0.0; full_pitch * height];
    for bx in (0..width).step_by(32) {
        let pitch = (width - bx).min(32) * channels;
        let mut horizontal = vec![0.0; pitch * (height + 2 * r)];
        for y in 0..height + 2 * r {
            let start = y * stride + bx * channels;
            convolution_line(
                &mut horizontal[y * pitch..(y + 1) * pitch],
                &input[start..start + pitch + 2 * r * channels],
                channels,
                weights,
            );
        }
        for y in 0..height {
            let start = y * full_pitch + bx * channels;
            convolution_line(
                &mut result[start..start + pitch],
                &horizontal[y * pitch..(y + 2 * r + 1) * pitch],
                pitch,
                weights,
            );
        }
    }
    result
}
#[inline(never)]
fn convolution_line(out: &mut [f32], input: &[f32], step: usize, weights: &[f32]) {
    let r = weights.len() / 2;
    assert!(input.len() >= out.len() + 2 * r * step);
    #[cfg(any(target_arch = "x86_64", target_arch = "wasm32"))]
    {
        #[cfg(target_arch = "wasm32")]
        use std::arch::wasm32::f32x4_relaxed_madd as madd;
        #[cfg(target_arch = "wasm32")]
        use std::arch::wasm32::{
            f32x4_add as add, f32x4_mul as mul, f32x4_splat as splat, v128_load as load,
            v128_store as store,
        };
        #[cfg(target_arch = "x86_64")]
        use std::arch::x86_64::{
            _mm_add_ps as add, _mm_loadu_ps as load, _mm_mul_ps as mul, _mm_set1_ps as splat,
            _mm_storeu_ps as store,
        };
        // SSE is baseline on x86_64; the WASM workspace enables simd128.
        // Checked slice bounds cover all vector loads, including halos.
        // Four independent accumulators avoid one long dependency chain.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            let madd = |a, b, c| add(c, mul(a, b));
            let mut x = 0;
            while x + 16 <= out.len() {
                let base = input.as_ptr().add(x + r * step);
                let w = splat(weights[r]);
                let mut a = mul(load(base.cast()), w);
                let mut b = mul(load(base.add(4).cast()), w);
                let mut c = mul(load(base.add(8).cast()), w);
                let mut d = mul(load(base.add(12).cast()), w);
                for k in 1..=r {
                    let w = splat(weights[r + k]);
                    let left = base.sub(k * step);
                    let right = base.add(k * step);
                    a = madd(add(load(left.cast()), load(right.cast())), w, a);
                    b = madd(
                        add(load(left.add(4).cast()), load(right.add(4).cast())),
                        w,
                        b,
                    );
                    c = madd(
                        add(load(left.add(8).cast()), load(right.add(8).cast())),
                        w,
                        c,
                    );
                    d = madd(
                        add(load(left.add(12).cast()), load(right.add(12).cast())),
                        w,
                        d,
                    );
                }
                store(out.as_mut_ptr().add(x).cast(), a);
                store(out.as_mut_ptr().add(x + 4).cast(), b);
                store(out.as_mut_ptr().add(x + 8).cast(), c);
                store(out.as_mut_ptr().add(x + 12).cast(), d);
                x += 16;
            }
            while x < out.len() {
                let center = x + r * step;
                let mut v = input[center] * weights[r];
                for k in 1..=r {
                    v += (input[center - k * step] + input[center + k * step]) * weights[r + k];
                }
                out[x] = v;
                x += 1;
            }
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "wasm32")))]
    for (x, out) in out.iter_mut().enumerate() {
        let center = x + r * step;
        *out = input[center] * weights[r];
        for k in 1..=r {
            *out += (input[center - k * step] + input[center + k * step]) * weights[r + k];
        }
    }
}

/// Fade interpolates once from immutable original samples, with the existing
/// sRGB blend functions. Normal/100% and 0% return byte-exact endpoints.
pub fn fade_sample(before: Rgba8, after: Rgba8, opacity: f64, mode: BlendMode) -> Rgba8 {
    if opacity == 0.0 || (before == after && mode == BlendMode::Normal) {
        return before;
    }
    if opacity == 1.0 && mode == BlendMode::Normal {
        return after;
    }
    let b: [u8; 3] = before.0[..3].try_into().unwrap();
    let s: [u8; 3] = after.0[..3].try_into().unwrap();
    let rgb = blend_rgb(
        mode,
        b.map(|v| f64::from(v) / 255.0),
        s.map(|v| f64::from(v) / 255.0),
    );
    let mut result = Composite::from_straight(after);
    for (i, v) in rgb.into_iter().enumerate() {
        // Hidden backdrop RGB must not color newly spread alpha. Blend functions
        // contribute only where the pre-filter backdrop has coverage.
        let base_alpha = f64::from(before.0[3]) / 255.0;
        let source = f64::from(after.0[i]) / 255.0;
        result.0[i] = (source + (v - source) * base_alpha) * result.0[3];
    }
    let result = Composite::from_straight(before).interpolate(result, opacity);
    let mut out = result.straight();
    if out.0[3] == 0 {
        out.0[..3].copy_from_slice(&before.0[..3]);
    }
    out
}

pub struct FadeJob {
    pub before: Document,
    pub filtered: Document,
    pub full: Document,
    pub current: Document,
    pub candidate: Document,
    pub layer: LayerId,
    coordinates: Vec<TileCoord>,
    next: usize,
    opacity: f64,
    mode: BlendMode,
    available: u64,
    allocated: u64,
}
impl FadeJob {
    pub fn allocated_bytes(&self) -> u64 {
        self.allocated
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        current: &Document,
        before: &Document,
        filtered: &Document,
        full: &Document,
        layer: LayerId,
        opacity: f64,
        mode: BlendMode,
        available: u64,
    ) -> Result<Self, String> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err("Fade opacity must be 0–1".into());
        }
        check_layer_edit(&current.layers, layer, false)?;
        let coordinates: BTreeSet<_> = before
            .layer(layer)?
            .raster_samples()?
            .tiles()
            .keys()
            .chain(filtered.layer(layer)?.raster_samples()?.tiles().keys())
            .chain(full.layer(layer)?.raster_samples()?.tiles().keys())
            .copied()
            .collect();
        if available < FILTER_SCRATCH {
            return Err("Not enough shared CPU budget for Fade".into());
        }
        Ok(Self {
            before: before.clone(),
            filtered: filtered.clone(),
            full: full.clone(),
            current: current.clone(),
            candidate: current.clone(),
            layer,
            coordinates: coordinates.into_iter().collect(),
            next: 0,
            opacity,
            mode,
            available,
            allocated: 0,
        })
    }
    pub fn done(&self) -> bool {
        self.next == self.coordinates.len()
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for d in [
            &self.before,
            &self.filtered,
            &self.full,
            &self.current,
            &self.candidate,
        ] {
            d.visit_tiles(visitor);
        }
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(c) = self.coordinates.get(self.next) {
            for d in [&self.before, &self.filtered, &self.full, &self.current] {
                if let Some(t) = d
                    .layer(self.layer)
                    .unwrap()
                    .raster_samples()
                    .unwrap()
                    .tiles()
                    .get(c)
                {
                    visitor(TileRef::Rgba(t));
                }
            }
            if let Some(t) = self
                .before
                .selection
                .as_ref()
                .and_then(|s| s.tiles().get(c))
            {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if FILTER_SCRATCH + self.allocated + (TILE_PIXELS * 4) as u64 > self.available {
            return Err("Fade exceeds shared CPU budget; document unchanged".into());
        }
        let c = self.coordinates[self.next];
        let surfaces = [&self.before, &self.filtered, &self.full, &self.current]
            .map(|d| d.layer(self.layer).unwrap().raster_samples().unwrap());
        let tiles = surfaces
            .map(|s| s.tiles().get(&c).map(|t| t.pin()).transpose())
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let selection = self.before.selection.as_ref();
        let mask = selection
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let mut pixels = tiles[3]
            .as_ref()
            .map_or_else(|| vec![Rgba8::default(); TILE_PIXELS], |t| t.pixels.clone());
        let mut changed = false;
        for y in 0..TILE_SIZE.min(self.current.height - c.1 * TILE_SIZE) {
            for x in 0..TILE_SIZE.min(self.current.width - c.0 * TILE_SIZE) {
                let i = (y * TILE_SIZE + x) as usize;
                let sample = |j: usize| {
                    tiles[j]
                        .as_ref()
                        .map_or(surfaces[j].default, |t| t.pixels[i])
                };
                let coverage = mask
                    .as_ref()
                    .map_or_else(|| selection.map_or(255, |s| s.default.0), |m| m.pixels[i].0);
                let p = if self.mode == BlendMode::Normal {
                    fade_sample(sample(0), sample(1), self.opacity, self.mode)
                } else {
                    fade_sample(
                        sample(0),
                        sample(2),
                        self.opacity * f64::from(coverage) / 255.0,
                        self.mode,
                    )
                };
                changed |= pixels[i] != p;
                pixels[i] = p;
            }
        }
        if changed {
            self.candidate
                .layer_mut(self.layer)?
                .raster_samples_mut()?
                .insert(c, pixels);
            self.allocated += (TILE_PIXELS * 4) as u64;
        }
        self.next += 1;
        Ok(self.done())
    }
    pub fn finish(&self) -> Result<TileCommand, String> {
        if !self.done() {
            return Err("Fade is incomplete".into());
        }
        Ok(
            TileCommand::for_layer(&self.current, &self.candidate, self.layer)?
                .named("Fade last filter"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn run(d: &Document, f: Filter) -> FilterJob {
        let mut j = FilterJob::new(d, d.layers[0].id, f, 128 << 20).unwrap();
        while !j.step().unwrap() {}
        j
    }
    #[test]
    fn independent_direct_convolution_matches_across_four_tile_corners() {
        let mut d = Document::blank(259, 259, 72.0).unwrap();
        for y in 0..259 {
            for x in 0..259 {
                d.raster_mut().write(
                    x,
                    y,
                    Rgba8([
                        (x * 31 + y * 7) as u8,
                        (x + y * 13) as u8,
                        91,
                        [0, 1, 128, 255][(x + y) as usize % 4],
                    ]),
                );
            }
        }
        for f in [
            Filter::Gaussian { radius: 3 },
            Filter::BoxBlur { radius: 2 },
            Filter::UnsharpMask {
                radius: 3,
                amount: 1.2,
                threshold: 7,
            },
            Filter::Sharpen { amount: 1.0 },
            Filter::Median { radius: 1 },
        ] {
            let j = run(&d, f.clone());
            let r = f.radius() as i32;
            for y in [0, 1, 254, 255, 256, 257, 258] {
                for x in [0, 1, 254, 255, 256, 257, 258] {
                    let original = d.raster().sample(x, y);
                    let mut p = [0.0f64; 4];
                    let mut values = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
                    let sigma = f64::from(r) / 3.0;
                    let w: Vec<_> = (-r..=r)
                        .map(|k| {
                            if matches!(f, Filter::BoxBlur { .. } | Filter::Sharpen { .. }) {
                                1.0
                            } else {
                                (-f64::from(k * k) / (2.0 * sigma * sigma)).exp()
                            }
                        })
                        .collect();
                    let sum: f64 = w.iter().sum();
                    for dy in -r..=r {
                        for dx in -r..=r {
                            let s = d.raster().sample(
                                (x as i32 + dx).clamp(0, 258) as u32,
                                (y as i32 + dy).clamp(0, 258) as u32,
                            );
                            let s = [
                                f64::from(s.0[0]) * f64::from(s.0[3]) / 255.0,
                                f64::from(s.0[1]) * f64::from(s.0[3]) / 255.0,
                                f64::from(s.0[2]) * f64::from(s.0[3]) / 255.0,
                                f64::from(s.0[3]),
                            ];
                            for c in 0..4 {
                                p[c] += s[c] * w[(dx + r) as usize] * w[(dy + r) as usize]
                                    / (sum * sum);
                                values[c].push(s[c]);
                            }
                        }
                    }
                    if matches!(f, Filter::Median { .. }) {
                        for c in 0..4 {
                            values[c].sort_by(f64::total_cmp);
                            p[c] = values[c][values[c].len() / 2];
                        }
                    }
                    let (amount, threshold) = match f {
                        Filter::UnsharpMask {
                            amount, threshold, ..
                        } => (f64::from(amount), f64::from(threshold)),
                        Filter::Sharpen { amount } => (f64::from(amount), 0.0),
                        _ => (0.0, 0.0),
                    };
                    if amount > 0.0 {
                        let b = p;
                        p = premul(original).map(f64::from);
                        if original.0[3] > 0 {
                            for c in 0..3 {
                                let delta = f64::from(original.0[c])
                                    - if b[3] > 0.0 { b[c] * 255.0 / b[3] } else { 0.0 };
                                if delta.abs() >= threshold {
                                    p[c] = (f64::from(original.0[c]) + amount * delta)
                                        .clamp(0.0, 255.0)
                                        * p[3]
                                        / 255.0;
                                }
                            }
                        }
                    }
                    let expected = filtered_sample(original, p.map(|v| v as f32), 255, false);
                    let actual = j.candidate.raster().sample(x, y);
                    for c in 0..4 {
                        assert!(
                            actual.0[c].abs_diff(expected.0[c]) <= 1,
                            "{f:?} at {x},{y}: {actual:?} != {expected:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn sparse_spread_selection_alpha_lock_and_exact_history() {
        let mut d = Document::blank(513, 3, 72.0).unwrap();
        d.raster_mut().write(255, 1, Rgba8([255, 0, 0, 255]));
        d.raster_mut().write(512, 2, Rgba8([17, 39, 87, 0]));
        let original = d.clone();
        let j = run(&d, Filter::BoxBlur { radius: 1 });
        assert_eq!(j.candidate.raster().sample(256, 1), Rgba8([255, 0, 0, 28]));
        assert_eq!(j.candidate.raster().sample(512, 2), Rgba8([17, 39, 87, 0]));
        let mut history = History::default();
        history
            .execute(&mut d, Box::new(j.finish().unwrap()), 0)
            .unwrap();
        assert!(history.undo(&mut d));
        assert_eq!(d.raster().sample(255, 1), original.raster().sample(255, 1));
        assert!(history.redo(&mut d));
        d = original;
        let mut s = TileSurface::new(513, 3);
        s.write(256, 1, Gray8(128));
        d.selection = Some(Arc::new(s));
        let j = run(&d, Filter::BoxBlur { radius: 1 });
        assert_eq!(j.candidate.raster().sample(255, 1), Rgba8([255, 0, 0, 255]));
        assert_eq!(j.candidate.raster().sample(256, 1).0[3], 14);
        d.selection = None;
        d.layers[0].properties.locks.alpha = true;
        let j = run(&d, Filter::BoxBlur { radius: 1 });
        assert_eq!(j.candidate.raster().sample(256, 1), Rgba8::default());
        assert_eq!(j.candidate.raster().sample(255, 1).0[3], 255);
    }
    #[test]
    fn identities_invalid_refused_cancelled_and_cold_halos_are_atomic() {
        let mut d = Document::blank(257, 1, 72.0).unwrap();
        d.raster_mut().write(255, 0, Rgba8([17, 39, 87, 128]));
        let id = d.layers[0].id;
        for f in [
            Filter::Gaussian { radius: 0 },
            Filter::BoxBlur { radius: 0 },
            Filter::Median { radius: 0 },
            Filter::Sharpen { amount: 0.0 },
        ] {
            assert!(run(&d, f).finish().unwrap().is_empty());
        }
        for f in [
            Filter::Gaussian { radius: 65 },
            Filter::Median { radius: 6 },
            Filter::Sharpen { amount: f32::NAN },
        ] {
            assert!(FilterJob::new(&d, id, f, 128 << 20).is_err());
        }
        let f = Filter::Gaussian { radius: 3 };
        let mut j = FilterJob::new(&d, id, f.clone(), FILTER_SCRATCH).unwrap();
        assert!(j.step().is_err());
        assert!(j.finish().is_err());
        let tile = d.raster().tiles()[&(0, 0)].clone();
        let pixels = tile.pin().unwrap().pixels.clone();
        let backing = TileBacking {
            store: 1,
            offset: 0,
            length: 262144,
            checksum: 0,
        };
        let mut pager = TilePager::default();
        pager.acknowledge_backing(&tile, backing).unwrap();
        pager.evict(&tile).unwrap();
        let mut j = FilterJob::new(&d, id, f.clone(), 128 << 20).unwrap();
        let mut ids = vec![];
        j.needed(&mut |t| ids.push(t.id()));
        assert!(ids.contains(&tile.id()));
        assert!(j.step().is_err());
        pager
            .hydrate(&tile, backing, pixels, Residency::default())
            .unwrap();
        j.step().unwrap();
        drop(j);
        assert_eq!(d.raster().sample(255, 0), Rgba8([17, 39, 87, 128]));
        d.layers[0].properties.locks.pixels = true;
        assert!(FilterJob::new(&d, id, f, 128 << 20).is_err());
    }
    #[test]
    fn fade_endpoints_all_modes_and_repeated_values_do_not_compound() {
        let b = Rgba8([51, 102, 153, 128]);
        let a = Rgba8([204, 77, 26, 255]);
        assert_eq!(fade_sample(b, a, 0.0, BlendMode::Normal), b);
        assert_eq!(fade_sample(b, a, 1.0, BlendMode::Normal), a);
        assert_eq!(
            fade_sample(b, a, 0.5, BlendMode::Normal),
            Rgba8([153, 85, 68, 192])
        );
        for mode in BlendMode::ALL {
            assert_eq!(fade_sample(b, a, 0.0, mode), b);
            assert_eq!(
                fade_sample(Rgba8([0, 0, 0, 0]), a, 0.5, mode),
                fade_sample(Rgba8([255, 37, 91, 0]), a, 0.5, mode)
            );
        }
        let mut d = Document::blank(3, 1, 72.0).unwrap();
        d.raster_mut().write(1, 0, b);
        let id = d.layers[0].id;
        let mut result = d.clone();
        result.raster_mut().write(1, 0, a);
        let mut history = History::default();
        let command = TileCommand::for_layer(&d, &result, id).unwrap();
        history.execute(&mut d, Box::new(command), 0).unwrap();
        let source = {
            let mut s = d.clone();
            s.raster_mut().write(1, 0, b);
            s
        };
        for opacity in [0.3, 0.7, 0.3] {
            let mut j = FadeJob::new(
                &d,
                &source,
                &result,
                &result,
                id,
                opacity,
                BlendMode::Multiply,
                128 << 20,
            )
            .unwrap();
            while !j.step().unwrap() {}
            let command = j.finish().unwrap();
            history.execute(&mut d, Box::new(command), 0).unwrap();
            assert_eq!(
                d.raster().sample(1, 0),
                fade_sample(b, a, opacity, BlendMode::Multiply)
            );
        }
        while history.undo(&mut d) {}
        assert_eq!(d.raster().sample(1, 0), b);
    }
    #[test]
    fn packed_opaque_and_rgba_kernels_agree_for_odd_widths_and_large_radius() {
        for r in [0usize, 1, 20, 64] {
            let w = 37;
            let h = 5;
            let stride = w + 2 * r;
            let input: Vec<_> = (0..stride * (h + 2 * r))
                .map(|i| {
                    [
                        (i * 7 % 256) as f32,
                        (i * 13 % 256) as f32,
                        (i * 31 % 256) as f32,
                        255.0,
                    ]
                })
                .collect();
            let filter = Filter::Gaussian { radius: r as u32 };
            let weights = filter.weights();
            let result = convolve(&input, stride, w, h, &filter, &weights);
            let reference = separable(input.as_flattened(), 4, stride, w, h, &weights);
            for (p, q) in result.iter().zip(reference.as_chunks::<4>().0) {
                for c in 0..4 {
                    assert!((p[c] - q[c]).abs() < 0.001);
                }
            }
        }
    }
    #[test]
    fn fade_blend_clips_fractional_selection_once_and_includes_unchanged_selected_samples() {
        let mut d = Document::blank(513, 3, 72.0).unwrap();
        for y in 0..3 {
            for x in 0..513 {
                d.raster_mut()
                    .write(x, y, Rgba8([if x == 255 { 128 } else { 64 }; 4]));
            }
        }
        // The fixture is opaque; RGB uses a one-column detail at the tile seam.
        for y in 0..3 {
            for x in 0..513 {
                let mut p = d.raster().sample(x, y);
                p.0[3] = 255;
                d.raster_mut().write(x, y, p);
            }
        }
        let mut s = TileSurface::new(513, 3);
        for y in 0..3 {
            s.write(255, y, Gray8(128));
            s.write(256, y, Gray8(255));
            s.write(257, y, Gray8(128));
        }
        d.selection = Some(Arc::new(s));
        let j = run(&d, Filter::BoxBlur { radius: 1 });
        let mut fade = FadeJob::new(
            &j.candidate,
            &d,
            &j.candidate,
            &j.full,
            d.layers[0].id,
            1.0,
            BlendMode::Multiply,
            128 << 20,
        )
        .unwrap();
        while !fade.step().unwrap() {}
        for (x, value) in [(254, 64), (255, 85), (256, 21), (257, 40), (258, 64)] {
            assert_eq!(
                fade.candidate.raster().sample(x, 1),
                Rgba8([value, value, value, 255])
            );
        }
    }
    #[test]
    fn allocation_bound_counts_shared_endpoints_once_and_covers_partial_selection_candidates() {
        for selected in [false, true] {
            let mut d = Document::blank(513, 3, 72.0).unwrap();
            d.raster_mut().write(255, 1, Rgba8([255, 31, 97, 255]));
            if selected {
                let mut s = TileSurface::new(513, 3);
                s.write(256, 1, Gray8(128));
                d.selection = Some(Arc::new(s));
            }
            let mut original = std::collections::HashSet::new();
            d.visit_tiles(&mut |t| {
                original.insert(t.id());
            });
            let mut j = FilterJob::new(
                &d,
                d.layers[0].id,
                Filter::Gaussian { radius: 20 },
                64 << 20,
            )
            .unwrap();
            while !j.done() {
                j.step().unwrap();
                let mut seen = original.clone();
                let mut actual = 0;
                j.visit_tiles(&mut |t| {
                    if seen.insert(t.id()) {
                        actual += t.bytes();
                    }
                });
                assert!(actual <= j.allocated_bytes());
                if !selected {
                    assert!(j.allocated_bytes() <= 3 * (TILE_PIXELS * 4) as u64);
                }
            }
        }
    }
}
