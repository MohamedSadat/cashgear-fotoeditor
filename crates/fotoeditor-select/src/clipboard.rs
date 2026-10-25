use fotoeditor_core::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct ClipboardImage {
    pub samples: Arc<TileSurface<Rgba8>>,
    pub origin: [i32; 2],
}
impl ClipboardImage {
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for t in self.samples.tiles().values() {
            visitor(TileRef::Rgba(t));
        }
    }
    pub fn bytes(&self) -> u64 {
        self.samples.bytes()
    }
}
fn coordinates<P: Pixel>(s: &TileSurface<P>, b: [i32; 4]) -> Vec<TileCoord> {
    let x0 = b[0].max(0).min(s.width as i32) as u32;
    let y0 = b[1].max(0).min(s.height as i32) as u32;
    let x1 = b[2].max(0).min(s.width as i32) as u32;
    let y1 = b[3].max(0).min(s.height as i32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return Vec::new();
    }
    (y0 / TILE_SIZE..y1.div_ceil(TILE_SIZE))
        .flat_map(|y| (x0 / TILE_SIZE..x1.div_ceil(TILE_SIZE)).map(move |x| (x, y)))
        .collect()
}
struct ReadRegion<'a, P: Pixel> {
    surface: &'a TileSurface<P>,
    tiles: HashMap<TileCoord, Arc<Tile<P>>>,
}
impl<'a, P: Pixel> ReadRegion<'a, P> {
    fn new(surface: &'a TileSurface<P>, b: [i32; 4]) -> Result<Self, String> {
        let tiles = coordinates(surface, b)
            .into_iter()
            .filter_map(|c| surface.tiles().get(&c).map(|t| (c, t)))
            .map(|(c, t)| Ok((c, t.pin()?)))
            .collect::<Result<_, String>>()?;
        Ok(Self { surface, tiles })
    }
    fn sample(&self, x: i32, y: i32) -> P {
        if x < 0 || y < 0 || x >= self.surface.width as i32 || y >= self.surface.height as i32 {
            return P::default();
        }
        let x = x as u32;
        let y = y as u32;
        self.tiles
            .get(&(x / TILE_SIZE, y / TILE_SIZE))
            .map_or(self.surface.default, |t| {
                t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize]
            })
    }
}
pub struct CopyJob {
    canvas: Option<CanvasRect>,
    pub source: Arc<TileSurface<Rgba8>>,
    selection: Selection,
    pub output: Option<ClipboardImage>,
    bounds: [u32; 4],
    finding_bounds: bool,
    next: u32,
    available: u64,
}
impl CopyJob {
    pub fn in_canvas(
        source: Arc<TileSurface<Rgba8>>,
        selection: Selection,
        canvas: Option<CanvasRect>,
        available: u64,
    ) -> Result<Self, String> {
        let Some(rect) = canvas else {
            return Self::new(source, selection, available);
        };
        rect.validate(source.width, source.height)?;
        let selection = selection.or_else(|| {
            Some(Arc::new(TileSurface::with_default(
                source.width,
                source.height,
                Gray8(255),
            )))
        });
        let mut job = Self::new(source, selection, available)?;
        job.canvas = Some(rect);
        Ok(job)
    }
    pub fn new(
        source: Arc<TileSurface<Rgba8>>,
        selection: Selection,
        available: u64,
    ) -> Result<Self, String> {
        validate_size(source.width, source.height, 72.0)?;
        if selection.is_none() {
            let count = source.width.div_ceil(TILE_SIZE) * source.height.div_ceil(TILE_SIZE);
            let bounds = [0, 0, source.width, source.height];
            return Ok(Self {
                canvas: None,
                source: source.clone(),
                selection,
                output: Some(ClipboardImage {
                    samples: source,
                    origin: [0, 0],
                }),
                bounds,
                finding_bounds: false,
                next: count,
                available,
            });
        }
        if selection
            .as_ref()
            .is_some_and(|s| (s.width, s.height) != (source.width, source.height))
        {
            return Err("Selection dimensions differ".into());
        }
        if available < 2 * 1024 * 1024 {
            return Err("Not enough memory for copying pixels".into());
        }
        let finding_bounds = selection.is_some();
        let bounds = if finding_bounds {
            [source.width, source.height, 0, 0]
        } else {
            [0, 0, source.width, source.height]
        };
        let mut job = Self {
            canvas: None,
            source,
            selection,
            output: None,
            bounds,
            finding_bounds,
            next: 0,
            available,
        };
        if !finding_bounds {
            job.make_output()?;
        }
        Ok(job)
    }
    fn make_output(&mut self) -> Result<(), String> {
        if self.bounds[0] >= self.bounds[2] || self.bounds[1] >= self.bounds[3] {
            return Err("The selection is empty; nothing to copy".into());
        }
        self.output = Some(ClipboardImage {
            samples: Arc::new(TileSurface::new(
                self.bounds[2] - self.bounds[0],
                self.bounds[3] - self.bounds[1],
            )),
            origin: [self.bounds[0] as i32, self.bounds[1] as i32],
        });
        self.finding_bounds = false;
        self.next = 0;
        Ok(())
    }
    fn coord(&self) -> TileCoord {
        let width = if self.finding_bounds {
            self.source.width
        } else {
            self.output.as_ref().unwrap().samples.width
        };
        (
            self.next % width.div_ceil(TILE_SIZE),
            self.next / width.div_ceil(TILE_SIZE),
        )
    }
    pub fn done(&self) -> bool {
        self.output.as_ref().is_some_and(|o| {
            !self.finding_bounds
                && self.next
                    == o.samples.width.div_ceil(TILE_SIZE) * o.samples.height.div_ceil(TILE_SIZE)
        })
    }
    fn region(&self) -> [i32; 4] {
        let c = self.coord();
        [
            self.bounds[0] as i32 + (c.0 * TILE_SIZE) as i32,
            self.bounds[1] as i32 + (c.1 * TILE_SIZE) as i32,
            self.bounds[0] as i32 + ((c.0 + 1) * TILE_SIZE) as i32,
            self.bounds[1] as i32 + ((c.1 + 1) * TILE_SIZE) as i32,
        ]
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if self.done() {
            return;
        }
        if self.finding_bounds {
            if let Some(t) = self
                .selection
                .as_ref()
                .and_then(|s| s.tiles().get(&self.coord()))
            {
                visitor(TileRef::Gray(t));
            }
        } else {
            let b = self.region();
            for c in coordinates(&self.source, b) {
                if let Some(t) = self.source.tiles().get(&c) {
                    visitor(TileRef::Rgba(t));
                }
            }
            if let Some(s) = &self.selection {
                for c in coordinates(s, b) {
                    if let Some(t) = s.tiles().get(&c) {
                        visitor(TileRef::Gray(t));
                    }
                }
            }
        }
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.output.as_ref().map_or(0, |o| o.bytes()) + 2 * 1024 * 1024
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if self.temporary_bytes() + (TILE_PIXELS * 4) as u64 > self.available {
            return Err("Clipboard copy exceeds the shared CPU budget".into());
        }
        let c = self.coord();
        if self.finding_bounds {
            let s = self.selection.as_ref().unwrap();
            let t = s.tiles().get(&c).map(|t| t.pin()).transpose()?;
            for y in 0..TILE_SIZE.min(s.height - c.1 * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(s.width - c.0 * TILE_SIZE) {
                    if self
                        .canvas
                        .is_some_and(|r| !r.contains(c.0 * TILE_SIZE + x, c.1 * TILE_SIZE + y))
                    {
                        continue;
                    }
                    if t.as_ref()
                        .map_or(s.default.0, |t| t.pixels[(y * TILE_SIZE + x) as usize].0)
                        > 0
                    {
                        self.bounds[0] = self.bounds[0].min(c.0 * TILE_SIZE + x);
                        self.bounds[1] = self.bounds[1].min(c.1 * TILE_SIZE + y);
                        self.bounds[2] = self.bounds[2].max(c.0 * TILE_SIZE + x + 1);
                        self.bounds[3] = self.bounds[3].max(c.1 * TILE_SIZE + y + 1);
                    }
                }
            }
            self.next += 1;
            if self.next == s.width.div_ceil(TILE_SIZE) * s.height.div_ceil(TILE_SIZE) {
                self.make_output()?;
            }
        } else {
            let b = self.region();
            let source = ReadRegion::new(&self.source, b)?;
            let selection = self
                .selection
                .as_ref()
                .map(|s| ReadRegion::new(s, b))
                .transpose()?;
            let o = self.output.as_mut().unwrap();
            let mut pixels = vec![Rgba8::default(); TILE_PIXELS];
            for y in 0..TILE_SIZE.min(o.samples.height - c.1 * TILE_SIZE) {
                for x in 0..TILE_SIZE.min(o.samples.width - c.0 * TILE_SIZE) {
                    let sx = b[0] + x as i32;
                    let sy = b[1] + y as i32;
                    let coverage = selection.as_ref().map_or(255, |s| s.sample(sx, sy).0);
                    if coverage == 0 {
                        continue;
                    }
                    let mut p = source.sample(sx, sy);
                    p.0[3] = ((u16::from(p.0[3]) * u16::from(coverage) + 127) / 255) as u8;
                    pixels[(y * TILE_SIZE + x) as usize] = p;
                }
            }
            Arc::make_mut(&mut o.samples).insert(c, pixels);
            self.next += 1;
        }
        Ok(self.done())
    }
    pub fn finish(self) -> Result<ClipboardImage, String> {
        if !self.done() {
            return Err("Copy is incomplete".into());
        }
        Ok(self.output.unwrap())
    }
}
pub struct PasteJob {
    pub image: ClipboardImage,
    pub output: TileSurface<Rgba8>,
    offset: [i32; 2],
    next: u32,
    available: u64,
    in_place: bool,
}
impl PasteJob {
    pub fn new(
        document: &Document,
        image: ClipboardImage,
        in_place: bool,
        available: u64,
    ) -> Result<Self, String> {
        if available < 2 * 1024 * 1024 {
            return Err("Not enough memory for paste".into());
        }
        let rect = document.canvas_rect();
        let offset = if in_place {
            image.origin
        } else {
            [
                rect.x as i32 + (rect.width as i32 - image.samples.width as i32) / 2,
                rect.y as i32 + (rect.height as i32 - image.samples.height as i32) / 2,
            ]
        };
        Ok(Self {
            image,
            output: TileSurface::new(document.width, document.height),
            offset,
            next: 0,
            available,
            in_place,
        })
    }
    fn coord(&self) -> TileCoord {
        (
            self.next % self.output.width.div_ceil(TILE_SIZE),
            self.next / self.output.width.div_ceil(TILE_SIZE),
        )
    }
    pub fn done(&self) -> bool {
        self.next == self.output.width.div_ceil(TILE_SIZE) * self.output.height.div_ceil(TILE_SIZE)
    }
    fn region(&self) -> [i32; 4] {
        let c = self.coord();
        [
            (c.0 * TILE_SIZE) as i32 - self.offset[0],
            (c.1 * TILE_SIZE) as i32 - self.offset[1],
            ((c.0 + 1) * TILE_SIZE) as i32 - self.offset[0],
            ((c.1 + 1) * TILE_SIZE) as i32 - self.offset[1],
        ]
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if !self.done() {
            for c in coordinates(&self.image.samples, self.region()) {
                if let Some(t) = self.image.samples.tiles().get(&c) {
                    visitor(TileRef::Rgba(t));
                }
            }
        }
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.output.bytes() + 2 * 1024 * 1024
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        if self.temporary_bytes() + (TILE_PIXELS * 4) as u64 > self.available {
            return Err("Paste exceeds the shared CPU budget".into());
        }
        let c = self.coord();
        let b = self.region();
        if coordinates(&self.image.samples, b).is_empty() {
            self.next += 1;
            return Ok(self.done());
        }
        let source = ReadRegion::new(&self.image.samples, b)?;
        let mut pixels = vec![Rgba8::default(); TILE_PIXELS];
        for y in 0..TILE_SIZE.min(self.output.height - c.1 * TILE_SIZE) {
            for x in 0..TILE_SIZE.min(self.output.width - c.0 * TILE_SIZE) {
                pixels[(y * TILE_SIZE + x) as usize] =
                    source.sample(b[0] + x as i32, b[1] + y as i32);
            }
        }
        self.output.insert(c, pixels);
        self.next += 1;
        Ok(self.done())
    }
    pub fn finish(self, document: &Document) -> Result<PasteCommand, String> {
        if !self.done() {
            return Err("Paste is incomplete".into());
        }
        let mut layers = document.layers.clone();
        let layer = Layer::raster("Pasted pixels".into(), self.output);
        let target = layer.id;
        layers.push(layer);
        Ok(PasteCommand {
            layers: LayerCommand::replacing(
                document,
                layers,
                if self.in_place {
                    "Paste in place"
                } else {
                    "Paste"
                },
            )?,
            selection: SelectionCommand::new(document, None, "Deselect")?,
            target,
        })
    }
}
pub struct PasteCommand {
    layers: LayerCommand,
    selection: SelectionCommand,
    pub target: LayerId,
}
impl Command for PasteCommand {
    fn label(&self) -> &str {
        self.layers.label()
    }
    fn apply(&self, d: &mut Document) {
        self.layers.apply(d);
        self.selection.apply(d);
    }
    fn revert(&self, d: &mut Document) {
        self.selection.revert(d);
        self.layers.revert(d);
    }
    fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.layers.visit_tiles(v);
        self.selection.visit_tiles(v);
    }
    fn is_empty(&self) -> bool {
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_hidden_rgb_partial_selection_and_paste_position_restore_exactly() {
        let mut d = Document::blank(520, 4, 72.0).unwrap();
        d.raster_mut().write(255, 1, Rgba8([87, 33, 19, 0]));
        d.raster_mut().write(256, 1, Rgba8([201, 3, 7, 255]));
        let mut s = TileSurface::new(520, 4);
        s.write(255, 1, Gray8(255));
        s.write(256, 1, Gray8(128));
        d.selection = Some(Arc::new(s));
        let mut j = CopyJob::new(
            Arc::new(d.raster().clone()),
            d.selection.clone(),
            32 * 1024 * 1024,
        )
        .unwrap();
        while !j.step().unwrap() {}
        let image = j.finish().unwrap();
        assert_eq!(image.origin, [255, 1]);
        assert_eq!((image.samples.width, image.samples.height), (2, 1));
        assert_eq!(image.samples.sample(0, 0), Rgba8([87, 33, 19, 0]));
        assert_eq!(image.samples.sample(1, 0), Rgba8([201, 3, 7, 128]));
        for in_place in [false, true] {
            let mut j = PasteJob::new(&d, image.clone(), in_place, 32 * 1024 * 1024).unwrap();
            while !j.step().unwrap() {}
            let cmd = j.finish(&d).unwrap();
            let target = cmd.target;
            let mut history = History::default();
            let mut doc = d.clone();
            history.execute(&mut doc, Box::new(cmd), 0).unwrap();
            assert!(doc.selection.is_none());
            assert_eq!(
                doc.layer(target)
                    .unwrap()
                    .raster_samples()
                    .unwrap()
                    .sample(if in_place { 255 } else { 259 }, 1),
                Rgba8([87, 33, 19, 0])
            );
            assert!(history.undo(&mut doc));
            assert_eq!(doc.layers.len(), 1);
            assert_eq!(selection_coverage(&doc.selection, 256, 1), 128);
            assert!(history.redo(&mut doc));
            assert_eq!(doc.layers.len(), 2);
        }
    }
}
