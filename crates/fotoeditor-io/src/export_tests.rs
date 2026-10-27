use super::*;
use fotoeditor_core::{CanvasRect, Gray8, LayerMask, TileSurface};
use image::ImageDecoder;
use std::{io::Cursor, sync::Arc};

fn capture(d: &Document, composite: &Document, source: ImageExportSource) -> Vec<u8> {
    let mut job = ExportPixelsJob::new(
        d,
        composite,
        source,
        Some(d.layers[0].id),
        Residency::default(),
    )
    .unwrap();
    while !job.step().unwrap() {}
    job.finish().unwrap()
}
#[test]
fn sources_preserve_hidden_rgb_crop_mask_opacity_and_read_only_samples() {
    let mut d = Document::blank(513, 259, 300.0).unwrap();
    d.canvas = Some(CanvasRect {
        x: 255,
        y: 1,
        width: 257,
        height: 257,
    });
    for y in 0..259 {
        for x in 0..513 {
            d.raster_mut()
                .write(x, y, Rgba8([x as u8, y as u8, 79, 201]));
        }
    }
    let flat = d.clone();
    let mut selection = TileSurface::new(513, 259);
    selection.write(256, 2, Gray8(128));
    d.selection = Some(Arc::new(selection));
    let original = d.raster().sample(256, 2);
    let pixels = capture(&d, &flat, ImageExportSource::Selection);
    assert_eq!(&pixels[4..8], &[0, 1, 79, 0]);
    let at = (257 + 1) * 4;
    assert_eq!(&pixels[at..at + 4], &[0, 2, 79, 101]);
    let mut mask = TileSurface::new(513, 259);
    mask.default = Gray8(128);
    mask.write(256, 2, Gray8(0));
    d.layers[0].mask = Some(LayerMask {
        enabled: true,
        samples: Arc::new(mask),
    });
    d.layers[0].properties.opacity = 0.5;
    d.layers[0].properties.fill = 0.5;
    d.layers[0].properties.visible = false;
    let isolated = capture(&d, &flat, ImageExportSource::Layer);
    assert_eq!(&isolated[..4], &[255, 1, 79, 25]);
    assert_eq!(&isolated[at..at + 4], &[0, 2, 79, 0]);
    let mut png = ExportJob::new_custom(
        &d,
        &flat,
        ImageExportSource::Layer,
        Some(d.layers[0].id),
        false,
        Residency::default(),
    )
    .unwrap();
    while !png.step().unwrap() {}
    let bytes = png.finish().unwrap();
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(&bytes)).unwrap();
    assert!(decoder.icc_profile().unwrap().is_none());
    assert_eq!(
        image::load_from_memory(&bytes)
            .unwrap()
            .into_rgba8()
            .as_raw(),
        &isolated
    );
    assert_eq!(d.raster().sample(256, 2), original);
    assert_eq!(
        (d.canvas, d.layers[0].properties.visible),
        (flat.canvas, false)
    );
    assert!(ExportPixelsJob::new(
        &d,
        &flat,
        ImageExportSource::Full,
        None,
        Residency {
            document: fotoeditor_core::CPU_BUDGET,
            ..Default::default()
        }
    )
    .is_err());
    d.selection = None;
    assert!(ExportPixelsJob::new(
        &d,
        &flat,
        ImageExportSource::Selection,
        None,
        Residency::default()
    )
    .is_err());
    d.selection = Some(Arc::new(TileSurface::new(513, 259)));
    let mut empty = ExportPixelsJob::new(
        &d,
        &flat,
        ImageExportSource::Selection,
        None,
        Residency::default(),
    )
    .unwrap();
    while !empty.step().unwrap() {}
    assert!(empty.finish().unwrap_err().contains("empty"));
    d.layers[0] = fotoeditor_core::Layer::group("Group".into(), Vec::new());
    assert!(ExportPixelsJob::new(
        &d,
        &flat,
        ImageExportSource::Layer,
        Some(d.layers[0].id),
        Residency::default()
    )
    .err()
    .unwrap()
    .contains("raster layer"));
}

