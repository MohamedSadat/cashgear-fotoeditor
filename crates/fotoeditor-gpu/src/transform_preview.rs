//! Worker-owned immutable preview source and portable homography fragment pass.
use wgpu::*;
pub struct TransformPreview {
    source: Texture,
    auxiliary: Texture,
    output: Texture,
    uniform: Buffer,
    binding: BindGroup,
    pipeline: RenderPipeline,
}
impl TransformPreview {
    pub fn new(device: &Device, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 1536 || height > 1536 {
            return Err("Transform source preview exceeds 1536 pixels per side".into());
        }
        let texture = |label, width, height, format, usage| {
            device.create_texture(&TextureDescriptor {
                label: Some(label),
                size: Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let source = texture(
            "Immutable transform preview",
            width,
            height,
            TextureFormat::Rgba8Unorm,
            TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
        );
        let auxiliary = texture(
            "Transform selection and mask",
            width,
            height,
            TextureFormat::Rgba8Unorm,
            TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
        );
        let output = texture(
            "Transformed straight tile",
            256,
            256,
            TextureFormat::Rgba32Float,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        );
        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("Transform homography"),
            size: 144,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut entries = vec![];
        for binding in 0..2 {
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
            binding: 2,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Transform preview inputs"),
            entries: &entries,
        });
        let binding = device.create_bind_group(&BindGroupDescriptor {
            label: None,
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
                    resource: BindingResource::TextureView(
                        &auxiliary.create_view(&TextureViewDescriptor::default()),
                    ),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Transform sampling"),
            source: ShaderSource::Wgsl(include_str!("transform_preview.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Transform preview"),
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
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            source,
            auxiliary,
            output,
            uniform,
            binding,
            pipeline,
        })
    }
    pub fn bytes(&self) -> u64 {
        u64::from(self.source.width()) * u64::from(self.source.height()) * 8 + 256 * 256 * 16 + 144
    }
    pub fn upload(
        &self,
        queue: &Queue,
        coord: fotoeditor_core::TileCoord,
        source: &[u8],
        auxiliary: &[u8],
    ) -> Result<(), String> {
        if source.len() != 256 * 256 * 4
            || auxiliary.len() != source.len()
            || coord.0 * 256 >= self.source.width()
            || coord.1 * 256 >= self.source.height()
        {
            return Err("Invalid transform cache tile".into());
        }
        for (texture, bytes) in [(&self.source, source), (&self.auxiliary, auxiliary)] {
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: coord.0 * 256,
                        y: coord.1 * 256,
                        z: 0,
                    },
                    aspect: TextureAspect::All,
                },
                bytes,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024),
                    rows_per_image: Some(256),
                },
                Extent3d {
                    width: 256.min(texture.width() - coord.0 * 256),
                    height: 256.min(texture.height() - coord.1 * 256),
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(())
    }
    pub fn render(&self, device: &Device, queue: &Queue, params: &[u8]) -> Result<Texture, String> {
        if params.len() != 144 {
            return Err("Invalid transform uniforms".into());
        }
        queue.write_buffer(&self.uniform, 0, params);
        let view = self.output.create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Resample transform preview"),
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
            pass.set_bind_group(0, &self.binding, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        Ok(self.output.clone())
    }
}
