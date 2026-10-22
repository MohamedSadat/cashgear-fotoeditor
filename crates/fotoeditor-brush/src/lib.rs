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
    sin: f64,
    cos: f64,
    x0: u32,
    x1: u32,
    y1: u32,
    x: u32,
    y: u32,
}
/// Scratch preview becomes one atomic TileCommand; work and input are bounded.
pub struct StrokeCommand {
    pub preview: Document,
    pub applied_time: f64,
    base: Document,
    settings: BrushSettings,
    coverage: HashMap<TileCoord, Vec<f32>>,
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
        if available < (QUEUE_LIMIT * size_of::<Sample>()) as u64 {
            return Err("Not enough memory for a stroke".into());
        }
        let mut preview = base.clone();
        preview.raster.drain_dirty();
        Ok(Self {
            preview,
            applied_time: 0.0,
            base: base.clone(),
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
                    let p = s.from.lerp(s.to, s.next / s.length);
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
        let (sin, cos) = self.settings.angle.to_radians().sin_cos();
        self.dab = Some(Dab {
            p,
            radius,
            influence,
            sin,
            cos,
            x0,
            x1,
            y1,
            x: x0,
            y: y0,
        });
    }
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
                    + (self.queue.capacity() * size_of::<Sample>()) as u64;
                if required > self.available {
                    return Err("This stroke exceeds the 768 MiB document/history/temporary budget; stroke cancelled".into());
                }
                self.coverage.insert(coord, vec![0.0; TILE_PIXELS]);
                self.touched.push(coord);
            }
            let mask = self.coverage.get_mut(&coord).unwrap();
            let base = self.base.raster.tiles().get(&coord);
            let output = self.preview.raster.edit_tile(coord);
            let end = dab
                .x1
                .min((coord.0 + 1) * TILE_SIZE)
                .min(dab.x + pixels.min(TILE_SIZE as usize) as u32);
            let inv_x = 1.0 / dab.radius;
            let inv_y = inv_x
                / if self.settings.pencil {
                    1.0
                } else {
                    self.settings.roundness
                };
            for x in dab.x..end {
                let dx = x as f64 + 0.5 - dab.p.x;
                let dy = dab.y as f64 + 0.5 - dab.p.y;
                let rx = (dx * dab.cos + dy * dab.sin) * inv_x;
                let ry = (-dx * dab.sin + dy * dab.cos) * inv_y;
                let distance2 = rx * rx + ry * ry;
                if distance2 > 1.0 {
                    continue;
                }
                let tip = if self.settings.hardness >= 1.0 || self.settings.pencil {
                    1.0
                } else {
                    ((1.0 - distance2.sqrt()) / (1.0 - self.settings.hardness)).min(1.0)
                };
                let i = ((dab.y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize;
                mask[i] = (1.0
                    - (1.0 - mask[i] as f64) * (1.0 - tip * self.settings.flow * dab.influence))
                    as f32;
                let before = base.map_or(Rgba8::default(), |t| t.pixels[i]);
                output[i] = composite(
                    before,
                    self.settings.color,
                    self.settings.opacity * mask[i] as f64,
                    self.settings.eraser,
                );
            }
            pixels -= (end - dab.x) as usize;
            dab.x = end;
            if dab.x == dab.x1 {
                dab.x = dab.x0;
                dab.y += 1;
            }
            if dab.y == dab.y1 {
                self.applied_time = self.applied_time.max(dab.p.time);
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
pub fn composite(base: Rgba8, color: [u8; 4], coverage: f64, eraser: bool) -> Rgba8 {
    let source = coverage.clamp(0.0, 1.0) * if eraser { 1.0 } else { color[3] as f64 / 255.0 };
    if source <= 0.0 {
        return base;
    }
    let mut out = base.0;
    if eraser {
        out[3] = (base.0[3] as f64 * (1.0 - source)).round() as u8;
    } else {
        let d = base.0[3] as f64 / 255.0 * (1.0 - source);
        let alpha = source + d;
        for c in 0..3 {
            out[c] = ((color[c] as f64 * source + base.0[c] as f64 * d) / alpha).round() as u8;
        }
        out[3] = (alpha * 255.0).round() as u8;
    }
    Rgba8(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fotoeditor_core::{History, CPU_BUDGET};
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
