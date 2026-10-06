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
use crate::i18n::{PhotoCatalog, PhotoTextKey};
use ui_kit::icons::{Icon, IconRegistry};
use ui_kit::theme::CygnusTheme;

/// Icône + aide contextuelle d'un outil du rail.
fn tool_icon(tool: PhotoCanvasTool, texts: PhotoCatalog) -> (Icon, &'static str) {
    match tool {
        PhotoCanvasTool::Move => (Icon::MoveTool, texts.get(PhotoTextKey::MoveHint)),
        PhotoCanvasTool::Pan => (Icon::Hand, texts.get(PhotoTextKey::PanHint)),
        PhotoCanvasTool::Zoom => (Icon::Search, texts.get(PhotoTextKey::ZoomHint)),
        PhotoCanvasTool::Brush => (Icon::Brush, texts.get(PhotoTextKey::BrushHint)),
        PhotoCanvasTool::Eraser => (Icon::Eraser, texts.get(PhotoTextKey::EraserHint)),
        PhotoCanvasTool::Eyedropper => (Icon::Eyedropper, texts.get(PhotoTextKey::EyedropperHint)),
    }
}

/// Dessine le rail d'outils vertical compact (32 px, couleur du
/// pinceau juste après les outils, façon Affinity) pour le `mode`
/// courant et retourne l'outil choisi éventuel.
///
/// Survol et sélection peints avec les tokens du thème
/// (`item_hover`, `item_selected`) : les boutons sont sans frame,
/// le fond est dessiné avant le bouton (derrière l'icône).
pub fn draw_tool_rail(
    ui: &mut egui::Ui,
    current_tool: &mut PhotoCanvasTool,
    brush_color: &mut [u8; 3],
    mode: PhotoEditMode,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) -> Option<PhotoCanvasTool> {
    let mut chosen = None;
    ui.vertical_centered(|ui| {
        for (index, tool) in mode.tools().iter().enumerate() {
            let tool = *tool;
            let (icon, tip) = tool_icon(tool, texts);
            let size = egui::vec2(theme.sizes.tool_button, theme.sizes.tool_button);
            let (rect, hover) = ui.allocate_exact_size(size, egui::Sense::hover());
            // Fondu d'apparition (egui natif) vers survol/sélection.
            let active = *current_tool == tool;
            let fade = ui.ctx().animate_bool_with_time(
                ui.make_persistent_id(("photo_tool_hover", index)),
                active || hover.hovered(),
                0.12,
            );
            if fade > 0.0 {
                let base = if active {
                    theme.colors.item_selected
                } else {
                    theme.colors.item_hover
                };
                ui.painter()
                    .rect_filled(rect, theme.radius.sm, base.gamma_multiply(fade));
            }
            let response = ui
                .put(
                    rect,
                    egui::Button::new(
                        IconRegistry::new().sized(icon, theme.typography.heading_size),
                    )
                    .frame(false),
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
        let theme = CygnusTheme::dark();
        let texts = PhotoCatalog::new(ui_kit::i18n::Language::Fr);
        let mut tool = PhotoCanvasTool::Move;
        let mut color = [255u8, 255, 255];
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert_eq!(
                    draw_tool_rail(
                        ui,
                        &mut tool,
                        &mut color,
                        PhotoEditMode::Pixel,
                        &theme,
                        texts
                    ),
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
        let theme = CygnusTheme::dark();
        let texts = PhotoCatalog::new(ui_kit::i18n::Language::Fr);
        for mode in [
            PhotoEditMode::Vector,
            PhotoEditMode::Pixel,
            PhotoEditMode::Layout,
        ] {
            let mut tool = PhotoCanvasTool::Move;
            let mut color = [255u8, 255, 255];
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = draw_tool_rail(ui, &mut tool, &mut color, mode, &theme, texts);
                });
            })
            .drop_without_applying_deltas();
        }
    }
}
