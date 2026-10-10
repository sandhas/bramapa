//! egui overlay: stats panel and controls help.

use bramapa_core::data::{days_to_ymd, period_label, ymd_to_days};
use bramapa_core::{DataStore, EntityDef, Level, PropertyData, Row, Tileset, Value};
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
    pub data: Option<&'a DataStore>,
}

/// Mutable UI state that survives between frames.
pub struct UiState {
    /// Selected period range as years (inclusive).
    pub year_start: i32,
    pub year_end: i32,
    /// Bounds offered by the sliders.
    pub year_min: i32,
    pub year_max: i32,
}

impl UiState {
    pub fn new(data: Option<&DataStore>) -> Self {
        let (lo, hi) = data
            .and_then(|d| d.period_bounds)
            .map(|(a, b)| (days_to_ymd(a).0, days_to_ymd(b).0))
            .unwrap_or((2000, 2024));
        Self {
            year_start: lo,
            year_end: hi,
            year_min: lo,
            year_max: hi,
        }
    }
}

/// `1332845` → `1.332.845`; floats keep one decimal with a comma.
pub fn format_number(v: Value) -> String {
    fn group(int: &str) -> String {
        let bytes = int.as_bytes();
        let mut out = String::new();
        for (i, b) in bytes.iter().enumerate() {
            if i > 0 && (bytes.len() - i) % 3 == 0 {
                out.push('.');
            }
            out.push(*b as char);
        }
        out
    }
    match v {
        Value::I64(i) => {
            let s = i.abs().to_string();
            format!("{}{}", if i < 0 { "-" } else { "" }, group(&s))
        }
        Value::F64(f) => {
            let text = format!("{:.1}", f.abs());
            let (int, frac) = text.split_once('.').unwrap_or((&text, "0"));
            format!("{}{},{}", if f < 0.0 { "-" } else { "" }, group(int), frac)
        }
        Value::Str(_) => String::new(),
    }
}

fn row_in_range(r: &Row, year_start: i32, year_end: i32) -> bool {
    let lo = ymd_to_days(year_start, 1, 1);
    let hi = ymd_to_days(year_end, 12, 31);
    r.end >= lo && r.start <= hi
}

fn sparkline(ui: &mut egui::Ui, p: &PropertyData, rows: &[&Row]) {
    let values: Vec<f64> = rows
        .iter()
        .filter_map(|r| match r.value {
            Value::I64(i) => Some(i as f64),
            Value::F64(f) => Some(f),
            Value::Str(_) => None,
        })
        .collect();
    if values.len() < 2 {
        return;
    }
    let (w, h) = (ui.available_width().min(320.0), 44.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let (min, max) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let span = if max > min { max - min } else { 1.0 };
    let pts: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / (values.len() - 1) as f32;
            let y = rect.bottom() - 2.0 - (rect.height() - 4.0) * ((v - min) / span) as f32;
            egui::pos2(x, y)
        })
        .collect();
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(60));
    painter.add(egui::Shape::line(
        pts.clone(),
        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 205, 90)),
    ));
    if let Some(last) = pts.last() {
        painter.circle_filled(*last, 2.5, egui::Color32::from_rgb(255, 205, 90));
    }
    let _ = p;
}

fn entity_panel(ctx: &egui::Context, m: &Model, st: &mut UiState) {
    let Some(entity) = m.selected_entity else {
        return;
    };
    egui::Window::new(format!("{} · {}", entity.name, entity.id))
        .id(egui::Id::new("entity-panel"))
        .default_pos([12.0, 360.0])
        .default_width(360.0)
        .show(ctx, |ui| {
            let Some(data) = m.data else {
                ui.label("Sem dados carregados (use --data <dir de build>).");
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Período");
                ui.add(egui::DragValue::new(&mut st.year_start).range(st.year_min..=st.year_max));
                ui.label("a");
                ui.add(egui::DragValue::new(&mut st.year_end).range(st.year_min..=st.year_max));
                if st.year_end < st.year_start {
                    st.year_end = st.year_start;
                }
                if ui.small_button("tudo").clicked() {
                    st.year_start = st.year_min;
                    st.year_end = st.year_max;
                }
            });
            ui.add(egui::Slider::new(&mut st.year_start, st.year_min..=st.year_max).text("de"));
            ui.add(egui::Slider::new(&mut st.year_end, st.year_start..=st.year_max).text("até"));
            ui.separator();
            let props = data.for_entity(&entity.id);
            if props.is_empty() {
                ui.label("Nenhuma propriedade para esta entidade.");
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    for (p, rows) in props {
                        let rows: Vec<&Row> = rows
                            .iter()
                            .filter(|r| row_in_range(r, st.year_start, st.year_end))
                            .collect();
                        let header = if p.unit.is_empty() {
                            p.name.clone()
                        } else {
                            format!("{} ({})", p.name, p.unit)
                        };
                        egui::CollapsingHeader::new(header)
                            .id_salt(&p.id)
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.small(format!("{} · {} · {}", p.id, p.periodicity, p.source));
                                if rows.is_empty() {
                                    ui.label("sem valores no período");
                                    return;
                                }
                                if let Some(last) = rows.last() {
                                    ui.horizontal(|ui| {
                                        ui.strong(format_number(last.value));
                                        ui.label(format!(
                                            "em {}",
                                            period_label(last.start, last.end)
                                        ));
                                    });
                                }
                                sparkline(ui, p, &rows);
                                egui::CollapsingHeader::new(format!("{} períodos", rows.len()))
                                    .id_salt((&p.id, "rows"))
                                    .show(ui, |ui| {
                                        egui::Grid::new((&p.id, "grid"))
                                            .num_columns(2)
                                            .striped(true)
                                            .show(ui, |ui| {
                                                for r in rows.iter().rev() {
                                                    ui.label(period_label(r.start, r.end));
                                                    ui.label(match r.value {
                                                        Value::Str(_) => p.value_str(r.value),
                                                        v => format_number(v),
                                                    });
                                                    ui.end_row();
                                                }
                                            });
                                    });
                            });
                    }
                });
        });
}

pub fn draw(ctx: &egui::Context, m: &Model, st: &mut UiState) {
    if !m.show {
        return;
    }
    entity_panel(ctx, m, st);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_pt_br() {
        assert_eq!(format_number(Value::I64(1_332_845)), "1.332.845");
        assert_eq!(format_number(Value::I64(-42)), "-42");
        assert_eq!(format_number(Value::I64(999)), "999");
        assert_eq!(format_number(Value::F64(281_707.3)), "281.707,3");
        assert_eq!(
            format_number(Value::F64(0.96)),
            "1,0".replace("1,0", "0,10").replace("0,10", "1,0")
        );
    }

    #[test]
    fn year_range_filter_is_inclusive() {
        let r = Row {
            entity_idx: 0,
            start: ymd_to_days(2022, 1, 1),
            end: ymd_to_days(2022, 12, 31),
            value: Value::I64(1),
        };
        assert!(row_in_range(&r, 2022, 2022));
        assert!(row_in_range(&r, 2000, 2022));
        assert!(!row_in_range(&r, 2023, 2024));
    }
}
