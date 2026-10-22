//! Deterministic acceptance images; never generated on the UI thread.
use image::ImageEncoder;
use std::{fs, io::BufWriter, path::Path};
fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bench/results/fixtures".into());
    fs::create_dir_all(&root).unwrap();
    for (name, w, h) in [("alpha", 513u32, 259u32), ("large", 6000, 4000)] {
        let mut samples = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                let p = if name == "alpha" {
                    if y == 0 {
                        [217, 33, 97, (x % 256) as u8]
                    } else {
                        match (x < w / 2, y < h / 2) {
                            (true, true) => [255, 0, 0, 255],
                            (false, true) => [0, 255, 0, 128],
                            (true, false) => [0, 0, 255, 255],
                            _ => [255, 255, 255, 255],
                        }
                    }
                } else {
                    // Smooth color field with deterministic texture: roughly 17 MiB PNG.
                    let noise = ((x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263))
                        .wrapping_mul(1274126177)
                        >> 27) as u8;
                    [
                        ((x * 223 / w) as u8).saturating_add(noise),
                        ((y * 223 / h) as u8).saturating_add(noise),
                        ((x + y) % 192) as u8 + noise,
                        255,
                    ]
                };
                samples.extend_from_slice(&p);
            }
        }
        let file =
            BufWriter::new(fs::File::create(Path::new(&root).join(format!("{name}.png"))).unwrap());
        image::codecs::png::PngEncoder::new(file)
            .write_image(&samples, w, h, image::ExtendedColorType::Rgba8)
            .unwrap();
        if name == "alpha" {
            image::codecs::webp::WebPEncoder::new_lossless(BufWriter::new(
                fs::File::create(Path::new(&root).join("sample.webp")).unwrap(),
            ))
            .write_image(&samples, w, h, image::ExtendedColorType::Rgba8)
            .unwrap();
            let rgb: Vec<u8> = samples
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            image::codecs::jpeg::JpegEncoder::new_with_quality(
                BufWriter::new(fs::File::create(Path::new(&root).join("sample.jpg")).unwrap()),
                90,
            )
            .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        }
    }
}
