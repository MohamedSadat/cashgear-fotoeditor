//! Bounded CPU codecs; construction/export jobs yield between tile rows/scanlines.
mod project;
mod project_layers;
mod project_stream;
use fotoeditor_core::{Document, Residency, Rgba8, MAX_SIDE, TILE_PIXELS, TILE_SIZE};
use image::{ImageDecoder, ImageReader};
pub use project::*;
pub use project_layers::*;
pub use project_stream::*;
use std::{
    cell::RefCell,
    io::{Cursor, Write},
    rc::Rc,
};
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub fn encode_thumbnail(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0
        || height == 0
        || width > 64
        || height > 64
        || pixels.len() != (width * height * 4) as usize
    {
        return Err("Invalid thumbnail dimensions".into());
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(pixels).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

pub struct ImportJob {
    pub document: Document,
    samples: Vec<u8>,
    next_y: u32,
}
impl ImportJob {
    pub fn decode(bytes: &[u8], residency: Residency) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Image files must be at most 128 MiB".into());
        }
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let format = reader
            .format()
            .ok_or("Unrecognized image. Open PNG, JPEG or WebP.")?;
        if !matches!(
            format,
            image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
        ) {
            return Err("Open PNG, JPEG or WebP images".into());
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(384 * 1024 * 1024);
        reader.limits(limits);
        let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        let (w, h) = decoder.dimensions();
        fotoeditor_core::validate_size(w, h, 72.0)?;
        if decoder.color_type().bits_per_pixel() / decoder.color_type().channel_count() as u16 > 8 {
            return Err("16-bit images will be supported in P11; open an 8-bit image".into());
        }
        let padded =
            w.div_ceil(TILE_SIZE) as u64 * h.div_ceil(TILE_SIZE) as u64 * TILE_PIXELS as u64 * 4;
        // Decoder input/output/conversion, pending tiles and current document coexist.
        residency
            .check_cpu(padded + w as u64 * h as u64 * 12 + bytes.len() as u64 + 4 * 1024 * 1024)?;
        let mut dpi = 72.0;
        if format == image::ImageFormat::Png {
            if let Ok(p) = png::Decoder::new(Cursor::new(bytes)).read_info() {
                if let Some(p) = p.info().pixel_dims {
                    if p.unit == png::Unit::Meter && p.xppu > 0 {
                        dpi = (p.xppu as f64 * 0.0254).clamp(1.0, 2400.0);
                    }
                }
            }
        }
        let image = image::DynamicImage::from_decoder(decoder)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        Ok(Self {
            document: Document::blank(w, h, dpi)?,
            samples: image.into_raw(),
            next_y: 0,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.samples.capacity() as u64 + self.document.raster().bytes()
    }
    pub fn step(&mut self) -> bool {
        let w = self.document.width;
        let h = self.document.height;
        let ty = self.next_y / TILE_SIZE;
        for tx in 0..w.div_ceil(TILE_SIZE) {
            let mut tile = vec![Rgba8::default(); TILE_PIXELS];
            for y in self.next_y..(self.next_y + TILE_SIZE).min(h) {
                let x0 = tx * TILE_SIZE;
                let n = TILE_SIZE.min(w - x0) as usize;
                let src = ((y * w + x0) * 4) as usize;
                let dst = ((y - self.next_y) * TILE_SIZE) as usize;
                for (target, pixel) in tile[dst..dst + n]
                    .iter_mut()
                    .zip(self.samples[src..src + n * 4].as_chunks::<4>().0.iter())
                {
                    target.0.copy_from_slice(pixel);
                }
            }
            self.document.raster_mut().insert((tx, ty), tile);
        }
        self.next_y += TILE_SIZE;
        self.next_y >= h
    }
}

#[derive(Clone)]
struct Output(Rc<RefCell<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut out = self.0.borrow_mut();
        if out.len() + bytes.len() > MAX_FILE_BYTES as usize {
            return Err(std::io::Error::other(
                "PNG export exceeds the 128 MiB output budget",
            ));
        }
        out.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub struct ExportJob {
    document: Document,
    stream: Option<png::StreamWriter<'static, Output>>,
    out: Output,
    next_y: u32,
    row: Vec<u8>,
}
impl ExportJob {
    pub fn needed(&self, visitor: &mut dyn FnMut(fotoeditor_core::TileRef<'_>)) {
        if self.next_y >= self.document.height {
            return;
        }
        let y1 = (self.next_y + 32).min(self.document.height);
        for ty in self.next_y / TILE_SIZE..y1.div_ceil(TILE_SIZE) {
            for tx in 0..self.document.width.div_ceil(TILE_SIZE) {
                if let Some(t) = self.document.raster().tiles().get(&(tx, ty)) {
                    visitor(fotoeditor_core::TileRef::Rgba(t));
                }
            }
        }
    }
    pub fn new(document: &Document, residency: Residency) -> Result<Self, String> {
        document.export_raster()?;
        // Conservative output capacity + JS bridge copy; shared tile Arcs add no pixel copies.
        residency.check_cpu(2 * MAX_FILE_BYTES + document.width as u64 * 8 + 4 * 1024 * 1024)?;
        let out = Output(Rc::new(RefCell::new(Vec::new())));
        let mut encoder = png::Encoder::new(out.clone(), document.width, document.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let ppu = (document.dpi / 0.0254).round() as u32;
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppu,
            yppu: ppu,
            unit: png::Unit::Meter,
        }));
        let stream = encoder
            .write_header()
            .map_err(|e| e.to_string())?
            .into_stream_writer()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            document: document.clone(),
            stream: Some(stream),
            out,
            next_y: 0,
            row: vec![0; document.width as usize * 4],
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.out.0.borrow().capacity() as u64 + self.row.capacity() as u64 + 4 * 1024 * 1024
    }
    pub fn step(&mut self) -> Result<bool, String> {
        for y in self.next_y..(self.next_y + 32).min(self.document.height) {
            self.row.fill(0);
            for tx in 0..self.document.width.div_ceil(TILE_SIZE) {
                if let Some(tile) = self.document.raster().tiles().get(&(tx, y / TILE_SIZE)) {
                    let tile = tile.pin()?;
                    let n = TILE_SIZE.min(self.document.width - tx * TILE_SIZE) as usize;
                    let start = ((y % TILE_SIZE) * TILE_SIZE) as usize;
                    for (dst, p) in self.row[tx as usize * TILE_SIZE as usize * 4..][..n * 4]
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(&tile.pixels[start..start + n])
                    {
                        dst.copy_from_slice(&p.0);
                    }
                }
            }
            self.stream
                .as_mut()
                .ok_or("Export already finished")?
                .write_all(&self.row)
                .map_err(|e| e.to_string())?;
        }
        self.next_y = (self.next_y + 32).min(self.document.height);
        Ok(self.next_y == self.document.height)
    }
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        if self.next_y != self.document.height {
            return Err("Export is incomplete".into());
        }
        self.stream
            .take()
            .unwrap()
            .finish()
            .map_err(|e| e.to_string())?;
        Ok(std::mem::take(&mut *self.out.0.borrow_mut()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_preserves_every_rgba8_sample_at_tile_edges_and_dpi() {
        let mut d = Document::blank(513, 259, 300.0).unwrap();
        for y in 0..d.height {
            for x in 0..d.width {
                d.raster_mut().write(
                    x,
                    y,
                    Rgba8([(x % 256) as u8, (y % 256) as u8, 79, ((x + y) % 256) as u8]),
                );
            }
        }
        d.layers[0].properties.locks.all = true;
        let mut e = ExportJob::new(&d, Residency::default()).unwrap();
        while !e.step().unwrap() {}
        let png = e.finish().unwrap();
        let mut i = ImportJob::decode(&png, Residency::default()).unwrap();
        while !i.step() {}
        assert!((i.document.dpi - 300.0).abs() < 0.02);
        for y in 0..d.height {
            for x in 0..d.width {
                assert_eq!(d.raster().sample(x, y), i.document.raster().sample(x, y));
            }
        }
    }
    #[test]
    fn rejects_invalid_and_over_budget_without_document_change() {
        assert!(ImportJob::decode(b"broken", Residency::default()).is_err());
        assert!(Document::blank(16385, 1, 72.0).is_err());
        assert!(Residency {
            document: 768 * 1024 * 1024,
            ..Default::default()
        }
        .check_cpu(1)
        .is_err());
    }
    #[test]
    fn rejects_16_bit_and_oversized_headers_before_decoding_pixels() {
        for (width, depth, size) in [
            (1, png::BitDepth::Sixteen, 8),
            (16385, png::BitDepth::Eight, 16385 * 4),
        ] {
            let mut bytes = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut bytes, width, 1);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(depth);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(&vec![0; size])
                    .unwrap();
            }
            let error = ImportJob::decode(&bytes, Residency::default())
                .err()
                .unwrap();
            if width == 1 {
                assert!(error.contains("16-bit"));
            } else {
                assert!(
                    error.contains("limit") || error.contains("16384"),
                    "{error}"
                );
            }
        }
    }
}
