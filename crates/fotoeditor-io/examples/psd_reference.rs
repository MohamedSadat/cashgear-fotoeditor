//! Export decoded channels and the CPU composite for independent corpus comparison.
use fotoeditor_core::{CompositePlan, Document, Layer, LayerContent, Residency, Rgba8};
use fotoeditor_io::PsdImportJob;
use std::path::Path;
fn layers(
    doc: &Document,
    nodes: &[Layer],
    out: &Path,
    index: &mut usize,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut result = Vec::new();
    for layer in nodes {
        let i = *index;
        *index += 1;
        let mut item =
            serde_json::json!({"name":layer.name,"properties":layer.properties,"index":i});
        match &layer.content {
            LayerContent::Raster(samples) => {
                let image = image::RgbaImage::from_fn(doc.width, doc.height, |x, y| {
                    image::Rgba(samples.sample(x, y).0)
                });
                image.save(out.join(format!("layer-{i}.png")))?;
            }
            LayerContent::Group(children) => {
                item["children"] = layers(doc, children, out, index)?;
            }
            _ => unreachable!(),
        }
        if let Some(mask) = &layer.mask {
            item["mask"] =
                serde_json::json!({"enabled":mask.enabled,"default":mask.samples.default.0});
            let image = image::GrayImage::from_fn(doc.width, doc.height, |x, y| {
                image::Luma([mask.samples.sample(x, y).0])
            });
            image.save(out.join(format!("mask-{i}.png")))?;
        }
        result.push(item);
    }
    Ok(serde_json::Value::Array(result))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err("Expected OUTPUT_DIRECTORY PSD...".into());
    }
    for file in &args[2..] {
        let name = Path::new(file).file_stem().unwrap();
        let out = Path::new(&args[1]).join(name);
        std::fs::create_dir_all(&out)?;
        let result = (|| -> Result<_, Box<dyn std::error::Error>> {
            let input = std::fs::read(file)?;
            let mut job = PsdImportJob::new(&input, Residency::default())?;
            let mut steps = 0;
            while !job.step()? {
                steps += 1;
            }
            let doc = job.finish()?;
            let mut index = 0;
            let layers = layers(&doc, &doc.layers, &out, &mut index)?;
            let rect = doc.canvas_rect();
            let mut pixels = image::RgbaImage::new(rect.width, rect.height);
            for ty in 0..doc.height.div_ceil(256) {
                for tx in 0..doc.width.div_ceil(256) {
                    let tile = CompositePlan::tile(&doc, (tx, ty), None)?
                        .reference(Residency::default())?;
                    for (i, pixel) in tile.iter().enumerate() {
                        let x = tx * 256 + i as u32 % 256;
                        let y = ty * 256 + i as u32 / 256;
                        if rect.contains(x, y) {
                            let Rgba8(rgba) = pixel.straight();
                            pixels.put_pixel(x - rect.x, y - rect.y, image::Rgba(rgba));
                        }
                    }
                }
            }
            pixels.save(out.join("composite.png"))?;
            Ok(
                serde_json::json!({"file":file,"steps":steps,"width":doc.width,"height":doc.height,"canvas":rect,"dpi":doc.dpi,"bytes":doc.bytes(),"layers":layers}),
            )
        })();
        let report = match result {
            Ok(value) => value,
            Err(e) => serde_json::json!({"file":file,"error":e.to_string()}),
        };
        std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
        println!("{report}");
    }
    Ok(())
}
