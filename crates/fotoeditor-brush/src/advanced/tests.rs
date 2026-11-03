use super::*;
use fotoeditor_core::{Gray8, History, TileSurface, CPU_BUDGET};
use std::sync::Arc;
fn point(x: f64, y: f64) -> Sample {
    Sample {
        x,
        y,
        pressure: 1.0,
        ..Sample::default()
    }
}
fn settings() -> BrushSettings {
    BrushSettings {
        size: 20.0,
        pressure_size: false,
        ..BrushSettings::default()
    }
}
fn image() -> CoverageImage {
    CoverageImage {
        width: 3,
        height: 2,
        coverage: "ff0000ffff00".into(),
    }
}
fn run(doc: &Document, s: BrushSettings, points: &[Sample], step: usize) -> StrokeCommand {
    let mut stroke = StrokeCommand::new(doc, s, CPU_BUDGET).unwrap();
    for p in points {
        stroke.push(*p).unwrap();
    }
    stroke.end();
    let mut limit = 1000000;
    while !stroke.done() {
        stroke.step(step).unwrap();
        limit -= 1;
        assert!(limit > 0);
    }
    stroke
}
fn pixels(doc: &Document) -> Vec<Rgba8> {
    (0..doc.height)
        .flat_map(|y| (0..doc.width).map(move |x| doc.raster().sample(x, y)))
        .collect()
}
#[test]
fn asset_validation_and_bilinear_oracle() {
    let mask = CoverageImage {
        width: 2,
        height: 2,
        coverage: "ff0000ff".into(),
    };
    mask.validate().unwrap();
    assert_eq!(mask.sample(0.5, 0.5), 0.5);
    assert_eq!(mask.sample(0.0, 0.0), 1.0);
    assert_eq!(mask.sample(-0.51, 0.0), 0.0);
    for mask in [
        CoverageImage {
            width: 513,
            ..mask.clone()
        },
        CoverageImage {
            coverage: "gg0000ff".into(),
            ..mask.clone()
        },
        CoverageImage {
            coverage: "ff".into(),
            ..mask
        },
    ] {
        assert!(mask.validate().is_err());
    }
}
#[test]
fn asymmetric_image_mirrors_and_radial_quarters_match_independent_coordinate_oracle() {
    let doc = Document::blank(160, 160, 300.0).unwrap();
    for mode in [
        SymmetryMode::Vertical,
        SymmetryMode::Horizontal,
        SymmetryMode::Both,
        SymmetryMode::Radial,
    ] {
        let s = BrushSettings {
            tip_image: Some(image()),
            symmetry: Symmetry {
                mode,
                center: [80.0, 80.0],
                sectors: 4,
            },
            ..settings()
        };
        let stroke = run(&doc, s, &[point(40.0, 35.0)], 131072);
        let raster = stroke.preview.raster();
        let mut nonzero = 0;
        for y in 20..50 {
            for x in 25..55 {
                let expected = raster.sample(x, y);
                nonzero += usize::from(expected.0[3] > 0);
                match mode {
                    SymmetryMode::Vertical => assert_eq!(raster.sample(159 - x, y), expected),
                    SymmetryMode::Horizontal => assert_eq!(raster.sample(x, 159 - y), expected),
                    SymmetryMode::Both => {
                        assert_eq!(raster.sample(159 - x, y), expected);
                        assert_eq!(raster.sample(x, 159 - y), expected);
                        assert_eq!(raster.sample(159 - x, 159 - y), expected);
                    }
                    SymmetryMode::Radial => {
                        assert_eq!(raster.sample(159 - y, x), expected);
                        assert_eq!(raster.sample(159 - x, 159 - y), expected);
                        assert_eq!(raster.sample(y, 159 - x), expected);
                    }
                    _ => unreachable!(),
                }
            }
        }
        assert!(nonzero > 30);
    }
}
#[test]
fn fixed_seed_is_independent_of_cooperative_slicing_and_changes_with_seed() {
    let doc = Document::blank(300, 160, 72.0).unwrap();
    let s = BrushSettings {
        tip_image: Some(image()),
        texture: Some(image()),
        texture_scale: 7.0,
        texture_depth: 0.7,
        scatter: 0.9,
        size_jitter: 0.7,
        angle_jitter: 0.8,
        opacity_jitter: 0.6,
        seed: 77,
        symmetry: Symmetry {
            mode: SymmetryMode::Both,
            center: [150.0, 80.0],
            sectors: 6,
        },
        ..settings()
    };
    let path = [
        point(35.5, 40.5),
        Sample {
            time: 100.0,
            ..point(110.5, 55.5)
        },
    ];
    let a = run(&doc, s.clone(), &path, 1);
    let b = run(&doc, s.clone(), &path, 262144);
    assert_eq!(pixels(&a.preview), pixels(&b.preview));
    let c = run(&doc, BrushSettings { seed: 78, ..s }, &path, 333);
    assert_ne!(pixels(&a.preview), pixels(&c.preview));
    assert!(a.temporary_bytes() < 16 * 1024 * 1024);
}
#[test]
fn planned_scatter_and_jitter_are_bounded_and_coincident_copies_deduplicate() {
    let doc = Document::blank(512, 512, 72.0).unwrap();
    let mut stroke = StrokeCommand::new(
        &doc,
        BrushSettings {
            scatter: 2.0,
            size_jitter: 1.0,
            opacity_jitter: 1.0,
            angle_jitter: 1.0,
            ..settings()
        },
        CPU_BUDGET,
    )
    .unwrap();
    for _ in 0..1000 {
        stroke.plan_dabs(point(200.0, 200.0));
        let d = stroke.planned.pop_front().unwrap();
        assert!((d.p.x - 200.0).hypot(d.p.y - 200.0) <= 40.0);
        assert!((0.0..=1.0).contains(&d.size));
        assert!((0.0..=1.0).contains(&d.influence));
        assert!((d.transform[0].hypot(d.transform[1]) - 1.0).abs() < 1e-12);
    }
    let s = BrushSettings {
        flow: 0.3,
        opacity: 0.5,
        symmetry: Symmetry {
            mode: SymmetryMode::Radial,
            center: [200.0, 200.0],
            sectors: 16,
        },
        ..settings()
    };
    let a = run(&doc, s.clone(), &[point(200.0, 200.0)], 262144);
    let b = run(
        &doc,
        BrushSettings {
            symmetry: Symmetry::default(),
            ..s
        },
        &[point(200.0, 200.0)],
        262144,
    );
    assert_eq!(pixels(&a.preview), pixels(&b.preview));
}
#[test]
fn texture_depth_zero_is_identity_and_full_white_texture_is_noop() {
    let doc = Document::blank(60, 60, 72.0).unwrap();
    let base = run(&doc, settings(), &[point(30.0, 30.0)], 512);
    let zero = run(
        &doc,
        BrushSettings {
            texture: Some(image()),
            texture_depth: 0.0,
            ..settings()
        },
        &[point(30.0, 30.0)],
        512,
    );
    assert_eq!(pixels(&base.preview), pixels(&zero.preview));
    let white = run(
        &doc,
        BrushSettings {
            texture: Some(CoverageImage {
                width: 1,
                height: 1,
                coverage: "00".into(),
            }),
            ..settings()
        },
        &[point(30.0, 30.0)],
        512,
    );
    assert_eq!(pixels(&doc), pixels(&white.preview));
}
#[test]
fn symmetry_selection_crop_hidden_rgb_alpha_locks_and_exact_history() {
    let mut doc = Document::blank(520, 40, 300.0).unwrap();
    for x in 0..520 {
        doc.raster_mut()
            .write(x, 20, Rgba8([91, 32, 7, if x % 2 == 0 { 0 } else { 127 }]));
    }
    doc.canvas = Some(fotoeditor_core::CanvasRect {
        x: 10,
        y: 5,
        width: 500,
        height: 30,
    });
    let mut selection = TileSurface::new(520, 40);
    selection.write(255, 20, Gray8(128));
    selection.write(264, 20, Gray8(255));
    doc.selection = Some(Arc::new(selection));
    for eraser in [false, true] {
        for alpha_lock in [false, true] {
            let mut live = doc.clone();
            live.layers[0].properties.locks.alpha = alpha_lock;
            let original = pixels(&live);
            let stroke = run(
                &live,
                BrushSettings {
                    eraser,
                    symmetry: Symmetry {
                        mode: SymmetryMode::Vertical,
                        center: [260.0, 20.0],
                        sectors: 6,
                    },
                    ..settings()
                },
                &[point(255.5, 20.5)],
                7,
            );
            for y in 0..40 {
                for x in 0..520 {
                    if y != 20 || (x != 255 && x != 264) {
                        assert_eq!(
                            stroke.preview.raster().sample(x, y),
                            live.raster().sample(x, y)
                        );
                    }
                }
            }
            if alpha_lock {
                assert_eq!(
                    stroke.preview.raster().sample(264, 20),
                    Rgba8([91, 32, 7, 0])
                );
            }
            let edited = pixels(&stroke.preview);
            let mut history = History::default();
            history
                .execute(&mut live, Box::new(stroke.finish().unwrap()), 0)
                .unwrap();
            if edited != original {
                assert_eq!(history.depths(), (1, 0));
                assert!(history.undo(&mut live));
                assert_eq!(pixels(&live), original);
                assert!(history.redo(&mut live));
                assert_eq!(pixels(&live), edited);
            }
            assert_eq!(live.dpi, 300.0);
            assert_eq!(live.canvas, doc.canvas);
        }
    }
}
#[test]
fn advanced_admission_and_invalid_settings_refuse_before_allocation() {
    let doc = Document::blank(60, 60, 72.0).unwrap();
    let mut s = BrushSettings {
        texture: Some(image()),
        tip_image: Some(image()),
        ..settings()
    };
    assert!(StrokeCommand::new(&doc, s.clone(), 100).is_err());
    s.scatter = f64::NAN;
    assert!(s.validate().is_err());
    s.scatter = 0.0;
    s.symmetry.sectors = 17;
    assert!(s.validate().is_err());
    s.symmetry.sectors = 6;
    s.symmetry.mode = SymmetryMode::Both;
    s.clone = Some(CloneOptions::default());
    assert!(s.validate().is_err());
}

