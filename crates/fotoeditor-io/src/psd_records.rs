//! Checked raster PSD metadata. Unknown rendering features are refused explicitly.
use crate::PsdPreflight;
use fotoeditor_core::{BlendMode, LayerLocks, LayerProperties, MAX_GROUP_DEPTH, MAX_SIDE};
use std::ops::Range;

pub(crate) struct Cursor<'a> {
    pub data: &'a [u8],
    pub at: usize,
}
impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("PSD length overflow")?;
        let out = self.data.get(self.at..end).ok_or("Truncated PSD record")?;
        self.at = end;
        Ok(out)
    }
    pub fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn section(&mut self) -> Result<&'a [u8], String> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    fn remaining(&self) -> &'a [u8] {
        &self.data[self.at..]
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    fn read(c: &mut Cursor<'_>) -> Result<Self, String> {
        let y = c.u32()? as i32;
        let x = c.u32()? as i32;
        let bottom = c.u32()? as i32;
        let right = c.u32()? as i32;
        let width = i64::from(right) - i64::from(x);
        let height = i64::from(bottom) - i64::from(y);
        if !(0..=i64::from(MAX_SIDE)).contains(&width)
            || !(0..=i64::from(MAX_SIDE)).contains(&height)
            || (width == 0) != (height == 0)
        {
            return Err("PSD layer or mask bounds exceed 16,384 pixels or are inverted".into());
        }
        Ok(Self {
            x,
            y,
            width: width as u32,
            height: height as u32,
        })
    }
    pub fn translated(self, x: i32, y: i32) -> Result<Self, String> {
        Ok(Self {
            x: self.x.checked_add(x).ok_or("PSD mask origin overflow")?,
            y: self.y.checked_add(y).ok_or("PSD mask origin overflow")?,
            ..self
        })
    }
    pub fn padded_bytes(self, origin: (i32, i32), channels: u64) -> u64 {
        if self.width == 0 {
            return 0;
        }
        let x = (i64::from(self.x) - i64::from(origin.0)) as u64;
        let y = (i64::from(self.y) - i64::from(origin.1)) as u64;
        ((x + u64::from(self.width)).div_ceil(256) - x / 256)
            * ((y + u64::from(self.height)).div_ceil(256) - y / 256)
            * 65536
            * channels
    }
}

pub(crate) struct Mask {
    pub rect: Rect,
    pub default: u8,
    pub enabled: bool,
}
pub(crate) struct Channel {
    pub id: i16,
    pub data: Range<usize>,
}
pub(crate) struct Record {
    pub rect: Rect,
    pub name: String,
    pub properties: LayerProperties,
    pub mask: Option<Mask>,
    /// 0 raster, 1/2 group metadata after its children, 3 group start marker.
    pub kind: u32,
    pub channels: Vec<Channel>,
}
pub(crate) struct Records {
    pub layers: Vec<Record>,
    pub origin: (i32, i32),
    pub width: u32,
    pub height: u32,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub dpi: f64,
    pub padded_bytes: u64,
}

fn blend(key: &[u8]) -> Result<(BlendMode, bool), String> {
    use BlendMode::*;
    Ok((
        match key {
            b"norm" | b"pass" => Normal,
            b"mul " => Multiply,
            b"scrn" => Screen,
            b"over" => Overlay,
            b"dark" => Darken,
            b"lite" => Lighten,
            b"div " => ColorDodge,
            b"idiv" => ColorBurn,
            b"hLit" => HardLight,
            b"sLit" => SoftLight,
            b"diff" => Difference,
            b"smud" => Exclusion,
            b"hue " => Hue,
            b"sat " => Saturation,
            b"colr" => Color,
            b"lum " => Luminosity,
            _ => {
                return Err(format!(
                    "Unsupported PSD blend mode: {}",
                    String::from_utf8_lossy(key)
                ))
            }
        },
        key == b"pass",
    ))
}

