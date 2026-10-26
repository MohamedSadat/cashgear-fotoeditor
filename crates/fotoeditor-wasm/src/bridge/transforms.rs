use super::*;
use fotoeditor_core::*;
use std::sync::Arc;
pub(super) struct TransformRuntime {
    pub session: TransformSession,
    pub revision: u32,
    view: View,
    size: (u32, u32),
    cache: (u32, u32),
    output: (u32, u32),
    source_job: Option<GeometryJob>,
    source_cache: Option<Document>,
    background_job: Option<ViewportPreviewJob>,
    background: Option<Document>,
    upload: u32,
    next: u32,
    gpu_started: bool,
    uploaded: bool,
    bounds_job: Option<TransformBoundsJob>,
}
fn js(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}
fn tile_count(w: u32, h: u32) -> u32 {
    w.div_ceil(256) * h.div_ceil(256)
}
impl TransformRuntime {
    pub fn valid_view(&self, view: View, size: (u32, u32)) -> bool {
        self.view == view && self.size == size
    }
    fn new(
        session: TransformSession,
        revision: u32,
        view: View,
        size: (u32, u32),
        available: u64,
    ) -> Result<Self, String> {
        let source = &session.source;
        let factor = (1536.0 / source.width.max(source.height) as f64).min(1.0);
        let cache = (
            (source.width as f64 * factor).round().max(1.0) as u32,
            (source.height as f64 * factor).round().max(1.0) as u32,
        );
        let mut active = (**source).clone();
        active.layers = vec![source.layer(session.target)?.clone()];
        active.canvas = None;
        active.guides.clear();
        let planes =
            4 + u64::from(active.layers[0].mask.is_some()) + u64::from(active.selection.is_some());
        let source_bytes = u64::from(tile_count(cache.0, cache.1)) * TILE_PIXELS as u64 * planes;
        let reserved = source_bytes + 32 * 1024 * 1024;
        let mut background_planes = u64::from(source.selection.is_some());
        fn count(nodes: &[Layer], p: &mut u64) {
            for l in nodes {
                *p += u64::from(l.mask.is_some());
                match &l.content {
                    LayerContent::Raster(_) => *p += 4,
                    LayerContent::Group(c) => count(c, p),
                    _ => {}
                }
            }
        }
        count(&source.layers, &mut background_planes);
        let factor = (1536.0 / size.0.max(size.1).max(1) as f64).min(1.0);
        let mut output = (
            (size.0 as f64 * factor).round().max(1.0) as u32,
            (size.1 as f64 * factor).round().max(1.0) as u32,
        );
        loop {
            let bytes =
                u64::from(tile_count(output.0, output.1)) * TILE_PIXELS as u64 * background_planes;
            if reserved + bytes <= available {
                break;
            }
            if output == (1, 1) {
                return Err("Not enough shared CPU budget for transform preview".into());
            }
            output = (
                (output.0 as f64 * 0.8).floor().max(1.0) as u32,
                (output.1 as f64 * 0.8).floor().max(1.0) as u32,
            );
        }
        let background_bytes =
            u64::from(tile_count(output.0, output.1)) * TILE_PIXELS as u64 * background_planes;
        let source_job = GeometryJob::new(
            Arc::new(active),
            GeometryAction::ImageSize {
                width: cache.0,
                height: cache.1,
                dpi: source.dpi,
                interpolation: Interpolation::Nearest,
            },
            available.saturating_sub(background_bytes + 16 * 1024 * 1024),
        )?;
        let background_job = ViewportPreviewJob::new(source, view, output.0, output.1)?;
        let bounds_job = if session.bounds_ready() {
            None
        } else {
            Some(TransformBoundsJob::new(&session)?)
        };
        Ok(Self {
            session,
            revision,
            view,
            size,
            cache,
            output,
            source_job: Some(source_job),
            source_cache: None,
            background_job: Some(background_job),
            background: None,
            upload: 0,
            next: 0,
            gpu_started: false,
            uploaded: false,
            bounds_job,
        })
    }
    pub fn scratch_bytes(&self) -> u64 {
        if self.source_job.is_some() || self.background_job.is_some() {
            32 * 1024 * 1024
        } else {
            1024 * 1024
        }
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.session.visit_tiles(v);
        if let Some(j) = &self.source_job {
            j.visit_tiles(v);
        }
        if let Some(j) = &self.background_job {
            j.visit_tiles(v);
        }
        for d in [&self.source_cache, &self.background].into_iter().flatten() {
            d.visit_tiles(v);
        }
    }
    pub fn needed_source(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.source_job {
            j.needed(v);
            return;
        }
        if let Some(d) = &self.source_cache {
            let c = (
                self.upload % self.cache.0.div_ceil(256),
                self.upload / self.cache.0.div_ceil(256),
            );
            let l = d.layer(self.session.target).unwrap();
            if let Some(t) = l.raster_samples().unwrap().tiles().get(&c) {
                v(TileRef::Rgba(t));
            }
            if let Some(t) = l.mask.as_ref().and_then(|m| m.samples.tiles().get(&c)) {
                v(TileRef::Gray(t));
            }
            if let Some(t) = d.selection.as_ref().and_then(|m| m.tiles().get(&c)) {
                v(TileRef::Gray(t));
            }
        }
    }
    pub fn needed_bounds(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.bounds_job {
            j.needed(v);
        }
    }
    pub fn needed_background(&self, v: &mut dyn FnMut(TileRef<'_>)) -> Result<(), String> {
        if let Some(j) = &self.background_job {
            j.needed(v)?;
        }
        Ok(())
    }
    pub fn needed_render(&self, v: &mut dyn FnMut(TileRef<'_>)) -> Result<(), String> {
        if let Some((_, p)) = self.plan()? {
            p.visit_tiles(v);
        }
        Ok(())
    }
    fn pack(&self) -> Result<(Vec<u8>, Vec<u8>), String> {
        let d = self
            .source_cache
            .as_ref()
            .ok_or("No transform source cache")?;
        let c = (
            self.upload % self.cache.0.div_ceil(256),
            self.upload / self.cache.0.div_ceil(256),
        );
        let l = d.layer(self.session.target)?;
        let s = l.raster_samples()?;
        let rgba = s.tiles().get(&c).map(TileHandle::pin).transpose()?;
        let mask = l.mask.as_ref().filter(|m| m.enabled);
        let mt = mask
            .and_then(|m| m.samples.tiles().get(&c))
            .map(TileHandle::pin)
            .transpose()?;
        let st = d
            .selection
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
            .map(TileHandle::pin)
            .transpose()?;
        let mut bytes = Vec::with_capacity(TILE_PIXELS * 4);
        let mut auxiliary = Vec::with_capacity(TILE_PIXELS * 4);
        for i in 0..TILE_PIXELS {
            bytes.extend(rgba.as_ref().map_or(s.default, |t| t.pixels[i]).0);
            auxiliary.extend([
                st.as_ref()
                    .map_or(d.selection.as_ref().map_or(255, |s| s.default.0), |t| {
                        t.pixels[i].0
                    }),
                mt.as_ref()
                    .map_or(mask.map_or(255, |m| m.samples.default.0), |t| t.pixels[i].0),
                0,
                255,
            ]);
        }
        Ok((bytes, auxiliary))
    }
    fn plan(&self) -> Result<Option<(TileCoord, CompositePlan)>, String> {
        if self.next == tile_count(self.output.0, self.output.1) {
            return Ok(None);
        }
        let d = self
            .background
            .as_ref()
            .ok_or("Transform background is preparing")?;
        let c = (
            self.next % self.output.0.div_ceil(256),
            self.next / self.output.0.div_ceil(256),
        );
        let mut p = CompositePlan::tile(d, c, None)?;
        for step in &mut p.steps {
            if let CompositeStep::Raster {
                id, source, mask, ..
            } = step
            {
                if *id == self.session.target {
                    *source = None;
                    if !self.session.selected {
                        *mask = CompositeMask::default();
                    }
                }
            }
        }
        Ok(Some((c, p)))
    }
    fn uniforms(&self, c: TileCoord) -> Result<Vec<u8>, String> {
        let m = self.session.matrix.inverse()?.values;
        let doc = &self.session.source;
        let crop = doc.canvas_rect();
        let point = |x: f64, y: f64| {
            self.view.screen_to_document(
                (c.0 as f64 * 256.0 + 0.5 + x) * self.view.width / self.output.0 as f64,
                (c.1 as f64 * 256.0 + 0.5 + y) * self.view.height / self.output.1 as f64,
            )
        };
        let o = point(0.0, 0.0);
        let x = point(1.0, 0.0);
        let y = point(0.0, 1.0);
        let layer = doc.layer(self.session.target)?;
        let mask = layer.mask.as_ref().filter(|m| m.enabled);
        let mode = match self.session.interpolation {
            Interpolation::Nearest => 0.0,
            Interpolation::Bilinear => 1.0,
            Interpolation::Bicubic => 2.0,
            Interpolation::Lanczos3 => 3.0,
        };
        let mut out = Vec::with_capacity(144);
        for v in [
            m[0],
            m[1],
            m[2],
            0.0,
            m[3],
            m[4],
            m[5],
            0.0,
            m[6],
            m[7],
            m[8],
            0.0,
            o.0,
            o.1,
            x.0 - o.0,
            x.1 - o.1,
            y.0 - o.0,
            y.1 - o.1,
            doc.width as f64,
            doc.height as f64,
            self.cache.0 as f64,
            self.cache.1 as f64,
            mode,
            f64::from(self.session.selected),
            crop.x as f64,
            crop.y as f64,
            crop.width as f64,
            crop.height as f64,
            f64::from(mask.is_some()),
            mask.map_or(1.0, |m| m.samples.default.0 as f64 / 255.0),
            0.0,
            0.0,
            0.0,
            0.0,
            self.cache.0 as f64 / doc.width as f64,
            self.cache.1 as f64 / doc.height as f64,
        ] {
            out.extend((v as f32).to_le_bytes());
        }
        Ok(out)
    }
    fn into_session(self) -> TransformSession {
        self.session
    }
}
impl Engine {
    pub(super) fn transform_json(&self) -> serde_json::Value {
        self.geometric.session.as_ref().filter(|p|p.revision==self.revision).map_or(serde_json::Value::Null,|p|serde_json::json!({"target":p.session.target,"selected":p.session.selected,"bounds":p.session.bounds,"quad":p.session.quad().ok(),"matrix":p.session.matrix,"interpolation":p.session.interpolation,"sourceRevision":p.revision}))
    }
}
#[wasm_bindgen]
impl Engine {
    pub fn hit_test_begin(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.residency().check_cpu(8 * 1024 * 1024).map_err(js)?;
        self.geometric.hit = Some((
            self.revision,
            HitTestJob::new(
                Arc::new(self.document.as_ref().ok_or("No document")?.clone()),
                [x, y],
            )
            .map_err(js)?,
        ));
        self.refresh_pages();
        Ok(())
    }
    pub fn hit_test_step(&mut self) -> Result<bool, JsValue> {
        self.geometric
            .hit
            .as_mut()
            .ok_or("No layer hit test")?
            .1
            .step()
            .map_err(js)
    }
    pub fn hit_test_finish(&mut self) -> Result<u32, JsValue> {
        let (revision, j) = self.geometric.hit.take().ok_or("No layer hit test")?;
        if revision != self.revision {
            return Err(js("Layer hit-test source changed"));
        }
        let id = j.finish().map_err(js)?.unwrap_or(0);
        self.refresh_pages();
        Ok(id)
    }
    pub fn hit_test_cancel(&mut self) {
        self.geometric.hit = None;
        self.refresh_pages();
    }
    pub fn transform_begin(&mut self, target: u32, selected: bool) -> Result<(), JsValue> {
        if self.stroke.is_some() {
            return Err(js("Finish the stroke first"));
        }
        let session = TransformSession::new(
            Arc::new(self.document.as_ref().ok_or("No document")?.clone()),
            target,
            selected,
        )
        .map_err(js)?;
        self.preview_cancel();
        let m = self.residency();
        let available = CPU_BUDGET.saturating_sub(m.document + m.history + m.temporary);
        let p = TransformRuntime::new(
            session,
            self.revision,
            self.view,
            (self.renderer.width(), self.renderer.height()),
            available,
        )
        .map_err(js)?;
        self.layered.active = Some(target);
        self.layered.mask_target = false;
        self.layered.mask_view = false;
        self.selected.quick_mask = false;
        self.renderer.clear_selection();
        self.geometric.session = Some(p);
        self.refresh_pages();
        Ok(())
    }
    pub fn transform_update(&mut self, encoded: &str) -> Result<(), JsValue> {
        self.residency().check_cpu(1024 * 1024).map_err(js)?;
        if encoded.len() > 4096 {
            return Err(js("Transform settings exceed 4 KiB"));
        }
        let update: TransformUpdate = serde_json::from_str(encoded).map_err(js)?;
        let p = self
            .geometric
            .session
            .as_mut()
            .ok_or("No transform session")?;
        if p.revision != self.revision {
            return Err(js("Transform source changed"));
        }
        p.session.update(update).map_err(js)?;
        let size = (self.renderer.width(), self.renderer.height());
        if p.view != self.view || p.size != size {
            let p = self.geometric.session.take().unwrap().into_session();
            self.renderer.clear_preview();
            let m = self.residency();
            let available = CPU_BUDGET.saturating_sub(m.document + m.history + m.temporary);
            self.geometric.session = Some(
                TransformRuntime::new(p, self.revision, self.view, size, available).map_err(js)?,
            );
        }
        self.geometric.session.as_mut().unwrap().next = 0;
        self.renderer.preview_hide();
        self.refresh_pages();
        Ok(())
    }
    pub fn transform_preview_width(&self) -> u32 {
        self.geometric.session.as_ref().map_or(0, |p| p.output.0)
    }
    pub fn transform_preview_valid(&self) -> bool {
        self.geometric.session.as_ref().is_some_and(|p| {
            p.revision == self.revision
                && p.valid_view(self.view, (self.renderer.width(), self.renderer.height()))
        })
    }
    pub fn transform_bounds_ready(&self) -> bool {
        self.geometric
            .session
            .as_ref()
            .is_some_and(|p| p.bounds_job.is_none())
    }
    pub fn transform_bounds_step(&mut self) -> Result<bool, JsValue> {
        let p = self
            .geometric
            .session
            .as_mut()
            .ok_or("No transform session")?;
        if p.revision != self.revision {
            return Err(js("Transform source changed"));
        }
        if let Some(j) = &mut p.bounds_job {
            if !j.step().map_err(js)? {
                return Ok(false);
            }
            p.bounds_job
                .take()
                .unwrap()
                .finish(&mut p.session)
                .map_err(js)?;
        }
        self.refresh_pages();
        Ok(true)
    }
    pub fn transform_preview_height(&self) -> u32 {
        self.geometric.session.as_ref().map_or(0, |p| p.output.1)
    }
    pub fn transform_source_ready(&self) -> bool {
        self.geometric.session.as_ref().is_some_and(|p| p.uploaded)
    }
    pub fn transform_background_ready(&self) -> bool {
        self.geometric
            .session
            .as_ref()
            .is_some_and(|p| p.background.is_some())
    }
    pub fn transform_source_step(&mut self) -> Result<bool, JsValue> {
        self.paging_residency()
            .check_cpu(TILE_PIXELS as u64 * 12)
            .map_err(js)?;
        let p = self
            .geometric
            .session
            .as_mut()
            .ok_or("No transform session")?;
        if let Some(j) = &mut p.source_job {
            if !j.step().map_err(js)? {
                self.refresh_pages();
                return Ok(false);
            }
            let j = p.source_job.take().unwrap();
            p.source_cache = Some(j.output);
            self.refresh_pages();
            return Ok(false);
        }
        if !p.gpu_started {
            self.renderer
                .transform_preview_begin(p.output.0, p.output.1, p.cache.0, p.cache.1)
                .map_err(js)?;
            p.gpu_started = true;
        }
        if !p.uploaded {
            let (source, auxiliary) = p.pack().map_err(js)?;
            let c = (
                p.upload % p.cache.0.div_ceil(256),
                p.upload / p.cache.0.div_ceil(256),
            );
            self.renderer
                .transform_source_upload(c, &source, &auxiliary)
                .map_err(js)?;
            p.upload += 1;
            if p.upload == tile_count(p.cache.0, p.cache.1) {
                p.uploaded = true;
                p.source_cache = None;
            }
        }
        let ready = p.uploaded;
        self.refresh_pages();
        Ok(ready)
    }
    pub fn transform_background_step(&mut self) -> Result<bool, JsValue> {
        self.paging_residency()
            .check_cpu(TILE_PIXELS as u64 * 8)
            .map_err(js)?;
        let p = self
            .geometric
            .session
            .as_mut()
            .ok_or("No transform session")?;
        if let Some(j) = &mut p.background_job {
            if !j.step().map_err(js)? {
                self.refresh_pages();
                return Ok(false);
            }
            p.background = Some(p.background_job.take().unwrap().output);
        }
        self.refresh_pages();
        Ok(true)
    }
    pub fn transform_render_step(&mut self) -> Result<bool, JsValue> {
        let p = self
            .geometric
            .session
            .as_ref()
            .ok_or("No transform session")?;
        if p.revision != self.revision
            || p.view != self.view
            || p.size != (self.renderer.width(), self.renderer.height())
        {
            return Err(js("Transform preview view changed"));
        }
        let Some((c, plan)) = p.plan().map_err(js)? else {
            return Ok(true);
        };
        let params = p.uniforms(c).map_err(js)?;
        self.renderer
            .transform_preview_tile(c, &plan, p.session.target, &params)
            .map_err(js)?;
        let p = self.geometric.session.as_mut().unwrap();
        p.next += 1;
        let done = p.next == tile_count(p.output.0, p.output.1);
        if done {
            self.renderer.preview_present();
        }
        self.layered.invalidate_memory();
        Ok(done)
    }
    pub fn transform_apply_begin(&mut self) -> Result<(), JsValue> {
        let p = self
            .geometric
            .session
            .take()
            .ok_or("No transform session")?;
        if p.revision != self.revision {
            return Err(js("Transform source changed"));
        }
        let session = p.into_session();
        self.preview_cancel();
        let m = self.residency();
        let available = CPU_BUDGET.saturating_sub(m.document + m.history + m.temporary);
        self.geometric.apply = Some(session.job(available).map_err(js)?);
        self.geometric.cpu_peak = 0;
        self.refresh_pages();
        Ok(())
    }
    pub fn transform_cancel(&mut self) {
        self.preview_cancel();
    }
}
