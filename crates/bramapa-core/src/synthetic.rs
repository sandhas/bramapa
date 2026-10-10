//! In-code synthetic level, so the app runs with no pipeline output on disk.
//! Uses the shared placeholder tile id convention (see `bramapa-tileset/tiles.toml`).

use std::collections::HashMap;

use crate::chunk::{Chunk, ChunkObject};
use crate::level::{
    EntityDef, GridInfo, Level, LevelInfo, LevelManifest, ProjectionInfo, TilesetRef,
};
use crate::{CHUNK_SIZE, SPEC_VERSION};

pub mod tiles {
    pub const CAMPO: u16 = 1;
    pub const AGUA: u16 = 2;
    pub const OCEANO: u16 = 3;
    pub const FLORESTA: u16 = 4;
    pub const AREIA: u16 = 5;
    pub const URBANO: u16 = 6;
    pub const MONTANHA: u16 = 7;
    pub const SAVANA: u16 = 8;
    pub const AGRICULTURA: u16 = 9;
    pub const PASTAGEM: u16 = 10;
    pub const CAATINGA: u16 = 11;
    pub const PANTANAL: u16 = 12;
    pub const RODOVIA: u16 = 20;
    pub const RIO: u16 = 21;
    pub const LIMITE_ESTADO: u16 = 22;
    pub const LIMITE_MUNICIPIO: u16 = 23;
    pub const LIMITE_PAIS: u16 = 24;
    pub const FERROVIA: u16 = 25;
    pub const ARVORE: u16 = 40;
    pub const PREDIO: u16 = 41;
    pub const CASA: u16 = 42;
}

/// Small deterministic hash → [0, 1).
fn hash01(x: i64, y: i64, seed: u64) -> f32 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Value noise with bilinear interpolation at integer lattice spacing `period`.
fn value_noise(x: f32, y: f32, period: f32, seed: u64) -> f32 {
    let fx = x / period;
    let fy = y / period;
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let n00 = hash01(x0, y0, seed);
    let n10 = hash01(x0 + 1, y0, seed);
    let n01 = hash01(x0, y0 + 1, seed);
    let n11 = hash01(x0 + 1, y0 + 1, seed);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sy
}

fn fbm(x: f32, y: f32, seed: u64) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut period = 64.0;
    for i in 0..4 {
        sum += amp * value_noise(x, y, period, seed.wrapping_add(i));
        amp *= 0.5;
        period *= 0.5;
    }
    sum
}

