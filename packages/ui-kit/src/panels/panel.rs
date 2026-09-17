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

//! Panneau titré Cygnus (latéral, haut, bas ou central).
//!
//! Enveloppe homogène autour de [`egui::Panel`] : titre au style du
//! thème, séparateur, puis contenu. Pour les zones redimensionnables,
//! voir [`CygnusSplitPanel`](super::split::CygnusSplitPanel).

use crate::theme::CygnusTheme;
use crate::theme::typography::heading_text;

/// Panneau titré standard Cygnus.
///
/// # Exemple
/// ```rust
/// # use ui_kit::panels::CygnusPanel;
/// # egui::__run_test_ui(|ui| {
/// CygnusPanel::new("Calques").show_left(ui, "photo_left", |ui| {
///     ui.label("contenu");
/// });
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CygnusPanel<'a> {
    title: &'a str,
}

impl<'a> CygnusPanel<'a> {
    /// Crée un panneau avec le titre donné.
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    /// Dessine le titre puis le contenu dans l'`Ui` du panneau.
    fn decorate<R>(self, ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let theme = CygnusTheme::dark();
        ui.label(heading_text(&theme, self.title));
        ui.separator();
        add_contents(ui)
    }

    /// Panneau latéral gauche (largeur fixe, non redimensionnable).
    pub fn show_left<R>(
        self,
        ui: &mut egui::Ui,
        id: &'static str,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Panel::left(id)
            .resizable(false)
            .show(ui, |ui| self.decorate(ui, add_contents))
    }

    /// Panneau latéral droit (largeur fixe, non redimensionnable).
    pub fn show_right<R>(
        self,
        ui: &mut egui::Ui,
        id: &'static str,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Panel::right(id)
            .resizable(false)
            .show(ui, |ui| self.decorate(ui, add_contents))
    }

    /// Panneau haut (hauteur fixe, non redimensionnable).
    pub fn show_top<R>(
        self,
        ui: &mut egui::Ui,
        id: &'static str,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Panel::top(id)
            .resizable(false)
            .show(ui, |ui| self.decorate(ui, add_contents))
    }

    /// Panneau bas (hauteur fixe, non redimensionnable).
    pub fn show_bottom<R>(
        self,
        ui: &mut egui::Ui,
        id: &'static str,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Panel::bottom(id)
            .resizable(false)
            .show(ui, |ui| self.decorate(ui, add_contents))
    }

    /// Zone centrale (doit être ajoutée en dernier).
    pub fn show_central<R>(
        self,
        ui: &mut egui::Ui,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::CentralPanel::default().show(ui, |ui| self.decorate(ui, add_contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_render_without_panic() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            CygnusPanel::new("Gauche").show_left(ui, "test_left", |ui| {
                ui.label("gauche");
            });
            CygnusPanel::new("Droite").show_right(ui, "test_right", |ui| {
                ui.label("droite");
            });
            CygnusPanel::new("Bas").show_bottom(ui, "test_bottom", |ui| {
                ui.label("bas");
            });
            CygnusPanel::new("Centre").show_central(ui, |ui| {
                ui.label("centre");
            });
        })
        .drop_without_applying_deltas();
    }
}
