//! Bounded spatial controls over immutable premultiplied tile halos.
use crate::spatial::{Filter, NoiseDistribution};

pub(crate) fn hash(mut v: u32) -> u32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846ca68b);
    v ^ (v >> 16)
}
fn uniform(v: u32) -> f32 {
    ((hash(v) >> 8) as f32 + 0.5) / 16777216.0
}
pub(crate) fn noise(
    x: u32,
    y: u32,
    seed: u32,
    channel: u32,
    distribution: NoiseDistribution,
) -> f32 {
    let key = x.wrapping_mul(0x9e3779b9)
        ^ y.wrapping_mul(0x85ebca6b)
        ^ seed
        ^ channel.wrapping_mul(0xc2b2ae35);
    let u = uniform(key);
    match distribution {
        NoiseDistribution::Uniform => 2.0 * u - 1.0,
        NoiseDistribution::Gaussian => {
            (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * uniform(key ^ 0xa511e9b3)).cos() / 3.0
        }
    }
}
pub(crate) fn offset(distance: i32, angle: f32) -> (i32, i32) {
    let a = f64::from(angle).to_radians();
    (
        (f64::from(distance) * a.cos()).round() as i32,
        (f64::from(distance) * a.sin()).round() as i32,
    )
}
fn straight(p: [f32; 4]) -> [f32; 3] {
    if p[3] > 0.0 {
        std::array::from_fn(|c| p[c] * 255.0 / p[3])
    } else {
        [0.0; 3]
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_extra(
    input: &[[f32; 4]],
    out: &mut [[f32; 4]],
    stride: usize,
    width: usize,
    height: usize,
    origin: (u32, u32),
    canvas: (u32, u32),
    filter: &Filter,
) -> bool {
    let r = filter.radius() as i32;
    let at = |x: i32, y: i32| input[((y + r) as usize) * stride + (x + r) as usize];
    match *filter {
        Filter::Pixelate { size } => {
            let first = (origin.0 / size * size, origin.1 / size * size);
            let end = (origin.0 + width as u32, origin.1 + height as u32);
            for by in (first.1..end.1).step_by(size as usize) {
                for bx in (first.0..end.0).step_by(size as usize) {
                    let ex = (bx + size).min(canvas.0);
                    let ey = (by + size).min(canvas.1);
                    let mut sum = [0.0; 4];
                    for y in by..ey {
                        for x in bx..ex {
                            let p = at(x as i32 - origin.0 as i32, y as i32 - origin.1 as i32);
                            for c in 0..4 {
                                sum[c] += p[c];
                            }
                        }
                    }
                    let count = ((ex - bx) * (ey - by)) as f32;
                    let value = sum.map(|v| v / count);
                    for y in by.max(origin.1)..ey.min(end.1) {
                        for x in bx.max(origin.0)..ex.min(end.0) {
                            out[(y - origin.1) as usize * width + (x - origin.0) as usize] = value;
                        }
                    }
                }
            }
        }
        Filter::MotionBlur { angle, .. } => {
            let offsets: Vec<_> = (-r..=r).map(|k| offset(k, angle)).collect();
            for y in 0..height {
                for x in 0..width {
                    let mut sum = [0.0; 4];
                    for &(dx, dy) in &offsets {
                        let p = at(x as i32 + dx, y as i32 + dy);
                        for c in 0..4 {
                            sum[c] += p[c];
                        }
                    }
                    out[y * width + x] = sum.map(|v| v / offsets.len() as f32);
                }
            }
        }
        Filter::AddNoise {
            amount,
            seed,
            monochrome,
            distribution,
        } => {
            for y in 0..height {
                for x in 0..width {
                    let before = at(x as i32, y as i32);
                    let rgb = straight(before);
                    let mut p = before;
                    for c in 0..3 {
                        let n = noise(
                            origin.0 + x as u32,
                            origin.1 + y as u32,
                            seed,
                            if monochrome { 0 } else { c as u32 },
                            distribution,
                        );
                        p[c] = (rgb[c] + 255.0 * amount * n).clamp(0.0, 255.0) * before[3] / 255.0;
                    }
                    out[y * width + x] = p;
                }
            }
        }
        Filter::Emboss { angle, amount, .. } => {
            let (dx, dy) = offset(r, angle);
            for y in 0..height {
                for x in 0..width {
                    let before = at(x as i32, y as i32);
                    let a = straight(at(x as i32 + dx, y as i32 + dy));
                    let b = straight(at(x as i32 - dx, y as i32 - dy));
                    let mut p = before;
                    if r > 0 && amount > 0.0 {
                        for c in 0..3 {
                            p[c] = (128.0 + amount * (a[c] - b[c]) * 0.5).clamp(0.0, 255.0)
                                * before[3]
                                / 255.0;
                        }
                    }
                    out[y * width + x] = p;
                }
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilterJob, FILTER_SCRATCH};
    use fotoeditor_core::*;
    fn run(doc: &Document, filter: Filter) -> FilterJob {
        let mut job = FilterJob::new(doc, doc.layers[0].id, filter, 100 * 1024 * 1024).unwrap();
        while !job.step().unwrap() {}
        job
    }
    fn controls() -> Vec<Filter> {
        vec![
            Filter::MotionBlur {
                radius: 7,
                angle: 31.0,
            },
            Filter::HighPass { radius: 3 },
            Filter::AddNoise {
                amount: 0.4,
                seed: 123,
                monochrome: false,
                distribution: NoiseDistribution::Uniform,
            },
            Filter::AddNoise {
                amount: 0.4,
                seed: u32::MAX,
                monochrome: true,
                distribution: NoiseDistribution::Gaussian,
            },
            Filter::Emboss {
                radius: 4,
                angle: -41.0,
                amount: 1.3,
            },
            Filter::Pixelate { size: 37 },
        ]
    }
    #[test]
    fn independent_motion_emboss_and_pixel_blocks_cross_both_tile_axes() {
        let mut doc = Document::blank(259, 259, 72.0).unwrap();
        for y in 0..259 {
            for x in 0..259 {
                doc.raster_mut().write(
                    x,
                    y,
                    Rgba8([
                        (x * 7 + y * 3) as u8,
                        (x + y * 11) as u8,
                        91,
                        [0, 1, 128, 255][(x + y) as usize % 4],
                    ]),
                );
            }
        }
        let source = |x: i32, y: i32| {
            let p = doc
                .raster()
                .sample(x.clamp(0, 258) as u32, y.clamp(0, 258) as u32)
                .0;
            [
                f64::from(p[0]) * f64::from(p[3]) / 255.0,
                f64::from(p[1]) * f64::from(p[3]) / 255.0,
                f64::from(p[2]) * f64::from(p[3]) / 255.0,
                f64::from(p[3]),
            ]
        };
        for filter in [
            Filter::MotionBlur {
                radius: 7,
                angle: 31.0,
            },
            Filter::Emboss {
                radius: 4,
                angle: -41.0,
                amount: 1.3,
            },
            Filter::Pixelate { size: 37 },
            Filter::Pixelate { size: 36 },
        ] {
            let job = run(&doc, filter.clone());
            for y in [0, 1, 254, 255, 256, 257, 258] {
                for x in [0, 1, 254, 255, 256, 257, 258] {
                    let mut expected = [0.0; 4];
                    match filter {
                        Filter::MotionBlur { radius, angle } => {
                            let a = f64::from(angle).to_radians();
                            for k in -(radius as i32)..=radius as i32 {
                                let p = source(
                                    x + (f64::from(k) * a.cos()).round() as i32,
                                    y + (f64::from(k) * a.sin()).round() as i32,
                                );
                                for c in 0..4 {
                                    expected[c] += p[c] / f64::from(2 * radius + 1);
                                }
                            }
                        }
                        Filter::Pixelate { size } => {
                            let bx = x / size as i32 * size as i32;
                            let by = y / size as i32 * size as i32;
                            let ex = (bx + size as i32).min(259);
                            let ey = (by + size as i32).min(259);
                            for sy in by..ey {
                                for sx in bx..ex {
                                    let p = source(sx, sy);
                                    for c in 0..4 {
                                        expected[c] += p[c] / f64::from((ex - bx) * (ey - by));
                                    }
                                }
                            }
                        }
                        Filter::Emboss {
                            radius,
                            angle,
                            amount,
                        } => {
                            let a = f64::from(angle).to_radians();
                            let dx = (f64::from(radius) * a.cos()).round() as i32;
                            let dy = (f64::from(radius) * a.sin()).round() as i32;
                            let a = source(x + dx, y + dy);
                            let b = source(x - dx, y - dy);
                            expected = source(x, y);
                            for c in 0..3 {
                                let sa = if a[3] > 0.0 { a[c] * 255.0 / a[3] } else { 0.0 };
                                let sb = if b[3] > 0.0 { b[c] * 255.0 / b[3] } else { 0.0 };
                                expected[c] = (128.0 + f64::from(amount) * (sa - sb) * 0.5)
                                    .clamp(0.0, 255.0)
                                    * expected[3]
                                    / 255.0;
                            }
                        }
                        _ => unreachable!(),
                    }
                    let original = doc.raster().sample(x as u32, y as u32);
                    let alpha = (expected[3] + 0.5) as u8;
                    let mut pixel = original;
                    pixel.0[3] = alpha;
                    if alpha > 0 {
                        for c in 0..3 {
                            pixel.0[c] = (expected[c] * 255.0 / expected[3] + 0.5) as u8;
                        }
                    }
                    let actual = job.candidate.raster().sample(x as u32, y as u32);
                    for c in 0..4 {
                        assert!(
                            pixel.0[c].abs_diff(actual.0[c]) <= 1,
                            "{filter:?} {x},{y}: {pixel:?} {actual:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn noise_hash_vectors_seed_repeatability_and_high_pass_flat_fields() {
        assert_eq!(offset(1, 60.0), (1, 1));
        assert_eq!(offset(1, -60.0), (1, -1));
        assert_eq!(hash(0), 0);
        assert_eq!(hash(1), 0x688990c0);
        let expected = ((0x688990u32 as f32 + 0.5) / 16777216.0) * 2.0 - 1.0;
        assert_eq!(noise(0, 0, 1, 0, NoiseDistribution::Uniform), expected);
        let mut doc = Document::blank(513, 3, 72.0).unwrap();
        for x in 0..513 {
            for y in 0..3 {
                doc.raster_mut().write(x, y, Rgba8([87, 33, 19, 128]));
            }
        }
        let high = run(&doc, Filter::HighPass { radius: 20 });
        for x in [0, 255, 256, 512] {
            assert_eq!(
                high.candidate.raster().sample(x, 1),
                Rgba8([128, 128, 128, 128])
            );
        }
        for distribution in [NoiseDistribution::Uniform, NoiseDistribution::Gaussian] {
            let f = Filter::AddNoise {
                amount: 0.4,
                seed: 1,
                monochrome: false,
                distribution,
            };
            let a = run(&doc, f.clone());
            let b = run(&doc, f);
            let other = run(
                &doc,
                Filter::AddNoise {
                    amount: 0.4,
                    seed: 2,
                    monochrome: false,
                    distribution,
                },
            );
            assert_eq!(
                a.candidate.raster().sample(256, 1),
                b.candidate.raster().sample(256, 1)
            );
            assert_ne!(
                a.candidate.raster().sample(256, 1),
                other.candidate.raster().sample(256, 1)
            );
        }
    }
    #[test]
    fn remaining_filters_selection_hidden_rgb_locks_budget_and_exact_history() {
        let mut doc = Document::blank(513, 3, 72.0).unwrap();
        for x in 0..513 {
            for y in 0..3 {
                doc.raster_mut().write(
                    x,
                    y,
                    Rgba8([x as u8, (x * 7) as u8, 91, [0, 1, 128, 255][x as usize % 4]]),
                );
            }
        }
        let mut mask = TileSurface::new(513, 3);
        mask.write(255, 1, Gray8(128));
        mask.write(256, 1, Gray8(255));
        doc.selection = Some(std::sync::Arc::new(mask));
        for filter in controls() {
            let mut before = doc.clone();
            let mut history = History::default();
            let job = run(&doc, filter.clone());
            let command = job.finish().unwrap();
            history.execute(&mut before, Box::new(command), 0).unwrap();
            assert_eq!(before.raster().sample(254, 1), doc.raster().sample(254, 1));
            assert_eq!(before.raster().sample(257, 1), doc.raster().sample(257, 1));
            if before.raster().sample(256, 1).0[3] == 0 {
                assert_eq!(before.raster().sample(256, 1), doc.raster().sample(256, 1));
            }
            assert!(history.undo(&mut before));
            for x in 0..513 {
                assert_eq!(before.raster().sample(x, 1), doc.raster().sample(x, 1));
            }
            assert!(history.redo(&mut before));
            assert_eq!(
                before.raster().sample(255, 1),
                job.candidate.raster().sample(255, 1)
            );
            assert!(
                FilterJob::new(&doc, doc.layers[0].id, filter.clone(), FILTER_SCRATCH - 1).is_err()
            );
            let mut locked = doc.clone();
            locked.layers[0].properties.locks.pixels = true;
            assert!(
                FilterJob::new(&locked, locked.layers[0].id, filter, 100 * 1024 * 1024).is_err()
            );
        }
        for filter in [
            Filter::Pixelate { size: 0 },
            Filter::Pixelate { size: 65 },
            Filter::MotionBlur {
                radius: 1,
                angle: f32::NAN,
            },
            Filter::AddNoise {
                amount: 2.0,
                seed: 0,
                monochrome: true,
                distribution: NoiseDistribution::Uniform,
            },
        ] {
            assert!(filter.validate().is_err());
        }
    }
    #[test]
    fn remaining_identity_controls_and_alpha_lock_are_exact() {
        let mut doc = Document::blank(259, 3, 72.0).unwrap();
        for x in 0..259 {
            for y in 0..3 {
                doc.raster_mut().write(
                    x,
                    y,
                    Rgba8([x as u8, 33, 91, [0, 1, 128, 255][x as usize % 4]]),
                );
            }
        }
        for filter in [
            Filter::MotionBlur {
                radius: 0,
                angle: 137.0,
            },
            Filter::HighPass { radius: 0 },
            Filter::AddNoise {
                amount: 0.0,
                seed: 123,
                monochrome: false,
                distribution: NoiseDistribution::Gaussian,
            },
            Filter::Emboss {
                radius: 0,
                angle: -41.0,
                amount: 3.0,
            },
            Filter::Emboss {
                radius: 5,
                angle: -41.0,
                amount: 0.0,
            },
            Filter::Pixelate { size: 1 },
        ] {
            let job = run(&doc, filter);
            for x in 0..259 {
                assert_eq!(
                    job.candidate.raster().sample(x, 1),
                    doc.raster().sample(x, 1)
                );
            }
            assert!(job.finish().unwrap().is_empty());
        }
        doc.layers[0].properties.locks.alpha = true;
        for filter in controls() {
            let job = run(&doc, filter);
            for x in 0..259 {
                let before = doc.raster().sample(x, 1);
                let after = job.candidate.raster().sample(x, 1);
                assert_eq!(after.0[3], before.0[3]);
                if before.0[3] == 0 {
                    assert_eq!(after, before);
                }
            }
        }
    }
}