fn mask(bytes: &[u8], layer: Rect) -> Result<Option<Mask>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }
    // A real-mask record can encode the combination of a raster and vector mask.
    // Do not silently select one of those masks or bake a vector into editable data.
    if bytes.len() != 20 {
        return Err(
            "PSD mask parameters, real/vector masks and feather/density are not supported".into(),
        );
    }
    let mut c = Cursor::new(bytes);
    let mut rect = Rect::read(&mut c)?;
    let default = c.byte()?;
    let flags = c.byte()?;
    if flags & !3 != 0 || c.take(2)? != [0, 0] {
        return Err("Unsupported PSD mask flags or parameters".into());
    }
    if flags & 1 != 0 {
        rect = rect.translated(layer.x, layer.y)?;
    }
    Ok(Some(Mask {
        rect,
        default,
        enabled: flags & 2 == 0,
    }))
}

fn tagged(record: &mut Record, bytes: &[u8]) -> Result<(), String> {
    let mut c = Cursor::new(bytes);
    let mut seen = std::collections::HashSet::new();
    while !c.remaining().is_empty() {
        if c.remaining().len() < 12 && c.remaining().iter().all(|v| *v == 0) {
            break;
        }
        if c.take(4)? != b"8BIM" {
            return Err("Invalid PSD layer tag signature".into());
        }
        let key = c.take(4)?;
        if !seen.insert(key) {
            return Err("Duplicate PSD layer tag".into());
        }
        let data = c.section()?;
        if data.len() % 2 != 0 {
            c.take(1)?;
        }
        let mut d = Cursor::new(data);
        match key {
            b"luni" => {
                let n = d.u32()? as usize;
                if n > 512 {
                    return Err("PSD Unicode layer name exceeds 512 UTF-16 units".into());
                }
                let units: Vec<u16> = d
                    .take(n * 2)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| u16::from_be_bytes(*v))
                    .collect();
                record.name =
                    String::from_utf16(&units).map_err(|_| "Invalid PSD Unicode layer name")?;
            }
            b"lsct" | b"lsdk" => {
                record.kind = d.u32()?;
                if record.kind > 3 {
                    return Err("Unknown PSD group divider".into());
                }
                if data.len() >= 12 {
                    if d.take(4)? != b"8BIM" {
                        return Err("Invalid PSD group blend signature".into());
                    }
                    let (mode, pass) = blend(d.take(4)?)?;
                    record.properties.blend = mode;
                    record.properties.pass_through = pass;
                }
                if !matches!(data.len(), 4 | 12 | 16) || (data.len() == 16 && d.u32()? != 0) {
                    return Err("Unsupported PSD group subtype".into());
                }
            }
            b"iOpa" if matches!(data.len(), 1 | 4) && data[1..].iter().all(|v| *v == 0) => {
                record.properties.fill = f64::from(data[0]) / 255.0
            }
            b"lspf" if data.len() == 4 => {
                let bits = d.u32()?;
                if bits & !0x80000007 != 0 {
                    return Err("Unsupported PSD layer locks".into());
                }
                record.properties.locks = LayerLocks {
                    alpha: bits & 1 != 0,
                    pixels: bits & 2 != 0,
                    position: bits & 4 != 0,
                    all: bits & 0x80000000 != 0,
                };
            }
            b"clbl" | b"infx" | b"knko" | b"tsly" => {
                let expected = u8::from(matches!(key, b"clbl" | b"tsly"));
                if !matches!(data.len(), 1 | 4)
                    || data[0] != expected
                    || data[1..].iter().any(|v| *v != 0)
                {
                    return Err("Unsupported PSD knockout or advanced blend settings".into());
                }
            }
            // Identifiers, UI colors, reference point, thumbnails and author metadata.
            // None of these changes the raster layer or its rendered appearance.
            b"lyid" | b"lclr" | b"fxrp" | b"shmd" | b"lyvr" | b"lnsr" => {}
            b"sn2P" if data.len() == 4 && d.u32()? <= 1 => {}
            _ => {
                return Err(format!(
                    "Unsupported PSD layer feature: {}",
                    String::from_utf8_lossy(key)
                ))
            }
        }
    }
    if record.name.is_empty() || record.name.len() > 512 || record.name.contains('\0') {
        return Err("Invalid or excessive PSD layer name".into());
    }
    Ok(())
}

