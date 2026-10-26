//! Straight-sample resampling with premultiplied filtering and bounded paging.
//! Float values below are temporary precision, never authoritative document depth.
use crate::*;
use fast_image_resize::{
    images::TypedImage, pixels::F32x4, FilterType, ResizeAlg, ResizeOptions, Resizer,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const RESAMPLE_SCRATCH: u64 = 16 * 1024 * 1024;
pub const RESAMPLE_FAST_INPUT: u64 = 2 * 1024 * 1024;

pub trait ResamplePixel: Pixel {
    fn components(self) -> [f64; 4];
    fn reconstruct(value: [f64; 4], nearest: Self) -> Self;
    fn reference(tile: &TileHandle<Self>) -> TileRef<'_>;
}
impl ResamplePixel for Rgba8 {
    fn components(self) -> [f64; 4] {
        let a = self.0[3] as f64 / 255.0;
        [
            self.0[0] as f64 / 255.0 * a,
            self.0[1] as f64 / 255.0 * a,
            self.0[2] as f64 / 255.0 * a,
            a,
        ]
    }
    fn reconstruct(value: [f64; 4], nearest: Self) -> Self {
        let alpha = (value[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        if alpha == 0 {
            return Rgba8([nearest.0[0], nearest.0[1], nearest.0[2], 0]);
        }
        Rgba8([
            byte(value[0] / value[3]),
            byte(value[1] / value[3]),
            byte(value[2] / value[3]),
            alpha,
        ])
    }
    fn reference(tile: &TileHandle<Self>) -> TileRef<'_> {
        TileRef::Rgba(tile)
    }
}
impl ResamplePixel for Gray8 {
    fn components(self) -> [f64; 4] {
        [self.0 as f64 / 255.0, 0.0, 0.0, 0.0]
    }
    fn reconstruct(value: [f64; 4], _nearest: Self) -> Self {
        Gray8(byte(value[0]))
    }
    fn reference(tile: &TileHandle<Self>) -> TileRef<'_> {
        TileRef::Gray(tile)
    }
}
fn byte(v: f64) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}
pub fn interpolation_support(mode: Interpolation) -> f64 {
    match mode {
        Interpolation::Nearest => 0.5,
        Interpolation::Bilinear => 1.0,
        Interpolation::Bicubic => 2.0,
        Interpolation::Lanczos3 => 3.0,
    }
}
pub fn interpolation_weight(mode: Interpolation, x: f64) -> f64 {
    let x = x.abs();
    match mode {
        Interpolation::Nearest => f64::from(x < 0.5),
        Interpolation::Bilinear => (1.0 - x).max(0.0),
        Interpolation::Bicubic => {
            if x < 1.0 {
                1.5 * x * x * x - 2.5 * x * x + 1.0
            } else if x < 2.0 {
                -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
            } else {
                0.0
            }
        }
        Interpolation::Lanczos3 => {
            if x < 1e-12 {
                1.0
            } else if x < 3.0 {
                let p = std::f64::consts::PI * x;
                p.sin() / p * (p / 3.0).sin() / (p / 3.0)
            } else {
                0.0
            }
        }
    }
}
struct AxisWeights {
    values: Vec<(u32, f64)>,
    sum: f64,
}
fn weights(
    mode: Interpolation,
    edge: f64,
    scale: f64,
    range: (u32, u32),
    normalize_edges: bool,
) -> AxisWeights {
    let scale = scale.max(1.0);
    let center = edge - 0.5;
    let support = interpolation_support(mode) * scale;
    let lo = (center - support).ceil();
    let hi = (center + support).floor();
    let start = lo.clamp(range.0 as f64, range.1 as f64) as u32;
    let end = (hi + 1.0).clamp(range.0 as f64, range.1 as f64) as u32;
    let mut values: Vec<_> = (start..end)
        .map(|i| {
            (
                i,
                interpolation_weight(mode, (i as f64 - center) / scale) / scale,
            )
        })
        .collect();
    let inside: f64 = values.iter().map(|v| v.1).sum();
    // Finite document-sized reductions use the exact lattice normalization.
    // More extreme projective footprints approach the integral; their finite
    // source coverage is sub-byte, without allocating or iterating huge kernels.
    let total = if normalize_edges {
        inside
    } else if hi - lo <= 131072.0 {
        (lo as i64..=hi as i64)
            .map(|i| interpolation_weight(mode, (i as f64 - center) / scale) / scale)
            .sum()
    } else {
        1.0
    };
    if total.abs() > 1e-20 {
        for (_, v) in &mut values {
            *v /= total;
        }
    }
    let sum = values.iter().map(|v| v.1).sum();
    AxisWeights { values, sum }
}

