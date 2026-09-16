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

//! Rail d'outils PHOTO (panneau gauche, widget métier).
//!
//! Barre verticale d'icônes avec tooltip affichant le nom de chaque
//! outil. Le contenu suit le mode d'édition (voir `super::modebar` :
//! tous les outils en Pixel, navigation seule en Vector/Layout).
//! Style exclusivement ui-kit.

use super::modebar::PhotoEditMode;
use super::viewport::PhotoCanvasTool;
use ui_kit::widgets::icon::CygnusIcon;

/// Icône + aide contextuelle d'un outil du rail.
fn tool_icon(tool: PhotoCanvasTool) -> (CygnusIcon, &'static str) {
    match tool {
        PhotoCanvasTool::Move => (CygnusIcon::MoveTool, "Deplacer / selectionner"),
        PhotoCanvasTool::Pan => (CygnusIcon::Hand, "Main (deplacer la vue)"),
        PhotoCanvasTool::Zoom => (CygnusIcon::Search, "Loupe"),
        PhotoCanvasTool::Brush => (CygnusIcon::Brush, "Pinceau"),
        PhotoCanvasTool::Eraser => (CygnusIcon::Eraser, "Gomme"),
        PhotoCanvasTool::Eyedropper => (CygnusIcon::Eyedropper, "Pipette"),
    }
}

/// Dessine le rail d'outils vertical (nuancier pinceau en bas,
/// façon Affinity) pour le `mode` courant et retourne l'outil choisi
/// éventuel.
pub fn draw_tool_rail(
    ui: &mut egui::Ui,
    current_tool: &mut PhotoCanvasTool,
    brush_color: &mut [u8; 3],
    mode: PhotoEditMode,
) -> Option<PhotoCanvasTool> {
    let mut chosen = None;
    ui.vertical_centered(|ui| {
        for tool in mode.tools() {
            let tool = *tool;
            let (icon, tip) = tool_icon(tool);
            let response = ui
                .add_sized(
                    egui::vec2(44.0, 44.0),
                    egui::Button::new(icon.sized(20.0))
                        .frame(false)
                        .selected(*current_tool == tool),
                )
                .on_hover_text(tip);
            if response.clicked() {
                *current_tool = tool;
                chosen = Some(tool);
            }
        }
        ui.separator();
        egui::color_picker::color_edit_button_srgb(ui, brush_color);
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_rail_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut tool = PhotoCanvasTool::Move;
        let mut color = [255u8, 255, 255];
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert_eq!(
                    draw_tool_rail(ui, &mut tool, &mut color, PhotoEditMode::Pixel),
                    None
                );
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(tool, PhotoCanvasTool::Move);
    }

    #[test]
    fn every_mode_rail_renders() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        for mode in [
            PhotoEditMode::Vector,
            PhotoEditMode::Pixel,
            PhotoEditMode::Layout,
        ] {
            let mut tool = PhotoCanvasTool::Move;
            let mut color = [255u8, 255, 255];
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = draw_tool_rail(ui, &mut tool, &mut color, mode);
                });
            })
            .drop_without_applying_deltas();
        }
    }
}
