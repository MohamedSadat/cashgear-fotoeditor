//! Prepare the credited NASA photograph at exactly the P5 acceptance dimensions.
//! The delivered JPEG is 6002×4000 despite its download page's 6000×4000 label.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let input = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("bench/results/fixtures/p5-photo.jpg");
    let output = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("bench/results/fixtures/p5-photo.png");
    let image = image::open(input)?;
    if image.width() != 6002 || image.height() != 4000 {
        return Err("Expected NASA PIA22955 delivered at 6002×4000".into());
    }
    let cropped = image.crop_imm(1, 0, 6000, 4000).to_rgb8();
    let mut encoder = png::Encoder::new(std::fs::File::create(output)?, 6000, 4000);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.write_header()?.write_image_data(cropped.as_raw())?;
    println!("6000×4000 RGB8; crop [1,0,6000,4000], no resampling; NASA/JPL-Caltech/IPGP PIA22955");
    Ok(())
}
