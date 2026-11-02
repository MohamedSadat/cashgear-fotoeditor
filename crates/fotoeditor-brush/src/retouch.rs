//! Cumulative retouch kernels. A complete dab reads one immutable prior preview.
use super::*;
use fotoeditor_core::{Gray8, Tile, TileRef};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RetouchMode {
    Smudge,
    Blur,
    Sharpen,
    Dodge,
    Burn,
    Sponge,
}
impl RetouchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Smudge => "Smudge",
            Self::Blur => "Blur brush",
            Self::Sharpen => "Sharpen brush",
            Self::Dodge => "Dodge",
            Self::Burn => "Burn",
            Self::Sponge => "Sponge",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TonalRange {
    #[default]
    All,
    Shadows,
    Midtones,
    Highlights,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetouchOptions {
    pub mode: RetouchMode,
    pub strength: f64,
    #[serde(default)]
    pub range: TonalRange,
    #[serde(default)]
    pub saturate: bool,
}
pub(super) fn needed(
    source: &Document,
    dab: &Dab,
    offset: [i32; 2],
    visit: &mut dyn FnMut(TileRef<'_>),
) {
    let bounds = source.canvas_rect();
    // Include unshifted destination, shifted smear source and blur's one-pixel halo.
    let x0 = (i64::from(dab.x0) + i64::from(offset[0].min(0)) - 1).max(i64::from(bounds.x));
    let x1 = (i64::from(dab.x1) + i64::from(offset[0].max(0)) + 1)
        .min(i64::from(bounds.x + bounds.width));
    let y0 = (i64::from(dab.y) + i64::from(offset[1].min(0)) - 1).max(i64::from(bounds.y));
    let y1 = (i64::from(dab.y1) + i64::from(offset[1].max(0)) + 1)
        .min(i64::from(bounds.y + bounds.height));
    for y in y0 as u32 / TILE_SIZE..=(y1 as u32 - 1) / TILE_SIZE {
        for x in x0 as u32 / TILE_SIZE..=(x1 as u32 - 1) / TILE_SIZE {
            if let Some(t) = source.raster().tiles().get(&(x, y)) {
                visit(TileRef::Rgba(t));
            }
        }
    }
}
fn sample(source: &Document, x: i64, y: i64) -> Result<Rgba8, String> {
    let b = source.canvas_rect();
    let x = x.clamp(i64::from(b.x), i64::from(b.x + b.width - 1)) as u32;
    let y = y.clamp(i64::from(b.y), i64::from(b.y + b.height - 1)) as u32;
    Ok(
        if let Some(t) = source.raster().tiles().get(&(x / TILE_SIZE, y / TILE_SIZE)) {
            t.pin()?.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
        } else {
            source.raster().default
        },
    )
}
fn luminance(c: [u8; 4]) -> f64 {
    (0.2126 * f64::from(c[0]) + 0.7152 * f64::from(c[1]) + 0.0722 * f64::from(c[2])) / 255.0
}
fn target(
    source: &Document,
    x: u32,
    y: u32,
    before: Rgba8,
    o: RetouchOptions,
    offset: [i32; 2],
) -> Result<([f64; 3], f64), String> {
    let rgb: [f64; 3] = std::array::from_fn(|c| f64::from(before.0[c]));
    let l = luminance(before.0);
    let weight = match o.range {
        TonalRange::All => 1.0,
        TonalRange::Shadows => (1.0 - l).powi(2),
        TonalRange::Midtones => 4.0 * l * (1.0 - l),
        TonalRange::Highlights => l * l,
    };
    let mut color = rgb;
    let mut amount = 1.0;
    match o.mode {
        RetouchMode::Smudge => {
            let p = sample(
                source,
                i64::from(x) + i64::from(offset[0]),
                i64::from(y) + i64::from(offset[1]),
            )?;
            color = std::array::from_fn(|c| f64::from(p.0[c]));
            amount = f64::from(p.0[3]) / 255.0;
        }
        RetouchMode::Blur | RetouchMode::Sharpen => {
            let mut sum = [0.0; 3];
            let mut alpha = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = sample(source, i64::from(x) + dx, i64::from(y) + dy)?;
                    let w = (if dx == 0 { 2.0 } else { 1.0 })
                        * (if dy == 0 { 2.0 } else { 1.0 })
                        * f64::from(p.0[3]);
                    alpha += w;
                    for (c, value) in sum.iter_mut().enumerate() {
                        *value += w * f64::from(p.0[c]);
                    }
                }
            }
            if alpha > 0.0 {
                color = std::array::from_fn(|c| {
                    let blur = sum[c] / alpha;
                    if o.mode == RetouchMode::Blur {
                        blur
                    } else {
                        (2.0 * rgb[c] - blur).clamp(0.0, 255.0)
                    }
                });
            }
        }
        RetouchMode::Dodge => color = rgb.map(|v| v + (255.0 - v) * weight),
        RetouchMode::Burn => color = rgb.map(|v| v * (1.0 - weight)),
        RetouchMode::Sponge => {
            color = rgb.map(|v| {
                (l * 255.0 + (v - l * 255.0) * if o.saturate { 2.0 } else { 0.0 }).clamp(0.0, 255.0)
            })
        }
    }
    Ok((color, amount))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn stamp_row(
    dab: &Dab,
    tip: &Tip,
    settings: &BrushSettings,
    options: RetouchOptions,
    selection: (Option<&Tile<Gray8>>, u8),
    output: &mut [Rgba8],
    end: u32,
    source: &Document,
    offset: [i32; 2],
) -> Result<bool, String> {
    let scale = tip.radius / dab.radius;
    let sy = (f64::from(dab.y) + 0.5 - dab.p.y) * scale + tip.radius + 0.5;
    let mut changed = false;
    for x in dab.x..end {
        let i = ((dab.y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize;
        let clip = selection.0.map_or(selection.1, |s| s.pixels[i].0);
        let value = tip.sample(
            (f64::from(x) + 0.5 - dab.p.x) * scale + tip.radius + 0.5,
            sy,
        );
        if clip == 0 || value == 0 {
            continue;
        }
        let before = sample(source, i64::from(x), i64::from(dab.y))?;
        if before.0[3] == 0 {
            continue;
        }
        let (color, alpha) = target(source, x, dab.y, before, options, offset)?;
        let amount = f64::from(value) / 65535.0
            * dab.influence
            * settings.opacity
            * options.strength
            * f64::from(clip)
            / 255.0
            * alpha;
        let mut after = before;
        for (c, v) in color.into_iter().enumerate() {
            after.0[c] = (f64::from(before.0[c]) * (1.0 - amount) + v * amount + 0.5) as u8;
        }
        changed |= after != output[i];
        output[i] = after;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
