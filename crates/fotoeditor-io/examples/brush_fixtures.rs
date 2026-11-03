//! Original procedural image files for exercising real brush-asset imports.
use image::{ImageBuffer, Rgba};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new("bench/results/fixtures");
    std::fs::create_dir_all(root)?;
    let tip = ImageBuffer::from_fn(64, 64, |x, y| {
        let x = f64::from(x) - 31.5;
        let y = f64::from(y) - 31.5;
        let leaf = ((x + 7.0 + 0.35 * y) / 13.0).powi(2) + (y / 26.0).powi(2);
        let stem = (x - 0.2 * y).abs() < 1.5 && y.abs() < 30.0;
        Rgba([0_u8, 0, 0, if leaf < 1.0 || stem { 255 } else { 0 }])
    });
    tip.save(root.join("brush-leaf.png"))?;
    let mut random = 7_u32;
    let texture = ImageBuffer::from_fn(32, 32, |x, y| {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        let c = if (x / 4 + y / 4) % 2 == 0 {
            40 + (random % 100) as u8
        } else {
            170 + (random % 70) as u8
        };
        Rgba([c, c, c, 255_u8])
    });
    texture.save(root.join("brush-texture.png"))?;
    let opaque = image::DynamicImage::ImageRgba8(texture);
    opaque.to_rgb8().save(root.join("brush-texture.jpg"))?;
    opaque.save(root.join("brush-texture.webp"))?;
    let mut oracle = ImageBuffer::new(3, 2);
    for (i, p) in oracle.pixels_mut().enumerate() {
        *p = if [0, 3, 4].contains(&i) {
            Rgba([0_u8, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        };
    }
    oracle.save(root.join("brush-asymmetric.png"))?;
    ImageBuffer::from_pixel(513, 1, Rgba([0_u8, 0, 0, 255]))
        .save(root.join("brush-oversized.png"))?;
    Ok(())
}
