//! egui overlay: stats panel and controls help.

use bramapa_core::{EntityDef, Level, Tileset};
use bramapa_render::{Camera, RenderStats};

pub struct FpsCounter {
    samples: [f32; 60],
    idx: usize,
    filled: usize,
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            samples: [0.0; 60],
            idx: 0,
            filled: 0,
        }
    }
}

impl FpsCounter {
    pub fn tick(&mut self, dt: f32) {
        self.samples[self.idx] = dt;
        self.idx = (self.idx + 1) % self.samples.len();
        self.filled = (self.filled + 1).min(self.samples.len());
    }
    pub fn frame_ms(&self) -> f32 {
        if self.filled == 0 {
            return 0.0;
        }
        self.samples[..self.filled].iter().sum::<f32>() / self.filled as f32 * 1000.0
    }
    pub fn fps(&self) -> f32 {
        let ms = self.frame_ms();
        if ms > 0.0 {
            1000.0 / ms
        } else {
            0.0
        }
    }
}

pub struct CursorCell {
    pub x: u32,
    pub y: u32,
    pub terrain: u16,
    pub overlay: u16,
    pub entity: u16,
}

pub struct Model<'a> {
    pub fps: f32,
    pub frame_ms: f32,
    pub camera: &'a Camera,
    pub stats: RenderStats,
    pub level: &'a Level,
    pub tileset: &'a Tileset,
    pub level_source: &'a str,
    pub tileset_source: &'a str,
    pub cursor_cell: Option<CursorCell>,
    pub hover_entity: Option<&'a EntityDef>,
    pub selected_entity: Option<&'a EntityDef>,
    pub adapter: &'a str,
    pub show: bool,
}

pub fn draw(ctx: &egui::Context, m: &Model) {
    if !m.show {
        return;
    }
    egui::Window::new("Bramapa")
        .default_pos([12.0, 12.0])
        .resizable(false)
        .show(ctx, |ui| {
            let g = &m.level.manifest.grid;
            egui::Grid::new("stats")
                .num_columns(2)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    ui.label("FPS");
                    ui.label(format!("{:.0} ({:.2} ms)", m.fps, m.frame_ms));
                    ui.end_row();
                    ui.label("GPU");
                    ui.label(m.adapter);
                    ui.end_row();
                    ui.label("Nível");
                    ui.label(format!(
                        "{} · {}×{} células · {}",
                        m.level.manifest.level.name,
                        g.width,
                        g.height,
                        m.level.manifest.level.entity
                    ));
                    ui.end_row();
                    ui.label("Fonte");
                    ui.label(m.level_source);
                    ui.end_row();
                    ui.label("Tileset");
                    ui.label(format!(
                        "{} v{} · {} tiles · {}",
                        m.tileset.tileset.id,
                        m.tileset.tileset.version,
                        m.tileset.tiles.len(),
                        m.tileset_source
                    ));
                    ui.end_row();
                    ui.label("Chunks");
                    ui.label(format!(
                        "{} visíveis / {}",
                        m.stats.chunks_drawn, m.stats.chunks_total
                    ));
                    ui.end_row();
                    ui.label("Draw calls");
                    ui.label(format!("{}", m.stats.draw_calls));
                    ui.end_row();
                    ui.label("Instâncias");
                    ui.label(format!(
                        "{} / {}",
                        m.stats.instances_drawn, m.stats.instances_total
                    ));
                    ui.end_row();
                    if m.stats.unknown_tile_ids > 0 {
                        ui.label("Ids desconhecidos");
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("{}", m.stats.unknown_tile_ids),
                        );
                        ui.end_row();
                    }
                    ui.label("Câmera");
                    ui.label(format!(
                        "({:.0}, {:.0}) · zoom {}",
                        m.camera.center.x,
                        m.camera.center.y,
                        m.camera.zoom_label()
                    ));
                    ui.end_row();
                    ui.label("Célula");
                    match &m.cursor_cell {
                        Some(c) => {
                            let name =
                                |id: u16| m.tileset.get(id).map(|t| t.name.as_str()).unwrap_or("?");
                            ui.label(format!(
                                "({}, {}) · terreno {} {} · overlay {} {}",
                                c.x,
                                c.y,
                                c.terrain,
                                name(c.terrain),
                                c.overlay,
                                name(c.overlay)
                            ));
                        }
                        None => {
                            ui.label("fora da grade");
                        }
                    }
                    ui.end_row();
                    ui.label("Entidade");
                    match m.hover_entity {
                        Some(e) => {
                            ui.label(format!("{} · {}", e.name, e.id));
                        }
                        None => {
                            ui.label("—");
                        }
                    }
                    ui.end_row();
                    ui.label("Selecionada");
                    match m.selected_entity {
                        Some(e) => {
                            ui.strong(format!("{} · {}", e.name, e.id));
                        }
                        None => {
                            ui.label("clique numa entidade");
                        }
                    }
                    ui.end_row();
                });
            ui.separator();
            ui.small(
                "Arrastar: mover · Roda/+/-: zoom · Setas/WASD: mover · F1: esconder · Esc: sair",
            );
        });
}
