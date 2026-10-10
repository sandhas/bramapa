//! Reader for the built data (`index.toml` + `.bmd` columnar files). See
//! `docs/spec/data-binary.md`. Everything is loaded into memory once; queries are binary
//! searches over rows sorted by `(entity_idx, period_start)`.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::addon::{DType, Manifest};
use crate::SPEC_VERSION;

const MAGIC: &[u8; 4] = b"BMDT";
const HEADER_LEN: usize = 24;

#[derive(Clone, Debug, Deserialize)]
pub struct Index {
    pub spec_version: u16,
    #[serde(default)]
    pub addons: Vec<IndexAddon>,
    #[serde(default)]
    pub properties: Vec<IndexProperty>,
    #[serde(default)]
    pub map_overrides: Vec<IndexMapOverride>,
}

/// A manual map override shipped by an addon (chunks replacing procedural ones).
#[derive(Clone, Debug, Deserialize)]
pub struct IndexMapOverride {
    pub addon: String,
    pub level: u8,
    pub entity: String,
    /// Directory with `<cx>_<cy>.bmc`, relative to the build dir.
    pub path: String,
    #[serde(default)]
    pub chunks: Vec<[i32; 2]>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IndexAddon {
    pub id: String,
    pub version: String,
    pub manifest: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IndexProperty {
    /// Canonical id `<addon>/<entity_type>/<name>`.
    pub id: String,
    pub file: String,
    #[serde(default)]
    pub rows: u32,
    #[serde(default)]
    pub period_min: Option<String>,
    #[serde(default)]
    pub period_max: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    I64(i64),
    F64(f64),
    /// Index into `PropertyData::strings`.
    Str(u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub entity_idx: u32,
    /// Days since 1970-01-01, inclusive range.
    pub start: i32,
    pub end: i32,
    pub value: Value,
}

/// One property's metadata (from the addon manifest) plus all its rows.
#[derive(Clone, Debug)]
pub struct PropertyData {
    pub id: String,
    pub addon: String,
    pub entity_type: String,
    pub name: String,
    pub unit: String,
    pub dtype: DType,
    pub periodicity: String,
    pub source: String,
    pub source_url: Option<String>,
    pub entities: Vec<String>,
    pub strings: Vec<String>,
    pub rows: Vec<Row>,
}

impl PropertyData {
    /// Rows for one entity id (empty if unknown), in period order.
    pub fn rows_for(&self, entity_id: &str) -> &[Row] {
        let Some(idx) = self.entity_index(entity_id) else {
            return &[];
        };
        let lo = self.rows.partition_point(|r| r.entity_idx < idx);
        let hi = self.rows.partition_point(|r| r.entity_idx <= idx);
        &self.rows[lo..hi]
    }

    pub fn entity_index(&self, entity_id: &str) -> Option<u32> {
        // The entity table is sorted lexicographically by the builder; fall back to linear scan
        // if a producer did not sort it.
        match self
            .entities
            .binary_search_by(|e| e.as_str().cmp(entity_id))
        {
            Ok(i) => Some(i as u32),
            Err(_) => self
                .entities
                .iter()
                .position(|e| e == entity_id)
                .map(|i| i as u32),
        }
    }

    pub fn value_str(&self, v: Value) -> String {
        match v {
            Value::I64(i) => i.to_string(),
            Value::F64(f) => f.to_string(),
            Value::Str(s) => self.strings.get(s as usize).cloned().unwrap_or_default(),
        }
    }

    /// Decode one `.bmd` file. `meta` supplies the manifest-level fields.
    pub fn decode(bytes: &[u8], id: &str, meta: &PropertyMeta) -> Result<Self> {
        ensure!(bytes.len() >= HEADER_LEN, "data file too short");
        ensure!(&bytes[0..4] == MAGIC, "bad magic");
        let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
        let u32_at =
            |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let version = u16_at(4);
        ensure!(
            version == SPEC_VERSION,
            "unsupported data spec_version {version}"
        );
        let dtype = match bytes[6] {
            0 => DType::I64,
            1 => DType::F64,
            2 => DType::Str,
            d => bail!("unknown dtype {d}"),
        };
        ensure!(
            dtype == meta.dtype,
            "dtype in file ({dtype:?}) differs from manifest ({:?})",
            meta.dtype
        );
        let row_count = u32_at(8) as usize;
        let entity_count = u32_at(12) as usize;
        let string_count = u32_at(16) as usize;
        let payload_len = u32_at(20) as usize;
        ensure!(
            bytes.len() == HEADER_LEN + payload_len,
            "payload_len does not match file size"
        );
        let raw = zstd::decode_all(&bytes[HEADER_LEN..]).context("zstd decode")?;

        let mut pos = 0usize;
        let mut read_strings = |count: usize| -> Result<Vec<String>> {
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                ensure!(raw.len() >= pos + 4, "string table truncated");
                let len = u32::from_le_bytes(raw[pos..pos + 4].try_into().unwrap()) as usize;
                pos += 4;
                ensure!(raw.len() >= pos + len, "string truncated");
                v.push(
                    std::str::from_utf8(&raw[pos..pos + len])
                        .context("utf-8")?
                        .to_string(),
                );
                pos += len;
            }
            Ok(v)
        };
        let entities = read_strings(entity_count)?;
        let strings = if dtype == DType::Str {
            read_strings(string_count)?
        } else {
            Vec::new()
        };

        let value_size = match dtype {
            DType::I64 | DType::F64 => 8,
            DType::Str => 4,
        };
        let need = pos + row_count * (4 + 4 + 4 + value_size);
        ensure!(
            raw.len() >= need,
            "columns truncated ({} < {need})",
            raw.len()
        );
        let col_u32 = |off: usize, i: usize| {
            u32::from_le_bytes(raw[off + i * 4..off + i * 4 + 4].try_into().unwrap())
        };
        let (o_ent, o_start, o_end, o_val) = (
            pos,
            pos + row_count * 4,
            pos + row_count * 8,
            pos + row_count * 12,
        );
        let mut rows = Vec::with_capacity(row_count);
        for i in 0..row_count {
            let entity_idx = col_u32(o_ent, i);
            ensure!(
                (entity_idx as usize) < entity_count,
                "row {i}: entity_idx out of range"
            );
            let value = match dtype {
                DType::I64 => Value::I64(i64::from_le_bytes(
                    raw[o_val + i * 8..o_val + i * 8 + 8].try_into().unwrap(),
                )),
                DType::F64 => Value::F64(f64::from_le_bytes(
                    raw[o_val + i * 8..o_val + i * 8 + 8].try_into().unwrap(),
                )),
                DType::Str => {
                    let s = col_u32(o_val, i);
                    ensure!(
                        (s as usize) < string_count,
                        "row {i}: string index out of range"
                    );
                    Value::Str(s)
                }
            };
            rows.push(Row {
                entity_idx,
                start: col_u32(o_start, i) as i32,
                end: col_u32(o_end, i) as i32,
                value,
            });
        }
        // The spec requires sorted rows; enforce it so queries can binary-search.
        if !rows
            .windows(2)
            .all(|w| (w[0].entity_idx, w[0].start) <= (w[1].entity_idx, w[1].start))
        {
            rows.sort_by_key(|r| (r.entity_idx, r.start));
        }
        Ok(Self {
            id: id.to_string(),
            addon: meta.addon.clone(),
            entity_type: meta.entity_type.clone(),
            name: meta.name.clone(),
            unit: meta.unit.clone(),
            dtype,
            periodicity: meta.periodicity.clone(),
            source: meta.source.clone(),
            source_url: meta.source_url.clone(),
            entities,
            strings,
            rows,
        })
    }
}

/// Manifest-level fields of a property, resolved from the addon manifest.
#[derive(Clone, Debug)]
pub struct PropertyMeta {
    pub addon: String,
    pub entity_type: String,
    pub name: String,
    pub unit: String,
    pub dtype: DType,
    pub periodicity: String,
    pub source: String,
    pub source_url: Option<String>,
}

/// All built data of a `build/` directory.
#[derive(Debug, Default)]
pub struct DataStore {
    pub properties: Vec<PropertyData>,
    pub addons: Vec<(String, String)>,
    /// Global period bounds over all rows (days), None when empty.
    pub period_bounds: Option<(i32, i32)>,
    pub map_overrides: Vec<IndexMapOverride>,
    /// Directory the store was loaded from (override paths are relative to it).
    pub dir: std::path::PathBuf,
}

impl DataStore {
    pub fn load(dir: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(dir.join("index.toml"))
            .with_context(|| format!("reading {}", dir.join("index.toml").display()))?;
        let index: Index = toml::from_str(&text).context("parsing index.toml")?;
        ensure!(
            index.spec_version == SPEC_VERSION,
            "unsupported index spec_version {}",
            index.spec_version
        );

        let mut metas: HashMap<String, PropertyMeta> = HashMap::new();
        let mut addons = Vec::new();
        for a in &index.addons {
            let path = dir.join(&a.manifest);
            let m = Manifest::parse(
                &std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?,
            )?;
            for p in &m.properties {
                metas.insert(
                    m.canonical_property_id(p),
                    PropertyMeta {
                        addon: m.addon.id.clone(),
                        entity_type: p.entity_type.clone(),
                        name: p.name.clone(),
                        unit: p.unit.clone(),
                        dtype: p.dtype,
                        periodicity: p.periodicity.clone(),
                        source: p.source.clone(),
                        source_url: p.source_url.clone(),
                    },
                );
            }
            addons.push((a.id.clone(), a.version.clone()));
        }

        let mut properties = Vec::new();
        let mut bounds: Option<(i32, i32)> = None;
        for p in &index.properties {
            let meta = metas
                .get(&p.id)
                .with_context(|| format!("property {} not declared by any addon manifest", p.id))?;
            let path = dir.join(&p.file);
            let bytes =
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            let data = PropertyData::decode(&bytes, &p.id, meta)
                .with_context(|| format!("decoding {}", path.display()))?;
            for r in &data.rows {
                bounds = Some(match bounds {
                    None => (r.start, r.end),
                    Some((lo, hi)) => (lo.min(r.start), hi.max(r.end)),
                });
            }
            properties.push(data);
        }
        Ok(Self {
            properties,
            addons,
            period_bounds: bounds,
            map_overrides: index.map_overrides,
            dir: dir.to_path_buf(),
        })
    }

    /// Properties that have at least one row for `entity_id`, in index order.
    pub fn for_entity(&self, entity_id: &str) -> Vec<(&PropertyData, &[Row])> {
        self.properties
            .iter()
            .filter_map(|p| {
                let rows = p.rows_for(entity_id);
                (!rows.is_empty()).then_some((p, rows))
            })
            .collect()
    }
}

/// Civil date from days since 1970-01-01 (proleptic Gregorian).
pub fn days_to_ymd(days: i32) -> (i32, u32, u32) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// Days since 1970-01-01 for a civil date.
pub fn ymd_to_days(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i32
}

/// Human label for an inclusive day range: `2022`, `2022-03`, or `2022-01-15..2022-02-10`.
pub fn period_label(start: i32, end: i32) -> String {
    let (y0, m0, d0) = days_to_ymd(start);
    let (y1, m1, d1) = days_to_ymd(end);
    if m0 == 1 && d0 == 1 && m1 == 12 && d1 == 31 {
        return if y0 == y1 {
            y0.to_string()
        } else {
            format!("{y0}–{y1}")
        };
    }
    let month_end = ymd_to_days(
        if m1 == 12 { y1 + 1 } else { y1 },
        if m1 == 12 { 1 } else { m1 + 1 },
        1,
    ) - 1;
    if d0 == 1 && end == month_end && y0 == y1 && m0 == m1 {
        return format!("{y0}-{m0:02}");
    }
    format!("{y0}-{m0:02}-{d0:02}..{y1}-{m1:02}-{d1:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_roundtrip() {
        assert_eq!(ymd_to_days(1970, 1, 1), 0);
        assert_eq!(ymd_to_days(2022, 1, 1), 18993);
        assert_eq!(ymd_to_days(2022, 12, 31), 19357);
        for days in [-1000, 0, 18993, 19357, 20000] {
            let (y, m, d) = days_to_ymd(days);
            assert_eq!(ymd_to_days(y, m, d), days);
        }
        assert_eq!(period_label(18993, 19357), "2022");
        assert_eq!(
            period_label(ymd_to_days(2022, 3, 1), ymd_to_days(2022, 3, 31)),
            "2022-03"
        );
        assert_eq!(period_label(0, 10), "1970-01-01..1970-01-11");
    }

    fn encode(
        dtype: u8,
        entities: &[&str],
        strings: &[&str],
        rows: &[(u32, i32, i32, [u8; 8])],
    ) -> Vec<u8> {
        let mut raw = Vec::new();
        for s in entities.iter().chain(strings) {
            raw.extend_from_slice(&(s.len() as u32).to_le_bytes());
            raw.extend_from_slice(s.as_bytes());
        }
        for r in rows {
            raw.extend_from_slice(&r.0.to_le_bytes());
        }
        for r in rows {
            raw.extend_from_slice(&r.1.to_le_bytes());
        }
        for r in rows {
            raw.extend_from_slice(&r.2.to_le_bytes());
        }
        for r in rows {
            if dtype == 2 {
                raw.extend_from_slice(&r.3[..4]);
            } else {
                raw.extend_from_slice(&r.3);
            }
        }
        let payload = zstd::encode_all(raw.as_slice(), 3).unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(b"BMDT");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.push(dtype);
        out.push(0);
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        out.extend_from_slice(&(entities.len() as u32).to_le_bytes());
        out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    fn meta(dtype: DType) -> PropertyMeta {
        PropertyMeta {
            addon: "ibge-core".into(),
            entity_type: "estado".into(),
            name: "População".into(),
            unit: "habitantes".into(),
            dtype,
            periodicity: "anual".into(),
            source: "IBGE".into(),
            source_url: None,
        }
    }

    #[test]
    fn decodes_i64_and_queries_by_entity() {
        let y = |y: i32| (ymd_to_days(y, 1, 1), ymd_to_days(y, 12, 31));
        let rows = [
            (0, y(2021).0, y(2021).1, 100i64.to_le_bytes()),
            (0, y(2022).0, y(2022).1, 110i64.to_le_bytes()),
            (1, y(2022).0, y(2022).1, 5i64.to_le_bytes()),
        ];
        let bytes = encode(0, &["estado:35", "estado:43"], &[], &rows);
        let p =
            PropertyData::decode(&bytes, "ibge-core/estado/populacao", &meta(DType::I64)).unwrap();
        assert_eq!(p.rows.len(), 3);
        let rs = p.rows_for("estado:43");
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].value, Value::I64(5));
        assert_eq!(p.rows_for("estado:35").len(), 2);
        assert!(p.rows_for("estado:99").is_empty());
        assert_eq!(period_label(rs[0].start, rs[0].end), "2022");
    }

    #[test]
    fn decodes_str_and_rejects_bad_dtype() {
        let rows = [(0, 0, 0, [1, 0, 0, 0, 0, 0, 0, 0])];
        let bytes = encode(2, &["pais:BR"], &["a", "b"], &rows);
        let mut m = meta(DType::Str);
        let p = PropertyData::decode(&bytes, "x/pais/y", &m).unwrap();
        assert_eq!(p.value_str(p.rows[0].value), "b");
        m.dtype = DType::I64;
        assert!(PropertyData::decode(&bytes, "x/pais/y", &m).is_err());
    }

    #[test]
    fn loads_store_from_dir() {
        let dir = std::env::temp_dir().join(format!("bramapa-data-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ibge-core/estado")).unwrap();
        std::fs::write(
            dir.join("ibge-core/manifest.toml"),
            r#"
spec_version = 0
[addon]
id = "ibge-core"
name = "IBGE"
version = "0.1.0"
license = "CC-BY-4.0"
[[properties]]
id = "populacao"
entity_type = "estado"
name = "População"
unit = "habitantes"
dtype = "i64"
periodicity = "anual"
source = "IBGE"
file = "data/x.csv"
"#,
        )
        .unwrap();
        let rows = [(0, 18993, 19357, 7i64.to_le_bytes())];
        std::fs::write(
            dir.join("ibge-core/estado/populacao.bmd"),
            encode(0, &["estado:43"], &[], &rows),
        )
        .unwrap();
        std::fs::write(
            dir.join("index.toml"),
            r#"
spec_version = 0
[[addons]]
id = "ibge-core"
version = "0.1.0"
manifest = "ibge-core/manifest.toml"
[[properties]]
id = "ibge-core/estado/populacao"
file = "ibge-core/estado/populacao.bmd"
rows = 1
period_min = "2022"
period_max = "2022"
"#,
        )
        .unwrap();
        let store = DataStore::load(&dir).unwrap();
        assert_eq!(store.properties.len(), 1);
        assert!(store.map_overrides.is_empty());
        assert_eq!(store.period_bounds, Some((18993, 19357)));
        let found = store.for_entity("estado:43");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "População");
        assert!(store.for_entity("estado:35").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
