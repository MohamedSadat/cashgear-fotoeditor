use super::*;
use fotoeditor_core::{
    CanvasRect, History, Residency, TileBacking, TileHandle, TilePager, TileSurface, CPU_BUDGET,
};
use std::sync::Arc;

fn point(x: f64, y: f64) -> Sample {
    Sample {
        x,
        y,
        pressure: 1.0,
        tilt_x: 0.0,
        tilt_y: 0.0,
        time: 1.0,
    }
}
fn stroke(
    doc: &Document,
    settings: BrushSettings,
    source: Document,
    offset: [i32; 2],
) -> StrokeCommand {
    StrokeCommand::for_clone(doc, settings, CPU_BUDGET, doc.layers[0].id, source, offset).unwrap()
}
fn pencil() -> BrushSettings {
    BrushSettings {
        pencil: true,
        size: 1.0,
        pressure_size: false,
        ..Default::default()
    }
}
fn finish(s: &mut StrokeCommand) {
    s.end();
    for _ in 0..10000 {
        s.step(262144).unwrap();
        if s.done() {
            return;
        }
    }
    panic!("stroke failed to finish");
}
fn pixels(doc: &Document) -> Vec<Rgba8> {
    (0..doc.height)
        .flat_map(|y| (0..doc.width).map(move |x| doc.raster().sample(x, y)))
        .collect()
}

#[test]
fn source_pick_and_alignment_are_pixel_exact_and_cancellation_does_not_change_anchor() {
    let mut anchor = CloneAnchor::default();
    let bounds = CanvasRect {
        x: 10,
        y: 20,
        width: 300,
        height: 200,
    };
    assert!(anchor.mapping([100.0, 100.0], true).is_err());
    anchor.select([20.9, 30.8], bounds).unwrap();
    let offset = anchor.mapping([100.8, 100.9], true).unwrap();
    assert_eq!(offset, [-80, -70]);
    assert_eq!(anchor.offset, None);
    // Only the adapter's successful command commit stores the pending offset.
    anchor.offset = Some(offset);
    assert_eq!(anchor.mapping([200.0, 150.0], true).unwrap(), offset);
    assert_eq!(anchor.mapping([200.0, 150.0], false).unwrap(), [-180, -120]);
    for p in [
        [f64::NAN, 30.0],
        [f64::INFINITY, 30.0],
        [-0.1, 30.0],
        [9.9, 30.0],
        [310.0, 30.0],
    ] {
        assert!(anchor.select(p, bounds).is_err());
        assert_eq!(anchor.source, Some([20, 30]));
        assert_eq!(anchor.offset, Some(offset));
    }
    anchor.select([40.0, 50.0], bounds).unwrap();
    assert_eq!(anchor.offset, None);
}

#[test]
fn overlapping_clone_never_samples_preview_and_command_undo_redo_are_exact() {
    let mut doc = Document::blank(520, 2, 300.0).unwrap();
    for x in 0..520 {
        doc.raster_mut()
            .write(x, 0, Rgba8([(x % 251) as u8, 33, 19, 255]));
    }
    doc.raster_mut().write(519, 1, Rgba8([87, 33, 19, 0]));
    let before = pixels(&doc);
    let source = doc.clone();
    let mut s = stroke(&doc, pencil(), source, [-1, 0]);
    for x in 1..519 {
        s.push(point(x as f64 + 0.5, 0.5)).unwrap();
    }
    finish(&mut s);
    for x in 1..519 {
        assert_eq!(s.preview.raster().sample(x, 0), before[x as usize - 1]);
    }
    assert_eq!(pixels(&doc), before);
    let mut h = History::default();
    h.execute(&mut doc, Box::new(s.finish().unwrap()), 0)
        .unwrap();
    assert_eq!(h.labels(), vec!["Clone Stamp"]);
    let after = pixels(&doc);
    assert!(h.undo(&mut doc));
    assert_eq!(pixels(&doc), before);
    assert!(h.redo(&mut doc));
    assert_eq!(pixels(&doc), after);
    assert_eq!(doc.raster().sample(519, 1), Rgba8([87, 33, 19, 0]));
    assert_eq!(doc.dpi, 300.0);
}

