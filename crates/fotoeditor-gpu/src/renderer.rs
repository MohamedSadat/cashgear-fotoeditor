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
    pub fn invalidate_tiles(&mut self, dirty: &fotoeditor_core::DirtySet) {
        self.viewport.invalidate(dirty);
    }
    pub fn gpu_bytes(&self) -> u64 {
        self.viewport.bytes()
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
            self.viewport
                .prepare(&self.device, &self.queue, doc, view, dpr)?;
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
                self.viewport.draw(&mut pass, view);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
        Ok(true)
    }

    pub fn dispose(&self) {
        self.device.destroy();
    }
    pub fn simulate_device_loss(&self) {
        self.device.destroy();
    }
}
