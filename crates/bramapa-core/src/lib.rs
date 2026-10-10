//! Bramapa core: file formats (chunk, level, tileset, addon manifest), the data model
//! and loading. No GPU, no windowing. Everything here follows `docs/spec/` (spec v0).

pub mod addon;
pub mod chunk;
pub mod data;
pub mod level;
pub mod synthetic;
pub mod tileset;

/// The spec version this crate reads and writes. See `docs/spec/README.md`.
pub const SPEC_VERSION: u16 = 0;

/// Fixed chunk side (cells) in spec v0.
pub const CHUNK_SIZE: u16 = 64;

pub use chunk::{Chunk, ChunkObject, NO_PERIOD};
pub use data::{DataStore, PropertyData, Row, Value};
pub use level::{EntityDef, Level, LevelManifest};
pub use tileset::{TileClass, TileDef, Tileset};
