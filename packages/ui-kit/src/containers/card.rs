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

//! Carte : surface bordée pour un contenu autonome.
//!
//! Fine enveloppe sur [`Surface`](crate::primitives::Surface) avec
//! la bordure du thème.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::containers::Card;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Card::new().show(ui, &theme, |ui| {
//!     ui.label("apercu");
//! });
//! # });
//! ```

use crate::primitives::Surface;
use crate::theme::CygnusTheme;

/// Carte via API builder.
#[derive(Debug, Clone, Copy, Default)]
pub struct Card;

impl Card {
    /// Crée une carte.
    pub fn new() -> Self {
        Self
    }

    /// Affiche la carte et son contenu.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        Surface::new(theme)
            .border(egui::Stroke::new(theme.borders.thin, theme.colors.border))
            .show(ui, add_contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Card::new().show(ui, &theme, |ui| {
                    ui.label("contenu");
                });
            });
        })
        .drop_without_applying_deltas();
    }
}
