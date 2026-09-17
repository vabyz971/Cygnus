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

//! Modale standard Cygnus (titre + contenu + boutons d'action).
//!
//! L'ouverture/fermeture est détenue par l'app (`open: &mut bool`) :
//! la modale ne fait qu'afficher et rapporter l'action choisie.

use crate::theme::CygnusTheme;
use crate::theme::typography::heading_text;

/// Action rapportée par [`CygnusModal::show`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalAction {
    /// Bouton primaire (valider).
    Confirm,
    /// Bouton secondaire (annuler) ou fermeture.
    Cancel,
}

/// Modale standard Cygnus.
///
/// # Exemple
/// ```rust
/// # use ui_kit::dialogs::CygnusModal;
/// # egui::__run_test_ui(|ui| {
/// let mut open = true;
/// CygnusModal::new("Exporter", "Valider", "Annuler").show(ui, &mut open, |ui| {
///     ui.label("options d'export");
/// });
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CygnusModal<'a> {
    title: &'a str,
    confirm_label: &'a str,
    cancel_label: &'a str,
}

impl<'a> CygnusModal<'a> {
    /// Crée une modale (titre + libellés des boutons).
    pub fn new(title: &'a str, confirm_label: &'a str, cancel_label: &'a str) -> Self {
        Self {
            title,
            confirm_label,
            cancel_label,
        }
    }

    /// Affiche la modale si `open`. Retourne l'action éventuelle
    /// (et referme sur Confirm/Cancel).
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        open: &mut bool,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<ModalAction> {
        if !*open {
            return None;
        }
        let theme = CygnusTheme::dark();
        let mut action = None;
        egui::Window::new(heading_text(&theme, self.title))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                add_contents(ui);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(self.cancel_label).clicked() {
                        action = Some(ModalAction::Cancel);
                    }
                    if ui.button(self.confirm_label).clicked() {
                        action = Some(ModalAction::Confirm);
                    }
                });
            });
        if action.is_some() {
            *open = false;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_closed_renders_nothing() {
        let ctx = egui::Context::default();
        let mut open = false;
        let mut shown = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let action = CygnusModal::new("Titre", "OK", "Non").show(ui.ctx(), &mut open, |_ui| {
                shown = true;
            });
            assert_eq!(action, None);
        })
        .drop_without_applying_deltas();
        assert!(!shown);
        assert!(!open);
    }

    #[test]
    fn modal_open_renders_without_panic() {
        let ctx = egui::Context::default();
        let mut open = true;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = CygnusModal::new("Exporter", "Valider", "Annuler").show(
                ui.ctx(),
                &mut open,
                |ui| {
                    ui.label("contenu");
                },
            );
        })
        .drop_without_applying_deltas();
        assert!(open, "sans clic, la modale reste ouverte");
    }
}
