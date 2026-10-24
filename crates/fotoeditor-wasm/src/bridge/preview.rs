use super::*;
use fotoeditor_core::*;

pub(super) struct PreviewRuntime {
    pub job: ViewportPreviewJob,
    pub target: AdjustmentPreviewTarget,
    pub revision: u32,
    pub settings: Option<Adjustment>,
    pub next: u32,
}
impl PreviewRuntime {
    pub fn plan(&self) -> Result<Option<(TileCoord, CompositePlan)>, String> {
        let width = self.job.output.width.div_ceil(TILE_SIZE);
        let height = self.job.output.height.div_ceil(TILE_SIZE);
        if self.next >= width * height {
            return Ok(None);
        }
        let Some(settings) = &self.settings else {
            return Ok(None);
        };
        let coord = (self.next % width, self.next / width);
        Ok(Some((coord, self.job.plan(coord, &self.target, settings)?)))
    }
}
fn js(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
#[wasm_bindgen]
impl Engine {
    pub fn preview_valid(&self) -> bool {
        self.adjusted.preview.as_ref().is_some_and(|p| {
            p.revision == self.revision
                && p.job.view == self.view
                && p.job.output.width == self.renderer.width().min(2048)
                && p.job.output.height == self.renderer.height().min(2048)
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
        self.renderer.preview_tile(coord, &plan).map_err(js)?;
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
        if self.adjusted.preview.take().is_some() {
            self.layered.invalidate_memory();
            self.refresh_pages();
        }
        self.renderer.clear_preview();
    }
    pub fn preview_settled(&self, diagnostic_hold: bool) -> js_sys::Promise {
        self.renderer.preview_settled(diagnostic_hold)
    }
}
