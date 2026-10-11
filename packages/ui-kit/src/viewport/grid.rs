// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! World grid: pure line computation + drawing via `egui::Painter`.
//!
//! No knowledge of content (pixels, shapes, clips): the grid is a
//! world-coordinate reference frame, projected by the
//! [`Camera`](super::camera::Camera).

use super::camera::Camera;
use crate::theme::CygnusTheme;

/// A world grid's configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridConfig {
    /// Step in world units (> 0).
    pub spacing: f32,
    /// Line color.
    pub color: egui::Color32,
}

impl GridConfig {
    /// Theme-calibrated grid (`spacing` step, border color).
    pub fn themed(theme: &CygnusTheme, spacing: f32) -> Self {
        Self {
            spacing: if spacing > 0.0 { spacing } else { 1.0 },
            color: theme.colors.border,
        }
    }

    /// World abscissas / ordinates of the lines crossing `world`.
    /// Negative `world` or invalid step → empty vectors.
    pub fn lines_in(&self, world: egui::Rect) -> (Vec<f32>, Vec<f32>) {
        if self.spacing <= 0.0 || world.is_negative() {
            return (Vec::new(), Vec::new());
        }
        let xs = stepped(world.min.x, world.max.x, self.spacing);
        let ys = stepped(world.min.y, world.max.y, self.spacing);
        (xs, ys)
    }

    /// Draws the grid on `painter` (screen rect `screen_rect`).
    pub fn draw(
        self,
        painter: &egui::Painter,
        camera: Camera,
        viewport_size: egui::Vec2,
        screen_rect: egui::Rect,
    ) {
        let world = egui::Rect::from_two_pos(
            camera.screen_to_world(screen_rect.min, viewport_size),
            camera.screen_to_world(screen_rect.max, viewport_size),
        );
        let (xs, ys) = self.lines_in(world);
        let stroke = egui::Stroke::new(1.0, self.color);
        for x in xs {
            let a = camera.world_to_screen(egui::pos2(x, world.min.y), viewport_size);
            let b = camera.world_to_screen(egui::pos2(x, world.max.y), viewport_size);
            painter.line_segment([a, b], stroke);
        }
        for y in ys {
            let a = camera.world_to_screen(egui::pos2(world.min.x, y), viewport_size);
            let b = camera.world_to_screen(egui::pos2(world.max.x, y), viewport_size);
            painter.line_segment([a, b], stroke);
        }
    }
}

/// `min..=max` values aligned on `step` (included, ordered).
fn stepped(min: f32, max: f32, step: f32) -> Vec<f32> {
    if step <= 0.0 || max < min {
        return Vec::new();
    }
    let first = (min / step).ceil() as i64;
    let last = (max / step).floor() as i64;
    if last < first {
        return Vec::new();
    }
    // Guard: never more than 10,000 lines per axis.
    let count = (last - first + 1).min(10_000);
    (0..count)
        .map(|offset| (first + offset) as f32 * step)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_align_on_step() {
        let grid = GridConfig {
            spacing: 10.0,
            color: egui::Color32::GRAY,
        };
        let world = egui::Rect::from_min_max(egui::pos2(5.0, 5.0), egui::pos2(25.0, 25.0));
        let (xs, ys) = grid.lines_in(world);
        assert_eq!(xs, vec![10.0, 20.0]);
        assert_eq!(ys, vec![10.0, 20.0]);
    }

    #[test]
    fn degenerate_input_gives_no_lines() {
        let grid = GridConfig {
            spacing: 0.0,
            color: egui::Color32::GRAY,
        };
        let world = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(10.0, 10.0));
        assert_eq!(grid.lines_in(world), (Vec::new(), Vec::new()));
        let grid = GridConfig {
            spacing: 10.0,
            color: egui::Color32::GRAY,
        };
        assert_eq!(grid.lines_in(egui::Rect::NOTHING), (Vec::new(), Vec::new()));
    }

    #[test]
    fn themed_grid_uses_theme_color() {
        let theme = CygnusTheme::dark();
        let grid = GridConfig::themed(&theme, 32.0);
        assert_eq!(grid.spacing, 32.0);
        assert_eq!(grid.color, theme.colors.border);
    }
}
