use fotoeditor_core::{Gray8, TileCoord, TileRef, TileSurface, TILE_PIXELS, TILE_SIZE};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    #[default]
    New,
    Add,
    Subtract,
    Intersect,
}
impl SelectionMode {
    pub fn combine(self, before: u8, incoming: u8) -> u8 {
        match self {
            Self::New => incoming,
            Self::Add => before.max(incoming),
            Self::Subtract => before.saturating_sub(incoming),
            Self::Intersect => before.min(incoming),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum SelectionShape {
    Rectangle { x0: f64, y0: f64, x1: f64, y1: f64 },
    Ellipse { x0: f64, y0: f64, x1: f64, y1: f64 },
    Polygon { points: Vec<[f64; 2]> },
}
impl SelectionShape {
    fn bounds(&self) -> Result<[f64; 4], String> {
        let b = match self {
            Self::Rectangle { x0, y0, x1, y1 } | Self::Ellipse { x0, y0, x1, y1 } => {
                if [x0, y0, x1, y1]
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
                {
                    return Err("Invalid selection coordinate".into());
                }
                [x0.min(*x1), y0.min(*y1), x0.max(*x1), y0.max(*y1)]
            }
            Self::Polygon { points } => {
                if points.len() < 3 || points.len() > 4096 {
                    return Err("A lasso needs 3–4096 points".into());
                }
                let mut b = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for p in points {
                    if p.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) {
                        return Err("Invalid lasso coordinate".into());
                    }
                    b[0] = b[0].min(p[0]);
                    b[1] = b[1].min(p[1]);
                    b[2] = b[2].max(p[0]);
                    b[3] = b[3].max(p[1]);
                }
                b
            }
        };
        Ok(b)
    }
    fn contains(&self, x: f64, y: f64, b: [f64; 4]) -> bool {
        if x < b[0] || y < b[1] || x >= b[2] || y >= b[3] {
            return false;
        }
        match self {
            Self::Rectangle { .. } => true,
            Self::Ellipse { .. } => {
                let dx = (2.0 * x - b[0] - b[2]) / (b[2] - b[0]);
                let dy = (2.0 * y - b[1] - b[3]) / (b[3] - b[1]);
                dx * dx + dy * dy <= 1.0
            }
            Self::Polygon { points } => {
                let mut inside = false;
                let mut a = points[points.len() - 1];
                for &p in points {
                    if (a[1] > y) != (p[1] > y)
                        && x < (p[0] - a[0]) * (y - a[1]) / (p[1] - a[1]) + a[0]
                    {
                        inside = !inside;
                    }
                    a = p;
                }
                inside
            }
        }
    }
}
/// One tile per step. The caller hydrates `needed` and yields between steps.
pub struct ShapeSelectionJob {
    pub output: TileSurface<Gray8>,
    before: Option<Arc<TileSurface<Gray8>>>,
    shape: SelectionShape,
    bounds: [f64; 4],
    mode: SelectionMode,
    antialias: bool,
    next: u32,
    available: u64,
}
impl ShapeSelectionJob {
    pub fn new(
        width: u32,
        height: u32,
        before: Option<Arc<TileSurface<Gray8>>>,
        shape: SelectionShape,
        mode: SelectionMode,
        antialias: bool,
        available: u64,
    ) -> Result<Self, String> {
        fotoeditor_core::validate_size(width, height, 72.0)?;
        if before
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (width, height))
        {
            return Err("Selection dimensions differ from the document".into());
        }
        let bounds = shape.bounds()?;
        if available < TILE_PIXELS as u64 {
            return Err("Not enough memory for selection scratch".into());
        }
        let default = mode.combine(before.as_ref().map_or(0, |s| s.default.0), 0);
        Ok(Self {
            output: TileSurface::with_default(width, height, Gray8(default)),
            before,
            shape,
            bounds,
            mode,
            antialias,
            next: 0,
            available,
        })
    }
    pub fn remaining(&self) -> u32 {
        self.output.width.div_ceil(TILE_SIZE) * self.output.height.div_ceil(TILE_SIZE) - self.next
    }
    fn coord(&self) -> TileCoord {
        (
            self.next % self.output.width.div_ceil(TILE_SIZE),
            self.next / self.output.width.div_ceil(TILE_SIZE),
        )
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if self.remaining() > 0 {
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
        self.output.bytes() + TILE_PIXELS as u64
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.remaining() == 0 {
            return Ok(true);
        }
        let c = self.coord();
        let old = self
            .before
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let outside = f64::from((c.0 + 1) * TILE_SIZE) <= self.bounds[0]
            || f64::from((c.1 + 1) * TILE_SIZE) <= self.bounds[1]
            || f64::from(c.0 * TILE_SIZE) >= self.bounds[2]
            || f64::from(c.1 * TILE_SIZE) >= self.bounds[3];
        if outside && old.is_none() {
            self.next += 1;
            return Ok(self.remaining() == 0);
        }
        if self.output.bytes() + 2 * TILE_PIXELS as u64 > self.available {
            return Err("Selection exceeds the shared CPU budget".into());
        }
        let mut pixels = vec![self.output.default; TILE_PIXELS];
        let steps = if self.antialias { 4 } else { 1 };
        for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
            for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                let i = (y * TILE_SIZE + x) as usize;
                let mut hits = 0;
                if !outside {
                    for sy in 0..steps {
                        for sx in 0..steps {
                            hits += u32::from(self.shape.contains(
                                f64::from(c.0 * TILE_SIZE + x)
                                    + (f64::from(sx) + 0.5) / f64::from(steps),
                                f64::from(c.1 * TILE_SIZE + y)
                                    + (f64::from(sy) + 0.5) / f64::from(steps),
                                self.bounds,
                            ));
                        }
                    }
                }
                let incoming = ((hits * 255 + steps * steps / 2) / (steps * steps)) as u8;
                let before = old.as_ref().map_or_else(
                    || self.before.as_ref().map_or(0, |s| s.default.0),
                    |t| t.pixels[i].0,
                );
                pixels[i] = Gray8(self.mode.combine(before, incoming));
            }
        }
        self.output.insert(c, pixels);
        self.next += 1;
        Ok(self.remaining() == 0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(shape: SelectionShape, aa: bool) -> TileSurface<Gray8> {
        let mut j = ShapeSelectionJob::new(
            513,
            260,
            None,
            shape,
            SelectionMode::New,
            aa,
            4 * 1024 * 1024,
        )
        .unwrap();
        while !j.step().unwrap() {}
        j.output
    }
    #[test]
    fn rectangle_fractional_edges_tile_seams_and_degenerate_geometry() {
        let s = run(
            SelectionShape::Rectangle {
                x0: 255.5,
                y0: 0.0,
                x1: 257.0,
                y1: 260.0,
            },
            true,
        );
        assert_eq!(s.sample(254, 0), Gray8(0));
        assert_eq!(s.sample(255, 259), Gray8(128));
        assert_eq!(s.sample(256, 259), Gray8(255));
        assert_eq!(s.sample(257, 0), Gray8(0));
        assert_eq!(
            run(
                SelectionShape::Ellipse {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 0.0,
                    y1: 200.0
                },
                true
            )
            .bytes(),
            0
        );
        assert!(ShapeSelectionJob::new(
            2,
            2,
            None,
            SelectionShape::Rectangle {
                x0: f64::NAN,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0
            },
            SelectionMode::New,
            true,
            65536
        )
        .is_err());
    }
    #[test]
    fn polygon_ellipse_and_coverage_algebra() {
        let s = run(
            SelectionShape::Polygon {
                points: vec![[250.0, 0.0], [260.0, 0.0], [255.0, 10.0]],
            },
            false,
        );
        assert_eq!(s.sample(255, 1), Gray8(255));
        assert_eq!(s.sample(250, 9), Gray8(0));
        let e = run(
            SelectionShape::Ellipse {
                x0: 250.0,
                y0: 0.0,
                x1: 260.0,
                y1: 10.0,
            },
            true,
        );
        assert_eq!(e.sample(255, 5), Gray8(255));
        assert_eq!(e.sample(250, 0), Gray8(0));
        assert_eq!(SelectionMode::Add.combine(100, 150), 150);
        assert_eq!(SelectionMode::Subtract.combine(100, 150), 0);
        assert_eq!(SelectionMode::Intersect.combine(100, 150), 100);
    }
}
