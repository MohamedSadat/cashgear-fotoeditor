//! A bounded display mirror only. Authoritative selection samples remain Gray8.
use crate::*;
use std::sync::Arc;

pub enum SelectionDisplaySource<'a> {
    Gray(&'a TileSurface<Gray8>),
    Quick(&'a TileSurface<Rgba8>),
    All,
}
fn pin<P: Pixel>(
    surface: &TileSurface<P>,
    view: &View,
) -> Result<Vec<Option<Arc<Tile<P>>>>, String> {
    let cols = surface.width.div_ceil(TILE_SIZE);
    let rows = surface.height.div_ceil(TILE_SIZE);
    let mut tiles = vec![None; (cols * rows) as usize];
    for (c, t) in surface.tiles() {
        if view.visible(c.0 * TILE_SIZE, c.1 * TILE_SIZE, TILE_SIZE, TILE_SIZE) {
            tiles[(c.1 * cols + c.0) as usize] = Some(t.pin()?);
        }
    }
    Ok(tiles)
}
/// Interleaved coverage/inside-canvas bytes, at most 2048²×2 = 8 MiB.
pub fn selection_display(
    source: SelectionDisplaySource<'_>,
    view: &View,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return Err("Selection display exceeds its 2048 pixel side bound".into());
    }
    let gray = if let SelectionDisplaySource::Gray(s) = source {
        Some((s, pin(s, view)?))
    } else {
        None
    };
    let quick = if let SelectionDisplaySource::Quick(s) = source {
        Some((s, pin(s, view)?))
    } else {
        None
    };
    let mut bytes = vec![0; (width * height * 2) as usize];
    let dx = view.screen_to_document(view.width / f64::from(width), 0.0);
    let origin = view.screen_to_document(0.0, 0.0);
    let step = (dx.0 - origin.0, dx.1 - origin.1);
    for y in 0..height {
        let mut p = view.screen_to_document(
            view.width / (2.0 * f64::from(width)),
            (f64::from(y) + 0.5) * view.height / f64::from(height),
        );
        for x in 0..width {
            if view.inside_canvas(p.0, p.1) {
                let xx = p.0.floor() as u32;
                let yy = p.1.floor() as u32;
                let offset = ((yy % TILE_SIZE) * TILE_SIZE + xx % TILE_SIZE) as usize;
                let value = if let Some((s, tiles)) = &gray {
                    tiles
                        [((yy / TILE_SIZE) * s.width.div_ceil(TILE_SIZE) + xx / TILE_SIZE) as usize]
                        .as_ref()
                        .map_or(s.default.0, |t| t.pixels[offset].0)
                } else if let Some((s, tiles)) = &quick {
                    tiles
                        [((yy / TILE_SIZE) * s.width.div_ceil(TILE_SIZE) + xx / TILE_SIZE) as usize]
                        .as_ref()
                        .map_or(s.default.0[0], |t| t.pixels[offset].0[0])
                } else {
                    255
                };
                let i = ((y * width + x) * 2) as usize;
                bytes[i] = value;
                bytes[i + 1] = 255;
            }
            p.0 += step.0;
            p.1 += step.1;
        }
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_coverage_matches_document_transform_and_never_changes_samples() {
        let mut s = TileSurface::new(513, 2);
        s.write(256, 1, Gray8(128));
        let v = View {
            width: 513.0,
            height: 2.0,
            document_width: 513.0,
            document_height: 2.0,
            ..View::default()
        };
        let bytes = selection_display(SelectionDisplaySource::Gray(&s), &v, 513, 2).unwrap();
        assert_eq!(bytes[((513 + 256) * 2)..((513 + 256) * 2 + 2)], [128, 255]);
        assert_eq!(s.sample(256, 1), Gray8(128));
        let v = View { flipped: true, ..v };
        let bytes = selection_display(SelectionDisplaySource::Gray(&s), &v, 513, 2).unwrap();
        assert_eq!(bytes[((513 + 256) * 2)..((513 + 256) * 2 + 2)], [128, 255]);
        assert!(selection_display(SelectionDisplaySource::All, &v, 4096, 2).is_err());
    }
}
