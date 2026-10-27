//! Reproducible P8 format/ICC inputs and native output for browser regression.
use fotoeditor_core::Residency;
use fotoeditor_io::{srgb_icc, ExportJob, ImportJob};
use std::{io::Cursor, path::Path};

fn tagged_png(profile: &[u8], pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut info = png::Info::with_size(width, height);
    info.color_type = png::ColorType::Rgba;
    info.icc_profile = Some(std::borrow::Cow::Borrowed(profile));
    png::Encoder::with_info(&mut out, info)
        .unwrap()
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
    out
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bench/results/fixtures".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir)?;
    let pixels = image::RgbaImage::from_fn(257, 259, |x, y| {
        image::Rgba([x as u8, y as u8, 79, ((x + y) % 256) as u8])
    });
    for (extension, format) in [
        ("bmp", image::ImageFormat::Bmp),
        ("tiff", image::ImageFormat::Tiff),
    ] {
        let mut output = Cursor::new(Vec::new());
        let image = if format == image::ImageFormat::Bmp {
            image::DynamicImage::ImageRgb8(
                image::DynamicImage::ImageRgba8(pixels.clone()).into_rgb8(),
            )
        } else {
            image::DynamicImage::ImageRgba8(pixels.clone())
        };
        image.write_to(&mut output, format)?;
        std::fs::write(dir.join(format!("p8-input.{extension}")), output.get_ref())?;
    }
    let mut gif = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
        encoder.encode_frames([
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
        ])?;
    }
    std::fs::write(dir.join("p8-input.gif"), gif)?;
    let mut linear = srgb_icc().to_vec();
    // Replace the shared sampled TRC with identity, making linear-light sRGB
    // primaries; this is a test profile, not a copied third-party ICC asset.
    let mut at = 132;
    while &linear[at..at + 4] != b"rTRC" {
        at += 12;
    }
    let curve = u32::from_be_bytes(linear[at + 4..at + 8].try_into().unwrap()) as usize;
    for i in 0..4096 {
        linear[curve + 12 + i * 2..curve + 14 + i * 2]
            .copy_from_slice(&((i as f64 / 4095.0 * 65535.0).round() as u16).to_be_bytes());
    }
    std::fs::write(dir.join("p8-srgb.icc"), srgb_icc())?;
    std::fs::write(dir.join("p8-linear.icc"), &linear)?;
    std::fs::write(
        dir.join("p8-icc-linear.png"),
        tagged_png(&linear, pixels.as_raw(), 257, 259),
    )?;
    std::fs::write(
        dir.join("p8-invalid-icc.png"),
        tagged_png(b"invalid ICC profile", &[79, 31, 3, 0], 1, 1),
    )?;
    let mut high = Vec::new();
    let mut encoder = png::Encoder::new(&mut high, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder.write_header()?.write_image_data(&[0; 8])?;
    std::fs::write(dir.join("p8-16bit.png"), high)?;
    for name in [
        "p8-input.bmp",
        "p8-input.tiff",
        "p8-input.gif",
        "p8-icc-linear.png",
    ] {
        let bytes = std::fs::read(dir.join(name))?;
        let mut import = ImportJob::decode(&bytes, Residency::default())?;
        while !import.step() {}
        let mut export = ExportJob::new(&import.document, Residency::default())?;
        while !export.step()? {}
        std::fs::write(dir.join(format!("{name}.expected.png")), export.finish()?)?;
    }
    // A small independently inspectable ZIP, with retained outside pixels.
    use std::io::Write;
    let input = std::fs::read(dir.join("p8-input.tiff"))?;
    let mut import = ImportJob::decode(&input, Residency::default())?;
    while !import.step() {}
    import.document.canvas = Some(fotoeditor_core::CanvasRect {
        x: 1,
        y: 2,
        width: 255,
        height: 256,
    });
    import.document.guides = vec![fotoeditor_core::Guide {
        axis: fotoeditor_core::GuideAxis::Vertical,
        position: 128.5,
    }];
    let mut project = fotoeditor_io::ZipProjectWriter::new(
        &import.document,
        &import.document,
        Residency::default(),
    )?;
    let mut output = std::io::BufWriter::new(std::fs::File::create(dir.join("p8-native.cgfoto"))?);
    while let Some(bytes) = project.next_packet(Residency::default())? {
        output.write_all(&bytes)?;
    }
    output.flush()?;
    Ok(())
}
