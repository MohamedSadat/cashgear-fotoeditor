//! Deterministic sRGB color controls. See the M4 numerical contract.
use crate::Adjustment;
impl Adjustment {
    pub(crate) fn color(&self, p: [u8; 4]) -> [f64; 3] {
        let rgb: [f64; 3] = std::array::from_fn(|c| f64::from(p[c]) / 255.0);
        let luma = |c: [f64; 3]| (54.0 * c[0] + 183.0 * c[1] + 19.0 * c[2]) / 256.0;
        let bin = (54 * u32::from(p[0]) + 183 * u32::from(p[1]) + 19 * u32::from(p[2]) + 128) / 256;
        match self {
            Self::Vibrance { amount } => {
                let high = rgb[0].max(rgb[1]).max(rgb[2]);
                let low = rgb[0].min(rgb[1]).min(rgb[2]);
                let light = luma(rgb);
                let gain = if *amount > 0.0 {
                    amount * (1.0 - high + low)
                } else {
                    *amount
                };
                rgb.map(|v| (v + gain * (v - light)).clamp(0.0, 1.0))
            }
            Self::ColorBalance {
                shadows,
                midtones,
                highlights,
                preserve_luminosity,
            } => {
                let l = luma(rgb);
                let s = (1.0 - 2.0 * l).max(0.0);
                let h = (2.0 * l - 1.0).max(0.0);
                let mut result: [f64; 3] = std::array::from_fn(|c| {
                    rgb[c] + shadows[c] * s + midtones[c] * (1.0 - s - h) + highlights[c] * h
                });
                if *preserve_luminosity {
                    let delta = l - luma(result);
                    result = result.map(|v| v + delta);
                    // Compress gamut around the original luma, retaining it exactly.
                    let min = result[0].min(result[1]).min(result[2]);
                    let max = result[0].max(result[1]).max(result[2]);
                    let scale =
                        (if min < 0.0 { l / (l - min) } else { 1.0f64 }).min(if max > 1.0 {
                            (1.0 - l) / (max - l)
                        } else {
                            1.0
                        });
                    result = result.map(|v| l + (v - l) * scale);
                }
                result.map(|v| v.clamp(0.0, 1.0))
            }
            Self::BlackWhite { weights } => {
                // Six hue sectors are piecewise linear in the byte channels.
                // Avoid hue division/wrapping near the red endpoint.
                let [r, g, b] = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
                let (low, high, middle, i, j) = if r >= g && g >= b {
                    (b, r, g, 0, 1)
                } else if g >= r && r >= b {
                    (b, g, r, 2, 1)
                } else if g >= b && b >= r {
                    (r, g, b, 2, 3)
                } else if b >= g && g >= r {
                    (r, b, g, 4, 3)
                } else if b >= r && r >= g {
                    (g, b, r, 4, 5)
                } else {
                    (g, r, b, 0, 5)
                };
                [(low + (high - low) * weights[i] + (middle - low) * (weights[j] - weights[i]))
                    .clamp(0.0, 255.0)
                    / 255.0; 3]
            }

            Self::Threshold { level } => [f64::from(u8::from(bin >= u32::from(*level))); 3],
            Self::GradientMap { stops } => {
                let v = f64::from(bin) / 255.0;
                let pair = stops.windows(2).find(|p| v <= p[1][0]).unwrap();
                let t = (v - pair[0][0]) / (pair[1][0] - pair[0][0]);
                std::array::from_fn(|c| pair[0][c + 1] + t * (pair[1][c + 1] - pair[0][c + 1]))
            }
            _ => unreachable!("Not a color transform"),
        }
    }
}

