//! GPU-only tile conversion/presentation. Preview never reads image pixels back.
use wgpu::*;
pub struct PreviewFrame {
    texture: Texture,
    layout: BindGroupLayout,
    uniform: Buffer,
    binding: BindGroup,
    convert: RenderPipeline,
    display: RenderPipeline,
    pub width: u32,
    pub height: u32,
}
impl PreviewFrame {
    pub fn new(
        device: &Device,
        format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 2048 || height > 2048 {
            return Err("Preview frame exceeds its side bound".into());
        }
        let padded_width = width.div_ceil(256) * 256;
        let padded_height = height.div_ceil(256) * 256;
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Visible adjustment preview"),
            size: Extent3d {
                width: padded_width,
                height: padded_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Preview blit inputs"),
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
        let uniform = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Preview scale"),
            contents: &[
                width as f32 / padded_width as f32,
                height as f32 / padded_height as f32,
                0.0,
                0.0,
            ]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
            usage: BufferUsages::UNIFORM,
        });
        let bind = |source: &Texture| {
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("Preview blit"),
                layout: &layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(
                            &source.create_view(&TextureViewDescriptor::default()),
                        ),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        };
        let binding = bind(&texture);
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Preview presentation"),
            source: ShaderSource::Wgsl(include_str!("preview.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, format, blend| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(ColorTargetState {
                        format,
                        blend,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let convert = pipeline("tile", TextureFormat::Rgba8Unorm, None);
        let display = pipeline(
            "display",
            format,
            Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        Ok(Self {
            texture,
            layout,
            uniform,
            binding,
            convert,
            display,
            width,
            height,
        })
    }
    pub fn bytes(&self) -> u64 {
        u64::from(self.texture.width()) * u64::from(self.texture.height()) * 4 + 16
    }
    pub fn accept(
        &self,
        device: &Device,
        queue: &Queue,
        coord: fotoeditor_core::TileCoord,
        source: &Texture,
    ) {
        let binding = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Preview tile"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(
                        &source.create_view(&TextureViewDescriptor::default()),
                    ),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Write preview tile"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.texture.create_view(&TextureViewDescriptor::default()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.convert);
            pass.set_bind_group(0, &binding, &[]);
            pass.set_viewport(
                (coord.0 * 256) as f32,
                (coord.1 * 256) as f32,
                256.0,
                256.0,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(coord.0 * 256, coord.1 * 256, 256, 256);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
    pub fn draw(&self, pass: &mut RenderPass<'_>) {
        pass.set_pipeline(&self.display);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.draw(0..3, 0..1);
    }
}
use wgpu::util::DeviceExt;
