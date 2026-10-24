//! The only JavaScript-facing engine API.
#[cfg(target_arch = "wasm32")]
mod bridge {
    mod adjustments;
    mod filters;
    mod layers;
    mod preview;
    mod selection;
    use fotoeditor_brush::{BrushSettings, Sample, StrokeCommand};
    use fotoeditor_core::{Document, History, HistoryLimits, Residency, ResolutionCommand, View};
    use fotoeditor_io::{ExportJob, ImportJob, ProjectReader, ProjectWriter};
    use std::cell::RefCell;
    use wasm_bindgen::prelude::*;
    const DISPLAY_SCRATCH: u64 = 1024 * 1024;

    thread_local! {
        static ERROR_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    }

    fn report_error(message: String) {
        ERROR_CALLBACK.with(|callback| {
            if let Some(callback) = callback.borrow().as_ref() {
                let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&message));
            }
        });
    }

    #[wasm_bindgen]
    pub struct Engine {
        renderer: fotoeditor_gpu::Renderer,
        document: Option<Document>,
        history: History,
        stroke: Option<StrokeCommand>,
        paint_time: f64,
        paint_complete_time: f64,
        view: View,
        dpr: f64,
        import: Option<ImportJob>,
        export: Option<ExportJob>,
        project_read: Option<ProjectReader>,
        project_write: Option<ProjectWriter>,
        revision: u32,
        content_revision: u32,
        external_bytes: u64,
        layered: layers::LayerRuntime,
        selected: selection::SelectionRuntime,
        adjusted: adjustments::AdjustmentRuntime,
    }

    // Lifecycle replacement starts a new editing session and releases its history.
    enum SessionCommand {
        ReplaceDocument(Document),
    }
    impl Engine {
        fn execute(&mut self, command: SessionCommand) {
            match command {
                SessionCommand::ReplaceDocument(doc) => {
                    self.view.document_width = doc.width as f64;
                    self.view.document_height = doc.height as f64;
                    self.view.rotation = 0.0;
                    self.view.flipped = false;
                    self.view.fit();
                    self.document = Some(doc);
                    self.history.clear();
                    self.stroke = None;
                    self.paint_time = 0.0;
                    self.paint_complete_time = 0.0;
                    self.renderer.clear_tiles();
                    self.renderer.clear_preview();
                    self.layered = layers::LayerRuntime::default();
                    self.selected.cancel();
                    self.selected.quick_mask = false;
                    self.adjusted = adjustments::AdjustmentRuntime::default();
                    self.revision = self.revision.wrapping_add(1);
                    self.content_revision = self.content_revision.wrapping_add(1);
                }
            }
        }
        fn residency(&self) -> Residency {
            Residency {
                document: self.document.as_ref().map_or(0, |d| d.bytes()),
                history: self.document.as_ref().map_or(0, |d| self.history.bytes(d)),
                temporary: self.import.as_ref().map_or(0, |j| j.temporary_bytes())
                    + self.renderer.selection_cpu_bytes()
                    + self.external_bytes
                    + self.layered.bytes()
                    + self.selected.scratch_bytes()
                    + self.adjusted.scratch_bytes()
                    + self.layered.extra_source_bytes(
                        self.document.as_ref(),
                        &self.history,
                        self.stroke.as_ref().map(|s| &s.preview),
                        &self.selected,
                        &self.adjusted,
                    )
                    + self.export.as_ref().map_or(0, |j| j.temporary_bytes())
                    + self
                        .project_read
                        .as_ref()
                        .map_or(0, |j| j.temporary_bytes())
                    + self
                        .project_write
                        .as_ref()
                        .map_or(0, |j| j.temporary_bytes())
                    + self
                        .stroke
                        .as_ref()
                        .map_or(0, |s| s.temporary_bytes() + DISPLAY_SCRATCH),
                gpu: self.renderer.gpu_bytes(),
            }
        }
    }

    #[wasm_bindgen]
    impl Engine {
        pub async fn create(
            canvas: web_sys::OffscreenCanvas,
            preference: String,
            diagnostic: bool,
            on_error: js_sys::Function,
        ) -> Result<Engine, JsValue> {
            ERROR_CALLBACK.with(|callback| *callback.borrow_mut() = Some(on_error));
            std::panic::set_hook(Box::new(|info| report_error(format!("Rust panic: {info}"))));
            let renderer =
                fotoeditor_gpu::Renderer::new(canvas, &preference, diagnostic, report_error)
                    .await
                    .map_err(|error| JsValue::from_str(&error))?;
            Ok(Self {
                renderer,
                document: None,
                history: History::default(),
                stroke: None,
                paint_time: 0.0,
                paint_complete_time: 0.0,
                view: View::default(),
                dpr: 1.0,
                import: None,
                export: None,
                project_read: None,
                project_write: None,
                revision: 0,
                content_revision: 0,
                external_bytes: 0,
                layered: layers::LayerRuntime::default(),
                selected: selection::SelectionRuntime::default(),
                adjusted: adjustments::AdjustmentRuntime::default(),
            })
        }
        pub fn backend(&self) -> String {
            self.renderer.backend().into()
        }
        pub fn adapter_name(&self) -> String {
            self.renderer.adapter_name().into()
        }
        pub fn width(&self) -> u32 {
            self.renderer.width()
        }
        pub fn height(&self) -> u32 {
            self.renderer.height()
        }
        pub fn resize(&mut self, width: u32, height: u32, dpr: f64) {
            self.renderer.resize(width, height);
            self.dpr = if dpr.is_finite() && dpr > 0.0 {
                dpr
            } else {
                1.0
            };
            self.view.width = self.renderer.width() as f64 / self.dpr;
            self.view.height = self.renderer.height() as f64 / self.dpr;
        }
        pub fn frame(&mut self) -> Result<bool, JsValue> {
            // Rendering may prepare display/selection scratch. Refresh the
            // filter's incremental residency bound after every UI frame.
            if self.adjusted.filter.scratch_bytes() > 0 {
                self.layered.invalidate_memory();
            }
            if self.adjusted.filter.last.is_some() && !self.filter_can_fade() {
                self.adjusted.filter.last = None;
                self.refresh_pages();
            }
            if self.adjusted.preview.is_some() && !self.preview_valid() {
                self.preview_cancel();
            }
            if self.layered.display.is_some()
                || self.layered.requires_composite(self.document.as_ref())
            {
                if let Some(doc) = self.layered.display.as_mut() {
                    let regions = doc.raster_mut().drain_regions();
                    self.renderer.invalidate_tiles(doc.raster(), &regions);
                }
                return self
                    .renderer
                    .frame(self.layered.display.as_ref(), &self.view, self.dpr)
                    .map_err(|e| JsValue::from_str(&e));
            }
            let document = self
                .stroke
                .as_mut()
                .filter(|_| !self.selected.quick_mask)
                .map(|s| &mut s.preview)
                .or(self.document.as_mut());
            if let Some(doc) = document {
                let regions = doc.raster_mut().drain_regions();
                self.renderer.invalidate_tiles(doc.raster(), &regions);
            }
            self.renderer
                .frame(
                    self.stroke
                        .as_ref()
                        .filter(|_| !self.selected.quick_mask)
                        .map(|s| &s.preview)
                        .or(self.document.as_ref()),
                    &self.view,
                    self.dpr,
                )
                .map_err(|error| JsValue::from_str(&error))
        }
        pub fn dispose(&self) {
            self.renderer.dispose();
            ERROR_CALLBACK.with(|callback| *callback.borrow_mut() = None);
        }
        pub fn diagnostic_device_loss(&self) {
            self.renderer.simulate_device_loss();
            report_error("GPU device lost (diagnostic). Retry to restart the engine.".into());
        }
        pub fn diagnostic_panic(&self) {
            panic!("diagnostic engine panic");
        }
        pub fn new_document(&mut self, width: u32, height: u32, dpi: f64) -> Result<(), JsValue> {
            let doc = Document::blank(width, height, dpi).map_err(|e| JsValue::from_str(&e))?;
            self.execute(SessionCommand::ReplaceDocument(doc));
            Ok(())
        }
        pub fn undo(&mut self) -> bool {
            self.preview_cancel();
            let only_selection = self
                .history
                .selection_only_jump(self.history.depths().0.saturating_sub(1));
            let before = self.revision;
            let changed = self.document.as_mut().is_some_and(|d| self.history.undo(d));
            self.revision = self.revision.wrapping_add(u32::from(changed));
            self.content_revision = self
                .content_revision
                .wrapping_add(u32::from(changed && !only_selection));
            self.normalize_layer_target();
            if only_selection {
                self.layered.selection_revision(before, self.revision);
            }
            changed
        }
        pub fn redo(&mut self) -> bool {
            self.preview_cancel();
            let only_selection = self
                .history
                .selection_only_jump(self.history.depths().0 + 1);
            let before = self.revision;
            let changed = self.document.as_mut().is_some_and(|d| self.history.redo(d));
            self.revision = self.revision.wrapping_add(u32::from(changed));
            self.content_revision = self
                .content_revision
                .wrapping_add(u32::from(changed && !only_selection));
            self.normalize_layer_target();
            if only_selection {
                self.layered.selection_revision(before, self.revision);
            }
            changed
        }
        pub fn history_limits(&mut self, bytes: u32, steps: u32) -> Result<(), JsValue> {
            if let Some(doc) = &self.document {
                self.history
                    .configure(
                        doc,
                        HistoryLimits {
                            bytes: bytes as u64,
                            steps: steps as usize,
                        },
                    )
                    .map_err(|e| JsValue::from_str(&e))?;
            } else if bytes as u64 <= fotoeditor_core::CPU_BUDGET && steps <= 2000 {
                self.history.limits = HistoryLimits {
                    bytes: bytes as u64,
                    steps: steps as usize,
                };
            } else {
                return Err(JsValue::from_str("Invalid history limits"));
            }
            Ok(())
        }
        pub fn history_jump(&mut self, target: u32) -> Result<(), JsValue> {
            self.preview_cancel();
            let only_selection = self.history.selection_only_jump(target as usize);
            let before = self.revision;
            let doc = self
                .document
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No document"))?;
            let changed = self
                .history
                .jump(doc, target as usize)
                .map_err(|e| JsValue::from_str(&e))?;
            self.revision = self.revision.wrapping_add(u32::from(changed));
            self.content_revision = self
                .content_revision
                .wrapping_add(u32::from(changed && !only_selection));
            self.normalize_layer_target();
            if only_selection {
                self.layered.selection_revision(before, self.revision);
            }
            Ok(())
        }
        pub fn resolution(&mut self, dpi: f64, gesture: u32) -> Result<(), JsValue> {
            let doc = self
                .document
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No document"))?;
            fotoeditor_core::validate_size(doc.width, doc.height, dpi)
                .map_err(|e| JsValue::from_str(&e))?;
            let command = ResolutionCommand {
                before: doc.dpi,
                after: dpi,
                gesture,
            };
            let changed = self
                .history
                .execute(doc, Box::new(command), 0)
                .map_err(|e| JsValue::from_str(&e))?;
            self.revision = self.revision.wrapping_add(u32::from(changed));
            self.content_revision = self.content_revision.wrapping_add(u32::from(changed));
            Ok(())
        }
        pub fn stroke_begin(&mut self, settings: &str) -> Result<(), JsValue> {
            self.preview_cancel();
            self.layered.invalidate_memory();
            if self.stroke.is_some() {
                return Err(JsValue::from_str("Finish the current stroke first"));
            }
            let mut settings: BrushSettings =
                serde_json::from_str(settings).map_err(|e| JsValue::from_str(&e.to_string()))?;
            let doc = self
                .document
                .as_ref()
                .ok_or_else(|| JsValue::from_str("Create or open a document first"))?;
            let r = self.residency();
            let available = fotoeditor_core::CPU_BUDGET
                .saturating_sub(r.document + r.history + r.temporary + DISPLAY_SCRATCH);
            let target = self
                .layered
                .active
                .or_else(|| doc.layers.first().map(|l| l.id))
                .ok_or_else(|| JsValue::from_str("Choose a layer to paint"))?;
            let mask_doc;
            let source = if self.selected.quick_mask || self.layered.mask_target {
                mask_doc = self
                    .quick_or_layer_mask_document(target)
                    .map_err(|e| JsValue::from_str(&e))?;
                self.layered.mask_base_bytes = mask_doc.raster().bytes();
                let gray = if settings.eraser {
                    255
                } else {
                    ((u32::from(settings.color[0]) * 30
                        + u32::from(settings.color[1]) * 59
                        + u32::from(settings.color[2]) * 11
                        + 50)
                        / 100) as u8
                };
                settings.color = [gray, gray, gray, settings.color[3]];
                settings.eraser = false;
                &mask_doc
            } else {
                doc
            };
            let target = if self.selected.quick_mask {
                source.layers[0].id
            } else {
                target
            };
            self.stroke = Some(
                StrokeCommand::for_layer(
                    source,
                    settings,
                    available.saturating_sub(self.layered.mask_base_bytes),
                    target,
                )
                .map_err(|e| JsValue::from_str(&e))?,
            );
            Ok(())
        }
        pub fn stroke_sample(&mut self, sample: &[f64]) -> Result<(), JsValue> {
            if sample.len() != 6 {
                return Err(JsValue::from_str("Invalid pointer sample"));
            }
            let (x, y) = self.view.screen_to_document(sample[0], sample[1]);
            self.stroke
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No stroke in progress"))?
                .push(Sample {
                    x,
                    y,
                    pressure: sample[2],
                    tilt_x: sample[3],
                    tilt_y: sample[4],
                    time: sample[5],
                })
                .map_err(|e| JsValue::from_str(&e))
        }
        pub fn stroke_end(&mut self) {
            if let Some(s) = &mut self.stroke {
                s.end();
            }
        }
        pub fn painting(&self) -> bool {
            self.stroke.is_some()
        }
        pub fn needs_render(&self) -> bool {
            if self.layered.requires_composite(self.document.as_ref()) {
                return false;
            }
            self.stroke
                .as_ref()
                .filter(|_| !self.selected.quick_mask)
                .map(|s| &s.preview)
                .or(self.document.as_ref())
                .is_some_and(|d| d.raster().has_dirty())
        }
        pub fn stroke_cancel(&mut self) {
            self.layered.mask_base_bytes = 0;
            if self.stroke.take().is_some() {
                self.layered.invalidate_display();
                self.renderer.clear_tiles();
            }
        }
        pub fn stroke_step(&mut self) -> Result<(), JsValue> {
            if self.selected.quick_mask && self.stroke.is_some() {
                self.selected.serial = self.selected.serial.wrapping_add(1);
            }
            if self.stroke.is_some() {
                self.layered.invalidate_memory();
            }
            if let Some(s) = &mut self.stroke {
                s.step(262144).map_err(|e| JsValue::from_str(&e))?;
                self.paint_time = self.paint_time.max(s.applied_time);
                self.paint_complete_time = self.paint_complete_time.max(s.completed_time);
            }
            if self.stroke.as_ref().is_some_and(|s| s.done()) {
                if self.selected.quick_mask {
                    let preview = self.stroke.as_ref().unwrap().preview.clone();
                    let result = self.finish_quick_stroke(&preview);
                    self.stroke = None;
                    self.layered.mask_base_bytes = 0;
                    return result;
                }
                let mut stroke = self.stroke.take().unwrap();
                self.layered.dirty = Some(
                    stroke
                        .preview
                        .raster_mut()
                        .drain_dirty()
                        .into_iter()
                        .collect(),
                );
                let command: Box<dyn fotoeditor_core::Command> = if self.layered.mask_target {
                    let layer = self
                        .mask_preview(&stroke.preview)
                        .map_err(|e| JsValue::from_str(&e))?;
                    let doc = self.document.as_ref().unwrap();
                    let mut candidate = doc.clone();
                    let target = layer.id;
                    *candidate
                        .layer_mut(target)
                        .map_err(|e| JsValue::from_str(&e))? = layer;
                    Box::new(
                        fotoeditor_core::LayerCommand::replacing(
                            doc,
                            candidate.layers,
                            "Paint mask",
                        )
                        .map_err(|e| JsValue::from_str(&e))?,
                    )
                } else {
                    Box::new(stroke.finish().map_err(|e| JsValue::from_str(&e))?)
                };
                let changed = self
                    .history
                    .execute(
                        self.document.as_mut().unwrap(),
                        command,
                        self.layered.bytes(),
                    )
                    .map_err(|e| JsValue::from_str(&e))?;
                self.revision = self.revision.wrapping_add(u32::from(changed));
                self.content_revision = self.content_revision.wrapping_add(u32::from(changed));
                self.layered.mask_base_bytes = 0;
            }
            Ok(())
        }
        pub fn sample_color(&self, x: f64, y: f64) -> Vec<u8> {
            let p = self.view.screen_to_document(x, y);
            self.document
                .as_ref()
                .filter(|d| {
                    p.0 >= 0.0 && p.1 >= 0.0 && p.0 < d.width as f64 && p.1 < d.height as f64
                })
                .map_or_else(Vec::new, |d| {
                    self.layered
                        .display
                        .as_ref()
                        .unwrap_or(d)
                        .raster()
                        .sample(p.0.floor() as u32, p.1.floor() as u32)
                        .0
                        .to_vec()
                })
        }
        pub fn begin_import(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            self.import = Some(
                ImportJob::decode(bytes, self.residency()).map_err(|e| JsValue::from_str(&e))?,
            );
            Ok(())
        }
        pub fn import_step(&mut self) -> Result<bool, JsValue> {
            let done = self
                .import
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No import in progress"))?
                .step();
            if done {
                let job = self.import.take().unwrap();
                self.execute(SessionCommand::ReplaceDocument(job.document));
            }
            Ok(done)
        }
        pub fn begin_export(&mut self) -> Result<(), JsValue> {
            let source = if self.export_original() {
                self.document.as_ref()
            } else {
                self.layered.display.as_ref()
            };
            self.export = Some(
                ExportJob::new(
                    source.ok_or_else(|| JsValue::from_str("Open or create a document first"))?,
                    self.residency(),
                )
                .map_err(|e| JsValue::from_str(&e))?,
            );
            Ok(())
        }
        pub fn export_step(&mut self) -> Result<bool, JsValue> {
            self.export
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No export in progress"))?
                .step()
                .map_err(|e| JsValue::from_str(&e))
        }
        pub fn finish_export(&mut self) -> Result<Vec<u8>, JsValue> {
            self.export
                .take()
                .ok_or_else(|| JsValue::from_str("No export in progress"))?
                .finish()
                .map_err(|e| JsValue::from_str(&e))
        }
        pub fn cancel_job(&mut self) {
            self.filter_cancel();
            self.adjusted.apply = None;
            self.adjusted.histogram = None;
            self.selected.cancel();
            self.layered.cancel();
            self.import = None;
            self.export = None;
            self.project_read = None;
            self.project_write = None;
        }
        pub fn begin_project_read(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            self.project_read = Some(
                ProjectReader::new(bytes, self.residency()).map_err(|e| JsValue::from_str(&e))?,
            );
            Ok(())
        }
        pub fn external_bytes(&mut self, bytes: u32) -> Result<(), JsValue> {
            self.layered.invalidate_memory();
            let mut r = self.residency();
            r.temporary = r.temporary.saturating_sub(self.external_bytes);
            r.check_cpu(bytes as u64)
                .map_err(|e| JsValue::from_str(&e))?;
            self.external_bytes = bytes as u64;
            Ok(())
        }
        pub fn admit_project_buffer(&self, size: u32) -> Result<(), JsValue> {
            self.residency()
                .check_cpu(3 * size as u64 + 262144)
                .map_err(|e| JsValue::from_str(&e))
        }
        pub fn begin_project_validation(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            self.project_read = Some(
                ProjectReader::validator(bytes, self.residency())
                    .map_err(|e| JsValue::from_str(&e))?,
            );
            Ok(())
        }
        pub fn project_read_step(&mut self) -> Result<bool, JsValue> {
            self.project_read
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No project read"))?
                .step()
                .map_err(|e| JsValue::from_str(&e))
        }
        pub fn finish_project_read(&mut self, commit: bool) -> Result<(), JsValue> {
            let doc = self
                .project_read
                .take()
                .ok_or_else(|| JsValue::from_str("No project read"))?
                .finish()
                .map_err(|e| JsValue::from_str(&e))?;
            if commit {
                self.execute(SessionCommand::ReplaceDocument(doc));
            }
            Ok(())
        }
        pub fn begin_project_write(&mut self, recovery: bool) -> Result<(), JsValue> {
            let doc = self
                .document
                .as_ref()
                .ok_or_else(|| JsValue::from_str("No document"))?;
            let size = 40 + doc.raster().tiles().len() as u64 * (262144 + 8);
            if size > fotoeditor_io::MAX_FILE_BYTES {
                return Err(JsValue::from_str("Project exceeds the 128 MiB file limit"));
            }
            // Recovery also holds serialized output while readback validation uses
            // JS input, bridge input and parser bytes. Evict before allocating them.
            self.history
                .reserve(doc, size * if recovery { 4 } else { 2 } + 262144)
                .map_err(|e| JsValue::from_str(&e))?;
            self.project_write = Some(
                ProjectWriter::new(
                    self.document
                        .as_ref()
                        .ok_or_else(|| JsValue::from_str("No document"))?,
                    self.residency(),
                )
                .map_err(|e| JsValue::from_str(&e))?,
            );
            Ok(())
        }
        pub fn project_write_step(&mut self) -> Result<bool, JsValue> {
            Ok(self
                .project_write
                .as_mut()
                .ok_or_else(|| JsValue::from_str("No project save"))?
                .step())
        }
        pub fn finish_project_write(&mut self) -> Result<Vec<u8>, JsValue> {
            self.project_write
                .take()
                .ok_or_else(|| JsValue::from_str("No project save"))?
                .finish()
                .map_err(|e| JsValue::from_str(&e))
        }
        pub fn navigate(&mut self, kind: &str, x: f64, y: f64, amount: f64) {
            if !x.is_finite() || !y.is_finite() || !amount.is_finite() {
                return;
            }
            match kind {
                "pan" => {
                    self.view.pan_x += x;
                    self.view.pan_y += y;
                }
                "zoom" => self.view.zoom_at(self.view.zoom * amount, x, y),
                "actual" => self
                    .view
                    .zoom_at(1.0, self.view.width / 2.0, self.view.height / 2.0),
                "fit" => self.view.fit(),
                "rotate" => {
                    self.view.rotation =
                        (self.view.rotation + amount).rem_euclid(std::f64::consts::TAU)
                }
                "reset-rotation" => self.view.rotation = 0.0,
                "flip" => self.view.flipped = !self.view.flipped,
                _ => {}
            }
        }
        pub fn cursor(&self, x: f64, y: f64) -> Vec<f64> {
            let p = self.view.screen_to_document(x, y);
            vec![p.0, p.1]
        }
        pub fn snapshot(&self) -> String {
            let r = self.residency();
            let v = &self.view;
            serde_json::json!({"document":self.document.as_ref().map(|d|serde_json::json!({"width":d.width,"height":d.height,"dpi":d.dpi,"tiles":d.tile_count()})),
                "canFade":self.filter_can_fade(),
                "adjustmentPreview":{"cached":self.preview_ready(),"active":self.preview_valid() && self.adjusted.preview.as_ref().is_some_and(|p| (p.settings.is_some() || p.filter.is_some()) && p.next == p.job.output.width.div_ceil(256)*p.job.output.height.div_ceil(256))},
                "layers":self.layer_json(),
                "selection":self.selection_json(),
                "view":{"zoom":v.zoom,"rotation":v.rotation,"flipped":v.flipped,"panX":v.pan_x,"panY":v.pan_y,"screenWidth":v.width,"screenHeight":v.height},
                "memory":{"document":r.document,"history":r.history,"temporary":r.temporary,"gpu":r.gpu},
                "history":{"undo":self.history.depths().0,"redo":self.history.depths().1,"labels":self.history.labels(),"bytesLimit":self.history.limits.bytes,"stepsLimit":self.history.limits.steps,"evicted":self.history.evicted},
                "revision":self.revision,
                "contentRevision":self.content_revision,
                "painting":self.stroke.is_some(),"pendingSamples":self.stroke.as_ref().map_or(0,|s|s.pending()),
                "paintTime":self.paint_time,
                "paintCompleteTime":self.paint_complete_time,
                "visibleTiles":self.renderer.visible_tiles(),"refining":self.renderer.refining_tiles(),"uploadedBytes":self.renderer.uploaded_bytes()}).to_string()
        }
    }
}
