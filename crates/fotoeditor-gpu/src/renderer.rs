use std::sync::Arc;

use crate::viewport::ViewportRenderer;
use fotoeditor_core::{Document, View};
use web_sys::OffscreenCanvas;
use wgpu::*;

pub struct Renderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    canvas: OffscreenCanvas,
    config: SurfaceConfiguration,
    pipeline: RenderPipeline,
    bind_group: BindGroup,
    backend: &'static str,
    adapter_name: String,
    diagnostic: bool,
    paused: bool,
    resized: bool,
    viewport: ViewportRenderer,
    compositor: Option<crate::compositor::Compositor>,
    selection: crate::selection::SelectionRenderer,
    preview: Option<crate::preview::PreviewFrame>,
    preview_visible: bool,
    preview_cancelled: Arc<std::sync::atomic::AtomicBool>,
}

async fn device_for(adapter: &Adapter) -> Result<(Device, Queue), String> {
    adapter
        .request_device(&DeviceDescriptor {
            label: Some("FotoEditor engine"),
            required_limits: Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        })
        .await
        .map_err(|error| format!("GPU device unavailable: {error}"))
}

async fn webgpu_device() -> Result<(Instance, Adapter, Device, Queue), String> {
    let instance = Instance::new(InstanceDescriptor {
        backends: Backends::BROWSER_WEBGPU,
        ..InstanceDescriptor::new_without_display_handle()
    });
    // Request the device before acquiring a canvas context: a failed request must
    // leave the canvas free to acquire a WebGL2 context instead.
    let adapter = instance
        .request_adapter(&RequestAdapterOptions::default())
        .await
        .map_err(|error| format!("WebGPU adapter unavailable: {error}"))?;
    let (device, queue) = device_for(&adapter).await?;
    Ok((instance, adapter, device, queue))
}

