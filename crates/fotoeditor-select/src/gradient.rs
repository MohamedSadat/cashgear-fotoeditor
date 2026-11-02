//! Document-coordinate gradient evaluation with premultiplied sRGB stop mixing.
use serde::Deserialize;
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GradientShape {
    Linear,
    Radial,
    Angle,
    Reflected,
    Diamond,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GradientStop {
    pub position: f64,
    pub color: [u8; 4],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gradient {
    pub shape: GradientShape,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub stops: Vec<GradientStop>,
    pub opacity: f64,
    pub reverse: bool,
}
impl Gradient {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .start
            .iter()
            .chain(self.end.iter())
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || !(2..=32).contains(&self.stops.len())
            || self
                .stops
                .iter()
                .any(|s| !s.position.is_finite() || !(0.0..=1.0).contains(&s.position))
            || self
                .stops
                .windows(2)
                .any(|s| s[0].position >= s[1].position)
            || self.stops.first().is_none_or(|s| s.position != 0.0)
            || self.stops.last().is_none_or(|s| s.position != 1.0)
            || (self.end[0] - self.start[0]).hypot(self.end[1] - self.start[1]) < 0.001
        {
            return Err(
                "Gradient needs a nonzero drag and 2..32 ordered stops with endpoints at 0 and 1"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn parameter(&self, point: [f64; 2]) -> f64 {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let px = point[0] - self.start[0];
        let py = point[1] - self.start[1];
        let squared = dx * dx + dy * dy;
        let u = (px * dx + py * dy) / squared;
        let v = (py * dx - px * dy) / squared;
        let t = match self.shape {
            GradientShape::Linear => u,
            GradientShape::Radial => u.hypot(v),
            GradientShape::Angle => {
                if px == 0.0 && py == 0.0 {
                    0.0
                } else {
                    v.atan2(u).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU
                }
            }
            GradientShape::Reflected => u.abs(),
            GradientShape::Diamond => u.abs() + v.abs(),
        }
        .clamp(0.0, 1.0);
        if self.reverse {
            1.0 - t
        } else {
            t
        }
    }
    pub fn sample(&self, point: [f64; 2]) -> [u8; 4] {
        let t = self.parameter(point);
        let index = self
            .stops
            .partition_point(|s| s.position < t)
            .clamp(1, self.stops.len() - 1);
        let a = &self.stops[index - 1];
        let b = &self.stops[index];
        let f = ((t - a.position) / (b.position - a.position)).clamp(0.0, 1.0);
        let alpha = f64::from(a.color[3]) * (1.0 - f) + f64::from(b.color[3]) * f;
        let mut out = [0; 4];
        if alpha > 0.0 {
            for (c, v) in out[..3].iter_mut().enumerate() {
                *v = ((f64::from(a.color[c]) * f64::from(a.color[3]) * (1.0 - f)
                    + f64::from(b.color[c]) * f64::from(b.color[3]) * f)
                    / alpha
                    + 0.5) as u8;
            }
        }
        out[3] = (alpha * self.opacity + 0.5) as u8;
        out
    }
}
#[cfg(test)]
mod tests;
