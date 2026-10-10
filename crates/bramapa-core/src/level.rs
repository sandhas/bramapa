//! `level.toml` manifest and loading of a level directory (manifest + chunks).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::chunk::Chunk;
use crate::{CHUNK_SIZE, SPEC_VERSION};

#[derive(Clone, Debug, Deserialize)]
pub struct LevelManifest {
    pub spec_version: u16,
    pub level: LevelInfo,
    pub grid: GridInfo,
    pub projection: ProjectionInfo,
    pub tileset: TilesetRef,
    #[serde(default)]
    pub period: Option<PeriodInfo>,
    /// Entities referenced by the chunks' entity layer (index 0 = none).
    #[serde(default)]
    pub entities: Vec<EntityDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EntityDef {
    pub index: u16,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LevelInfo {
    pub level: u8,
    pub entity: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GridInfo {
    pub width: u32,
    pub height: u32,
    pub chunk_size: u16,
    pub cell_meters: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectionInfo {
    pub crs: String,
    pub origin_x: f64,
    pub origin_y: f64,
    pub rotation_deg: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TilesetRef {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PeriodInfo {
    pub start: String,
    pub end: String,
}

impl LevelManifest {
    pub fn parse(text: &str) -> Result<Self> {
        let m: Self = toml::from_str(text).context("parsing level.toml")?;
        if m.spec_version != SPEC_VERSION {
            bail!("unsupported level spec_version {}", m.spec_version);
        }
        if m.grid.chunk_size != CHUNK_SIZE {
            bail!("unsupported chunk_size {}", m.grid.chunk_size);
        }
        if !(1..=3).contains(&m.level.level) {
            bail!("level must be 1, 2 or 3 (got {})", m.level.level);
        }
        let mut seen = std::collections::HashSet::new();
        for e in &m.entities {
            if e.index == 0 {
                bail!("entity index 0 is reserved ('{}')", e.id);
            }
            if !seen.insert(e.index) {
                bail!("duplicate entity index {}", e.index);
            }
        }
        Ok(m)
    }

    /// Entity by chunk-layer index (None for 0 or unknown).
    pub fn entity(&self, index: u16) -> Option<&EntityDef> {
        if index == 0 {
            return None;
        }
        self.entities.iter().find(|e| e.index == index)
    }

    pub fn chunks_x(&self) -> u32 {
        self.grid.width.div_ceil(self.grid.chunk_size as u32)
    }

    pub fn chunks_y(&self) -> u32 {
        self.grid.height.div_ceil(self.grid.chunk_size as u32)
    }
}

/// On-disk directory name of an entity id (`:` is not allowed on every filesystem).
pub fn entity_dir_name(entity_id: &str) -> String {
    entity_id.replace(':', "_")
}

/// `<root>/<level>/<entity-dir>` per `docs/spec/chunk.md`.
pub fn level_dir(root: &Path, level: u8, entity_id: &str) -> PathBuf {
    root.join(level.to_string())
        .join(entity_dir_name(entity_id))
}

/// A loaded level: manifest plus all chunks found on disk, keyed by `(cx, cy)`.
#[derive(Clone, Debug)]
pub struct Level {
    pub manifest: LevelManifest,
    pub chunks: HashMap<(i32, i32), Chunk>,
    pub dir: Option<PathBuf>,
}

impl Level {
    /// Load `dir/level.toml` and every `dir/chunks/<cx>_<cy>.bmc`.
    pub fn load(dir: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(dir.join("level.toml"))
            .with_context(|| format!("reading {}", dir.join("level.toml").display()))?;
        let manifest = LevelManifest::parse(&text)?;
        let mut chunks = HashMap::new();
        let chunks_dir = dir.join("chunks");
        if chunks_dir.is_dir() {
            for entry in std::fs::read_dir(&chunks_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("bmc") {
                    continue;
                }
                let chunk = Chunk::read_file(&path)?;
                let (cx, cy) = (chunk.cx, chunk.cy);
                if cx < 0
                    || cy < 0
                    || cx as u32 >= manifest.chunks_x()
                    || cy as u32 >= manifest.chunks_y()
                {
                    bail!("chunk {} is outside the grid", path.display());
                }
                chunks.insert((cx, cy), chunk);
            }
        }
        Ok(Self {
            manifest,
            chunks,
            dir: Some(dir.to_path_buf()),
        })
    }

    /// Entity index at a global cell (0 outside the grid, in missing chunks, or without layer).
    pub fn entity_at(&self, x: u32, y: u32) -> u16 {
        let cs = self.manifest.grid.chunk_size as u32;
        self.chunks
            .get(&((x / cs) as i32, (y / cs) as i32))
            .map_or(0, |c| c.entity_at((x % cs) as u16, (y % cs) as u16))
    }

    /// Replace chunks with manual override chunks read from `dir/<cx>_<cy>.bmc`.
    /// `priorities` remembers the winning priority per chunk across several overrides.
    pub fn apply_override(
        &mut self,
        dir: &Path,
        chunks: &[[i32; 2]],
        priority: i32,
        priorities: &mut HashMap<(i32, i32), i32>,
    ) -> Result<usize> {
        let mut applied = 0;
        for &[cx, cy] in chunks {
            if cx < 0
                || cy < 0
                || cx as u32 >= self.manifest.chunks_x()
                || cy as u32 >= self.manifest.chunks_y()
            {
                bail!("override chunk ({cx}, {cy}) is outside the grid");
            }
            if priorities.get(&(cx, cy)).is_some_and(|&p| p >= priority) {
                continue;
            }
            let path = dir.join(format!("{cx}_{cy}.bmc"));
            let chunk = Chunk::read_file(&path)?;
            if (chunk.cx, chunk.cy) != (cx, cy) {
                bail!(
                    "{} declares coordinates ({}, {})",
                    path.display(),
                    chunk.cx,
                    chunk.cy
                );
            }
            self.chunks.insert((cx, cy), chunk);
            priorities.insert((cx, cy), priority);
            applied += 1;
        }
        Ok(applied)
    }

    pub fn cell_count(&self) -> u64 {
        self.manifest.grid.width as u64 * self.manifest.grid.height as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
spec_version = 0
[level]
level = 3
entity = "municipio:4314902"
name = "Porto Alegre"
[grid]
width = 1000
height = 1024
chunk_size = 64
cell_meters = 30.0
[projection]
crs = "EPSG:5880"
origin_x = 0.0
origin_y = 0.0
rotation_deg = 45.0
[tileset]
id = "bramapa-base"
version = "0.0.1"
[period]
start = "2023"
end = "2023"
[[entities]]
index = 1
id = "estado:43"
name = "Rio Grande do Sul"
[[entities]]
index = 2
id = "pais:UY"
name = "Uruguai"
"#;

    #[test]
    fn parses_spec_example() {
        let m = LevelManifest::parse(EXAMPLE).unwrap();
        assert_eq!(m.level.entity, "municipio:4314902");
        assert_eq!(m.chunks_x(), 16);
        assert_eq!(m.chunks_y(), 16);
        assert_eq!(m.period.as_ref().unwrap().start, "2023");
        assert_eq!(m.entity(2).unwrap().id, "pais:UY");
        assert!(m.entity(0).is_none());
        assert!(LevelManifest::parse(&EXAMPLE.replace("index = 2", "index = 1")).is_err());
    }

    #[test]
    fn level_dir_layout() {
        assert_eq!(entity_dir_name("municipio:4314902"), "municipio_4314902");
        assert_eq!(
            level_dir(Path::new("/r"), 3, "municipio:4314902"),
            PathBuf::from("/r/3/municipio_4314902")
        );
    }

    #[test]
    fn rejects_wrong_version() {
        assert!(
            LevelManifest::parse(&EXAMPLE.replace("spec_version = 0", "spec_version = 9")).is_err()
        );
    }
}
