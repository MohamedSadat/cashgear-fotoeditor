//! Real-photo performance fixture; all image processing stays in Rust.
//! Provenance/license/hash: tests/fixtures/retouch/README.md.
use image::{imageops, ImageEncoder};
use std::{fs, io::BufWriter};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = image::open("tests/fixtures/retouch/melissa-diaz-usaf.jpg")?.to_rgba8();
    assert_eq!(source.dimensions(), (1905, 2889));
    // Preserve aspect ratio: a 1905x1270 face/neck crop, enlarged with Lanczos3.
    let crop = imageops::crop_imm(&source, 0, 600, 1905, 1270).to_image();
    let output = imageops::resize(&crop, 6000, 4000, imageops::FilterType::Lanczos3);
    fs::create_dir_all("bench/results/fixtures")?;
    image::codecs::png::PngEncoder::new(BufWriter::new(fs::File::create(
        "bench/results/fixtures/retouch-photo-6000x4000.png",
    )?))
    .write_image(&output, 6000, 4000, image::ExtendedColorType::Rgba8)?;
    Ok(())
}
