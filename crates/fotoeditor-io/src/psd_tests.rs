use super::PsdImportJob;
use fotoeditor_core::{Residency, CPU_BUDGET};
use std::io::Write;

#[test]
fn public_photoshop_channels_masks_tree_and_stored_composites_match_independent_oracles() {
    use fotoeditor_core::{BlendMode, CompositePlan, Layer, LayerContent};
    use serde_json::Value;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/psd");
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(root.join("oracle.json")).unwrap()).unwrap();
    fn flatten<'a>(
        nodes: &'a [Layer],
        parent: Option<usize>,
        out: &mut Vec<(&'a Layer, Option<usize>)>,
    ) {
        for layer in nodes {
            let index = out.len();
            out.push((layer, parent));
            if let LayerContent::Group(children) = &layer.content {
                flatten(children, Some(index), out);
            }
        }
    }
    for case in oracle["fixtures"].as_array().unwrap() {
        let name = case["file"].as_str().unwrap();
        let bytes = std::fs::read(root.join(name)).unwrap();
        let mut job = PsdImportJob::new(&bytes, Residency::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        while !job.step().unwrap_or_else(|e| panic!("{name}: {e}")) {}
        let doc = job.finish().unwrap();
        assert_eq!(doc.width, case["width"].as_u64().unwrap() as u32, "{name}");
        assert_eq!(
            doc.height,
            case["height"].as_u64().unwrap() as u32,
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(doc.canvas_rect()).unwrap(),
            case["canvas"],
            "{name}"
        );
        let mut layers = Vec::new();
        flatten(&doc.layers, None, &mut layers);
        let expected = case["layers"].as_array().unwrap();
        assert_eq!(layers.len(), expected.len(), "{name}");
        for ((layer, parent), expected) in layers.iter().zip(expected) {
            assert_eq!(layer.name, expected["name"].as_str().unwrap(), "{name}");
            assert_eq!(
                serde_json::to_value(parent).unwrap(),
                expected["parentIndex"],
                "{name}"
            );
            assert_eq!(
                layer.properties.visible,
                expected["visible"].as_bool().unwrap(),
                "{name}"
            );
            assert_eq!(
                layer.properties.opacity,
                expected["opacity"].as_f64().unwrap() / 255.0,
                "{name}"
            );
            assert_eq!(
                layer.properties.fill,
                expected["fill"].as_f64().unwrap() / 255.0,
                "{name}"
            );
            assert_eq!(
                layer.properties.clipped,
                expected["clipped"].as_bool().unwrap(),
                "{name}"
            );
            let blend = expected["blend"].as_str().unwrap();
            assert_eq!(layer.properties.pass_through, blend == "pass", "{name}");
            assert_eq!(
                layer.properties.blend,
                match blend {
                    "norm" | "pass" => BlendMode::Normal,
                    "mul " => BlendMode::Multiply,
                    _ => panic!("Uncovered corpus blend: {blend}"),
                },
                "{name}"
            );
            if let LayerContent::Raster(surface) = &layer.content {
                let mut crc = crc32fast::Hasher::new();
                for y in 0..doc.height {
                    for x in 0..doc.width {
                        crc.update(&surface.sample(x, y).0);
                    }
                }
                if crc.finalize() != expected["rgbaCrc32"].as_u64().unwrap() as u32 {
                    // Independent LittleCMS and qcms quantize matrix/TRC conversion
                    // differently. Untagged samples and every alpha remain exact.
                    let reference =
                        image::open(root.join(expected["rgbaReference"].as_str().unwrap()))
                            .unwrap()
                            .into_rgba8();
                    let tolerance = case["channelRgbTolerance"].as_u64().unwrap() as u8;
                    for y in 0..doc.height {
                        for x in 0..doc.width {
                            let actual = surface.sample(x, y).0;
                            let reference = reference.get_pixel(x, y).0;
                            assert_eq!(
                                actual[3], reference[3],
                                "{name} {} alpha at {x},{y}",
                                layer.name
                            );
                            assert!(
                                (0..3).all(|c| actual[c].abs_diff(reference[c]) <= tolerance),
                                "{name} {} RGB at {x},{y}: {actual:?} {reference:?}",
                                layer.name
                            );
                        }
                    }
                }
            }
            if let Some(mask) = &layer.mask {
                assert_eq!(
                    mask.enabled,
                    expected["mask"]["enabled"].as_bool().unwrap(),
                    "{name}"
                );
                assert_eq!(
                    u64::from(mask.samples.default.0),
                    expected["mask"]["default"].as_u64().unwrap(),
                    "{name}"
                );
                let mut crc = crc32fast::Hasher::new();
                for y in 0..doc.height {
                    for x in 0..doc.width {
                        crc.update(&[mask.samples.sample(x, y).0]);
                    }
                }
                assert_eq!(
                    crc.finalize(),
                    expected["mask"]["crc32"].as_u64().unwrap() as u32,
                    "{name} {} mask",
                    layer.name
                );
            } else {
                assert!(expected["mask"].is_null(), "{name}");
            }
        }
        let reference = image::open(root.join(case["compositeReference"].as_str().unwrap()))
            .unwrap()
            .into_rgba8();
        let rect = doc.canvas_rect();
        let mut maximum = [0u16; 4];
        for ty in 0..doc.height.div_ceil(256) {
            for tx in 0..doc.width.div_ceil(256) {
                let pixels = CompositePlan::tile(&doc, (tx, ty), None)
                    .unwrap()
                    .reference(Residency::default())
                    .unwrap();
                for (index, pixel) in pixels.iter().enumerate() {
                    let x = tx * 256 + index as u32 % 256;
                    let y = ty * 256 + index as u32 / 256;
                    if !rect.contains(x, y) {
                        continue;
                    }
                    let expected = reference.get_pixel(x - rect.x, y - rect.y).0;
                    let actual = pixel.straight().0;
                    let alpha = u32::from(actual[3]);
                    for channel in 0..3 {
                        let matte = ((u32::from(actual[channel]) * alpha
                            + 255 * (255 - alpha)
                            + 127)
                            / 255) as u16;
                        maximum[channel] =
                            maximum[channel].max(matte.abs_diff(u16::from(expected[channel])));
                    }
                    maximum[3] =
                        maximum[3].max(u16::from(actual[3]).abs_diff(u16::from(expected[3])));
                }
            }
        }
        assert!(
            maximum[..3].iter().all(|v| *v <= 2) && maximum[3] <= 1,
            "{name} Photoshop stored matte/alpha: {maximum:?}"
        );
    }
}

