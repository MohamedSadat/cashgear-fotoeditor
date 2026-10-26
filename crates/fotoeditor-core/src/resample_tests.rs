use crate::*;
use std::sync::Arc;

fn run<P: ResamplePixel>(mut job: SurfaceResampleJob<P>) -> TileSurface<P> {
    for _ in 0..1_000_000 {
        if job.step().unwrap() {
            return job.finish().unwrap();
        }
    }
    panic!("resampling failed to finish");
}
fn fixture(w: u32, h: u32) -> Arc<TileSurface<Rgba8>> {
    let mut s = TileSurface::new(w, h);
    for ty in 0..h.div_ceil(TILE_SIZE) {
        for tx in 0..w.div_ceil(TILE_SIZE) {
            let mut tile = vec![Rgba8::default(); TILE_PIXELS];
            for y in 0..TILE_SIZE.min(h - ty * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(w - tx * TILE_SIZE) {
                    let (xx, yy) = (tx * TILE_SIZE + x, ty * TILE_SIZE + y);
                    tile[(y * TILE_SIZE + x) as usize] = Rgba8([
                        (xx * 37 + yy * 3) as u8,
                        (xx * 11 + yy * 17) as u8,
                        (xx * 7 + yy * 51) as u8,
                        ((xx + yy * 3) % 5 * 63) as u8,
                    ]);
                }
            }
            s.insert((tx, ty), tile);
        }
    }
    Arc::new(s)
}
#[test]
fn identity_is_cow_and_permutations_preserve_straight_samples() {
    let source = fixture(513, 259);
    for mode in [
        Interpolation::Nearest,
        Interpolation::Bilinear,
        Interpolation::Bicubic,
        Interpolation::Lanczos3,
    ] {
        let job = SurfaceResampleJob::new(
            source.clone(),
            513,
            259,
            TransformMatrix::IDENTITY,
            mode,
            false,
            0,
        )
        .unwrap();
        assert!(job.identity && job.done());
        assert_eq!(job.temporary_bytes(), 0);
        let result = job.finish().unwrap();
        for (c, t) in source.tiles() {
            assert_eq!(result.tiles()[c].id(), t.id());
        }
    }
    let matrix =
        TransformMatrix::rotation(90.0, [0.0, 0.0]).then(TransformMatrix::translation(259.0, 0.0));
    let result = run(SurfaceResampleJob::new(
        source.clone(),
        259,
        513,
        matrix,
        Interpolation::Lanczos3,
        false,
        CPU_BUDGET,
    )
    .unwrap());
    for y in 0..259 {
        for x in 0..513 {
            assert_eq!(source.sample(x, y), result.sample(258 - y, x), "{x},{y}");
        }
    }
    let matrix = TransformMatrix::scale(-1.0, 1.0).then(TransformMatrix::translation(513.0, 0.0));
    let result = run(SurfaceResampleJob::new(
        source.clone(),
        513,
        259,
        matrix,
        Interpolation::Lanczos3,
        false,
        CPU_BUDGET,
    )
    .unwrap());
    for y in 0..259 {
        for x in 0..513 {
            assert_eq!(source.sample(x, y), result.sample(512 - x, y));
        }
    }
}
#[test]
fn translated_exact_samples_finish_in_tile_batches_despite_inverse_roundoff() {
    let source = fixture(513, 259);
    for scale in [1.0, 7.0, 0.125] {
        let matrix = TransformMatrix {
            values: TransformMatrix::translation(20.0, 12.0)
                .values
                .map(|v| v * scale),
        };
        let mut job = SurfaceResampleJob::new(
            source.clone(),
            540,
            300,
            matrix,
            Interpolation::Lanczos3,
            false,
            CPU_BUDGET,
        )
        .unwrap();
        let mut steps = 0;
        loop {
            let mut dependencies = 0;
            job.needed(&mut |_| dependencies += 1);
            assert!(dependencies <= 4);
            steps += 1;
            assert!(
                steps <= 6,
                "an integer translation must copy one tile per step"
            );
            if job.step().unwrap() {
                break;
            }
        }
        let out = job.finish().unwrap();
        for y in 0..259 {
            for x in 0..513 {
                assert_eq!(source.sample(x, y), out.sample(x + 20, y + 12));
            }
        }
        assert_eq!(out.sample(19, 12), Rgba8::default());
    }
}
#[test]
fn axis_blocks_match_full_kernel_across_odd_tile_and_block_boundaries() {
    let source = fixture(779, 531);
    for mode in [
        Interpolation::Nearest,
        Interpolation::Bilinear,
        Interpolation::Bicubic,
        Interpolation::Lanczos3,
    ] {
        let matrix = TransformMatrix::scale(389.0 / 779.0, 267.0 / 531.0);
        let fast =
            run(
                SurfaceResampleJob::new(source.clone(), 389, 267, matrix, mode, true, CPU_BUDGET)
                    .unwrap(),
            );
        // A tiny non-axis displacement forces the independent generic path.
        let generic = run(SurfaceResampleJob::new(
            source.clone(),
            389,
            267,
            matrix.then(TransformMatrix::translation(1e-9, 0.0)),
            mode,
            true,
            CPU_BUDGET,
        )
        .unwrap());
        let mut worst = [0_u8; 4];
        for y in 0..267 {
            for x in 0..389 {
                let a = fast.sample(x, y).0;
                let b = generic.sample(x, y).0;
                for c in 0..4 {
                    worst[c] = worst[c].max(a[c].abs_diff(b[c]));
                }
            }
        }
        assert!(worst.into_iter().all(|v| v <= 2), "{mode:?}: {worst:?}");
    }
}
#[test]
fn premultiplied_filtering_avoids_hidden_color_bleed_and_retains_zero_alpha_rgb() {
    let mut s = TileSurface::new(4, 2);
    for y in 0..2 {
        for x in 0..4 {
            s.write(
                x,
                y,
                if x < 2 {
                    Rgba8([255, 0, 0, 0])
                } else {
                    Rgba8([0, 120, 240, 255])
                },
            );
        }
    }
    for mode in [
        Interpolation::Bilinear,
        Interpolation::Bicubic,
        Interpolation::Lanczos3,
    ] {
        let source = Arc::new(s.clone());
        let result = run(SurfaceResampleJob::new(
            source,
            2,
            1,
            TransformMatrix::scale(0.5, 0.5),
            mode,
            true,
            CPU_BUDGET,
        )
        .unwrap());
        for x in 0..2 {
            let p = result.sample(x, 0).0;
            assert_eq!(p[0], 0);
            assert!(
                p[1].abs_diff(120) <= 1 && p[2].abs_diff(240) <= 1,
                "{mode:?}: {p:?}"
            );
        }
    }
    let mut s = TileSurface::new(4, 2);
    for y in 0..2 {
        for x in 0..4 {
            s.write(x, y, Rgba8([87 + x as u8, 33, 19, 0]));
        }
    }
    let result = run(SurfaceResampleJob::new(
        Arc::new(s),
        2,
        1,
        TransformMatrix::scale(0.5, 0.5),
        Interpolation::Lanczos3,
        true,
        CPU_BUDGET,
    )
    .unwrap());
    assert_eq!(result.sample(0, 0), Rgba8([88, 33, 19, 0]));
    assert_eq!(result.sample(1, 0), Rgba8([90, 33, 19, 0]));
}
#[test]
fn cold_extreme_reduction_exposes_one_dependency_and_resumes_without_pins() {
    let mut source = TileSurface::with_default(1025, 1025, Gray8(255));
    source.write(700, 800, Gray8(0));
    let handle = source.tiles()[&(2, 3)].clone();
    let samples = handle.pin().unwrap().pixels.clone();
    let mut pager = TilePager::default();
    let backing = TileBacking {
        store: 1,
        offset: 0,
        length: TILE_PIXELS as u32,
        checksum: 0,
    };
    pager.acknowledge_backing(&handle, backing).unwrap();
    pager.evict(&handle).unwrap();
    let source = Arc::new(source);
    let mut job = SurfaceResampleJob::new(
        source.clone(),
        1,
        1,
        TransformMatrix::scale(1.0 / 1025.0, 1.0 / 1025.0),
        Interpolation::Lanczos3,
        true,
        CPU_BUDGET,
    )
    .unwrap();
    let mut found = false;
    let mut steps = 0;
    while !job.done() {
        let mut needed = vec![];
        job.needed(&mut |t| needed.push(t.id()));
        assert!(needed.len() <= 1);
        if needed == [handle.id()] && handle.resident_bytes() == 0 {
            assert!(job.step().is_err());
            pager
                .hydrate(&handle, backing, samples.clone(), Residency::default())
                .unwrap();
            found = true;
        }
        job.step().unwrap();
        steps += 1;
        assert!(steps < 100);
    }
    assert!(found && steps > 1);
    assert_eq!(job.finish().unwrap().sample(0, 0), Gray8(255));
    assert_eq!(source.sample(700, 800), Gray8(0));
    assert_eq!(source.tiles()[&(2, 3)].id(), handle.id());
}
#[test]
fn refusal_and_dropping_partial_candidates_preserve_source_generations() {
    let source = fixture(513, 259);
    let matrix = TransformMatrix::translation(17.0, 9.0);
    assert!(SurfaceResampleJob::new(
        source.clone(),
        513,
        259,
        matrix,
        Interpolation::Lanczos3,
        false,
        RESAMPLE_SCRATCH - 1
    )
    .is_err());
    let mut job = SurfaceResampleJob::new(
        source.clone(),
        513,
        259,
        matrix,
        Interpolation::Lanczos3,
        false,
        CPU_BUDGET,
    )
    .unwrap();
    for _ in 0..4 {
        job.step().unwrap();
    }
    drop(job);
    assert_eq!(
        source.sample(17, 9),
        Rgba8([
            (17 * 37 + 9 * 3) as u8,
            (17 * 11 + 9 * 17) as u8,
            (17 * 7 + 9 * 51) as u8,
            ((17 + 9 * 3) % 5 * 63) as u8
        ])
    );
    let mut job = SurfaceResampleJob::new(
        source.clone(),
        513,
        259,
        matrix,
        Interpolation::Nearest,
        false,
        RESAMPLE_SCRATCH,
    )
    .unwrap();
    loop {
        match job.step() {
            Err(e) => {
                assert!(e.contains("budget"));
                break;
            }
            Ok(false) => {}
            Ok(true) => panic!("candidate must be refused"),
        }
    }
}