#[test]
fn codec_quality_lossless_icc_matte_and_alpha_contracts() {
    let (width, height) = (65, 33);
    let mut rgba = Vec::new();
    for y in 0..height {
        for x in 0..width {
            rgba.extend([
                x as u8 * 3,
                y as u8 * 7,
                79,
                ((x * 37 + y * 19) % 256) as u8,
            ]);
        }
    }
    for format in [ImageExportFormat::Jpeg, ImageExportFormat::Webp] {
        for lossless in [false, true] {
            let options = ImageExportOptions {
                format,
                lossless,
                ..Default::default()
            };
            let bytes = encode_image_rgba(&rgba, width, height, 300.0, &options).unwrap();
            let mut decoder = ImageReader::new(Cursor::new(&bytes))
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            assert_eq!(decoder.icc_profile().unwrap().as_deref(), Some(srgb_icc()));
            let pixels = image::load_from_memory(&bytes).unwrap().into_rgba8();
            if format == ImageExportFormat::Webp {
                assert!(pixels
                    .as_raw()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(rgba.as_chunks::<4>().0)
                    .all(|(a, b)| a[3] == b[3]));
                if lossless {
                    assert_eq!(pixels.as_raw(), &rgba);
                }
            } else {
                assert!(pixels.pixels().all(|p| p.0[3] == 255));
            }
            let untagged = encode_image_rgba(
                &rgba,
                width,
                height,
                300.0,
                &ImageExportOptions {
                    embed_icc: false,
                    ..options.clone()
                },
            )
            .unwrap();
            let mut decoder = ImageReader::new(Cursor::new(&untagged))
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            assert!(decoder.icc_profile().unwrap().is_none());
        }
        let small = encode_image_rgba(
            &rgba,
            width,
            height,
            72.0,
            &ImageExportOptions {
                format,
                quality: 15,
                ..Default::default()
            },
        )
        .unwrap();
        let large = encode_image_rgba(
            &rgba,
            width,
            height,
            72.0,
            &ImageExportOptions {
                format,
                quality: 95,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(small.len() < large.len());
    }
    let transparent = vec![123; 16 * 16 * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|_| [79, 53, 192, 0])
        .collect::<Vec<_>>();
    for matte in [[255; 3], [0; 3]] {
        let options = ImageExportOptions {
            format: ImageExportFormat::Jpeg,
            matte,
            quality: 100,
            ..Default::default()
        };
        let bytes = encode_image_rgba(&transparent, 16, 16, 72.0, &options).unwrap();
        let pixels = image::load_from_memory(&bytes).unwrap().into_rgb8();
        assert!(pixels
            .pixels()
            .all(|p| p.0.iter().zip(matte).all(|(a, b)| a.abs_diff(b) <= 1)));
    }
    assert!(ImageExportOptions {
        quality: 0,
        format: ImageExportFormat::Jpeg,
        ..Default::default()
    }
    .validate()
    .is_err());
    assert!(encode_image_rgba(&[], 1, 1, 72.0, &ImageExportOptions::default()).is_err());
    let wide = vec![179; 16384 * 4];
    let options = ImageExportOptions {
        format: ImageExportFormat::Webp,
        ..Default::default()
    };
    assert!(encode_image_rgba(&wide, 16384, 1, 72.0, &options)
        .unwrap_err()
        .contains("16,383"));
    let encoded = encode_image_rgba(
        &wide,
        16384,
        1,
        72.0,
        &ImageExportOptions {
            lossless: true,
            ..options
        },
    )
    .unwrap();
    assert_eq!(
        image::load_from_memory(&encoded)
            .unwrap()
            .into_rgba8()
            .as_raw(),
        &wide
    );
}
