#[cfg(test)]
mod tests {
    use crate::*;
    use std::sync::Arc;
    #[test]
    fn adjustment_clipping_base_inherits_backdrop_alpha_and_keeps_its_blend_mode() {
        let mut doc = Document::blank(1, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([64, 128, 192, 128]));
        doc.layers.push(Layer::adjustment(
            "Invert".into(),
            Adjustment::Curves {
                points: vec![[0.0, 1.0], [1.0, 0.0]],
            },
        ));
        let mut samples = TileSurface::new(1, 1);
        samples.write(0, 0, Rgba8([255, 0, 0, 128]));
        let mut top = Layer::raster("Clipped red".into(), samples);
        top.properties.clipped = true;
        doc.layers.push(top);
        let plan = CompositePlan::tile(&doc, (0, 0), None).unwrap();
        assert_eq!(
            plan.sample(0).unwrap().straight(),
            Rgba8([223, 63, 31, 128])
        );
        assert_eq!(
            plan.reference(Residency::default()).unwrap()[0].straight(),
            Rgba8([223, 63, 31, 128])
        );
        doc.layers[1].properties.blend = BlendMode::Multiply;
        doc.layers[2].properties.opacity = 0.0;
        assert_eq!(
            CompositePlan::tile(&doc, (0, 0), None)
                .unwrap()
                .sample(0)
                .unwrap()
                .straight(),
            Rgba8([48, 64, 47, 128])
        );
        doc.layers[1].properties.visible = false;
        assert_eq!(
            CompositePlan::tile(&doc, (0, 0), None)
                .unwrap()
                .sample(0)
                .unwrap()
                .straight(),
            Rgba8([64, 128, 192, 128])
        );
        assert_eq!(doc.raster().sample(0, 0), Rgba8([64, 128, 192, 128]));
    }

    fn settings() -> [Adjustment; 5] {
        [
            Adjustment::BrightnessContrast {
                brightness: 0.17,
                contrast: -0.3,
            },
            Adjustment::Levels {
                black: 0.12,
                white: 0.85,
                gamma: 1.7,
                output_black: 0.04,
                output_white: 0.94,
            },
            Adjustment::Curves {
                points: vec![[0.0, 0.9], [0.3, 0.2], [0.8, 0.7], [1.0, 0.1]],
            },
            Adjustment::Exposure {
                stops: 1.3,
                offset: -0.04,
                gamma: 0.8,
            },
            Adjustment::HueSaturation {
                hue: -127.0,
                saturation: 0.27,
                lightness: -0.17,
            },
        ]
    }
    #[test]
    fn layer_matches_destructive_math_at_alpha_selection_and_tile_boundaries() {
        for settings in settings()
            .into_iter()
            .chain(crate::color_adjustments::settings())
        {
            let kernel = settings.compile().unwrap();
            let mut doc = Document::blank(513, 3, 72.0).unwrap();
            let mut mask = TileSurface::new(513, 3);
            for y in 0..3 {
                for x in 0..513 {
                    let alpha = [0, 1, 128, 255][x as usize % 4];
                    doc.raster_mut().write(
                        x,
                        y,
                        Rgba8([x as u8, (x * 7) as u8, (x * 31) as u8, alpha]),
                    );
                    if x >= 255 {
                        mask.write(x, y, Gray8([0, 128, 255][x as usize % 3]));
                    }
                }
            }
            let original = doc.raster().clone();
            let mut layer = Layer::adjustment("Test".into(), settings);
            layer.mask = Some(LayerMask {
                enabled: true,
                samples: Arc::new(mask.clone()),
            });
            doc.layers.push(layer);
            for coord in [(0, 0), (1, 0), (2, 0)] {
                let plan = CompositePlan::tile(&doc, coord, None).unwrap();
                let reference = plan.reference(Residency::default()).unwrap();
                for y in 0..3 {
                    for x in coord.0 * 256..((coord.0 + 1) * 256).min(513) {
                        let before = original.sample(x, y);
                        let expected = kernel.sample(before, mask.sample(x, y).0);
                        let i = (y * 256 + x % 256) as usize;
                        let actual = reference[i].straight();
                        assert_eq!(actual.0[3], before.0[3]);
                        if before.0[3] != 0 {
                            for c in 0..3 {
                                assert!(
                                    actual.0[c].abs_diff(expected.0[c]) <= 1,
                                    "{x},{y} {actual:?} {expected:?}"
                                );
                            }
                        }
                        assert_eq!(reference[i], plan.sample(i).unwrap());
                    }
                }
            }
            assert_eq!(doc.raster().sample(0, 0), original.sample(0, 0));
        }
    }
    #[test]
    fn selection_mask_metadata_history_and_locks_are_exact() {
        let mut doc = Document::blank(513, 3, 72.0).unwrap();
        let mut selected = TileSurface::new(513, 3);
        selected.write(256, 1, Gray8(128));
        doc.selection = Some(Arc::new(selected));
        let mut history = History::default();
        let setting = settings()[0].clone();
        let command = LayerCommand::prepare(
            &doc,
            LayerAction::AddAdjustment {
                adjustment: setting.clone(),
                parent: None,
                index: 1,
            },
        )
        .unwrap();
        history.execute(&mut doc, Box::new(command), 0).unwrap();
        let layer = doc.layers[1].clone();
        let id = layer.id;
        assert!(Arc::ptr_eq(
            &layer.mask.as_ref().unwrap().samples,
            doc.selection.as_ref().unwrap()
        ));
        assert_eq!(layer.mask.as_ref().unwrap().samples.default, Gray8(0));
        let command = LayerCommand::prepare(
            &doc,
            LayerAction::SetAdjustment {
                target: id,
                adjustment: settings()[4].clone(),
            },
        )
        .unwrap();
        history.execute(&mut doc, Box::new(command), 0).unwrap();
        assert!(history.undo(&mut doc));
        let LayerContent::Adjustment(actual) = &doc.layers[1].content else {
            panic!()
        };
        assert_eq!(actual, &setting);
        let command = LayerCommand::prepare(
            &doc,
            LayerAction::SetAdjustment {
                target: id,
                adjustment: setting.clone(),
            },
        )
        .unwrap();
        assert!(!history.execute(&mut doc, Box::new(command), 0).unwrap());
        doc.layers[1].properties.locks.pixels = true;
        assert!(LayerCommand::prepare(
            &doc,
            LayerAction::SetAdjustment {
                target: id,
                adjustment: settings()[4].clone()
            }
        )
        .is_err());
        doc.layers[1].properties.locks.pixels = false;
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers.len(), 1);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[1].id, id);
    }
}
