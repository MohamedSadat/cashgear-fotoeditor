use super::*;
use fotoeditor_core::*;
use fotoeditor_filters::*;

#[derive(Default)]
pub(super) struct AdjustmentRuntime {
    pub apply: Option<AdjustmentJob>,
    pub histogram: Option<HistogramJob>,
}
impl AdjustmentRuntime {
    pub fn scratch_bytes(&self) -> u64 {
        u64::from(self.apply.is_some()) * ADJUSTMENT_SCRATCH
            + u64::from(self.histogram.is_some()) * HISTOGRAM_SCRATCH
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.apply {
            j.visit_tiles(visitor);
        }
        if let Some(j) = &self.histogram {
            j.visit_tiles(visitor);
        }
    }
}
fn js(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}
#[wasm_bindgen]
impl Engine {
    pub fn adjustment_begin(&mut self, encoded: &str) -> Result<(), JsValue> {
        if encoded.len() > 4096 {
            return Err(js("Adjustment settings exceed 4 KiB"));
        }
        if self.adjusted.apply.is_some() || self.stroke.is_some() {
            return Err(js("Finish the current operation"));
        }
        if self.layered.mask_target || self.selected.quick_mask {
            return Err(js("Select raster layer pixels before adjusting color"));
        }
        self.residency()
            .check_cpu(ADJUSTMENT_SCRATCH + 65536)
            .map_err(js)?;
        let adjustment: Adjustment = serde_json::from_str(encoded).map_err(js)?;
        let doc = self.document.as_ref().ok_or("No document")?;
        let target = self
            .layered
            .active
            .or_else(|| doc.layers.first().map(|l| l.id))
            .ok_or("Choose a raster layer")?;
        let r = self.residency();
        let available = CPU_BUDGET.saturating_sub(r.document + r.history + r.temporary);
        self.adjusted.apply =
            Some(AdjustmentJob::new(doc, target, adjustment, available).map_err(js)?);
        self.refresh_pages();
        Ok(())
    }
    pub fn adjustment_ready(&self) -> bool {
        self.adjusted.apply.as_ref().is_some_and(|j| j.done())
    }
    pub fn adjustment_step(&mut self) -> Result<bool, JsValue> {
        self.residency()
            .check_cpu((TILE_PIXELS * 4) as u64)
            .map_err(js)?;
        let done = self
            .adjusted
            .apply
            .as_mut()
            .ok_or("No adjustment job")?
            .step()
            .map_err(js)?;
        self.layered.invalidate_memory();
        Ok(done)
    }
    pub fn adjustment_commit(&mut self) -> Result<(), JsValue> {
        let command = self
            .adjusted
            .apply
            .take()
            .ok_or("No adjustment job")?
            .finish()
            .map_err(js)?;
        self.layered.invalidate_memory();
        let temporary = self.residency().temporary;
        let changed = self
            .history
            .execute(
                self.document.as_mut().ok_or("No document")?,
                Box::new(command),
                temporary,
            )
            .map_err(js)?;
        if changed {
            self.revision = self.revision.wrapping_add(1);
            self.content_revision = self.content_revision.wrapping_add(1);
            self.layered.invalidate_display();
            self.renderer.clear_tiles();
        }
        self.refresh_pages();
        Ok(())
    }
    pub fn adjustment_cancel(&mut self) {
        self.adjusted.apply = None;
        self.refresh_pages();
    }
    pub fn histogram_begin(&mut self, target: u32) -> Result<(), JsValue> {
        if self.adjusted.histogram.is_some() {
            return Err(js("Histogram is already updating"));
        }
        self.residency().check_cpu(HISTOGRAM_SCRATCH).map_err(js)?;
        self.adjusted.histogram = Some(
            HistogramJob::new(self.document.as_ref().ok_or("No document")?, target).map_err(js)?,
        );
        self.refresh_pages();
        Ok(())
    }
    pub fn histogram_step(&mut self) -> Result<bool, JsValue> {
        self.adjusted
            .histogram
            .as_mut()
            .ok_or("Histogram cancelled")?
            .step()
            .map_err(js)
    }
    pub fn histogram_finish(&mut self) -> Result<String, JsValue> {
        let j = self
            .adjusted
            .histogram
            .take()
            .ok_or("Histogram cancelled")?;
        let result = serde_json::to_string(&j.output).map_err(js);
        self.refresh_pages();
        result
    }
    pub fn histogram_cancel(&mut self) {
        self.adjusted.histogram = None;
        self.refresh_pages();
    }
}
