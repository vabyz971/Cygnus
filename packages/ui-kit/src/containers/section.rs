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

//! Section titrée sans fond (regroupe des composants).
//!
//! Contrairement à [`Panel`](super::panel::Panel), la section ne
//! dessine aucune surface : titre + contenu, sans chrome.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::containers::Section;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Section::new("Transform").show(ui, &theme, |ui| {
//!     ui.label("position, rotation, echelle");
//! });
//! # });
//! ```

use crate::primitives::{Text, divider};
use crate::theme::CygnusTheme;

/// Section titrée via API builder.
#[derive(Debug, Clone)]
pub struct Section<'a> {
    title: &'a str,
}

impl<'a> Section<'a> {
    /// Crée une section avec titre.
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    /// Affiche la section et son contenu.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        ui.vertical(|ui| {
            Text::heading(theme, self.title).show(ui);
            divider(ui, theme);
            add_contents(ui)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Section::new("Transform").show(ui, &theme, |ui| {
                    ui.label("contenu");
                });
            });
        })
        .drop_without_applying_deltas();
    }
}
