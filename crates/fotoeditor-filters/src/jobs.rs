use crate::{Adjustment, AdjustmentKernel};
use fotoeditor_core::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

/// Includes next tile/output copies, coordinate directory and compiled LUT.
pub const ADJUSTMENT_SCRATCH: u64 = 1024 * 1024;
pub const HISTOGRAM_SCRATCH: u64 = 64 * 1024;
pub struct AdjustmentJob {
    pub before: Document,
    pub candidate: Document,
    layer: LayerId,
    kernel: AdjustmentKernel,
    label: &'static str,
    coordinates: Vec<TileCoord>,
    next: usize,
    available: u64,
}
impl AdjustmentJob {
    pub fn new(
        document: &Document,
        layer: LayerId,
        adjustment: Adjustment,
        available: u64,
    ) -> Result<Self, String> {
        let kernel = adjustment.compile()?;
        check_layer_edit(&document.layers, layer, false)?;
        let samples = document.layer(layer)?.raster_samples()?;
        if available < ADJUSTMENT_SCRATCH {
            return Err("Not enough shared CPU budget for adjustment".into());
        }
        let mut coordinates: Vec<_> = samples.tiles().keys().copied().collect();
        coordinates.sort_unstable();
        Ok(Self {
            before: document.clone(),
            candidate: document.clone(),
            layer,
            kernel,
            label: adjustment.label(),
            coordinates,
            next: 0,
            available,
        })
    }
    pub fn done(&self) -> bool {
        self.next == self.coordinates.len()
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(c) = self.coordinates.get(self.next) {
            if let Some(t) = self
                .before
                .layer(self.layer)
                .unwrap()
                .raster_samples()
                .unwrap()
                .tiles()
                .get(c)
            {
                visitor(TileRef::Rgba(t));
            }
            if let Some(t) = self
                .before
                .selection
                .as_ref()
                .and_then(|s| s.tiles().get(c))
            {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        self.before.visit_tiles(visitor);
        self.candidate.visit_tiles(visitor);
    }
    pub fn temporary_bytes(&self) -> u64 {
        let mut seen = HashSet::new();
        self.before.visit_tiles(&mut |t| {
            seen.insert(t.id());
        });
        let mut bytes = ADJUSTMENT_SCRATCH;
        self.candidate.visit_tiles(&mut |t| {
            if seen.insert(t.id()) {
                bytes += t.bytes();
            }
        });
        bytes
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if self.temporary_bytes() + (TILE_PIXELS * 4) as u64 > self.available {
            return Err("Adjustment exceeds the shared CPU budget; document unchanged".into());
        }
        let c = self.coordinates[self.next];
        let source = self.before.layer(self.layer)?.raster_samples()?.tiles()[&c].pin()?;
        let selection = self.before.selection.as_ref();
        let mask = selection
            .and_then(|s| s.tiles().get(&c))
            .map(|t| t.pin())
            .transpose()?;
        let mut pixels = source.pixels.clone();
        let mut changed = false;
        for y in 0..TILE_SIZE.min(self.before.height - c.1 * TILE_SIZE) {
            for x in 0..TILE_SIZE.min(self.before.width - c.0 * TILE_SIZE) {
                let i = (y * TILE_SIZE + x) as usize;
                let coverage = mask
                    .as_ref()
                    .map_or_else(|| selection.map_or(255, |s| s.default.0), |m| m.pixels[i].0);
                let after = self.kernel.sample(pixels[i], coverage);
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
            return Err("Adjustment is incomplete".into());
        }
        Ok(TileCommand::for_layer(&self.before, &self.candidate, self.layer)?.named(self.label))
    }
}

#[derive(Serialize, Default)]
pub struct Histogram {
    /// R/G/B/luma, each with 256 bins. Counts are exact alpha weights.
    pub bins: Vec<Vec<u64>>,
    pub weight: u64,
    pub pixels: u64,
}
pub struct HistogramJob {
    source: Arc<TileSurface<Rgba8>>,
    coordinates: Vec<TileCoord>,
    next: usize,
    pub output: Histogram,
}
impl HistogramJob {
    pub fn new(document: &Document, layer: LayerId) -> Result<Self, String> {
        let source = Arc::new(document.layer(layer)?.raster_samples()?.clone());
        let mut coordinates: Vec<_> = source.tiles().keys().copied().collect();
        coordinates.sort_unstable();
        Ok(Self {
            source,
            coordinates,
            next: 0,
            output: Histogram {
                bins: vec![vec![0; 256]; 4],
                weight: 0,
                pixels: 0,
            },
        })
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for t in self.source.tiles().values() {
            visitor(TileRef::Rgba(t));
        }
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(c) = self.coordinates.get(self.next) {
            visitor(TileRef::Rgba(&self.source.tiles()[c]));
        }
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if let Some(c) = self.coordinates.get(self.next) {
            let tile = self.source.tiles()[c].pin()?;
            for y in 0..TILE_SIZE.min(self.source.height - c.1 * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(self.source.width - c.0 * TILE_SIZE) {
                    let p = tile.pixels[(y * TILE_SIZE + x) as usize].0;
                    let alpha = u64::from(p[3]);
                    if alpha == 0 {
                        continue;
                    }
                    for (channel, value) in p[..3].iter().enumerate() {
                        self.output.bins[channel][*value as usize] += alpha;
                    }
                    let luma = (u32::from(p[0]) * 2126
                        + u32::from(p[1]) * 7152
                        + u32::from(p[2]) * 722
                        + 5000)
                        / 10000;
                    self.output.bins[3][luma as usize] += alpha;
                    self.output.weight += alpha;
                    self.output.pixels += 1;
                }
            }
            self.next += 1;
        }
        Ok(self.next == self.coordinates.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn invert() -> Adjustment {
        Adjustment::Curves {
            points: vec![[0.0, 1.0], [1.0, 0.0]],
        }
    }
    #[test]
    fn tile_seams_partial_selection_and_exact_undo_redo() {
        let mut d = Document::blank(513, 3, 72.0).unwrap();
        for x in [0, 255, 256, 511, 512] {
            d.raster_mut().write(x, 1, Rgba8([0, 100, 200, 1]));
        }
        d.raster_mut().write(256, 2, Rgba8([87, 33, 19, 0]));
        let mut mask = TileSurface::new(513, 3);
        for x in [255, 256, 511, 512] {
            mask.write(x, 1, Gray8(128));
        }
        d.selection = Some(Arc::new(mask));
        let mut j = AdjustmentJob::new(&d, d.layers[0].id, invert(), 4 << 20).unwrap();
        assert!(j.finish().is_err());
        j = AdjustmentJob::new(&d, d.layers[0].id, invert(), 4 << 20).unwrap();
        while !j.step().unwrap() {}
        assert_eq!(j.candidate.raster().sample(0, 1), Rgba8([0, 100, 200, 1]));
        for x in [255, 256, 511, 512] {
            assert_eq!(j.candidate.raster().sample(x, 1), Rgba8([128, 128, 127, 1]));
        }
        assert_eq!(j.candidate.raster().sample(256, 2), Rgba8([87, 33, 19, 0]));
        let mut history = History::default();
        history
            .execute(&mut d, Box::new(j.finish().unwrap()), 0)
            .unwrap();
        assert!(history.undo(&mut d));
        assert_eq!(d.raster().sample(256, 1), Rgba8([0, 100, 200, 1]));
        assert!(history.redo(&mut d));
        assert_eq!(d.raster().sample(256, 1), Rgba8([128, 128, 127, 1]));
    }
    #[test]
    fn noop_cancel_refusal_and_locks_preserve_redo() {
        let mut d = Document::blank(257, 1, 72.0).unwrap();
        d.raster_mut().write(0, 0, Rgba8([10, 20, 30, 255]));
        let id = d.layers[0].id;
        let mut history = History::default();
        let mut j = AdjustmentJob::new(&d, id, invert(), 4 << 20).unwrap();
        while !j.step().unwrap() {}
        history
            .execute(&mut d, Box::new(j.finish().unwrap()), 0)
            .unwrap();
        history.undo(&mut d);
        d.selection = Some(Arc::new(TileSurface::new(257, 1)));
        let mut j = AdjustmentJob::new(&d, id, invert(), 4 << 20).unwrap();
        while !j.step().unwrap() {}
        assert!(!history
            .execute(&mut d, Box::new(j.finish().unwrap()), 0)
            .unwrap());
        assert_eq!(history.depths(), (0, 1));
        d.selection = None;
        let mut j = AdjustmentJob::new(&d, id, invert(), ADJUSTMENT_SCRATCH).unwrap();
        assert!(j.step().is_err());
        drop(j);
        let mut j = AdjustmentJob::new(&d, id, invert(), 4 << 20).unwrap();
        j.step().unwrap();
        drop(j);
        assert_eq!(d.raster().sample(0, 0), Rgba8([10, 20, 30, 255]));
        assert_eq!(history.depths(), (0, 1));
        d.layers[0].properties.locks.alpha = true;
        assert!(AdjustmentJob::new(&d, id, invert(), 4 << 20).is_ok());
        d.layers[0].properties.locks.pixels = true;
        assert!(AdjustmentJob::new(&d, id, invert(), 4 << 20).is_err());
    }
    #[test]
    fn histogram_exact_alpha_weighting_and_odd_canvas_padding() {
        let mut d = Document::blank(257, 1, 72.0).unwrap();
        d.raster_mut().write(0, 0, Rgba8([255, 0, 0, 255]));
        d.raster_mut().write(256, 0, Rgba8([0, 255, 0, 128]));
        d.raster_mut().write(255, 0, Rgba8([87, 33, 19, 0]));
        let mut j = HistogramJob::new(&d, d.layers[0].id).unwrap();
        while !j.step().unwrap() {}
        assert_eq!(j.output.weight, 383);
        assert_eq!(j.output.pixels, 2);
        assert_eq!(j.output.bins[0][255], 255);
        assert_eq!(j.output.bins[1][255], 128);
        assert_eq!(j.output.bins[3][54], 255);
        assert_eq!(j.output.bins[3][182], 128);
        for bins in &j.output.bins {
            assert_eq!(bins.iter().sum::<u64>(), 383);
        }
        assert_eq!(d.raster().sample(255, 0), Rgba8([87, 33, 19, 0]));
    }
    #[test]
    fn cold_source_and_selection_require_hydration_without_partial_mutation() {
        let mut d = Document::blank(257, 1, 72.0).unwrap();
        d.raster_mut().write(256, 0, Rgba8([10, 20, 30, 128]));
        let mut s = TileSurface::new(257, 1);
        s.write(256, 0, Gray8(128));
        d.selection = Some(Arc::new(s));
        let rgba = d.raster().tiles()[&(1, 0)].clone();
        let gray = d.selection.as_ref().unwrap().tiles()[&(1, 0)].clone();
        let pixels = rgba.pin().unwrap().pixels.clone();
        let coverage = gray.pin().unwrap().pixels.clone();
        let rb = TileBacking {
            store: 1,
            offset: 0,
            length: 262144,
            checksum: 1,
        };
        let gb = TileBacking {
            store: 1,
            offset: 262144,
            length: 65536,
            checksum: 2,
        };
        let mut rp = TilePager::default();
        let mut gp = TilePager::default();
        rp.acknowledge_backing(&rgba, rb).unwrap();
        gp.acknowledge_backing(&gray, gb).unwrap();
        rp.evict(&rgba).unwrap();
        gp.evict(&gray).unwrap();
        let mut j = AdjustmentJob::new(&d, d.layers[0].id, invert(), 4 << 20).unwrap();
        let mut ids = Vec::new();
        j.needed(&mut |t| ids.push(t.id()));
        assert_eq!(ids, vec![rgba.id(), gray.id()]);
        assert!(j.step().is_err());
        rp.hydrate(&rgba, rb, pixels, Residency::default()).unwrap();
        assert!(j.step().is_err());
        gp.hydrate(&gray, gb, coverage, Residency::default())
            .unwrap();
        assert!(j.step().unwrap());
        assert_eq!(d.raster().sample(256, 0), Rgba8([10, 20, 30, 128]));
        assert_eq!(
            j.candidate.raster().sample(256, 0),
            Rgba8([128, 128, 128, 128])
        );
        rp.evict(&rgba).unwrap();
        let mut h = HistogramJob::new(&d, d.layers[0].id).unwrap();
        assert!(h.step().is_err());
        assert_eq!(h.output.weight, 0);
    }
}