#[test]
fn selection_source_alpha_and_alpha_lock_apply_once_and_ignore_foreground_alpha() {
    for locked in [false, true] {
        let mut doc = Document::blank(520, 1, 72.0).unwrap();
        for x in [254, 255, 256, 257] {
            doc.raster_mut().write(x, 0, Rgba8([200, 40, 10, 128]));
        }
        doc.raster_mut().write(258, 0, Rgba8([87, 33, 19, 0]));
        doc.layers[0].properties.locks.alpha = locked;
        let mut clip = TileSurface::new(520, 1);
        for x in [255, 256, 257, 258] {
            clip.write(x, 0, Gray8(if x == 255 { 128 } else { 255 }));
        }
        doc.selection = Some(Arc::new(clip));
        let mut source = Document::blank(520, 1, 72.0).unwrap();
        for x in 0..520 {
            source.raster_mut().write(x, 0, Rgba8([10, 80, 220, 128]));
        }
        source.raster_mut().write(267, 0, Rgba8([99, 88, 77, 0]));
        let mut s = stroke(
            &doc,
            BrushSettings {
                size: 100.0,
                hardness: 1.0,
                pressure_size: false,
                color: [0, 0, 0, 0],
                ..Default::default()
            },
            source,
            [10, 0],
        );
        for _ in 0..3 {
            s.push(point(256.5, 0.5)).unwrap();
        }
        finish(&mut s);
        assert_eq!(
            s.preview.raster().sample(254, 0),
            doc.raster().sample(254, 0)
        );
        assert_eq!(
            s.preview.raster().sample(257, 0),
            doc.raster().sample(257, 0)
        );
        let actual = s.preview.raster().sample(256, 0);
        if locked {
            assert_eq!(actual, Rgba8([105, 60, 115, 128]));
            assert_eq!(s.preview.raster().sample(258, 0), Rgba8([87, 33, 19, 0]));
        } else {
            let expected =
                super::super::composite(Rgba8([200, 40, 10, 128]), [10, 80, 220, 128], 1.0, false);
            for c in 0..4 {
                assert!(actual.0[c].abs_diff(expected.0[c]) <= 1);
            }
        }
        let partial = s.preview.raster().sample(255, 0);
        assert!(partial.0[0] > actual.0[0]);
    }
}

#[test]
fn crop_clips_source_and_destination_without_revealing_retained_pixels() {
    let mut doc = Document::blank(520, 1, 72.0).unwrap();
    for x in 0..520 {
        doc.raster_mut().write(x, 0, Rgba8([x as u8, 33, 19, 255]));
    }
    doc.canvas = Some(CanvasRect {
        x: 250,
        y: 0,
        width: 20,
        height: 1,
    });
    let original = pixels(&doc);
    let mut s = stroke(
        &doc,
        BrushSettings {
            size: 80.0,
            pressure_size: false,
            ..Default::default()
        },
        doc.clone(),
        [-10, 0],
    );
    s.push(point(260.5, 0.5)).unwrap();
    finish(&mut s);
    for x in 0..520 {
        let after = s.preview.raster().sample(x, 0);
        if (260..270).contains(&x) {
            assert_eq!(after, doc.raster().sample(x - 10, 0));
        } else {
            assert_eq!(after, original[x as usize]);
        }
    }
}

