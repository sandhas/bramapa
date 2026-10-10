//! `.bmc` chunk codec. Byte layout is defined in `docs/spec/chunk.md`.

use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

use crate::{CHUNK_SIZE, SPEC_VERSION};

const MAGIC: &[u8; 4] = b"BMCK";
const HEADER_LEN: usize = 32;
const FLAG_HAS_OBJECTS: u16 = 1;

/// Sentinel for "no period" in `period_start`/`period_end` (days since 1970-01-01).
pub const NO_PERIOD: i32 = i32::MIN;

/// Object layer entry: a sprite anchored to one cell of the chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkObject {
    pub cell_x: u8,
    pub cell_y: u8,
    pub tile_id: u16,
    /// bit 0 = mirrored horizontally.
    pub flags: u16,
}

impl ChunkObject {
    pub const FLAG_MIRROR: u16 = 1;
}

/// One chunk: `size × size` cells with a terrain layer, an overlay layer and optional objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub cx: i32,
    pub cy: i32,
    pub size: u16,
    /// `size * size` tile ids, row-major (`y` outer, `x` inner). 0 = empty.
    pub terrain: Vec<u16>,
    pub overlay: Vec<u16>,
    pub objects: Vec<ChunkObject>,
    pub period_start: i32,
    pub period_end: i32,
}

impl Chunk {
    pub fn empty(cx: i32, cy: i32) -> Self {
        let n = CHUNK_SIZE as usize * CHUNK_SIZE as usize;
        Self {
            cx,
            cy,
            size: CHUNK_SIZE,
            terrain: vec![0; n],
            overlay: vec![0; n],
            objects: Vec::new(),
            period_start: NO_PERIOD,
            period_end: NO_PERIOD,
        }
    }

