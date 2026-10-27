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

#[test]
fn bmp_v5_profiles_convert_straight_rgb_and_refuse_ambiguous_color_metadata() {
    let make = |profile: &[u8]| {
        let mut bytes = vec![0u8; 138];
        bytes[..2].copy_from_slice(b"BM");
        for (at, word) in [
            (10, 138u32),
            (14, 124),
            (18, 2),
            (22, (-2i32) as u32),
            (30, 3),
            (34, 16),
            (54, 0x00ff0000),
            (58, 0x0000ff00),
            (62, 0x000000ff),
            (66, 0xff000000),
            (70, 0x4d424544),
            (122, 2),
            (126, 140),
            (130, profile.len() as u32),
        ] {
            bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&32u16.to_le_bytes());
        for alpha in [0, 51, 128, 255] {
            bytes.extend([127, 51, 79, alpha]);
        }
        bytes.extend(profile);
        let size = bytes.len() as u32;
        bytes[2..6].copy_from_slice(&size.to_le_bytes());
        bytes
    };
    let canonical = make(srgb_icc());
    let document = decode(&canonical).unwrap();
    for (at, alpha) in [0, 51, 128, 255].into_iter().enumerate() {
        assert_eq!(
            document.raster().sample(at as u32 % 2, at as u32 / 2),
            Rgba8([79, 51, 127, alpha])
        );
    }
    let mut linear = srgb_icc().to_vec();
    let tag = (132..linear.len())
        .step_by(12)
        .find(|at| &linear[*at..*at + 4] == b"rTRC")
        .unwrap();
    let curve = u32::from_be_bytes(linear[tag + 4..tag + 8].try_into().unwrap()) as usize;
    for i in 0..4096 {
        linear[curve + 12 + i * 2..curve + 14 + i * 2]
            .copy_from_slice(&((i as f64 / 4095.0 * 65535.0).round() as u16).to_be_bytes());
    }
    let converted = decode(&make(&linear)).unwrap();
    for (at, alpha) in [0, 51, 128, 255].into_iter().enumerate() {
        let pixel = converted.raster().sample(at as u32 % 2, at as u32 / 2).0;
        assert_eq!(pixel[3], alpha);
        for (actual, expected) in pixel[..3].iter().zip([151u8, 124, 187]) {
            assert!(actual.abs_diff(expected) <= 2, "{pixel:?}");
        }
    }
    for (at, value) in [
        (126, u32::MAX),
        (126, 124),
        (130, 0),
        (130, 4 * 1024 * 1024 + 1),
        (70, 0),
        (70, 0x4c494e4b),
        (134, 1),
    ] {
        let mut invalid = canonical.clone();
        invalid[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(decode(&invalid).is_err(), "{at}/{value}");
    }
    assert!(decode(&canonical[..canonical.len() - 1]).is_err());
    assert!(decode(&canonical[..137]).is_err());
    assert!(decode(&make(b"invalid ICC profile"))
        .err()
        .unwrap()
        .contains("ICC"));
}

#[test]
fn associated_tiff_becomes_straight_before_color_conversion() {
    // image's TIFF wrapper retains premultiplied samples. Append a new IFD
    // specifying AssociatedAlpha; its existing strip/profile offsets stay valid.
    let make = |profile: Option<Vec<u8>>| {
        let mut out = Cursor::new(Vec::new());
        let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut out);
        if let Some(profile) = profile {
            encoder.set_icc_profile(profile).unwrap();
        }
        encoder
            .write_image(
                &[32, 16, 8, 128, 8, 4, 2, 64, 7, 9, 11, 0],
                3,
                1,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        let mut bytes = out.into_inner();
        assert_eq!(&bytes[..4], b"II\x2a\0");
        let at = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let count = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
        let mut entries: Vec<[u8; 12]> = bytes[at + 2..at + 2 + count * 12]
            .as_chunks::<12>()
            .0
            .to_vec();
        entries.retain(|e| u16::from_le_bytes(e[..2].try_into().unwrap()) != 338);
        let mut alpha = [0u8; 12];
        alpha[..2].copy_from_slice(&338u16.to_le_bytes());
        alpha[2..4].copy_from_slice(&3u16.to_le_bytes());
        alpha[4..8].copy_from_slice(&1u32.to_le_bytes());
        alpha[8..10].copy_from_slice(&1u16.to_le_bytes());
        entries.push(alpha);
        entries.sort_by_key(|e| u16::from_le_bytes(e[..2].try_into().unwrap()));
        let directory = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&directory.to_le_bytes());
        bytes.extend((entries.len() as u16).to_le_bytes());
        for entry in entries {
            bytes.extend(entry);
        }
        bytes.extend([0u8; 4]);
        bytes
    };
    let doc = decode(&make(None)).unwrap();
    assert_eq!(doc.raster().sample(0, 0), Rgba8([64, 32, 16, 128]));
    assert_eq!(doc.raster().sample(1, 0), Rgba8([32, 16, 8, 64]));
    assert_eq!(doc.raster().sample(2, 0), Rgba8([7, 9, 11, 0]));
    let mut unassociated = make(None);
    let directory = u32::from_le_bytes(unassociated[4..8].try_into().unwrap()) as usize;
    let count =
        u16::from_le_bytes(unassociated[directory..directory + 2].try_into().unwrap()) as usize;
    let alpha = (0..count)
        .map(|i| directory + 2 + i * 12)
        .find(|at| u16::from_le_bytes(unassociated[*at..*at + 2].try_into().unwrap()) == 338)
        .unwrap();
    unassociated[alpha + 8..alpha + 10].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        decode(&unassociated).unwrap().raster().sample(0, 0),
        Rgba8([32, 16, 8, 128])
    );
    unassociated[alpha + 8..alpha + 10].copy_from_slice(&3u16.to_le_bytes());
    assert!(decode(&unassociated)
        .err()
        .unwrap()
        .contains("extra sample"));
    let mut profile = srgb_icc().to_vec();
    let tag = (132..profile.len())
        .step_by(12)
        .find(|at| &profile[*at..*at + 4] == b"rTRC")
        .unwrap();
    let curve = u32::from_be_bytes(profile[tag + 4..tag + 8].try_into().unwrap()) as usize;
    for i in 0..4096 {
        profile[curve + 12 + i * 2..curve + 14 + i * 2]
            .copy_from_slice(&((i as f64 / 4095.0 * 65535.0).round() as u16).to_be_bytes());
    }
    let encoded = make(Some(profile));
    let converted = decode(&encoded).unwrap();
    let pixel = converted.raster().sample(0, 0).0;
    assert_eq!(pixel[3], 128);
    for (actual, expected) in pixel[..3].iter().zip([137u8, 99, 71]) {
        assert!(actual.abs_diff(expected) <= 2, "{pixel:?}");
    }
}
