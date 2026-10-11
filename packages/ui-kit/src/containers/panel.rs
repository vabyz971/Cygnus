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

//! Titled surface panel (studio block with chrome).
//!
//! Unlike [`Section`](super::section::Section) (background-free),
//! the panel draws a bordered surface with a header. Placement
//! in the layout (left / right / bottom) stays per-app.
//!
//! # Example
//! ```rust,no_run
//! # use ui_kit::containers::Panel;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Panel::new("Calques").show(ui, &theme, |ui| {
//!     ui.label("liste des calques");
//! });
//! # });
//! ```

use crate::primitives::{Surface, Text, divider};
use crate::theme::CygnusTheme;

/// Titled panel via builder API.
#[derive(Debug, Clone)]
pub struct Panel<'a> {
    title: &'a str,
}

impl<'a> Panel<'a> {
    /// Creates a panel with title.
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    /// Displays the panel and its content.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        Surface::new(theme)
            .border(egui::Stroke::new(theme.borders.thin, theme.colors.border))
            .show(ui, |ui| {
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
    fn panel_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Panel::new("Calques").show(ui, &theme, |ui| {
                    ui.label("contenu");
                });
            });
        })
        .drop_without_applying_deltas();
    }
}