pub(crate) fn parse(bytes: &[u8], preflight: &PsdPreflight<'_>) -> Result<Records, String> {
    let mut outer = Cursor::new(preflight.layer_and_mask_data);
    if outer.data.is_empty() || preflight.layer_count == 0 {
        return Err("PSD has no raster layers; save a layered 8-bit RGB PSD".into());
    }
    let mut c = Cursor::new(outer.section()?);
    let count = (c.u16()? as i16).unsigned_abs() as usize;
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let rect = Rect::read(&mut c)?;
        let n = c.u16()? as usize;
        if !(3..=5).contains(&n) {
            return Err("PSD layers require RGB, optional alpha and one raster mask".into());
        }
        let mut channels = Vec::with_capacity(n);
        for _ in 0..n {
            let id = c.u16()? as i16;
            let length = c.u32()? as usize;
            if !(-2..=2).contains(&id)
                || length < 2
                || channels.iter().any(|v: &Channel| v.id == id)
            {
                return Err("Unsupported or duplicate PSD channel".into());
            }
            channels.push(Channel {
                id,
                data: 0..length,
            });
        }
        if !(0..3).all(|id| channels.iter().any(|v| v.id == id)) {
            return Err("PSD layer is missing an RGB channel".into());
        }
        if c.take(4)? != b"8BIM" {
            return Err("Invalid PSD blend signature".into());
        }
        let (mode, pass) = blend(c.take(4)?)?;
        let opacity = f64::from(c.byte()?) / 255.0;
        let clipping = c.byte()?;
        let flags = c.byte()?;
        if clipping > 1 || flags & 0xe0 != 0 || c.byte()? != 0 {
            return Err("Unsupported PSD layer flags, clipping or non-raster content".into());
        }
        let mut e = Cursor::new(c.section()?);
        if e.data.len() > 2 * 1024 * 1024 {
            return Err("PSD layer metadata exceeds 2 MiB".into());
        }
        let mask = mask(e.section()?, rect)?;
        if channels.iter().any(|v| v.id == -2) && mask.is_none()
            || mask.as_ref().is_some_and(|m| m.rect.width != 0)
                && !channels.iter().any(|v| v.id == -2)
        {
            return Err("PSD mask channel and metadata disagree".into());
        }
        let ranges = e.section()?;
        if !ranges.len().is_multiple_of(8)
            || ranges
                .as_chunks::<8>()
                .0
                .iter()
                .any(|v| *v != [0, 0, 255, 255, 0, 0, 255, 255])
        {
            return Err("PSD Blend If ranges are not supported".into());
        }
        let name_len = e.byte()? as usize;
        let name = String::from_utf8_lossy(e.take(name_len)?).into_owned();
        e.take((4 - (name_len + 1) % 4) % 4)?;
        let mut record = Record {
            rect,
            name,
            mask,
            kind: 0,
            channels,
            properties: LayerProperties {
                visible: flags & 2 == 0,
                opacity,
                blend: mode,
                pass_through: pass,
                clipped: clipping != 0,
                locks: LayerLocks {
                    alpha: flags & 1 != 0,
                    ..LayerLocks::default()
                },
                ..LayerProperties::default()
            },
        };
        tagged(&mut record, e.remaining())?;
        if record.kind == 0 && flags & 0x10 != 0 {
            return Err("PSD layer contains non-raster content".into());
        }
        if record.kind == 0 && pass {
            return Err("PSD pass-through blend requires a group".into());
        }
        layers.push(record);
    }
    let mut depth = 0;
    for record in &mut layers {
        match record.kind {
            3 => {
                depth += 1;
                if depth > MAX_GROUP_DEPTH {
                    return Err("PSD groups exceed the depth limit".into());
                }
            }
            1 | 2 => {
                depth = depth
                    .checked_sub(1)
                    .ok_or("Unbalanced PSD group dividers")?;
            }
            _ => {}
        }
        for channel in &mut record.channels {
            let data = c.take(channel.data.end)?;
            let start = data.as_ptr() as usize - bytes.as_ptr() as usize;
            channel.data = start..start + data.len();
        }
    }
    if depth != 0 || c.remaining().len() > 3 || c.remaining().iter().any(|v| *v != 0) {
        return Err("Unbalanced PSD groups or trailing layer data".into());
    }
    let global_mask = if outer.remaining().is_empty() {
        &[]
    } else {
        outer.section()?
    };
    // Photoshop's preferred kind 128 says to use each layer's stored mask.
    // Its overlay color/opacity describes mask display, not exported RGB samples.
    if !global_mask.is_empty()
        && (global_mask.len() < 13
            || global_mask.len() > 16
            || global_mask[12] != 128
            || u16::from_be_bytes(global_mask[10..12].try_into().unwrap()) > 100
            || global_mask[13..].iter().any(|v| *v != 0))
    {
        return Err("Unsupported PSD global mask kind or malformed overlay metadata".into());
    }
    while !outer.remaining().is_empty() {
        if outer.remaining().len() <= 3 && outer.remaining().iter().all(|v| *v == 0) {
            break;
        }
        if outer.take(4)? != b"8BIM" {
            return Err("Invalid PSD global tag signature".into());
        }
        let key = outer.take(4)?;
        let data = outer.section()?;
        outer.take((4 - data.len() % 4) % 4)?;
        match key {
            // Cached-compositor provenance; the imported raster tree is recomposited.
            b"cinf" => {}
            b"Patt" | b"Pat2" | b"Pat3" if data.is_empty() => {}
            // Quick-mask display overlay, not a layer mask or filter effect.
            b"FMsk"
                if data.len() == 12
                    && u16::from_be_bytes(data[10..12].try_into().unwrap()) <= 100 => {}
            _ => {
                return Err(format!(
                    "Unsupported PSD global feature: {}",
                    String::from_utf8_lossy(key)
                ))
            }
        }
    }
    let mut x0 = 0i64;
    let mut y0 = 0i64;
    let mut x1 = i64::from(preflight.header.width);
    let mut y1 = i64::from(preflight.header.height);
    for r in &layers {
        for rect in std::iter::once(r.rect).chain(r.mask.as_ref().map(|m| m.rect)) {
            if rect.width == 0 {
                continue;
            }
            x0 = x0.min(i64::from(rect.x));
            y0 = y0.min(i64::from(rect.y));
            x1 = x1.max(i64::from(rect.x) + i64::from(rect.width));
            y1 = y1.max(i64::from(rect.y) + i64::from(rect.height));
        }
    }
    if x1 - x0 > i64::from(MAX_SIDE) || y1 - y0 > i64::from(MAX_SIDE) {
        return Err("PSD retained layers and masks exceed a 16,384-pixel side".into());
    }
    let origin = (x0 as i32, y0 as i32);
    let padded_bytes = layers
        .iter()
        .map(|r| {
            (if r.kind == 0 {
                r.rect.padded_bytes(origin, 4)
            } else {
                0
            }) + r
                .mask
                .as_ref()
                .map_or(0, |m| m.rect.padded_bytes(origin, 1))
        })
        .sum();
    let mut dpi = 72.0;
    let mut resources = Cursor::new(preflight.image_resources);
    while !resources.remaining().is_empty() {
        resources.take(4)?;
        let id = resources.u16()?;
        let n = resources.byte()? as usize;
        resources.take(n)?;
        if !(n + 1).is_multiple_of(2) {
            resources.take(1)?;
        }
        let data = resources.section()?;
        if data.len() % 2 != 0 {
            resources.take(1)?;
        }
        if id == 1005 {
            if data.len() != 16 {
                return Err("Invalid PSD resolution resource".into());
            }
            dpi = f64::from(u32::from_be_bytes(data[..4].try_into().unwrap())) / 65536.0;
            fotoeditor_core::validate_size(1, 1, dpi)?;
        }
    }
    Ok(Records {
        layers,
        origin,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
        canvas_width: preflight.header.width,
        canvas_height: preflight.header.height,
        dpi,
        padded_bytes,
    })
}
