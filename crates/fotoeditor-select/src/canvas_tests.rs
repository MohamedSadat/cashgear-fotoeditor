use crate::*;
use fotoeditor_core::*;
use std::sync::Arc;

#[test]
fn cropped_fill_clear_never_changes_hidden_retained_samples() {
    let mut d = Document::blank(513, 3, 72.0).unwrap();
    for x in [0, 254, 255, 256, 257, 512] {
        d.raster_mut().write(x, 1, Rgba8([87, 33, 19, 1]));
    }
    d.canvas = Some(CanvasRect {
        x: 255,
        y: 1,
        width: 2,
        height: 1,
    });
    let mut j = FillJob::new(&d, d.layers[0].id, None, [0; 4], true, 8 << 20).unwrap();
    while !j.step().unwrap() {}
    for x in [0, 254, 257, 512] {
        assert_eq!(j.candidate.raster().sample(x, 1), d.raster().sample(x, 1));
    }
    for x in [255, 256] {
        assert_eq!(j.candidate.raster().sample(x, 1), Rgba8([87, 33, 19, 0]));
    }
    let mut history = History::default();
    history
        .execute(&mut d, Box::new(j.finish().unwrap()), 0)
        .unwrap();
    assert!(history.undo(&mut d));
    assert_eq!(d.raster().sample(256, 1), Rgba8([87, 33, 19, 1]));
}
#[test]
fn cropped_copy_intersects_selection_and_preserves_hidden_rgb() {
    let mut source = TileSurface::new(8, 8);
    for y in 0..8 {
        for x in 0..8 {
            source.write(x, y, Rgba8([x as u8, y as u8, 87, ((x + y) * 16) as u8]));
        }
    }
    source.write(2, 2, Rgba8([87, 33, 19, 0]));
    let source = Arc::new(source);
    let canvas = CanvasRect {
        x: 2,
        y: 2,
        width: 3,
        height: 3,
    };
    for selected in [false, true] {
        let selection = selected.then(|| {
            let mut s = TileSurface::with_default(8, 8, Gray8(255));
            s.write(3, 3, Gray8(128));
            Arc::new(s)
        });
        let mut job = CopyJob::in_canvas(source.clone(), selection, Some(canvas), 8 << 20).unwrap();
        while !job.step().unwrap() {}
        let output = job.finish().unwrap();
        assert_eq!((output.samples.width, output.samples.height), (3, 3));
        assert_eq!(output.origin, [2, 2]);
        assert_eq!(output.samples.sample(0, 0), Rgba8([87, 33, 19, 0]));
        for y in 0..3 {
            for x in 0..3 {
                let mut expected = source.sample(x + 2, y + 2);
                if selected && x == 1 && y == 1 {
                    expected.0[3] = ((u16::from(expected.0[3]) * 128 + 127) / 255) as u8;
                }
                assert_eq!(output.samples.sample(x, y), expected);
            }
        }
    }
}
#[test]
fn cropped_wand_membership_and_antialiasing_stop_at_visible_boundary() {
    let mut source = TileSurface::new(8, 8);
    for y in 0..8 {
        for x in 0..8 {
            source.write(x, y, Rgba8([10, 20, 30, 255]));
        }
    }
    let source = Arc::new(source);
    let canvas = CanvasRect {
        x: 2,
        y: 2,
        width: 3,
        height: 3,
    };
    for tolerance in [0, 255] {
        for contiguous in [false, true] {
            let mut job = WandJob::in_canvas(
                source.clone(),
                None,
                WandOptions {
                    seed: [3, 3],
                    tolerance,
                    contiguous,
                    ..WandOptions::default()
                },
                None,
                16 << 20,
                Some(canvas),
            )
            .unwrap();
            while !job.step().unwrap() {}
            let (output, count) = job.finish().unwrap();
            assert_eq!(count, 9);
            for y in 0..8 {
                for x in 0..8 {
                    if !canvas.contains(x, y) {
                        assert_eq!(output.sample(x, y), Gray8(0));
                    }
                }
            }
            assert!(output.sample(3, 3).0 > 0);
        }
    }
    assert!(WandJob::in_canvas(
        source,
        None,
        WandOptions::default(),
        None,
        16 << 20,
        Some(canvas)
    )
    .is_err());
}
