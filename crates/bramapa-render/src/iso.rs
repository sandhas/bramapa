//! Isometric (2:1 dimetric) cell ↔ world-pixel math. World units are atlas pixels at 1x.
//! World `y` grows downwards (screen convention).

use glam::Vec2;

/// Geometry of a tileset, copied out of `tiles.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsoGeometry {
    pub tile_w: f32,
    pub tile_h: f32,
    pub cell_w: f32,
    pub cell_h: f32,
}

impl IsoGeometry {
    pub const DEFAULT: Self = Self {
        tile_w: 64.0,
        tile_h: 32.0,
        cell_w: 64.0,
        cell_h: 64.0,
    };

    /// Top vertex of the diamond for cell `(x, y)`.
    #[inline]
    pub fn cell_top(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new((x - y) * self.tile_w * 0.5, (x + y) * self.tile_h * 0.5)
    }

    /// Centre of the diamond for cell `(x, y)`.
    #[inline]
    pub fn cell_center(&self, x: f32, y: f32) -> Vec2 {
        self.cell_top(x, y) + Vec2::new(0.0, self.tile_h * 0.5)
    }

    /// Inverse of `cell_center`: world point → fractional cell coordinates.
    #[inline]
    pub fn world_to_cell(&self, p: Vec2) -> Vec2 {
        let py = p.y - self.tile_h * 0.5;
        let a = p.x / (self.tile_w * 0.5); // x - y
        let b = py / (self.tile_h * 0.5); // x + y
        Vec2::new((a + b) * 0.5, (b - a) * 0.5)
    }

    /// Axis-aligned world bounds `(min, max)` of the quads drawn for cells
    /// `x0..=x1, y0..=y1` (inclusive), including tall sprites above the diamond.
    pub fn cells_bounds(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> (Vec2, Vec2) {
        let min_x = self.cell_top(x0, y1).x - self.cell_w * 0.5;
        let max_x = self.cell_top(x1, y0).x + self.cell_w * 0.5;
        let min_y = self.cell_top(x0, y0).y + self.tile_h - self.cell_h;
        let max_y = self.cell_top(x1, y1).y + self.tile_h;
        (Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_cell_world() {
        let g = IsoGeometry::DEFAULT;
        for (x, y) in [(0.0, 0.0), (3.0, 7.0), (120.0, 5.0)] {
            let c = g.world_to_cell(g.cell_center(x, y));
            assert!(
                (c.x - x).abs() < 1e-4 && (c.y - y).abs() < 1e-4,
                "{c:?} vs ({x},{y})"
            );
        }
    }

    #[test]
    fn bounds_contain_corners() {
        let g = IsoGeometry::DEFAULT;
        let (min, max) = g.cells_bounds(0.0, 0.0, 63.0, 63.0);
        assert!(min.x < g.cell_top(0.0, 63.0).x);
        assert!(max.x > g.cell_top(63.0, 0.0).x);
        assert!(min.y < 0.0 && max.y > g.cell_top(63.0, 63.0).y);
    }
}
