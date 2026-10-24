use crate::Rgba8;
use serde::{Deserialize, Serialize};

/// The numerical specification is recorded in docs/P6-DESIGN.md.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Adjustment {
    BrightnessContrast {
        brightness: f64,
        contrast: f64,
    },
    Levels {
        black: f64,
        white: f64,
        gamma: f64,
        #[serde(rename = "outputBlack")]
        output_black: f64,
        #[serde(rename = "outputWhite")]
        output_white: f64,
    },
    Curves {
        points: Vec<[f64; 2]>,
    },
    Exposure {
        stops: f64,
        offset: f64,
        gamma: f64,
    },
    HueSaturation {
        hue: f64,
        saturation: f64,
        lightness: f64,
    },
}
fn range(v: f64, min: f64, max: f64) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
fn unit(v: f64) -> bool {
    range(v, 0.0, 1.0)
}
fn decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn quantize(v: f64) -> u8 {
    // Stabilize exact half-byte ties against floating-point evaluation order.
    (v.clamp(0.0, 1.0) * 255.0 + 0.5 + 1e-10) as u8
}
impl Adjustment {
    pub fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::BrightnessContrast {
                brightness,
                contrast,
            } => range(*brightness, -1.0, 1.0) && range(*contrast, -1.0, 1.0),
            Self::Levels {
                black,
                white,
                gamma,
                output_black,
                output_white,
            } => {
                unit(*black)
                    && unit(*white)
                    && black < white
                    && range(*gamma, 0.1, 10.0)
                    && unit(*output_black)
                    && unit(*output_white)
                    && output_black <= output_white
            }
            Self::Curves { points } => {
                (2..=16).contains(&points.len())
                    && points.first().is_some_and(|p| p[0] == 0.0)
                    && points.last().is_some_and(|p| p[0] == 1.0)
                    && points.iter().all(|p| unit(p[0]) && unit(p[1]))
                    && points.windows(2).all(|p| p[0][0] < p[1][0])
            }
            Self::Exposure {
                stops,
                offset,
                gamma,
            } => {
                range(*stops, -10.0, 10.0) && range(*offset, -1.0, 1.0) && range(*gamma, 0.1, 10.0)
            }
            Self::HueSaturation {
                hue,
                saturation,
                lightness,
            } => {
                range(*hue, -180.0, 180.0)
                    && range(*saturation, -1.0, 1.0)
                    && range(*lightness, -1.0, 1.0)
            }
        };
        if valid {
            Ok(())
        } else {
            Err("Invalid adjustment settings or curve points".into())
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::BrightnessContrast { .. } => "Brightness / Contrast",
            Self::Levels { .. } => "Levels",
            Self::Curves { .. } => "Curves",
            Self::Exposure { .. } => "Exposure",
            Self::HueSaturation { .. } => "Hue / Saturation",
        }
    }
    fn channel(&self, v: f64) -> f64 {
        match self {
            Self::BrightnessContrast {
                brightness,
                contrast,
            } => (v + brightness - 0.5) * (2.0f64).powf(2.0 * contrast) + 0.5,
            Self::Levels {
                black,
                white,
                gamma,
                output_black,
                output_white,
            } => {
                output_black
                    + ((v - black) / (white - black))
                        .clamp(0.0, 1.0)
                        .powf(1.0 / gamma)
                        * (output_white - output_black)
            }
            Self::Curves { points } => {
                let pair = points.windows(2).find(|p| v <= p[1][0]).unwrap();
                pair[0][1]
                    + (pair[1][1] - pair[0][1]) * (v - pair[0][0]) / (pair[1][0] - pair[0][0])
            }
            Self::Exposure {
                stops,
                offset,
                gamma,
            } => encode(
                (decode(v) * (2.0f64).powf(*stops) + offset)
                    .clamp(0.0, 1.0)
                    .powf(1.0 / gamma),
            ),
            Self::HueSaturation { .. } => unreachable!("HSL is not separable"),
        }
    }
    pub fn compile(&self) -> Result<AdjustmentKernel, String> {
        self.validate()?;
        Ok(match self {
            Self::HueSaturation {
                hue,
                saturation,
                lightness,
            } => AdjustmentKernel::Hsl([*hue / 360.0, *saturation, *lightness]),
            _ => AdjustmentKernel::Lut(Box::new(std::array::from_fn(|i| {
                self.channel(i as f64 / 255.0).clamp(0.0, 1.0)
            }))),
        })
    }
}
#[derive(Clone)]
pub enum AdjustmentKernel {
    Lut(Box<[f64; 256]>),
    Hsl([f64; 3]),
}
impl AdjustmentKernel {
    /// Straight color interpolation happens once, before quantization. Fully
    /// transparent hidden RGB and every alpha byte remain exact.
    pub fn sample(&self, before: Rgba8, coverage: u8) -> Rgba8 {
        if coverage == 0 || before.0[3] == 0 {
            return before;
        }
        let rgb = match self {
            Self::Lut(lut) => std::array::from_fn(|c| lut[before.0[c] as usize]),
            Self::Hsl(settings) => hsl(before.0, *settings),
        };
        let coverage = f64::from(coverage) / 255.0;
        let mut out = before;
        for (c, value) in rgb.into_iter().enumerate() {
            let old = f64::from(before.0[c]) / 255.0;
            out.0[c] = quantize(old + (value - old) * coverage);
        }
        out
    }
}
fn hsl(p: [u8; 4], settings: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = std::array::from_fn(|i| f64::from(p[i]) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut light = (max + min) * 0.5;
    let mut saturation = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * light - 1.0).abs())
    };
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    let hue = (hue / 6.0 + settings[0]).rem_euclid(1.0) * 6.0;
    saturation = (saturation * (1.0 + settings[1])).clamp(0.0, 1.0);
    light = if settings[2] < 0.0 {
        light * (1.0 + settings[2])
    } else {
        light + (1.0 - light) * settings[2]
    };
    let chroma = (1.0 - (2.0 * light - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - ((hue % 2.0) - 1.0).abs());
    let color = match hue as u32 {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    color.map(|c| (c + light - chroma * 0.5).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apply(a: Adjustment, p: [u8; 4]) -> [u8; 4] {
        a.compile().unwrap().sample(Rgba8(p), 255).0
    }
    #[test]
    fn independent_control_vectors() {
        assert_eq!(
            apply(
                Adjustment::BrightnessContrast {
                    brightness: 0.2,
                    contrast: 0.0
                },
                [0, 100, 255, 1]
            ),
            [51, 151, 255, 1]
        );
        assert_eq!(
            apply(
                Adjustment::BrightnessContrast {
                    brightness: 0.0,
                    contrast: 0.5
                },
                [64, 128, 192, 128]
            ),
            [1, 129, 255, 128]
        );
        assert_eq!(
            apply(
                Adjustment::Levels {
                    black: 0.25,
                    white: 0.75,
                    gamma: 1.0,
                    output_black: 0.2,
                    output_white: 0.8
                },
                [0, 128, 255, 255]
            ),
            [51, 128, 204, 255]
        );
        assert_eq!(
            apply(
                Adjustment::Curves {
                    points: vec![[0.0, 1.0], [1.0, 0.0]]
                },
                [0, 87, 255, 255]
            ),
            [255, 168, 0, 255]
        );
        assert_eq!(
            apply(
                Adjustment::Exposure {
                    stops: 1.0,
                    offset: 0.0,
                    gamma: 1.0
                },
                [0, 64, 128, 128]
            ),
            [0, 90, 176, 128]
        );
        assert_eq!(
            apply(
                Adjustment::HueSaturation {
                    hue: 120.0,
                    saturation: 0.0,
                    lightness: 0.0
                },
                [255, 0, 0, 255]
            ),
            [0, 255, 0, 255]
        );
        assert_eq!(
            apply(
                Adjustment::HueSaturation {
                    hue: 0.0,
                    saturation: -1.0,
                    lightness: 0.0
                },
                [255, 0, 0, 255]
            ),
            [128, 128, 128, 255]
        );
    }
    #[test]
    fn identities_are_exact_over_every_channel_and_alpha() {
        let settings = [
            Adjustment::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
            Adjustment::Levels {
                black: 0.0,
                white: 1.0,
                gamma: 1.0,
                output_black: 0.0,
                output_white: 1.0,
            },
            Adjustment::Curves {
                points: vec![[0.0, 0.0], [0.4, 0.4], [1.0, 1.0]],
            },
            Adjustment::Exposure {
                stops: 0.0,
                offset: 0.0,
                gamma: 1.0,
            },
            Adjustment::HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: 0.0,
            },
        ];
        for a in settings {
            let kernel = a.compile().unwrap();
            for v in 0..=255 {
                for alpha in [0, 1, 128, 255] {
                    let p = Rgba8([v, 255 - v, v.wrapping_mul(13), alpha]);
                    assert_eq!(kernel.sample(p, 255), p, "{a:?}");
                    assert_eq!(kernel.sample(p, 0), p);
                }
            }
        }
    }
    #[test]
    fn fractional_coverage_does_not_double_quantize_and_hidden_rgb_stays_exact() {
        let k = Adjustment::Curves {
            points: vec![[0.0, 1.0], [1.0, 0.0]],
        }
        .compile()
        .unwrap();
        assert_eq!(
            k.sample(Rgba8([87, 33, 19, 0]), 255),
            Rgba8([87, 33, 19, 0])
        );
        assert_eq!(
            k.sample(Rgba8([0, 100, 200, 1]), 128),
            Rgba8([128, 128, 127, 1])
        );
    }
    #[test]
    fn malformed_and_nonfinite_parameters_are_refused() {
        for points in [
            vec![],
            vec![[0.0, 0.0], [0.0, 1.0]],
            vec![[0.1, 0.0], [1.0, 1.0]],
            vec![[0.0, f64::NAN], [1.0, 1.0]],
        ] {
            assert!(Adjustment::Curves { points }.compile().is_err());
        }
        assert!(Adjustment::Exposure {
            stops: f64::INFINITY,
            offset: 0.0,
            gamma: 1.0
        }
        .compile()
        .is_err());
        assert!(Adjustment::Levels {
            black: 0.5,
            white: 0.5,
            gamma: 1.0,
            output_black: 0.0,
            output_white: 1.0
        }
        .compile()
        .is_err());
    }
}
