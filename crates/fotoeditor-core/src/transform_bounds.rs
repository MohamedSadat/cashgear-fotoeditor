//! Pageable bounds of meaningful straight samples, linked masks or selection.
use crate::*;
use std::sync::Arc;
#[derive(Clone, Copy)]
enum Plane {
    Raster,
    Mask,
    Selection,
}

pub struct TransformBoundsJob {
    source: Arc<Document>,
    target: LayerId,
    selected: bool,
    tasks: Vec<(Plane, TileCoord)>,
    next: usize,
    low: [u32; 2],
    high: [u32; 2],
}
impl TransformBoundsJob {
    pub fn new(s: &TransformSession) -> Result<Self, String> {
        let mut tasks = vec![];
        let mut add = |plane, coords: Vec<TileCoord>| {
            let mut coords = coords;
            coords.sort_unstable();
            tasks.extend(coords.into_iter().map(|c| (plane, c)));
        };
        let full = || {
            let mut c = vec![];
            for y in 0..s.source.height.div_ceil(TILE_SIZE) {
                for x in 0..s.source.width.div_ceil(TILE_SIZE) {
                    c.push((x, y));
                }
            }
            c
        };
        if s.selected {
            let selection = s
                .source
                .selection
                .as_ref()
                .ok_or("No selection for bounds")?;
            add(
                Plane::Selection,
                if selection.default.0 > 0 {
                    full()
                } else {
                    selection.tiles().keys().copied().collect()
                },
            );
        } else {
            let l = s.source.layer(s.target)?;
            let raster = l.raster_samples()?;
            add(
                Plane::Raster,
                if raster.default != Rgba8::default() {
                    full()
                } else {
                    raster.tiles().keys().copied().collect()
                },
            );
            if let Some(m) = &l.mask {
                add(Plane::Mask, m.samples.tiles().keys().copied().collect());
            }
        }
        Ok(Self {
            source: s.source.clone(),
            target: s.target,
            selected: s.selected,
            tasks,
            next: 0,
            low: [u32::MAX; 2],
            high: [0; 2],
        })
    }
    pub fn done(&self) -> bool {
        self.next == self.tasks.len()
    }
    pub fn progress(&self) -> f64 {
        if self.tasks.is_empty() {
            1.0
        } else {
            self.next as f64 / self.tasks.len() as f64
        }
    }
    pub fn scratch_bytes(&self) -> u64 {
        512 * 1024
    }
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(&(p, c)) = self.tasks.get(self.next) {
            let l = self.source.layer(self.target).unwrap();
            match p {
                Plane::Raster => {
                    if let Some(t) = l.raster_samples().unwrap().tiles().get(&c) {
                        v(TileRef::Rgba(t));
                    }
                }
                Plane::Mask | Plane::Selection => {
                    let s = if matches!(p, Plane::Mask) {
                        &l.mask.as_ref().unwrap().samples
                    } else {
                        self.source.selection.as_ref().unwrap()
                    };
                    if let Some(t) = s.tiles().get(&c) {
                        v(TileRef::Gray(t));
                    }
                }
            }
        }
    }
    pub fn step(&mut self) -> Result<bool, String> {
        let Some(&(p, c)) = self.tasks.get(self.next) else {
            return Ok(true);
        };
        let rect = CanvasRect {
            x: c.0 * TILE_SIZE,
            y: c.1 * TILE_SIZE,
            width: TILE_SIZE.min(self.source.width - c.0 * TILE_SIZE),
            height: TILE_SIZE.min(self.source.height - c.1 * TILE_SIZE),
        };
        let layer = self.source.layer(self.target)?;
        let mut low = self.low;
        let mut high = self.high;
        let mut include = |x: u32, y: u32| {
            low[0] = low[0].min(x);
            low[1] = low[1].min(y);
            high[0] = high[0].max(x + 1);
            high[1] = high[1].max(y + 1);
        };
        match p {
            Plane::Raster => {
                let s = layer.raster_samples()?;
                let pin = s.tiles().get(&c).map(TileHandle::pin).transpose()?;
                if pin.as_ref().is_some_and(|t| t.known_opaque)
                    || pin.is_none() && s.default != Rgba8::default()
                {
                    include(rect.x, rect.y);
                    include(rect.x + rect.width - 1, rect.y + rect.height - 1);
                } else {
                    for y in 0..rect.height {
                        for x in 0..rect.width {
                            if pin
                                .as_ref()
                                .map_or(s.default, |t| t.pixels[(y * TILE_SIZE + x) as usize])
                                != Rgba8::default()
                            {
                                include(rect.x + x, rect.y + y);
                            }
                        }
                    }
                }
            }
            Plane::Mask | Plane::Selection => {
                let s = if matches!(p, Plane::Mask) {
                    &layer
                        .mask
                        .as_ref()
                        .ok_or("Mask bounds source changed")?
                        .samples
                } else {
                    self.source
                        .selection
                        .as_ref()
                        .ok_or("Selection bounds source changed")?
                };
                let pin = s.tiles().get(&c).map(TileHandle::pin).transpose()?;
                let canvas = self.source.canvas_rect();
                for y in 0..rect.height {
                    for x in 0..rect.width {
                        let value = pin
                            .as_ref()
                            .map_or(s.default, |t| t.pixels[(y * TILE_SIZE + x) as usize]);
                        if if matches!(p, Plane::Selection) {
                            value.0 > 0 && canvas.contains(rect.x + x, rect.y + y)
                        } else {
                            value != s.default
                        } {
                            include(rect.x + x, rect.y + y);
                        }
                    }
                }
            }
        }
        self.low = low;
        self.high = high;
        self.next += 1;
        Ok(self.done())
    }
    pub fn finish(self, s: &mut TransformSession) -> Result<(), String> {
        if !self.done()
            || !Arc::ptr_eq(&self.source, &s.source)
            || self.target != s.target
            || self.selected != s.selected
        {
            return Err("Transform bounds source changed or is incomplete".into());
        }
        let bounds = (self.high[0] > 0 && self.high[1] > 0).then(|| TransformRect {
            x: self.low[0] as f64,
            y: self.low[1] as f64,
            width: (self.high[0] - self.low[0]) as f64,
            height: (self.high[1] - self.low[1]) as f64,
        });
        s.accept_bounds(bounds);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scan(s: &mut TransformSession) {
        let mut j = TransformBoundsJob::new(s).unwrap();
        while !j.step().unwrap() {}
        j.finish(s).unwrap();
    }
    #[test]
    fn exact_bounds_include_hidden_rgb_linked_mask_and_selected_coverage() {
        let mut d = Document::blank(1040, 777, 72.0).unwrap();
        d.raster_mut().write(512, 256, Rgba8([87, 33, 19, 0]));
        let mut mask = TileSurface::with_default(1040, 777, Gray8(255));
        mask.write(700, 300, Gray8(128));
        d.layers[0].mask = Some(LayerMask {
            enabled: false,
            samples: Arc::new(mask),
        });
        let mut selection = TileSurface::new(1040, 777);
        selection.write(77, 22, Gray8(128));
        selection.write(7, 22, Gray8(255));
        d.selection = Some(Arc::new(selection));
        d.canvas = Some(CanvasRect {
            x: 50,
            y: 0,
            width: 100,
            height: 100,
        });
        let target = d.layers[0].id;
        let source = Arc::new(d);
        let mut whole = TransformSession::new(source.clone(), target, false).unwrap();
        scan(&mut whole);
        assert_eq!(
            whole.bounds,
            TransformRect {
                x: 512.0,
                y: 256.0,
                width: 189.0,
                height: 45.0
            }
        );
        let mut selected = TransformSession::new(source, target, true).unwrap();
        scan(&mut selected);
        assert_eq!(
            selected.bounds,
            TransformRect {
                x: 77.0,
                y: 22.0,
                width: 1.0,
                height: 1.0
            }
        );
    }
    #[test]
    fn cold_bounds_refuse_then_resume_and_require_the_same_immutable_source() {
        let mut d = Document::blank(513, 259, 72.0).unwrap();
        d.raster_mut().write(300, 200, Rgba8([1, 2, 3, 1]));
        let target = d.layers[0].id;
        let tile = d.raster().tiles()[&(1, 0)].clone();
        let pixels = tile.pin().unwrap().pixels.clone();
        let backing = TileBacking {
            store: 1,
            offset: 0,
            length: (TILE_PIXELS * 4) as u32,
            checksum: 0,
        };
        let mut pager = TilePager::default();
        pager.acknowledge_backing(&tile, backing).unwrap();
        pager.evict(&tile).unwrap();
        let mut session = TransformSession::new(Arc::new(d), target, false).unwrap();
        let mut job = TransformBoundsJob::new(&session).unwrap();
        let mut needed = vec![];
        job.needed(&mut |t| needed.push(t.id()));
        assert_eq!(needed, vec![tile.id()]);
        assert!(job.step().is_err());
        pager
            .hydrate(&tile, backing, pixels, Residency::default())
            .unwrap();
        while !job.step().unwrap() {}
        job.finish(&mut session).unwrap();
        assert_eq!(
            session.bounds,
            TransformRect {
                x: 300.0,
                y: 200.0,
                width: 1.0,
                height: 1.0
            }
        );
    }
    #[test]
    fn sparse_maximum_canvas_nudges_do_not_accumulate_empty_padding() {
        let mut d = Document::blank(MAX_SIDE, MAX_SIDE, 72.0).unwrap();
        d.raster_mut().write(300, 200, Rgba8([87, 33, 19, 0]));
        let target = d.layers[0].id;
        for dx in [1.0, -1.0] {
            let mut j = GeometryJob::new(
                Arc::new(d.clone()),
                GeometryAction::LayerTransform {
                    target,
                    matrix: TransformMatrix::translation(dx, 0.0),
                    interpolation: Interpolation::Lanczos3,
                    selection: false,
                },
                CPU_BUDGET,
            )
            .unwrap();
            while !j.step().unwrap() {}
            j.finish().unwrap().apply(&mut d);
            assert_eq!((d.width, d.height), (MAX_SIDE, MAX_SIDE));
            assert!(d.canvas.is_none());
        }
        assert_eq!(d.raster().sample(300, 200), Rgba8([87, 33, 19, 0]));
        assert_eq!(d.raster().tiles().len(), 1);
    }
    #[test]
    fn perspective_horizon_in_empty_space_does_not_reject_a_valid_content_quad() {
        let mut d = Document::blank(100, 80, 72.0).unwrap();
        d.raster_mut().write(10, 20, Rgba8([200, 30, 70, 255]));
        let target = d.layers[0].id;
        let mut s = TransformSession::new(Arc::new(d.clone()), target, false).unwrap();
        scan(&mut s);
        let matrix = TransformMatrix {
            values: [1.0, 0.0, 100.0, 0.0, 1.0, 0.0, -0.01, 0.0, 0.5],
        };
        assert!(matrix
            .validate_for(TransformRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0
            })
            .is_err());
        s.update(TransformUpdate::Matrix { matrix }).unwrap();
        let mut j = GeometryJob::new(
            s.source.clone(),
            GeometryAction::LayerTransform {
                target,
                matrix,
                interpolation: Interpolation::Nearest,
                selection: false,
            },
            CPU_BUDGET,
        )
        .unwrap();
        while !j.step().unwrap() {}
        let command = j.finish().unwrap();
        command.apply(&mut d);
        assert!(!d.raster().tiles().is_empty());
        command.revert(&mut d);
        assert_eq!(d.raster().sample(10, 20), Rgba8([200, 30, 70, 255]));
    }
}
