//! Bramapa render: isometric tilemap renderer on wgpu. One instanced draw call per
//! chunk per layer; animation and depth are computed in the shader.

pub mod atlas;
pub mod camera;
pub mod iso;
pub mod tilemap;

pub use atlas::{AtlasTexture, PlaceholderAtlas};
pub use camera::Camera;
pub use tilemap::{RenderStats, TilemapRenderer};

/// Depth format used by the tilemap pipeline (the app creates the depth texture).
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
