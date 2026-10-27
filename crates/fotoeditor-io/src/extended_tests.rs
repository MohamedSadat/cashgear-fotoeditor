use crate::{srgb_icc, ImportJob};
use fotoeditor_core::{Residency, Rgba8, CPU_BUDGET};
use image::{ImageEncoder, ImageFormat};
use std::io::Cursor;

fn decode(bytes: &[u8]) -> Result<fotoeditor_core::Document, String> {
    let mut job = ImportJob::decode(bytes, Residency::default())?;
    while !job.step() {}
    Ok(job.document)
}

#[test]
fn bmp_and_tiff_imports_cross_tiles_and_preserve_supported_samples() {
    let rgba = image::RgbaImage::from_fn(257, 259, |x, y| {
        image::Rgba([x as u8, y as u8, 79, ((x + y) % 256) as u8])
    });
    for format in [ImageFormat::Bmp, ImageFormat::Tiff] {
        let mut output = Cursor::new(Vec::new());
        let image = if format == ImageFormat::Bmp {
            image::DynamicImage::ImageRgb8(
                image::DynamicImage::ImageRgba8(rgba.clone()).into_rgb8(),
            )
        } else {
            image::DynamicImage::ImageRgba8(rgba.clone())
        };
        image.write_to(&mut output, format).unwrap();
        let doc = decode(output.get_ref()).unwrap();
        assert_eq!((doc.width, doc.height), (257, 259));
        for y in 0..259 {
            for x in 0..257 {
                let mut expected = rgba.get_pixel(x, y).0;
                if format == ImageFormat::Bmp {
                    expected[3] = 255;
                }
                assert_eq!(
                    doc.raster().sample(x, y),
                    Rgba8(expected),
                    "{format:?}: {x},{y}"
                );
            }
        }
    }
}

#[test]
fn gif_import_uses_first_frame_and_high_depth_tiff_is_rejected() {
    let mut output = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut output);
        encoder
            .encode_frames([
                image::Frame::new(image::RgbaImage::from_pixel(
                    3,
                    2,
                    image::Rgba([221, 31, 7, 255]),
                )),
                image::Frame::new(image::RgbaImage::from_pixel(
                    3,
                    2,
                    image::Rgba([1, 201, 4, 255]),
                )),
            ])
            .unwrap();
    }
    let doc = decode(&output).unwrap();
    assert_eq!((doc.width, doc.height), (3, 2));
    assert_eq!(doc.raster().sample(0, 0), Rgba8([221, 31, 7, 255]));
    let mut tiff = Cursor::new(Vec::new());
    image::codecs::tiff::TiffEncoder::new(&mut tiff)
        .write_image(&[0; 8], 1, 1, image::ExtendedColorType::Rgba16)
        .unwrap();
    assert!(decode(tiff.get_ref()).err().unwrap().contains("16-bit"));
}

#[test]
fn tagged_png_is_exact_invalid_profile_is_not_silently_assigned_and_admission_is_early() {
    let make = |profile: &[u8]| {
        let mut output = Vec::new();
        let mut info = png::Info::with_size(1, 1);
        info.color_type = png::ColorType::Rgba;
        info.icc_profile = Some(std::borrow::Cow::Borrowed(profile));
        png::Encoder::with_info(&mut output, info)
            .unwrap()
            .write_header()
            .unwrap()
            .write_image_data(&[87, 33, 19, 0])
            .unwrap();
        output
    };
    let bytes = make(srgb_icc());
    assert_eq!(
        decode(&bytes).unwrap().raster().sample(0, 0),
        Rgba8([87, 33, 19, 0])
    );
    assert!(decode(&make(b"invalid profile"))
        .err()
        .unwrap()
        .contains("ICC"));
    let full = Residency {
        document: CPU_BUDGET,
        ..Residency::default()
    };
    assert!(ImportJob::decode(&bytes, full).is_err());
}
