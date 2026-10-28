use super::*;
use fotoeditor_core::{CanvasRect, Gray8, History, TileBacking, TileHandle, CPU_BUDGET};
use std::sync::Arc;

fn point(x: f64, y: f64) -> Sample {
    Sample {
        x,
        y,
        pressure: 1.0,
        time: 1.0,
        ..Default::default()
    }
}
fn settings(spot: bool, size: f64) -> BrushSettings {
    BrushSettings {
        size,
        hardness: 1.0,
        pressure_size: false,
        healing: Some(HealingOptions {
            spot,
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn stroke(
    doc: &Document,
    source: Document,
    spot: bool,
    size: f64,
    offset: [i32; 2],
) -> StrokeCommand {
    StrokeCommand::for_clone(
        doc,
        settings(spot, size),
        CPU_BUDGET,
        doc.layers[0].id,
        source,
        offset,
    )
    .unwrap()
}
fn finish(s: &mut StrokeCommand, chunk: usize) {
    s.end();
    for _ in 0..200_000 {
        s.step(chunk).unwrap();
        if s.done() {
            return;
        }
    }
    panic!("healing did not finish");
}
fn pixels(doc: &Document) -> Vec<Rgba8> {
    (0..doc.height)
        .flat_map(|y| (0..doc.width).map(move |x| doc.raster().sample(x, y)))
        .collect()
}
fn photo() -> Document {
    let mut d = Document::blank(520, 96, 300.0).unwrap();
    for y in 0..96 {
        for x in 0..520 {
            let texture = ((x % 8) * (y % 8) % 7) as u8;
            d.raster_mut().write(
                x,
                y,
                Rgba8([120 + texture, 90 + y as u8 / 4, 60 + texture, 255]),
            );
        }
    }
    for y in 44..53 {
        for x in 252..261 {
            d.raster_mut().write(x, y, Rgba8([200, 20, 30, 255]));
        }
    }
    d
}

#[test]
fn poisson_uses_source_gradients_and_destination_tone_instead_of_copying_source_color() {
    let mut doc = Document::blank(128, 64, 72.0).unwrap();
    for y in 0..64 {
        for x in 0..128 {
            doc.raster_mut().write(
                x,
                y,
                Rgba8(if x < 64 {
                    [20, 60, 200, 255]
                } else {
                    [170, 110, 80, 255]
                }),
            );
        }
    }
    for y in 29..36 {
        for x in 93..100 {
            doc.raster_mut().write(x, y, Rgba8([220, 30, 40, 255]));
        }
    }
    let mut s = stroke(&doc, doc.clone(), false, 24.0, [-64, 0]);
    s.push(point(96.5, 32.5)).unwrap();
    finish(&mut s, 262144);
    for y in 28..37 {
        for x in 92..101 {
            assert_eq!(s.preview.raster().sample(x, y), Rgba8([170, 110, 80, 255]));
        }
    }
}

#[test]
fn unsuitable_spot_source_and_conflicting_tool_options_are_atomic_errors() {
    let doc = Document::blank(64, 64, 72.0).unwrap();
    let mut s = stroke(&doc, doc.clone(), true, 16.0, [0, 0]);
    s.push(point(32.5, 32.5)).unwrap();
    s.end();
    let before = pixels(&doc);
    let mut failure = false;
    for _ in 0..1000 {
        if s.step(262144).is_err() {
            failure = true;
            break;
        }
    }
    assert!(failure);
    assert_eq!(pixels(&doc), before);
    assert_eq!(pixels(&s.preview), before);
    let mut both = settings(false, 16.0);
    both.clone = Some(CloneOptions::default());
    assert!(StrokeCommand::for_clone(
        &doc,
        both,
        CPU_BUDGET,
        doc.layers[0].id,
        doc.clone(),
        [0, 0]
    )
    .is_err());
}

#[test]
fn overlapping_healing_paths_keep_one_immutable_source_and_one_exact_command() {
    for spot in [false, true] {
        let mut doc = photo();
        let before = pixels(&doc);
        let mut s = stroke(&doc, doc.clone(), spot, 24.0, [-64, 0]);
        for (i, x) in [244.5, 256.5, 268.5].into_iter().enumerate() {
            s.push(Sample {
                time: i as f64 + 1.0,
                ..point(x, 48.5)
            })
            .unwrap();
        }
        finish(&mut s, 262144);
        assert_eq!(pixels(s.sampled_source.as_ref().unwrap()), before);
        let after = pixels(&s.preview);
        assert_ne!(after, before);
        let mut h = History::default();
        h.execute(&mut doc, Box::new(s.finish().unwrap()), 0)
            .unwrap();
        assert_eq!(h.depths(), (1, 0));
        assert!(h.undo(&mut doc));
        assert_eq!(pixels(&doc), before);
        assert!(h.redo(&mut doc));
        assert_eq!(pixels(&doc), after);
    }
}

#[test]
fn healed_alpha_flow_opacity_and_fractional_selection_match_independent_source_over() {
    for source_alpha in [0, 1, 128, 255] {
        for locked in [false, true] {
            let mut doc = Document::blank(128, 64, 72.0).unwrap();
            for y in 0..64 {
                for x in 0..128 {
                    doc.raster_mut().write(
                        x,
                        y,
                        Rgba8([160, 120, 90, if x < 64 { source_alpha } else { 128 }]),
                    );
                }
            }
            let before = Rgba8([200, 40, 70, 128]);
            doc.raster_mut().write(96, 32, before);
            doc.layers[0].properties.locks.alpha = locked;
            doc.selection = Some(Arc::new(TileSurface::with_default(128, 64, Gray8(128))));
            let mut options = settings(false, 16.0);
            options.opacity = 0.5;
            options.flow = 0.5;
            let mut source = doc.clone();
            source.layers[0].properties = Default::default();
            source.selection = None;
            let mut s = StrokeCommand::for_clone(
                &doc,
                options,
                CPU_BUDGET,
                doc.layers[0].id,
                source,
                [-64, 0],
            )
            .unwrap();
            for i in 1..=3 {
                s.push(Sample {
                    time: f64::from(i),
                    ..point(96.5, 32.5)
                })
                .unwrap();
            }
            finish(&mut s, 262144);
            let coverage = 0.5 * (1.0 - 0.5_f64.powi(3)) * 128.0 / 255.0;
            let expected = if locked {
                let amount = coverage * f64::from(source_alpha) / 255.0;
                Rgba8([
                    ((200.0 - 40.0 * amount).round()) as u8,
                    ((40.0 + 80.0 * amount).round()) as u8,
                    ((70.0 + 20.0 * amount).round()) as u8,
                    128,
                ])
            } else {
                composite(before, [160, 120, 90, source_alpha], coverage, false)
            };
            let actual = s.preview.raster().sample(96, 32);
            for c in 0..4 {
                assert!(
                    actual.0[c].abs_diff(expected.0[c]) <= 1,
                    "alpha {source_alpha} lock {locked}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
}

#[test]
fn poisson_matches_independent_dense_gaussian_elimination_on_nontrivial_rgb_domain() {
    let mut doc = Document::blank(40, 24, 72.0).unwrap();
    for y in 0..24 {
        for x in 0..40 {
            doc.raster_mut().write(
                x,
                y,
                Rgba8([
                    40 + ((x * 13 + y * 7) % 130) as u8,
                    60 + ((x * y + 31) % 120) as u8,
                    80 + ((x * 3 + y * 11) % 100) as u8,
                    255,
                ]),
            );
        }
    }
    let options = settings(false, 7.0);
    let tip = Tip::new(&options);
    let dab = Dab {
        p: point(25.5, 12.5),
        radius: 3.5,
        influence: 1.0,
        x0: 22,
        x1: 29,
        y1: 16,
        x: 22,
        y: 9,
    };
    let mut patch = HealingPatch::new(&dab, false, [-15, 0], CPU_BUDGET).unwrap();
    while patch.stage != Stage::Initialize {
        patch.step(&doc, &dab, &tip, 13).unwrap();
    }
    let ids: Vec<_> = patch
        .domain
        .iter()
        .enumerate()
        .filter_map(|(i, v)| (*v != 0).then_some(i))
        .collect();
    assert!(ids.len() > 15);
    let n = ids.len();
    let mut a = vec![vec![0.0; n + 3]; n];
    for (row, &i) in ids.iter().enumerate() {
        a[row][row] = 4.0;
        for c in 0..3 {
            a[row][n + c] = 4.0 * f64::from(patch.source[i].0[c]);
        }
        for j in [i - 1, i + 1, i - patch.width, i + patch.width] {
            if let Some(column) = ids.iter().position(|&k| k == j) {
                a[row][column] -= 1.0;
            }
            for c in 0..3 {
                a[row][n + c] -= f64::from(patch.source[j].0[c]);
                if patch.domain[j] == 0 {
                    a[row][n + c] += f64::from(patch.target[j].0[c]);
                }
            }
        }
    }
    // Independent direct elimination, with no reuse of CG operators/vectors.
    for pivot in 0..n {
        let divisor = a[pivot][pivot];
        for value in &mut a[pivot][pivot..] {
            *value /= divisor;
        }
        let pivot_row = a[pivot].clone();
        for (row, values) in a.iter_mut().enumerate() {
            if row != pivot {
                let scale = values[pivot];
                for (value, reference) in values[pivot..].iter_mut().zip(&pivot_row[pivot..]) {
                    *value -= scale * reference;
                }
            }
        }
    }
    for _ in 0..10000 {
        patch.step(&doc, &dab, &tip, 13).unwrap();
        if patch.stage == Stage::Output {
            break;
        }
    }
    assert!(patch.stage == Stage::Output);
    for (row, &i) in ids.iter().enumerate() {
        for c in 0..3 {
            assert!((patch.solution[i][c] - a[row][n + c]).abs() < 0.0001);
        }
    }
    while !patch.done() {
        patch.step(&doc, &dab, &tip, 13).unwrap();
    }
    for (row, &i) in ids.iter().enumerate() {
        for c in 0..3 {
            assert_eq!(
                patch.source[i].0[c],
                a[row][n + c].round().clamp(0.0, 255.0) as u8
            );
        }
    }
    assert_eq!(
        patch.bytes(),
        (patch.source.capacity() * 4 + patch.domain.capacity()) as u64
    );
}

#[test]
fn healing_removes_blemish_adapts_tone_across_tile_boundary_and_undo_redo_are_exact() {
    let mut doc = photo();
    let before = pixels(&doc);
    let mut s = stroke(&doc, doc.clone(), false, 24.0, [-64, 0]);
    s.push(point(256.5, 48.5)).unwrap();
    finish(&mut s, 262144);
    let healed = s.preview.raster().sample(256, 48).0;
    assert!((120..128).contains(&healed[0]), "{healed:?}");
    assert!(healed[1] >= 100);
    let mut history = History::default();
    history
        .execute(&mut doc, Box::new(s.finish().unwrap()), 0)
        .unwrap();
    assert_eq!(history.labels(), vec!["Healing Brush"]);
    let after = pixels(&doc);
    assert_ne!(after, before);
    assert_eq!(doc.dpi, 300.0);
    assert!(history.undo(&mut doc));
    assert_eq!(pixels(&doc), before);
    assert!(history.redo(&mut doc));
    assert_eq!(pixels(&doc), after);
}

#[test]
fn nearby_search_is_deterministic_excludes_overlap_and_removes_the_spot() {
    let doc = photo();
    let mut outputs = Vec::new();
    for chunk in [257, 262144] {
        let mut s = stroke(&doc, doc.clone(), true, 24.0, [0, 0]);
        s.push(point(256.5, 48.5)).unwrap();
        finish(&mut s, chunk);
        let p = s.preview.raster().sample(256, 48).0;
        assert!((110..140).contains(&p[0]) && p[1] > 90, "{p:?}");
        outputs.push(pixels(&s.preview));
    }
    assert_eq!(outputs[0], outputs[1]);
    let dab = Dab {
        p: point(256.5, 48.5),
        radius: 12.0,
        influence: 1.0,
        x0: 244,
        x1: 269,
        y1: 61,
        x: 244,
        y: 36,
    };
    let mut p = HealingPatch::new(&dab, true, [0, 0], CPU_BUDGET).unwrap();
    while p.stage == Stage::Search {
        p.step(&doc, &dab, &Tip::new(&settings(true, 24.0)), 8192)
            .unwrap();
    }
    assert!(f64::from(p.offset[0]).hypot(f64::from(p.offset[1])) > 27.0);
    assert!(p.candidates.len() <= 192);
}

#[test]
fn selection_and_crop_preserve_every_unaffected_sample_and_hidden_rgb() {
    let mut doc = photo();
    doc.canvas = Some(CanvasRect {
        x: 128,
        y: 16,
        width: 256,
        height: 64,
    });
    doc.raster_mut().write(256, 47, Rgba8([99, 77, 55, 0]));
    let mut clip = TileSurface::new(doc.width, doc.height);
    for y in 16..80 {
        for x in 256..384 {
            clip.write(x, y, Gray8(128));
        }
    }
    doc.selection = Some(Arc::new(clip));
    let before = pixels(&doc);
    let mut s = stroke(&doc, doc.clone(), false, 30.0, [-64, 0]);
    s.push(point(256.5, 48.5)).unwrap();
    finish(&mut s, 4096);
    for y in 0..96 {
        for x in 0..520 {
            if x < 256 || !doc.canvas_rect().contains(x, y) || (x == 256 && y == 47) {
                assert_eq!(
                    s.preview.raster().sample(x, y),
                    before[(y * 520 + x) as usize]
                );
            }
        }
    }
    let p = s.preview.raster().sample(257, 48).0;
    // A transparent hole inside the blemish creates an additional unchanged
    // Dirichlet boundary. The partial selection must still blend exactly once.
    assert!((160..200).contains(&p[0]), "{p:?}");
}

#[test]
fn all_visible_reference_can_heal_into_blank_layer_and_alpha_lock_preserves_alpha() {
    for locked in [false, true] {
        let source = photo();
        let mut doc = Document::blank(520, 96, 72.0).unwrap();
        doc.layers[0].properties.locks.alpha = locked;
        doc.raster_mut()
            .write(256, 48, Rgba8([70, 60, 50, if locked { 128 } else { 0 }]));
        let mut s = stroke(&doc, source, false, 24.0, [-64, 0]);
        s.push(point(256.5, 48.5)).unwrap();
        finish(&mut s, 262144);
        let p = s.preview.raster().sample(256, 48).0;
        assert_eq!(p[3], if locked { 128 } else { 255 });
        assert!((120..128).contains(&p[0]));
        if locked {
            assert_eq!(s.preview.raster().sample(257, 48), Rgba8::default());
        }
    }
}

#[test]
fn no_op_cancel_refusal_and_locks_preserve_pixels_and_redo() {
    let mut doc = photo();
    let mut h = History::default();
    let mut s = stroke(&doc, doc.clone(), false, 24.0, [-64, 0]);
    s.push(point(256.5, 48.5)).unwrap();
    finish(&mut s, 262144);
    h.execute(&mut doc, Box::new(s.finish().unwrap()), 0)
        .unwrap();
    assert!(h.undo(&mut doc));
    let before = pixels(&doc);
    let mut cancelled = stroke(&doc, doc.clone(), false, 200.0, [-64, 0]);
    cancelled.push(point(256.5, 48.5)).unwrap();
    cancelled.step(1).unwrap();
    cancelled.step(1).unwrap();
    drop(cancelled);
    let mut refused = StrokeCommand::for_clone(
        &doc,
        settings(false, 2048.0),
        1000000,
        doc.layers[0].id,
        doc.clone(),
        [-64, 0],
    )
    .unwrap();
    refused.push(point(256.5, 48.5)).unwrap();
    assert!(refused.step(262144).is_err());
    let mut noop = stroke(
        &doc,
        Document::blank(520, 96, 72.0).unwrap(),
        false,
        24.0,
        [0, 0],
    );
    noop.push(point(256.5, 48.5)).unwrap();
    finish(&mut noop, 262144);
    assert!(!h
        .execute(&mut doc, Box::new(noop.finish().unwrap()), 0)
        .unwrap());
    assert_eq!(h.depths().1, 1);
    assert_eq!(pixels(&doc), before);
    doc.layers[0].properties.locks.pixels = true;
    assert!(StrokeCommand::for_clone(
        &doc,
        settings(false, 24.0),
        CPU_BUDGET,
        doc.layers[0].id,
        doc.clone(),
        [-64, 0]
    )
    .is_err());
}

#[test]
fn cold_halo_is_requested_before_solver_reads_and_source_snapshots_remain_immutable() {
    let doc = photo();
    let mut sampled = doc.clone();
    let tile = TileHandle::paged(TileBacking {
        store: 1,
        offset: 0,
        length: (TILE_PIXELS * 4) as u32,
        checksum: 0,
    })
    .unwrap();
    sampled
        .raster_mut()
        .insert_handle((0, 0), tile.clone())
        .unwrap();
    let mut s = stroke(&doc, sampled, false, 24.0, [-64, 0]);
    s.push(point(256.5, 48.5)).unwrap();
    s.step(262144).unwrap();
    let mut requested = Vec::new();
    s.clone_needed(&mut |t| requested.push(t.id()));
    assert!(requested.contains(&tile.id()));
    let before = pixels(&doc);
    s.step(262144).unwrap();
    assert_eq!(pixels(&s.preview), before);
    assert!(s.temporary_bytes() >= (27 * 27 * 105) as u64);
}
