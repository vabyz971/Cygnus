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

//! Barre des paramètres de l'outil actif PHOTO (sous le menu).
//!
//! Reprend l'ancienne interface : pinceau (taille, opacité, couleur),
//! gomme (taille, opacité), autres outils (aide contextuelle).
//! Rien si l'outil n'a pas de réglages — la barre reste vide.

use super::viewport::{PhotoBrushSettings, PhotoCanvasTool};
use ui_kit::theme::tokens::CygnusTheme;
use ui_kit::theme::typography::body_text;
use ui_kit::widgets::{CygnusSlider, CygnusToggle};

/// Dessine les réglages de l'outil actif (met à jour `brush` et
/// `show_grid` en place, sans channel : état purement local).
pub fn draw_tool_options(
    ui: &mut egui::Ui,
    tool: PhotoCanvasTool,
    brush: &mut PhotoBrushSettings,
    show_grid: &mut bool,
) {
    let theme = CygnusTheme::dark();
    match tool {
        PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser => {
            let title = if tool == PhotoCanvasTool::Brush {
                "Pinceau"
            } else {
                "Gomme"
            };
            ui.horizontal(|ui| {
                ui.label(body_text(&theme, title));
                ui.separator();
                let mut size = brush.radius * 2.0;
                CygnusSlider::new("Taille", 1.0..=200.0).show(ui, &mut size);
                brush.radius = (size / 2.0).clamp(0.5, 100.0);
                let mut opacity = brush.opacity;
                CygnusSlider::new("Opacite", 0.0..=1.0).show(ui, &mut opacity);
                brush.opacity = opacity.clamp(0.0, 1.0);
                if tool == PhotoCanvasTool::Brush {
                    ui.label(body_text(&theme, "Couleur"));
                    let mut color = brush.color;
                    egui::color_picker::color_edit_button_srgb(ui, &mut color);
                    brush.color = color;
                }
            });
        }
        PhotoCanvasTool::Move => {
            ui.horizontal(|ui| {
                ui.label(body_text(&theme, "Deplacement"));
                ui.separator();
                CygnusToggle::new("Grille").show(ui, show_grid);
            });
        }
        PhotoCanvasTool::Pan => {
            ui.horizontal(|ui| {
                ui.label(body_text(
                    &theme,
                    "Main : glisser pour deplacer la vue, molette pour zoomer",
                ));
            });
        }
        PhotoCanvasTool::Zoom => {
            ui.horizontal(|ui| {
                ui.label(body_text(
                    &theme,
                    "Loupe : clic = zoom avant, clic droit = zoom arriere",
                ));
            });
        }
        PhotoCanvasTool::Eyedropper => {
            ui.horizontal(|ui| {
                ui.label(body_text(&theme, "Pipette"));
                ui.separator();
                ui.label(body_text(
                    &theme,
                    "Cliquer sur le canvas pour echantillonner la couleur",
                ));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_render_for_every_tool_without_panic() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            let mut brush = PhotoBrushSettings::default();
            let mut show_grid = false;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    draw_tool_options(ui, tool, &mut brush, &mut show_grid);
                });
            })
            .drop_without_applying_deltas();
        }
    }
}
