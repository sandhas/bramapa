//! Instanced tilemap renderer. Each chunk becomes up to three GPU buffers (terrain,
//! overlay, objects); the shader does placement, animation and depth.

use std::collections::HashMap;

use bramapa_core::tileset::{TileClass, TileDef};
use bramapa_core::{Chunk, Level, Tileset};
use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use crate::atlas::AtlasTexture;
use crate::camera::Camera;
use crate::iso::IsoGeometry;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    cell: [u16; 2],
    atlas: [u16; 2],
    anim: [u16; 2],
    misc: [u16; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    time: f32,
    atlas_cols: f32,
    atlas_rows: f32,
    depth_norm: f32,
    tile_w: f32,
    tile_h: f32,
    cell_w: f32,
    cell_h: f32,
    hover_entity: u32,
    selected_entity: u32,
    _pad: [u32; 2],
}

/// Entity indices to emphasise this frame (0 = none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Highlight {
    pub hover: u16,
    pub selected: u16,
}

struct LayerBuffer {
    buffer: wgpu::Buffer,
    count: u32,
}

struct ChunkMesh {
    bounds: (Vec2, Vec2),
    layers: [Option<LayerBuffer>; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub chunks_total: u32,
    pub chunks_drawn: u32,
    pub draw_calls: u32,
    pub instances_drawn: u32,
    pub instances_total: u32,
    pub unknown_tile_ids: u32,
}

pub struct TilemapRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    pub atlas_layout: wgpu::BindGroupLayout,
    geometry: IsoGeometry,
    atlas_cells: (f32, f32),
    chunks: HashMap<(i32, i32), ChunkMesh>,
    depth_norm: f32,
    pub stats: RenderStats,
}

impl TilemapRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tilemap"),
            source: wgpu::ShaderSource::Wgsl(include_str!("tilemap.wgsl").into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tilemap uniforms bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let atlas_layout = AtlasTexture::bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tilemap pl"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint16x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint16x2,
                    offset: 4,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint16x2,
                    offset: 8,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint16x2,
                    offset: 12,
                    shader_location: 3,
                },
            ],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tilemap pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(instance_layout)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tilemap uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tilemap uniforms bg"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            uniform_buf,
            uniform_bg,
            atlas_layout,
            geometry: IsoGeometry::DEFAULT,
            atlas_cells: (1.0, 1.0),
            chunks: HashMap::new(),
            depth_norm: 1.0,
            stats: RenderStats::default(),
        }
    }

    pub fn geometry(&self) -> IsoGeometry {
        self.geometry
    }

    /// Upload every chunk of `level` using `tileset` for atlas placement. Replaces any
    /// previously uploaded level.
    pub fn upload_level(
        &mut self,
        device: &wgpu::Device,
        level: &Level,
        tileset: &Tileset,
        atlas: &AtlasTexture,
    ) {
        let info = &tileset.tileset;
        self.geometry = IsoGeometry {
            tile_w: info.tile_width as f32,
            tile_h: info.tile_height as f32,
            cell_w: info.cell_width as f32,
            cell_h: info.cell_height as f32,
        };
        self.atlas_cells = (
            atlas.width as f32 / info.cell_width as f32,
            atlas.height as f32 / info.cell_height as f32,
        );
        let g = &level.manifest.grid;
        self.depth_norm = 1.0 / (g.width + g.height + 2) as f32;
        let table = tileset.lookup_table();
        self.chunks.clear();
        self.stats = RenderStats {
            chunks_total: level.chunks.len() as u32,
            ..Default::default()
        };
        for chunk in level.chunks.values() {
            let mesh = build_chunk(device, chunk, &table, &self.geometry, &mut self.stats);
            self.chunks.insert((chunk.cx, chunk.cy), mesh);
        }
    }

    /// Record draw calls for everything visible from `camera`. The pass must already have
    /// a colour attachment of the pipeline's format and a depth attachment.
    pub fn render(
        &mut self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        camera: &Camera,
        atlas: &AtlasTexture,
        time: f32,
        highlight: Highlight,
    ) {
        let uniforms = Uniforms {
            view_proj: camera.view_proj().to_cols_array_2d(),
            time,
            atlas_cols: self.atlas_cells.0,
            atlas_rows: self.atlas_cells.1,
            depth_norm: self.depth_norm,
            tile_w: self.geometry.tile_w,
            tile_h: self.geometry.tile_h,
            cell_w: self.geometry.cell_w,
            cell_h: self.geometry.cell_h,
            hover_entity: highlight.hover as u32,
            selected_entity: highlight.selected as u32,
            _pad: [0; 2],
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let (vmin, vmax) = camera.world_rect();
        let mut drawn = 0;
        let mut draw_calls = 0;
        let mut instances = 0;
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bg, &[]);
        pass.set_bind_group(1, &atlas.bind_group, &[]);
        for mesh in self.chunks.values() {
            let (bmin, bmax) = mesh.bounds;
            if bmax.x < vmin.x || bmin.x > vmax.x || bmax.y < vmin.y || bmin.y > vmax.y {
                continue;
            }
            drawn += 1;
            for layer in mesh.layers.iter().flatten() {
                pass.set_vertex_buffer(0, layer.buffer.slice(..));
                pass.draw(0..6, 0..layer.count);
                draw_calls += 1;
                instances += layer.count;
            }
        }
        self.stats.chunks_drawn = drawn;
        self.stats.draw_calls = draw_calls;
        self.stats.instances_drawn = instances;
    }
}

