//! Lift selected coverage once, clear once, resample once and composite once.
use crate::*;
use std::{collections::HashSet, sync::Arc};
enum Stage {
    Lift(u32),
    Transform(Box<GeometryJob>),
    Rebase(Box<SurfaceResampleJob<Rgba8>>),
    Composite(u32),
    Done,
}
pub struct SelectedTransformJob {
    source: Arc<Document>,
    target: LayerId,
    matrix: TransformMatrix,
    mode: Interpolation,
    available: u64,
    lifted: TileSurface<Rgba8>,
    cleared: TileSurface<Rgba8>,
    coverage: TileSurface<Gray8>,
    output: Document,
    base: Option<Arc<TileSurface<Rgba8>>>,
    stage: Stage,
    domain: Option<TransformRect>,
}
fn visit<P: ResamplePixel>(s: &TileSurface<P>, v: &mut dyn FnMut(TileRef<'_>)) {
    for t in s.tiles().values() {
        v(P::reference(t));
    }
}
fn pin<P: Pixel>(s: &TileSurface<P>, c: TileCoord) -> Result<Option<Arc<Tile<P>>>, String> {
    s.tiles().get(&c).map(TileHandle::pin).transpose()
}
fn sample<P: Pixel>(s: &TileSurface<P>, t: &Option<Arc<Tile<P>>>, i: usize) -> P {
    t.as_ref().map_or(s.default, |t| t.pixels[i])
}
impl SelectedTransformJob {
    pub fn new(
        source: Arc<Document>,
        target: LayerId,
        matrix: TransformMatrix,
        mode: Interpolation,
        available: u64,
    ) -> Result<Self, String> {
        Self::with_domain(source, target, matrix, mode, available, None)
    }
    pub(crate) fn with_domain(
        source: Arc<Document>,
        target: LayerId,
        matrix: TransformMatrix,
        mode: Interpolation,
        available: u64,
        domain: Option<TransformRect>,
    ) -> Result<Self, String> {
        check_layer_edit(&source.layers, target, true)?;
        check_layer_edit(&source.layers, target, false)?;
        if source.layer(target)?.properties.locks.alpha {
            return Err("Unlock alpha to transform selected pixels".into());
        }
        let raster = source.layer(target)?.raster_samples()?;
        let selection = source.selection.as_ref().ok_or("No active selection")?;
        let bounds = TransformRect {
            x: 0.0,
            y: 0.0,
            width: source.width as f64,
            height: source.height as f64,
        };
        matrix.validate_for(domain.unwrap_or(bounds))?;
        let identity = bounds.corners().iter().all(|p| {
            matrix
                .map(*p)
                .is_ok_and(|q| (p[0] - q[0]).abs().max((p[1] - q[1]).abs()) < 1e-8)
        });
        let empty = selection.default == Gray8(0) && selection.tiles().is_empty();
        if !identity && !empty && available < RESAMPLE_SCRATCH + TILE_PIXELS as u64 * 16 {
            return Err("Not enough shared CPU budget for selected transform".into());
        }
        Ok(Self {
            output: (*source).clone(),
            target,
            matrix,
            mode,
            available,
            domain,
            lifted: TileSurface::new(source.width, source.height),
            cleared: raster.clone(),
            coverage: TileSurface::new(source.width, source.height),
            base: None,
            stage: if identity || empty {
                Stage::Done
            } else {
                Stage::Lift(0)
            },
            source,
        })
    }
    pub fn done(&self) -> bool {
        matches!(self.stage, Stage::Done)
    }
    pub fn progress(&self) -> f64 {
        match self.stage {
            Stage::Lift(n) => 0.25 * n as f64 / self.tile_count() as f64,
            Stage::Transform(_) => 0.3,
            Stage::Rebase(_) => 0.6,
            Stage::Composite(n) => 0.75 + 0.25 * n as f64 / self.output_count() as f64,
            Stage::Done => 1.0,
        }
    }
    pub fn scratch_bytes(&self) -> u64 {
        if self.done() {
            0
        } else {
            RESAMPLE_SCRATCH + TILE_PIXELS as u64 * 16
        }
    }
    fn tile_count(&self) -> u32 {
        self.source.width.div_ceil(TILE_SIZE) * self.source.height.div_ceil(TILE_SIZE)
    }
    fn output_count(&self) -> u32 {
        self.output.width.div_ceil(TILE_SIZE) * self.output.height.div_ceil(TILE_SIZE)
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.source.visit_tiles(v);
        self.output.visit_tiles(v);
        visit(&self.lifted, v);
        visit(&self.cleared, v);
        visit(&self.coverage, v);
        if let Some(b) = &self.base {
            visit(b, v);
        }
        match &self.stage {
            Stage::Transform(j) => j.visit_tiles(v),
            Stage::Rebase(j) => j.visit_tiles(v),
            _ => {}
        }
    }
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        let add = |s: &TileSurface<Rgba8>, c, v: &mut dyn FnMut(TileRef<'_>)| {
            if let Some(t) = s.tiles().get(&c) {
                v(TileRef::Rgba(t));
            }
        };
        match &self.stage {
            Stage::Lift(n) => {
                let c = (
                    n % self.source.width.div_ceil(TILE_SIZE),
                    n / self.source.width.div_ceil(TILE_SIZE),
                );
                add(
                    self.source
                        .layer(self.target)
                        .unwrap()
                        .raster_samples()
                        .unwrap(),
                    c,
                    v,
                );
                if let Some(t) = self
                    .source
                    .selection
                    .as_ref()
                    .and_then(|s| s.tiles().get(&c))
                {
                    v(TileRef::Gray(t));
                }
            }
            Stage::Transform(j) => j.needed(v),
            Stage::Rebase(j) => j.needed(v),
            Stage::Composite(n) => {
                let c = (
                    n % self.output.width.div_ceil(TILE_SIZE),
                    n / self.output.width.div_ceil(TILE_SIZE),
                );
                add(
                    self.output
                        .layer(self.target)
                        .unwrap()
                        .raster_samples()
                        .unwrap(),
                    c,
                    v,
                );
                if let Some(b) = &self.base {
                    add(b, c, v);
                }
                if let Some(t) = self
                    .output
                    .selection
                    .as_ref()
                    .and_then(|s| s.tiles().get(&c))
                {
                    v(TileRef::Gray(t));
                }
            }
            Stage::Done => {}
        }
    }
    fn extra_bytes(&self) -> u64 {
        let mut ids = HashSet::new();
        self.source.visit_tiles(&mut |t| {
            ids.insert(t.id());
        });
        let mut bytes = 0;
        self.visit_tiles(&mut |t| {
            if ids.insert(t.id()) {
                bytes += t.bytes();
            }
        });
        bytes
    }
    fn admit(&self, additional: u64) -> Result<(), String> {
        if self.extra_bytes() + self.scratch_bytes() + additional > self.available {
            Err("Selected transform exceeds shared CPU budget; document unchanged".into())
        } else {
            Ok(())
        }
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        self.admit(TILE_PIXELS as u64 * 12)?;
        // A cold first dependency can be hydrated and retried without consuming
        // a stage. Processing/admission failures still discard the whole job.
        let mut cold = None;
        self.needed(&mut |t| {
            let result = match t {
                TileRef::Rgba(t) => t.pin().map(|_| ()),
                TileRef::Gray(t) => t.pin().map(|_| ()),
            };
            if let Err(e) = result {
                cold = Some(e);
            }
        });
        if let Some(e) = cold {
            return Err(e);
        }
        let stage = std::mem::replace(&mut self.stage, Stage::Done);
        self.stage = match stage {
            Stage::Lift(n) => {
                let c = (
                    n % self.source.width.div_ceil(TILE_SIZE),
                    n / self.source.width.div_ceil(TILE_SIZE),
                );
                let raster = self.source.layer(self.target)?.raster_samples()?;
                let selection = self.source.selection.as_ref().unwrap();
                let rp = pin(raster, c)?;
                let sp = pin(selection, c)?;
                let mut lift = vec![Rgba8::default(); TILE_PIXELS];
                let mut clear = rp
                    .as_ref()
                    .map_or_else(|| vec![raster.default; TILE_PIXELS], |p| p.pixels.clone());
                let mut coverage = vec![Gray8(0); TILE_PIXELS];
                let mut changed = false;
                for y in 0..TILE_SIZE.min(self.source.height - c.1 * TILE_SIZE) {
                    for x in 0..TILE_SIZE.min(self.source.width - c.0 * TILE_SIZE) {
                        let i = (y * TILE_SIZE + x) as usize;
                        let p = sample(raster, &rp, i);
                        let coverage_value = if self
                            .source
                            .canvas_rect()
                            .contains(c.0 * TILE_SIZE + x, c.1 * TILE_SIZE + y)
                        {
                            sample(selection, &sp, i).0
                        } else {
                            0
                        };
                        coverage[i] = Gray8(coverage_value);
                        if coverage_value == 0 {
                            continue;
                        }
                        let mut l = p;
                        let mut b = p;
                        l.0[3] =
                            ((u32::from(p.0[3]) * u32::from(coverage_value) + 127) / 255) as u8;
                        b.0[3] = ((u32::from(p.0[3]) * u32::from(255 - coverage_value) + 127) / 255)
                            as u8;
                        lift[i] = l;
                        clear[i] = b;
                        changed |= b != p;
                    }
                }
                self.lifted.insert(c, lift);
                self.lifted.compact_tile(c)?;
                if changed {
                    self.cleared.insert(c, clear);
                    self.cleared.compact_tile(c)?;
                }
                self.coverage.insert(c, coverage);
                self.coverage.compact_tile(c)?;
                if n + 1 == self.tile_count() {
                    let mut lifted = (*self.source).clone();
                    lifted.layer_mut(self.target)?.content =
                        LayerContent::Raster(Arc::new(self.lifted.clone()));
                    lifted.selection = Some(Arc::new(self.coverage.clone()));
                    let available = self
                        .available
                        .saturating_sub(self.extra_bytes() + TILE_PIXELS as u64 * 16);
                    Stage::Transform(Box::new(GeometryJob::lifted_selection(
                        Arc::new(lifted),
                        self.target,
                        self.matrix,
                        self.mode,
                        available,
                        self.domain,
                    )?))
                } else {
                    Stage::Lift(n + 1)
                }
            }
            Stage::Transform(mut j) => {
                if j.step()? {
                    let shift = j.rebase_matrix();
                    self.output = j.output;
                    self.lifted = TileSurface::new(self.source.width, self.source.height);
                    self.coverage = TileSurface::new(self.source.width, self.source.height);
                    let available = self
                        .available
                        .saturating_sub(self.extra_bytes() + TILE_PIXELS as u64 * 16);
                    let cleared = Arc::new(std::mem::replace(
                        &mut self.cleared,
                        TileSurface::new(self.source.width, self.source.height),
                    ));
                    Stage::Rebase(Box::new(SurfaceResampleJob::new(
                        cleared,
                        self.output.width,
                        self.output.height,
                        shift,
                        Interpolation::Nearest,
                        false,
                        available,
                    )?))
                } else {
                    Stage::Transform(j)
                }
            }
            Stage::Rebase(mut j) => {
                if j.step()? {
                    self.base = Some(Arc::new(j.finish()?));
                    Stage::Composite(0)
                } else {
                    Stage::Rebase(j)
                }
            }
            Stage::Composite(n) => {
                let c = (
                    n % self.output.width.div_ceil(TILE_SIZE),
                    n / self.output.width.div_ceil(TILE_SIZE),
                );
                let lift = self.output.layer(self.target)?.raster_samples()?;
                let base = self.base.as_ref().ok_or("Missing cleared base")?;
                let lp = pin(lift, c)?;
                let bp = pin(base, c)?;
                let selected = self
                    .output
                    .selection
                    .as_ref()
                    .ok_or("Missing transformed selection")?;
                let sp = pin(selected, c)?;
                let mut out = vec![Rgba8::default(); TILE_PIXELS];
                for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
                    for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                        let i = (y * TILE_SIZE + x) as usize;
                        let l = sample(lift, &lp, i);
                        let b = sample(base, &bp, i);
                        out[i] = if l.0[3] == 0 {
                            if b.0[3] == 0 && sample(selected, &sp, i).0 > 0 {
                                l
                            } else {
                                b
                            }
                        } else if b.0[3] == 0 {
                            l
                        } else {
                            Composite::from_straight(b)
                                .blend(Composite::from_straight(l), BlendMode::Normal, 1.0, false)
                                .straight()
                        };
                    }
                }
                let raster = self.output.layer_mut(self.target)?.raster_samples_mut()?;
                raster.insert(c, out);
                raster.compact_tile(c)?;
                if n + 1 == self.output_count() {
                    self.base = None;
                    Stage::Done
                } else {
                    Stage::Composite(n + 1)
                }
            }
            Stage::Done => Stage::Done,
        };
        Ok(self.done())
    }
    pub fn finish(self) -> Result<DocumentCommand, String> {
        if !self.done() {
            return Err("Selected transform incomplete".into());
        }
        DocumentCommand::between(&self.source, self.output, "Transform selection")
    }
}
