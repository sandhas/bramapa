//! Bramapa app: window, GPU surface, input, egui overlay. Desktop and Android.

mod gpu;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use bramapa_core::{Level, Tileset};
use bramapa_render::{AtlasTexture, Camera, PlaceholderAtlas, TilemapRenderer};
use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

pub use gpu::Gpu;

#[derive(Clone, Debug)]
pub struct Options {
    pub level_dir: Option<PathBuf>,
    pub tileset_dir: Option<PathBuf>,
    pub synthetic_size: (u32, u32),
    /// Initial camera centre in cell coordinates (default: grid centre).
    pub center: Option<(f32, f32)>,
    /// Initial integer zoom.
    pub zoom: u32,
    pub exit_after_frames: Option<u32>,
    /// Vsync on (default). Off only for benchmarking.
    pub vsync: bool,
    pub show_ui: bool,
    pub fullscreen: bool,
    pub screenshot: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            level_dir: None,
            tileset_dir: None,
            synthetic_size: (512, 512),
            center: None,
            zoom: 1,
            exit_after_frames: None,
            vsync: true,
            show_ui: true,
            fullscreen: false,
            screenshot: None,
        }
    }
}

/// Everything loaded from disk (or generated) before the window exists.
struct Content {
    level: Level,
    tileset: Tileset,
    /// RGBA atlas pixels when using the embedded placeholder (None = load from tileset dir).
    placeholder: Option<(u32, u32, Vec<u8>)>,
    level_source: String,
    tileset_source: String,
}

fn load_content(opts: &Options) -> Result<Content> {
    let (level, level_source) = match &opts.level_dir {
        Some(dir) => (
            Level::load(dir).with_context(|| format!("loading level {}", dir.display()))?,
            dir.display().to_string(),
        ),
        None => {
            let (w, h) = opts.synthetic_size;
            (
                bramapa_core::synthetic::level(w, h, 42),
                format!("sintético {w}x{h}"),
            )
        }
    };
    let (tileset, placeholder, tileset_source) = match &opts.tileset_dir {
        Some(dir) => (
            Tileset::load(dir).with_context(|| format!("loading tileset {}", dir.display()))?,
            None,
            dir.display().to_string(),
        ),
        None => {
            let p = PlaceholderAtlas::generate();
            (
                p.tileset,
                Some((p.width, p.height, p.rgba)),
                "placeholder embutido".to_string(),
            )
        }
    };
    log::info!(
        "level '{}' ({}x{} cells, {} chunks) with tileset '{}' v{} ({} tiles)",
        level.manifest.level.name,
        level.manifest.grid.width,
        level.manifest.grid.height,
        level.chunks.len(),
        tileset.tileset.id,
        tileset.tileset.version,
        tileset.tiles.len()
    );
    Ok(Content {
        level,
        tileset,
        placeholder,
        level_source,
        tileset_source,
    })
}

/// Per-window GPU state: created on `resumed`, dropped on `suspended` (Android).
struct Scene {
    window: Arc<Window>,
    gpu: Gpu,
    renderer: TilemapRenderer,
    atlas: AtlasTexture,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

#[derive(Default)]
struct Input {
    dragging: bool,
    last_cursor: Option<Vec2>,
    cursor: Vec2,
    touch_id: Option<u64>,
    keys: KeyState,
}

#[derive(Default)]
struct KeyState {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

pub struct App {
    opts: Options,
    content: Content,
    scene: Option<Scene>,
    camera: Camera,
    input: Input,
    start: Instant,
    last_frame: Instant,
    frame: u32,
    fps: ui::FpsCounter,
    show_ui: bool,
}

impl App {
    fn new(opts: Options) -> Result<Self> {
        let content = load_content(&opts)?;
        let mut camera = Camera::new(Vec2::new(1280.0, 720.0));
        let g = &content.level.manifest.grid;
        let (cx, cy) = opts
            .center
            .unwrap_or((g.width as f32 * 0.5, g.height as f32 * 0.5));
        camera.center = bramapa_render::iso::IsoGeometry::DEFAULT.cell_center(cx, cy);
        camera.zoom = opts.zoom.clamp(camera.min_zoom, camera.max_zoom);
        let show_ui = opts.show_ui;
        Ok(Self {
            opts,
            content,
            scene: None,
            camera,
            input: Input::default(),
            start: Instant::now(),
            last_frame: Instant::now(),
            frame: 0,
            fps: ui::FpsCounter::default(),
            show_ui,
        })
    }

