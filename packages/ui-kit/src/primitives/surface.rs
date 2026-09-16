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

//! Surface : fond calibré (carte, encart, zone de contenu).
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::primitives::Surface;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Surface::new(&theme).show(ui, |ui| {
//!     ui.label("Layer properties");
//! });
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Surface au style du thème (fond, rayon, padding, bordure).
#[derive(Debug, Clone, Copy)]
pub struct Surface {
    fill: egui::Color32,
    radius: f32,
    padding: f32,
    border: Option<egui::Stroke>,
}

impl Surface {
    /// Surface standard : fond secondaire, rayon et padding du thème.
    pub fn new(theme: &CygnusTheme) -> Self {
        Self {
            fill: theme.colors.bg_secondary,
            radius: theme.radius.md,
            padding: theme.spacing.md,
            border: None,
        }
    }

    /// Couleur de fond (préférer un token `theme.colors.*`).
    #[must_use]
    pub fn fill(mut self, fill: egui::Color32) -> Self {
        self.fill = fill;
        self
    }

    /// Rayon des coins (préférer un token `theme.radius.*`).
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Marge intérieure (préférer un token `theme.spacing.*`).
    #[must_use]
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Bordure optionnelle (couleur : `theme.colors.border`).
    #[must_use]
    pub fn border(mut self, stroke: egui::Stroke) -> Self {
        self.border = Some(stroke);
        self
    }

    /// Affiche la surface et son contenu.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let mut frame = egui::Frame::NONE
            .fill(self.fill)
            .corner_radius(self.radius)
            .inner_margin(self.padding);
        if let Some(stroke) = self.border {
            frame = frame.stroke(stroke);
        }
        frame.show(ui, add_contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Surface::new(&theme).show(ui, |ui| {
                    ui.label("contenu");
                });
                Surface::new(&theme)
                    .fill(theme.colors.bg_tertiary)
                    .border(egui::Stroke::new(theme.borders.thin, theme.colors.border))
                    .show(ui, |ui| {
                        ui.label("contenu borde");
                    });
            });
        })
        .drop_without_applying_deltas();
    }
}
