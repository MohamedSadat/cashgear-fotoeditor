//! Incremental raster-channel PSD decoding into a private, bounded candidate.
use crate::{
    color::IccTransform,
    psd_records::{self, Channel, Cursor, Record, Records, Rect},
    PsdPreflight,
};
use flate2::{Decompress, FlushDecompress, Status};
use fotoeditor_core::{
    CanvasRect, Document, Gray8, Layer, LayerContent, LayerMask, Residency, Rgba8, TileSurface,
};
use std::{ops::Range, sync::Arc};

const SCRATCH: u64 = crate::color::COLOR_SCRATCH + 2 * 1024 * 1024;

struct Plane {
    range: Range<usize>,
    compression: u16,
    at: usize,
    y: u32,
    width: u32,
    height: u32,
    zip: Option<Decompress>,
    ended: bool,
}
impl Plane {
    fn new(bytes: &[u8], channel: &Channel, rect: Rect) -> Result<Self, String> {
        let data = &bytes[channel.data.clone()];
        let mut c = Cursor::new(data);
        let compression = c.u16()?;
        match compression {
            0 if data.len() != 2 + rect.width as usize * rect.height as usize => {
                return Err("PSD raw channel has the wrong length".into())
            }
            1 => {
                let mut length = 2usize + rect.height as usize * 2;
                for _ in 0..rect.height {
                    length = length
                        .checked_add(c.u16()? as usize)
                        .ok_or("PSD RLE length overflow")?;
                }
                if length != data.len() {
                    return Err("PSD RLE row lengths do not match the channel".into());
                }
            }
            0 | 2 | 3 => {}
            _ => return Err("Unsupported PSD channel compression".into()),
        }
        Ok(Self {
            range: channel.data.clone(),
            compression,
            at: c.at,
            y: 0,
            width: rect.width,
            height: rect.height,
            zip: (compression >= 2).then(|| Decompress::new(true)),
            ended: false,
        })
    }
    fn row(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        if self.y >= self.height {
            return Err("PSD channel has no remaining rows".into());
        }
        let data = &bytes[self.range.clone()];
        let mut row = vec![0; self.width as usize];
        match self.compression {
            0 => {
                row.copy_from_slice(&data[self.at..self.at + self.width as usize]);
                self.at += self.width as usize;
            }
            1 => {
                let n = u16::from_be_bytes(data[2 + self.y as usize * 2..][..2].try_into().unwrap())
                    as usize;
                let mut c = Cursor::new(&data[self.at..self.at + n]);
                let mut x = 0usize;
                while c.at < c.data.len() {
                    let control = c.byte()?;
                    if control == 128 {
                        continue;
                    }
                    let count = if control < 128 {
                        usize::from(control) + 1
                    } else {
                        257 - usize::from(control)
                    };
                    let target = row
                        .get_mut(x..x + count)
                        .ok_or("PSD RLE expands past the row width")?;
                    if control < 128 {
                        target.copy_from_slice(c.take(count)?);
                    } else {
                        target.fill(c.byte()?);
                    }
                    x += count;
                }
                if x != row.len() {
                    return Err("Truncated PSD RLE row".into());
                }
                self.at += n;
            }
            2 | 3 => {
                let zip = self.zip.as_mut().unwrap();
                let mut written = 0;
                while written < row.len() {
                    let input = zip.total_in();
                    let output = zip.total_out();
                    let status = zip
                        .decompress(&data[self.at..], &mut row[written..], FlushDecompress::None)
                        .map_err(|_| "Invalid PSD ZIP channel")?;
                    let used = (zip.total_in() - input) as usize;
                    let made = (zip.total_out() - output) as usize;
                    self.at += used;
                    written += made;
                    if status == Status::StreamEnd {
                        self.ended = true;
                        break;
                    }
                    if used == 0 && made == 0 {
                        return Err("Truncated PSD ZIP channel".into());
                    }
                }
                if written != row.len() || (self.ended && self.y + 1 != self.height) {
                    return Err("PSD ZIP channel has the wrong decoded length".into());
                }
                if self.compression == 3 {
                    for x in 1..row.len() {
                        row[x] = row[x].wrapping_add(row[x - 1]);
                    }
                }
            }
            _ => unreachable!(),
        }
        self.y += 1;
        Ok(row)
    }
    fn finish(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.y != self.height {
            return Err("Incomplete PSD channel".into());
        }
        let data = &bytes[self.range.clone()];
        if let Some(zip) = self.zip.as_mut() {
            while !self.ended {
                let input = zip.total_in();
                let output = zip.total_out();
                let status = zip
                    .decompress(&data[self.at..], &mut [0u8; 1], FlushDecompress::Finish)
                    .map_err(|_| "Invalid PSD ZIP trailer")?;
                self.at += (zip.total_in() - input) as usize;
                if zip.total_out() != output {
                    return Err("PSD ZIP expands past its declared plane".into());
                }
                if status == Status::StreamEnd {
                    self.ended = true;
                } else if zip.total_in() == input {
                    return Err("Truncated PSD ZIP trailer".into());
                }
            }
        }
        if self.at != data.len() {
            return Err("Trailing bytes in PSD channel".into());
        }
        Ok(())
    }
}