#[test]
fn missing_source_pages_pause_before_changes_and_request_only_dab_tiles() {
    let doc = Document::blank(16384, 16384, 72.0).unwrap();
    let mut source = doc.clone();
    let backing = TileBacking {
        store: 1,
        offset: 0,
        length: (TILE_PIXELS * 4) as u32,
        checksum: 0,
    };
    let tile = TileHandle::paged(backing).unwrap();
    let unrelated = TileHandle::paged(TileBacking {
        offset: u64::from(backing.length),
        ..backing
    })
    .unwrap();
    source
        .raster_mut()
        .insert_handle((1, 0), tile.clone())
        .unwrap();
    source
        .raster_mut()
        .insert_handle((60, 60), unrelated.clone())
        .unwrap();
    let mut s = stroke(&doc, pencil(), source, [256, 0]);
    s.push(point(0.5, 0.5)).unwrap();
    s.end();
    s.prepare_clone();
    let mut ids = Vec::new();
    s.clone_needed(&mut |t| ids.push(t.id()));
    assert_eq!(ids, vec![tile.id()]);
    s.step(262144).unwrap();
    assert!(!s.done());
    assert_eq!(s.preview.tile_count(), 0);
    assert_eq!(s.coverage.len(), 0);
    let mut pager = TilePager::default();
    pager
        .hydrate(
            &tile,
            backing,
            vec![Rgba8([10, 20, 30, 255]); TILE_PIXELS],
            Residency::default(),
        )
        .unwrap();
    finish(&mut s);
    assert_eq!(s.preview.raster().sample(0, 0), Rgba8([10, 20, 30, 255]));
    assert_eq!(unrelated.resident_bytes(), 0);
}

#[test]
fn locked_invalid_and_refused_clone_preserve_document_and_redo() {
    let mut doc = Document::blank(520, 1, 72.0).unwrap();
    doc.raster_mut().write(0, 0, Rgba8([200, 40, 10, 255]));
    let mut h = History::default();
    let mut s = stroke(&doc, pencil(), doc.clone(), [-256, 0]);
    s.push(point(256.5, 0.5)).unwrap();
    finish(&mut s);
    h.execute(&mut doc, Box::new(s.finish().unwrap()), 0)
        .unwrap();
    h.undo(&mut doc);
    let before = pixels(&doc);
    let mut refused = stroke(&doc, pencil(), doc.clone(), [-256, 0]);
    refused.limit_available(1);
    refused.push(point(256.5, 0.5)).unwrap();
    refused.end();
    assert!(refused.step(262144).is_err());
    assert_eq!(pixels(&doc), before);
    assert!(h.redo(&mut doc));
    h.undo(&mut doc);
    assert!(
        StrokeCommand::for_clone(&doc, pencil(), 1, doc.layers[0].id, doc.clone(), [0, 0]).is_err()
    );
    assert!(StrokeCommand::for_clone(
        &doc,
        pencil(),
        CPU_BUDGET,
        doc.layers[0].id,
        doc.clone(),
        [i32::MAX, 0]
    )
    .is_err());
    doc.layers[0].properties.locks.pixels = true;
    assert!(StrokeCommand::for_clone(
        &doc,
        pencil(),
        CPU_BUDGET,
        doc.layers[0].id,
        doc.clone(),
        [0, 0]
    )
    .is_err());
    assert_eq!(pixels(&doc), before);
    assert!(h.redo(&mut doc));
    h.undo(&mut doc);
}

#[test]
fn opaque_clone_optimization_matches_reference_across_source_and_destination_tile_edges() {
    let mut doc = Document::blank(520, 256, 72.0).unwrap();
    for coord in [(0, 0), (1, 0), (2, 0)] {
        doc.raster_mut().insert(
            coord,
            (0..TILE_PIXELS)
                .map(|i| Rgba8([(i % 251) as u8, ((i / 256) % 239) as u8, 97, 255]))
                .collect(),
        );
    }
    let mut general = doc.clone();
    for coord in [(0, 0), (1, 0), (2, 0)] {
        general.raster_mut().edit_tile(coord);
    }
    let settings = BrushSettings {
        size: 80.0,
        hardness: 0.0,
        opacity: 0.4,
        flow: 0.2,
        pressure_size: false,
        roundness: 0.7,
        angle: 36.0,
        ..Default::default()
    };
    let mut a = stroke(&doc, settings.clone(), doc.clone(), [-13, 17]);
    let mut b = stroke(&general, settings, general.clone(), [-13, 17]);
    for p in [
        point(10.1, 120.2),
        point(500.00390625, 123.00390625),
        point(30.75, 140.25),
    ] {
        a.push(p).unwrap();
        b.push(p).unwrap();
    }
    finish(&mut a);
    finish(&mut b);
    assert_eq!(pixels(&a.preview), pixels(&b.preview));
}