#[test]
fn partial_symmetry_budget_refusal_cancel_and_empty_texture_preserve_redo() {
    let mut doc = Document::blank(1024, 64, 72.0).unwrap();
    let s = BrushSettings {
        tip_image: Some(image()),
        symmetry: Symmetry {
            mode: SymmetryMode::Vertical,
            center: [512.0, 32.0],
            sectors: 6,
        },
        ..settings()
    };
    let mut h = History::default();
    let first = run(&doc, s.clone(), &[point(100.0, 32.0)], 131072);
    h.execute(&mut doc, Box::new(first.finish().unwrap()), 0)
        .unwrap();
    assert!(h.undo(&mut doc));
    let before = pixels(&doc);
    let mut refused = StrokeCommand::new(&doc, s.clone(), CPU_BUDGET).unwrap();
    refused.limit_available(refused.temporary_bytes() + TILE_BYTES * 2);
    refused.push(point(100.0, 32.0)).unwrap();
    refused.end();
    assert!(refused.step(262144).is_err());
    assert!(!refused.coverage.is_empty());
    assert_eq!(pixels(&doc), before);
    assert!(refused.finish().is_err());
    let mut cancelled = StrokeCommand::new(&doc, s.clone(), CPU_BUDGET).unwrap();
    cancelled.push(point(100.0, 32.0)).unwrap();
    cancelled.step(1).unwrap();
    drop(cancelled);
    let noop = run(
        &doc,
        BrushSettings {
            texture: Some(CoverageImage {
                width: 1,
                height: 1,
                coverage: "00".into(),
            }),
            ..s
        },
        &[point(100.0, 32.0)],
        262144,
    );
    assert!(!h
        .execute(&mut doc, Box::new(noop.finish().unwrap()), 0)
        .unwrap());
    assert_eq!(h.depths(), (0, 1));
    assert_eq!(pixels(&doc), before);
    assert!(h.redo(&mut doc));
}

