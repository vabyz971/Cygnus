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

//! Case à cocher labellisée, calibrée sur le thème.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::components::Checkbox;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut visible = true;
//! Checkbox::new("Visible").show(ui, &theme, &mut visible);
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Case à cocher via API builder.
#[derive(Debug, Clone)]
pub struct Checkbox<'a> {
    label: &'a str,
}

impl<'a> Checkbox<'a> {
    /// Crée une case avec libellé.
    pub fn new(label: &'a str) -> Self {
        Self { label }
    }

    /// Affiche la case, met à jour `checked`, retourne la réponse egui.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        checked: &mut bool,
    ) -> egui::Response {
        ui.checkbox(
            checked,
            egui::RichText::new(self.label)
                .size(theme.typography.body_size)
                .color(theme.colors.fg_primary),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut checked = false;
                let _ = Checkbox::new("Visible").show(ui, &theme, &mut checked);
            });
        })
        .drop_without_applying_deltas();
    }
}
