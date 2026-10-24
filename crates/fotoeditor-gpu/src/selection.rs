use fotoeditor_core::{selection_display, SelectionDisplaySource, View};
use wgpu::*;
pub struct SelectionRenderer {
    pipeline: RenderPipeline,
    layout: BindGroupLayout,
    uniform: Buffer,
    texture: Option<Texture>,
    binding: Option<BindGroup>,
    size: (u32, u32),
    key: Vec<u64>,
    pixels: Vec<u8>,
    quick: bool,
}
impl SelectionRenderer {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Selection overlay"),
            source: ShaderSource::Wgsl(include_str!("selection.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Selection overlay"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Mask-edge marching ants and Quick Mask"),
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
                    format,
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
            label: Some("Selection settings"),
            size: 32,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            uniform,
            texture: None,
            binding: None,
            size: (0, 0),
            key: Vec::new(),
            pixels: Vec::new(),
            quick: false,
        }
    }
    pub fn clear(&mut self) {
        self.texture = None;
        self.binding = None;
        self.size = (0, 0);
        self.key.clear();
        self.pixels = Vec::new();
    }
    pub fn gpu_bytes(&self) -> u64 {
        if self.texture.is_some() {
            u64::from(self.size.0) * u64::from(self.size.1) * 2 + 32
        } else {
            32
        }
    }
    pub fn cpu_bytes(&self) -> u64 {
        self.pixels.capacity() as u64
    }
    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        source: SelectionDisplaySource<'_>,
        view: &View,
        screen: (u32, u32),
        state: (u32, u32, bool),
    ) -> Result<(), String> {
        let scale = (2048.0 / f64::from(screen.0.max(screen.1).max(1))).min(1.0);
        let w = (f64::from(screen.0) * scale).ceil().max(1.0) as u32;
        let h = (f64::from(screen.1) * scale).ceil().max(1.0) as u32;
        let key = vec![
            view.width.to_bits(),
            view.height.to_bits(),
            view.zoom.to_bits(),
            view.pan_x.to_bits(),
            view.pan_y.to_bits(),
            view.rotation.to_bits(),
            u64::from(view.flipped),
            u64::from(state.0),
            u64::from(state.1),
            u64::from(w),
            u64::from(h),
        ];
        self.quick = state.2;
        if self.key == key {
            return Ok(());
        }
        self.pixels = selection_display(source, view, w, h)?;
        if self.size != (w, h) || self.texture.is_none() {
            self.texture = None;
            self.binding = None;
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("Bounded selection display mirror"),
                size: Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rg8Unorm,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let binding = device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(
                            &texture.create_view(&TextureViewDescriptor::default()),
                        ),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            });
            self.texture = Some(texture);
            self.binding = Some(binding);
            self.size = (w, h);
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: self.texture.as_ref().unwrap(),
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &self.pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 2),
                rows_per_image: Some(h),
            },
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.key = key;
        Ok(())
    }
    pub fn update_time(&self, queue: &Queue, screen: (u32, u32)) {
        if self.binding.is_none() {
            return;
        }
        let time = (js_sys::Date::now() % 1000.0) as f32 / 1000.0;
        let values = [
            self.size.0 as f32,
            self.size.1 as f32,
            screen.0 as f32,
            screen.1 as f32,
            f32::from(self.quick),
            time,
            0.0,
            0.0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_le_bytes).collect();
        queue.write_buffer(&self.uniform, 0, &bytes);
    }
    pub fn draw(&self, pass: &mut RenderPass<'_>) {
        if let Some(binding) = &self.binding {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, binding, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