struct PinnedRegion<P: Pixel> {
    tiles: BTreeMap<TileCoord, Arc<Tile<P>>>,
    default: P,
    outside: P,
    width: u32,
    height: u32,
}
impl<P: Pixel> PinnedRegion<P> {
    fn new(source: &TileSurface<P>, rect: CanvasRect, outside: P) -> Result<Self, String> {
        let mut tiles = BTreeMap::new();
        for y in rect.y / TILE_SIZE..(rect.y + rect.height).div_ceil(TILE_SIZE) {
            for x in rect.x / TILE_SIZE..(rect.x + rect.width).div_ceil(TILE_SIZE) {
                if let Some(t) = source.tiles().get(&(x, y)) {
                    tiles.insert((x, y), t.pin()?);
                }
            }
        }
        Ok(Self {
            tiles,
            default: source.default,
            outside,
            width: source.width,
            height: source.height,
        })
    }
    fn sample(&self, x: i64, y: i64) -> P {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return self.outside;
        }
        let (x, y) = (x as u32, y as u32);
        self.tiles
            .get(&(x / TILE_SIZE, y / TILE_SIZE))
            .map_or(self.default, |t| {
                t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
            })
    }
}

struct SamplePlan<P: ResamplePixel> {
    x: AxisWeights,
    y: AxisWeights,
    coords: Vec<TileCoord>,
    next: usize,
    value: [f64; 4],
    nearest: P,
    nearest_at: Option<(u32, u32)>,
}
impl<P: ResamplePixel> SamplePlan<P> {
    fn new(
        source: &TileSurface<P>,
        inverse: TransformMatrix,
        mode: Interpolation,
        point: [f64; 2],
        normalize: bool,
        outside: P,
        domain: Option<TransformRect>,
    ) -> Result<Self, String> {
        // Destination pixels outside the projected source can lie on the inverse
        // horizon. They contribute the field default, rather than aborting a job.
        let edge = inverse.map(point).unwrap_or([f64::INFINITY; 2]);
        let d = domain.unwrap_or(TransformRect {
            x: 0.0,
            y: 0.0,
            width: source.width as f64,
            height: source.height as f64,
        });
        let xr = (d.x as u32, (d.x + d.width) as u32);
        let yr = (d.y as u32, (d.y + d.height) as u32);
        let m = inverse.values;
        let w = m[6] * point[0] + m[7] * point[1] + m[8];
        let sx = ((m[0] - edge[0] * m[6]) / w)
            .hypot((m[1] - edge[0] * m[7]) / w)
            .max(1.0);
        let sy = ((m[3] - edge[1] * m[6]) / w)
            .hypot((m[4] - edge[1] * m[7]) / w)
            .max(1.0);
        let exact = matches!(mode, Interpolation::Nearest)
            || (sx <= 1.0 + 1e-10
                && sy <= 1.0 + 1e-10
                && (edge[0] - 0.5 - (edge[0] - 0.5).round()).abs() < 1e-8
                && (edge[1] - 0.5 - (edge[1] - 0.5).round()).abs() < 1e-8);
        let (x, y) = if edge.iter().any(|v| !v.is_finite()) {
            (
                AxisWeights {
                    values: vec![],
                    sum: 0.0,
                },
                AxisWeights {
                    values: vec![],
                    sum: 0.0,
                },
            )
        } else if exact {
            let axis = |v: f64, range: (u32, u32)| {
                let n = v.floor();
                let valid = n >= range.0 as f64 && n < range.1 as f64;
                AxisWeights {
                    values: if valid { vec![(n as u32, 1.0)] } else { vec![] },
                    sum: if valid { 1.0 } else { 0.0 },
                }
            };
            (axis(edge[0], xr), axis(edge[1], yr))
        } else {
            (
                weights(mode, edge[0], sx, xr, normalize),
                weights(mode, edge[1], sy, yr, normalize),
            )
        };
        let mut coords = Vec::new();
        if let (Some(x0), Some(x1), Some(y0), Some(y1)) = (
            x.values.first(),
            x.values.last(),
            y.values.first(),
            y.values.last(),
        ) {
            for ty in y0.0 / TILE_SIZE..=y1.0 / TILE_SIZE {
                for tx in x0.0 / TILE_SIZE..=x1.0 / TILE_SIZE {
                    coords.push((tx, ty));
                }
            }
        }
        let nearest_at = (edge[0] >= d.x
            && edge[1] >= d.y
            && edge[0] < d.x + d.width
            && edge[1] < d.y + d.height)
            .then(|| (edge[0].floor() as u32, edge[1].floor() as u32));
        if let Some((x, y)) = nearest_at {
            let coord = (x / TILE_SIZE, y / TILE_SIZE);
            if !coords.contains(&coord) {
                coords.push(coord);
            }
        }
        let nearest = outside;
        let value = outside.components().map(|v| v * (1.0 - x.sum * y.sum));
        Ok(Self {
            x,
            y,
            coords,
            next: 0,
            value,
            nearest,
            nearest_at,
        })
    }
}

