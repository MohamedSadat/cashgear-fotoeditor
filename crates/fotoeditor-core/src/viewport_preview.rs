//! Bounded visible-source resampling for GPU adjustment previews. Each step
//! pins at most one authoritative tile. Settings changes reuse the cache.
use crate::*;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const PREVIEW_SCRATCH: u64 = 6 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AdjustmentPreviewTarget {
    Pixels {
        target: LayerId,
    },
    Layer {
        target: LayerId,
    },
    NewLayer {
        parent: Option<LayerId>,
        index: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_preview_source_refuses_a_step_until_its_single_generation_is_hydrated() {
        let mut source = Document::blank(1, 1, 72.0).unwrap();
        let backing = TileBacking {
            store: 1,
            offset: 0,
            length: (TILE_PIXELS * 4) as u32,
            checksum: 7,
        };
        let tile = TileHandle::paged(backing).unwrap();
        source
            .raster_mut()
            .insert_handle((0, 0), tile.clone())
            .unwrap();
        let view = View {
            document_width: 1.0,
            document_height: 1.0,
            ..View::default()
        };
        let mut job = ViewportPreviewJob::new(&source, view, 1, 1).unwrap();
        let mut ids = Vec::new();
        job.needed(&mut |t| ids.push(t.id())).unwrap();
        assert_eq!(ids, vec![tile.id()]);
        assert!(job.step().is_err());
        assert_eq!(job.output.bytes(), 0);
        TilePager::default()
            .hydrate(
                &tile,
                backing,
                vec![Rgba8([81, 29, 61, 128]); TILE_PIXELS],
                Residency::default(),
            )
            .unwrap();
        while !job.step().unwrap() {}
        assert_eq!(job.output.raster().sample(0, 0), Rgba8([81, 29, 61, 128]));
    }
    #[test]
    fn viewport_cache_pins_one_tile_and_preserves_selected_source_samples() {
        let mut source = Document::blank(513, 3, 72.0).unwrap();
        let mut selection = TileSurface::new(513, 3);
        for y in 0..3 {
            for x in 0..513 {
                source.raster_mut().write(
                    x,
                    y,
                    Rgba8([x as u8, (x * 7) as u8, 17, [0, 1, 128, 255][x as usize % 4]]),
                );
                if x >= 255 {
                    selection.write(x, y, Gray8(128));
                }
            }
        }
        source.selection = Some(Arc::new(selection));
        let view = View {
            width: 513.0,
            height: 3.0,
            document_width: 513.0,
            document_height: 3.0,
            ..View::default()
        };
        let mut job = ViewportPreviewJob::new(&source, view, 513, 3).unwrap();
        while !job.done() {
            let mut count = 0;
            job.needed(&mut |_| count += 1).unwrap();
            assert!(count <= 1);
            job.step().unwrap();
        }
        for y in 0..3 {
            for x in 0..513 {
                assert_eq!(
                    job.output.raster().sample(x, y),
                    source.raster().sample(x, y)
                );
            }
        }
        let setting = Adjustment::Curves {
            points: vec![[0.0, 1.0], [1.0, 0.0]],
        };
        let target = AdjustmentPreviewTarget::Pixels {
            target: source.layers[0].id,
        };
        let kernel = setting.compile().unwrap();
        for x in [254, 255, 256, 257, 512] {
            let p = job
                .plan((x / 256, 0), &target, &setting)
                .unwrap()
                .sample((x % 256) as usize)
                .unwrap()
                .straight();
            let expected = kernel.sample(
                source.raster().sample(x, 0),
                source.selection.as_ref().unwrap().sample(x, 0).0,
            );
            if expected.0[3] > 0 {
                assert_eq!(p, expected);
            }
        }
        assert_eq!(source.raster().sample(256, 0).0[0], 0);
        assert!(ViewportPreviewJob::new(&source, view, 2049, 1).is_err());
    }
    #[test]
    fn resampled_cache_tracks_rotation_flip_pan_and_outside_canvas() {
        let mut source = Document::blank(33, 17, 72.0).unwrap();
        for y in 0..17 {
            for x in 0..33 {
                source
                    .raster_mut()
                    .write(x, y, Rgba8([x as u8, y as u8, 17, 255]));
            }
        }
        let view = View {
            width: 128.0,
            canvas: None,
            height: 128.0,
            document_width: 33.0,
            document_height: 17.0,
            zoom: 2.1,
            rotation: 0.43,
            flipped: true,
            pan_x: 7.0,
            pan_y: -11.0,
        };
        let mut job = ViewportPreviewJob::new(&source, view, 64, 64).unwrap();
        while !job.step().unwrap() {}
        for y in 0..64 {
            for x in 0..64 {
                let (dx, dy) =
                    view.screen_to_document((f64::from(x) + 0.5) * 2.0, (f64::from(y) + 0.5) * 2.0);
                let expected = if dx >= 0.0 && dy >= 0.0 && dx < 33.0 && dy < 17.0 {
                    source.raster().sample(dx.floor() as u32, dy.floor() as u32)
                } else {
                    Rgba8::default()
                };
                assert_eq!(job.output.raster().sample(x, y), expected);
            }
        }
    }
}
impl AdjustmentPreviewTarget {
    pub fn validate(&self, document: &Document) -> Result<(), String> {
        match *self {
            Self::Pixels { target } => {
                check_layer_edit(&document.layers, target, false)?;
                document.layer(target)?.raster_samples()?;
            }
            Self::Layer { target } => {
                check_layer_edit(&document.layers, target, false)?;
                if !matches!(document.layer(target)?.content, LayerContent::Adjustment(_)) {
                    return Err("Choose an adjustment layer".into());
                }
            }
            Self::NewLayer { parent, index } => {
                LayerCommand::prepare(
                    document,
                    LayerAction::AddAdjustment {
                        parent,
                        index,
                        adjustment: Adjustment::BrightnessContrast {
                            brightness: 0.0,
                            contrast: 0.0,
                        },
                    },
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Target {
    Raster(LayerId),
    Mask(LayerId),
    Selection,
}
enum SourceTile {
    Rgba(TileHandle<Rgba8>),
    Gray(TileHandle<Gray8>),
}
enum Samples {
    Rgba(Vec<Rgba8>),
    Gray(Vec<Gray8>),
}

pub struct ViewportPreviewJob {
    pub source: Document,
    pub output: Document,
    pub view: View,
    clamp_raster: Option<LayerId>,
    tasks: Vec<(Target, TileCoord)>,
    next: usize,
    mapping_coord: Option<TileCoord>,
    mapping_clamped: bool,
    mapping: BTreeMap<TileCoord, Vec<(usize, usize)>>,
    coordinates: Vec<TileCoord>,
    group: usize,
    pixels: Option<Samples>,
}
impl ViewportPreviewJob {
    pub fn new(source: &Document, view: View, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 2048 || height > 2048 {
            return Err("Viewport preview is bounded to 2048 pixels per side".into());
        }
        validate_layers(&source.layers, source.width, source.height)?;
        let mut targets = Vec::new();
        let mut output = source.clone();
        output.width = width;
        output.height = height;
        output.canvas = None;
        output.guides.clear();
        fn reset(nodes: &mut [Layer], targets: &mut Vec<Target>, w: u32, h: u32) {
            for node in nodes {
                if let Some(mask) = &mut node.mask {
                    targets.push(Target::Mask(node.id));
                    mask.samples = Arc::new(TileSurface::with_default(w, h, mask.samples.default));
                }
                match &mut node.content {
                    LayerContent::Raster(surface) => {
                        targets.push(Target::Raster(node.id));
                        *surface = Arc::new(TileSurface::new(w, h));
                    }
                    LayerContent::Group(children) => reset(children, targets, w, h),
                    LayerContent::Adjustment(_) => {}
                }
            }
        }
        reset(&mut output.layers, &mut targets, width, height);
        output.selection = source.selection.as_ref().map(|s| {
            targets.push(Target::Selection);
            Arc::new(TileSurface::with_default(width, height, s.default))
        });
        let mut tasks = Vec::new();
        for y in 0..height.div_ceil(TILE_SIZE) {
            for x in 0..width.div_ceil(TILE_SIZE) {
                for target in &targets {
                    tasks.push((*target, (x, y)));
                }
            }
        }
        let mut job = Self {
            source: source.clone(),
            output,
            view,
            clamp_raster: None,
            tasks,
            next: 0,
            mapping_coord: None,
            mapping_clamped: false,
            mapping: BTreeMap::new(),
            coordinates: Vec::new(),
            group: 0,
            pixels: None,
        };
        job.prepare()?;
        Ok(job)
    }
    /// Filter halos sample the target's clamped canvas boundary; other layers
    /// retain transparent outside-canvas samples for composition.
    pub fn clamp_target(&mut self, target: LayerId) -> Result<(), String> {
        self.clamp_raster = Some(target);
        self.mapping_coord = None;
        self.prepare()
    }
    pub fn done(&self) -> bool {
        self.next == self.tasks.len()
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        self.source.visit_tiles(visitor);
        self.output.visit_tiles(visitor);
    }
    fn gray(&self, target: Target) -> Result<&TileSurface<Gray8>, String> {
        match target {
            Target::Mask(id) => self
                .source
                .layer(id)?
                .mask
                .as_ref()
                .map(|m| m.samples.as_ref())
                .ok_or("Preview mask expired".into()),
            Target::Selection => self
                .source
                .selection
                .as_deref()
                .ok_or("Preview selection expired".into()),
            _ => Err("Expected a gray preview source".into()),
        }
    }
    fn current_tile(&self) -> Result<Option<SourceTile>, String> {
        let Some((target, _)) = self.tasks.get(self.next).copied() else {
            return Ok(None);
        };
        let Some(coord) = self.coordinates.get(self.group) else {
            return Ok(None);
        };
        Ok(match target {
            Target::Raster(id) => self
                .source
                .layer(id)?
                .raster_samples()?
                .tiles()
                .get(coord)
                .cloned()
                .map(SourceTile::Rgba),
            _ => self
                .gray(target)?
                .tiles()
                .get(coord)
                .cloned()
                .map(SourceTile::Gray),
        })
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) -> Result<(), String> {
        match self.current_tile()? {
            Some(SourceTile::Rgba(tile)) => visitor(TileRef::Rgba(&tile)),
            Some(SourceTile::Gray(tile)) => visitor(TileRef::Gray(&tile)),
            None => {}
        }
        Ok(())
    }
    fn prepare(&mut self) -> Result<(), String> {
        self.pixels = None;
        let Some((target, coord)) = self.tasks.get(self.next).copied() else {
            return Ok(());
        };
        let clamped = matches!(target, Target::Raster(id) if Some(id) == self.clamp_raster);
        if self.mapping_coord != Some(coord) || self.mapping_clamped != clamped {
            self.mapping.clear();
            for y in 0..TILE_SIZE {
                for x in 0..TILE_SIZE {
                    let sx = coord.0 * TILE_SIZE + x;
                    let sy = coord.1 * TILE_SIZE + y;
                    if sx >= self.output.width || sy >= self.output.height {
                        continue;
                    }
                    let (mut dx, mut dy) = self.view.screen_to_document(
                        (f64::from(sx) + 0.5) * self.view.width / f64::from(self.output.width),
                        (f64::from(sy) + 0.5) * self.view.height / f64::from(self.output.height),
                    );
                    if clamped {
                        dx = dx.clamp(0.0, f64::from(self.source.width) - 1.0);
                        dy = dy.clamp(0.0, f64::from(self.source.height) - 1.0);
                    }
                    if dx < 0.0
                        || dy < 0.0
                        || dx >= f64::from(self.source.width)
                        || dy >= f64::from(self.source.height)
                    {
                        continue;
                    }
                    let dx = dx.floor() as u32;
                    let dy = dy.floor() as u32;
                    self.mapping
                        .entry((dx / TILE_SIZE, dy / TILE_SIZE))
                        .or_default()
                        .push((
                            (y * TILE_SIZE + x) as usize,
                            ((dy % TILE_SIZE) * TILE_SIZE + dx % TILE_SIZE) as usize,
                        ));
                }
            }
            self.coordinates = self.mapping.keys().copied().collect();
            self.mapping_coord = Some(coord);
            self.mapping_clamped = clamped;
        }
        self.group = 0;
        self.pixels = Some(match target {
            Target::Raster(_) => Samples::Rgba(vec![Rgba8::default(); TILE_PIXELS]),
            _ => Samples::Gray(vec![self.gray(target)?.default; TILE_PIXELS]),
        });
        Ok(())
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if let Some(coord) = self.coordinates.get(self.group) {
            let points = &self.mapping[coord];
            match (
                self.current_tile()?,
                self.pixels.as_mut().ok_or("Missing preview scratch")?,
            ) {
                (Some(SourceTile::Rgba(tile)), Samples::Rgba(pixels)) => {
                    let tile = tile.pin()?;
                    for &(out, input) in points {
                        pixels[out] = tile.pixels[input];
                    }
                }
                (Some(SourceTile::Gray(tile)), Samples::Gray(pixels)) => {
                    let tile = tile.pin()?;
                    for &(out, input) in points {
                        pixels[out] = tile.pixels[input];
                    }
                }
                (None, _) => {}
                _ => return Err("Mismatched preview source".into()),
            }
            self.group += 1;
        }
        if self.group == self.coordinates.len() {
            let (target, coord) = self.tasks[self.next];
            match (target, self.pixels.take().ok_or("Missing preview samples")?) {
                (Target::Raster(id), Samples::Rgba(pixels)) => {
                    if pixels.iter().any(|p| *p != Rgba8::default()) {
                        self.output
                            .layer_mut(id)?
                            .raster_samples_mut()?
                            .insert(coord, pixels);
                    }
                }
                (target, Samples::Gray(pixels)) => {
                    let surface = match target {
                        Target::Mask(id) => {
                            &mut self
                                .output
                                .layer_mut(id)?
                                .mask
                                .as_mut()
                                .ok_or("Missing preview mask")?
                                .samples
                        }
                        Target::Selection => self
                            .output
                            .selection
                            .as_mut()
                            .ok_or("Missing preview selection")?,
                        _ => return Err("Mismatched preview output".into()),
                    };
                    if pixels.iter().any(|p| *p != surface.default) {
                        Arc::make_mut(surface).insert(coord, pixels);
                    }
                }
                _ => return Err("Mismatched preview output".into()),
            }
            self.next += 1;
            self.prepare()?;
        }
        Ok(self.done())
    }
    pub fn plan(
        &self,
        coord: TileCoord,
        target: &AdjustmentPreviewTarget,
        settings: &Adjustment,
    ) -> Result<CompositePlan, String> {
        if !self.done() {
            return Err("Viewport preview sources are still preparing".into());
        }
        settings.validate()?;
        let mut output = self.output.clone();
        match *target {
            AdjustmentPreviewTarget::Layer { target } => {
                output.layer_mut(target)?.content = LayerContent::Adjustment(settings.clone());
            }
            AdjustmentPreviewTarget::NewLayer { parent, index } => {
                LayerCommand::prepare(
                    &output,
                    LayerAction::AddAdjustment {
                        parent,
                        index,
                        adjustment: settings.clone(),
                    },
                )?
                .apply(&mut output);
            }
            _ => {}
        }
        let mut plan = CompositePlan::tile(&output, coord, None)?;
        if let AdjustmentPreviewTarget::Pixels { target } = *target {
            let coverage = output
                .selection
                .as_ref()
                .map_or_else(CompositeMask::default, |s| CompositeMask {
                    tile: s.tiles().get(&coord).cloned(),
                    default: s.default.0,
                });
            let kernel = Arc::new(settings.compile()?);
            for step in &mut plan.steps {
                if let CompositeStep::Raster { id, adjustment, .. } = step {
                    if *id == target {
                        *adjustment = Some((kernel.clone(), coverage.clone()));
                    }
                }
            }
        }
        Ok(plan)
    }
}
