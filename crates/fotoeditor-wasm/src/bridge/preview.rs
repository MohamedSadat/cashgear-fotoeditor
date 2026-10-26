use super::*;
use fotoeditor_core::*;
use fotoeditor_filters::Filter;

pub(super) struct PreviewRuntime {
    pub job: ViewportPreviewJob,
    pub target: AdjustmentPreviewTarget,
    pub revision: u32,
    pub settings: Option<Adjustment>,
    pub next: u32,
    pub view: View,
    pub size: (u32, u32),
    pub filter_mode: bool,
    pub filter: Option<Filter>,
}
impl PreviewRuntime {
    fn filter_target(&self) -> Result<u32, String> {
        if let AdjustmentPreviewTarget::Pixels { target } = self.target {
            Ok(target)
        } else {
            Err("Expected filter pixel target".into())
        }
    }
    pub fn filter_needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) -> Result<(), String> {
        if !self.filter_mode || self.filter.is_none() {
            return Ok(());
        }
        let w = self.job.output.width.div_ceil(256);
        if self.next >= w * self.job.output.height.div_ceil(256) {
            return Ok(());
        }
        let c = (self.next % w, self.next / w);
        let surface = self
            .job
            .output
            .layer(self.filter_target()?)?
            .raster_samples()?;
        let x0 = (c.0 * 256).saturating_sub(64) / 256;
        let y0 = (c.1 * 256).saturating_sub(64) / 256;
        let x1 = ((c.0 + 1) * 256 + 63).min(surface.width - 1) / 256;
        let y1 = ((c.1 + 1) * 256 + 63).min(surface.height - 1) / 256;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(t) = surface.tiles().get(&(x, y)) {
                    visitor(TileRef::Rgba(t));
                }
            }
        }
        if let Some(t) = self
            .job
            .output
            .selection
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
        {
            visitor(TileRef::Gray(t));
        }
        Ok(())
    }
    fn filter_input(&self, c: TileCoord) -> Result<fotoeditor_gpu::FilterTileInput, String> {
        let target = self.filter_target()?;
        let surface = self.job.output.layer(target)?.raster_samples()?;
        let mut tiles = std::collections::HashMap::new();
        let mut error = None;
        self.filter_needed(&mut |t| {
            if let TileRef::Rgba(tile) = t {
                match tile.pin() {
                    Ok(p) => {
                        tiles.insert(tile.id(), p);
                    }
                    Err(e) => error = Some(e),
                }
            }
        })?;
        if let Some(e) = error {
            return Err(e);
        }
        let mut source = vec![0u8; 384 * 384 * 4];
        for y in 0..384u32 {
            let sy = (i64::from(c.1) * 256 + i64::from(y) - 64)
                .clamp(0, i64::from(surface.height) - 1) as u32;
            let mut x = 0u32;
            while x < 384 {
                let raw = i64::from(c.0) * 256 + i64::from(x) - 64;
                let sx = raw.clamp(0, i64::from(surface.width) - 1) as u32;
                let repeated = raw < 0 || raw >= i64::from(surface.width);
                let count = if raw < 0 {
                    (-raw) as u32
                } else if repeated {
                    384 - x
                } else {
                    (256 - sx % 256).min(surface.width - sx)
                }
                .min(384 - x);
                let pixels = surface
                    .tiles()
                    .get(&(sx / 256, sy / 256))
                    .and_then(|t| tiles.get(&t.id()));
                let start = ((sy % 256) * 256 + sx % 256) as usize;
                for i in 0..count as usize {
                    let p = pixels.map_or(surface.default, |t| {
                        t.pixels[start + if repeated { 0 } else { i }]
                    });
                    let out = (y as usize * 384 + x as usize + i) * 4;
                    source[out..out + 4].copy_from_slice(&p.0);
                }
                x += count;
            }
        }
        let selection = self.job.output.selection.as_ref();
        let mut coverage = vec![selection.map_or(255, |s| s.default.0); TILE_PIXELS];
        if let Some(t) = selection.and_then(|s| s.tiles().get(&c)) {
            for (out, p) in coverage.iter_mut().zip(&t.pin()?.pixels) {
                *out = p.0;
            }
        }
        let mut filter = self.filter.clone().ok_or("Missing filter settings")?;
        let scale = self.job.view.zoom * f64::from(self.job.output.width) / self.job.view.width;
        match &mut filter {
            Filter::Gaussian { radius }
            | Filter::BoxBlur { radius }
            | Filter::Median { radius }
            | Filter::MotionBlur { radius, .. }
            | Filter::HighPass { radius }
            | Filter::Emboss { radius, .. }
            | Filter::UnsharpMask { radius, .. } => {
                *radius = (f64::from(*radius) * scale).round().clamp(0.0, 64.0) as u32
            }
            _ => {}
        }
        let (kind, amount, threshold) = match filter {
            Filter::UnsharpMask {
                amount, threshold, ..
            } => (2u32, amount, f32::from(threshold) / 255.0),
            Filter::Sharpen { amount } => (2, amount, 0.0),
            Filter::Median { .. } => (3, 0.0, 0.0),
            Filter::MotionBlur { .. } => (4, 0.0, 0.0),
            Filter::HighPass { .. } => (5, 0.0, 0.0),
            Filter::AddNoise { amount, .. } => (6, amount, 0.0),
            Filter::Emboss { amount, .. } => (7, amount, 0.0),
            Filter::Pixelate { size } => (8, size as f32, 0.0),
            _ => (0, 0.0, 0.0),
        };
        let mut params = Vec::with_capacity(2128);
        let weights = if matches!(filter, Filter::Sharpen { .. }) && scale < 0.5 {
            vec![1.0]
        } else {
            filter.weights()
        };
        params.extend(kind.to_le_bytes());
        params.extend(((weights.len() / 2) as u32).to_le_bytes());
        params.extend(amount.to_le_bytes());
        params.extend(threshold.to_le_bytes());
        params
            .extend(u32::from(self.job.source.layer(target)?.properties.locks.alpha).to_le_bytes());
        let flags = match filter {
            Filter::AddNoise {
                seed,
                monochrome,
                distribution,
                ..
            } => [
                seed,
                u32::from(monochrome),
                u32::from(matches!(
                    distribution,
                    fotoeditor_filters::NoiseDistribution::Gaussian
                )),
            ],
            _ => [0; 3],
        };
        for value in flags {
            params.extend(value.to_le_bytes());
        }
        let point = |x: f64, y: f64| {
            self.job.view.screen_to_document(
                (f64::from(c.0 * 256) + 0.5 + x) * self.job.view.width
                    / f64::from(self.job.output.width),
                (f64::from(c.1 * 256) + 0.5 + y) * self.job.view.height
                    / f64::from(self.job.output.height),
            )
        };
        let origin = point(0.0, 0.0);
        let dx = point(1.0, 0.0);
        let dy = point(0.0, 1.0);
        for v in [
            origin.0,
            origin.1,
            dx.0 - origin.0,
            dx.1 - origin.1,
            dy.0 - origin.0,
            dy.1 - origin.1,
            f64::from(self.job.source.width),
            f64::from(self.job.source.height),
        ] {
            params.extend((v as f32).to_le_bytes());
        }
        let direction = match filter {
            Filter::MotionBlur { angle, .. } | Filter::Emboss { angle, .. } => {
                // Rotate/flip the document direction into the sampled viewport.
                let a = f64::from(angle).to_radians();
                let x = dx.0 - origin.0;
                let y = dx.1 - origin.1;
                let u = dy.0 - origin.0;
                let v = dy.1 - origin.1;
                let det = x * v - y * u;
                (
                    (a.cos() * v - a.sin() * u) / det / scale,
                    (a.sin() * x - a.cos() * y) / det / scale,
                )
            }
            _ => (0.0, 0.0),
        };
        for i in 0..129 {
            let k = i as i32 - filter.radius() as i32;
            let tap = [
                weights.get(i).copied().unwrap_or(0.0),
                (f64::from(k) * direction.0).round() as f32,
                (f64::from(k) * direction.1).round() as f32,
                0.0,
            ];
            for value in tap {
                params.extend(value.to_le_bytes());
            }
        }
        Ok(fotoeditor_gpu::FilterTileInput {
            source,
            coverage,
            params,
            target,
        })
    }
    pub fn plan(&self) -> Result<Option<(TileCoord, CompositePlan)>, String> {
        let width = self.job.output.width.div_ceil(TILE_SIZE);
        let height = self.job.output.height.div_ceil(TILE_SIZE);
        if self.next >= width * height {
            return Ok(None);
        }
        let coord = (self.next % width, self.next / width);
        if self.filter_mode {
            return if self.filter.is_some() {
                Ok(Some((
                    coord,
                    CompositePlan::tile(&self.job.output, coord, None)?,
                )))
            } else {
                Ok(None)
            };
        }
        let Some(settings) = &self.settings else {
            return Ok(None);
        };
        Ok(Some((coord, self.job.plan(coord, &self.target, settings)?)))
    }
}
fn js(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
#[wasm_bindgen]
impl Engine {
    pub fn filter_golden_begin(
        &mut self,
        encoded: &str,
        tile: u32,
    ) -> Result<js_sys::Promise, JsValue> {
        if encoded.len() > 4096 || tile >= 9 {
            return Err(js("Invalid filter golden request"));
        }
        self.residency().check_cpu(32 * 1024 * 1024).map_err(js)?;
        let filter: Filter = serde_json::from_str(encoded).map_err(js)?;
        filter.validate().map_err(js)?;
        let mut doc = Document::blank(513, 513, 72.0).map_err(js)?;
        let target = doc.layers[0].id;
        doc.layers[0].properties.locks.alpha = tile % 2 == 1;
        let mut selection = TileSurface::new(513, 513);
        for y in 0..513 {
            for x in 0..513 {
                doc.raster_mut().write(
                    x,
                    y,
                    Rgba8([
                        (x * 31 + y * 7) as u8,
                        (x * 13 + y * 19) as u8,
                        (x * 3 + y * 43) as u8,
                        if x < 512 && y < 512 {
                            255
                        } else {
                            [0, 1, 128, 255][(x + y) as usize % 4]
                        },
                    ]),
                );
                selection.write(x, y, Gray8([0, 128, 255][(x / 7 + y / 11) as usize % 3]));
            }
        }
        doc.raster_mut().compact_dirty();
        doc.selection = if tile.is_multiple_of(2) {
            None
        } else {
            Some(std::sync::Arc::new(selection))
        };
        let view = View {
            width: 513.0,
            height: 513.0,
            document_width: 513.0,
            document_height: 513.0,
            ..View::default()
        };
        let mut job = ViewportPreviewJob::new(&doc, view, 513, 513).map_err(js)?;
        job.clamp_target(target).map_err(js)?;
        while !job.step().map_err(js)? {}
        let runtime = PreviewRuntime {
            job,
            target: AdjustmentPreviewTarget::Pixels { target },
            revision: 0,
            settings: None,
            next: tile,
            view,
            size: (513, 513),
            filter_mode: true,
            filter: Some(filter.clone()),
        };
        let coord = (tile % 3, tile / 3);
        let input = runtime.filter_input(coord).map_err(js)?;
        let mut cpu = fotoeditor_filters::FilterJob::new(&doc, target, filter, 32 * 1024 * 1024)
            .map_err(js)?;
        while !cpu.step().map_err(js)? {}
        self.layered.golden = Some(
            CompositePlan::tile(&cpu.candidate, coord, None)
                .map_err(js)?
                .reference(self.residency())
                .map_err(js)?
                .into_iter()
                .map(Composite::straight)
                .collect(),
        );
        self.renderer
            .filter_golden(
                &CompositePlan::tile(&runtime.job.output, coord, None).map_err(js)?,
                &input,
            )
            .map_err(js)
    }
    pub fn preview_valid(&self) -> bool {
        self.adjusted.preview.as_ref().is_some_and(|p| {
            p.revision == self.revision
                && p.view == self.view
                && p.size
                    == (
                        self.renderer.width().min(2048),
                        self.renderer.height().min(2048),
                    )
        })
    }
    pub fn preview_begin(&mut self, encoded: &str) -> Result<(), JsValue> {
        if encoded.len() > 4096 {
            return Err(js("Preview target exceeds 4 KiB"));
        }
        if self.stroke.is_some() || self.layered.mask_target || self.selected.quick_mask {
            return Err(js("Select layer color before previewing adjustments"));
        }
        self.preview_cancel();
        let target: AdjustmentPreviewTarget = serde_json::from_str(encoded).map_err(js)?;
        let doc = self.document.as_ref().ok_or("No document")?;
        target.validate(doc).map_err(js)?;
        self.residency().check_cpu(PREVIEW_SCRATCH).map_err(js)?;
        let job = ViewportPreviewJob::new(
            doc,
            self.view,
            self.renderer.width().min(2048),
            self.renderer.height().min(2048),
        )
        .map_err(js)?;
        self.adjusted.preview = Some(PreviewRuntime {
            job,
            target,
            revision: self.revision,
            settings: None,
            next: 0,
            view: self.view,
            size: (
                self.renderer.width().min(2048),
                self.renderer.height().min(2048),
            ),
            filter_mode: false,
            filter: None,
        });
        self.refresh_pages();
        Ok(())
    }
    pub fn preview_ready(&self) -> bool {
        self.preview_valid() && self.adjusted.preview.as_ref().is_some_and(|p| p.job.done())
    }
    pub fn preview_step(&mut self) -> Result<bool, JsValue> {
        if !self.preview_valid() {
            return Err(js("Preview source changed"));
        }
        self.residency()
            .check_cpu((TILE_PIXELS * 4) as u64)
            .map_err(js)?;
        let done = self
            .adjusted
            .preview
            .as_mut()
            .ok_or("No viewport preview")?
            .job
            .step()
            .map_err(js)?;
        self.layered.invalidate_memory();
        if done {
            self.refresh_pages();
        }
        Ok(done)
    }
    pub fn preview_update(&mut self, encoded: &str) -> Result<(), JsValue> {
        if encoded.len() > 4096 {
            return Err(js("Adjustment settings exceed 4 KiB"));
        }
        if !self.preview_ready() {
            return Err(js("Viewport preview source is not ready"));
        }
        let settings: Adjustment = serde_json::from_str(encoded).map_err(js)?;
        settings.validate().map_err(js)?;
        let p = self
            .adjusted
            .preview
            .as_mut()
            .ok_or("No viewport preview")?;
        if p.settings.is_none() {
            self.renderer
                .preview_begin(p.job.output.width, p.job.output.height)
                .map_err(js)?;
        }
        p.settings = Some(settings);
        p.next = 0;
        self.renderer.preview_hide();
        Ok(())
    }
    pub fn filter_preview_begin(&mut self, encoded: &str) -> Result<(), JsValue> {
        self.preview_begin(encoded)?;
        let p = self.adjusted.preview.as_mut().unwrap();
        let AdjustmentPreviewTarget::Pixels { target } = p.target else {
            return Err(js("Filter preview requires raster pixels"));
        };
        let divisor = (self.view.zoom * self.dpr).max(1.0);
        let width = (f64::from(self.renderer.width().min(1920)) / divisor)
            .round()
            .max(1.0) as u32;
        let height = (f64::from(self.renderer.height().min(1920)) / divisor)
            .round()
            .max(1.0) as u32;
        let mut view = self.view;
        view.width *= f64::from(width + 128) / f64::from(width);
        view.height *= f64::from(height + 128) / f64::from(height);
        let mut job = ViewportPreviewJob::new(
            self.document.as_ref().unwrap(),
            view,
            width + 128,
            height + 128,
        )
        .map_err(js)?;
        job.clamp_target(target).map_err(js)?;
        p.job = job;
        p.filter_mode = true;
        self.refresh_pages();
        Ok(())
    }
    pub fn filter_preview_update(&mut self, encoded: &str) -> Result<(), JsValue> {
        if encoded.len() > 4096 {
            return Err(js("Filter settings exceed 4 KiB"));
        }
        if !self.preview_ready() {
            return Err(js("Filter preview source is not ready"));
        }
        let filter: Filter = serde_json::from_str(encoded).map_err(js)?;
        filter.validate().map_err(js)?;
        let p = self.adjusted.preview.as_mut().unwrap();
        if !p.filter_mode {
            return Err(js("Expected a filter preview"));
        }
        if p.filter.is_none() {
            self.renderer
                .filter_preview_begin(p.job.output.width, p.job.output.height)
                .map_err(js)?;
            self.renderer.preview_crop(64);
        }
        p.filter = Some(filter);
        p.next = 0;
        self.renderer.preview_hide();
        Ok(())
    }
    pub fn preview_render_step(&mut self) -> Result<bool, JsValue> {
        if !self.preview_valid() {
            return Err(js("Preview source changed"));
        }
        let p = self
            .adjusted
            .preview
            .as_ref()
            .ok_or("No viewport preview")?;
        let Some((coord, plan)) = p.plan().map_err(js)? else {
            return Ok(true);
        };
        if p.filter_mode {
            let input = p.filter_input(coord).map_err(js)?;
            self.renderer
                .filter_preview_tile(coord, &plan, &input)
                .map_err(js)?;
        } else {
            self.renderer.preview_tile(coord, &plan).map_err(js)?;
        }
        let p = self.adjusted.preview.as_mut().unwrap();
        p.next += 1;
        let done = p.next
            == p.job.output.width.div_ceil(TILE_SIZE) * p.job.output.height.div_ceil(TILE_SIZE);
        if done {
            self.renderer.preview_present();
        }
        self.layered.invalidate_memory();
        Ok(done)
    }
    pub fn preview_cancel(&mut self) {
        let geometry = self.geometric.session.take().is_some();
        if self.adjusted.preview.take().is_some() || geometry {
            self.layered.invalidate_memory();
            self.refresh_pages();
        }
        self.renderer.clear_preview();
    }
    pub fn preview_settled(&self, diagnostic_hold: bool) -> js_sys::Promise {
        self.renderer.preview_settled(diagnostic_hold)
    }
}
