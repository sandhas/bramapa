//! 2D camera over the world (atlas pixels). Integer zoom only, centre snapped to the pixel
//! grid, so nearest-neighbour sampling never shimmers.

use glam::{Mat4, Vec2};

#[derive(Clone, Debug)]
pub struct Camera {
    /// World point at the centre of the viewport.
    pub center: Vec2,
    /// Integer scale factor (1 = one atlas pixel per physical pixel).
    pub zoom: u32,
    pub viewport: Vec2,
    pub min_zoom: u32,
    pub max_zoom: u32,
}

impl Camera {
    pub fn new(viewport: Vec2) -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 2,
            viewport,
            min_zoom: 1,
            max_zoom: 6,
        }
    }

    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.viewport = Vec2::new(w.max(1.0), h.max(1.0));
    }

    /// Visible world rectangle `(min, max)`.
    pub fn world_rect(&self) -> (Vec2, Vec2) {
        let half = self.viewport / (2.0 * self.zoom as f32);
        (self.center - half, self.center + half)
    }

    /// Move the camera by a delta in screen (physical) pixels.
    pub fn pan_screen(&mut self, dx: f32, dy: f32) {
        self.center -= Vec2::new(dx, dy) / self.zoom as f32;
    }

    pub fn pan_world(&mut self, d: Vec2) {
        self.center += d;
    }

    /// Change zoom by `steps` (+1 = in), keeping the world point under `screen_pos` fixed.
    pub fn zoom_at(&mut self, steps: i32, screen_pos: Vec2) {
        let new_zoom =
            (self.zoom as i32 + steps).clamp(self.min_zoom as i32, self.max_zoom as i32) as u32;
        if new_zoom == self.zoom {
            return;
        }
        let before = self.screen_to_world(screen_pos);
        self.zoom = new_zoom;
        let after = self.screen_to_world(screen_pos);
        self.center += before - after;
    }

    pub fn screen_to_world(&self, p: Vec2) -> Vec2 {
        self.center + (p - self.viewport * 0.5) / self.zoom as f32
    }

    /// Centre snapped so that world pixels land on whole physical pixels.
    fn snapped_center(&self) -> Vec2 {
        let z = self.zoom as f32;
        (self.center * z).round() / z
    }

    /// Orthographic view-projection: world → clip, y down, z passed through by the shader.
    pub fn view_proj(&self) -> Mat4 {
        let c = self.snapped_center();
        let half = self.viewport / (2.0 * self.zoom as f32);
        let w = half.x * 2.0;
        let h = half.y * 2.0;
        Mat4::from_cols_array(&[
            2.0 / w,
            0.0,
            0.0,
            0.0, //
            0.0,
            -2.0 / h,
            0.0,
            0.0, //
            0.0,
            0.0,
            1.0,
            0.0, //
            -2.0 * c.x / w,
            2.0 * c.y / h,
            0.0,
            1.0,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut cam = Camera::new(Vec2::new(800.0, 600.0));
        cam.center = Vec2::new(100.0, 50.0);
        let cursor = Vec2::new(200.0, 100.0);
        let before = cam.screen_to_world(cursor);
        cam.zoom_at(1, cursor);
        let after = cam.screen_to_world(cursor);
        assert!((before - after).length() < 1e-3);
        assert_eq!(cam.zoom, 3);
    }

    #[test]
    fn pan_moves_center_opposite_to_drag() {
        let mut cam = Camera::new(Vec2::new(800.0, 600.0));
        cam.pan_screen(20.0, 0.0);
        assert_eq!(cam.center, Vec2::new(-10.0, 0.0));
    }
}