#[cfg(test)]
pub(crate) fn settings() -> Vec<Adjustment> {
    vec![
        Adjustment::Vibrance { amount: 0.7 },
        Adjustment::ColorBalance {
            shadows: [-0.3, 0.1, 0.4],
            midtones: [0.2, -0.4, 0.1],
            highlights: [0.3, 0.1, -0.2],
            preserve_luminosity: true,
        },
        Adjustment::BlackWhite {
            weights: [0.4, 0.8, 0.3, 0.7, -0.2, 1.1],
        },
        Adjustment::Invert,
        Adjustment::Threshold { level: 128 },
        Adjustment::Posterize { levels: 5 },
        Adjustment::GradientMap {
            stops: vec![
                [0.0, 0.2, 0.0, 0.4],
                [0.37, 1.0, 0.1, 0.2],
                [1.0, 0.8, 1.0, 0.2],
            ],
        },
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba8;
    fn apply(a: Adjustment, p: [u8; 4]) -> [u8; 4] {
        a.compile().unwrap().sample(Rgba8(p), 255).0
    }
    #[test]
    fn independent_color_vectors_and_threshold_boundary() {
        assert_eq!(
            apply(Adjustment::Invert, [17, 100, 255, 1]),
            [238, 155, 0, 1]
        );
        assert_eq!(
            apply(Adjustment::Posterize { levels: 4 }, [42, 128, 212, 128]),
            [0, 170, 170, 128]
        );
        assert_eq!(
            apply(Adjustment::Threshold { level: 128 }, [127, 127, 127, 255]),
            [0, 0, 0, 255]
        );
        assert_eq!(
            apply(Adjustment::Threshold { level: 128 }, [128, 128, 128, 255]),
            [255; 4]
        );
        assert_eq!(
            apply(Adjustment::Threshold { level: 128 }, [255, 0, 0, 255]),
            [0, 0, 0, 255]
        );
        let bw = Adjustment::BlackWhite {
            weights: [0.3, 0.89, 0.59, 0.7, 0.11, 0.41],
        };
        for (p, v) in [
            ([255, 0, 0, 255], 77),
            ([255, 255, 0, 255], 227),
            ([0, 255, 0, 255], 150),
            ([0, 255, 255, 255], 179),
            ([0, 0, 255, 255], 28),
            ([255, 0, 255, 255], 105),
        ] {
            assert_eq!(apply(bw.clone(), p), [v, v, v, 255]);
        }
        assert_eq!(
            apply(Adjustment::Vibrance { amount: -1.0 }, [255, 0, 0, 1]),
            [54, 54, 54, 1]
        );
        assert_eq!(
            apply(Adjustment::Vibrance { amount: 1.0 }, [128, 128, 128, 255]),
            [128, 128, 128, 255]
        );
        assert_eq!(
            apply(
                Adjustment::ColorBalance {
                    shadows: [0.0; 3],
                    midtones: [0.2, 0.0, -0.2],
                    highlights: [0.0; 3],
                    preserve_luminosity: false
                },
                [128, 128, 128, 255]
            ),
            [179, 128, 77, 255]
        );
        let gradient = Adjustment::GradientMap {
            stops: vec![[0.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0]],
        };
        assert_eq!(apply(gradient, [128, 128, 128, 128]), [127, 0, 128, 128]);
    }
    #[test]
    fn all_remaining_controls_preserve_hidden_rgb_alpha_and_zero_coverage() {
        for a in settings() {
            let kernel = a.compile().unwrap();
            for value in 0u8..=255 {
                for alpha in [0, 1, 128, 255] {
                    let p = Rgba8([value, value.wrapping_mul(13), 255 - value, alpha]);
                    assert_eq!(kernel.sample(p, 0), p);
                    for coverage in [1, 127, 128, 255] {
                        let out = kernel.sample(p, coverage);
                        assert_eq!(out.0[3], alpha);
                        if alpha == 0 {
                            assert_eq!(out, p);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn remaining_invalid_settings_are_refused_before_compile() {
        for a in [
            Adjustment::Vibrance { amount: f64::NAN },
            Adjustment::Posterize { levels: 1 },
            Adjustment::Posterize { levels: 257 },
            Adjustment::BlackWhite {
                weights: [f64::INFINITY; 6],
            },
            Adjustment::ColorBalance {
                shadows: [2.0; 3],
                midtones: [0.0; 3],
                highlights: [0.0; 3],
                preserve_luminosity: false,
            },
            Adjustment::GradientMap {
                stops: vec![[0.0; 4], [0.0, 1.0, 1.0, 1.0]],
            },
        ] {
            assert!(a.compile().is_err());
        }
    }
}