fn section(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(data);
    out
}
fn plane(compression: u16, samples: &[u8]) -> Vec<u8> {
    let mut out = compression.to_be_bytes().to_vec();
    match compression {
        0 => out.extend(samples),
        1 => {
            out.extend([0, 3, 0, 3]);
            for row in samples.as_chunks::<2>().0 {
                out.push(1);
                out.extend(row);
            }
        }
        2 | 3 => {
            let mut encoded = samples.to_vec();
            if compression == 3 {
                for row in encoded.as_chunks_mut::<2>().0 {
                    row[1] = row[1].wrapping_sub(row[0]);
                }
            }
            let mut zip =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            zip.write_all(&encoded).unwrap();
            out.extend(zip.finish().unwrap());
        }
        _ => unreachable!(),
    }
    out
}
fn fixture(compression: u16) -> Vec<u8> {
    let planes: Vec<_> = [
        vec![201, 31, 51, 71],
        vec![3, 33, 53, 73],
        vec![77, 35, 55, 75],
        vec![0, 1, 127, 255],
        vec![0, 37, 128, 255],
    ]
    .iter()
    .map(|v| plane(compression, v))
    .collect();
    let mut record = Vec::new();
    for value in [-1i32, -1, 1, 1] {
        record.extend(value.to_be_bytes());
    }
    record.extend(5u16.to_be_bytes());
    for (id, data) in [0i16, 1, 2, -1, -2].iter().zip(&planes) {
        record.extend(id.to_be_bytes());
        record.extend((data.len() as u32).to_be_bytes());
    }
    record.extend(b"8BIMnorm");
    record.extend([128, 0, 2, 0]);
    let mut mask = Vec::new();
    for value in [-1i32, -1, 1, 1] {
        mask.extend(value.to_be_bytes());
    }
    mask.extend([255, 2, 0, 0]);
    let mut extra = section(&mask);
    extra.extend(section(&[]));
    extra.extend([3, b'R', b'G', b'B']);
    record.extend(section(&extra));
    let mut info = 1i16.to_be_bytes().to_vec();
    info.extend(record);
    for data in planes {
        info.extend(data);
    }
    if !info.len().is_multiple_of(2) {
        info.push(0);
    }
    let mut outer = section(&info);
    outer.extend(section(&[]));
    let mut out = b"8BPS\0\x01\0\0\0\0\0\0\0\x04".to_vec();
    out.extend(1u32.to_be_bytes());
    out.extend(1u32.to_be_bytes());
    out.extend(8u16.to_be_bytes());
    out.extend(3u16.to_be_bytes());
    out.extend(section(&[]));
    out.extend(section(&[]));
    out.extend(section(&outer));
    out.extend([0, 0]);
    out
}