    fn create_scene(&mut self, event_loop: &ActiveEventLoop) -> Result<Scene> {
        let mut attrs = Window::default_attributes()
            .with_title("Bramapa")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        if self.opts.fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        let window = Arc::new(event_loop.create_window(attrs)?);
        let gpu = pollster::block_on(Gpu::new(window.clone(), self.opts.vsync))?;
        let size = window.inner_size();
        self.camera
            .set_viewport(size.width as f32, size.height as f32);

        let mut renderer =
            TilemapRenderer::new(&gpu.device, gpu.config.format, bramapa_render::DEPTH_FORMAT);
        let atlas = match &self.content.placeholder {
            Some((w, h, rgba)) => AtlasTexture::from_rgba(
                &gpu.device,
                &gpu.queue,
                &renderer.atlas_layout,
                *w,
                *h,
                rgba,
            ),
            None => {
                let path = self
                    .content
                    .tileset
                    .atlas_path()
                    .context("tileset without directory")?;
                AtlasTexture::from_png(&gpu.device, &gpu.queue, &renderer.atlas_layout, &path)?
            }
        };
        let t = Instant::now();
        renderer.upload_level(
            &gpu.device,
            &self.content.level,
            &self.content.tileset,
            &atlas,
        );
        log::info!(
            "uploaded {} chunks, {} instances in {:.1} ms ({} unknown tile ids)",
            renderer.stats.chunks_total,
            renderer.stats.instances_total,
            t.elapsed().as_secs_f32() * 1000.0,
            renderer.stats.unknown_tile_ids
        );

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        Ok(Scene {
            window,
            gpu,
            renderer,
            atlas,
            egui_ctx,
            egui_state,
            egui_renderer,
        })
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.fps.tick(dt);
        self.apply_keys(dt);

        let cursor_cell = self.cursor_cell();
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        let frame_tex = match scene.gpu.acquire() {
            Ok(t) => t,
            Err(gpu::Acquire::Reconfigured) => {
                scene.window.request_redraw();
                return;
            }
            Err(gpu::Acquire::Skip) => return,
        };
        let view = frame_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            scene
                .gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                });

        // --- map pass ---
        let time = now.duration_since(self.start).as_secs_f32();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("map"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.09,
                            b: 0.2,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &scene.gpu.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            scene.renderer.render(
                &scene.gpu.queue,
                &mut pass,
                &self.camera,
                &scene.atlas,
                time,
            );
        }

        // --- egui pass ---
        let raw_input = scene.egui_state.take_egui_input(&scene.window);
        let ui_model = ui::Model {
            fps: self.fps.fps(),
            frame_ms: self.fps.frame_ms(),
            camera: &self.camera,
            stats: scene.renderer.stats,
            level: &self.content.level,
            tileset: &self.content.tileset,
            level_source: &self.content.level_source,
            tileset_source: &self.content.tileset_source,
            cursor_cell,
            adapter: &scene.gpu.adapter_info,
            show: self.show_ui,
        };
        let mut full_output = scene
            .egui_ctx
            .run_ui(raw_input, |ui| ui::draw(ui.ctx(), &ui_model));
        scene
            .egui_state
            .handle_platform_output(&scene.window, full_output.platform_output);
        let primitives = scene
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [scene.gpu.config.width, scene.gpu.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };
        for (id, deltas) in full_output.textures_delta.set.drain() {
            for delta in deltas {
                scene
                    .egui_renderer
                    .update_texture(&scene.gpu.device, &scene.gpu.queue, id, &delta);
            }
        }
        let egui_cmds = scene.egui_renderer.update_buffers(
            &scene.gpu.device,
            &scene.gpu.queue,
            &mut encoder,
            &primitives,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            scene.egui_renderer.render(&mut pass, &primitives, &screen);
        }
        for id in full_output.textures_delta.free.drain() {
            scene.egui_renderer.free_texture(&id);
        }

        // --- optional screenshot on the last frame ---
        let is_last = self
            .opts
            .exit_after_frames
            .is_some_and(|n| self.frame + 1 >= n);
        let readback = if is_last && self.opts.screenshot.is_some() {
            Some(scene.gpu.copy_to_buffer(&mut encoder, &frame_tex.texture))
        } else {
            None
        };

        scene.gpu.queue.submit(
            egui_cmds
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        scene.window.pre_present_notify();
        scene.gpu.queue.present(frame_tex);

        if let Some(rb) = readback {
            let path = self.opts.screenshot.clone().unwrap();
            match scene.gpu.save_readback(rb, &path) {
                Ok(()) => log::info!("screenshot saved to {}", path.display()),
                Err(e) => log::error!("screenshot failed: {e:#}"),
            }
        }
        self.frame += 1;
        if self.frame % 300 == 0 {
            // Periodic line for logcat / terminal benchmarking (every ~5 s at 60 fps).
            log::info!(
                "frame {}: {:.1} fps ({:.2} ms), {} chunks, {} draw calls, {} instances, zoom {}x",
                self.frame,
                self.fps.fps(),
                self.fps.frame_ms(),
                scene.renderer.stats.chunks_drawn,
                scene.renderer.stats.draw_calls,
                scene.renderer.stats.instances_drawn,
                self.camera.zoom
            );
        }
        if is_last {
            log::info!(
                "exiting after {} frames (avg {:.1} fps)",
                self.frame,
                self.fps.fps()
            );
            event_loop.exit();
        }
    }

    fn apply_keys(&mut self, dt: f32) {
        let k = &self.input.keys;
        let speed = 600.0 * dt; // world px per second
        let mut d = Vec2::ZERO;
        if k.left {
            d.x -= speed;
        }
        if k.right {
            d.x += speed;
        }
        if k.up {
            d.y -= speed;
        }
        if k.down {
            d.y += speed;
        }
        if d != Vec2::ZERO {
            self.camera.pan_world(d / self.camera.zoom as f32 * 2.0);
        }
    }

