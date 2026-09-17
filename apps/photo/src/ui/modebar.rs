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

//! Barre des modes et paramètres d'outils PHOTO (2e rangée haute,
//! toute la largeur, widget métier).
//!
//! Rangée 1 : grands boutons de changement de mode (Vector, Pixel,
//! Layout) puis séparateur. Rangée 2 : nom et paramètres de l'outil
//! sélectionné (délégué à `super::optionsbar`). Le mode restreint les
//! outils du rail (les outils de peinture vivent en Pixel ; Vector et
//! Layout exposent déplacement, main et loupe en attendant leurs
//! outils propres) : basculer de mode rebascule sur Déplacement si
//! l'outil courant n'y existe pas.

use super::optionsbar::draw_tool_options;
use super::viewport::{PhotoBrushSettings, PhotoCanvasTool};
use ui_kit::widgets::{CygnusButton, CygnusButtonStyle};

/// Mode d'édition du document (façon personas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoEditMode {
    /// Tracés vectoriels (déplacement, main, loupe pour l'instant).
    Vector,
    /// Retouche pixels (tous les outils).
    #[default]
    Pixel,
    /// Mise en page (déplacement, main, loupe pour l'instant).
    Layout,
}

impl PhotoEditMode {
    /// Libellé du mode (ASCII).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Vector => "Vector",
            Self::Pixel => "Pixel",
            Self::Layout => "Layout",
        }
    }

    /// Tous les modes, pour l'affichage et les tests.
    pub const ALL: &[Self] = &[Self::Vector, Self::Pixel, Self::Layout];

    /// Outils disponibles dans ce mode.
    #[must_use]
    pub fn tools(self) -> &'static [PhotoCanvasTool] {
        match self {
            Self::Pixel => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Brush,
                PhotoCanvasTool::Eraser,
                PhotoCanvasTool::Eyedropper,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
            Self::Vector | Self::Layout => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
        }
    }

    /// Vrai si l'outil est disponible dans ce mode.
    #[must_use]
    pub fn supports(self, tool: PhotoCanvasTool) -> bool {
        self.tools().contains(&tool)
    }
}

/// Dessine la barre 2 rangées (modes puis paramètres de l'outil).
/// Retourne le mode éventuellement choisi (l'app rebascule l'outil
/// si besoin via [`PhotoEditMode::supports`]).
pub fn draw_photo_modebar(
    ui: &mut egui::Ui,
    mode: &mut PhotoEditMode,
    tool: PhotoCanvasTool,
    brush: &mut PhotoBrushSettings,
    show_grid: &mut bool,
) -> Option<PhotoEditMode> {
    let mut chosen = None;
    // Rangée 1 : grands boutons de mode
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            for candidate in PhotoEditMode::ALL {
                let active = *mode == *candidate;
                if CygnusButton::new(candidate.label())
                    .style(if active {
                        CygnusButtonStyle::Primary
                    } else {
                        CygnusButtonStyle::Secondary
                    })
                    .show(ui)
                    .clicked()
                {
                    *mode = *candidate;
                    chosen = Some(*candidate);
                }
            }
        });
    });
    ui.separator();
    // Rangée 2 : nom et paramètres de l'outil sélectionné.
    ui.horizontal(|ui| {
        draw_tool_options(ui, tool, brush, show_grid);
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_mode_supports_every_tool() {
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            assert!(PhotoEditMode::Pixel.supports(tool));
        }
    }

    #[test]
    fn vector_and_layout_are_navigation_only() {
        for mode in [PhotoEditMode::Vector, PhotoEditMode::Layout] {
            assert!(mode.supports(PhotoCanvasTool::Move));
            assert!(!mode.supports(PhotoCanvasTool::Brush));
            assert!(!mode.supports(PhotoCanvasTool::Eraser));
        }
    }

    #[test]
    fn modebar_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut mode = PhotoEditMode::Pixel;
        let mut brush = PhotoBrushSettings::default();
        let mut show_grid = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let chosen = draw_photo_modebar(
                    ui,
                    &mut mode,
                    PhotoCanvasTool::Brush,
                    &mut brush,
                    &mut show_grid,
                );
                assert_eq!(chosen, None);
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(mode, PhotoEditMode::Pixel);
    }
}
