//! Reproducible Gray8 masks for the still-pending Photoshop feather comparison.
use fotoeditor_core::{Gray8, TileSurface, CPU_BUDGET};
use fotoeditor_select::{MaskJob, MaskOperation, SelectionMode, SelectionShape, ShapeSelectionJob};
use image::ImageEncoder;
use std::{fs, io::BufWriter, path::Path, sync::Arc};

fn write(root: &Path, name: &str, mask: &TileSurface<Gray8>) {
    let samples: Vec<_> = (0..mask.height)
        .flat_map(|y| (0..mask.width).map(move |x| mask.sample(x, y).0))
        .collect();
    image::codecs::png::PngEncoder::new(BufWriter::new(fs::File::create(root.join(name)).unwrap()))
        .write_image(
            &samples,
            mask.width,
            mask.height,
            image::ExtendedColorType::L8,
        )
        .unwrap();
}
fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bench/results/feather".into());
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    let mut selection = None;
    for shape in [
        SelectionShape::Rectangle {
            x0: 0.0,
            y0: 80.0,
            x1: 260.0,
            y1: 240.0,
        },
        SelectionShape::Ellipse {
            x0: 300.0,
            y0: 40.0,
            x1: 490.0,
            y1: 280.0,
        },
    ] {
        let mut job = ShapeSelectionJob::new(
            513,
            321,
            selection,
            shape,
            SelectionMode::Add,
            true,
            CPU_BUDGET,
        )
        .unwrap();
        while !job.step().unwrap() {}
        selection = Some(Arc::new(job.output));
    }
    let input = selection.unwrap();
    write(root, "input-mask.png", &input);
    for radius in [1, 3, 8, 24] {
        let mut job =
            MaskJob::new(input.clone(), MaskOperation::Feather, radius, CPU_BUDGET).unwrap();
        while !job.step().unwrap() {}
        write(
            root,
            &format!("fotoeditor-radius-{radius}.png"),
            &job.finish().unwrap(),
        );
    }
}
