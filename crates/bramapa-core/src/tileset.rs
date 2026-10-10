//! `tiles.toml` parsing. See `docs/spec/tileset.md`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::SPEC_VERSION;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TileClass {
    Terrain,
    Overlay,
    Object,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TileDef {
    pub id: u16,
    pub name: String,
    pub class: TileClass,
    pub col: u16,
    pub row: u16,
    #[serde(default = "one")]
    pub frames: u16,
    #[serde(default)]
    pub fps: u16,
    #[serde(default)]
    pub cover: Option<String>,
}

fn one() -> u16 {
    1
}

#[derive(Clone, Debug, Deserialize)]
pub struct TilesetInfo {
    pub id: String,
    pub version: String,
    pub atlas: String,
    pub tile_width: u16,
    pub tile_height: u16,
    pub cell_width: u16,
    pub cell_height: u16,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AutotileRule {
    pub cover: String,
    pub default: u16,
    #[serde(default)]
    pub masks: HashMap<String, u16>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Tileset {
    pub spec_version: u16,
    pub tileset: TilesetInfo,
    #[serde(default)]
    pub tiles: Vec<TileDef>,
    #[serde(default)]
    pub autotile: Vec<AutotileRule>,
    /// Directory the tileset was loaded from (None for in-memory tilesets).
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

impl Tileset {
    pub fn parse(text: &str) -> Result<Self> {
        let t: Self = toml::from_str(text).context("parsing tiles.toml")?;
        t.validate()?;
        Ok(t)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("tiles.toml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut t = Self::parse(&text)?;
        t.dir = Some(dir.to_path_buf());
        Ok(t)
    }

    pub fn atlas_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(&self.tileset.atlas))
    }

    fn validate(&self) -> Result<()> {
        if self.spec_version != SPEC_VERSION {
            bail!("unsupported tileset spec_version {}", self.spec_version);
        }
        let mut seen = HashMap::new();
        for t in &self.tiles {
            if t.id == 0 {
                bail!("tile id 0 is reserved (tile '{}')", t.name);
            }
            if let Some(prev) = seen.insert(t.id, &t.name) {
                bail!("duplicate tile id {} ('{}' and '{}')", t.id, prev, t.name);
            }
            if t.frames == 0 {
                bail!("tile '{}' has frames = 0", t.name);
            }
            if t.frames > 1 && t.fps == 0 {
                bail!("animated tile '{}' has fps = 0", t.name);
            }
        }
        Ok(())
    }

    pub fn get(&self, id: u16) -> Option<&TileDef> {
        self.tiles.iter().find(|t| t.id == id)
    }

    /// Dense lookup table indexed by tile id (for hot loops).
    pub fn lookup_table(&self) -> Vec<Option<&TileDef>> {
        let max = self.tiles.iter().map(|t| t.id).max().unwrap_or(0) as usize;
        let mut v = vec![None; max + 1];
        for t in &self.tiles {
            v[t.id as usize] = Some(t);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
spec_version = 0
[tileset]
id = "bramapa-base"
version = "0.0.1"
atlas = "atlas.png"
tile_width = 64
tile_height = 32
cell_width = 64
cell_height = 64
license = "CC-BY-4.0"
[[tiles]]
id = 1
name = "campo"
class = "terrain"
col = 0
row = 0
cover = "campo"
[[tiles]]
id = 2
name = "agua"
class = "terrain"
col = 1
row = 0
frames = 4
fps = 4
cover = "agua"
[[autotile]]
cover = "agua"
default = 2
[autotile.masks]
"0" = 2
"15" = 2
"#;

    #[test]
    fn parses_example() {
        let t = Tileset::parse(EXAMPLE).unwrap();
        assert_eq!(t.tiles.len(), 2);
        assert_eq!(t.get(2).unwrap().frames, 4);
        assert_eq!(t.get(1).unwrap().class, TileClass::Terrain);
        assert_eq!(t.lookup_table().len(), 3);
        assert_eq!(t.autotile[0].masks["15"], 2);
    }

    #[test]
    fn rejects_duplicate_ids() {
        let bad = EXAMPLE.replace("id = 2", "id = 1");
        assert!(Tileset::parse(&bad).is_err());
    }
}
