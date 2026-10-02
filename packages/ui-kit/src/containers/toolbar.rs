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

//! Barre d'outils standard Cygnus.
//!
//! Bandeau haut non redimensionnable avec ligne de séparation. Les apps
//! y placent des [`icon_button`](crate::icons::icon_button)
//! et des groupes séparés par `ui.separator()` — jamais de style en dur.

/// Barre d'outils standard Cygnus.
///
/// # Exemple
/// ```rust,no_run
/// # use ui_kit::containers::Toolbar;
/// # use ui_kit::icons::{Icon, icon_button};
/// # egui::__run_test_ui(|ui| {
/// Toolbar::new("photo_toolbar").show(ui, |ui| {
///     ui.horizontal(|ui| {
///         icon_button(ui, Icon::Undo, Some("Annuler"));
///         icon_button(ui, Icon::Redo, Some("Retablir"));
///     });
/// });
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Toolbar {
    id: &'static str,
}

impl Toolbar {
    /// Crée une barre d'outils (identifiant globalement unique).
    pub fn new(id: &'static str) -> Self {
        Self { id }
    }

    /// Affiche la barre en haut et exécute le contenu.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Panel::top(self.id)
            .resizable(false)
            .show_separator_line(true)
            .show(ui, add_contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::{ALL_ICONS, icon_button};

    #[test]
    fn toolbar_with_ten_icons_renders_without_panic() {
        let ctx = egui::Context::default();
        crate::theme::setup_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            Toolbar::new("test_toolbar").show(ui, |ui| {
                ui.horizontal(|ui| {
                    for icon in ALL_ICONS.iter().take(10) {
                        let _ = icon_button(ui, *icon, None);
                    }
                });
            });
        })
        .drop_without_applying_deltas();
        assert!(ALL_ICONS.len() >= 10);
    }
}
