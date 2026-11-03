//! CPU-only coverage assets, reproducible dab dynamics and painting symmetry.
use super::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageImage {
    pub width: u32,
    pub height: u32,
    /// Canonical lowercase hexadecimal Gray8 coverage; no encoded-image decoder here.
    pub coverage: String,
}
impl CoverageImage {
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || self.width > 512
            || self.height > 512
            || self.coverage.len() != (self.width * self.height * 2) as usize
            || !self
                .coverage
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Invalid brush coverage image (maximum 512 x 512)".into());
        }
        Ok(())
    }
    fn pixel(&self, x: usize, y: usize) -> f64 {
        let i = (y * self.width as usize + x) * 2;
        let nibble = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        f64::from(
            nibble(self.coverage.as_bytes()[i]) * 16 + nibble(self.coverage.as_bytes()[i + 1]),
        ) / 255.0
    }
    pub(super) fn sample(&self, x: f64, y: f64) -> f64 {
        if x < -0.5
            || y < -0.5
            || x >= f64::from(self.width) - 0.5
            || y >= f64::from(self.height) - 0.5
        {
            return 0.0;
        }
        let x = x.clamp(0.0, f64::from(self.width - 1));
        let y = y.clamp(0.0, f64::from(self.height - 1));
        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        let jx = (ix + 1).min(self.width as usize - 1);
        let jy = (iy + 1).min(self.height as usize - 1);
        let fx = x - ix as f64;
        let fy = y - iy as f64;
        (self.pixel(ix, iy) * (1.0 - fx) + self.pixel(jx, iy) * fx) * (1.0 - fy)
            + (self.pixel(ix, jy) * (1.0 - fx) + self.pixel(jx, jy) * fx) * fy
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SymmetryMode {
    #[default]
    Off,
    Vertical,
    Horizontal,
    Both,
    Radial,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Symmetry {
    pub mode: SymmetryMode,
    pub center: [f64; 2],
    pub sectors: u32,
}
impl Default for Symmetry {
    fn default() -> Self {
        Self {
            mode: SymmetryMode::Off,
            center: [0.0; 2],
            sectors: 6,
        }
    }
}
impl Symmetry {
    pub(super) fn validate(self) -> Result<(), String> {
        if !(2..=16).contains(&self.sectors)
            || self
                .center
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=16384.0).contains(v))
        {
            return Err("Invalid painting symmetry".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct PlannedDab {
    pub p: Sample,
    pub size: f64,
    pub influence: f64,
    /// Inverse orthonormal transform from document offsets to tip offsets.
    pub transform: [f64; 4],
}
impl StrokeCommand {
    fn random(&mut self) -> f64 {
        self.random = self.random.wrapping_mul(1664525).wrapping_add(1013904223);
        f64::from(self.random) / 4294967296.0
    }
    pub(super) fn plan_dabs(&mut self, mut p: Sample) {
        let scatter_radius = self.random().sqrt() * self.settings.scatter * self.settings.size;
        let scatter_angle = self.random() * std::f64::consts::TAU;
        p.x += scatter_radius * scatter_angle.cos();
        p.y += scatter_radius * scatter_angle.sin();
        let size = 1.0 - self.random() * self.settings.size_jitter;
        let angle = (self.random() * 2.0 - 1.0) * self.settings.angle_jitter * std::f64::consts::PI;
        let influence = 1.0 - self.random() * self.settings.opacity_jitter;
        let (sin, cos) = angle.sin_cos();
        let sym = self.settings.symmetry;
        let n = match sym.mode {
            SymmetryMode::Off => 1,
            SymmetryMode::Both => 4,
            SymmetryMode::Radial => sym.sectors,
            _ => 2,
        };
        for i in 0..n {
            let (a, b, c, d) = match sym.mode {
                SymmetryMode::Vertical if i == 1 => (-1.0, 0.0, 0.0, 1.0),
                SymmetryMode::Horizontal if i == 1 => (1.0, 0.0, 0.0, -1.0),
                SymmetryMode::Both => (
                    if i & 1 == 0 { 1.0 } else { -1.0 },
                    0.0,
                    0.0,
                    if i & 2 == 0 { 1.0 } else { -1.0 },
                ),
                SymmetryMode::Radial => {
                    if i * 4 % n == 0 {
                        match i * 4 / n {
                            0 => (1.0, 0.0, 0.0, 1.0),
                            1 => (0.0, -1.0, 1.0, 0.0),
                            2 => (-1.0, 0.0, 0.0, -1.0),
                            _ => (0.0, 1.0, -1.0, 0.0),
                        }
                    } else {
                        let (s, c) =
                            (f64::from(i) * std::f64::consts::TAU / f64::from(n)).sin_cos();
                        (c, -s, s, c)
                    }
                }
                _ => (1.0, 0.0, 0.0, 1.0),
            };
            let x = p.x - sym.center[0];
            let y = p.y - sym.center[1];
            let q = if i == 0 {
                p
            } else {
                Sample {
                    x: sym.center[0] + a * x + b * y,
                    y: sym.center[1] + c * x + d * y,
                    ..p
                }
            };
            if self
                .planned
                .iter()
                .any(|v| (v.p.x - q.x).abs() < 1e-8 && (v.p.y - q.y).abs() < 1e-8)
            {
                continue;
            }
            self.planned.push_back(PlannedDab {
                p: q,
                size,
                influence,
                transform: [
                    cos * a + sin * b,
                    cos * c + sin * d,
                    -sin * a + cos * b,
                    -sin * c + cos * d,
                ],
            });
        }
    }
    pub(super) fn begin_next_dab(&mut self) -> bool {
        if self.planned.is_empty() {
            let Some(p) = self.next_dab() else {
                return false;
            };
            self.plan_dabs(p);
        }
        let planned = self.planned.pop_front().unwrap();
        self.begin_planned_dab(planned);
        true
    }
}

impl BrushSettings {
    pub(super) fn asset_bytes(&self) -> u64 {
        self.tip_image
            .as_ref()
            .map_or(0, |v| v.coverage.capacity() as u64)
            + self
                .texture
                .as_ref()
                .map_or(0, |v| v.coverage.capacity() as u64)
    }
}

impl Tip {
    #[inline(always)]
    pub(super) fn at(&self, dab: &Dab, x: f64, y: f64) -> u32 {
        let dx = x + 0.5 - dab.p.x;
        let dy = y + 0.5 - dab.p.y;
        let [a, b, c, d] = dab.transform;
        let tx = a * dx + b * dy;
        let ty = c * dx + d * dy;
        let scale = self.radius / dab.radius;
        let value = self.sample(
            tx * scale + self.radius + 0.5,
            ty * scale + self.radius + 0.5,
        );
        let Some(texture) = &self.texture else {
            return value;
        };
        let sx = (tx / self.texture_scale + 0.5).rem_euclid(1.0) * f64::from(texture.width);
        let sy = (ty / self.texture_scale + 0.5).rem_euclid(1.0) * f64::from(texture.height);
        // Nearest periodic texture avoids fading at the wrap seam.
        let coverage = texture.pixel(sx.floor() as usize, sy.floor() as usize);
        (f64::from(value) * (1.0 - self.texture_depth + self.texture_depth * coverage) + 0.5) as u32
    }
}

#[cfg(test)]
mod tests;
