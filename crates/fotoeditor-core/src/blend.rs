//! sRGB blend reference, using unassociated colors for blend functions and
//! premultiplied temporary composites. Authoritative Rgba8 samples are untouched.
//! Equations: https://www.w3.org/TR/compositing-1/#blending
use crate::Rgba8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
impl BlendMode {
    pub const ALL: [Self; 16] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::HardLight,
        Self::SoftLight,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
    pub fn from_index(index: u32) -> Result<Self, String> {
        Self::ALL
            .get(index as usize)
            .copied()
            .ok_or_else(|| "Unknown blend mode".into())
    }
}
type Color = [f64; 3];
fn lum(c: Color) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn sat(c: Color) -> f64 {
    c.into_iter().fold(0.0, f64::max) - c.into_iter().fold(1.0, f64::min)
}
fn set_lum(mut c: Color, l: f64) -> Color {
    let delta = l - lum(c);
    c = c.map(|v| v + delta);
    let n = c.into_iter().fold(f64::INFINITY, f64::min);
    let x = c.into_iter().fold(f64::NEG_INFINITY, f64::max);
    if n < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    c
}
fn set_sat(c: Color, s: f64) -> Color {
    let n = c.into_iter().fold(1.0, f64::min);
    let x = c.into_iter().fold(0.0, f64::max);
    if x > n {
        c.map(|v| (v - n) * s / (x - n))
    } else {
        [0.0; 3]
    }
}
fn hard_light(b: f64, s: f64) -> f64 {
    if s <= 0.5 {
        2.0 * b * s
    } else {
        1.0 - 2.0 * (1.0 - b) * (1.0 - s)
    }
}
pub fn blend_rgb(mode: BlendMode, b: Color, s: Color) -> Color {
    use BlendMode::*;
    let result = match mode {
        Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        Color => set_lum(s, lum(b)),
        Luminosity => set_lum(b, lum(s)),
        _ => std::array::from_fn(|i| {
            let (b, s) = (b[i], s[i]);
            match mode {
                Normal => s,
                Multiply => b * s,
                Screen => b + s - b * s,
                Overlay => hard_light(s, b),
                Darken => b.min(s),
                Lighten => b.max(s),
                ColorDodge => {
                    if b == 0.0 {
                        0.0
                    } else if s == 1.0 {
                        1.0
                    } else {
                        (b / (1.0 - s)).min(1.0)
                    }
                }
                ColorBurn => {
                    if b == 1.0 {
                        1.0
                    } else if s == 0.0 {
                        0.0
                    } else {
                        1.0 - ((1.0 - b) / s).min(1.0)
                    }
                }
                HardLight => hard_light(b, s),
                SoftLight => {
                    if s <= 0.5 {
                        b - (1.0 - 2.0 * s) * b * (1.0 - b)
                    } else {
                        let d = if b <= 0.25 {
                            ((16.0 * b - 12.0) * b + 4.0) * b
                        } else {
                            b.sqrt()
                        };
                        b + (2.0 * s - 1.0) * (d - b)
                    }
                }
                Difference => (b - s).abs(),
                Exclusion => b + s - 2.0 * b * s,
                _ => unreachable!("nonseparable modes handled above"),
            }
        }),
    };
    result.map(|v| v.clamp(0.0, 1.0))
}
/// Float premultiplied display/compositing sample. Never stored as document data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Composite(pub [f64; 4]);
impl Composite {
    pub fn from_straight(p: Rgba8) -> Self {
        let a = f64::from(p.0[3]) / 255.0;
        Self([
            f64::from(p.0[0]) / 255.0 * a,
            f64::from(p.0[1]) / 255.0 * a,
            f64::from(p.0[2]) / 255.0 * a,
            a,
        ])
    }
    pub fn straight(self) -> Rgba8 {
        let a = self.0[3];
        if a <= 0.0 {
            return Rgba8::default();
        }
        Rgba8([
            quantize(self.0[0] / a),
            quantize(self.0[1] / a),
            quantize(self.0[2] / a),
            quantize(a),
        ])
    }
    pub fn premultiplied(self) -> [u8; 4] {
        self.0.map(quantize)
    }
    /// Source-over, or source-atop for a clipping chain (preserves base alpha).
    pub fn blend(self, source: Self, mode: BlendMode, factor: f64, clip: bool) -> Self {
        let ab = self.0[3];
        let original_as = source.0[3];
        let a = original_as * factor.clamp(0.0, 1.0);
        if a == 0.0 || (clip && ab == 0.0) {
            return self;
        }
        let b = std::array::from_fn(|i| if ab > 0.0 { self.0[i] / ab } else { 0.0 });
        let s = std::array::from_fn(|i| source.0[i] / original_as);
        let blended = blend_rgb(mode, b, s);
        let rgb: [f64; 3] = std::array::from_fn(|i| {
            (1.0 - a) * self.0[i]
                + a * (ab * blended[i] + if clip { 0.0 } else { (1.0 - ab) * s[i] })
        });
        Self([
            rgb[0],
            rgb[1],
            rgb[2],
            if clip { ab } else { a + ab * (1.0 - a) },
        ])
    }
    /// Pass-through group opacity/mask interpolates its entire effect on backdrop.
    pub fn interpolate(self, result: Self, factor: f64) -> Self {
        let f = factor.clamp(0.0, 1.0);
        Self(std::array::from_fn(|i| {
            self.0[i] + (result.0[i] - self.0[i]) * f
        }))
    }
}
fn quantize(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_opaque_channel_goldens_cover_all_modes() {
        // Hand-calculated 8-bit expected values for b=.2,.4,.6; s=.8,.3,.1.
        let expected = [
            [204, 77, 26],
            [41, 31, 15],
            [214, 148, 163],
            [82, 61, 71],
            [51, 77, 26],
            [204, 102, 153],
            [255, 146, 170],
            [0, 0, 0],
            [173, 61, 31],
            [89, 78, 104],
            [153, 26, 128],
            [173, 117, 148],
            [147, 74, 45],
            [20, 109, 199],
            [187, 60, 9],
            [68, 119, 170],
        ];
        for (mode, expected) in BlendMode::ALL.into_iter().zip(expected) {
            let actual = blend_rgb(mode, [0.2, 0.4, 0.6], [0.8, 0.3, 0.1]).map(quantize);
            for (a, e) in actual.into_iter().zip(expected) {
                assert!(
                    (i32::from(a) - e).abs() <= 1,
                    "{mode:?}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
    #[test]
    fn alpha_mask_clipping_and_pass_through_have_independent_expected_results() {
        let b = Composite::from_straight(Rgba8([255, 0, 0, 128]));
        let s = Composite::from_straight(Rgba8([0, 0, 255, 255]));
        assert_eq!(
            b.blend(s, BlendMode::Normal, 0.5, false).straight(),
            Rgba8([85, 0, 170, 192])
        );
        assert_eq!(
            b.blend(s, BlendMode::Normal, 0.5, true).straight(),
            Rgba8([128, 0, 128, 128])
        );
        assert_eq!(b.interpolate(s, 0.5).straight(), Rgba8([85, 0, 170, 192]));
        for mode in BlendMode::ALL {
            assert_eq!(b.blend(s, mode, 0.0, false), b);
            assert_eq!(Composite::default().blend(s, mode, 1.0, false), s);
            assert_eq!(
                Composite::default().blend(s, mode, 1.0, true),
                Composite::default()
            );
        }
    }
    #[test]
    fn boundaries_never_produce_nan_and_hidden_rgb_never_contributes() {
        let values = [0, 1, 127, 128, 254, 255];
        for mode in BlendMode::ALL {
            for b in values {
                for s in values {
                    let b = Composite::from_straight(Rgba8([b, 255 - b, b, b]));
                    let s = Composite::from_straight(Rgba8([s, s, 255 - s, s]));
                    assert!(b
                        .blend(s, mode, 1.0, false)
                        .0
                        .into_iter()
                        .all(|v| v.is_finite() && (0.0..=1.0000001).contains(&v)));
                }
            }
        }
        assert_eq!(
            Composite::from_straight(Rgba8([87, 33, 19, 0])),
            Composite::default()
        );
    }
}