/// Build a `width × height` synthetic level: an island continent with lakes, forests,
/// mountains, a coastal town, a road and a river.
pub fn level(width: u32, height: u32, seed: u64) -> Level {
    let cs = CHUNK_SIZE as u32;
    let chunks_x = width.div_ceil(cs);
    let chunks_y = height.div_ceil(cs);
    let (w, h) = (width as f32, height as f32);
    let (cx0, cy0) = (w * 0.5, h * 0.5);
    let radius = w.min(h) * 0.42;

    let mut chunks = HashMap::new();
    for cy in 0..chunks_y {
        for cx in 0..chunks_x {
            let mut chunk = Chunk::empty(cx as i32, cy as i32);
            let mut entity = vec![0u16; CHUNK_SIZE as usize * CHUNK_SIZE as usize];
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let gx = cx * cs + lx as u32;
                    let gy = cy * cs + ly as u32;
                    if gx >= width || gy >= height {
                        continue;
                    }
                    let (x, y) = (gx as f32, gy as f32);
                    let dx = x - cx0;
                    let dy = y - cy0;
                    let dist = (dx * dx + dy * dy).sqrt() / radius;
                    let coast = fbm(x, y, seed) * 0.5 - 0.25;
                    let land = 1.0 - dist + coast; // > 0 on land
                    let i = chunk.idx(lx, ly);
                    let terrain = if land < -0.08 {
                        tiles::OCEANO
                    } else if land < 0.0 {
                        tiles::AGUA
                    } else if land < 0.05 {
                        tiles::AREIA
                    } else {
                        let biome = fbm(x + 1000.0, y - 500.0, seed ^ 0xABCD);
                        let height_n = fbm(x - 300.0, y + 700.0, seed ^ 0x1234);
                        if height_n > 0.68 {
                            tiles::MONTANHA
                        } else if biome < 0.35 {
                            tiles::FLORESTA
                        } else if biome < 0.45 {
                            tiles::SAVANA
                        } else if biome < 0.55 {
                            tiles::CAMPO
                        } else if biome < 0.62 {
                            tiles::PASTAGEM
                        } else if biome < 0.7 && height_n < 0.3 {
                            tiles::AGUA
                        } else {
                            tiles::AGRICULTURA
                        }
                    };
                    chunk.terrain[i] = terrain;
                    if terrain != tiles::OCEANO && terrain != tiles::AGUA {
                        // Four fake "states" by quadrant, so hover/click can be exercised.
                        entity[i] = 1 + (x >= cx0) as u16 + 2 * (y >= cy0) as u16;
                    }

                    // Coastal town: a square patch east of the centre.
                    let tx = x - (cx0 + radius * 0.45);
                    let ty = y - (cy0 + radius * 0.1);
                    let in_town = tx.abs() < w * 0.03 && ty.abs() < h * 0.03;
                    if in_town && terrain != tiles::OCEANO && terrain != tiles::AGUA {
                        chunk.terrain[i] = tiles::URBANO;
                        let r = hash01(gx as i64, gy as i64, seed ^ 0x77);
                        if r < 0.08 {
                            chunk.objects.push(ChunkObject {
                                cell_x: lx as u8,
                                cell_y: ly as u8,
                                tile_id: tiles::PREDIO,
                                flags: 0,
                            });
                        } else if r < 0.2 {
                            chunk.objects.push(ChunkObject {
                                cell_x: lx as u8,
                                cell_y: ly as u8,
                                tile_id: tiles::CASA,
                                flags: 0,
                            });
                        }
                    } else if terrain == tiles::FLORESTA {
                        let r = hash01(gx as i64, gy as i64, seed ^ 0x99);
                        if r < 0.12 {
                            let flags = if r < 0.06 {
                                ChunkObject::FLAG_MIRROR
                            } else {
                                0
                            };
                            chunk.objects.push(ChunkObject {
                                cell_x: lx as u8,
                                cell_y: ly as u8,
                                tile_id: tiles::ARVORE,
                                flags,
                            });
                        }
                    }

                    // Road: a straight east-west line through the town; river: a wavy north-south line.
                    let road_y = cy0 + radius * 0.1;
                    if (y - road_y).abs() < 0.6 && chunk.terrain[i] != tiles::OCEANO && land > -0.08
                    {
                        chunk.overlay[i] = tiles::RODOVIA;
                    }
                    let river_x = cx0 - radius * 0.2 + (y * 0.05).sin() * 12.0;
                    if (x - river_x).abs() < 0.8 && land > 0.0 {
                        chunk.overlay[i] = tiles::RIO;
                    }
                }
            }
            chunk.entity = Some(entity);
            chunks.insert((cx as i32, cy as i32), chunk);
        }
    }

    let manifest = LevelManifest {
        spec_version: SPEC_VERSION,
        level: LevelInfo {
            level: 1,
            entity: "pais:BR".into(),
            name: "Nível sintético (em código)".into(),
        },
        grid: GridInfo {
            width,
            height,
            chunk_size: CHUNK_SIZE,
            cell_meters: 8000.0,
        },
        projection: ProjectionInfo {
            crs: "EPSG:5880".into(),
            origin_x: 0.0,
            origin_y: 0.0,
            rotation_deg: 45.0,
        },
        tileset: TilesetRef {
            id: "bramapa-base".into(),
            version: "0.0.1".into(),
        },
        period: None,
        entities: ["Noroeste", "Nordeste", "Sudoeste", "Sudeste"]
            .iter()
            .enumerate()
            .map(|(i, n)| EntityDef {
                index: i as u16 + 1,
                id: format!("estado:9{}", i + 1),
                name: format!("Quadrante {n}"),
            })
            .collect(),
    };
    Level {
        manifest,
        chunks,
        dir: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_chunk_grid() {
        let l = level(200, 130, 7);
        assert_eq!(l.manifest.chunks_x(), 4);
        assert_eq!(l.manifest.chunks_y(), 3);
        assert_eq!(l.chunks.len(), 12);
        let has_land = l.chunks.values().any(|c| {
            c.terrain
                .iter()
                .any(|&t| t == tiles::CAMPO || t == tiles::FLORESTA)
        });
        assert!(has_land);
        let objects: usize = l.chunks.values().map(|c| c.objects.len()).sum();
        assert!(objects > 0);
        // deterministic
        assert_eq!(level(200, 130, 7).chunks[&(1, 1)], l.chunks[&(1, 1)]);
    }
}
