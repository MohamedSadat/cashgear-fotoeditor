//! Portable fragment passes with float32 premultiplied intermediates. One tile
//! is read back at a time; authoritative straight-alpha sources are never changed.
use fotoeditor_core::{CompositePlan, CompositeStep, LayerProperties, TILE_SIZE};
use wasm_bindgen::JsValue;
use wgpu::*;
const FLOAT_BYTES: u64 = 256 * 256 * 16;
pub const COMPOSITE_RESERVE: u64 = 80 * 1024 * 1024;
pub struct Compositor {
    pipeline: RenderPipeline,
    layout: BindGroupLayout,
    uniform: Buffer,
    source: Texture,
    mask: Texture,
    pool: Vec<Texture>,
}
fn texture(device: &Device, format: TextureFormat) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("Bounded composite tile"),
        size: Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING
            | TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
impl Compositor {
    pub fn new(device: &Device) -> Self {
        let mut entries = Vec::new();
        for binding in 0..3 {
            entries.push(BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        entries.push(BindGroupLayoutEntry {
            binding: 3,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Composite inputs"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Composite layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sRGB composite"),
            source: ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Portable float composite"),
            layout: Some(&pipeline_layout),
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
                    format: TextureFormat::Rgba32Float,
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
        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("Composite parameters"),
            size: 32,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            uniform,
            source: texture(device, TextureFormat::Rgba8Unorm),
            mask: texture(device, TextureFormat::R8Unorm),
            pool: Vec::new(),
        }
    }
    pub fn bytes(&self) -> u64 {
        (self.pool.len() as u64 + 1) * FLOAT_BYTES + 262144 + 65536 + 32
    }
    fn clear(&self, device: &Device, queue: &Queue, index: usize) {
        let view = self.pool[index].create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Clear composite"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        queue.submit([encoder.finish()]);
    }
    fn upload(&self, queue: &Queue, texture: &Texture, bytes: &[u8], stride: u32) {
        queue.write_texture(
            TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(TILE_SIZE),
            },
            texture.size(),
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn pass(
        &self,
        device: &Device,
        queue: &Queue,
        backdrop: usize,
        source: &Texture,
        destination: usize,
        properties: &LayerProperties,
        operation: u32,
        premultiplied: bool,
        masked: bool,
    ) {
        let mut params = Vec::with_capacity(32);
        for value in [
            properties.blend as u32,
            operation,
            u32::from(premultiplied),
            u32::from(masked),
        ] {
            params.extend(value.to_le_bytes());
        }
        params.extend((properties.opacity as f32).to_le_bytes());
        params.extend((properties.fill as f32).to_le_bytes());
        params.extend([0; 8]);
        queue.write_buffer(&self.uniform, 0, &params);
        let views = [
            self.pool[backdrop].create_view(&TextureViewDescriptor::default()),
            source.create_view(&TextureViewDescriptor::default()),
            self.mask.create_view(&TextureViewDescriptor::default()),
        ];
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Composite pass"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&views[0]),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&views[1]),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&views[2]),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let view = self.pool[destination].create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Blend sRGB tile"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
    pub fn compose(
        &mut self,
        device: &Device,
        queue: &Queue,
        plan: &CompositePlan,
    ) -> Result<js_sys::Promise, String> {
        let count = plan.stack_depth + 3;
        if (count as u64 + 1) * FLOAT_BYTES + 327712 > COMPOSITE_RESERVE {
            return Err("Composite stack exceeds the GPU scratch budget".into());
        }
        while self.pool.len() < count {
            self.pool.push(texture(device, TextureFormat::Rgba32Float));
        }
        let mut stack: Vec<(usize, bool)> = Vec::new();
        let mut current = 0;
        self.clear(device, queue, current);
        for step in &plan.steps {
            let free = |extra: usize| {
                (0..self.pool.len())
                    .find(|i| *i != current && *i != extra && !stack.iter().any(|s| s.0 == *i))
                    .expect("reserved composite pool")
            };
            match step {
                CompositeStep::Begin { pass_through } => {
                    let next = free(usize::MAX);
                    if *pass_through {
                        let mut encoder =
                            device.create_command_encoder(&CommandEncoderDescriptor::default());
                        encoder.copy_texture_to_texture(
                            self.pool[current].as_image_copy(),
                            self.pool[next].as_image_copy(),
                            self.pool[current].size(),
                        );
                        queue.submit([encoder.finish()]);
                    } else {
                        self.clear(device, queue, next);
                    }
                    stack.push((current, *pass_through));
                    current = next;
                }
                CompositeStep::Raster {
                    source,
                    mask,
                    properties,
                    clipped,
                } => {
                    let Some(source) = source else {
                        continue;
                    };
                    let source = source.pin()?;
                    self.upload(
                        queue,
                        &self.source,
                        &source.pixels.iter().flat_map(|p| p.0).collect::<Vec<_>>(),
                        TILE_SIZE * 4,
                    );
                    if let Some(mask) = mask {
                        self.upload(
                            queue,
                            &self.mask,
                            &mask.pin()?.pixels.iter().map(|p| p.0).collect::<Vec<_>>(),
                            TILE_SIZE,
                        );
                    }
                    let next = free(usize::MAX);
                    self.pass(
                        device,
                        queue,
                        current,
                        &self.source,
                        next,
                        properties,
                        u32::from(*clipped),
                        false,
                        mask.is_some(),
                    );
                    current = next;
                }
                CompositeStep::End {
                    mask,
                    properties,
                    clipped,
                } => {
                    let (backdrop, pass_through) =
                        *stack.last().ok_or("Unbalanced composite stack")?;
                    if let Some(mask) = mask {
                        self.upload(
                            queue,
                            &self.mask,
                            &mask.pin()?.pixels.iter().map(|p| p.0).collect::<Vec<_>>(),
                            TILE_SIZE,
                        );
                    }
                    let next = free(backdrop);
                    self.pass(
                        device,
                        queue,
                        backdrop,
                        &self.pool[current],
                        next,
                        properties,
                        if pass_through { 2 } else { u32::from(*clipped) },
                        true,
                        mask.is_some(),
                    );
                    stack.pop();
                    current = next;
                }
            }
        }
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("One composite readback"),
            size: FLOAT_BYTES,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            self.pool[current].as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TILE_SIZE * 16),
                    rows_per_image: Some(TILE_SIZE),
                },
            },
            self.pool[current].size(),
        );
        queue.submit([encoder.finish()]);
        let complete = std::rc::Rc::new(std::cell::Cell::new(false));
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let mapped = buffer.clone();
            let complete = complete.clone();
            buffer.map_async(MapMode::Read, .., move |result| match result {
                Ok(()) => {
                    complete.set(true);
                    let bytes = match mapped.get_mapped_range(..) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            let _ = reject.call1(
                                &JsValue::NULL,
                                &JsValue::from_str(&format!("Composite mapping failed: {error}")),
                            );
                            return;
                        }
                    };
                    let output = js_sys::Uint8Array::from(&*bytes);
                    drop(bytes);
                    mapped.unmap();
                    let _ = resolve.call1(&JsValue::NULL, &output);
                }
                Err(error) => {
                    complete.set(true);
                    let _ = reject.call1(
                        &JsValue::NULL,
                        &JsValue::from_str(&format!("Composite readback failed: {error}")),
                    );
                }
            });
        });
        // WebGPU progresses mappings itself. The WebGL2 core backend requires
        // polling after its fence becomes ready, without blocking the worker.
        let device = device.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while !complete.get() {
                let _ = device.poll(PollType::Poll);
                if complete.get() {
                    break;
                }
                let delay = js_sys::Promise::new(&mut |resolve, _| {
                    if let Ok(timer) =
                        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("setTimeout"))
                    {
                        let timer = wasm_bindgen::JsCast::unchecked_into::<js_sys::Function>(timer);
                        let _ = timer.call2(&JsValue::NULL, &resolve, &JsValue::from_f64(4.0));
                    }
                });
                let _ = wasm_bindgen_futures::JsFuture::from(delay).await;
            }
        });
        Ok(promise)
    }
}
