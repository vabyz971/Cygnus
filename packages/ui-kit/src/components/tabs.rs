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

//! Onglets génériques (studios Pixel/Vecteur/Layout, panneaux…).
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::components::Tabs;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut studio = 0;
//! Tabs::new(&["Pixel", "Vector", "Layout"])
//!     .show(ui, &theme, &mut studio);
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Barre d'onglets via API builder.
#[derive(Debug, Clone, Copy)]
pub struct Tabs<'a> {
    options: &'a [&'a str],
}

impl<'a> Tabs<'a> {
    /// Crée une barre d'onglets.
    pub fn new(options: &'a [&'a str]) -> Self {
        Self { options }
    }

    /// Affiche les onglets, met à jour l'index, retourne la réponse
    /// du conteneur.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        selected: &mut usize,
    ) -> egui::Response {
        if !self.options.is_empty() {
            *selected = (*selected).min(self.options.len() - 1);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
            for (index, option) in self.options.iter().enumerate() {
                let active = index == *selected;
                let mut label = egui::RichText::new(*option)
                    .size(theme.typography.body_size)
                    .color(if active {
                        theme.colors.fg_primary
                    } else {
                        theme.colors.fg_secondary
                    });
                if active {
                    label = label.strong();
                }
                if ui.selectable_label(active, label).clicked() {
                    *selected = index;
                }
            }
        })
        .response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut selected = 0;
                let _ = Tabs::new(&["Pixel", "Vector", "Layout"]).show(ui, &theme, &mut selected);
                assert_eq!(selected, 0);
            });
        })
        .drop_without_applying_deltas();
    }
}