    /// Cell under the cursor and the tile ids there, if inside the grid.
    fn cursor_cell(&self) -> Option<ui::CursorCell> {
        let scene = self.scene.as_ref()?;
        let g = scene.renderer.geometry();
        let world = self.camera.screen_to_world(self.input.cursor);
        let c = g.world_to_cell(world);
        let grid = &self.content.level.manifest.grid;
        if c.x < 0.0 || c.y < 0.0 || c.x >= grid.width as f32 || c.y >= grid.height as f32 {
            return None;
        }
        let (x, y) = (c.x as u32, c.y as u32);
        let cs = grid.chunk_size as u32;
        let chunk = self
            .content
            .level
            .chunks
            .get(&((x / cs) as i32, (y / cs) as i32));
        let (terrain, overlay) = chunk
            .map(|ch| {
                let i = ch.idx((x % cs) as u16, (y % cs) as u16);
                (ch.terrain[i], ch.overlay[i])
            })
            .unwrap_or((0, 0));
        Some(ui::CursorCell {
            x,
            y,
            terrain,
            overlay,
        })
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.scene.is_some() {
            return;
        }
        match self.create_scene(event_loop) {
            Ok(scene) => {
                scene.window.request_redraw();
                self.scene = Some(scene);
            }
            Err(e) => {
                log::error!("failed to create window/GPU: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android: the surface is gone; rebuild everything on the next `resumed`.
        self.scene = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        let response = scene.egui_state.on_window_event(&scene.window, &event);
        if response.repaint {
            scene.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                scene.gpu.resize(size.width, size.height);
                self.camera
                    .set_viewport(size.width as f32, size.height as f32);
            }
            WindowEvent::RedrawRequested => self.render(event_loop),
            _ if response.consumed => {}
            WindowEvent::CursorMoved { position, .. } => {
                let p = Vec2::new(position.x as f32, position.y as f32);
                if self.input.dragging {
                    if let Some(last) = self.input.last_cursor {
                        let d = p - last;
                        self.camera.pan_screen(d.x, d.y);
                    }
                    self.input.last_cursor = Some(p);
                }
                self.input.cursor = p;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left | MouseButton::Middle,
                ..
            } => {
                self.input.dragging = state == ElementState::Pressed;
                self.input.last_cursor = self.input.dragging.then_some(self.input.cursor);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.signum() as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y.signum() as i32,
                };
                self.camera.zoom_at(steps, self.input.cursor);
            }
            WindowEvent::Touch(touch) => {
                let p = Vec2::new(touch.location.x as f32, touch.location.y as f32);
                match touch.phase {
                    TouchPhase::Started if self.input.touch_id.is_none() => {
                        self.input.touch_id = Some(touch.id);
                        self.input.last_cursor = Some(p);
                        self.input.cursor = p;
                    }
                    TouchPhase::Moved if self.input.touch_id == Some(touch.id) => {
                        if let Some(last) = self.input.last_cursor {
                            let d = p - last;
                            self.camera.pan_screen(d.x, d.y);
                        }
                        self.input.last_cursor = Some(p);
                        self.input.cursor = p;
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled
                        if self.input.touch_id == Some(touch.id) =>
                    {
                        self.input.touch_id = None;
                        self.input.last_cursor = None;
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowLeft | KeyCode::KeyA) => {
                        self.input.keys.left = pressed
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight | KeyCode::KeyD) => {
                        self.input.keys.right = pressed
                    }
                    PhysicalKey::Code(KeyCode::ArrowUp | KeyCode::KeyW) => {
                        self.input.keys.up = pressed
                    }
                    PhysicalKey::Code(KeyCode::ArrowDown | KeyCode::KeyS) => {
                        self.input.keys.down = pressed
                    }
                    PhysicalKey::Code(KeyCode::Equal | KeyCode::NumpadAdd) if pressed => {
                        self.camera.zoom_at(1, self.camera.viewport * 0.5)
                    }
                    PhysicalKey::Code(KeyCode::Minus | KeyCode::NumpadSubtract) if pressed => {
                        self.camera.zoom_at(-1, self.camera.viewport * 0.5)
                    }
                    PhysicalKey::Code(KeyCode::F1) if pressed => self.show_ui = !self.show_ui,
                    PhysicalKey::Code(KeyCode::Escape) if pressed => event_loop.exit(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Continuous redraw: water/trees animate in the shader. (On-demand mode is a later
        // battery optimisation for mobile.)
        if let Some(scene) = &self.scene {
            scene.window.request_redraw();
        }
    }
}

/// Run the app on the given event loop (desktop `main` and Android `android_main`).
pub fn run(event_loop: EventLoop<()>, opts: Options) -> Result<()> {
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(opts)?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("bramapa"),
    );
    let event_loop = EventLoop::builder()
        .with_android_app(android_app)
        .build()
        .expect("event loop");
    if let Err(e) = run(event_loop, Options::default()) {
        log::error!("bramapa exited with error: {e:#}");
    }
}
