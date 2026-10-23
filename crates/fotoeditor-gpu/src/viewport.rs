use fotoeditor_core::{
    display_region, display_tile, DirtyRegions, Document, Rgba8, TileCoord, TileRect, TileSurface,
    View, GPU_BUDGET, TILE_SIZE,
};
use std::collections::{HashMap, HashSet};
use wgpu::*;
const ATLAS: u32 = 2048;
const PAGE_BYTES: u64 = ATLAS as u64 * ATLAS as u64 * 4;
fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
struct Page {
    texture: Texture,
    nearest: BindGroup,
    linear: BindGroup,
    vertices: Buffer,
    slots: Vec<Option<u64>>,
    instances: u32,
    lod: u32,
    overview: bool,
    last_used: u64,
}
pub struct ViewportRenderer {
    background: RenderPipeline,
    grid: RenderPipeline,
    tiles: RenderPipeline,
    uniform: Buffer,
    binding: BindGroup,
    atlas_layout: BindGroupLayout,
    pages: Vec<Page>,
    locations: HashMap<(u64, u32), (usize, usize)>,
    coordinates: HashMap<u64, TileCoord>,
    gutter_dirty: HashSet<u64>,
    pending_regions: HashMap<u64, TileRect>,
    tick: u64,
    pub refining: u32,
    pub uploaded: u64,
    pub visible: u32,
}
impl ViewportRenderer {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Viewport"),
            source: ShaderSource::Wgsl(include_str!("viewport.wgsl").into()),
        });
        let view_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("View"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let atlas_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Atlas"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("View transform"),
            size: 48,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let binding = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &view_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let background_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&view_layout)],
            immediate_size: 0,
        });
        let tile_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&view_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str,
                        fragment: &str,
                        layout: &PipelineLayout,
                        buffers: &[Option<VertexBufferLayout<'_>>],
                        blend: Option<BlendState>| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
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
        let background = pipeline("fullscreen", "background", &background_layout, &[], None);
        let grid = pipeline(
            "fullscreen",
            "grid",
            &background_layout,
            &[],
            Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let tiles = pipeline(
            "tile_vertex",
            "tile_fragment",
            &tile_layout,
            &[Some(VertexBufferLayout {
                array_stride: 32,
                step_mode: VertexStepMode::Instance,
                attributes: &vertex_attr_array![0=>Float32x4,1=>Float32x4],
            })],
            Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        Self {
            background,
            grid,
            tiles,
            uniform,
            binding,
            atlas_layout,
            pages: vec![],
            locations: HashMap::new(),
            coordinates: HashMap::new(),
            gutter_dirty: HashSet::new(),
            pending_regions: HashMap::new(),
            tick: 0,
            refining: 0,
            uploaded: 0,
            visible: 0,
        }
    }
    pub fn clear(&mut self) {
        self.pages.clear();
        self.locations.clear();
        self.coordinates.clear();
        self.gutter_dirty.clear();
        self.pending_regions.clear();
    }
    pub fn invalidate(&mut self, dirty: &DirtyRegions, surface: &TileSurface<Rgba8>) {
        if dirty.is_empty() {
            return;
        }
        // Neighbors' gutters also depend on a changed source tile.
        let stale: Vec<_> = self
            .coordinates
            .iter()
            .filter(|(_, c)| {
                dirty
                    .keys()
                    .any(|d| c.0.abs_diff(d.0) <= 1 && c.1.abs_diff(d.1) <= 1)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            let coord = self.coordinates[&id];
            let neighbor_only = !dirty.contains_key(&coord);
            let keep_base = !neighbor_only
                && surface.tiles().get(&coord).is_some_and(|t| t.id() == id)
                && self.locations.contains_key(&(id, 0));
            if keep_base {
                let rect = dirty[&coord];
                self.pending_regions
                    .entry(id)
                    .and_modify(|r| r.union(rect))
                    .or_insert(rect);
                self.gutter_dirty.insert(id);
            }
            for lod in 0..=8 {
                if lod == 0 && (neighbor_only || keep_base) {
                    self.gutter_dirty.insert(id);
                    continue;
                }
                if let Some((page, slot)) = self.locations.remove(&(id, lod)) {
                    self.pages[page].slots[slot] = None;
                }
            }
            if !neighbor_only && !keep_base {
                self.coordinates.remove(&id);
                self.gutter_dirty.remove(&id);
                self.pending_regions.remove(&id);
            }
        }
    }
    pub fn bytes(&self) -> u64 {
        self.pages
            .iter()
            .map(|p| PAGE_BYTES + p.vertices.size())
            .sum::<u64>()
            + 48
    }
    fn page(&self, device: &Device, slots: usize, lod: u32, overview: bool) -> Page {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Evictable display atlas"),
            size: Extent3d {
                width: ATLAS,
                height: ATLAS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let binding = |filter| {
            let sampler = device.create_sampler(&SamplerDescriptor {
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            });
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("Atlas samples"),
                layout: &self.atlas_layout,
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
            })
        };
        let nearest = binding(FilterMode::Nearest);
        let linear = binding(FilterMode::Linear);
        let vertices = device.create_buffer(&BufferDescriptor {
            label: Some("Visible tile instances"),
            size: slots as u64 * 32,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Page {
            texture,
            nearest,
            linear,
            vertices,
            slots: vec![None; slots],
            instances: 0,
            lod,
            overview,
            last_used: self.tick,
        }
    }
    fn cached(&self, id: u64, lod: u32) -> Option<(usize, usize)> {
        if let Some(location) = self.locations.get(&(id, lod)) {
            return Some(*location);
        }
        for distance in 1..=8 {
            if lod + distance <= 8 {
                if let Some(location) = self.locations.get(&(id, lod + distance)) {
                    return Some(*location);
                }
            }
            if let Some(finer) = lod.checked_sub(distance) {
                if let Some(location) = self.locations.get(&(id, finer)) {
                    return Some(*location);
                }
            }
        }
        None
    }
    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        doc: Option<&Document>,
        view: &View,
        dpr: f64,
    ) -> Result<(), String> {
        let active = doc.is_some();
        queue.write_buffer(
            &self.uniform,
            0,
            &bytes(&[
                (view.width * dpr) as f32,
                (view.height * dpr) as f32,
                view.document_width as f32,
                view.document_height as f32,
                (view.pan_x * dpr) as f32,
                (view.pan_y * dpr) as f32,
                (view.zoom * dpr) as f32,
                view.rotation as f32,
                if view.flipped { -1.0 } else { 1.0 },
                if view.zoom >= 8.0 { 1.0 } else { 0.0 },
                if active { 1.0 } else { 0.0 },
                0.0,
            ]),
        );
        for page in &mut self.pages {
            page.instances = 0;
        }
        self.tick += 1;
        self.refining = 0;
        let Some(doc) = doc else {
            return Ok(());
        };
        let lod = if view.zoom >= 1.0 {
            0
        } else {
            (1.0 / (view.zoom * dpr)).log2().floor().clamp(0.0, 8.0) as u32
        };
        let side = TILE_SIZE >> lod;
        let padded = side + 2;
        let cells = ATLAS / padded;
        let slots = (cells * cells).min(4096) as usize;
        let visible: Vec<_> = doc
            .raster
            .tiles()
            .iter()
            .filter(|(c, _)| view.visible(c.0 * TILE_SIZE, c.1 * TILE_SIZE, TILE_SIZE, TILE_SIZE))
            .collect();
        self.visible = visible.len() as u32;
        let ids: HashSet<_> = visible.iter().map(|(_, t)| t.id()).collect();
        let bulk = self.locations.is_empty();
        let start = js_sys::Date::now();
        for (coord, tile) in &visible {
            if lod == 0 && view.zoom >= 1.0 {
                if let (Some(rect), Some(&(page, slot))) = (
                    self.pending_regions.get(&tile.id()).copied(),
                    self.locations.get(&(tile.id(), 0)),
                ) {
                    let pixels = display_region(&doc.raster, **coord, rect);
                    let width = rect.x1 - rect.x0;
                    let height = rect.y1 - rect.y0;
                    queue.write_texture(
                        TexelCopyTextureInfo {
                            texture: &self.pages[page].texture,
                            mip_level: 0,
                            origin: Origin3d {
                                x: slot as u32 % cells * padded + 1 + rect.x0,
                                y: slot as u32 / cells * padded + 1 + rect.y0,
                                z: 0,
                            },
                            aspect: TextureAspect::All,
                        },
                        &pixels,
                        TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * 4),
                            rows_per_image: Some(height),
                        },
                        Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                    self.uploaded += pixels.len() as u64;
                    self.pending_regions.remove(&tile.id());
                    continue;
                }
            }
            let refresh_gutter =
                lod == 0 && view.zoom < 1.0 && self.gutter_dirty.contains(&tile.id());
            if self.locations.contains_key(&(tile.id(), lod)) && !refresh_gutter {
                continue;
            }
            // Keep a cached overview visible while sharper mips arrive. Long
            // navigation work must not monopolize the worker's next frame.
            if !bulk && js_sys::Date::now() - start >= 3.0 && self.cached(tile.id(), lod).is_some()
            {
                continue;
            }
            let mut available = self.locations.get(&(tile.id(), lod)).copied();
            for (p, page) in self.pages.iter().enumerate() {
                if available.is_some() {
                    break;
                }
                if page.lod != lod {
                    continue;
                }
                if let Some(slot) = page
                    .slots
                    .iter()
                    .position(|id| id.is_none_or(|id| !ids.contains(&id)))
                {
                    available = Some((p, slot));
                    break;
                }
            }
            let (p, slot) = match available {
                Some(slot) => slot,
                None => {
                    if self.bytes() + PAGE_BYTES + slots as u64 * 32 > GPU_BUDGET {
                        let victim = self
                            .pages
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| !p.overview && p.lod != lod)
                            .min_by_key(|(_, p)| p.last_used)
                            .map(|(i, _)| i);
                        let Some(victim) = victim else {
                            return Err("Viewport exceeds the 128 MiB GPU tile budget; use a smaller window".into());
                        };
                        let old = &self.pages[victim];
                        for id in old.slots.iter().flatten() {
                            self.locations.remove(&(*id, old.lod));
                        }
                        // Drop the old page before allocating its replacement.
                        let old_page = self.pages.remove(victim);
                        drop(old_page);
                        for location in self.locations.values_mut() {
                            if location.0 > victim {
                                location.0 -= 1;
                            }
                        }
                    }
                    self.pages.push(self.page(device, slots, lod, bulk));
                    (self.pages.len() - 1, 0)
                }
            };
            let page = &mut self.pages[p];
            if let Some(old) = page.slots[slot] {
                self.locations.remove(&(old, lod));
            }
            let pixels = display_tile(&doc.raster, **coord, lod);
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &page.texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: slot as u32 % cells * padded,
                        y: slot as u32 / cells * padded,
                        z: 0,
                    },
                    aspect: TextureAspect::All,
                },
                &pixels,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded * 4),
                    rows_per_image: Some(padded),
                },
                Extent3d {
                    width: padded,
                    height: padded,
                    depth_or_array_layers: 1,
                },
            );
            self.uploaded += pixels.len() as u64;
            page.slots[slot] = Some(tile.id());
            self.locations.insert((tile.id(), lod), (p, slot));
            self.coordinates.insert(tile.id(), **coord);
            if lod == 0 {
                self.gutter_dirty.remove(&tile.id());
                self.pending_regions.remove(&tile.id());
            }
        }
        let mut vertices = vec![Vec::<f32>::new(); self.pages.len()];
        for (coord, tile) in visible {
            let (p, slot) = self
                .cached(tile.id(), lod)
                .ok_or("Visible tile has no display backing")?;
            let page = &mut self.pages[p];
            page.last_used = self.tick;
            if page.lod != lod
                || (page.lod == 0
                    && (self.pending_regions.contains_key(&tile.id())
                        || (view.zoom < 1.0 && self.gutter_dirty.contains(&tile.id()))))
            {
                self.refining += 1;
            }
            let side = TILE_SIZE >> page.lod;
            let padded = side + 2;
            let cells = ATLAS / padded;
            vertices[p].extend_from_slice(&[
                (coord.0 * TILE_SIZE) as f32,
                (coord.1 * TILE_SIZE) as f32,
                TILE_SIZE as f32,
                TILE_SIZE as f32,
                (slot as u32 % cells * padded + 1) as f32 / ATLAS as f32,
                (slot as u32 / cells * padded + 1) as f32 / ATLAS as f32,
                side as f32 / ATLAS as f32,
                side as f32 / ATLAS as f32,
            ]);
        }
        for (page, vertices) in self.pages.iter_mut().zip(vertices) {
            page.instances = (vertices.len() / 8) as u32;
            if !vertices.is_empty() {
                queue.write_buffer(&page.vertices, 0, &bytes(&vertices));
            }
        }
        Ok(())
    }
    pub fn draw(&self, pass: &mut RenderPass<'_>, view: &View) {
        pass.set_pipeline(&self.background);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.tiles);
        for page in &self.pages {
            if page.instances > 0 {
                pass.set_bind_group(
                    1,
                    if view.zoom >= 1.0 {
                        &page.nearest
                    } else {
                        &page.linear
                    },
                    &[],
                );
                pass.set_vertex_buffer(0, page.vertices.slice(..));
                pass.draw(0..6, 0..page.instances);
            }
        }
        pass.set_pipeline(&self.grid);
        pass.draw(0..3, 0..1);
    }
}
