use super::*;

fn fixture(w: u32, h: u32, rect: [u32; 4]) -> Document {
    let mut d = Document::blank(w, h, 144.0).unwrap();
    for y in 0..h {
        for x in 0..w {
            let v = if (x / 4 + y / 4) % 2 == 0 { 90 } else { 130 };
            d.raster_mut().write(x, y, Rgba8([v, v + 15, v + 30, 255]));
        }
    }
    let mut mask = TileSurface::new(w, h);
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            mask.write(x, y, Gray8(255));
            d.raster_mut().write(x, y, Rgba8([250, 0, 240, 255]));
        }
    }
    d.selection = Some(Arc::new(mask));
    d
}
fn job(d: &Document) -> ContentAwareJob {
    ContentAwareJob::new(
        d,
        d.layers[0].id,
        Arc::new(d.raster().clone()),
        128 * 1024 * 1024,
    )
    .unwrap()
}
fn run(j: &mut ContentAwareJob) {
    for _ in 0..500_000 {
        if j.step().unwrap() {
            return;
        }
    }
    panic!("bounded job did not finish");
}
#[test]
fn deterministic_texture_donor_exclusion_and_exact_history() {
    let mut d = fixture(65, 49, [25, 17, 39, 31]);
    let original = d.clone();
    let mut a = job(&d);
    run(&mut a);
    let mut b = job(&d);
    run(&mut b);
    assert!(a.propagation_trials > 0 && a.random_trials > a.propagation_trials);
    let l = &a.levels[0];
    for i in 0..l.w * l.h {
        let x = (i % l.w) as u32;
        let y = (i / l.w) as u32;
        let got = a.candidate.raster().sample(x, y);
        assert_eq!(got, b.candidate.raster().sample(x, y));
        if l.mask[i] > 0 {
            assert!((90..=130).contains(&got.0[0]));
            assert_eq!(got.0[1], got.0[0] + 15);
            assert_eq!(got.0[2], got.0[0] + 30);
            let m = l.field[i];
            assert!(l.valid[m.y as usize * l.w + m.x as usize]);
        } else {
            assert_eq!(got, original.raster().sample(x, y));
        }
    }
    let edited = a.candidate.clone();
    let mut history = History::default();
    history
        .execute(&mut d, Box::new(a.finish().unwrap()), 0)
        .unwrap();
    assert_eq!(history.labels(), vec!["Content-aware fill"]);
    assert!(history.undo(&mut d));
    for y in 0..49 {
        for x in 0..65 {
            assert_eq!(d.raster().sample(x, y), original.raster().sample(x, y));
        }
    }
    assert!(history.redo(&mut d));
    for y in 0..49 {
        for x in 0..65 {
            assert_eq!(d.raster().sample(x, y), edited.raster().sample(x, y));
        }
    }
    assert_eq!(d.dpi, 144.0);
}
#[test]
fn fractional_coverage_applied_once_and_hidden_rgb_unchanged() {
    let mut d = fixture(49, 41, [19, 15, 27, 23]);
    Arc::make_mut(d.selection.as_mut().unwrap()).write(19, 15, Gray8(128));
    d.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
    let mut j = job(&d);
    run(&mut j);
    let l = &j.levels[0];
    let i = 15 * l.w + 19;
    assert_eq!(
        j.candidate.raster().sample(19, 15),
        fill_sample(d.raster().sample(19, 15), l.target[i].0, 128, false, false)
    );
    assert_eq!(j.candidate.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
    // No source patch can include the hidden RGB or feathered selection.
    assert!(!l.valid[0]);
    assert!(!l.valid[i]);
}
#[test]
fn alpha_lock_preserves_transparent_samples_and_alpha() {
    let mut d = fixture(49, 41, [19, 15, 27, 23]);
    d.layers[0].properties.locks.alpha = true;
    d.raster_mut().write(20, 16, Rgba8([87, 33, 19, 0]));
    d.raster_mut().write(21, 16, Rgba8([201, 3, 7, 71]));
    let mut j = job(&d);
    run(&mut j);
    assert_eq!(j.candidate.raster().sample(20, 16), Rgba8([87, 33, 19, 0]));
    assert_eq!(j.candidate.raster().sample(21, 16).0[3], 71);
}
#[test]
fn multiscale_tile_crop_boundary_and_immutable_source() {
    let mut d = fixture(290, 100, [243, 28, 280, 69]);
    d.canvas = Some(CanvasRect {
        x: 230,
        y: 8,
        width: 60,
        height: 84,
    });
    let mut j = job(&d);
    run(&mut j);
    assert!(j.levels.len() > 1);
    for y in 0..100 {
        for x in 0..290 {
            if !(243..280).contains(&x) || !(28..69).contains(&y) {
                assert_eq!(j.candidate.raster().sample(x, y), d.raster().sample(x, y));
            }
        }
    }
    assert_eq!(j.source.sample(250, 40), Rgba8([250, 0, 240, 255]));
    assert!(j.candidate.raster().sample(250, 40).0[0] < 200);
    assert_eq!(j.candidate.canvas, d.canvas);
}
#[test]
fn all_visible_source_can_fill_empty_retouch_layer_and_undo_sparse_absence() {
    let mut d = fixture(49, 41, [19, 15, 27, 23]);
    let source = Arc::new(d.raster().clone());
    let layer = Layer::raster("Retouch".into(), TileSurface::new(d.width, d.height));
    let id = layer.id;
    d.layers.push(layer);
    let mut j = ContentAwareJob::new(&d, id, source, 16 * 1024 * 1024).unwrap();
    run(&mut j);
    let mut h = History::default();
    h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap();
    assert_eq!(
        d.layer(id)
            .unwrap()
            .raster_samples()
            .unwrap()
            .sample(20, 16)
            .0[3],
        255
    );
    assert!(h.undo(&mut d));
    assert!(d
        .layer(id)
        .unwrap()
        .raster_samples()
        .unwrap()
        .tiles()
        .is_empty());
}
#[test]
fn locks_empty_selection_unsuitable_source_and_budget_refuse_atomically() {
    let mut d = fixture(49, 41, [19, 15, 27, 23]);
    assert!(ContentAwareJob::new(&d, d.layers[0].id, Arc::new(d.raster().clone()), 0).is_err());
    d.layers[0].properties.locks.pixels = true;
    assert!(
        ContentAwareJob::new(&d, d.layers[0].id, Arc::new(d.raster().clone()), 1 << 24).is_err()
    );
    d.layers[0].properties.locks.pixels = false;
    d.selection = None;
    assert!(
        ContentAwareJob::new(&d, d.layers[0].id, Arc::new(d.raster().clone()), 1 << 24).is_err()
    );
    d.selection = Some(Arc::new(TileSurface::new(d.width, d.height)));
    assert!(job(&d).step().is_err());
    d.selection = Some(Arc::new(TileSurface::with_default(
        d.width,
        d.height,
        Gray8(255),
    )));
    let mut j = job(&d);
    let mut error = None;
    for _ in 0..1000 {
        if let Err(e) = j.step() {
            error = Some(e);
            break;
        }
    }
    assert!(error.unwrap().contains("donor"));
    assert_eq!(d.raster().sample(20, 16), Rgba8([250, 0, 240, 255]));
    let big = fixture(180, 180, [60, 60, 100, 100]);
    let mut j = ContentAwareJob::new(
        &big,
        big.layers[0].id,
        Arc::new(big.raster().clone()),
        3 * TILE_BYTES,
    )
    .unwrap();
    assert!(j.step().unwrap_err().contains("budget"));
}
#[test]
fn dropping_incomplete_candidate_and_noop_preserve_redo() {
    let mut d = fixture(49, 41, [19, 15, 27, 23]);
    let mut h = History::default();
    let mut j = job(&d);
    run(&mut j);
    h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap();
    h.undo(&mut d);
    let before = d.raster().sample(20, 16);
    let mut cancelled = job(&d);
    for _ in 0..60 {
        cancelled.step().unwrap();
    }
    drop(cancelled);
    assert_eq!(h.depths(), (0, 1));
    assert_eq!(d.raster().sample(20, 16), before);
    // Alpha-locked transparent destination makes the final command a no-op.
    let source = Arc::new(d.raster().clone());
    d.layers[0].properties.locks.alpha = true;
    for y in 15..23 {
        for x in 19..27 {
            d.raster_mut().write(x, y, Rgba8([87, 33, 19, 0]));
        }
    }
    let mut j = ContentAwareJob::new(&d, d.layers[0].id, source, 1 << 24).unwrap();
    run(&mut j);
    assert!(!h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap());
    assert_eq!(h.depths(), (0, 1));
}
#[test]
fn cold_generations_are_requested_and_missing_hydration_is_an_error() {
    let mut d = fixture(49, 41, [19, 15, 27, 23]);
    let backing = TileBacking {
        store: 1,
        offset: 0,
        length: TILE_PIXELS as u32,
        checksum: 0,
    };
    Arc::make_mut(d.selection.as_mut().unwrap())
        .insert_handle((0, 0), TileHandle::paged(backing).unwrap())
        .unwrap();
    let mut j = job(&d);
    let mut cold = 0;
    j.needed(&mut |t| {
        if t.bytes() == 0 {
            cold += 1;
        }
    });
    assert_eq!(cold, 1);
    assert!(j.step().unwrap_err().contains("hydrate"));
}
#[test]
fn patch_distance_matches_independent_opaque_oracle() {
    let d = fixture(33, 33, [14, 14, 18, 18]);
    let mut j = job(&d);
    while j.stage != Stage::Match {
        j.step().unwrap();
    }
    let l = &j.levels[0];
    let i = 15 * l.w + 15;
    let donor = l.donors[0] as usize;
    let sx = donor % l.w;
    let sy = donor / l.w;
    let mut sum = 0.0f32;
    let mut weights = 0.0f32;
    for yy in 12..=18 {
        for xx in 12..=18 {
            let t = yy * l.w + xx;
            let s = (sy + yy - 15) * l.w + sx + xx - 15;
            let w = if l.mask[t] > 0 { 0.15 } else { 1.0 };
            for c in 0..3 {
                sum += w * (f32::from(l.target[t].0[c]) - f32::from(l.source[s].0[c])).powi(2);
            }
            weights += w;
        }
    }
    assert!((l.distance(i, sx as i32, sy as i32) - sum / weights).abs() < 0.01);
}

#[test]
fn selected_object_colors_cannot_influence_multiscale_synthesis() {
    let a = fixture(97, 81, [29, 23, 67, 59]);
    let mut b = a.clone();
    for y in 23..59 {
        for x in 29..67 {
            b.raster_mut().write(x, y, Rgba8([0, 255, 0, 255]));
        }
    }
    let mut ja = job(&a);
    let mut jb = job(&b);
    run(&mut ja);
    run(&mut jb);
    assert!(ja.levels.len() > 1);
    for y in 0..81 {
        for x in 0..97 {
            assert_eq!(
                ja.candidate.raster().sample(x, y),
                jb.candidate.raster().sample(x, y)
            );
        }
    }
}

#[test]
fn source_pages_are_requested_only_for_next_capture_tile() {
    let d = fixture(290, 90, [250, 32, 263, 49]);
    let mut source = d.raster().clone();
    source
        .insert_handle(
            (0, 0),
            TileHandle::paged(TileBacking {
                store: 2,
                offset: 0,
                length: (TILE_PIXELS * 4) as u32,
                checksum: 0,
            })
            .unwrap(),
        )
        .unwrap();
    let mut j = ContentAwareJob::new(&d, d.layers[0].id, Arc::new(source), 1 << 25).unwrap();
    while j.stage != Stage::Capture {
        j.step().unwrap();
    }
    let mut rgba = Vec::new();
    j.needed(&mut |t| {
        if let TileRef::Rgba(_) = t {
            rgba.push(t.id());
        }
    });
    assert_eq!(rgba.len(), 1);
    assert!(j.step().unwrap_err().contains("hydrate"));
}
