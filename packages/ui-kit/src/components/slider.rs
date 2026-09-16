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

//! Curseur numérique labellisé (opacité, taille, zoom…).
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::components::Slider;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut opacity = 1.0;
//! Slider::new("Opacite", 0.0..=1.0).show(ui, &theme, &mut opacity);
//! # });
//! ```

use crate::theme::CygnusTheme;
use std::ops::RangeInclusive;

/// Curseur `f32` via API builder.
#[derive(Debug, Clone)]
pub struct Slider<'a> {
    label: &'a str,
    range: RangeInclusive<f32>,
    step: Option<f64>,
}

impl<'a> Slider<'a> {
    /// Crée un curseur avec libellé et plage.
    pub fn new(label: &'a str, range: RangeInclusive<f32>) -> Self {
        Self {
            label,
            range,
            step: None,
        }
    }

    /// Pas de discrétisation (optionnel).
    #[must_use]
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Affiche le curseur, met à jour `value`, retourne la réponse egui.
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme, value: &mut f32) -> egui::Response {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(self.label)
                    .size(theme.typography.body_size)
                    .color(theme.colors.fg_secondary),
            );
            let mut slider = egui::Slider::new(value, self.range);
            if let Some(step) = self.step {
                slider = slider.step_by(step);
            }
            ui.add(slider)
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut value = 0.5;
                let _ = Slider::new("Opacite", 0.0..=1.0)
                    .step(0.01)
                    .show(ui, &theme, &mut value);
            });
        })
        .drop_without_applying_deltas();
    }
}
