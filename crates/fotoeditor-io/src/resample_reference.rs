//! Independent image 0.25.10 float-kernel Lanczos3 reference; opaque RGBA fixture
//! avoids conflating independent interpolation with alpha convention differences.
use fotoeditor_core::*;
use std::sync::Arc;
#[test]
fn half_size_lanczos3_matches_independent_image_reference_within_two() {
    let (w, h) = (1030, 518);
    let image = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([
            ((x * 13 + y * 7 + x * y % 31) % 256) as u8,
            ((x * 3 + y * 37 + (x ^ y) * 11) % 256) as u8,
            ((x * 19 + y * 17 + x * y % 53) % 256) as u8,
            255,
        ])
    });
    let mut source = TileSurface::new(w, h);
    for ty in 0..h.div_ceil(TILE_SIZE) {
        for tx in 0..w.div_ceil(TILE_SIZE) {
            let mut tile = vec![Rgba8::default(); TILE_PIXELS];
            for y in 0..TILE_SIZE.min(h - ty * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(w - tx * TILE_SIZE) {
                    tile[(y * TILE_SIZE + x) as usize] =
                        Rgba8(image.get_pixel(tx * TILE_SIZE + x, ty * TILE_SIZE + y).0);
                }
            }
            source.insert((tx, ty), tile);
        }
    }
    let mut job = SurfaceResampleJob::new(
        Arc::new(source),
        w / 2,
        h / 2,
        TransformMatrix::scale(0.5, 0.5),
        Interpolation::Lanczos3,
        true,
        CPU_BUDGET,
    )
    .unwrap();
    while !job.step().unwrap() {}
    let actual = job.finish().unwrap();
    let reference =
        image::imageops::resize(&image, w / 2, h / 2, image::imageops::FilterType::Lanczos3);
    let mut worst = [0_u8; 4];
    for (x, y, p) in reference.enumerate_pixels() {
        let a = actual.sample(x, y).0;
        for c in 0..4 {
            worst[c] = worst[c].max(a[c].abs_diff(p.0[c]));
        }
    }
    eprintln!("P7 50% Lanczos3 vs image 0.25.10 opaque RGBA 1030x518 max channel error: {worst:?}");
    assert!(
        worst.into_iter().all(|v| v <= 2),
        "max channel error {worst:?}"
    );
}
