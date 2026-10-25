//! Document-edge geometry. Pixel centers are half-integers; no resampling here.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl TransformRect {
    pub fn validate(self) -> Result<Self, String> {
        if [self.x, self.y, self.width, self.height]
            .iter()
            .any(|v| !v.is_finite())
            || self.width <= 0.0
            || self.height <= 0.0
            || !(self.x + self.width).is_finite()
            || !(self.y + self.height).is_finite()
            || self.x + self.width <= self.x
            || self.y + self.height <= self.y
        {
            return Err("Transform bounds must be finite and have positive sides".into());
        }
        Ok(self)
    }
    pub fn corners(self) -> [[f64; 2]; 4] {
        [
            [self.x, self.y],
            [self.x + self.width, self.y],
            [self.x + self.width, self.y + self.height],
            [self.x, self.y + self.height],
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Interpolation {
    Nearest,
    Bilinear,
    Bicubic,
    #[default]
    Lanczos3,
}

/// Row-major homography mapping source edges to destination edges. `then` applies
/// its argument after this matrix, keeping one immutable source per edit session.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformMatrix {
    pub values: [f64; 9],
}
impl Default for TransformMatrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl TransformMatrix {
    pub const IDENTITY: Self = Self {
        values: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };
    pub fn translation(x: f64, y: f64) -> Self {
        Self {
            values: [1.0, 0.0, x, 0.0, 1.0, y, 0.0, 0.0, 1.0],
        }
    }
    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            values: [x, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 1.0],
        }
    }
    pub fn rotation(degrees: f64, pivot: [f64; 2]) -> Self {
        let (s, c) = match degrees.rem_euclid(360.0) {
            0.0 => (0.0, 1.0),
            90.0 => (1.0, 0.0),
            180.0 => (0.0, -1.0),
            270.0 => (-1.0, 0.0),
            _ => degrees.to_radians().sin_cos(),
        };
        Self::translation(-pivot[0], -pivot[1])
            .then(Self {
                values: [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0],
            })
            .then(Self::translation(pivot[0], pivot[1]))
    }
    pub fn then(self, next: Self) -> Self {
        let mut values = [0.0; 9];
        for row in 0..3 {
            for col in 0..3 {
                values[row * 3 + col] = (0..3)
                    .map(|k| next.values[row * 3 + k] * self.values[k * 3 + col])
                    .sum();
            }
        }
        Self { values }
    }
    fn denominator(self, [x, y]: [f64; 2]) -> f64 {
        self.values[6] * x + self.values[7] * y + self.values[8]
    }
    pub fn map(self, point: [f64; 2]) -> Result<[f64; 2], String> {
        let [x, y] = point;
        let w = self.denominator(point);
        let result = [
            (self.values[0] * x + self.values[1] * y + self.values[2]) / w,
            (self.values[3] * x + self.values[4] * y + self.values[5]) / w,
        ];
        if w == 0.0 || result.iter().any(|v| !v.is_finite()) {
            return Err("Transform crosses the projective horizon".into());
        }
        Ok(result)
    }
    pub fn inverse(self) -> Result<Self, String> {
        if self.values.iter().any(|v| !v.is_finite()) {
            return Err("Transform matrix must be finite".into());
        }
        // Normalize before cofactors: multiplying a homography by any nonzero
        // scalar must not change validity or cause determinant overflow.
        let norm = self.values.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
        if norm == 0.0 {
            return Err("Transform matrix is singular".into());
        }
        let [a, b, c, d, e, f, g, h, i] = self.values.map(|v| v / norm);
        let cofactors = [
            e * i - f * h,
            c * h - b * i,
            b * f - c * e,
            f * g - d * i,
            a * i - c * g,
            c * d - a * f,
            d * h - e * g,
            b * g - a * h,
            a * e - b * d,
        ];
        let determinant = a * cofactors[0] + b * cofactors[3] + c * cofactors[6];
        if determinant == 0.0 || !determinant.is_finite() {
            return Err("Transform matrix is singular".into());
        }
        let values = cofactors.map(|v| v / determinant / norm);
        if values.iter().any(|v| !v.is_finite()) {
            return Err("Transform inverse exceeds the supported range".into());
        }
        Ok(Self { values })
    }
    /// A linear denominator has no zero in a convex rectangle exactly when all
    /// corner denominators have the same nonzero sign. Reject before allocation.
    pub fn validate_for(self, source: TransformRect) -> Result<Self, String> {
        source.validate()?;
        self.inverse()?;
        let corners = source.corners();
        let sign = self.denominator(corners[0]).is_sign_positive();
        for point in corners {
            let w = self.denominator(point);
            if w == 0.0 || !w.is_finite() || w.is_sign_positive() != sign {
                return Err("Transform crosses the projective horizon".into());
            }
            let mapped = self.map(point)?;
            if mapped.iter().any(|v| v.abs() > 16_777_216.0) {
                return Err("Transformed coordinates exceed the supported range".into());
            }
        }
        Ok(self)
    }
    pub fn bounds(self, source: TransformRect) -> Result<TransformRect, String> {
        self.validate_for(source)?;
        let mut low = [f64::INFINITY; 2];
        let mut high = [f64::NEG_INFINITY; 2];
        for point in source.corners() {
            let p = self.map(point)?;
            for axis in 0..2 {
                low[axis] = low[axis].min(p[axis]);
                high[axis] = high[axis].max(p[axis]);
            }
        }
        TransformRect {
            x: low[0],
            y: low[1],
            width: high[0] - low[0],
            height: high[1] - low[1],
        }
        .validate()
    }
    /// Map a rectangle to clockwise (in screen coordinates) or counterclockwise
    /// convex handles. Reflected quads are valid; crossed or collapsed ones aren't.
    pub fn from_quad(source: TransformRect, quad: [[f64; 2]; 4]) -> Result<Self, String> {
        source.validate()?;
        if quad.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Transform handles must be finite".into());
        }
        let mut sign = None;
        for n in 0..4 {
            let a = quad[n];
            let b = quad[(n + 1) % 4];
            let c = quad[(n + 2) % 4];
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if cross == 0.0 || !cross.is_finite() || sign.is_some_and(|s| s != (cross > 0.0)) {
                return Err(
                    "Transform handles must form a noncollapsed convex quadrilateral".into(),
                );
            }
            sign = Some(cross > 0.0);
        }
        let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = quad;
        let dx1 = x1 - x2;
        let dx2 = x3 - x2;
        let dx3 = x0 - x1 + x2 - x3;
        let dy1 = y1 - y2;
        let dy2 = y3 - y2;
        let dy3 = y0 - y1 + y2 - y3;
        let (g, h) = if dx3 == 0.0 && dy3 == 0.0 {
            (0.0, 0.0)
        } else {
            let det = dx1 * dy2 - dx2 * dy1;
            if det == 0.0 {
                return Err("Transform handles are singular".into());
            }
            ((dx3 * dy2 - dx2 * dy3) / det, (dx1 * dy3 - dx3 * dy1) / det)
        };
        Self::translation(-source.x, -source.y)
            .then(Self::scale(1.0 / source.width, 1.0 / source.height))
            .then(Self {
                values: [
                    x1 - x0 + g * x1,
                    x3 - x0 + h * x3,
                    x0,
                    y1 - y0 + g * y1,
                    y3 - y0 + h * y3,
                    y0,
                    g,
                    h,
                    1.0,
                ],
            })
            .validate_for(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RECT: TransformRect = TransformRect {
        x: 123.0,
        y: -456.0,
        width: 6000.0,
        height: 4000.0,
    };
    fn close(a: [f64; 2], b: [f64; 2]) {
        assert!(
            (a[0] - b[0]).abs().max((a[1] - b[1]).abs()) < 1e-8,
            "{a:?} != {b:?}"
        );
    }
    #[test]
    fn perspective_handles_and_interior_roundtrips() {
        for quad in [
            [[5.0, 8.0], [6123.0, 9.0], [5000.0, 4300.0], [-10.0, 4000.0]],
            [[6000.0, 0.0], [0.0, 0.0], [100.0, 4000.0], [5900.0, 3800.0]],
        ] {
            let m = TransformMatrix::from_quad(RECT, quad).unwrap();
            for (source, target) in RECT.corners().into_iter().zip(quad) {
                close(m.map(source).unwrap(), target);
            }
            let inv = m.inverse().unwrap();
            for y in 0..=10 {
                for x in 0..=10 {
                    let p = [
                        RECT.x + RECT.width * x as f64 / 10.0,
                        RECT.y + RECT.height * y as f64 / 10.0,
                    ];
                    close(inv.map(m.map(p).unwrap()).unwrap(), p);
                }
            }
            // Homogeneous scale has no geometric meaning.
            for scale in [1e-100, -1e100] {
                let scaled = TransformMatrix {
                    values: m.values.map(|v| v * scale),
                };
                scaled.validate_for(RECT).unwrap();
                close(
                    scaled
                        .inverse()
                        .unwrap()
                        .map(scaled.map([1000.0, 2000.0]).unwrap())
                        .unwrap(),
                    [1000.0, 2000.0],
                );
            }
        }
    }
    #[test]
    fn accumulated_rotation_returns_to_source_before_resampling() {
        let pivot = [3123.0, 1544.0];
        let m =
            TransformMatrix::rotation(10.0, pivot).then(TransformMatrix::rotation(-10.0, pivot));
        for p in RECT.corners() {
            close(m.map(p).unwrap(), p);
        }
        let bounds = TransformMatrix::rotation(90.0, [0.0; 2])
            .bounds(RECT)
            .unwrap();
        assert!((bounds.width - RECT.height).abs() < 1e-8);
        assert!((bounds.height - RECT.width).abs() < 1e-8);
        assert_eq!(
            TransformMatrix::rotation(90.0, [0.0; 2])
                .then(TransformMatrix::rotation(-90.0, [0.0; 2])),
            TransformMatrix::IDENTITY
        );
    }
    #[test]
    fn reject_nonfinite_singular_crossed_and_horizon_inputs() {
        for m in [
            TransformMatrix::scale(0.0, 1.0),
            TransformMatrix::translation(f64::NAN, 0.0),
            TransformMatrix {
                values: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.001, 0.0, -2.0],
            },
        ] {
            assert!(m.validate_for(RECT).is_err());
        }
        assert!(TransformMatrix::from_quad(
            RECT,
            [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]]
        )
        .is_err());
        assert!(TransformMatrix::from_quad(RECT, [[0.0, 0.0]; 4]).is_err());
        assert!(TransformRect { width: 0.0, ..RECT }.validate().is_err());
        assert!(TransformRect { x: 1e100, ..RECT }.validate().is_err());
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]
        #[test]
        fn affine_roundtrip_across_document_scale(
            x in 0.0_f64..16384.0,
            y in 0.0_f64..16384.0,
            sx in 0.01_f64..100.0,
            sy in 0.01_f64..100.0,
            tx in -16384.0_f64..16384.0,
            ty in -16384.0_f64..16384.0,
            angle in -360.0_f64..360.0,
        ) {
            let m = TransformMatrix::scale(sx, sy)
                .then(TransformMatrix::rotation(angle, [8192.0; 2]))
                .then(TransformMatrix::translation(tx, ty));
            close(m.inverse().unwrap().map(m.map([x, y]).unwrap()).unwrap(), [x, y]);
        }
    }
}
