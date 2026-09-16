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

//! Interrupteur (switch) dessiné au style du thème.
//!
//! Contrairement à la case à cocher, l'interrupteur convient aux
//! options "on/off" autonomes (grille, magnétisme, aperçu).
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::components::Toggle;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut grid = true;
//! Toggle::new("Grille").show(ui, &theme, &mut grid);
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Interrupteur labellisé via API builder.
#[derive(Debug, Clone)]
pub struct Toggle<'a> {
    label: &'a str,
}

impl<'a> Toggle<'a> {
    /// Crée un interrupteur avec libellé.
    pub fn new(label: &'a str) -> Self {
        Self { label }
    }

    /// Affiche l'interrupteur, met à jour `on`, retourne la réponse
    /// du switch (le libellé suit, non cliquable).
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme, on: &mut bool) -> egui::Response {
        let height = theme.sizes.min_touch * 0.8;
        let track_width = height * 1.8;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(track_width, height), egui::Sense::click());
        if response.clicked() {
            *on = !*on;
        }
        let painter = ui.painter_at(rect);
        let radius = height / 2.0;
        let track = if *on {
            theme.colors.accent
        } else {
            theme.colors.bg_tertiary
        };
        painter.rect_filled(rect, radius, track);
        if !*on {
            painter.rect_stroke(
                rect,
                radius,
                egui::Stroke::new(theme.borders.thin, theme.colors.border),
                egui::StrokeKind::Inside,
            );
        }
        let knob_radius = height / 2.0 - 2.0;
        let knob_x = if *on {
            rect.right() - knob_radius - 2.0
        } else {
            rect.left() + knob_radius + 2.0
        };
        painter.circle_filled(
            egui::pos2(knob_x, rect.center().y),
            knob_radius,
            theme.colors.fg_primary,
        );
        ui.label(
            egui::RichText::new(self.label)
                .size(theme.typography.body_size)
                .color(theme.colors.fg_primary),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut on = true;
                let _ = Toggle::new("Grille").show(ui, &theme, &mut on);
                let mut off = false;
                let _ = Toggle::new("Magnetisme").show(ui, &theme, &mut off);
            });
        })
        .drop_without_applying_deltas();
    }
}
