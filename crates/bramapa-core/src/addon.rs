//! Addon `manifest.toml` parsing (read side only). Validation of data files lives in
//! `bramapa-data`; here we only parse the structure the engine needs. See `docs/spec/addon.md`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::SPEC_VERSION;

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub spec_version: u16,
    pub addon: AddonInfo,
    #[serde(default)]
    pub entity_types: Vec<EntityTypeDef>,
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    #[serde(default)]
    pub render: Vec<RenderRule>,
    #[serde(default)]
    pub map_overrides: Vec<MapOverride>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AddonInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub depends: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EntityTypeDef {
    pub id: String,
    pub name: String,
    #[serde(default = "default_min_level")]
    pub min_level: u8,
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

fn default_min_level() -> u8 {
    1
}
fn default_position() -> String {
    "none".into()
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DType {
    I64,
    F64,
    Str,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PropertyDef {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    #[serde(default)]
    pub unit: String,
    pub dtype: DType,
    pub periodicity: String,
    pub source: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub file: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RenderRule {
    pub property: String,
    pub kind: String,
    #[serde(default)]
    pub ramp: Option<String>,
    #[serde(default)]
    pub scale: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MapOverride {
    pub level: u8,
    pub entity: String,
    pub path: String,
    #[serde(default)]
    pub chunks: Vec<[i32; 2]>,
    #[serde(default)]
    pub priority: i32,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let m: Self = toml::from_str(text).context("parsing manifest.toml")?;
        if m.spec_version != SPEC_VERSION {
            bail!("unsupported addon spec_version {}", m.spec_version);
        }
        let id = &m.addon.id;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            bail!("invalid addon id '{id}' (allowed: [a-z0-9-]+)");
        }
        Ok(m)
    }

    /// Canonical property id: `<addon>/<entity_type>/<property>`.
    pub fn canonical_property_id(&self, p: &PropertyDef) -> String {
        format!("{}/{}/{}", self.addon.id, p.entity_type, p.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest() {
        let text = r#"
spec_version = 0
[addon]
id = "ibge-core"
name = "IBGE"
version = "0.1.0"
license = "CC-BY-4.0"
[[properties]]
id = "populacao"
entity_type = "municipio"
name = "População"
unit = "habitantes"
dtype = "i64"
periodicity = "anual"
source = "IBGE"
file = "data/municipio_populacao.csv"
[[render]]
property = "municipio/populacao"
kind = "choropleth"
ramp = "viridis"
scale = "log"
[[map_overrides]]
level = 3
entity = "municipio:4314902"
path = "map/poa-centro"
chunks = [[8, 7], [9, 7]]
priority = 10
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(
            m.canonical_property_id(&m.properties[0]),
            "ibge-core/municipio/populacao"
        );
        assert_eq!(m.properties[0].dtype, DType::I64);
        assert_eq!(m.map_overrides[0].chunks.len(), 2);
        assert!(Manifest::parse(&text.replace("ibge-core", "IBGE_Core")).is_err());
    }
}
