//! Deterministic distinct dense v2 workload, generated with one tile buffer.
//! cargo run --release -p fotoeditor-io --example layers -- FILE WIDTH HEIGHT LAYERS
use fotoeditor_core::{LayerProperties, Residency, TILE_PIXELS, TILE_SIZE};
use fotoeditor_io::{ProjectRecordKind, ProjectStreamWriter};
use std::io::{BufWriter, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("Expected FILE WIDTH HEIGHT LAYERS".into());
    }
    let width: u32 = args[2].parse()?;
    let height: u32 = args[3].parse()?;
    let count: u32 = args[4].parse()?;
    fotoeditor_core::validate_size(width, height, 72.0)?;
    if !(1..=50).contains(&count) {
        return Err("Choose 1..50 dense layers".into());
    }
    let mut generation = 0u64;
    let mut layers = Vec::new();
    for layer in 0..count {
        let mut tiles = Vec::new();
        for x in 0..width.div_ceil(TILE_SIZE) {
            for y in 0..height.div_ceil(TILE_SIZE) {
                generation += 1;
                tiles.push(serde_json::json!({"x":x,"y":y,"generation":generation}));
            }
        }
        layers.push(serde_json::json!({"id":layer+1,"name":format!("Dense layer {:02}",layer+1),"properties":LayerProperties::default(),"mask":null,"content":{"kind":"raster","tiles":tiles}}));
    }
    let manifest = serde_json::to_vec(
        &serde_json::json!({"width":width,"height":height,"dpi":72.0,"blend_space":"srgb","layers":layers}),
    )?;
    let mut stream = ProjectStreamWriter::new(Residency::default())?;
    let mut output = BufWriter::with_capacity(1048576, std::fs::File::create(&args[1])?);
    output.write_all(stream.signature())?;
    output.write_all(&stream.record(ProjectRecordKind::Manifest, 0, &manifest)?)?;
    generation = 0;
    for layer in 0..count {
        for x in 0..width.div_ceil(TILE_SIZE) {
            for y in 0..height.div_ceil(TILE_SIZE) {
                generation += 1;
                let mut pixels = Vec::with_capacity(TILE_PIXELS * 4);
                for i in 0..TILE_PIXELS {
                    let px = x * TILE_SIZE + i as u32 % TILE_SIZE;
                    let py = y * TILE_SIZE + i as u32 / TILE_SIZE;
                    let mut value = px.wrapping_mul(0x45d9f3b)
                        ^ py.wrapping_mul(0x119de1f3)
                        ^ (layer + 1).wrapping_mul(0x9e3779b9);
                    value ^= value >> 16;
                    value = value.wrapping_mul(0x85ebca6b);
                    value ^= value >> 13;
                    let alpha = if (px + py + layer).is_multiple_of(97) {
                        0
                    } else {
                        64 + (value % 192) as u8
                    };
                    pixels.extend_from_slice(&[
                        (value >> 24) as u8,
                        (value >> 16) as u8,
                        (value >> 8) as u8,
                        alpha,
                    ]);
                }
                output.write_all(&stream.record(
                    ProjectRecordKind::RgbaTile,
                    generation,
                    &pixels,
                )?)?;
            }
        }
    }
    output.write_all(&stream.finish()?)?;
    output.flush()?;
    println!(
        "{} distinct dense layers, {}x{}, {} records, {} bytes; seed 0x9e3779b9",
        count,
        width,
        height,
        generation,
        std::fs::metadata(&args[1])?.len()
    );
    Ok(())
}
