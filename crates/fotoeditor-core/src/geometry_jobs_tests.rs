use crate::*;
use std::sync::Arc;
fn command(d: &Document, a: GeometryAction) -> DocumentCommand {
    let mut j = GeometryJob::new(Arc::new(d.clone()), a, CPU_BUDGET).unwrap();
    while !j.step().unwrap() {}
    j.finish().unwrap()
}
fn fixture() -> Document {
    let mut d = Document::blank(7, 5, 72.0).unwrap();
    for y in 0..5 {
        for x in 0..7 {
            d.raster_mut().write(
                x,
                y,
                Rgba8([
                    x as u8 * 30,
                    y as u8 * 40,
                    19,
                    if x == 0 { 0 } else { 1 + y as u8 * 50 },
                ]),
            );
        }
    }
    let mut mask = TileSurface::with_default(7, 5, Gray8(255));
    mask.write(1, 2, Gray8(73));
    d.layers[0].mask = Some(LayerMask {
        enabled: true,
        samples: Arc::new(mask),
    });
    let mut select = TileSurface::with_default(7, 5, Gray8(255));
    select.write(3, 2, Gray8(29));
    d.selection = Some(Arc::new(select));
    let child = d.layers.remove(0);
    d.layers.push(Layer::group("Nested".into(), vec![child]));
    d.guides = vec![
        Guide {
            axis: GuideAxis::Vertical,
            position: 2.5,
        },
        Guide {
            axis: GuideAxis::Horizontal,
            position: 1.25,
        },
    ];
    d
}
#[test]
fn whole_tree_right_angle_mask_selection_guides_and_history_are_exact() {
    let mut d = fixture();
    let original = d.clone();
    let LayerContent::Group(children) = &d.layers[0].content else {
        panic!()
    };
    let id = children[0].id;
    let mut h = History::default();
    let c = command(
        &d,
        GeometryAction::CanvasRotate {
            degrees: 90.0,
            interpolation: Interpolation::Lanczos3,
        },
    );
    h.execute(&mut d, Box::new(c), 0).unwrap();
    assert_eq!((d.width, d.height), (5, 7));
    assert_eq!(
        d.guides,
        vec![
            Guide {
                axis: GuideAxis::Horizontal,
                position: 2.5
            },
            Guide {
                axis: GuideAxis::Vertical,
                position: 3.75
            }
        ]
    );
    for y in 0..5 {
        for x in 0..7 {
            assert_eq!(
                original
                    .layer(id)
                    .unwrap()
                    .raster_samples()
                    .unwrap()
                    .sample(x, y),
                d.layer(id)
                    .unwrap()
                    .raster_samples()
                    .unwrap()
                    .sample(4 - y, x)
            );
            assert_eq!(
                original
                    .layer(id)
                    .unwrap()
                    .mask
                    .as_ref()
                    .unwrap()
                    .samples
                    .sample(x, y),
                d.layer(id)
                    .unwrap()
                    .mask
                    .as_ref()
                    .unwrap()
                    .samples
                    .sample(4 - y, x)
            );
            assert_eq!(
                original.selection.as_ref().unwrap().sample(x, y),
                d.selection.as_ref().unwrap().sample(4 - y, x)
            );
        }
    }
    assert!(h.undo(&mut d));
    assert_eq!((d.width, d.height), (7, 5));
    assert_eq!(d.guides, original.guides);
    for (coord, t) in original
        .layer(id)
        .unwrap()
        .raster_samples()
        .unwrap()
        .tiles()
    {
        assert_eq!(
            d.layer(id).unwrap().raster_samples().unwrap().tiles()[coord].id(),
            t.id()
        );
    }
    assert!(h.redo(&mut d));
    assert_eq!((d.width, d.height), (5, 7));
}
#[test]
fn anchored_growth_preserves_hidden_samples_and_zeroes_new_selection_area() {
    let mut d = fixture();
    let original = d.clone();
    let LayerContent::Group(children) = &d.layers[0].content else {
        panic!()
    };
    let id = children[0].id;
    let c = command(
        &d,
        GeometryAction::CanvasSize {
            width: 11,
            height: 9,
            anchor: [0.5, 0.5],
        },
    );
    c.apply(&mut d);
    assert_eq!((d.width, d.height), (11, 9));
    assert!(d.canvas.is_none());
    for y in 0..5 {
        for x in 0..7 {
            assert_eq!(
                d.layer(id)
                    .unwrap()
                    .raster_samples()
                    .unwrap()
                    .sample(x + 2, y + 2),
                original
                    .layer(id)
                    .unwrap()
                    .raster_samples()
                    .unwrap()
                    .sample(x, y)
            );
        }
    }
    assert_eq!(d.selection.as_ref().unwrap().sample(0, 0), Gray8(0));
    assert_eq!(d.selection.as_ref().unwrap().sample(2, 2), Gray8(255));
    assert_eq!(
        d.layer(id)
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .samples
            .sample(0, 0),
        Gray8(255)
    );
    assert_eq!(d.guides[0].position, 4.5);
    c.revert(&mut d);
    assert_eq!((d.width, d.height), (7, 5));
    let mut j = GeometryJob::new(
        Arc::new(d.clone()),
        GeometryAction::CanvasSize {
            width: 3,
            height: 3,
            anchor: [0.5, 0.5],
        },
        0,
    )
    .unwrap();
    while !j.step().unwrap() {}
    let c = j.finish().unwrap();
    c.apply(&mut d);
    assert_eq!(
        d.canvas_rect(),
        CanvasRect {
            x: 2,
            y: 1,
            width: 3,
            height: 3
        }
    );
    assert_eq!((d.width, d.height), (7, 5));
    for (coord, t) in original
        .layer(id)
        .unwrap()
        .raster_samples()
        .unwrap()
        .tiles()
    {
        assert_eq!(
            d.layer(id).unwrap().raster_samples().unwrap().tiles()[coord].id(),
            t.id()
        );
    }
}
#[test]
fn image_resize_respects_odd_crop_window_dpi_and_retained_content() {
    let mut d = Document::blank(13, 9, 72.0).unwrap();
    d.raster_mut().write(12, 8, Rgba8([87, 33, 19, 0]));
    d.canvas = Some(CanvasRect {
        x: 3,
        y: 1,
        width: 7,
        height: 5,
    });
    let c = command(
        &d,
        GeometryAction::ImageSize {
            width: 14,
            height: 15,
            dpi: 300.0,
            interpolation: Interpolation::Nearest,
        },
    );
    c.apply(&mut d);
    assert_eq!((d.width, d.height, d.dpi), (26, 27, 300.0));
    assert_eq!(
        d.canvas_rect(),
        CanvasRect {
            x: 6,
            y: 3,
            width: 14,
            height: 15
        }
    );
    assert_eq!(d.raster().sample(25, 26), Rgba8([87, 33, 19, 0]));
    let c = command(
        &d,
        GeometryAction::ImageSize {
            width: 7,
            height: 5,
            dpi: 72.0,
            interpolation: Interpolation::Nearest,
        },
    );
    c.apply(&mut d);
    assert_eq!((d.width, d.height), (13, 9));
    assert_eq!(d.raster().sample(12, 8), Rgba8([87, 33, 19, 0]));
}
#[test]
fn noop_refusal_and_cancel_keep_redo_and_original_generations() {
    let mut d = Document::blank(7, 5, 72.0).unwrap();
    d.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
    let mut h = History::default();
    let c = command(&d, GeometryAction::CanvasFlip { horizontal: true });
    h.execute(&mut d, Box::new(c), 0).unwrap();
    assert!(h.undo(&mut d));
    let c = command(
        &d,
        GeometryAction::ImageSize {
            width: 7,
            height: 5,
            dpi: 72.0,
            interpolation: Interpolation::Lanczos3,
        },
    );
    assert!(c.is_empty());
    assert!(!h.execute(&mut d, Box::new(c), 0).unwrap());
    let bad = GeometryAction::CanvasSize {
        width: MAX_SIDE,
        height: MAX_SIDE,
        anchor: [0.5, 0.5],
    };
    assert!(GeometryJob::new(Arc::new(d.clone()), bad, 0).is_err());
    let mut j = GeometryJob::new(
        Arc::new(d.clone()),
        GeometryAction::ImageSize {
            width: 14,
            height: 10,
            dpi: 72.0,
            interpolation: Interpolation::Lanczos3,
        },
        CPU_BUDGET,
    )
    .unwrap();
    j.step().unwrap();
    drop(j);
    assert_eq!(d.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
    assert!(h.redo(&mut d));
    assert_eq!(d.raster().sample(6, 0), Rgba8([87, 33, 19, 0]));
}
