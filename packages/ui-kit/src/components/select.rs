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

//! Liste déroulante (modes de fusion, presets, formats…).
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::components::Select;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut blend = 0;
//! Select::new("Fusion", &["Normal", "Multiplier"])
//!     .show(ui, &theme, &mut blend);
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Borne un index à la plage valide (0 pour liste vide).
pub fn sanitize_selected(selected: usize, len: usize) -> usize {
    if len == 0 { 0 } else { selected.min(len - 1) }
}

/// Liste déroulante via API builder.
#[derive(Debug, Clone, Copy)]
pub struct Select<'a> {
    label: &'a str,
    options: &'a [&'a str],
}

impl<'a> Select<'a> {
    /// Crée une liste avec libellé et options.
    pub fn new(label: &'a str, options: &'a [&'a str]) -> Self {
        Self { label, options }
    }

    /// Nombre d'options.
    pub fn len(self) -> usize {
        self.options.len()
    }

    /// Vrai si aucune option.
    pub fn is_empty(self) -> bool {
        self.options.is_empty()
    }

    /// Affiche la liste, met à jour l'index, retourne la réponse egui.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        selected: &mut usize,
    ) -> egui::Response {
        *selected = sanitize_selected(*selected, self.options.len());
        let current = self.options.get(*selected).copied().unwrap_or("");
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(self.label)
                    .size(theme.typography.body_size)
                    .color(theme.colors.fg_secondary),
            );
            egui::ComboBox::from_label("")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (index, option) in self.options.iter().enumerate() {
                        ui.selectable_value(selected, index, *option);
                    }
                })
                .response
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps() {
        assert_eq!(sanitize_selected(0, 0), 0);
        assert_eq!(sanitize_selected(9, 3), 2);
        assert_eq!(sanitize_selected(1, 3), 1);
    }

    #[test]
    fn select_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut selected = 5;
                let _ = Select::new("Fusion", &["Normal", "Multiplier"]).show(
                    ui,
                    &theme,
                    &mut selected,
                );
                assert_eq!(selected, 1);
            });
        })
        .drop_without_applying_deltas();
    }
}