#[derive(Clone, Copy)]
struct FastBlock {
    source: CanvasRect,
    crop: TransformRect,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// One immutable source, one sparse candidate. Generic steps process at most 327,679
/// source taps or 256 output pixels; extreme reductions stream one source tile
/// per dependency. Ordinary axis resizing uses padded premultiplied F32 FIR blocks.
pub struct SurfaceResampleJob<P: ResamplePixel> {
    pub source: Arc<TileSurface<P>>,
    pub output: TileSurface<P>,
    pub identity: bool,
    inverse: TransformMatrix,
    mode: Interpolation,
    normalize: bool,
    available: u64,
    next: u32,
    col: u32,
    row: u32,
    pixels: Vec<P>,
    sample: Option<SamplePlan<P>>,
    fast: Option<FastBlock>,
    axis: bool,
    block_size: (u32, u32),
    permutation: bool,
    permutation_region: Option<CanvasRect>,
    resizer: Resizer,
    outside: P,
    domain: Option<TransformRect>,
}
impl<P: ResamplePixel> SurfaceResampleJob<P> {
    pub fn new(
        source: Arc<TileSurface<P>>,
        width: u32,
        height: u32,
        matrix: TransformMatrix,
        mode: Interpolation,
        normalize_edges: bool,
        available: u64,
    ) -> Result<Self, String> {
        let outside = source.default;
        Self::with_outside(
            source,
            width,
            height,
            matrix,
            mode,
            normalize_edges,
            available,
            outside,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn with_outside(
        source: Arc<TileSurface<P>>,
        width: u32,
        height: u32,
        matrix: TransformMatrix,
        mode: Interpolation,
        normalize_edges: bool,
        available: u64,
        outside: P,
    ) -> Result<Self, String> {
        Self::with_domain(
            source,
            width,
            height,
            matrix,
            mode,
            normalize_edges,
            available,
            outside,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_domain(
        source: Arc<TileSurface<P>>,
        width: u32,
        height: u32,
        matrix: TransformMatrix,
        mode: Interpolation,
        normalize_edges: bool,
        available: u64,
        outside: P,
        domain: Option<TransformRect>,
    ) -> Result<Self, String> {
        validate_size(width, height, 72.0)?;
        validate_size(source.width, source.height, 72.0)?;
        let bounds = TransformRect {
            x: 0.0,
            y: 0.0,
            width: source.width as f64,
            height: source.height as f64,
        };
        if let Some(d) = domain {
            d.validate()?;
            if d.x < 0.0
                || d.y < 0.0
                || d.x + d.width > source.width as f64
                || d.y + d.height > source.height as f64
            {
                return Err("Resample source bounds exceed the retained surface".into());
            }
        }
        matrix.validate_for(domain.unwrap_or(bounds))?;
        let domain = domain.filter(|_| outside == source.default);
        let identity = width == source.width
            && height == source.height
            && bounds.corners().iter().all(|p| {
                matrix
                    .map(*p)
                    .is_ok_and(|q| (q[0] - p[0]).abs().max((q[1] - p[1]).abs()) < 1e-8)
            });
        if !identity && available < RESAMPLE_SCRATCH {
            return Err("Not enough shared CPU budget for resampling".into());
        }
        let inverse = matrix.inverse()?;
        let m = matrix.values.map(|v| v / matrix.values[8]);
        let mut axis = normalize_edges
            && m[1] == 0.0
            && m[3] == 0.0
            && m[6] == 0.0
            && m[7] == 0.0
            && m[2] == 0.0
            && m[5] == 0.0
            && (m[0] - width as f64 / source.width as f64).abs() < 1e-12
            && (m[4] - height as f64 / source.height as f64).abs() < 1e-12;
        let inv = inverse.values.map(|v| v / inverse.values[8]);
        let permutation = inv[6] == 0.0
            && inv[7] == 0.0
            && [inv[0], inv[1], inv[3], inv[4]]
                .iter()
                .all(|v| [-1.0, 0.0, 1.0].contains(v))
            && inv[0].abs() + inv[1].abs() == 1.0
            && inv[3].abs() + inv[4].abs() == 1.0
            && inv[0] * inv[3] + inv[1] * inv[4] == 0.0
            && inv[2] == inv[2].round()
            && inv[5] == inv[5].round();
        let mut block_size = (128_u32, 128_u32);
        if axis {
            let sx = source.width as f64 / width as f64;
            let sy = source.height as f64 / height as f64;
            let radius = interpolation_support(mode);
            loop {
                let iw = (block_size.0 as f64 * sx + 2.0 * radius * sx.max(1.0) + 4.0)
                    .ceil()
                    .min(source.width as f64) as u64;
                let ih = (block_size.1 as f64 * sy + 2.0 * radius * sy.max(1.0) + 4.0)
                    .ceil()
                    .min(source.height as f64) as u64;
                if iw * ih * 16 <= RESAMPLE_FAST_INPUT
                    && block_size.0 as u64 * ih * 16 <= RESAMPLE_FAST_INPUT
                {
                    break;
                }
                if block_size == (1, 1) {
                    axis = false;
                    break;
                }
                if block_size.0 >= block_size.1 && block_size.0 > 1 {
                    block_size.0 = block_size.0.div_ceil(2);
                } else {
                    block_size.1 = block_size.1.div_ceil(2);
                }
            }
        }
        let output = if identity {
            (*source).clone()
        } else {
            TileSurface::with_default(width, height, outside)
        };
        let mut job = Self {
            source,
            output,
            identity,
            inverse,
            mode,
            normalize: normalize_edges,
            available,
            next: 0,
            col: 0,
            row: 0,
            pixels: if identity {
                vec![]
            } else {
                vec![outside; TILE_PIXELS]
            },
            sample: None,
            fast: None,
            axis,
            block_size,
            permutation,
            permutation_region: None,
            resizer: Resizer::new(),
            outside,
            domain,
        };
        if identity {
            job.next = job.tile_count();
        } else {
            job.prepare()?;
        }
        Ok(job)
    }
    fn tile_count(&self) -> u32 {
        self.output.width.div_ceil(TILE_SIZE) * self.output.height.div_ceil(TILE_SIZE)
    }
    fn coord(&self) -> TileCoord {
        (
            self.next % self.output.width.div_ceil(TILE_SIZE),
            self.next / self.output.width.div_ceil(TILE_SIZE),
        )
    }
    fn tile_size(&self) -> (u32, u32) {
        let c = self.coord();
        (
            TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE),
            TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE),
        )
    }
    pub fn done(&self) -> bool {
        self.next == self.tile_count()
    }
    pub fn temporary_bytes(&self) -> u64 {
        if self.identity {
            0
        } else {
            RESAMPLE_SCRATCH + self.output.bytes()
        }
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for t in self
            .source
            .tiles()
            .values()
            .chain(self.output.tiles().values())
        {
            visitor(P::reference(t));
        }
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(region) = self.permutation_region.or(self.fast.map(|b| b.source)) {
            if region.width == 0 || region.height == 0 {
                return;
            }
            for y in region.y / TILE_SIZE..(region.y + region.height).div_ceil(TILE_SIZE) {
                for x in region.x / TILE_SIZE..(region.x + region.width).div_ceil(TILE_SIZE) {
                    if let Some(t) = self.source.tiles().get(&(x, y)) {
                        visitor(P::reference(t));
                    }
                }
            }
        } else if let Some(c) = self.sample.as_ref().and_then(|s| s.coords.get(s.next)) {
            if let Some(t) = self.source.tiles().get(c) {
                visitor(P::reference(t));
            }
        }
    }
    fn prepare(&mut self) -> Result<(), String> {
        if self.done() {
            return Ok(());
        }
        let c = self.coord();
        let (tw, th) = self.tile_size();
        if self.permutation {
            let bounds = self.inverse.bounds(TransformRect {
                x: (c.0 * TILE_SIZE) as f64,
                y: (c.1 * TILE_SIZE) as f64,
                width: tw as f64,
                height: th as f64,
            })?;
            let d = self.domain.unwrap_or(TransformRect {
                x: 0.0,
                y: 0.0,
                width: self.source.width as f64,
                height: self.source.height as f64,
            });
            let left = bounds.x.clamp(d.x, d.x + d.width) as u32;
            let top = bounds.y.clamp(d.y, d.y + d.height) as u32;
            let right = (bounds.x + bounds.width).clamp(d.x, d.x + d.width) as u32;
            let bottom = (bounds.y + bounds.height).clamp(d.y, d.y + d.height) as u32;
            self.permutation_region = Some(CanvasRect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            });
            return Ok(());
        }
        if self.axis {
            let width = (tw - self.col).min(self.block_size.0);
            let height = (th - self.row).min(self.block_size.1);
            let dx = c.0 * TILE_SIZE + self.col;
            let dy = c.1 * TILE_SIZE + self.row;
            let sx = self.source.width as f64 / self.output.width as f64;
            let sy = self.source.height as f64 / self.output.height as f64;
            let radius = interpolation_support(self.mode);
            let left = (dx as f64 * sx - radius * sx.max(1.0) - 1.0)
                .floor()
                .max(0.0) as u32;
            let top = (dy as f64 * sy - radius * sy.max(1.0) - 1.0)
                .floor()
                .max(0.0) as u32;
            let right = (((dx + width) as f64 * sx + radius * sx.max(1.0) + 1.0).ceil() as u32)
                .min(self.source.width);
            let bottom = (((dy + height) as f64 * sy + radius * sy.max(1.0) + 1.0).ceil() as u32)
                .min(self.source.height);
            if u64::from(right - left) * u64::from(bottom - top) * 16 <= RESAMPLE_FAST_INPUT {
                self.fast = Some(FastBlock {
                    source: CanvasRect {
                        x: left,
                        y: top,
                        width: right - left,
                        height: bottom - top,
                    },
                    crop: TransformRect {
                        x: dx as f64 * sx - left as f64,
                        y: dy as f64 * sy - top as f64,
                        width: width as f64 * sx,
                        height: height as f64 * sy,
                    },
                    x: self.col,
                    y: self.row,
                    width,
                    height,
                });
                return Ok(());
            }
            return Err("Resample block exceeds its reserved scratch space".into());
        }
        let point = [
            (c.0 * TILE_SIZE + self.col) as f64 + 0.5,
            (c.1 * TILE_SIZE + self.row) as f64 + 0.5,
        ];
        self.sample = Some(SamplePlan::new(
            &self.source,
            self.inverse,
            self.mode,
            point,
            self.normalize,
            self.outside,
            self.domain,
        )?);
        Ok(())
    }
    fn advance(&mut self, width: u32, height: u32) -> Result<(), String> {
        let (tw, th) = self.tile_size();
        self.col += width;
        if self.col == tw {
            self.col = 0;
            self.row += height;
        }
        if self.row == th {
            if self.temporary_bytes() + (TILE_PIXELS * size_of::<P>()) as u64 > self.available {
                return Err(
                    "Resample candidate exceeds shared CPU budget; document unchanged".into(),
                );
            }
            self.output.insert(
                self.coord(),
                std::mem::replace(&mut self.pixels, vec![self.outside; TILE_PIXELS]),
            );
            self.output.compact_tile(self.coord())?;
            self.next += 1;
            self.row = 0;
        }
        self.sample = None;
        self.fast = None;
        self.prepare()
    }
    fn fast_step(&mut self, b: FastBlock) -> Result<(), String> {
        let source = PinnedRegion::new(&self.source, b.source, self.outside)?;
        let mut input = Vec::with_capacity((b.source.width * b.source.height) as usize);
        for y in b.source.y..b.source.y + b.source.height {
            for x in b.source.x..b.source.x + b.source.width {
                input.push(F32x4::new(
                    source
                        .sample(x as i64, y as i64)
                        .components()
                        .map(|v| v as f32),
                ));
            }
        }
        let image = TypedImage::from_pixels(b.source.width, b.source.height, input)
            .map_err(|e| e.to_string())?;
        let mut output = TypedImage::<F32x4>::new(b.width, b.height);
        let algorithm = match self.mode {
            Interpolation::Nearest => ResizeAlg::Nearest,
            Interpolation::Bilinear => ResizeAlg::Convolution(FilterType::Bilinear),
            Interpolation::Bicubic => ResizeAlg::Convolution(FilterType::CatmullRom),
            Interpolation::Lanczos3 => ResizeAlg::Convolution(FilterType::Lanczos3),
        };
        let crop = b.crop;
        let options = ResizeOptions::new()
            .use_alpha(false)
            .resize_alg(algorithm)
            .crop(
                crop.x,
                crop.y,
                crop.width.min(b.source.width as f64 - crop.x),
                crop.height.min(b.source.height as f64 - crop.y),
            );
        self.resizer
            .resize_typed(&image, &mut output, Some(&options))
            .map_err(|e| e.to_string())?;
        let c = self.coord();
        for y in 0..b.height {
            for x in 0..b.width {
                let point = self.inverse.map([
                    (c.0 * TILE_SIZE + b.x + x) as f64 + 0.5,
                    (c.1 * TILE_SIZE + b.y + y) as f64 + 0.5,
                ])?;
                let nearest = source.sample(point[0].floor() as i64, point[1].floor() as i64);
                self.pixels[((b.y + y) * TILE_SIZE + b.x + x) as usize] = P::reconstruct(
                    output.pixels()[(y * b.width + x) as usize].0.map(f64::from),
                    nearest,
                );
            }
        }
        self.advance(b.width, b.height)
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if let Some(region) = self.permutation_region {
            if region.width == 0 || region.height == 0 {
                self.next += 1;
                self.permutation_region = None;
                self.prepare()?;
                return Ok(self.done());
            }
            let source = PinnedRegion::new(&self.source, region, self.outside)?;
            let c = self.coord();
            let (tw, th) = self.tile_size();
            for y in 0..th {
                for x in 0..tw {
                    let p = self.inverse.map([
                        (c.0 * TILE_SIZE + x) as f64 + 0.5,
                        (c.1 * TILE_SIZE + y) as f64 + 0.5,
                    ])?;
                    self.pixels[(y * TILE_SIZE + x) as usize] =
                        source.sample(p[0].floor() as i64, p[1].floor() as i64);
                }
            }
            self.advance(tw, th)?;
            return Ok(self.done());
        }
        if let Some(b) = self.fast {
            self.fast_step(b)?;
            return Ok(self.done());
        }
        let mut work = 0;
        let mut pixels = 0;
        while !self.done() && self.fast.is_none() && work < 262144 && pixels < 256 {
            let sample = self.sample.as_mut().ok_or("Missing resample plan")?;
            if let Some(c) = sample.coords.get(sample.next).copied() {
                let pin = self.source.tiles().get(&c).map(TileHandle::pin).transpose();
                let pin = match pin {
                    Ok(p) => p,
                    Err(_) if work > 0 => return Ok(false),
                    Err(e) => return Err(e),
                };
                if let Some((x, y)) = sample
                    .nearest_at
                    .filter(|(x, y)| (*x / TILE_SIZE, *y / TILE_SIZE) == c)
                {
                    sample.nearest = pin.as_ref().map_or(self.source.default, |t| {
                        t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
                    });
                }
                for &(y, wy) in sample.y.values.iter().filter(|p| p.0 / TILE_SIZE == c.1) {
                    for &(x, wx) in sample.x.values.iter().filter(|p| p.0 / TILE_SIZE == c.0) {
                        let p = pin.as_ref().map_or(self.source.default, |t| {
                            t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
                        });
                        let values = p.components();
                        for (out, value) in sample.value.iter_mut().zip(values) {
                            *out += value * wx * wy;
                        }
                        work += 1;
                    }
                }
                sample.next += 1;
            }
            if sample.next == sample.coords.len() {
                self.pixels[(self.row * TILE_SIZE + self.col) as usize] =
                    P::reconstruct(sample.value, sample.nearest);
                self.advance(1, 1)?;
                pixels += 1;
            }
        }
        Ok(self.done())
    }
    pub fn finish(self) -> Result<TileSurface<P>, String> {
        if !self.done() {
            return Err("Resample is incomplete".into());
        }
        Ok(self.output)
    }
}
