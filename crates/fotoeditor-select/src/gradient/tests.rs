use super::*;
use crate::FillJob;
use fotoeditor_core::*;
use std::sync::Arc;
fn gradient(shape: GradientShape) -> Gradient {
    Gradient {
        shape,
        start: [0.5, 0.5],
        end: [10.5, 0.5],
        stops: vec![
            GradientStop {
                position: 0.0,
                color: [255, 0, 0, 0],
            },
            GradientStop {
                position: 0.5,
                color: [0, 0, 255, 128],
            },
            GradientStop {
                position: 1.0,
                color: [0, 255, 0, 255],
            },
        ],
        opacity: 1.0,
        reverse: false,
    }
}
#[test]
fn independent_shape_parameters_and_reverse() {
    for (shape, p, expected) in [
        (GradientShape::Linear, [5.5, 9.5], 0.5),
        (GradientShape::Radial, [3.5, 4.5], 0.5),
        (GradientShape::Angle, [0.5, 10.5], 0.25),
        (GradientShape::Angle, [0.5, -9.5], 0.75),
        (GradientShape::Reflected, [-4.5, 99.5], 0.5),
        (GradientShape::Diamond, [2.5, 3.5], 0.5),
    ] {
        let mut g = gradient(shape);
        assert!((g.parameter(p) - expected).abs() < 1e-12);
        g.reverse = true;
        assert!((g.parameter(p) - (1.0 - expected)).abs() < 1e-12);
    }
    let mut g = gradient(GradientShape::Linear);
    g.end = [0.5, 10.5];
    assert_eq!(g.parameter([99.5, 5.5]), 0.5);
}
#[test]
fn premultiplied_multi_stop_oracle_and_validation() {
    let mut g = gradient(GradientShape::Linear);
    g.validate().unwrap();
    assert_eq!(g.sample([3.0, 0.5]), [0, 0, 255, 64]);
    assert_eq!(g.sample([5.5, 0.5]), [0, 0, 255, 128]);
    assert_eq!(g.sample([8.0, 0.5]), [0, 170, 85, 192]);
    g.opacity = 0.5;
    assert_eq!(g.sample([8.0, 0.5]), [0, 170, 85, 96]);
    g.end = g.start;
    assert!(g.validate().is_err());
    g.end = [10.5, 0.5];
    g.stops[1].position = 1.0;
    assert!(g.validate().is_err());
    g.stops[1].position = f64::NAN;
    assert!(g.validate().is_err());
}
#[test]
fn gradient_tile_crop_selection_alpha_lock_and_exact_command() {
    for locked in [false, true] {
        let mut d = Document::blank(258, 3, 300.0).unwrap();
        for x in 0..258 {
            d.raster_mut().write(x, 1, Rgba8([80, 40, 20, 128]));
        }
        d.raster_mut().write(257, 1, Rgba8([87, 33, 19, 0]));
        d.canvas = Some(CanvasRect {
            x: 254,
            y: 0,
            width: 4,
            height: 3,
        });
        d.layers[0].properties.locks.alpha = locked;
        let mut s = TileSurface::new(258, 3);
        s.write(256, 1, Gray8(128));
        s.write(257, 1, Gray8(255));
        d.selection = Some(Arc::new(s));
        let target = d.layers[0].id;
        let before = d.raster().tiles().clone();
        let mut g = gradient(GradientShape::Linear);
        g.start = [254.5, 1.5];
        g.end = [256.5, 1.5];
        let mut j = FillJob::gradient(&d, target, g, 8 * 1024 * 1024).unwrap();
        while !j.step().unwrap() {}
        assert_eq!(
            j.candidate.raster().sample(255, 1),
            d.raster().sample(255, 1)
        );
        assert_eq!(
            j.candidate.raster().sample(253, 1),
            d.raster().sample(253, 1)
        );
        if locked {
            assert_eq!(j.candidate.raster().sample(257, 1), Rgba8([87, 33, 19, 0]));
            assert_eq!(
                j.candidate.raster().sample(256, 1),
                Rgba8([40, 148, 10, 128])
            );
        } else {
            assert_eq!(
                j.candidate.raster().sample(256, 1),
                crate::fill_sample(
                    Rgba8([80, 40, 20, 128]),
                    [0, 255, 0, 255],
                    128,
                    false,
                    false
                )
            );
        }
        let after = j.candidate.raster().sample(256, 1);
        let mut h = History::default();
        h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap();
        assert_eq!(h.labels(), vec!["Gradient"]);
        h.undo(&mut d);
        assert!(d
            .raster()
            .tiles()
            .iter()
            .all(|(c, t)| t.id() == before[c].id()));
        h.redo(&mut d);
        assert_eq!(d.raster().sample(256, 1), after);
    }
}
#[test]
fn gradient_refusal_and_incomplete_candidate_do_not_mutate_history() {
    let mut d = Document::blank(257, 1, 72.0).unwrap();
    let layer = d.layers[0].id;
    assert!(FillJob::gradient(&d, layer, gradient(GradientShape::Radial), 0).is_err());
    let j = FillJob::gradient(&d, layer, gradient(GradientShape::Radial), 4 * 1024 * 1024).unwrap();
    assert!(j.finish().is_err());
    assert_eq!(d.raster().bytes(), 0);
    d.layers[0].properties.locks.pixels = true;
    assert!(
        FillJob::gradient(&d, layer, gradient(GradientShape::Radial), 4 * 1024 * 1024).is_err()
    );
}
