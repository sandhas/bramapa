//! 2D camera over the world (atlas pixels). Zoom moves along a fixed ladder: integer factors
//! when zoomed in (pixel-perfect, centre snapped to the pixel grid) and powers-of-two fractions
//! when zoomed out (the atlas is mipmapped, so the overview reads as a smooth minimap).

use glam::{Mat4, Vec2};

/// Zoom ladder, from the widest overview to the closest pixel-art view.
pub const ZOOM_STEPS: [f32; 11] = [
    1.0 / 32.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 2.0,
    1.0,
    2.0,
    3.0,
    4.0,
    5.0,
    6.0,
];

#[derive(Clone, Debug)]
pub struct Camera {
    /// World point at the centre of the viewport.
    pub center: Vec2,
    /// Scale factor: physical pixels per atlas pixel. Always one of `ZOOM_STEPS`.
    pub zoom: f32,
    pub viewport: Vec2,
}

impl Camera {
    pub fn new(viewport: Vec2) -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            viewport,
        }
    }

    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.viewport = Vec2::new(w.max(1.0), h.max(1.0));
    }

    /// Snap an arbitrary factor to the nearest ladder step.
    pub fn set_zoom(&mut self, zoom: f32) {
        let i = Self::nearest_step(zoom);
        self.zoom = ZOOM_STEPS[i];
    }

    fn nearest_step(zoom: f32) -> usize {
        let target = zoom.max(1e-6).ln();
        let mut best = 0;
        let mut best_d = f32::MAX;
        for (i, z) in ZOOM_STEPS.iter().enumerate() {
            let d = (z.ln() - target).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    /// Visible world rectangle `(min, max)`.
    pub fn world_rect(&self) -> (Vec2, Vec2) {
        let half = self.viewport / (2.0 * self.zoom);
        (self.center - half, self.center + half)
    }

    /// Move the camera by a delta in screen (physical) pixels.
    pub fn pan_screen(&mut self, dx: f32, dy: f32) {
        self.center -= Vec2::new(dx, dy) / self.zoom;
    }

    pub fn pan_world(&mut self, d: Vec2) {
        self.center += d;
    }

    /// Move `steps` along the ladder (+1 = in), keeping the world point under `screen_pos` fixed.
    pub fn zoom_at(&mut self, steps: i32, screen_pos: Vec2) {
        let i = Self::nearest_step(self.zoom) as i32 + steps;
        let new_zoom = ZOOM_STEPS[i.clamp(0, ZOOM_STEPS.len() as i32 - 1) as usize];
        if new_zoom == self.zoom {
            return;
        }
        let before = self.screen_to_world(screen_pos);
        self.zoom = new_zoom;
        let after = self.screen_to_world(screen_pos);
        self.center += before - after;
    }

    pub fn screen_to_world(&self, p: Vec2) -> Vec2 {
        self.center + (p - self.viewport * 0.5) / self.zoom
    }

    /// Human label: `2x`, `1/8x`.
    pub fn zoom_label(&self) -> String {
        if self.zoom >= 1.0 {
            format!("{}x", self.zoom as u32)
        } else {
            format!("1/{}x", (1.0 / self.zoom).round() as u32)
        }
    }

    /// Centre snapped so that world pixels land on whole physical pixels.
    fn snapped_center(&self) -> Vec2 {
        let z = self.zoom;
        (self.center * z).round() / z
    }

    /// Orthographic view-projection: world → clip, y down, z passed through by the shader.
    pub fn view_proj(&self) -> Mat4 {
        let c = self.snapped_center();
        let half = self.viewport / (2.0 * self.zoom);
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
        cam.set_zoom(2.0);
        cam.center = Vec2::new(100.0, 50.0);
        let cursor = Vec2::new(200.0, 100.0);
        let before = cam.screen_to_world(cursor);
        cam.zoom_at(1, cursor);
        let after = cam.screen_to_world(cursor);
        assert!((before - after).length() < 1e-3);
        assert_eq!(cam.zoom, 3.0);
    }

    #[test]
    fn ladder_goes_below_one_and_clamps() {
        let mut cam = Camera::new(Vec2::new(800.0, 600.0));
        cam.zoom_at(-3, Vec2::ZERO);
        assert_eq!(cam.zoom, 1.0 / 8.0);
        assert_eq!(cam.zoom_label(), "1/8x");
        cam.zoom_at(-10, Vec2::ZERO);
        assert_eq!(cam.zoom, 1.0 / 32.0);
        cam.zoom_at(100, Vec2::ZERO);
        assert_eq!(cam.zoom, 6.0);
        cam.set_zoom(0.3);
        assert_eq!(cam.zoom, 0.25);
    }

    #[test]
    fn pan_moves_center_opposite_to_drag() {
        let mut cam = Camera::new(Vec2::new(800.0, 600.0));
        cam.set_zoom(2.0);
        cam.pan_screen(20.0, 0.0);
        assert_eq!(cam.center, Vec2::new(-10.0, 0.0));
    }
}
