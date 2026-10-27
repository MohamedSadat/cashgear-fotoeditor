//! Immutable integer-mapped sampling. Browser adapters own source composition/I/O.
use super::*;
use fotoeditor_core::{Gray8, Tile, TileRef, MAX_SIDE};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloneOptions {
    pub aligned: bool,
    pub all_layers: bool,
}

#[derive(Default)]
pub struct CloneAnchor {
    pub source: Option<[i32; 2]>,
    pub offset: Option<[i32; 2]>,
}
impl CloneAnchor {
    pub fn select(
        &mut self,
        point: [f64; 2],
        bounds: fotoeditor_core::CanvasRect,
    ) -> Result<(), String> {
        if point.iter().any(|v| !v.is_finite() || *v < 0.0)
            || !bounds.contains(point[0].floor() as u32, point[1].floor() as u32)
        {
            return Err("Choose a Clone Stamp source inside the visible canvas".into());
        }
        self.source = Some(point.map(|v| v.floor() as i32));
        self.offset = None;
        Ok(())
    }
    pub fn mapping(&self, destination: [f64; 2], aligned: bool) -> Result<[i32; 2], String> {
        let source = self
            .source
            .ok_or("Alt-click to choose a Clone Stamp source first")?;
        if destination
            .iter()
            .any(|v| !v.is_finite() || v.abs() > f64::from(MAX_SIDE) * 2.0)
        {
            return Err("Invalid Clone Stamp destination".into());
        }
        if let (true, Some(offset)) = (aligned, self.offset) {
            return Ok(offset);
        }
        Ok(std::array::from_fn(|i| {
            source[i] - destination[i].floor() as i32
        }))
    }
}

impl StrokeCommand {
    pub fn for_clone(
        base: &Document,
        mut settings: BrushSettings,
        available: u64,
        layer: fotoeditor_core::LayerId,
        source: Document,
        offset: [i32; 2],
    ) -> Result<Self, String> {
        if offset.iter().any(|v| v.unsigned_abs() > MAX_SIDE)
            || source.width != base.width
            || source.height != base.height
            || source.canvas_rect() != base.canvas_rect()
        {
            return Err("Invalid Clone Stamp source geometry".into());
        }
        source.single_raster()?;
        if settings.eraser {
            return Err("Clone Stamp cannot erase".into());
        }
        settings.clone = None;
        let mut stroke = Self::for_layer(base, settings, available, layer)?;
        stroke.sampled_source = Some(source);
        stroke.clone_offset = offset;
        Ok(stroke)
    }