    #[inline]
    pub fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.size as usize + x as usize
    }

    pub fn has_period(&self) -> bool {
        self.period_start != NO_PERIOD
    }

    /// Serialize to the `.bmc` byte format.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let n = self.size as usize * self.size as usize;
        ensure!(
            self.terrain.len() == n,
            "terrain layer has {} cells, expected {n}",
            self.terrain.len()
        );
        ensure!(
            self.overlay.len() == n,
            "overlay layer has {} cells, expected {n}",
            self.overlay.len()
        );

        let has_objects = !self.objects.is_empty();
        let mut raw = Vec::with_capacity(n * 4 + self.objects.len() * 8 + 4);
        for v in &self.terrain {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        for v in &self.overlay {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        if has_objects {
            raw.extend_from_slice(&(self.objects.len() as u32).to_le_bytes());
            for o in &self.objects {
                raw.push(o.cell_x);
                raw.push(o.cell_y);
                raw.extend_from_slice(&o.tile_id.to_le_bytes());
                raw.extend_from_slice(&o.flags.to_le_bytes());
                raw.extend_from_slice(&0u16.to_le_bytes());
            }
        }
        let payload = zstd::encode_all(raw.as_slice(), 3).context("zstd encode")?;

        let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&SPEC_VERSION.to_le_bytes());
        out.extend_from_slice(&(if has_objects { FLAG_HAS_OBJECTS } else { 0 }).to_le_bytes());
        out.extend_from_slice(&self.size.to_le_bytes());
        out.push(if has_objects { 3 } else { 2 });
        out.push(0);
        out.extend_from_slice(&self.cx.to_le_bytes());
        out.extend_from_slice(&self.cy.to_le_bytes());
        out.extend_from_slice(&self.period_start.to_le_bytes());
        out.extend_from_slice(&self.period_end.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        debug_assert_eq!(out.len(), HEADER_LEN);
        out.extend_from_slice(&payload);
        Ok(out)
    }

    /// Parse a `.bmc` byte buffer.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() >= HEADER_LEN,
            "chunk too short ({} bytes)",
            bytes.len()
        );
        ensure!(&bytes[0..4] == MAGIC, "bad magic");
        let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
        let i32_at =
            |o: usize| i32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let version = u16_at(4);
        ensure!(
            version == SPEC_VERSION,
            "unsupported chunk spec_version {version}"
        );
        let flags = u16_at(6);
        let size = u16_at(8);
        ensure!(size == CHUNK_SIZE, "unsupported chunk_size {size}");
        let layer_count = bytes[10];
        let has_objects = flags & FLAG_HAS_OBJECTS != 0;
        ensure!(
            layer_count == if has_objects { 3 } else { 2 },
            "layer_count {layer_count} inconsistent with flags {flags:#x}"
        );
        let cx = i32_at(12);
        let cy = i32_at(16);
        let period_start = i32_at(20);
        let period_end = i32_at(24);
        let payload_len = i32_at(28) as u32 as usize;
        ensure!(
            bytes.len() == HEADER_LEN + payload_len,
            "payload_len {payload_len} does not match file size {}",
            bytes.len() - HEADER_LEN
        );
        let raw = zstd::decode_all(&bytes[HEADER_LEN..]).context("zstd decode")?;

        let n = size as usize * size as usize;
        let mut need = n * 4;
        ensure!(raw.len() >= need, "payload too short for two layers");
        let read_layer = |off: usize| -> Vec<u16> {
            raw[off..off + n * 2]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect()
        };
        let terrain = read_layer(0);
        let overlay = read_layer(n * 2);
        let mut objects = Vec::new();
        if has_objects {
            ensure!(raw.len() >= need + 4, "missing object count");
            let count = u32::from_le_bytes([raw[need], raw[need + 1], raw[need + 2], raw[need + 3]])
                as usize;
            need += 4;
            ensure!(raw.len() >= need + count * 8, "object table truncated");
            objects.reserve(count);
            for i in 0..count {
                let o = need + i * 8;
                let cell_x = raw[o];
                let cell_y = raw[o + 1];
                if cell_x as u16 >= size || cell_y as u16 >= size {
                    bail!("object {i} outside chunk ({cell_x},{cell_y})");
                }
                objects.push(ChunkObject {
                    cell_x,
                    cell_y,
                    tile_id: u16::from_le_bytes([raw[o + 2], raw[o + 3]]),
                    flags: u16::from_le_bytes([raw[o + 4], raw[o + 5]]),
                });
            }
        }
        Ok(Self {
            cx,
            cy,
            size,
            terrain,
            overlay,
            objects,
            period_start,
            period_end,
        })
    }

    pub fn read_file(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::decode(&bytes).with_context(|| format!("decoding {}", path.display()))
    }

    pub fn write_file(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.encode()?).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Chunk {
        let mut c = Chunk::empty(3, -2);
        for y in 0..64u16 {
            for x in 0..64u16 {
                let i = c.idx(x, y);
                c.terrain[i] = (x + y) % 7;
                c.overlay[i] = if x == y { 20 } else { 0 };
            }
        }
        c.objects.push(ChunkObject {
            cell_x: 5,
            cell_y: 9,
            tile_id: 40,
            flags: 1,
        });
        c.objects.push(ChunkObject {
            cell_x: 63,
            cell_y: 0,
            tile_id: 41,
            flags: 0,
        });
        c.period_start = 19358; // 2023-01-01
        c.period_end = 19722; // 2023-12-31
        c
    }

    #[test]
    fn roundtrip_with_objects_and_period() {
        let c = sample();
        let bytes = c.encode().unwrap();
        assert_eq!(&bytes[0..4], b"BMCK");
        assert_eq!(bytes[4], 0); // spec_version lo
        assert_eq!(bytes[6] & 1, 1); // has objects
        assert_eq!(bytes[8], 64);
        assert_eq!(bytes[10], 3);
        assert_eq!(i32::from_le_bytes(bytes[12..16].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(bytes[16..20].try_into().unwrap()), -2);
        let d = Chunk::decode(&bytes).unwrap();
        assert_eq!(c, d);
    }

    #[test]
    fn roundtrip_empty() {
        let c = Chunk::empty(0, 0);
        let bytes = c.encode().unwrap();
        assert_eq!(bytes[10], 2);
        assert_eq!(Chunk::decode(&bytes).unwrap(), c);
        assert!(!c.has_period());
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        let mut bytes = sample().encode().unwrap();
        assert!(Chunk::decode(&bytes[..20]).is_err());
        bytes[0] = b'X';
        assert!(Chunk::decode(&bytes).is_err());
    }
}
