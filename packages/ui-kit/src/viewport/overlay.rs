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

//! Overlays agnostiques : cadres monde + croix, sans type métier.
//!
//! Les apps y projettent leurs bornes (document, sélection, clip) en
//! rectangles monde : l'overlay ne sait pas ce qu'il encadre.

use super::camera::Camera;
use crate::theme::CygnusTheme;

/// Dessine un rectangle monde (bornes de contenu, sélection…).
pub fn draw_rect_world(
    painter: &egui::Painter,
    camera: Camera,
    viewport_size: egui::Vec2,
    rect_world: egui::Rect,
    stroke: egui::Stroke,
) {
    painter.rect_stroke(
        camera.rect_to_screen(rect_world, viewport_size),
        0.0,
        stroke,
        egui::StrokeKind::Inside,
    );
}

/// Dessine une croix centrée sur `center_screen` (origine, pivot…).
pub fn draw_crosshair(
    painter: &egui::Painter,
    center_screen: egui::Pos2,
    radius: f32,
    stroke: egui::Stroke,
) {
    let radius = radius.max(0.0);
    painter.line_segment(
        [
            egui::pos2(center_screen.x - radius, center_screen.y),
            egui::pos2(center_screen.x + radius, center_screen.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(center_screen.x, center_screen.y - radius),
            egui::pos2(center_screen.x, center_screen.y + radius),
        ],
        stroke,
    );
}

/// Cadre de sélection au style du thème (accent, trait moyen).
pub fn draw_selection(
    painter: &egui::Painter,
    camera: Camera,
    viewport_size: egui::Vec2,
    rect_world: egui::Rect,
    theme: &CygnusTheme,
) {
    draw_rect_world(
        painter,
        camera,
        viewport_size,
        rect_world,
        egui::Stroke::new(theme.borders.medium, theme.colors.accent),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_draw_without_panic() {
        let theme = CygnusTheme::dark();
        let camera = Camera::new();
        let size = egui::vec2(800.0, 600.0);
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let painter = ui.painter().clone();
                let rect =
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 80.0));
                draw_selection(&painter, camera, size, rect, &theme);
                draw_crosshair(
                    &painter,
                    egui::pos2(400.0, 300.0),
                    8.0,
                    egui::Stroke::new(theme.borders.thin, theme.colors.fg_secondary),
                );
            });
        })
        .drop_without_applying_deltas();
    }
}