#[test]
fn image_clone_hydrates_only_jittered_dab_source_and_keeps_snapshot_immutable() {
    use fotoeditor_core::{Residency, TileBacking, TileHandle, TilePager};
    let doc = Document::blank(16384, 16384, 72.0).unwrap();
    let mut source = doc.clone();
    let backing = TileBacking {
        store: 1,
        offset: 0,
        length: TILE_BYTES as u32,
        checksum: 0,
    };
    let needed = TileHandle::paged(backing).unwrap();
    let unrelated = TileHandle::paged(TileBacking {
        offset: TILE_BYTES,
        ..backing
    })
    .unwrap();
    source
        .raster_mut()
        .insert_handle((1, 0), needed.clone())
        .unwrap();
    source
        .raster_mut()
        .insert_handle((60, 60), unrelated.clone())
        .unwrap();
    let s = BrushSettings {
        tip_image: Some(image()),
        scatter: 0.1,
        angle_jitter: 0.4,
        clone: Some(CloneOptions::default()),
        ..settings()
    };
    let mut j =
        StrokeCommand::for_clone(&doc, s, CPU_BUDGET, doc.layers[0].id, source, [256, 0]).unwrap();
    j.push(point(40.0, 32.0)).unwrap();
    j.end();
    j.prepare_clone();
    let mut ids = Vec::new();
    j.clone_needed(&mut |t| ids.push(t.id()));
    assert_eq!(ids, vec![needed.id()]);
    j.step(262144).unwrap();
    assert_eq!(j.preview.tile_count(), 0);
    TilePager::default()
        .hydrate(
            &needed,
            backing,
            vec![Rgba8([70, 100, 190, 255]); TILE_PIXELS],
            Residency::default(),
        )
        .unwrap();
    while !j.done() {
        j.step(31).unwrap();
    }
    assert!(j.preview.tile_count() > 0);
    assert_eq!(unrelated.resident_bytes(), 0);
    assert_eq!(
        j.sampled_source.as_ref().unwrap().raster().sample(296, 32),
        Rgba8([70, 100, 190, 255])
    );
    assert_eq!(doc.tile_count(), 0);
}
