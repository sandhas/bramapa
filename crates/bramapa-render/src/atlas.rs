//! Atlas texture upload and an in-code placeholder atlas (so the app runs with no art on disk).

use anyhow::{Context, Result};
use bramapa_core::tileset::{TileClass, TileDef, Tileset, TilesetInfo};
use std::path::Path;

/// A GPU texture + nearest sampler + bind group for one atlas.
pub struct AtlasTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

impl AtlasTexture {
    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    pub fn from_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Self {
        assert_eq!(
            rgba.len(),
            (width * height * 4) as usize,
            "rgba size mismatch"
        );
        let dilated = dilate(rgba, width, height);
        let mips = mip_chain(&dilated, width, height);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size,
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, (w, h, data)) in mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            // Nearest when magnifying (pixel art); trilinear when minifying (overview zoom).
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            texture,
            view,
            sampler,
            bind_group,
            width,
            height,
        }
    }

    pub fn from_png(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        path: &Path,
    ) -> Result<Self> {
        let img = image::open(path)
            .with_context(|| format!("opening atlas {}", path.display()))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        Ok(Self::from_rgba(device, queue, layout, w, h, img.as_raw()))
    }
}

/// Give every fully transparent texel the colour of its nearest opaque neighbour (alpha stays
/// 0), so linear/mip filtering never blends towards black at sprite edges.
pub fn dilate(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let (w, h) = (width as i32, height as i32);
    // A few passes of 8-neighbour propagation are enough for 64 px cells.
    for _ in 0..8 {
        let src = out.clone();
        let mut changed = false;
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if src[i + 3] != 0 || (src[i] | src[i + 1] | src[i + 2]) != 0 {
                    continue; // opaque, or already filled in a previous pass
                }
                let mut acc = [0u32; 3];
                let mut n = 0u32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        let j = ((ny * w + nx) * 4) as usize;
                        if src[j + 3] != 0 || (src[j] | src[j + 1] | src[j + 2]) != 0 {
                            acc[0] += src[j] as u32;
                            acc[1] += src[j + 1] as u32;
                            acc[2] += src[j + 2] as u32;
                            n += 1;
                        }
                    }
                }
                if let Some(r) = acc[0].checked_div(n) {
                    out[i] = r.max(1) as u8; // never exactly black, marks "filled"
                    out[i + 1] = (acc[1] / n) as u8;
                    out[i + 2] = (acc[2] / n) as u8;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    out
}

/// Full mip chain down to 1×1 (alpha-weighted box filter, so transparent texels never bleed
/// their colour into the averages).
pub fn mip_chain(rgba: &[u8], width: u32, height: u32) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(width, height, rgba.to_vec())];
    while let Some((w, h, src)) = levels.last() {
        if *w == 1 && *h == 1 {
            break;
        }
        let (nw, nh) = ((*w / 2).max(1), (*h / 2).max(1));
        let (w, h) = (*w, *h);
        let mut dst = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0u32; 4];
                let mut n = 0u32;
                for (sy, sx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                    let px = (x * 2 + sx).min(w - 1);
                    let py = (y * 2 + sy).min(h - 1);
                    let i = ((py * w + px) * 4) as usize;
                    let a = src[i + 3] as u32;
                    acc[0] += src[i] as u32 * a;
                    acc[1] += src[i + 1] as u32 * a;
                    acc[2] += src[i + 2] as u32 * a;
                    acc[3] += a;
                    n += 1;
                }
                let o = ((y * nw + x) * 4) as usize;
                if let Some(r) = acc[0].checked_div(acc[3]) {
                    dst[o] = r as u8;
                    dst[o + 1] = (acc[1] / acc[3]) as u8;
                    dst[o + 2] = (acc[2] / acc[3]) as u8;
                }
                dst[o + 3] = (acc[3] / n) as u8;
            }
        }
        levels.push((nw, nh, dst));
    }
    levels
}

