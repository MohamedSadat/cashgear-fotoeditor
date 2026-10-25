use fotoeditor_core::*;
use std::collections::HashSet;
use std::sync::Arc;

/// A common source-over/erase contract for selection-aware destructive tools.
/// Zero coverage returns the original straight sample, including hidden RGB.
pub fn fill_sample(
    before: Rgba8,
    color: [u8; 4],
    coverage: u8,
    clear: bool,
    alpha_lock: bool,
) -> Rgba8 {
    if coverage == 0 {
        return before;
    }
    let source = f64::from(coverage) / 255.0
        * if clear {
            1.0
        } else {
            f64::from(color[3]) / 255.0
        };
    if source == 0.0 || (alpha_lock && (clear || before.0[3] == 0)) {
        return before;
    }
    let mut out = before.0;
    if clear {
        out[3] = (f64::from(out[3]) * (1.0 - source) + 0.5) as u8;
    } else if alpha_lock {
        for c in 0..3 {
            out[c] = (f64::from(before.0[c]) * (1.0 - source) + f64::from(color[c]) * source + 0.5)
                as u8;
        }
    } else {
        let destination = f64::from(before.0[3]) / 255.0 * (1.0 - source);
        let alpha = source + destination;
        for c in 0..3 {
            out[c] = ((f64::from(color[c]) * source + f64::from(before.0[c]) * destination) / alpha
                + 0.5) as u8;
        }
        out[3] = (alpha * 255.0 + 0.5) as u8;
    }
    Rgba8(out)
}
pub struct FillJob {
    pub before: Document,
    pub candidate: Document,
    layer: LayerId,
    region: Option<Arc<TileSurface<Gray8>>>,
    color: [u8; 4],
    clear: bool,
    alpha_lock: bool,
    next: u32,
    available: u64,
    label: &'static str,
}
impl FillJob {
    pub fn named(mut self, label: &'static str) -> Self {
        self.label = label;
        self
    }
    pub fn new(
        document: &Document,
        layer: LayerId,
        region: Option<Arc<TileSurface<Gray8>>>,
        color: [u8; 4],
        clear: bool,
        available: u64,
    ) -> Result<Self, String> {
        check_layer_edit(&document.layers, layer, false)?;
        document.layer(layer)?.raster_samples()?;
        if region
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (document.width, document.height))
        {
            return Err("Fill region dimensions differ".into());
        }
        if available < 2 * (TILE_PIXELS * 4) as u64 {
            return Err("Not enough memory for fill".into());
        }
        Ok(Self {
            before: document.clone(),
            candidate: document.clone(),
            layer,
            region,
            color,
            clear,
            alpha_lock: document.layer(layer)?.properties.locks.alpha,
            next: 0,
            available,
            label: if clear { "Clear selection" } else { "Fill" },
        })
    }
    fn coord(&self) -> TileCoord {
        (
            self.next % self.before.width.div_ceil(TILE_SIZE),
            self.next / self.before.width.div_ceil(TILE_SIZE),
        )
    }
    pub fn done(&self) -> bool {
        self.next == self.before.width.div_ceil(TILE_SIZE) * self.before.height.div_ceil(TILE_SIZE)
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if self.done() {
            return;
        }
        let c = self.coord();
        if let Some(t) = self
            .before
            .layer(self.layer)
            .unwrap()
            .raster_samples()
            .unwrap()
            .tiles()
            .get(&c)
        {
            visitor(TileRef::Rgba(t));
        }
        for s in [&self.before.selection, &self.region].into_iter().flatten() {
            if let Some(t) = s.tiles().get(&c) {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn visit_candidates(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        self.before.visit_tiles(visitor);
        self.candidate.visit_tiles(visitor);
        if let Some(s) = &self.region {
            for t in s.tiles().values() {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn temporary_bytes(&self) -> u64 {
        let mut seen = HashSet::new();
        self.before.visit_tiles(&mut |t| {
            seen.insert(t.id());
        });
        let mut bytes = 0;
        self.candidate.visit_tiles(&mut |t| {
            if seen.insert(t.id()) {
                bytes += t.bytes();
            }
        });
        bytes + 2 * (TILE_PIXELS * 4) as u64
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if self.temporary_bytes() + (TILE_PIXELS * 4) as u64 > self.available {
            return Err("Fill exceeds the shared CPU budget; document unchanged".into());
        }
        let c = self.coord();
        let samples = self.before.layer(self.layer)?.raster_samples()?;
        let old = samples.tiles().get(&c).map(|t| t.pin()).transpose()?;
        let selection = self.before.selection.as_ref();
        let region = self.region.as_ref();
        let selected = selection
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let mask = region
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let mut pixels = old
            .as_ref()
            .map_or_else(|| vec![samples.default; TILE_PIXELS], |t| t.pixels.clone());
        let mut changed = false;
        for y in 0..TILE_SIZE.min(self.before.height - c.1 * TILE_SIZE) {
            for x in 0..TILE_SIZE.min(self.before.width - c.0 * TILE_SIZE) {
                if !self
                    .before
                    .canvas_rect()
                    .contains(c.0 * TILE_SIZE + x, c.1 * TILE_SIZE + y)
                {
                    continue;
                }
                let i = (y * TILE_SIZE + x) as usize;
                let a = selected
                    .as_ref()
                    .map_or_else(|| selection.map_or(255, |s| s.default.0), |t| t.pixels[i].0);
                let b = mask
                    .as_ref()
                    .map_or_else(|| region.map_or(255, |s| s.default.0), |t| t.pixels[i].0);
                let after = fill_sample(
                    pixels[i],
                    self.color,
                    ((u16::from(a) * u16::from(b) + 127) / 255) as u8,
                    self.clear,
                    self.alpha_lock,
                );
                changed |= after != pixels[i];
                pixels[i] = after;
            }
        }
        if changed {
            self.candidate
                .layer_mut(self.layer)?
                .raster_samples_mut()?
                .insert(c, pixels);
        }
        self.next += 1;
        Ok(self.done())
    }
    pub fn finish(self) -> Result<TileCommand, String> {
        if !self.done() {
            return Err("Fill is incomplete".into());
        }
        Ok(TileCommand::for_layer(&self.before, &self.candidate, self.layer)?.named(self.label))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fill_clear_locks_exact_undo_and_budget_rejection() {
        let mut d = Document::blank(257, 1, 72.0).unwrap();
        d.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        d.raster_mut().write(256, 0, Rgba8([201, 3, 7, 255]));
        let mut s = TileSurface::new(257, 1);
        s.write(256, 0, Gray8(128));
        d.selection = Some(Arc::new(s));
        let target = d.layers[0].id;
        let mut j =
            FillJob::new(&d, target, None, [0, 0, 255, 255], false, 4 * 1024 * 1024).unwrap();
        while !j.step().unwrap() {}
        assert_eq!(j.candidate.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
        assert_eq!(
            j.candidate.raster().sample(256, 0),
            Rgba8([100, 1, 131, 255])
        );
        let mut h = History::default();
        h.execute(&mut d, Box::new(j.finish().unwrap()), 0).unwrap();
        assert_eq!(h.labels(), vec!["Fill"]);
        assert!(h.undo(&mut d));
        assert_eq!(d.raster().sample(256, 0), Rgba8([201, 3, 7, 255]));
        let mut j = FillJob::new(&d, target, None, [0; 4], true, 4 * 1024 * 1024).unwrap();
        while !j.step().unwrap() {}
        assert_eq!(j.candidate.raster().sample(256, 0), Rgba8([201, 3, 7, 127]));
        d.layers[0].properties.locks.pixels = true;
        assert!(FillJob::new(&d, target, None, [0; 4], true, 1 << 20).is_err());
        assert!(FillJob::new(&d, target, None, [0; 4], true, 0).is_err());
        assert_eq!(h.depths(), (0, 1));
    }
}
