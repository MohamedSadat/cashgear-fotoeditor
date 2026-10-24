use super::*;
use fotoeditor_core::*;
use fotoeditor_filters::*;
pub(super) struct LastFilter {
    before: Document,
    filtered: Document,
    full: Document,
    layer: LayerId,
    revision: u32,
}
#[derive(Default)]
pub(super) struct FilterRuntime {
    apply: Option<FilterJob>,
    fade: Option<FadeJob>,
    pub last: Option<LastFilter>,
    allocation_reserve: u64,
    cpu_peak: u64,
}
impl FilterRuntime {
    pub fn has_tiles(&self) -> bool {
        self.apply.is_some() || self.fade.is_some() || self.last.is_some()
    }
    pub fn scratch_bytes(&self) -> u64 {
        u64::from(self.apply.is_some() || self.fade.is_some()) * FILTER_SCRATCH
            + self.allocation_reserve
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.apply {
            j.visit_tiles(visitor);
        }
        if let Some(j) = &self.fade {
            j.visit_tiles(visitor);
        }
        if let Some(j) = &self.last {
            j.before.visit_tiles(visitor);
            j.filtered.visit_tiles(visitor);
            j.full.visit_tiles(visitor);
        }
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.apply {
            j.needed(visitor);
        }
        if let Some(j) = &self.fade {
            j.needed(visitor);
        }
    }
}
fn js(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}
#[wasm_bindgen]
impl Engine {
    pub fn filter_can_fade(&self) -> bool {
        self.adjusted.filter.last.as_ref().is_some_and(|f| {
            f.revision == self.revision
                && Some(f.layer)
                    == self.layered.active.or_else(|| {
                        self.document
                            .as_ref()
                            .and_then(|d| d.layers.first().map(|l| l.id))
                    })
                && !self.layered.mask_target
                && !self.selected.quick_mask
        })
    }
    pub fn filter_begin(&mut self, encoded: &str) -> Result<(), JsValue> {
        self.preview_cancel();
        if encoded.len() > 4096 {
            return Err(js("Filter settings exceed 4 KiB"));
        }
        if self.stroke.is_some()
            || self.adjusted.filter.apply.is_some()
            || self.layered.mask_target
            || self.selected.quick_mask
        {
            return Err(js(
                "Select raster layer pixels and finish the current operation",
            ));
        }
        let filter: Filter = serde_json::from_str(encoded).map_err(js)?;
        let d = self.document.as_ref().ok_or("No document")?;
        let target = self
            .layered
            .active
            .or_else(|| d.layers.first().map(|l| l.id))
            .ok_or("Choose raster layer pixels")?;
        let r = self.residency();
        let available = CPU_BUDGET.saturating_sub(r.document + r.history + r.temporary);
        let job = FilterJob::new(d, target, filter, available).map_err(js)?;
        self.adjusted.filter.apply = Some(job);
        self.adjusted.filter.cpu_peak = 0;
        self.refresh_pages();
        Ok(())
    }
    pub fn fade_begin(&mut self, opacity: f64, blend: &str) -> Result<(), JsValue> {
        self.preview_cancel();
        if !self.filter_can_fade() {
            return Err(js(
                "Fade is available immediately after a filter, before another document edit",
            ));
        }
        if blend.len() > 64 {
            return Err(js("Invalid Fade blend mode"));
        }
        let mode: BlendMode = serde_json::from_str(blend).map_err(js)?;
        let d = self.document.as_ref().ok_or("No document")?;
        let last = self.adjusted.filter.last.as_ref().unwrap();
        let r = self.residency();
        let available = CPU_BUDGET.saturating_sub(r.document + r.history + r.temporary);
        self.adjusted.filter.fade = Some(
            FadeJob::new(
                d,
                &last.before,
                &last.filtered,
                &last.full,
                last.layer,
                opacity,
                mode,
                available,
            )
            .map_err(js)?,
        );
        self.adjusted.filter.cpu_peak = 0;
        self.refresh_pages();
        Ok(())
    }
    pub fn filter_ready(&self) -> bool {
        self.adjusted
            .filter
            .apply
            .as_ref()
            .is_some_and(|j| j.done())
            || self.adjusted.filter.fade.as_ref().is_some_and(|j| j.done())
    }
    pub fn filter_cpu_peak(&self) -> f64 {
        self.adjusted.filter.cpu_peak as f64
    }
    pub fn filter_step(&mut self) -> Result<bool, JsValue> {
        let memory = self.paging_residency();
        memory.check_cpu((TILE_PIXELS * 8) as u64).map_err(js)?;
        let available =
            CPU_BUDGET.saturating_sub(memory.document + memory.history + memory.temporary);
        let current = memory.document + memory.history + memory.temporary;
        self.adjusted.filter.cpu_peak = self.adjusted.filter.cpu_peak.max(current);
        if let Some(j) = &self.adjusted.filter.apply {
            let reservation = j.allocation_reservation(available);
            self.adjusted.filter.allocation_reserve = reservation;
            self.adjusted.filter.cpu_peak =
                self.adjusted.filter.cpu_peak.max(current + reservation);
            reserve_filter_capacity(reservation);
            self.adjusted.filter.allocation_reserve = 0;
        }
        let f = &mut self.adjusted.filter;
        let (result, allocated) = if let Some(j) = &mut f.apply {
            let before = j.allocated_bytes();
            let result = j.step();
            (result, j.allocated_bytes() - before)
        } else if let Some(j) = &mut f.fade {
            let before = j.allocated_bytes();
            let result = j.step();
            (result, j.allocated_bytes() - before)
        } else {
            (Err("No filter operation".into()), 0)
        };
        if result.is_err() {
            self.layered.invalidate_memory();
        } else {
            self.layered.account_temporary_allocations(allocated);
            self.adjusted.filter.cpu_peak = self.adjusted.filter.cpu_peak.max(current + allocated);
        }
        result.map_err(js)
    }
    pub fn filter_commit(&mut self) -> Result<(), JsValue> {
        let (command, last) = if let Some(j) = self.adjusted.filter.apply.take() {
            (
                j.finish().map_err(js)?,
                Some(LastFilter {
                    before: j.before,
                    filtered: j.candidate,
                    full: j.full,
                    layer: j.layer,
                    revision: 0,
                }),
            )
        } else if let Some(j) = self.adjusted.filter.fade.take() {
            (j.finish().map_err(js)?, None)
        } else {
            return Err(js("No filter operation"));
        };
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
            if let Some(last) = last {
                self.adjusted.filter.last = Some(last);
            }
            if let Some(last) = &mut self.adjusted.filter.last {
                last.revision = self.revision;
            }
            self.layered.invalidate_display();
            self.renderer.clear_tiles();
        }
        self.refresh_pages();
        Ok(())
    }
    pub fn filter_cancel(&mut self) {
        self.adjusted.filter.apply = None;
        self.adjusted.filter.fade = None;
        self.refresh_pages();
    }
}
