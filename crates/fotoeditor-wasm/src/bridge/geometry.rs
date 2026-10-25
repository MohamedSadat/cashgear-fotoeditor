use super::*;
use fotoeditor_core::{
    CanvasRect, GeometryAction, GeometryJob, GeometryMetadata, TileRef, CPU_BUDGET, TILE_PIXELS,
};
#[derive(Default)]
pub(super) struct GeometryRuntime {
    apply: Option<GeometryJob>,
    cpu_peak: u64,
}
impl GeometryRuntime {
    pub fn has_tiles(&self) -> bool {
        self.apply.is_some()
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.apply.as_ref().map_or(0, GeometryJob::scratch_bytes)
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.apply {
            j.visit_tiles(v);
        }
    }
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(j) = &self.apply {
            j.needed(v);
        }
    }
}
fn js(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}
#[wasm_bindgen]
impl Engine {
    pub fn geometry_begin(&mut self, encoded: &str) -> Result<(), JsValue> {
        if encoded.len() > 4096 {
            return Err(js("Geometry settings exceed 4 KiB"));
        }
        if self.stroke.is_some() || self.geometric.apply.is_some() {
            return Err(js("Finish the current operation"));
        }
        self.preview_cancel();
        let action: GeometryAction = serde_json::from_str(encoded).map_err(js)?;
        let memory = self.residency();
        let available =
            CPU_BUDGET.saturating_sub(memory.document + memory.history + memory.temporary);
        self.geometric.apply = Some(
            GeometryJob::new(
                std::sync::Arc::new(self.document.as_ref().ok_or("No document")?.clone()),
                action,
                available,
            )
            .map_err(js)?,
        );
        self.geometric.cpu_peak = 0;
        self.refresh_pages();
        Ok(())
    }
    pub fn geometry_ready(&self) -> bool {
        self.geometric.apply.as_ref().is_some_and(GeometryJob::done)
    }
    pub fn geometry_step(&mut self) -> Result<bool, JsValue> {
        let m = self.paging_residency();
        m.check_cpu((TILE_PIXELS * 8) as u64).map_err(js)?;
        self.geometric.cpu_peak = self
            .geometric
            .cpu_peak
            .max(m.document + m.history + m.temporary + (TILE_PIXELS * 8) as u64);
        let result = self
            .geometric
            .apply
            .as_mut()
            .ok_or("No geometry operation")?
            .step();
        self.refresh_pages();
        result.map_err(js)
    }
    pub fn geometry_cpu_peak(&self) -> f64 {
        self.geometric.cpu_peak as f64
    }
    pub fn geometry_commit(&mut self) -> Result<(), JsValue> {
        let command = self
            .geometric
            .apply
            .take()
            .ok_or("No geometry operation")?
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
            self.layered.invalidate_geometry();
            self.renderer.clear_tiles();
            self.renderer.clear_selection();
            self.sync_view_geometry();
        }
        self.refresh_pages();
        Ok(())
    }
    pub fn geometry_cancel(&mut self) {
        self.geometric.apply = None;
        self.refresh_pages();
    }
    pub fn geometry_metadata(&mut self, encoded: &str) -> Result<(), JsValue> {
        if encoded.len() > 32 * 1024 {
            return Err(JsValue::from_str("Geometry settings exceed 32 KiB"));
        }
        if self.stroke.is_some() {
            return Err(JsValue::from_str("Finish the stroke first"));
        }
        self.preview_cancel();
        let action: GeometryMetadata =
            serde_json::from_str(encoded).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let doc = self.document.as_ref().ok_or("No document")?;
        let command = match action {
            GeometryMetadata::Crop { rect } => doc.crop_command(rect.unwrap_or(CanvasRect {
                x: 0,
                y: 0,
                width: doc.width,
                height: doc.height,
            })),
            GeometryMetadata::Guides { guides } => doc.guide_command(guides),
        }
        .map_err(|e| JsValue::from_str(&e))?;
        let temporary = self.residency().temporary;
        let changed = self
            .history
            .execute(
                self.document.as_mut().unwrap(),
                Box::new(command),
                temporary,
            )
            .map_err(|e| JsValue::from_str(&e))?;
        self.revision = self.revision.wrapping_add(u32::from(changed));
        self.content_revision = self.content_revision.wrapping_add(u32::from(changed));
        self.sync_view_geometry();
        self.refresh_pages();
        Ok(())
    }
}
