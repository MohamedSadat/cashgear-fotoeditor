//! Bounded 8-bit brush-asset decoder. Assets are coverage, never document samples.
use image::{ImageDecoder, ImageReader};
use std::io::Cursor;

pub const BRUSH_IMAGE_RESERVE: u64 = 32 * 1024 * 1024;
pub const MAX_BRUSH_IMAGE_BYTES: usize = 4 * 1024 * 1024;
#[derive(serde::Serialize)]
pub struct BrushImage {
    pub width: u32,
    pub height: u32,
    pub coverage: String,
}
pub fn decode_brush_image(
    bytes: &[u8],
    residency: fotoeditor_core::Residency,
) -> Result<BrushImage, String> {
    if bytes.len() > MAX_BRUSH_IMAGE_BYTES {
        return Err("Brush images must be at most 4 MiB".into());
    }
    // Includes input/bridge copies, bounded decoder workspace, RGBA conversion,
    // coverage and UTF-8/UTF-16 return strings simultaneously.
    residency.check_cpu(BRUSH_IMAGE_RESERVE)?;
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err("Brush images must be PNG, JPEG or WebP".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > 512
        || height > 512
        || decoder.color_type().bits_per_pixel() / u16::from(decoder.color_type().channel_count())
            > 8
    {
        return Err("Use an 8-bit brush image at most 512 x 512 pixels".into());
    }
    let rgba = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let mut coverage = String::with_capacity((width * height * 2) as usize);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for p in rgba.pixels() {
        let luma = (u32::from(p[0]) * 2126 + u32::from(p[1]) * 7152 + u32::from(p[2]) * 722 + 5000)
            / 10000;
        let c = ((255 - luma) * u32::from(p[3]) + 127) / 255;
        coverage.push(HEX[(c >> 4) as usize] as char);
        coverage.push(HEX[(c & 15) as usize] as char);
    }
    Ok(BrushImage {
        width,
        height,
        coverage,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn png(w: u32, h: u32, p: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        {
            let mut e = png::Encoder::new(&mut b, w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(p).unwrap();
        }
        b
    }
    #[test]
    fn coverage_luminance_alpha_hidden_rgb_and_predecode_limits() {
        let bytes = png(
            5,
            1,
            &[
                0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 128, 53, 217, 91, 0, 255, 0, 0, 255,
            ],
        );
        let out = decode_brush_image(&bytes, Default::default()).unwrap();
        // Independent Rec.709 red luminance rounds to 54, inverse = 201.
        assert_eq!(out.coverage, "ff008000c9");
        assert_eq!((out.width, out.height), (5, 1));
        assert!(decode_brush_image(&png(513, 1, &vec![0; 513 * 4]), Default::default()).is_err());
        assert!(decode_brush_image(b"not an image", Default::default()).is_err());
        assert!(
            decode_brush_image(&vec![0; MAX_BRUSH_IMAGE_BYTES + 1], Default::default()).is_err()
        );
        assert!(decode_brush_image(
            &bytes,
            fotoeditor_core::Residency {
                document: fotoeditor_core::CPU_BUDGET - 1,
                ..Default::default()
            }
        )
        .is_err());
    }
}