/// Procedurally drawn placeholder atlas matching the shared placeholder tile id convention.
pub struct PlaceholderAtlas {
    pub tileset: Tileset,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const CELL: u32 = 64;
const TILE_H: u32 = 32;

impl PlaceholderAtlas {
    pub fn generate() -> Self {
        // (id, name, class, frames, fps, cover, base colour)
        type Def = (u16, &'static str, TileClass, u16, u16, [u8; 3]);
        let defs: &[Def] = &[
            (1, "campo", TileClass::Terrain, 1, 0, [120, 170, 80]),
            (2, "agua", TileClass::Terrain, 4, 4, [70, 130, 200]),
            (3, "oceano", TileClass::Terrain, 4, 3, [35, 80, 160]),
            (4, "floresta", TileClass::Terrain, 1, 0, [50, 110, 55]),
            (5, "areia", TileClass::Terrain, 1, 0, [225, 205, 150]),
            (6, "urbano", TileClass::Terrain, 1, 0, [150, 150, 155]),
            (7, "montanha", TileClass::Terrain, 1, 0, [120, 110, 100]),
            (8, "savana", TileClass::Terrain, 1, 0, [170, 180, 90]),
            (9, "agricultura", TileClass::Terrain, 1, 0, [160, 150, 70]),
            (10, "pastagem", TileClass::Terrain, 1, 0, [150, 190, 100]),
            (11, "caatinga", TileClass::Terrain, 1, 0, [190, 170, 120]),
            (12, "pantanal", TileClass::Terrain, 1, 0, [80, 150, 140]),
            (20, "rodovia", TileClass::Overlay, 1, 0, [90, 90, 95]),
            (21, "rio", TileClass::Overlay, 4, 4, [80, 150, 220]),
            (
                22,
                "limite_estado",
                TileClass::Overlay,
                1,
                0,
                [255, 255, 255],
            ),
            (
                23,
                "limite_municipio",
                TileClass::Overlay,
                1,
                0,
                [230, 230, 200],
            ),
            (24, "limite_pais", TileClass::Overlay, 1, 0, [255, 220, 120]),
            (25, "ferrovia", TileClass::Overlay, 1, 0, [60, 50, 45]),
            (40, "arvore", TileClass::Object, 1, 0, [40, 100, 50]),
            (41, "predio", TileClass::Object, 1, 0, [200, 200, 210]),
            (42, "casa", TileClass::Object, 1, 0, [210, 150, 110]),
        ];
        let total_cells: u32 = defs.iter().map(|d| d.3 as u32).sum();
        let cols = 16u32;
        let rows = total_cells.div_ceil(cols);
        let (width, height) = (cols * CELL, rows * CELL);
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let mut tiles = Vec::new();
        let mut cursor = 0u32;
        for &(id, name, class, frames, fps, color) in defs {
            let col = cursor % cols;
            let row = cursor / cols;
            for f in 0..frames as u32 {
                let (c, r) = ((cursor + f) % cols, (cursor + f) / cols);
                draw_cell(&mut rgba, width, c * CELL, r * CELL, class, color, f, id);
            }
            tiles.push(TileDef {
                id,
                name: name.into(),
                class,
                col: col as u16,
                row: row as u16,
                frames,
                fps,
                cover: matches!(class, TileClass::Terrain | TileClass::Overlay)
                    .then(|| name.to_string()),
            });
            cursor += frames as u32;
        }
        let tileset = Tileset {
            spec_version: bramapa_core::SPEC_VERSION,
            tileset: TilesetInfo {
                id: "bramapa-placeholder".into(),
                version: "0.0.0".into(),
                atlas: String::new(),
                tile_width: CELL as u16,
                tile_height: TILE_H as u16,
                cell_width: CELL as u16,
                cell_height: CELL as u16,
                license: Some("MIT OR Apache-2.0".into()),
            },
            tiles,
            autotile: Vec::new(),
            dir: None,
        };
        Self {
            tileset,
            width,
            height,
            rgba,
        }
    }
}

fn put(rgba: &mut [u8], width: u32, x: u32, y: u32, c: [u8; 3]) {
    let i = ((y * width + x) * 4) as usize;
    rgba[i..i + 3].copy_from_slice(&c);
    rgba[i + 3] = 255;
}

fn shade(c: [u8; 3], d: i16) -> [u8; 3] {
    [
        (c[0] as i16 + d).clamp(0, 255) as u8,
        (c[1] as i16 + d).clamp(0, 255) as u8,
        (c[2] as i16 + d).clamp(0, 255) as u8,
    ]
}

/// Is `(x, y)` (cell-local) inside the 64×32 diamond at the bottom of the cell?
fn in_diamond(x: u32, y: u32) -> bool {
    if y < CELL - TILE_H {
        return false;
    }
    let dy = (y - (CELL - TILE_H)) as i32; // 0..32
    let dx = x as i32 - 32; // -32..31
                            // Diamond: |dx| / 32 + |dy - 16| / 16 <= 1
    dx.abs() * 16 + (dy - 16).abs() * 32 < 32 * 16 + 1
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    rgba: &mut [u8],
    width: u32,
    ox: u32,
    oy: u32,
    class: TileClass,
    color: [u8; 3],
    frame: u32,
    id: u16,
) {
    match class {
        TileClass::Terrain => {
            for y in 0..CELL {
                for x in 0..CELL {
                    if !in_diamond(x, y) {
                        continue;
                    }
                    let mut c = color;
                    // Two-tone checker dither so tiles don't look flat.
                    if (x / 2 + y / 2) % 2 == 0 {
                        c = shade(c, -6);
                    }
                    // Animated water: moving diagonal highlights.
                    if id == 2 || id == 3 {
                        let phase = (x + y * 2 + frame * 4) % 16;
                        if phase < 2 {
                            c = shade(c, 28);
                        }
                    }
                    // Mountain: lighter cap in the upper half.
                    if id == 7 && y < CELL - TILE_H / 2 && (x as i32 - 32).abs() < 10 {
                        c = shade(c, 70);
                    }
                    // Urban: grid of darker lines.
                    if id == 6 && (x % 8 == 0 || y % 4 == 0) {
                        c = shade(c, -25);
                    }
                    put(rgba, width, ox + x, oy + y, c);
                }
            }
        }
        TileClass::Overlay => {
            // Band along the diamond's left-bottom → right-top diagonal (world "x" axis),
            // dashed for borders, with a wave shift for the river.
            let dashed = (22..=24).contains(&id);
            for y in 0..CELL {
                for x in 0..CELL {
                    if !in_diamond(x, y) {
                        continue;
                    }
                    let dy = (y - (CELL - TILE_H)) as i32 - 16;
                    let dx = x as i32 - 32;
                    // Distance from the diagonal running from (−32,0)…(32,0) rotated: use the
                    // iso x-axis direction (1, 0.5) in cell pixels → dy - dx/2.
                    let d = (dy - dx / 2).abs();
                    let w = if matches!(id, 20 | 21 | 25) { 3 } else { 1 };
                    if d > w {
                        continue;
                    }
                    if dashed && ((x / 4) % 2 == 0) {
                        continue;
                    }
                    let mut c = color;
                    if id == 21 && (x + frame * 3) % 9 < 2 {
                        c = shade(c, 40);
                    }
                    if id == 25 && x % 6 < 2 {
                        c = shade(c, 60);
                    }
                    put(rgba, width, ox + x, oy + y, c);
                }
            }
        }
        TileClass::Object => {
            let cx = 32u32;
            let base_y = CELL - TILE_H / 2; // diamond centre row
            match id {
                40 => {
                    // trunk
                    for y in base_y - 10..base_y {
                        for x in cx - 2..cx + 2 {
                            put(rgba, width, ox + x, oy + y, [100, 70, 40]);
                        }
                    }
                    // canopy: circle
                    let (ccx, ccy, r) = (cx as i32, base_y as i32 - 18, 11i32);
                    for y in 0..CELL as i32 {
                        for x in 0..CELL as i32 {
                            let dx = x - ccx;
                            let dy = y - ccy;
                            if dx * dx + dy * dy <= r * r {
                                let c = if dx + dy < -6 {
                                    shade(color, 25)
                                } else if dx + dy > 6 {
                                    shade(color, -25)
                                } else {
                                    color
                                };
                                put(rgba, width, ox + x as u32, oy + y as u32, c);
                            }
                        }
                    }
                }
                41 => {
                    // tall box: left face, right face, top
                    let h = 34u32;
                    for y in base_y - h..base_y + 8 {
                        for x in cx - 14..cx + 14 {
                            let left = x < cx;
                            let mut c = if left {
                                shade(color, -30)
                            } else {
                                shade(color, -60)
                            };
                            if y < base_y - h + 4 {
                                c = shade(color, 10);
                            } else if (y % 5 == 1) && (x % 4 == 1) {
                                c = [60, 80, 120]; // windows
                            }
                            put(rgba, width, ox + x, oy + y, c);
                        }
                    }
                }
                _ => {
                    // small house: box + darker roof band
                    let h = 14u32;
                    for y in base_y - h..base_y + 6 {
                        for x in cx - 10..cx + 10 {
                            let left = x < cx;
                            let mut c = if left { color } else { shade(color, -40) };
                            if y < base_y - h + 5 {
                                c = [150, 60, 50];
                            }
                            put(rgba, width, ox + x, oy + y, c);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dilate_fills_transparent_corners_with_edge_colour() {
        let p = PlaceholderAtlas::generate();
        let d = dilate(&p.rgba, p.width, p.height);
        let t = p.tileset.get(1).unwrap();
        // a texel just outside the campo diamond: transparent, but now green-ish
        let (x, y) = (t.col as u32 * CELL + 2, t.row as u32 * CELL + 40);
        let i = ((y * p.width + x) * 4) as usize;
        assert_eq!(p.rgba[i + 3], 0);
        assert_eq!(d[i + 3], 0);
        assert!(d[i + 1] > 100, "{:?}", &d[i..i + 4]);
    }

    #[test]
    fn mip_chain_reaches_one_pixel_and_keeps_colour() {
        let p = PlaceholderAtlas::generate();
        let mips = mip_chain(&p.rgba, p.width, p.height);
        assert_eq!(mips.last().unwrap().0, 1);
        assert_eq!(mips.last().unwrap().1, 1);
        assert_eq!(mips.len() as u32, 1 + p.width.max(p.height).ilog2());
        // level 1 of tile 1 (campo) is still green-ish, not darkened by transparent neighbours
        let (w1, _, d1) = &mips[1];
        let t = p.tileset.get(1).unwrap();
        let (x, y) = (
            (t.col as u32 * CELL + 32) / 2,
            (t.row as u32 * CELL + 48) / 2,
        );
        let i = ((y * w1 + x) * 4) as usize;
        assert!(
            d1[i + 1] > d1[i] && d1[i + 1] > d1[i + 2],
            "{:?}",
            &d1[i..i + 4]
        );
        assert!(d1[i + 3] > 200);
    }

    #[test]
    fn placeholder_is_consistent() {
        let p = PlaceholderAtlas::generate();
        assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
        assert_eq!(p.tileset.tiles.len(), 21);
        let agua = p.tileset.get(2).unwrap();
        assert_eq!(agua.frames, 4);
        // every tile's cells fit in the atlas
        for t in &p.tileset.tiles {
            assert!(
                (t.col as u32 + t.frames as u32) * CELL <= p.width,
                "{}",
                t.name
            );
            assert!((t.row as u32 + 1) * CELL <= p.height, "{}", t.name);
        }
        // the diamond of tile 1 has opaque pixels at its centre and none at the cell's top-left
        let t = p.tileset.get(1).unwrap();
        let px = |x: u32, y: u32| {
            p.rgba
                [(((t.row as u32 * CELL + y) * p.width + t.col as u32 * CELL + x) * 4 + 3) as usize]
        };
        assert_eq!(px(32, 48), 255);
        assert_eq!(px(0, 0), 0);
    }
}
