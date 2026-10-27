//! Small deterministic independent-decoder inputs; no expected output uses a self-roundtrip.
use fotoeditor_io::{encode_image_rgba, srgb_icc, ImageExportFormat, ImageExportOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output =
        std::path::PathBuf::from(std::env::args().nth(1).ok_or("Expected output directory")?);
    std::fs::create_dir_all(&output)?;
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
    std::fs::write(output.join("source.rgba"), &rgba)?;
    std::fs::write(output.join("srgb.icc"), srgb_icc())?;
    for (format, extension, lossless) in [
        (ImageExportFormat::Jpeg, "jpg", false),
        (ImageExportFormat::Webp, "lossy.webp", false),
        (ImageExportFormat::Webp, "lossless.webp", true),
    ] {
        for tagged in [false, true] {
            let options = ImageExportOptions {
                format,
                lossless,
                embed_icc: tagged,
                ..Default::default()
            };
            let bytes = encode_image_rgba(&rgba, width, height, 300.0, &options)?;
            std::fs::write(
                output.join(format!(
                    "{}.{extension}",
                    if tagged { "tagged" } else { "untagged" }
                )),
                bytes,
            )?;
        }
    }
    Ok(())
}
