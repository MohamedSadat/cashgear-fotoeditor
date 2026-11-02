use super::*;
use fotoeditor_core::{CanvasRect, History, TileSurface, CPU_BUDGET};
use std::sync::Arc;
#[test]
fn cold_halo_and_selection_pause_without_allocating_and_unrelated_pages_stay_cold() {
    use fotoeditor_core::{Residency, TileBacking, TileHandle, TilePager};
    let mut d = Document::blank(16384, 16384, 72.0).unwrap();
    let backing = TileBacking {
        store: 1,
        offset: 0,
        length: (TILE_PIXELS * 4) as u32,
        checksum: 0,
    };
    let halo = TileHandle::paged(backing).unwrap();
    let unrelated = TileHandle::paged(TileBacking {
        offset: u64::from(backing.length),
        ..backing
    })
    .unwrap();
    d.raster_mut().insert_handle((0, 0), halo.clone()).unwrap();
    d.raster_mut()
        .insert_handle((60, 60), unrelated.clone())
        .unwrap();
    let mut j = StrokeCommand::new(&d, settings(RetouchMode::Blur), CPU_BUDGET).unwrap();
    j.push(Sample {
        x: 256.5,
        y: 1.5,
        pressure: 1.0,
        ..Default::default()
    })
    .unwrap();
    j.end();
    j.prepare_clone();
    let mut ids = Vec::new();
    j.clone_needed(&mut |t| ids.push(t.id()));
    ids.sort();
    ids.dedup();
    assert_eq!(ids, vec![halo.id()]);
    j.step(1000).unwrap();
    assert_eq!(j.coverage.len(), 0);
    assert!(!j.done());
    let mut pager = TilePager::default();
    pager
        .hydrate(
            &halo,
            backing,
            vec![Rgba8([200, 100, 50, 255]); TILE_PIXELS],
            Residency::default(),
        )
        .unwrap();
    // Only the edge adjacent to the blank tile can change; the halo remains loaded.
    for _ in 0..1000 {
        j.step(1000).unwrap();
        if j.done() {
            break;
        }
    }
    assert!(j.done());
    assert_eq!(unrelated.resident_bytes(), 0);
}
fn options(mode: RetouchMode) -> RetouchOptions {
    RetouchOptions {
        mode,
        strength: 0.5,
        range: TonalRange::All,
        saturate: false,
    }
}
fn settings(mode: RetouchMode) -> BrushSettings {
    BrushSettings {
        size: 5.0,
        pressure_size: false,
        spacing: 1.0,
        retouch: Some(options(mode)),
        ..Default::default()
    }
}
fn run(d: &Document, s: BrushSettings, points: &[[f64; 2]], step: usize) -> StrokeCommand {
    let mut j = StrokeCommand::new(d, s, CPU_BUDGET).unwrap();
    for (i, p) in points.iter().enumerate() {
        j.push(Sample {
            x: p[0],
            y: p[1],
            pressure: 1.0,
            time: i as f64 + 1.0,
            ..Default::default()
        })
        .unwrap();
    }
    j.end();
    for _ in 0..100000 {
        j.step(step).unwrap();
        if j.done() {
            return j;
        }
    }
    panic!("unbounded stroke");
}
fn pixels(d: &Document) -> Vec<Rgba8> {
    (0..d.height)
        .flat_map(|y| (0..d.width).map(move |x| d.raster().sample(x, y)))
        .collect()
}
#[test]
fn binomial_oracle_premultiplied_alpha_and_crop_clamping() {
    let mut d = Document::blank(258, 3, 72.0).unwrap();
    for y in 0..3 {
        for x in 0..258 {
            d.raster_mut().write(x, y, Rgba8([40, 80, 120, 255]));
        }
    }
    d.raster_mut().write(256, 1, Rgba8([200, 160, 120, 255]));
    let (c, _) = target(
        &d,
        256,
        1,
        d.raster().sample(256, 1),
        options(RetouchMode::Blur),
        [0, 0],
    )
    .unwrap();
    assert_eq!(c, [80.0, 100.0, 120.0]); // 4/16 center, 12/16 neighbors.
    d.raster_mut().write(255, 1, Rgba8([255, 0, 0, 0]));
    let (c, _) = target(
        &d,
        256,
        1,
        d.raster().sample(256, 1),
        options(RetouchMode::Blur),
        [0, 0],
    )
    .unwrap();
    assert!((c[0] - 600.0 / 7.0).abs() < 1e-10);
    d.canvas = Some(CanvasRect {
        x: 256,
        y: 0,
        width: 2,
        height: 3,
    });
    let before = target(
        &d,
        256,
        1,
        d.raster().sample(256, 1),
        options(RetouchMode::Blur),
        [0, 0],
    )
    .unwrap();
    d.raster_mut().write(255, 1, Rgba8([0, 255, 0, 255]));
    assert_eq!(
        before,
        target(
            &d,
            256,
            1,
            d.raster().sample(256, 1),
            options(RetouchMode::Blur),
            [0, 0]
        )
        .unwrap()
    );
}
#[test]
fn tonal_and_sponge_oracles_and_range_weighting() {
    let mut d = Document::blank(1, 1, 72.0).unwrap();
    let p = Rgba8([200, 100, 50, 128]);
    d.raster_mut().write(0, 0, p);
    assert_eq!(
        target(&d, 0, 0, p, options(RetouchMode::Dodge), [0, 0])
            .unwrap()
            .0,
        [255.0; 3]
    );
    assert_eq!(
        target(&d, 0, 0, p, options(RetouchMode::Burn), [0, 0])
            .unwrap()
            .0,
        [0.0; 3]
    );
    let c = target(&d, 0, 0, p, options(RetouchMode::Sponge), [0, 0])
        .unwrap()
        .0;
    assert!((c[0] - 117.65).abs() < 0.01);
    assert_eq!(c[0], c[1]);
    let mut o = options(RetouchMode::Dodge);
    o.range = TonalRange::Shadows;
    let dark = target(&d, 0, 0, Rgba8([30; 4]), o, [0, 0]).unwrap().0[0] - 30.0;
    let light = target(&d, 0, 0, Rgba8([220; 4]), o, [0, 0]).unwrap().0[0] - 220.0;
    assert!(dark > light);
}
#[test]
fn every_mode_selection_hidden_rgb_tile_edges_and_exact_undo_redo() {
    for mode in [
        RetouchMode::Smudge,
        RetouchMode::Blur,
        RetouchMode::Sharpen,
        RetouchMode::Dodge,
        RetouchMode::Burn,
        RetouchMode::Sponge,
    ] {
        let mut d = Document::blank(264, 8, 300.0).unwrap();
        for y in 0..8 {
            for x in 0..264 {
                d.raster_mut().write(
                    x,
                    y,
                    Rgba8([if x % 2 == 0 { 80 } else { 160 }, 100, 40, 128]),
                );
            }
        }
        d.raster_mut().write(257, 4, Rgba8([87, 33, 19, 0]));
        let mut mask = TileSurface::new(264, 8);
        for y in 0..8 {
            for x in 256..264 {
                mask.write(x, y, Gray8(if x == 256 { 128 } else { 255 }));
            }
        }
        d.selection = Some(Arc::new(mask));
        let before = pixels(&d);
        let j = run(&d, settings(mode), &[[255.5, 4.5], [261.5, 4.5]], 17);
        let after = pixels(&j.preview);
        assert_ne!(after, before, "{mode:?}");
        for y in 0..8 {
            for x in 0..256 {
                assert_eq!(j.preview.raster().sample(x, y), d.raster().sample(x, y));
            }
        }
        assert_eq!(j.preview.raster().sample(257, 4), Rgba8([87, 33, 19, 0]));
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.0[3], b.0[3]);
        }
        let mut h = History::default();
        h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap();
        assert_eq!(h.labels(), vec![mode.label()]);
        assert!(h.undo(&mut d));
        assert_eq!(pixels(&d), before);
        assert!(h.redo(&mut d));
        assert_eq!(pixels(&d), after);
    }
}
#[test]
fn smudge_carries_color_cumulatively_and_step_size_does_not_change_samples() {
    let mut d = Document::blank(20, 3, 72.0).unwrap();
    for x in 0..20 {
        for y in 0..3 {
            d.raster_mut()
                .write(x, y, Rgba8([if x < 5 { 240 } else { 0 }, 0, 0, 255]));
        }
    }
    let mut s = settings(RetouchMode::Smudge);
    s.size = 3.0;
    s.spacing = 0.34;
    s.retouch.as_mut().unwrap().strength = 1.0;
    let a = run(&d, s.clone(), &[[3.5, 1.5], [14.5, 1.5]], 1);
    let b = run(&d, s.clone(), &[[3.5, 1.5], [14.5, 1.5]], 262144);
    assert_eq!(pixels(&a.preview), pixels(&b.preview));
    assert!(a.preview.raster().sample(12, 1).0[0] > 0);
    assert_eq!(pixels(&run(&d, s, &[[3.5, 1.5]], 64).preview), pixels(&d));
}
#[test]
fn fractional_selection_applies_once_and_repeated_dabs_accumulate() {
    let mut d = Document::blank(5, 5, 72.0).unwrap();
    d.raster_mut().write(2, 2, Rgba8([100, 100, 100, 255]));
    d.selection = Some(Arc::new(TileSurface::with_default(5, 5, Gray8(128))));
    let one = run(&d, settings(RetouchMode::Dodge), &[[2.5, 2.5]], 64);
    assert_eq!(
        one.preview.raster().sample(2, 2),
        Rgba8([139, 139, 139, 255])
    );
    let two = run(
        &d,
        settings(RetouchMode::Dodge),
        &[[2.5, 2.5], [2.5, 2.5]],
        64,
    );
    assert_eq!(
        two.preview.raster().sample(2, 2),
        Rgba8([168, 168, 168, 255])
    );
}
#[test]
fn no_op_refusal_cancel_and_bounded_source_requests_preserve_redo() {
    let mut d = Document::blank(520, 3, 72.0).unwrap();
    d.raster_mut().write(256, 1, Rgba8([80, 50, 20, 255]));
    let j = run(&d, settings(RetouchMode::Dodge), &[[256.5, 1.5]], 64);
    let mut h = History::default();
    h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap();
    h.undo(&mut d);
    let before = pixels(&d);
    let mut s = settings(RetouchMode::Dodge);
    s.retouch.as_mut().unwrap().strength = 0.0;
    let j = run(&d, s, &[[256.5, 1.5]], 64);
    assert!(!h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap());
    assert_eq!(h.depths(), (0, 1));
    assert!(StrokeCommand::new(&d, settings(RetouchMode::Blur), 0).is_err());
    let mut j = StrokeCommand::new(&d, settings(RetouchMode::Blur), CPU_BUDGET).unwrap();
    j.push(Sample {
        x: 256.5,
        y: 1.5,
        pressure: 1.0,
        ..Default::default()
    })
    .unwrap();
    j.prepare_clone();
    let mut ids = Vec::new();
    j.clone_needed(&mut |t| ids.push(t.id()));
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 1);
    j.step(1).unwrap();
    drop(j);
    assert_eq!(pixels(&d), before);
    assert_eq!(h.depths(), (0, 1));
    d.layers[0].properties.locks.pixels = true;
    assert!(StrokeCommand::new(&d, settings(RetouchMode::Blur), CPU_BUDGET).is_err());
}
