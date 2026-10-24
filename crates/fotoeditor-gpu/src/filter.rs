//! Bounded GPU convolution tile. Rust prepares halo samples; GPU runs both
//! separable passes and selection clipping, with no JavaScript pixel readback.
use wgpu::*;
pub struct FilterTileInput {
    pub source: Vec<u8>,
    pub coverage: Vec<u8>,
    pub params: Vec<u8>,
    pub target: u32,
}
pub struct FilterTile {
    source: Texture,
    horizontal: Texture,
    output: Texture,
    coverage: Texture,
    uniform: Buffer,
    layout: BindGroupLayout,
    first: RenderPipeline,
    finish: RenderPipeline,
}
fn texture(device: &Device, size: u32, format: TextureFormat) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("Filter scratch"),
        size: Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
impl FilterTile {
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
            label: Some("Filter inputs"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Spatial filter"),
            source: ShaderSource::Wgsl(include_str!("filter.wgsl").into()),
        });
        let pipeline = |entry: &str, format| {
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
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let first = pipeline("first", TextureFormat::Rgba32Float);
        let finish = pipeline("finish", TextureFormat::Rgba8Unorm);
        Self {
            source: texture(device, 384, TextureFormat::Rgba8Unorm),
            horizontal: texture(device, 384, TextureFormat::Rgba32Float),
            output: texture(device, 256, TextureFormat::Rgba8Unorm),
            coverage: texture(device, 256, TextureFormat::R8Unorm),
            uniform: device.create_buffer(&BufferDescriptor {
                label: Some("Filter settings"),
                size: 2128,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            layout,
            first,
            finish,
        }
    }
    pub fn bytes(&self) -> u64 {
        384 * 384 * 20 + 256 * 256 * 5 + 2128
    }
    pub fn render(
        &self,
        device: &Device,
        queue: &Queue,
        input: &FilterTileInput,
    ) -> Result<Texture, String> {
        if input.source.len() != 384 * 384 * 4
            || input.coverage.len() != 256 * 256
            || input.params.len() != 2128
        {
            return Err("Invalid GPU filter tile buffers".into());
        }
        for (texture, bytes, stride) in [
            (&self.source, &input.source, 384 * 4),
            (&self.coverage, &input.coverage, 256),
        ] {
            queue.write_texture(
                texture.as_image_copy(),
                bytes,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(texture.height()),
                },
                texture.size(),
            );
        }
        queue.write_buffer(&self.uniform, 0, &input.params);
        for (pipeline, destination, intermediate) in [
            (&self.first, &self.horizontal, &self.source),
            (&self.finish, &self.output, &self.horizontal),
        ] {
            let views = [
                self.source.create_view(&Default::default()),
                intermediate.create_view(&Default::default()),
                self.coverage.create_view(&Default::default()),
            ];
            let mut entries: Vec<_> = views
                .iter()
                .enumerate()
                .map(|(i, v)| BindGroupEntry {
                    binding: i as u32,
                    resource: BindingResource::TextureView(v),
                })
                .collect();
            entries.push(BindGroupEntry {
                binding: 3,
                resource: self.uniform.as_entire_binding(),
            });
            let bind = device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &entries,
            });
            let view = destination.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("Filter pass"),
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
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit([encoder.finish()]);
        }
        Ok(self.output.clone())
    }
}
