//! Validate a real streamed project without materializing its pixel buffers.
use fotoeditor_core::Residency;
use fotoeditor_io::LayerProjectReader;
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Expected project filename")?;
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut reader = LayerProjectReader::new(length, 1, Residency::default())?;
    let mut bytes = vec![0; 1048576];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        let mut used = 0;
        while used < n {
            used += reader.feed(&bytes[used..n])?;
        }
        total += n as u64;
    }
    let document = reader.finish()?;
    println!(
        "Validated {total} bytes, {} layers, {} tiles, {} resident sample bytes",
        document.layers.len(),
        document.tile_count(),
        document.bytes()
    );
    Ok(())
}