impl Renderer {
    pub async fn new(
        canvas: OffscreenCanvas,
        preference: &str,
        diagnostic: bool,
        on_error: fn(String),
    ) -> Result<Self, String> {
        let candidate = if preference != "webgl2" && util::is_browser_webgpu_supported().await {
            webgpu_device().await.ok()
        } else {
            None
        };
        let (surface, adapter, device, queue) = match candidate {
            Some((instance, adapter, device, queue)) => {
                let surface = instance
                    .create_surface(SurfaceTarget::OffscreenCanvas(canvas.clone()))
                    .map_err(|error| format!("WebGPU surface unavailable: {error}"))?;
                (surface, adapter, device, queue)
            }
            None => {
                let instance = Instance::new(InstanceDescriptor {
                    backends: Backends::GL,
                    ..InstanceDescriptor::new_without_display_handle()
                });
                let surface = instance
                    .create_surface(SurfaceTarget::OffscreenCanvas(canvas.clone()))
                    .map_err(|error| format!("WebGL2 surface unavailable: {error}"))?;
                let adapter = instance
                    .request_adapter(&RequestAdapterOptions {
                        compatible_surface: Some(&surface),
                        ..Default::default()
                    })
                    .await
                    .map_err(|error| format!("No usable WebGPU or WebGL2 adapter: {error}"))?;
                let (device, queue) = device_for(&adapter).await?;
                (surface, adapter, device, queue)
            }
        };
        let info = adapter.get_info();
        let backend = match info.backend {
            Backend::BrowserWebGpu => "WebGPU",
            Backend::Gl => "WebGL2",
            _ => return Err("Unexpected browser GPU backend".into()),
        };
        device.set_device_lost_callback(move |reason, message| {
            if reason != DeviceLostReason::Destroyed {
                on_error(format!(
                    "GPU device lost: {message}. Retry to restart the engine."
                ));
            }
        });
        device.on_uncaptured_error(Arc::new(move |error| {
            on_error(format!("GPU error: {error}"));
        }));
        let mut config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or("Adapter cannot present to the canvas")?;
        let unorm = config.format.remove_srgb_suffix();
        if surface.get_capabilities(&adapter).formats.contains(&unorm) {
            config.format = unorm;
        }
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Rust diagnostic RGBA8 samples"),
            size: Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(2),
            },
            texture.size(),
        );
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Diagnostic texture shader"),
            source: ShaderSource::Wgsl(include_str!("diagnostic.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FotoEditor texture pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor::default());
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Diagnostic texture binding"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let viewport = ViewportRenderer::new(&device, config.format);
        let selection = crate::selection::SelectionRenderer::new(&device, config.format);
        let mut renderer = Self {
            surface,
            device,
            queue,
            canvas,
            config,
            pipeline,
            bind_group,
            backend,
            adapter_name: info.name,
            diagnostic,
            paused: true,
            resized: true,
            viewport,
            compositor: None,
            selection,
            preview: None,
            preview_visible: false,
            preview_cancelled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        renderer.resize(renderer.canvas.width(), renderer.canvas.height());
        Ok(renderer)
    }

    pub fn backend(&self) -> &str {
        self.backend
    }
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn width(&self) -> u32 {
        if self.paused {
            0
        } else {
            self.config.width
        }
    }
    pub fn height(&self) -> u32 {
        if self.paused {
            0
        } else {
            self.config.height
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let was_paused = self.paused;
        self.paused = width == 0 || height == 0;
        if self.paused {
            return;
        }
        let max = self.device.limits().max_texture_dimension_2d;
        let width = width.min(max);
        let height = height.min(max);
        if !was_paused && width == self.config.width && height == self.config.height {
            return;
        }
        // Both wgpu browser backends set the canvas dimensions in configure.
        // Setting them here too resets the WebGPU presentation texture twice.
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.resized = true;
    }

    pub fn clear_tiles(&mut self) {
        self.viewport.clear();
    }
    pub fn invalidate_tiles(
        &mut self,
        surface: &fotoeditor_core::TileSurface<fotoeditor_core::Rgba8>,
        dirty: &fotoeditor_core::DirtyRegions,
    ) {
        self.viewport.invalidate(dirty, surface);
    }
    pub fn gpu_bytes(&self) -> u64 {
        self.viewport.bytes()
            + self.compositor.as_ref().map_or(0, |c| c.bytes())
            + self.selection.gpu_bytes()
            + self.preview.as_ref().map_or(0, |p| p.bytes())
    }
    pub fn selection_cpu_bytes(&self) -> u64 {
        self.selection.cpu_bytes()
    }
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }
    pub fn prepare_selection(
        &mut self,
        source: fotoeditor_core::SelectionDisplaySource<'_>,
        view: &View,
        state: (u32, u32, bool),
    ) -> Result<(), String> {
        self.viewport.budget(
            fotoeditor_core::GPU_BUDGET.saturating_sub(
                self.compositor
                    .as_ref()
                    .map_or(0, |_| crate::compositor::COMPOSITE_RESERVE)
                    + 8 * 1024 * 1024
                    + 32
                    + self.preview.as_ref().map_or(0, |p| p.bytes()),
            ),
        );
        self.selection.prepare(
            &self.device,
            &self.queue,
            source,
            view,
            (self.config.width, self.config.height),
            state,
        )
    }
    pub fn composite(
        &mut self,
        plan: &fotoeditor_core::CompositePlan,
    ) -> Result<js_sys::Promise, String> {
        self.viewport.budget(
            fotoeditor_core::GPU_BUDGET
                - crate::compositor::COMPOSITE_RESERVE
                - self.selection.gpu_bytes()
                - self.preview.as_ref().map_or(0, |p| p.bytes()),
        );
        self.compositor
            .get_or_insert_with(|| crate::compositor::Compositor::new(&self.device))
            .compose(&self.device, &self.queue, plan)
    }
    pub fn clear_preview(&mut self) {
        self.preview_cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.preview = None;
        self.preview_visible = false;
        self.viewport.budget(
            fotoeditor_core::GPU_BUDGET
                - self
                    .compositor
                    .as_ref()
                    .map_or(0, |_| crate::compositor::COMPOSITE_RESERVE)
                - self.selection.gpu_bytes(),
        );
    }
    pub fn preview_hide(&mut self) {
        self.preview_visible = false;
    }
    pub fn preview_begin(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.clear_preview();
        self.preview_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let bytes =
            u64::from(width.div_ceil(256) * 256) * u64::from(height.div_ceil(256) * 256) * 4 + 16;
        let reserved = crate::compositor::COMPOSITE_RESERVE + 8 * 1024 * 1024 + bytes;
        if reserved + 48 > fotoeditor_core::GPU_BUDGET {
            return Err("Preview exceeds the shared GPU budget".into());
        }
        self.viewport.budget(fotoeditor_core::GPU_BUDGET - reserved);
        self.preview = Some(crate::preview::PreviewFrame::new(
            &self.device,
            self.config.format,
            width,
            height,
        )?);
        Ok(())
    }
    pub fn preview_tile(
        &mut self,
        coord: fotoeditor_core::TileCoord,
        plan: &fotoeditor_core::CompositePlan,
    ) -> Result<(), String> {
        let frame = self.preview.as_ref().ok_or("No viewport preview")?;
        if coord.0 >= frame.width.div_ceil(256) || coord.1 >= frame.height.div_ceil(256) {
            return Err("Preview tile is outside the frame".into());
        }
        let texture = self
            .compositor
            .get_or_insert_with(|| crate::compositor::Compositor::new(&self.device))
            .compose_texture(&self.device, &self.queue, plan)?;
        frame.accept(&self.device, &self.queue, coord, &texture);
        Ok(())
    }
    pub fn preview_present(&mut self) {
        self.preview_visible = self.preview.is_some();
    }
    /// Initial pipeline work must finish before controls report ready. No image
    /// readback is needed; WebGL2 progresses its queue fence by async polling.
    pub fn preview_settled(&self, diagnostic_hold: bool) -> js_sys::Promise {
        use std::sync::atomic::{AtomicBool, Ordering};
        let complete = Arc::new(AtomicBool::new(false));
        let signal = complete.clone();
        if !diagnostic_hold {
            self.queue
                .on_submitted_work_done(move || signal.store(true, Ordering::Relaxed));
        }
        let device = self.device.clone();
        let cancelled = self.preview_cancelled.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            while !complete.load(Ordering::Relaxed) {
                if cancelled.load(Ordering::Relaxed) {
                    return Err(wasm_bindgen::JsValue::from_str("Preview cancelled"));
                }
                device.poll(PollType::Poll).map_err(|e| {
                    wasm_bindgen::JsValue::from_str(&format!("Preview queue failed: {e}"))
                })?;
                if complete.load(Ordering::Relaxed) {
                    break;
                }
                let delay = js_sys::Promise::new(&mut |resolve, _| {
                    use wasm_bindgen::{JsCast, JsValue};
                    if let Ok(timer) =
                        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("setTimeout"))
                    {
                        let timer = timer.unchecked_into::<js_sys::Function>();
                        let _ = timer.call2(&JsValue::NULL, &resolve, &JsValue::from_f64(4.0));
                    }
                });
                wasm_bindgen_futures::JsFuture::from(delay).await?;
            }
            Ok(wasm_bindgen::JsValue::UNDEFINED)
        })
    }
    pub fn visible_tiles(&self) -> u32 {
        self.viewport.visible
    }
    pub fn refining_tiles(&self) -> u32 {
        self.viewport.refining
    }
    pub fn uploaded_bytes(&self) -> u64 {
        self.viewport.uploaded
    }
    pub fn frame(&mut self, doc: Option<&Document>, view: &View, dpr: f64) -> Result<bool, String> {
        if self.paused {
            return Ok(false);
        }
        // A transferred canvas resize crosses the browser compositor boundary.
        // Acquire a fresh texture on the next animation frame, after it settles.
        if self.resized {
            self.resized = false;
            return Ok(false);
        }
        if !self.diagnostic {
            self.selection
                .update_time(&self.queue, (self.config.width, self.config.height));
            if self.preview_visible {
                self.viewport
                    .prepare_background(&self.queue, view, dpr, doc.is_some());
            } else {
                self.viewport
                    .prepare(&self.device, &self.queue, doc, view, dpr)?;
            }
        }
        let output = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            }
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(false);
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(false),
            CurrentSurfaceTexture::Validation => return Err("GPU surface validation failed".into()),
        };
        let output_view = output
            .texture
            .create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FotoEditor canvas"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 48.0 / 255.0,
                            g: 52.0 / 255.0,
                            b: 59.0 / 255.0,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if self.diagnostic {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..3, 0..1);
            } else {
                if self.preview_visible {
                    self.viewport.draw_background(&mut pass);
                    if let Some(preview) = &self.preview {
                        preview.draw(&mut pass);
                    }
                } else {
                    self.viewport.draw(&mut pass, view);
                }
                self.selection.draw(&mut pass);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
        Ok(true)
    }

    pub fn dispose(&self) {
        self.preview_cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.device.destroy();
    }
    pub fn simulate_device_loss(&self) {
        self.preview_cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.device.destroy();
    }
}