fn build_chunk(
    device: &wgpu::Device,
    chunk: &Chunk,
    table: &[Option<&TileDef>],
    g: &IsoGeometry,
    stats: &mut RenderStats,
) -> ChunkMesh {
    let size = chunk.size as u32;
    let base_x = chunk.cx as u32 * size;
    let base_y = chunk.cy as u32 * size;
    let lookup = |id: u16| -> Option<&TileDef> { table.get(id as usize).copied().flatten() };

    let mut layers: [Vec<Instance>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (layer_idx, cells) in [(0u16, &chunk.terrain), (1u16, &chunk.overlay)] {
        for (i, &id) in cells.iter().enumerate() {
            if id == 0 {
                continue;
            }
            let Some(t) = lookup(id) else {
                stats.unknown_tile_ids += 1;
                continue;
            };
            let x = base_x + (i as u32 % size);
            let y = base_y + (i as u32 / size);
            let entity = chunk.entity.as_ref().map_or(0, |e| e[i]);
            layers[layer_idx as usize].push(Instance {
                cell: [x as u16, y as u16],
                atlas: [t.col, t.row],
                anim: [t.frames, t.fps],
                misc: [layer_idx << 8, entity],
            });
        }
    }
    for o in &chunk.objects {
        let Some(t) = lookup(o.tile_id) else {
            stats.unknown_tile_ids += 1;
            continue;
        };
        if t.class != TileClass::Object {
            log::warn!(
                "object layer references non-object tile {} ({})",
                o.tile_id,
                t.name
            );
        }
        layers[2].push(Instance {
            cell: [
                (base_x + o.cell_x as u32) as u16,
                (base_y + o.cell_y as u32) as u16,
            ],
            atlas: [t.col, t.row],
            anim: [t.frames, t.fps],
            misc: [
                (o.flags & 0xff) | (2 << 8),
                chunk.entity_at(o.cell_x as u16, o.cell_y as u16),
            ],
        });
    }
    let make = |name: &str, v: &Vec<Instance>| -> Option<LayerBuffer> {
        if v.is_empty() {
            return None;
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(name),
            contents: bytemuck::cast_slice(v),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Some(LayerBuffer {
            buffer,
            count: v.len() as u32,
        })
    };
    stats.instances_total += layers.iter().map(|l| l.len() as u32).sum::<u32>();
    let bounds = g.cells_bounds(
        base_x as f32,
        base_y as f32,
        (base_x + size - 1) as f32,
        (base_y + size - 1) as f32,
    );
    ChunkMesh {
        bounds,
        layers: [
            make("terrain", &layers[0]),
            make("overlay", &layers[1]),
            make("objects", &layers[2]),
        ],
    }
}