#[test]
fn perspective_nearest_and_fractional_bilinear_match_independent_samples() {
    let source = fixture(31, 19);
    let matrix = TransformMatrix::from_quad(
        TransformRect {
            x: 0.0,
            y: 0.0,
            width: 31.0,
            height: 19.0,
        },
        [[3.0, 2.0], [35.0, 4.0], [29.0, 24.0], [1.0, 21.0]],
    )
    .unwrap();
    let result = run(SurfaceResampleJob::new(
        source.clone(),
        39,
        28,
        matrix,
        Interpolation::Nearest,
        false,
        CPU_BUDGET,
    )
    .unwrap());
    let inv = matrix.inverse().unwrap();
    for y in 0..28 {
        for x in 0..39 {
            let p = inv.map([x as f64 + 0.5, y as f64 + 0.5]).unwrap();
            let expected = if p[0] >= 0.0 && p[1] >= 0.0 && p[0] < 31.0 && p[1] < 19.0 {
                source.sample(p[0].floor() as u32, p[1].floor() as u32)
            } else {
                Rgba8::default()
            };
            assert_eq!(result.sample(x, y), expected, "{x},{y}");
        }
    }
    let mut surface = TileSurface::new(8, 6);
    for y in 0..6 {
        for x in 0..8 {
            surface.write(x, y, Rgba8([x as u8 * 20, y as u8 * 30, 150, 128]));
        }
    }
    let source = Arc::new(surface);
    let result = run(SurfaceResampleJob::new(
        source.clone(),
        8,
        6,
        TransformMatrix::translation(0.25, 0.75),
        Interpolation::Bilinear,
        false,
        CPU_BUDGET,
    )
    .unwrap());
    for y in 1..6 {
        for x in 1..8 {
            assert_eq!(
                result.sample(x, y),
                Rgba8([
                    (x as f64 * 20.0 - 5.0) as u8,
                    (y as f64 * 30.0 - 22.5).round() as u8,
                    150,
                    128
                ])
            );
        }
    }
    // Canvas growth has an explicit transparent outside even for a sparse field.
    let gray = Arc::new(TileSurface::with_default(3, 2, Gray8(255)));
    let result = run(SurfaceResampleJob::with_outside(
        gray,
        5,
        4,
        TransformMatrix::translation(1.0, 1.0),
        Interpolation::Lanczos3,
        false,
        CPU_BUDGET,
        Gray8(0),
    )
    .unwrap());
    for y in 0..4 {
        for x in 0..5 {
            assert_eq!(
                result.sample(x, y),
                Gray8(if (1..4).contains(&x) && (1..3).contains(&y) {
                    255
                } else {
                    0
                })
            );
        }
    }
}
