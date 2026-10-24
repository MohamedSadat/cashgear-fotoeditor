//! Single-thread kernel sanity benchmark; browser measurements are acceptance.
use fotoeditor_core::*;
use fotoeditor_filters::*;
fn main() {
    let mut d = Document::blank(6000, 4000, 300.0).unwrap();
    for y in 0..4000u32.div_ceil(256) {
        for x in 0..6000u32.div_ceil(256) {
            d.raster_mut().insert(
                (x, y),
                (0..TILE_PIXELS)
                    .map(|i| Rgba8([(i * 7) as u8, (i * 13) as u8, (i * 31) as u8, 255]))
                    .collect(),
            );
        }
    }
    let start = std::time::Instant::now();
    let mut job = FilterJob::new(
        &d,
        d.layers[0].id,
        Filter::Gaussian { radius: 20 },
        512 << 20,
    )
    .unwrap();
    while !job.step().unwrap() {}
    let command = job.finish().unwrap();
    println!(
        "Gaussian20 6000x4000: {:?}, empty={}",
        start.elapsed(),
        command.is_empty()
    );
}