    /// Prepare the next bounded dab before asking the worker to hydrate its source.
    pub fn prepare_clone(&mut self) {
        if self.sampled_source.is_none() || self.dab.is_some() {
            return;
        }
        for _ in 0..512 {
            let Some(p) = self.next_dab() else { break };
            self.begin_dab(p);
            if self.dab.is_some() {
                break;
            }
        }
    }
    pub fn clone_needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        let (Some(source), Some(dab)) = (&self.sampled_source, &self.dab) else {
            return;
        };
        let bounds = source.canvas_rect();
        let [dx, dy] = self.clone_offset;
        let x0 = (i64::from(dab.x0) + i64::from(dx)).max(i64::from(bounds.x));
        let x1 = (i64::from(dab.x1) + i64::from(dx)).min(i64::from(bounds.x + bounds.width));
        let y0 = (i64::from(dab.y) + i64::from(dy)).max(i64::from(bounds.y));
        let y1 = (i64::from(dab.y1) + i64::from(dy)).min(i64::from(bounds.y + bounds.height));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for y in y0 as u32 / TILE_SIZE..=(y1 as u32 - 1) / TILE_SIZE {
            for x in x0 as u32 / TILE_SIZE..=(x1 as u32 - 1) / TILE_SIZE {
                if let Some(tile) = source.raster().tiles().get(&(x, y)) {
                    visitor(TileRef::Rgba(tile));
                }
            }
        }
    }
    pub(super) fn clone_source_cold(&self) -> bool {
        let mut cold = false;
        self.clone_needed(&mut |tile| {
            cold |= tile.bytes() == 0;
        });
        cold
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn stamp_row(
    dab: &Dab,
    tip: &Tip,
    settings: &BrushSettings,
    destination: (Option<&Tile<Rgba8>>, Option<&Tile<Gray8>>, u8),
    mask: &mut [u32],
    output: &mut [Rgba8],
    end: u32,
    source: &Document,
    offset: [i32; 2],
) -> Result<bool, String> {
    let (base, selection, clip_default) = destination;
    let bounds = source.canvas_rect();
    let sy = i64::from(dab.y) + i64::from(offset[1]);
    if sy < i64::from(bounds.y) || sy >= i64::from(bounds.y + bounds.height) {
        return Ok(false);
    }
    let sy = sy as u32;
    let scale = tip.radius / dab.radius;
    let ty = (dab.y as f64 + 0.5 - dab.p.y) * scale + tip.radius + 0.5;
    let influence = (dab.influence * 65535.0 + 0.5) as u32;
    let opacity = (settings.opacity * 65535.0 + 0.5) as u32;
    let mut changed = false;
    let mut x = dab.x;
    while x < end {
        let sx = i64::from(x) + i64::from(offset[0]);
        if sx < i64::from(bounds.x) || sx >= i64::from(bounds.x + bounds.width) {
            x += 1;
            continue;
        }
        let sx = sx as u32;
        let count = (end - x)
            .min(TILE_SIZE - sx % TILE_SIZE)
            .min(bounds.x + bounds.width - sx);
        let sampled = source
            .raster()
            .tiles()
            .get(&(sx / TILE_SIZE, sy / TILE_SIZE))
            .map(|t| t.pin())
            .transpose()?;
        if selection.is_none()
            && clip_default == 255
            && scale == 1.0
            && base.is_some_and(|t| t.known_opaque)
            && sampled.as_ref().is_some_and(|t| t.known_opaque)
        {
            let at = ((dab.y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize;
            let sample_at = ((sy % TILE_SIZE) * TILE_SIZE + sx % TILE_SIZE) as usize;
            let length = count as usize;
            changed |= stamp_opaque(
                dab,
                tip,
                x,
                influence,
                opacity,
                &base.unwrap().pixels[at..][..length],
                &sampled.as_ref().unwrap().pixels[sample_at..][..length],
                &mut mask[at..][..length],
                &mut output[at..][..length],
            );
            x += count;
            continue;
        }
        for n in 0..count {
            let color = sampled
                .as_ref()
                .map_or(source.raster().default, |t| {
                    t.pixels[((sy % TILE_SIZE) * TILE_SIZE + sx % TILE_SIZE + n) as usize]
                })
                .0;
            if color[3] == 0 {
                continue;
            }
            let px = x + n;
            let tx = (px as f64 + 0.5 - dab.p.x) * scale + tip.radius + 0.5;
            let value = tip.sample(tx, ty);
            if value == 0 {
                continue;
            }
            let i = ((dab.y % TILE_SIZE) * TILE_SIZE + px % TILE_SIZE) as usize;
            let clip = u32::from(selection.map_or(clip_default, |t| t.pixels[i].0));
            if clip == 0 {
                continue;
            }
            let amount = divide_unorm16(value * influence + 32767);
            mask[i] += divide_unorm16((65535 - mask[i]) * amount + 32767);
            let before = base.map_or(Rgba8::default(), |t| t.pixels[i]);
            let coverage = (divide_unorm16(opacity * mask[i] + 32767) * clip + 127) / 255;
            let coverage = (coverage * u32::from(color[3]) + 127) / 255;
            let after = if settings.alpha_lock {
                if before.0[3] == 0 {
                    before
                } else {
                    let mut rgba = before.0;
                    for c in 0..3 {
                        rgba[c] = divide_unorm16(
                            u32::from(color[c]) * coverage
                                + u32::from(before.0[c]) * (65535 - coverage)
                                + 32767,
                        ) as u8;
                    }
                    Rgba8(rgba)
                }
            } else {
                composite_fixed(before, color, coverage, false)
            };
            changed |= after != output[i];
            output[i] = after;
        }
        x += count;
    }
    Ok(changed)
}

// Same UNORM16 coverage and straight-alpha reference, with row-invariant mask
// interpolation. Opaque source/destination are common in photo retouching.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn stamp_opaque(
    dab: &Dab,
    tip: &Tip,
    x: u32,
    influence: u32,
    opacity: u32,
    original: &[Rgba8],
    source: &[Rgba8],
    masks: &mut [u32],
    outputs: &mut [Rgba8],
) -> bool {
    let tx = x as f64 - dab.p.x + tip.radius + 1.0;
    let ty = dab.y as f64 - dab.p.y + tip.radius + 1.0;
    let ix = tx as usize;
    let iy = ty as usize;
    let fx = ((tx - ix as f64) * 256.0) as u32;
    let fy = ((ty - iy as f64) * 256.0) as u32;
    let top = &tip.pixels[iy * tip.side + ix..][..outputs.len() + 1];
    let bottom = &tip.pixels[(iy + 1) * tip.side + ix..][..outputs.len() + 1];
    let mut changed = false;
    for i in 0..outputs.len() {
        let a = u32::from(top[i]) * (256 - fx) + u32::from(top[i + 1]) * fx;
        let b = u32::from(bottom[i]) * (256 - fx) + u32::from(bottom[i + 1]) * fx;
        let value = (a * (256 - fy) + b * fy + 32768) >> 16;
        let amount = divide_unorm16(value * influence + 32767);
        masks[i] += divide_unorm16((65535 - masks[i]) * amount + 32767);
        let coverage = divide_unorm16(opacity * masks[i] + 32767);
        let keep = 65535 - coverage;
        let before = original[i].0;
        let color = source[i].0;
        let after = Rgba8([
            divide_unorm16(u32::from(color[0]) * coverage + u32::from(before[0]) * keep + 32767)
                as u8,
            divide_unorm16(u32::from(color[1]) * coverage + u32::from(before[1]) * keep + 32767)
                as u8,
            divide_unorm16(u32::from(color[2]) * coverage + u32::from(before[2]) * keep + 32767)
                as u8,
            255,
        ]);
        changed |= after != outputs[i];
        outputs[i] = after;
    }
    changed
}
