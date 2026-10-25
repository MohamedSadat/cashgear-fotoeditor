use super::*;
use fotoeditor_core::{CanvasRect, GeometryMetadata};
#[wasm_bindgen]
impl Engine {
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
