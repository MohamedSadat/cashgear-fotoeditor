//! Browser-independent straight-alpha procedural painting and stroke commands.
use fotoeditor_core::{Document, Rgba8, TileCommand, TileCoord, TILE_PIXELS, TILE_SIZE};
use kurbo::{Line, ParamCurve, Point};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
const QUEUE_LIMIT: usize = 2048;
const TILE_BYTES: u64 = (TILE_PIXELS * 4) as u64;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BrushSettings {
    pub size: f64,
    pub hardness: f64,
    pub opacity: f64,
    pub flow: f64,
    pub spacing: f64,
    pub roundness: f64,
    pub angle: f64,
    pub color: [u8; 4],
    pub eraser: bool,
    pub pencil: bool,
    pub stabilizer: Stabilizer,
    pub strength: f64,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    pub tilt_size: bool,
    pub tilt_opacity: bool,
    pub size_pressure: [f64; 5],
    pub opacity_pressure: [f64; 5],
    pub size_tilt: [f64; 5],
    pub opacity_tilt: [f64; 5],
}
impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            size: 30.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.1,
            roundness: 1.0,
            angle: 0.0,
            color: [0, 0, 0, 255],
            eraser: false,
            pencil: false,
            stabilizer: Stabilizer::Off,
            strength: 0.35,
            pressure_size: true,
            pressure_opacity: false,
            tilt_size: false,
            tilt_opacity: false,
            size_pressure: [0.1, 0.25, 0.5, 0.75, 1.0],
            opacity_pressure: [0.0, 0.25, 0.5, 0.75, 1.0],
            size_tilt: [1.0, 0.8, 0.6, 0.4, 0.2],
            opacity_tilt: [1.0, 0.8, 0.6, 0.4, 0.2],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Stabilizer {
    #[default]
    Off,
    Pulled,
    Weighted,
}
impl BrushSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.size.is_finite()
            || !(1.0..=2048.0).contains(&self.size)
            || !self.angle.is_finite()
            || self.angle.abs() > 360.0
            || !self.spacing.is_finite()
            || !(0.01..=1.0).contains(&self.spacing)
            || !self.roundness.is_finite()
            || !(0.05..=1.0).contains(&self.roundness)
            || [self.hardness, self.opacity, self.flow, self.strength]
                .iter()
                .chain(self.size_pressure.iter())
                .chain(self.opacity_pressure.iter())
                .chain(self.size_tilt.iter())
                .chain(self.opacity_tilt.iter())
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Invalid brush settings".into());
        }
        Ok(())
    }
    fn dynamics(&self, p: Sample) -> (f64, f64) {
        let tilt = (p.tilt_x.hypot(p.tilt_y) / 90.0).min(1.0);
        (
            if self.pressure_size {
                curve(self.size_pressure, p.pressure)
            } else {
                1.0
            } * if self.tilt_size {
                curve(self.size_tilt, tilt)
            } else {
                1.0
            },
            if self.pressure_opacity {
                curve(self.opacity_pressure, p.pressure)
            } else {
                1.0
            } * if self.tilt_opacity {
                curve(self.opacity_tilt, tilt)
            } else {
                1.0
            },
        )
    }
}
pub fn curve(points: [f64; 5], input: f64) -> f64 {
    let x = input.clamp(0.0, 1.0) * 4.0;
    let i = (x.floor() as usize).min(3);
    points[i] + (points[i + 1] - points[i]) * (x - i as f64)
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub x: f64,
    pub y: f64,
    pub pressure: f64,
    pub tilt_x: f64,
    pub tilt_y: f64,
    pub time: f64,
}
impl Sample {
    fn valid(self) -> bool {
        [
            self.x,
            self.y,
            self.pressure,
            self.tilt_x,
            self.tilt_y,
            self.time,
        ]
        .iter()
        .all(|v| v.is_finite())
            && self.x.abs() <= 1_000_000.0
            && self.y.abs() <= 1_000_000.0
            && (0.0..=1.0).contains(&self.pressure)
            && self.tilt_x.abs() <= 90.0
            && self.tilt_y.abs() <= 90.0
    }
    fn lerp(self, other: Self, t: f64) -> Self {
        let p = Line::new((self.x, self.y), (other.x, other.y)).eval(t);
        Self {
            x: p.x,
            y: p.y,
            pressure: self.pressure + (other.pressure - self.pressure) * t,
            tilt_x: self.tilt_x + (other.tilt_x - self.tilt_x) * t,
            tilt_y: self.tilt_y + (other.tilt_y - self.tilt_y) * t,
            time: self.time + (other.time - self.time) * t,
        }
    }
}
struct Segment {
    from: Sample,
    to: Sample,
    length: f64,
    next: f64,
    spacing: f64,
}
struct Dab {
    p: Sample,
    radius: f64,
    influence: f64,
    x0: u32,
    x1: u32,
    y1: u32,
    x: u32,
    y: u32,
}
/// Procedural tip prepared once per stroke; bounded even for a 2048px brush.
/// Dabs resample this 16-bit mask, including pressure scaling and subpixel motion.
struct Tip {
    pixels: Vec<u16>,
    side: usize,
    radius: f64,
}
impl Tip {
    fn new(settings: &BrushSettings) -> Self {
        let diameter = if settings.pencil {
            1.0
        } else {
            settings.size.ceil().min(512.0)
        };
        let radius = diameter / 2.0;
        let side = diameter as usize + 2;
        let mut pixels = vec![0; side * side];
        let (sin, cos) = settings.angle.to_radians().sin_cos();
        for y in 0..side {
            for x in 0..side {
                let dx = x as f64 + 0.5 - radius - 1.0;
                let dy = y as f64 + 0.5 - radius - 1.0;
                let rx = (dx * cos + dy * sin) / radius;
                let ry = (-dx * sin + dy * cos)
                    / radius
                    / if settings.pencil {
                        1.0
                    } else {
                        settings.roundness
                    };
                let d2 = rx * rx + ry * ry;
                if d2 > 1.0 {
                    continue;
                }
                let amount = if settings.hardness == 1.0 || settings.pencil {
                    1.0
                } else {
                    ((1.0 - d2.sqrt()) / (1.0 - settings.hardness)).min(1.0)
                };
                pixels[y * side + x] = (amount * settings.flow * 65535.0 + 0.5) as u16;
            }
        }
        Self {
            pixels,
            side,
            radius,
        }
    }
    fn bytes(&self) -> u64 {
        (self.pixels.capacity() * 2) as u64
    }
    #[inline(always)]
    fn sample(&self, x: f64, y: f64) -> u32 {
        if x < 0.0 || y < 0.0 || x >= self.side as f64 - 1.0 || y >= self.side as f64 - 1.0 {
            return 0;
        }
        let ix = x as usize;
        let iy = y as usize;
        let fx = ((x - ix as f64) * 256.0) as u32;
        let fy = ((y - iy as f64) * 256.0) as u32;
        let i = iy * self.side + ix;
        let a = self.pixels[i] as u32 * (256 - fx) + self.pixels[i + 1] as u32 * fx;
        let b = self.pixels[i + self.side] as u32 * (256 - fx)
            + self.pixels[i + self.side + 1] as u32 * fx;
        (a * (256 - fy) + b * fy + 32768) >> 16
    }
}
/// Scratch preview becomes one atomic TileCommand; work and input are bounded.
pub struct StrokeCommand {
    pub preview: Document,
    pub applied_time: f64,
    pub completed_time: f64,
    base: Document,
    settings: BrushSettings,
    tip: Tip,
    // 16-bit fixed-point coverage in u32 buffers; document samples stay RGBA8.
    coverage: HashMap<TileCoord, Vec<u32>>,
    queue: VecDeque<Sample>,
    previous: Option<Sample>,
    segment: Option<Segment>,
    dab: Option<Dab>,
    carry: f64,
    ending: bool,
    available: u64,
    touched: Vec<TileCoord>,
}
impl StrokeCommand {
    pub fn new(base: &Document, settings: BrushSettings, available: u64) -> Result<Self, String> {
        settings.validate()?;
        let tip_side = if settings.pencil {
            3
        } else {
            settings.size.ceil().min(512.0) as u64 + 2
        };
        if available < (QUEUE_LIMIT * size_of::<Sample>()) as u64 + tip_side * tip_side * 2 {
            return Err("Not enough memory for a stroke".into());
        }
        let mut preview = base.clone();
        preview.raster.drain_dirty();
        Ok(Self {
            preview,
            applied_time: 0.0,
            completed_time: 0.0,
            base: base.clone(),
            tip: Tip::new(&settings),
            settings,
            coverage: HashMap::new(),
            queue: VecDeque::with_capacity(QUEUE_LIMIT),
            previous: None,
            segment: None,
            dab: None,
            carry: 0.0,
            ending: false,
            available,
            touched: Vec::new(),
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.coverage.len() as u64 * TILE_BYTES * 2
            + (self.queue.capacity() * size_of::<Sample>()) as u64
            + self.tip.bytes()
    }
    pub fn push(&mut self, sample: Sample) -> Result<(), String> {
        if !sample.valid() {
            return Err("Invalid pointer sample".into());
        }
        if self.queue.len() >= QUEUE_LIMIT {
            return Err("Stroke input exceeded its bounded queue; stroke cancelled".into());
        }
        self.queue.push_back(sample);
        Ok(())
    }
    pub fn end(&mut self) {
        self.ending = true;
    }
    pub fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.segment.is_some()) + usize::from(self.dab.is_some())
    }
    pub fn done(&self) -> bool {
        self.ending && self.pending() == 0
    }
    fn filtered(&self, raw: Sample) -> Sample {
        let Some(previous) = self.previous else {
            return raw;
        };
        let strength = self.settings.strength;
        let t = match self.settings.stabilizer {
            Stabilizer::Off => 1.0,
            Stabilizer::Weighted => {
                if strength == 0.0 {
                    1.0
                } else {
                    1.0 - (-(raw.time - previous.time).clamp(1.0, 100.0) / (strength * 80.0)).exp()
                }
            }
            Stabilizer::Pulled => {
                let d = Point::new(previous.x, previous.y).distance(Point::new(raw.x, raw.y));
                let r = strength * 60.0;
                if d > r {
                    (d - r) / d
                } else {
                    0.0
                }
            }
        };
        Sample {
            time: raw.time,
            pressure: raw.pressure,
            tilt_x: raw.tilt_x,
            tilt_y: raw.tilt_y,
            ..previous.lerp(raw, t)
        }
    }
    fn next_dab(&mut self) -> Option<Sample> {
        loop {
            if let Some(s) = &mut self.segment {
                if s.next <= s.length {
                    // A dab depends on the newly received endpoint sample, even
                    // when its geometric position lies earlier along the path.
                    let p = Sample {
                        time: s.to.time,
                        ..s.from.lerp(s.to, s.next / s.length)
                    };
                    s.next += s.spacing;
                    return Some(p);
                }
                self.carry = s.next - s.length;
                self.previous = Some(s.to);
                self.segment = None;
            }
            let raw = self.queue.pop_front()?;
            let p = self.filtered(raw);
            let Some(from) = self.previous else {
                self.previous = Some(p);
                self.carry = self.spacing(p);
                return Some(p);
            };
            let length = Point::new(from.x, from.y).distance(Point::new(p.x, p.y));
            if length < 1e-9 {
                self.previous = Some(p);
                if p.time > from.time {
                    return Some(p);
                }
                continue;
            }
            let spacing = self.spacing(from).min(self.spacing(p));
            self.segment = Some(Segment {
                from,
                to: p,
                length,
                next: self.carry.min(spacing),
                spacing,
            });
        }
    }
    fn spacing(&self, p: Sample) -> f64 {
        (self.settings.size
            * self.settings.dynamics(p).0
            * self.settings.roundness
            * self.settings.spacing)
            .max(0.5)
    }
    fn begin_dab(&mut self, mut p: Sample) {
        let (size, influence) = self.settings.dynamics(p);
        if size == 0.0
            || influence == 0.0
            || self.settings.opacity == 0.0
            || self.settings.flow == 0.0
        {
            return;
        }
        let radius = if self.settings.pencil {
            0.5
        } else {
            (self.settings.size * size / 2.0).max(0.25)
        };
        if self.settings.pencil {
            p.x = p.x.floor() + 0.5;
            p.y = p.y.floor() + 0.5;
        }
        let x0 = (p.x - radius).floor().clamp(0.0, self.preview.width as f64) as u32;
        let x1 = (p.x + radius).ceil().clamp(0.0, self.preview.width as f64) as u32;
        let y0 = (p.y - radius)
            .floor()
            .clamp(0.0, self.preview.height as f64) as u32;
        let y1 = (p.y + radius).ceil().clamp(0.0, self.preview.height as f64) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        self.dab = Some(Dab {
            p,
            radius,
            influence,
            x0,
            x1,
            y1,
            x: x0,
            y: y0,
        });
    }
    #[inline(never)]
    pub fn step(&mut self, mut pixels: usize) -> Result<(), String> {
        let mut dabs = 0;
        while pixels > 0 && dabs < 512 {
            if self.dab.is_none() {
                let Some(p) = self.next_dab() else {
                    break;
                };
                self.begin_dab(p);
                dabs += 1;
                if self.dab.is_none() {
                    continue;
                }
            }
            let dab = self.dab.as_mut().unwrap();
            let coord = (dab.x / TILE_SIZE, dab.y / TILE_SIZE);
            if !self.coverage.contains_key(&coord) {
                let required = (self.coverage.len() as u64 + 1) * TILE_BYTES * 2
                    + (self.queue.capacity() * size_of::<Sample>()) as u64
                    + self.tip.bytes();
                if required > self.available {
                    return Err("This stroke exceeds the 768 MiB document/history/temporary budget; stroke cancelled".into());
                }
                self.coverage.insert(coord, vec![0; TILE_PIXELS]);
                self.touched.push(coord);
            }
            let mask = self.coverage.get_mut(&coord).unwrap();
            let base = self
                .base
                .raster
                .tiles()
                .get(&coord)
                .map(|tile| tile.pin())
                .transpose()?;
            let end = dab
                .x1
                .min((coord.0 + 1) * TILE_SIZE)
                .min(dab.x + pixels.min(TILE_SIZE as usize) as u32);
            let mut output = self.preview.raster.edit_region(
                coord,
                fotoeditor_core::TileRect {
                    x0: dab.x % TILE_SIZE,
                    y0: dab.y % TILE_SIZE,
                    x1: end - coord.0 * TILE_SIZE,
                    y1: dab.y % TILE_SIZE + 1,
                },
            );
            let changed = stamp_row(
                dab,
                &self.tip,
                &self.settings,
                base.as_deref(),
                mask,
                &mut output,
                end,
            );
            if changed {
                self.applied_time = self.applied_time.max(dab.p.time);
            }
            pixels -= (end - dab.x) as usize;
            dab.x = end;
            if dab.x == dab.x1 {
                dab.x = dab.x0;
                dab.y += 1;
            }
            if dab.y == dab.y1 {
                self.completed_time = self.completed_time.max(dab.p.time);
                self.dab = None;
            }
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<TileCommand, String> {
        if !self.done() {
            return Err("Stroke still has pending work".into());
        }
        for coord in &self.touched {
            self.preview.raster.edit_tile(*coord);
        }
        self.preview.raster.compact_dirty();
        TileCommand::between(&self.base, &self.preview)
    }
}
// Keep the hot loop small and frequently called for browser WASM tiering.
#[inline(never)]
fn stamp_row(
    dab: &Dab,
    tip: &Tip,
    settings: &BrushSettings,
    base: Option<&fotoeditor_core::Tile<Rgba8>>,
    mask: &mut [u32],
    output: &mut [Rgba8],
    end: u32,
) -> bool {
    let scale = tip.radius / dab.radius;
    if scale == 1.0 && !settings.eraser && base.is_some_and(|tile| tile.known_opaque) {
        return stamp_opaque_row(dab, tip, settings, base.unwrap(), mask, output, end);
    }
    let sy = (dab.y as f64 + 0.5 - dab.p.y) * scale + tip.radius + 0.5;
    let influence = (dab.influence * 65535.0 + 0.5) as u32;
    let opacity = (settings.opacity
        * if settings.eraser {
            1.0
        } else {
            settings.color[3] as f64 / 255.0
        }
        * 65535.0
        + 0.5) as u32;
    let mut changed = false;
    for x in dab.x..end {
        let sx = (x as f64 + 0.5 - dab.p.x) * scale + tip.radius + 0.5;
        let value = tip.sample(sx, sy);
        if value == 0 {
            continue;
        }
        let i = ((dab.y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize;
        let amount = divide_unorm16(value * influence + 32767);
        mask[i] += divide_unorm16((65535 - mask[i]) * amount + 32767);
        let before = base.map_or(Rgba8::default(), |t| t.pixels[i]);
        let after = composite_fixed(
            before,
            settings.color,
            divide_unorm16(opacity * mask[i] + 32767),
            settings.eraser,
        );
        changed |= after != output[i];
        output[i] = after;
    }
    changed
}
#[inline(never)]
fn stamp_opaque_row(
    dab: &Dab,
    tip: &Tip,
    settings: &BrushSettings,
    base: &fotoeditor_core::Tile<Rgba8>,
    mask: &mut [u32],
    output: &mut [Rgba8],
    end: u32,
) -> bool {
    let sx = dab.x as f64 - dab.p.x + tip.radius + 1.0;
    let sy = dab.y as f64 - dab.p.y + tip.radius + 1.0;
    let ix = sx as usize;
    let iy = sy as usize;
    let fx = ((sx - ix as f64) * 256.0) as u32;
    let fy = ((sy - iy as f64) * 256.0) as u32;
    let length = (end - dab.x) as usize;
    let top = &tip.pixels[iy * tip.side + ix..][..length + 1];
    let bottom = &tip.pixels[(iy + 1) * tip.side + ix..][..length + 1];
    let start = ((dab.y % TILE_SIZE) * TILE_SIZE + dab.x % TILE_SIZE) as usize;
    let masks = &mut mask[start..][..length];
    let outputs = &mut output[start..][..length];
    let original = &base.pixels[start..][..length];
    let influence = (dab.influence * 65535.0 + 0.5) as u32;
    let opacity = (settings.opacity * settings.color[3] as f64 / 255.0 * 65535.0 + 0.5) as u32;
    let mut changed = false;
    for i in 0..length {
        let a = top[i] as u32 * (256 - fx) + top[i + 1] as u32 * fx;
        let b = bottom[i] as u32 * (256 - fx) + bottom[i + 1] as u32 * fx;
        let value = (a * (256 - fy) + b * fy + 32768) >> 16;
        let amount = divide_unorm16(value * influence + 32767);
        masks[i] += divide_unorm16((65535 - masks[i]) * amount + 32767);
        let source = divide_unorm16(opacity * masks[i] + 32767);
        let keep = 65535 - source;
        let p = original[i].0;
        let after = Rgba8([
            divide_unorm16(settings.color[0] as u32 * source + p[0] as u32 * keep + 32767) as u8,
            divide_unorm16(settings.color[1] as u32 * source + p[1] as u32 * keep + 32767) as u8,
            divide_unorm16(settings.color[2] as u32 * source + p[2] as u32 * keep + 32767) as u8,
            255,
        ]);
        changed |= after != outputs[i];
        outputs[i] = after;
    }
    changed
}
#[inline(always)]
fn divide_unorm16(value: u32) -> u32 {
    // Exact /65535 for coverage products, without WASM integer division.
    (value + 1 + (value >> 16)) >> 16
}
#[inline(always)]
fn composite_fixed(base: Rgba8, mut color: [u8; 4], source: u32, eraser: bool) -> Rgba8 {
    if source == 0 {
        return base;
    }
    let keep = 65535 - source;
    let mut out = base.0;
    if eraser {
        out[3] = divide_unorm16(base.0[3] as u32 * keep + 32767) as u8;
    } else if base.0[3] == 0 {
        out = color;
        out[3] = divide_unorm16(source * 255 + 32767) as u8;
    } else if base.0[3] == 255 {
        for c in 0..3 {
            out[c] =
                divide_unorm16(color[c] as u32 * source + base.0[c] as u32 * keep + 32767) as u8;
        }
    } else {
        color[3] = 255;
        return composite(base, color, source as f64 / 65535.0, false);
    }
    Rgba8(out)
}
#[inline(never)]
pub fn composite(base: Rgba8, color: [u8; 4], coverage: f64, eraser: bool) -> Rgba8 {
    let source = coverage.clamp(0.0, 1.0) * if eraser { 1.0 } else { color[3] as f64 / 255.0 };
    if source <= 0.0 {
        return base;
    }
    let mut out = base.0;
    if eraser {
        out[3] = (base.0[3] as f64 * (1.0 - source) + 0.5) as u32 as u8;
    } else if base.0[3] == 255 {
        for c in 0..3 {
            out[c] = (base.0[c] as f64 + (color[c] as f64 - base.0[c] as f64) * source + 0.5) as u32
                as u8;
        }
    } else {
        let d = base.0[3] as f64 / 255.0 * (1.0 - source);
        let alpha = source + d;
        let inverse = 1.0 / alpha;
        let source_weight = source * inverse;
        let destination_weight = d * inverse;
        for c in 0..3 {
            out[c] = (color[c] as f64 * source_weight + base.0[c] as f64 * destination_weight + 0.5)
                as u32 as u8;
        }
        out[3] = (alpha * 255.0 + 0.5) as u32 as u8;
    }
    Rgba8(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fotoeditor_core::{History, CPU_BUDGET};
    #[test]
    fn fixed_coverage_division_is_exact_over_its_product_domain() {
        let limit = 65535_u32 * 65535 + 32767;
        for n in [0, 1, 65534, 65535, 65536, limit - 1, limit] {
            assert_eq!(divide_unorm16(n), n / 65535);
        }
        let mut random = 1_u32;
        for _ in 0..100000 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = random.min(limit);
            assert_eq!(divide_unorm16(n), n / 65535);
        }
    }
    #[test]
    fn opaque_row_optimization_matches_general_stamping() {
        let mut doc = Document::blank(512, 256, 72.0).unwrap();
        for coord in [(0, 0), (1, 0)] {
            doc.raster
                .insert(coord, vec![Rgba8([39, 79, 131, 255]); TILE_PIXELS]);
        }
        let mut general = doc.clone();
        for coord in [(0, 0), (1, 0)] {
            general.raster.edit_tile(coord);
        }
        let settings = BrushSettings {
            size: 80.0,
            hardness: 0.0,
            opacity: 0.4,
            flow: 0.2,
            roundness: 0.7,
            angle: 36.0,
            color: [200, 33, 99, 127],
            ..BrushSettings::default()
        };
        let mut a = StrokeCommand::new(&doc, settings.clone(), CPU_BUDGET).unwrap();
        let mut b = StrokeCommand::new(&general, settings, CPU_BUDGET).unwrap();
        for p in [
            point(10.1, 120.2),
            point(500.00390625, 123.00390625),
            point(30.75, 140.25),
        ] {
            a.push(p).unwrap();
            b.push(p).unwrap();
        }
        finish(&mut a);
        finish(&mut b);
        for coord in [(0, 0), (1, 0)] {
            assert_eq!(
                a.preview.raster.tiles()[&coord].pin().unwrap().pixels,
                b.preview.raster.tiles()[&coord].pin().unwrap().pixels
            );
        }
    }
    #[test]
    fn optimized_source_over_matches_reference_within_one_quantization_level() {
        for a in [0, 1, 2, 127, 254, 255] {
            for color_alpha in [0, 1, 128, 255] {
                for coverage in [0.0, 0.001, 0.3, 0.5, 0.99, 1.0] {
                    let base = Rgba8([217, 33, 97, a]);
                    let color = [53, 177, 251, color_alpha];
                    let actual = composite(base, color, coverage, false);
                    let source = coverage * color_alpha as f64 / 255.0;
                    let fixed_source = (source * 65535.0 + 0.5) as u32;
                    let fixed = composite_fixed(base, color, fixed_source, false);
                    if fixed_source == 0 {
                        assert_eq!(fixed, base);
                    } else {
                        for c in 0..4 {
                            assert!(actual.0[c].abs_diff(fixed.0[c]) <= 1);
                        }
                    }
                    if source == 0.0 {
                        assert_eq!(actual, base);
                        continue;
                    }
                    let d = a as f64 / 255.0 * (1.0 - source);
                    let alpha = source + d;
                    for (c, channel) in color[..3].iter().enumerate() {
                        let reference = ((*channel as f64 * source + base.0[c] as f64 * d) / alpha)
                            .round() as u8;
                        assert!(actual.0[c].abs_diff(reference) <= 1);
                    }
                    assert_eq!(actual.0[3], (alpha * 255.0).round() as u8);
                }
            }
        }
        assert_eq!(
            composite(Rgba8([77, 99, 33, 255]), [0, 0, 0, 0], 1.0, true),
            Rgba8([77, 99, 33, 0])
        );
    }
    fn point(x: f64, y: f64) -> Sample {
        Sample {
            x,
            y,
            pressure: 1.0,
            ..Sample::default()
        }
    }
    fn finish(stroke: &mut StrokeCommand) {
        stroke.end();
        while !stroke.done() {
            stroke.step(131072).unwrap();
        }
    }
    #[test]
    fn fast_flick_is_contiguous_and_one_command_undoes_exactly() {
        let mut doc = Document::blank(1200, 200, 72.0).unwrap();
        doc.raster.write(0, 0, Rgba8([99, 77, 3, 0]));
        let mut s = StrokeCommand::new(&doc, BrushSettings::default(), CPU_BUDGET).unwrap();
        s.push(point(10.0, 100.0)).unwrap();
        s.push(point(1190.0, 100.0)).unwrap();
        finish(&mut s);
        for x in 10..1190 {
            assert_eq!(s.preview.raster.sample(x, 100).0[3], 255);
        }
        let mut history = History::default();
        history
            .execute(&mut doc, Box::new(s.finish().unwrap()), 0)
            .unwrap();
        assert_eq!(history.depths(), (1, 0));
        assert!(history.undo(&mut doc));
        assert_eq!(doc.raster.sample(0, 0), Rgba8([99, 77, 3, 0]));
        assert_eq!(doc.raster.tiles().len(), 1);
        history.redo(&mut doc);
        assert_eq!(doc.raster.sample(600, 100).0[3], 255);
    }
    #[test]
    fn two_hundred_soft_strokes_obey_history_and_total_cpu_limits() {
        let mut doc = Document::blank(6000, 4000, 300.0).unwrap();
        let mut history = History::default();
        for i in 0..200 {
            let available = CPU_BUDGET - doc.raster.bytes() - history.bytes(&doc) - 1048576;
            let mut stroke = StrokeCommand::new(
                &doc,
                BrushSettings {
                    size: 300.0,
                    hardness: 0.0,
                    opacity: 0.5,
                    color: [(i % 256) as u8, 33, 99, 255],
                    ..BrushSettings::default()
                },
                available,
            )
            .unwrap();
            stroke
                .push(point(
                    200.0 + (i % 20) as f64 * 280.0,
                    200.0 + (i / 20) as f64 * 280.0,
                ))
                .unwrap();
            finish(&mut stroke);
            assert!(
                doc.raster.bytes() + history.bytes(&doc) + stroke.temporary_bytes() + 1048576
                    <= CPU_BUDGET
            );
            history
                .execute(&mut doc, Box::new(stroke.finish().unwrap()), 0)
                .unwrap();
            assert!(history.bytes(&doc) <= history.limits.bytes);
            assert!(history.depths().0 <= 200);
            assert!(doc.raster.bytes() + history.bytes(&doc) <= CPU_BUDGET);
        }
    }
    #[test]
    fn flow_builds_under_stroke_opacity_and_eraser_preserves_rgb() {
        let doc = Document::blank(100, 100, 72.0).unwrap();
        let mut s = StrokeCommand::new(
            &doc,
            BrushSettings {
                size: 50.0,
                opacity: 0.4,
                flow: 0.2,
                ..BrushSettings::default()
            },
            CPU_BUDGET,
        )
        .unwrap();
        for x in [40.0, 60.0, 40.0, 60.0] {
            s.push(point(x, 50.0)).unwrap();
        }
        finish(&mut s);
        let alpha = s.preview.raster.sample(50, 50).0[3];
        assert!(alpha > 90 && alpha <= 102);
        assert_eq!(
            composite(Rgba8([77, 99, 33, 255]), [1, 2, 3, 255], 1.0, true),
            Rgba8([77, 99, 33, 0])
        );
        assert_eq!(
            composite(Rgba8([77, 99, 33, 0]), [1, 2, 3, 255], 0.0, false),
            Rgba8([77, 99, 33, 0])
        );
    }
    #[test]
    fn pressure_tilt_curves_pencil_budget_and_invalid_input() {
        assert_eq!(curve([0.0, 0.2, 0.6, 0.8, 1.0], 0.5), 0.6);
        let settings = BrushSettings {
            tilt_size: true,
            pressure_opacity: true,
            ..BrushSettings::default()
        };
        let (size, opacity) = settings.dynamics(Sample {
            pressure: 0.5,
            tilt_x: 90.0,
            ..Sample::default()
        });
        assert!((size - 0.1).abs() < 1e-9);
        assert_eq!(opacity, 0.5);
        let doc = Document::blank(20, 20, 72.0).unwrap();
        let mut s = StrokeCommand::new(
            &doc,
            BrushSettings {
                pencil: true,
                ..BrushSettings::default()
            },
            CPU_BUDGET,
        )
        .unwrap();
        s.push(point(3.2, 4.8)).unwrap();
        finish(&mut s);
        assert_eq!(s.preview.raster.sample(3, 4).0[3], 255);
        assert_eq!(s.preview.raster.sample(4, 4).0[3], 0);
        assert!(s
            .push(Sample {
                x: f64::NAN,
                ..point(1.0, 1.0)
            })
            .is_err());
        let mut limited = StrokeCommand::new(&doc, BrushSettings::default(), 200000).unwrap();
        limited.push(point(5.0, 5.0)).unwrap();
        assert!(limited.step(1000).is_err());
        assert_eq!(doc.raster.bytes(), 0);
    }
    #[test]
    fn rotated_elliptical_tip_clips_at_canvas_edges() {
        let doc = Document::blank(30, 30, 72.0).unwrap();
        let mut stroke = StrokeCommand::new(
            &doc,
            BrushSettings {
                size: 20.0,
                roundness: 0.25,
                angle: 90.0,
                ..BrushSettings::default()
            },
            CPU_BUDGET,
        )
        .unwrap();
        stroke.push(point(5.0, 5.0)).unwrap();
        finish(&mut stroke);
        assert_eq!(stroke.preview.raster.sample(5, 14).0[3], 255);
        assert_eq!(stroke.preview.raster.sample(14, 5).0[3], 0);
        assert_eq!(stroke.preview.raster.sample(5, 0).0[3], 255);
    }
    #[test]
    fn stabilizers_and_chunk_sizes_are_deterministic() {
        let doc = Document::blank(300, 200, 72.0).unwrap();
        for mode in [Stabilizer::Off, Stabilizer::Pulled, Stabilizer::Weighted] {
            let make = || {
                StrokeCommand::new(
                    &doc,
                    BrushSettings {
                        stabilizer: mode,
                        ..BrushSettings::default()
                    },
                    CPU_BUDGET,
                )
                .unwrap()
            };
            let mut a = make();
            let mut b = make();
            for i in 0..30 {
                let p = Sample {
                    time: i as f64 * 8.0,
                    ..point(10.0 + i as f64 * 8.0, 100.0 + (i % 2) as f64 * 4.0)
                };
                a.push(p).unwrap();
                b.push(p).unwrap();
            }
            a.end();
            b.end();
            while !a.done() {
                a.step(317).unwrap();
            }
            while !b.done() {
                b.step(131072).unwrap();
            }
            for y in 0..200 {
                for x in 0..300 {
                    assert_eq!(a.preview.raster.sample(x, y), b.preview.raster.sample(x, y));
                }
            }
        }
    }
}
