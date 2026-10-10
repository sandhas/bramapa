//! wgpu instance/surface/device plus depth buffer and a screenshot readback helper.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use winit::window::Window;

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub depth_view: wgpu::TextureView,
    pub adapter_info: String,
    can_copy: bool,
}

pub enum Acquire {
    /// Surface was lost/outdated and has been reconfigured; try again next frame.
    Reconfigured,
    /// Nothing to draw this frame (timeout/occluded).
    Skip,
}

pub struct Readback {
    buffer: wgpu::Buffer,
    bytes_per_row: u32,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

impl Gpu {
    pub async fn new(window: Arc<Window>, vsync: bool) -> Result<Self> {
        // Backends from WGPU_BACKEND if set, else Vulkan/Metal/DX12 with GL as fallback.
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if wgpu::Backends::from_env().is_none() {
            desc.backends = wgpu::Backends::PRIMARY | wgpu::Backends::GL;
        }
        let instance = wgpu::Instance::new(desc);
        let surface = instance
            .create_surface(window.clone())
            .context("creating surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .context("no suitable GPU adapter")?;
        let info = adapter.get_info();
        let adapter_info = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        log::info!("adapter: {adapter_info}");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("bramapa"),
                required_features: wgpu::Features::empty(),
                // downlevel defaults = what GLES 3.0 / old Android phones can do
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .context("requesting device")?;

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow!("surface not supported by adapter"))?;
        // Prefer a non-sRGB format so atlas colours and egui colours are passed through as authored.
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        let can_copy = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        if can_copy {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&device, &config);
        let depth_view = create_depth(&device, config.width, config.height);
        log::info!(
            "surface {}x{} {:?} {:?}",
            config.width,
            config.height,
            config.format,
            config.present_mode
        );
        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_view,
            adapter_info,
            can_copy,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth(&self.device, width, height);
    }

    pub fn acquire(&mut self) -> std::result::Result<wgpu::SurfaceTexture, Acquire> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => Ok(t),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => Ok(t),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Err(Acquire::Skip)
            }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                self.surface.configure(&self.device, &self.config);
                Err(Acquire::Reconfigured)
            }
        }
    }

    /// Schedule a copy of `texture` into a mappable buffer (resolved by `save_readback`).
    pub fn copy_to_buffer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> Option<Readback> {
        if !self.can_copy {
            log::warn!("surface does not support COPY_SRC; screenshot unavailable");
            return None;
        }
        let (width, height) = (self.config.width, self.config.height);
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot"),
            size: bytes_per_row as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Some(Readback {
            buffer,
            bytes_per_row,
            width,
            height,
            format: self.config.format,
        })
    }

    pub fn save_readback(&self, rb: Option<Readback>, path: &Path) -> Result<()> {
        let rb = rb.ok_or_else(|| anyhow!("no readback"))?;
        let slice = rb.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| anyhow!("poll: {e:?}"))?;
        rx.recv()
            .context("map callback")?
            .map_err(|e| anyhow!("map: {e:?}"))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| anyhow!("mapped range: {e:?}"))?;
        let mut rgba = Vec::with_capacity((rb.width * rb.height * 4) as usize);
        let bgra = matches!(
            rb.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        for y in 0..rb.height {
            let row = &data[(y * rb.bytes_per_row) as usize..][..(rb.width * 4) as usize];
            for px in row.chunks_exact(4) {
                if bgra {
                    rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
                } else {
                    rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
        }
        drop(data);
        rb.buffer.unmap();
        image::save_buffer(path, &rgba, rb.width, rb.height, image::ColorType::Rgba8)
            .context("writing png")?;
        Ok(())
    }
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: bramapa_render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}