#[test]
fn psd_all_channel_compressions_preserve_hidden_samples_masks_and_negative_bounds() {
    for compression in 0..4 {
        let mut job = PsdImportJob::new(&fixture(compression), Residency::default()).unwrap();
        let mut steps = 0;
        while !job.step().unwrap() {
            steps += 1;
            assert!(steps < 100);
        }
        let doc = job.finish().unwrap();
        assert_eq!((doc.width, doc.height), (2, 2));
        let canvas = doc.canvas_rect();
        assert_eq!(
            (canvas.x, canvas.y, canvas.width, canvas.height),
            (1, 1, 1, 1)
        );
        let layer = &doc.layers[0];
        assert_eq!(layer.name, "RGB");
        assert!(!layer.properties.visible);
        assert_eq!(layer.properties.opacity, 128.0 / 255.0);
        let pixels = layer.raster_samples().unwrap();
        assert_eq!(pixels.sample(0, 0).0, [201, 3, 77, 0]);
        assert_eq!(pixels.sample(1, 0).0, [31, 33, 35, 1]);
        assert_eq!(pixels.sample(0, 1).0, [51, 53, 55, 127]);
        assert_eq!(pixels.sample(1, 1).0, [71, 73, 75, 255]);
        let mask = layer.mask.as_ref().unwrap();
        assert!(!mask.enabled);
        assert_eq!(mask.samples.default.0, 255);
        assert_eq!(
            [
                mask.samples.sample(0, 0).0,
                mask.samples.sample(1, 0).0,
                mask.samples.sample(0, 1).0,
                mask.samples.sample(1, 1).0
            ],
            [0, 37, 128, 255]
        );
    }
}

#[test]
fn psd_truncations_mutations_and_budget_refusal_never_publish_partial_layers() {
    for compression in 0..4 {
        let source = fixture(compression);
        for end in 0..source.len() {
            let result = PsdImportJob::new(&source[..end], Residency::default());
            if let Ok(mut job) = result {
                let result = (|| {
                    while !job.step()? {}
                    job.finish()
                })();
                assert!(result.is_err(), "accepted truncation {compression}:{end}");
            }
        }
        for index in 0..source.len() {
            let mut changed = source.clone();
            changed[index] ^= 0x80;
            if let Ok(mut job) = PsdImportJob::new(&changed, Residency::default()) {
                for _ in 0..100 {
                    match job.step() {
                        Ok(true) => {
                            let _ = job.finish();
                            break;
                        }
                        Ok(false) => {}
                        Err(_) => {
                            assert!(job.step().is_err());
                            assert!(job.finish().is_err());
                            break;
                        }
                    }
                }
            }
        }
        assert!(PsdImportJob::new(
            &source,
            Residency {
                document: CPU_BUDGET,
                ..Residency::default()
            }
        )
        .is_err());
    }
}