pub struct PsdImportJob {
    input: Vec<u8>,
    records: Records,
    display_reserve: u64,
    layers: Vec<Option<Layer>>,
    color: Option<IccTransform>,
    record: usize,
    channel: usize,
    plane: Option<Plane>,
    color_tiles: Vec<(u32, u32)>,
    color_index: usize,
    coloring: bool,
    failed: bool,
    done: bool,
}
impl PsdImportJob {
    pub fn new(bytes: &[u8], residency: Residency) -> Result<Self, String> {
        residency.check_cpu(SCRATCH + bytes.len() as u64)?;
        let preflight = PsdPreflight::inspect(bytes)?;
        let records = psd_records::parse(bytes, &preflight)?;
        // A layered import must also be able to present its initial composite
        // after replacement. Reserve a full display surface and the maximum
        // bounded compositor stack before accepting any candidate pixels.
        let display_reserve = u64::from(records.width.div_ceil(256))
            * u64::from(records.height.div_ceil(256))
            * 262144
            + 80 * 1024 * 1024;
        residency
            .check_cpu(SCRATCH + bytes.len() as u64 + records.padded_bytes + display_reserve)?;
        let color = preflight
            .icc_profile
            .map(|p| IccTransform::new(p, image::ColorType::Rgba8))
            .transpose()?
            .flatten();
        let layers = records
            .layers
            .iter()
            .map(|r| {
                if r.kind == 3 {
                    return None;
                }
                let mut layer = if r.kind == 0 {
                    Layer::raster(
                        r.name.clone(),
                        TileSurface::new(records.width, records.height),
                    )
                } else {
                    Layer::group(r.name.clone(), Vec::new())
                };
                layer.properties = r.properties.clone();
                layer.mask = r.mask.as_ref().map(|m| LayerMask {
                    enabled: m.enabled,
                    samples: Arc::new(TileSurface::with_default(
                        records.width,
                        records.height,
                        Gray8(m.default),
                    )),
                });
                Some(layer)
            })
            .collect();
        Ok(Self {
            input: bytes.to_vec(),
            records,
            display_reserve,
            layers,
            color,
            record: 0,
            channel: 0,
            plane: None,
            color_tiles: Vec::new(),
            color_index: 0,
            coloring: false,
            failed: false,
            done: false,
        })
    }
    /// Reserve the admitted candidate, rather than just its currently decoded tiles.
    pub fn temporary_bytes(&self) -> u64 {
        SCRATCH + self.input.capacity() as u64 + self.records.padded_bytes + self.display_reserve
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.failed {
            return Err("PSD import already failed".into());
        }
        let result = self.advance();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn advance(&mut self) -> Result<bool, String> {
        if self.done {
            return Ok(true);
        }
        if self.record == self.records.layers.len() {
            self.done = true;
            return Ok(true);
        }
        let record = &self.records.layers[self.record];
        if self.coloring {
            if let Some(&coord) = self.color_tiles.get(self.color_index) {
                let surface = self.layers[self.record]
                    .as_mut()
                    .unwrap()
                    .raster_samples_mut()?;
                let mut tile = surface.edit_tile(coord);
                let x0 = (i64::from(record.rect.x) - i64::from(self.records.origin.0)) as u32;
                let y0 = (i64::from(record.rect.y) - i64::from(self.records.origin.1)) as u32;
                let start_x = x0.max(coord.0 * 256) - coord.0 * 256;
                let end_x = (x0 + record.rect.width).min(coord.0 * 256 + 256) - coord.0 * 256;
                for y in y0.max(coord.1 * 256)..(y0 + record.rect.height).min(coord.1 * 256 + 256) {
                    let start = ((y % 256) * 256 + start_x) as usize;
                    let pixels = &mut tile[start..start + (end_x - start_x) as usize];
                    let mut row = [0u8; 1024];
                    for (dst, p) in row.as_chunks_mut::<4>().0.iter_mut().zip(pixels.iter()) {
                        dst.copy_from_slice(&p.0);
                    }
                    self.color
                        .as_ref()
                        .unwrap()
                        .convert(&mut row[..pixels.len() * 4]);
                    for (dst, p) in pixels.iter_mut().zip(row.as_chunks::<4>().0.iter()) {
                        dst.0.copy_from_slice(p);
                    }
                }
                self.color_index += 1;
                return Ok(false);
            }
            self.coloring = false;
            self.color_tiles.clear();
            self.record += 1;
            self.channel = 0;
            return Ok(false);
        }
        if self.channel == record.channels.len() {
            if record.kind == 0 && self.color.is_some() {
                self.color_tiles = self.layers[self.record]
                    .as_ref()
                    .unwrap()
                    .raster_samples()?
                    .tiles()
                    .keys()
                    .copied()
                    .collect();
                self.color_index = 0;
                self.coloring = true;
            } else {
                self.record += 1;
                self.channel = 0;
            }
            return Ok(false);
        }
        let channel = &record.channels[self.channel];
        let rect = if channel.id == -2 {
            record.mask.as_ref().unwrap().rect
        } else {
            record.rect
        };
        if record.kind != 0 && channel.id != -2 && (rect.width != 0 || rect.height != 0) {
            return Err("PSD group divider contains unsupported raster channels".into());
        }
        if self.plane.is_none() {
            self.plane = Some(Plane::new(&self.input, channel, rect)?);
        }
        let plane = self.plane.as_mut().unwrap();
        // At most 16 scanlines per step, including 16,384-wide adversarial inputs.
        for _ in 0..16 {
            if plane.y == plane.height {
                break;
            }
            let y = plane.y;
            let row = plane.row(&self.input)?;
            if let Some(layer) = self.layers[self.record].as_mut() {
                put_row(
                    layer,
                    record,
                    channel.id,
                    rect,
                    self.records.origin,
                    y,
                    &row,
                )?;
            } else if row.iter().any(|v| *v != 0) {
                return Err("PSD group marker contains raster data".into());
            }
        }
        if plane.y == plane.height {
            plane.finish(&self.input)?;
            self.plane = None;
            self.channel += 1;
        }
        Ok(false)
    }
    pub fn finish(mut self) -> Result<Document, String> {
        if self.failed || !self.done {
            return Err("PSD import is incomplete or failed".into());
        }
        let mut stack: Vec<Vec<Layer>> = vec![Vec::new()];
        for (record, layer) in self.records.layers.iter().zip(self.layers.iter_mut()) {
            match record.kind {
                3 => stack.push(Vec::new()),
                1 | 2 => {
                    let children = stack.pop().ok_or("Unbalanced PSD group")?;
                    let mut group = layer.take().ok_or("Missing PSD group")?;
                    group.content = LayerContent::Group(children);
                    stack.last_mut().ok_or("Unbalanced PSD group")?.push(group);
                }
                _ => stack
                    .last_mut()
                    .unwrap()
                    .push(layer.take().ok_or("Missing PSD raster")?),
            }
        }
        if stack.len() != 1 {
            return Err("Unbalanced PSD group".into());
        }
        let r = self.records;
        let canvas = CanvasRect {
            x: (-i64::from(r.origin.0)) as u32,
            y: (-i64::from(r.origin.1)) as u32,
            width: r.canvas_width,
            height: r.canvas_height,
        };
        let mut document = Document::blank(r.width, r.height, r.dpi)?;
        document.canvas = (canvas.x != 0
            || canvas.y != 0
            || canvas.width != r.width
            || canvas.height != r.height)
            .then_some(canvas);
        document.layers = stack.pop().unwrap();
        document.validate_geometry()?;
        fotoeditor_core::validate_layers(&document.layers, document.width, document.height)?;
        Ok(document)
    }
}

fn put_row(
    layer: &mut Layer,
    record: &Record,
    id: i16,
    rect: Rect,
    origin: (i32, i32),
    y: u32,
    row: &[u8],
) -> Result<(), String> {
    let x0 = (i64::from(rect.x) - i64::from(origin.0)) as u32;
    let y = (i64::from(rect.y) - i64::from(origin.1)) as u32 + y;
    let mut offset = 0;
    while offset < row.len() {
        let x = x0 + offset as u32;
        let count = (256 - x % 256).min((row.len() - offset) as u32) as usize;
        let start = ((y % 256) * 256 + x % 256) as usize;
        if id == -2 {
            let surface = Arc::make_mut(&mut layer.mask.as_mut().unwrap().samples);
            let mut tile = surface.edit_tile((x / 256, y / 256));
            for (dst, src) in tile[start..start + count]
                .iter_mut()
                .zip(&row[offset..offset + count])
            {
                dst.0 = *src;
            }
        } else {
            let surface = layer.raster_samples_mut()?;
            let mut tile = surface.edit_tile((x / 256, y / 256));
            for (dst, src) in tile[start..start + count]
                .iter_mut()
                .zip(&row[offset..offset + count])
            {
                // Initialize alpha when the first channel is decoded, irrespective of channel order.
                if record
                    .channels
                    .iter()
                    .find(|c| c.id != -2)
                    .is_some_and(|c| c.id == id)
                {
                    *dst = Rgba8([0, 0, 0, 255]);
                }
                dst.0[if id == -1 { 3 } else { id as usize }] = *src;
            }
        }
        offset += count;
    }
    Ok(())
}
