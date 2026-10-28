use super::*;
use fotoeditor_core::*;
use fotoeditor_select::*;
use std::sync::Arc;
fn js(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}

pub(super) enum Job {
    Shape(ShapeSelectionJob),
    Wand(WandJob),
    Mask(MaskJob),
    Fill(FillJob),
    ContentAware(ContentAwareJob),
    Copy(CopyJob),
    Paste(PasteJob),
}
#[derive(Default, Clone, Copy)]
enum FillTarget {
    #[default]
    Pixels,
    Mask,
    Quick,
}
#[derive(Default)]
pub(super) struct SelectionRuntime {
    pub quick_mask: bool,
    pub serial: u32,
    job: Option<Job>,
    source: Option<Arc<TileSurface<Rgba8>>>,
    seed: [u32; 2],
    pub clipboard: Option<ClipboardImage>,
    bucket: Option<[u8; 4]>,
    pub last_wand_pixels: u64,
    cut: bool,
    ready: bool,
    fill_target: FillTarget,
    action_bytes: u64,
}
impl SelectionRuntime {
    pub fn cancel(&mut self) {
        self.job = None;
        self.source = None;
        self.bucket = None;
        self.cut = false;
        self.ready = false;
        self.fill_target = FillTarget::Pixels;
        self.action_bytes = 0;
    }
    pub fn has_tiles(&self) -> bool {
        self.job.is_some() || self.clipboard.is_some() || self.source.is_some()
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.action_bytes
            + match &self.job {
                Some(Job::Shape(j)) => j.temporary_bytes().saturating_sub(j.output.bytes()),
                Some(Job::Wand(j)) => j.temporary_bytes().saturating_sub(j.output.bytes()),
                Some(Job::Mask(j)) => {
                    u64::from(j.input.width.max(j.input.height) + 1024) * 32 + 4 * 65536
                }
                Some(Job::Fill(_)) => 2 * (TILE_PIXELS * 4) as u64,
                Some(Job::ContentAware(j)) => j.scratch_bytes(),
                Some(Job::Copy(_)) | Some(Job::Paste(_)) => 2 * 1024 * 1024,
                None => 0,
            }
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(s) = &self.source {
            for t in s.tiles().values() {
                v(TileRef::Rgba(t));
            }
        }
        if let Some(c) = &self.clipboard {
            c.visit_tiles(v);
        }
        match &self.job {
            Some(Job::Shape(j)) => {
                for t in j.output.tiles().values() {
                    v(TileRef::Gray(t));
                }
            }
            Some(Job::Wand(j)) => {
                for t in j.output.tiles().values() {
                    v(TileRef::Gray(t));
                }
                j.visit_sources(v);
            }
            Some(Job::Mask(j)) => j.visit_candidates(v),
            Some(Job::Fill(j)) => j.visit_candidates(v),
            Some(Job::ContentAware(j)) => j.visit_candidates(v),
            Some(Job::Copy(j)) => {
                for t in j.source.tiles().values() {
                    v(TileRef::Rgba(t));
                }
                if let Some(c) = &j.output {
                    c.visit_tiles(v);
                }
            }
            Some(Job::Paste(j)) => {
                j.image.visit_tiles(v);
                for t in j.output.tiles().values() {
                    v(TileRef::Rgba(t));
                }
            }
            None => {}
        }
    }
    pub fn seed_needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(t) = self.source.as_ref().and_then(|s| {
            s.tiles()
                .get(&(self.seed[0] / TILE_SIZE, self.seed[1] / TILE_SIZE))
        }) {
            v(TileRef::Rgba(t));
        }
    }
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        match &self.job {
            Some(Job::Shape(j)) => j.needed(v),
            Some(Job::Wand(j)) => j.needed(v),
            Some(Job::Mask(j)) => j.needed(v),
            Some(Job::Fill(j)) => j.needed(v),
            Some(Job::ContentAware(j)) => j.needed(v),
            Some(Job::Copy(j)) => j.needed(v),
            Some(Job::Paste(j)) => j.needed(v),
            None => {}
        }
    }
}
#[wasm_bindgen]
impl Engine {
    fn decode_selection_action(&mut self, action: &str) -> Result<serde_json::Value, JsValue> {
        if action.len() > 256 * 1024 {
            return Err(js("Selection input exceeds its 256 KiB limit"));
        }
        let reserve = action.len() as u64 * 32 + 4096;
        if reserve > self.selected.action_bytes {
            self.residency()
                .check_cpu(reserve - self.selected.action_bytes)
                .map_err(js)?;
            self.selected.action_bytes = reserve;
        }
        serde_json::from_str(action).map_err(js)
    }
    pub fn selection_active(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|d| d.selection.is_some())
    }
    pub fn quick_mask_enabled(&self) -> bool {
        self.selected.quick_mask
    }
    pub fn linear_memory_bytes(&self) -> f64 {
        (std::arch::wasm32::memory_size(0) as u64 * 65536) as f64
    }
    pub fn selection_overlay_prepare(&mut self) -> Result<(), JsValue> {
        if self.geometric.session.is_some() {
            self.renderer.clear_selection();
            return Ok(());
        }
        let Some(doc) = &self.document else {
            self.renderer.clear_selection();
            return Ok(());
        };
        if !self.selected.quick_mask && doc.selection.is_none() {
            self.renderer.clear_selection();
            return Ok(());
        }
        let scale =
            (2048.0 / f64::from(self.renderer.width().max(self.renderer.height()).max(1))).min(1.0);
        let required = (f64::from(self.renderer.width()) * scale).ceil().max(1.0) as u64
            * (f64::from(self.renderer.height()) * scale).ceil().max(1.0) as u64
            * 4;
        self.residency().check_cpu(required).map_err(js)?;
        let source = if let Some(stroke) = self.stroke.as_ref().filter(|_| self.selected.quick_mask)
        {
            SelectionDisplaySource::Quick(stroke.preview.raster())
        } else if let Some(s) = &doc.selection {
            SelectionDisplaySource::Gray(s)
        } else {
            SelectionDisplaySource::All
        };
        self.renderer
            .prepare_selection(
                source,
                &self.view,
                (
                    self.revision,
                    self.selected.serial,
                    self.selected.quick_mask,
                ),
            )
            .map_err(js)?;
        self.layered.invalidate_memory();
        Ok(())
    }
    pub fn selection_overlay_clear(&mut self) {
        self.renderer.clear_selection();
    }
}
impl Engine {
    pub(super) fn quick_document(&self) -> Result<Document, JsValue> {
        let doc = self.document.as_ref().ok_or("No document")?;
        let default = doc.selection.as_ref().map_or(255, |s| s.default.0);
        let bytes = doc.selection.as_ref().map_or(0, |s| s.bytes() * 4);
        self.residency().check_cpu(bytes).map_err(js)?;
        let mut samples = TileSurface::with_default(
            doc.width,
            doc.height,
            Rgba8([default, default, default, 255]),
        );
        if let Some(s) = &doc.selection {
            for (c, t) in s.tiles() {
                samples.insert(
                    *c,
                    t.pin()
                        .map_err(js)?
                        .pixels
                        .iter()
                        .map(|p| Rgba8([p.0, p.0, p.0, 255]))
                        .collect(),
                );
            }
        }
        Ok(Document {
            canvas: doc.canvas,
            guides: Vec::new(),
            width: doc.width,
            height: doc.height,
            dpi: doc.dpi,
            layers: vec![Layer::raster("Quick Mask".into(), samples)],
            selection: None,
        })
    }
    pub(super) fn finish_quick_stroke(&mut self, preview: &Document) -> Result<(), JsValue> {
        self.finish_quick_mask(preview, "Paint Quick Mask")
    }
    fn finish_quick_mask(
        &mut self,
        preview: &Document,
        label: &'static str,
    ) -> Result<(), JsValue> {
        self.residency()
            .check_cpu(preview.raster().bytes() / 4)
            .map_err(js)?;
        let mut samples = TileSurface::with_default(
            preview.width,
            preview.height,
            Gray8(preview.raster().default.0[0]),
        );
        for (c, t) in preview.raster().tiles() {
            samples.insert(
                *c,
                t.pin()
                    .map_err(js)?
                    .pixels
                    .iter()
                    .map(|p| Gray8(p.0[0]))
                    .collect(),
            );
        }
        let existing = self.document.as_ref().and_then(|d| d.selection.clone());
        let unchanged = existing.as_ref().is_some_and(|s| {
            s.default == samples.default
                && s.tiles().len() == samples.tiles().len()
                && samples.tiles().iter().all(|(c, t)| {
                    s.tiles()
                        .get(c)
                        .is_some_and(|old| match (t.pin(), old.pin()) {
                            (Ok(a), Ok(b)) => a.pixels == b.pixels,
                            _ => false,
                        })
                })
        });
        self.selection_command(
            if unchanged {
                existing
            } else {
                Some(Arc::new(samples))
            },
            label,
        )
    }
    fn fill_candidate(
        &mut self,
        region: Selection,
        mut color: [u8; 4],
        clear: bool,
    ) -> Result<FillJob, JsValue> {
        let available = self
            .selection_available()
            .saturating_sub(region.as_ref().map_or(0, |s| s.bytes()));
        let doc = self.document.as_ref().ok_or("No document")?;
        let target = self
            .layered
            .active
            .or_else(|| doc.layers.first().map(|l| l.id))
            .ok_or("Choose a layer")?;
        if self.selected.quick_mask || self.layered.mask_target {
            let base = if self.selected.quick_mask {
                doc.selection.as_ref().map_or(0, |s| s.bytes() * 4)
            } else {
                doc.layer(target)
                    .map_err(js)?
                    .mask
                    .as_ref()
                    .ok_or("Layer has no mask")?
                    .samples
                    .bytes()
                    * 4
            };
            if base > available {
                return Err(js("Mask fill exceeds the shared CPU budget"));
            }
            let source = self.quick_or_layer_mask_document(target).map_err(js)?;
            self.selected.fill_target = if self.selected.quick_mask {
                FillTarget::Quick
            } else {
                FillTarget::Mask
            };
            let gray = if clear {
                255
            } else {
                ((u32::from(color[0]) * 30
                    + u32::from(color[1]) * 59
                    + u32::from(color[2]) * 11
                    + 50)
                    / 100) as u8
            };
            color = [gray, gray, gray, if clear { 255 } else { color[3] }];
            FillJob::new(
                &source,
                source.layers[0].id,
                region,
                color,
                false,
                available.saturating_sub(source.raster().bytes()),
            )
            .map_err(js)
        } else {
            self.selected.fill_target = FillTarget::Pixels;
            FillJob::new(doc, target, region, color, clear, available).map_err(js)
        }
    }
    fn selection_available(&self) -> u64 {
        let r = self.residency();
        CPU_BUDGET.saturating_sub(r.document + r.history + r.temporary)
    }
    fn selection_command(
        &mut self,
        selection: Selection,
        label: &'static str,
    ) -> Result<(), JsValue> {
        let command = SelectionCommand::new(
            self.document.as_ref().ok_or("No document")?,
            selection,
            label,
        )
        .map_err(js)?;
        let before = self.revision;
        self.commit_selection_command(Box::new(command))?;
        self.layered.selection_revision(before, self.revision);
        Ok(())
    }
    fn commit_selection_command(&mut self, command: Box<dyn Command>) -> Result<(), JsValue> {
        let only_selection = (command.as_ref() as &dyn std::any::Any).is::<SelectionCommand>();
        self.layered.invalidate_memory();
        let temporary = self.residency().temporary;
        let changed = self
            .history
            .execute(
                self.document.as_mut().ok_or("No document")?,
                command,
                temporary,
            )
            .map_err(js)?;
        self.revision = self.revision.wrapping_add(u32::from(changed));
        self.content_revision = self
            .content_revision
            .wrapping_add(u32::from(changed && !only_selection));
        self.refresh_pages();
        Ok(())
    }
    pub(super) fn selection_json(&self) -> serde_json::Value {
        serde_json::json!({"active":self.document.as_ref().is_some_and(|d|d.selection.is_some()),"quickMask":self.selected.quick_mask,
            "busy":self.selected.job.is_some(),"lastWandPixels":self.selected.last_wand_pixels,
            "clipboard":self.selected.clipboard.as_ref().map(|c|serde_json::json!({"width":c.samples.width,"height":c.samples.height}))})
    }
    fn selection_source(&self, all_layers: bool) -> Result<Arc<TileSurface<Rgba8>>, JsValue> {
        let doc = self.document.as_ref().ok_or("No document")?;
        if all_layers {
            let doc = self.layered.display.as_ref().unwrap_or(doc);
            return Ok(Arc::new(doc.export_raster().map_err(js)?.clone()));
        }
        let target = self
            .layered
            .active
            .or_else(|| doc.layers.first().map(|l| l.id))
            .ok_or("Choose a raster layer")?;
        Ok(Arc::new(
            doc.layer(target)
                .map_err(js)?
                .raster_samples()
                .map_err(js)?
                .clone(),
        ))
    }
}
#[wasm_bindgen]
impl Engine {
    pub fn clipboard_import_step(&mut self) -> Result<bool, JsValue> {
        let done = self
            .import
            .as_mut()
            .ok_or("No clipboard image import")?
            .step()
            .map_err(js)?;
        if done {
            let job = self.import.take().unwrap();
            self.import_ready = false;
            let document = job.finish().map_err(js)?;
            self.selected.clipboard = Some(ClipboardImage {
                samples: Arc::new(document.export_raster().map_err(js)?.clone()),
                origin: [0, 0],
            });
            self.refresh_pages();
        }
        Ok(done)
    }
    /// Prepare source handles before the worker hydrates only the seed tile.
    pub fn selection_prepare(&mut self, action: &str) -> Result<(), JsValue> {
        if self.selected.job.is_some() || self.stroke.is_some() {
            return Err(js("Finish the current operation"));
        }
        let a = self.decode_selection_action(action)?;
        let kind = a["kind"].as_str().ok_or("Invalid selection action")?;
        if matches!(kind, "copy" | "cut") && (self.selected.quick_mask || self.layered.mask_target)
        {
            return Err(js("Return to layer pixels before copying or cutting"));
        }
        if kind == "content-aware" && (self.selected.quick_mask || self.layered.mask_target) {
            return Err(js("Return to layer pixels before content-aware fill"));
        }
        if matches!(kind, "wand" | "bucket" | "copy" | "cut" | "content-aware") {
            self.selected.source =
                Some(self.selection_source(a["allLayers"].as_bool().unwrap_or(false))?);
            self.selected.seed = if matches!(kind, "copy" | "cut" | "content-aware") {
                [0, 0]
            } else {
                serde_json::from_value::<WandOptions>(a["options"].clone())
                    .map_err(js)?
                    .seed
            };
        }
        Ok(())
    }
    pub fn selection_begin(&mut self, action: &str) -> Result<(), JsValue> {
        self.preview_cancel();
        if self.selected.job.is_some() || self.stroke.is_some() {
            return Err(js("Finish the current operation"));
        }
        let a = self.decode_selection_action(action)?;
        let doc = self
            .document
            .as_ref()
            .ok_or("Create or open a document first")?;
        let available = self.selection_available();
        let job = match a["kind"].as_str().ok_or("Invalid selection action")? {
            "all" => {
                if let Some(rect) = doc.canvas {
                    self.selected.job = Some(Job::Shape(
                        ShapeSelectionJob::new(
                            doc.width,
                            doc.height,
                            None,
                            SelectionShape::Rectangle {
                                x0: rect.x as f64,
                                y0: rect.y as f64,
                                x1: (rect.x + rect.width) as f64,
                                y1: (rect.y + rect.height) as f64,
                            },
                            SelectionMode::New,
                            false,
                            available,
                        )
                        .map_err(js)?,
                    ));
                    self.refresh_pages();
                    return Ok(());
                }
                return self.selection_command(
                    Some(Arc::new(TileSurface::with_default(
                        doc.width,
                        doc.height,
                        Gray8(255),
                    ))),
                    "Select all",
                );
            }
            "none" => {
                return self.selection_command(None, "Deselect");
            }
            "shape" => Job::Shape(
                ShapeSelectionJob::new(
                    doc.width,
                    doc.height,
                    doc.selection.clone(),
                    serde_json::from_value(a["shape"].clone()).map_err(js)?,
                    serde_json::from_value(a["mode"].clone()).map_err(js)?,
                    a["antialias"].as_bool().unwrap_or(true),
                    available,
                )
                .map_err(js)?,
            ),
            "modify" => Job::Mask(
                MaskJob::new(
                    doc.selection.clone().ok_or("Select an area first")?,
                    serde_json::from_value(a["operation"].clone()).map_err(js)?,
                    a["radius"]
                        .as_u64()
                        .filter(|v| *v <= 256)
                        .ok_or("Radius must be 0–256 pixels")? as u32,
                    available,
                )
                .map_err(js)?,
            ),
            "wand" | "bucket" => {
                let bucket = a["kind"] == "bucket";
                self.selected.bucket = if bucket {
                    Some(serde_json::from_value(a["color"].clone()).map_err(js)?)
                } else {
                    None
                };
                let mut options: WandOptions =
                    serde_json::from_value(a["options"].clone()).map_err(js)?;
                if bucket {
                    options.mode = SelectionMode::New;
                }
                Job::Wand(
                    WandJob::in_canvas(
                        self.selected
                            .source
                            .clone()
                            .ok_or("Prepare the selection source first")?,
                        if bucket { None } else { doc.selection.clone() },
                        options,
                        if bucket { doc.selection.clone() } else { None },
                        available,
                        doc.canvas,
                    )
                    .map_err(js)?,
                )
            }
            "fill" => Job::Fill(self.fill_candidate(
                None,
                serde_json::from_value(a["color"].clone()).map_err(js)?,
                a["clear"].as_bool().unwrap_or(false),
            )?),
            "content-aware" => {
                if self.selected.quick_mask || self.layered.mask_target {
                    return Err(js("Return to layer pixels before content-aware fill"));
                }
                let target = self
                    .layered
                    .active
                    .or_else(|| doc.layers.first().map(|l| l.id))
                    .ok_or("Choose a raster layer")?;
                Job::ContentAware(
                    ContentAwareJob::new(
                        doc,
                        target,
                        self.selected
                            .source
                            .clone()
                            .ok_or("Prepare the content-aware source first")?,
                        available,
                    )
                    .map_err(js)?,
                )
            }
            "copy" | "cut" => {
                self.selected.cut = a["kind"] == "cut";
                Job::Copy(
                    CopyJob::in_canvas(
                        self.selected
                            .source
                            .clone()
                            .ok_or("Prepare the copy source first")?,
                        doc.selection.clone(),
                        doc.canvas,
                        available,
                    )
                    .map_err(js)?,
                )
            }
            "paste" => Job::Paste(
                PasteJob::new(
                    doc,
                    self.selected
                        .clipboard
                        .clone()
                        .ok_or("The internal clipboard is empty")?,
                    a["inPlace"].as_bool().unwrap_or(false),
                    available,
                )
                .map_err(js)?,
            ),
            _ => return Err(js("Unknown selection action")),
        };
        self.selected.job = Some(job);
        self.refresh_pages();
        Ok(())
    }
    /// The worker hydrates `page_requests(selection)` before each bounded step.
    pub fn selection_cold(&self) -> bool {
        let mut cold = false;
        self.selected.needed(&mut |t| {
            cold |= t.bytes() == 0;
        });
        cold
    }
    pub fn selection_phase(&self) -> u32 {
        if self.selected.ready {
            return 3;
        }
        match &self.selected.job {
            Some(Job::Wand(j)) => j.phase(),
            _ => 4,
        }
    }
    pub fn selection_step(&mut self) -> Result<bool, JsValue> {
        if self.selected.ready && !matches!(self.selected.fill_target, FillTarget::Pixels) {
            if let Some(Job::Fill(j)) = &self.selected.job {
                self.residency()
                    .check_cpu(j.candidate.raster().bytes() / 4)
                    .map_err(js)?;
            }
        }
        let Some(job) = self.selected.job.as_mut() else {
            return Ok(true);
        };
        let done = if self.selected.ready {
            Ok(true)
        } else {
            match job {
                Job::Shape(j) => j.step(),
                Job::Wand(j) => j.step(),
                Job::Mask(j) => j.step(),
                Job::Fill(j) => j.step(),
                Job::ContentAware(j) => j.step(),
                Job::Copy(j) => j.step(),
                Job::Paste(j) => j.step(),
            }
        }
        .map_err(js)?;
        self.layered.invalidate_memory();
        if !done {
            return Ok(false);
        }
        if !self.selected.ready {
            self.selected.ready = true;
            return Ok(false);
        }
        self.selected.ready = false;
        let job = self.selected.job.take().unwrap();
        self.selected.source = None;
        match job {
            Job::Shape(j) => self.selection_command(Some(Arc::new(j.output)), "Selection")?,
            Job::Mask(j) => {
                self.selection_command(Some(Arc::new(j.finish().map_err(js)?)), "Modify selection")?
            }
            Job::Wand(j) => {
                let (output, selected_pixels) = j.finish().map_err(js)?;
                self.selected.last_wand_pixels = selected_pixels;
                if let Some(color) = self.selected.bucket.take() {
                    let region = Arc::new(output);
                    self.selected.job = Some(Job::Fill(self.fill_candidate(
                        Some(region),
                        color,
                        false,
                    )?));
                    self.refresh_pages();
                    return Ok(false);
                } else {
                    self.selection_command(Some(Arc::new(output)), "Magic wand")?;
                }
            }
            Job::Fill(j) => match self.selected.fill_target {
                FillTarget::Pixels => {
                    self.commit_selection_command(Box::new(j.finish().map_err(js)?))?
                }
                FillTarget::Quick => self.finish_quick_mask(&j.candidate, "Fill Quick Mask")?,
                FillTarget::Mask => {
                    let layer = self.mask_preview(&j.candidate).map_err(js)?;
                    let target = layer.id;
                    let doc = self.document.as_ref().unwrap();
                    let mut candidate = doc.clone();
                    *candidate.layer_mut(target).map_err(js)? = layer;
                    let command =
                        LayerCommand::replacing(doc, candidate.layers, "Fill mask").map_err(js)?;
                    self.commit_selection_command(Box::new(command))?;
                }
            },
            Job::ContentAware(j) => {
                self.commit_selection_command(Box::new(j.finish().map_err(js)?))?
            }
            Job::Copy(j) => {
                self.selected.clipboard = Some(j.finish().map_err(js)?);
                self.refresh_pages();
                if self.selected.cut {
                    self.selected.cut = false;
                    let doc = self.document.as_ref().unwrap();
                    let target = self
                        .layered
                        .active
                        .or_else(|| doc.layers.first().map(|l| l.id))
                        .ok_or("Choose a layer")?;
                    self.selected.job = Some(Job::Fill(
                        FillJob::new(doc, target, None, [0; 4], true, self.selection_available())
                            .map_err(js)?
                            .named("Cut"),
                    ));
                    self.refresh_pages();
                    return Ok(false);
                }
            }
            Job::Paste(j) => {
                let command = j.finish(self.document.as_ref().unwrap()).map_err(js)?;
                let target = command.target;
                self.commit_selection_command(Box::new(command))?;
                self.layered.active = Some(target);
                self.layered.mask_target = false;
                self.layered.mask_view = false;
            }
        }
        Ok(true)
    }
    pub fn selection_cancel(&mut self) {
        self.selected.cancel();
        self.refresh_pages();
    }
    pub fn quick_mask(&mut self, enabled: bool) {
        self.selected.quick_mask = enabled;
    }
    pub fn clipboard_export_begin(&mut self) -> Result<(), JsValue> {
        let c = self
            .selected
            .clipboard
            .as_ref()
            .ok_or("The internal clipboard is empty")?;
        let mut layer = Layer::raster(
            "Clipboard".into(),
            TileSurface::new(c.samples.width, c.samples.height),
        );
        layer.content = LayerContent::Raster(c.samples.clone());
        let doc = Document {
            canvas: None,
            guides: Vec::new(),
            width: c.samples.width,
            height: c.samples.height,
            dpi: 72.0,
            layers: vec![layer],
            selection: None,
        };
        self.export = Some(ExportJob::new(&doc, self.residency()).map_err(js)?);
        Ok(())
    }
}
