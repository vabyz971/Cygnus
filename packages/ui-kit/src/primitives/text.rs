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

//! Texte : primitive d'affichage typé via builder.
//!
//! Générique et indépendante du domaine : le style provient du
//! [`CygnusTheme`](crate::theme::CygnusTheme), jamais de littéraux.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::primitives::Text;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Text::new("Layers")
//!     .size(theme.typography.heading_size)
//!     .color(theme.colors.fg_primary)
//!     .show(ui);
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Texte affichable via une API builder.
#[derive(Debug, Clone)]
pub struct Text<'a> {
    text: &'a str,
    color: Option<egui::Color32>,
    size: Option<f32>,
    strong: bool,
}

impl<'a> Text<'a> {
    /// Crée un texte brut (style hérité du `egui::Visuals` courant).
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            color: None,
            size: None,
            strong: false,
        }
    }

    /// Texte calibré "titre de panneau" (taille + gras du thème).
    pub fn heading(theme: &CygnusTheme, text: &'a str) -> Self {
        Self::new(text).size(theme.typography.heading_size).strong()
    }

    /// Texte calibré "corps" (taille + couleur principale du thème).
    pub fn body(theme: &CygnusTheme, text: &'a str) -> Self {
        Self::new(text)
            .size(theme.typography.body_size)
            .color(theme.colors.fg_primary)
    }

    /// Texte calibré "légende" (taille + couleur secondaire du thème).
    pub fn caption(theme: &CygnusTheme, text: &'a str) -> Self {
        Self::new(text)
            .size(theme.typography.caption_size)
            .color(theme.colors.fg_secondary)
    }

    /// Teinte du texte.
    #[must_use]
    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Taille du texte (préférer les tokens `theme.typography.*`).
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Texte en gras.
    #[must_use]
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    /// Affiche le texte et retourne la réponse egui.
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let mut text = egui::RichText::new(self.text);
        if let Some(color) = self.color {
            text = text.color(color);
        }
        if let Some(size) = self.size {
            text = text.size(size);
        }
        if self.strong {
            text = text.strong();
        }
        ui.label(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Text::new("Layers")
                    .size(theme.typography.heading_size)
                    .color(theme.colors.fg_primary)
                    .show(ui);
                Text::heading(&theme, "Titre").show(ui);
                Text::body(&theme, "Corps").show(ui);
                Text::caption(&theme, "Legende").show(ui);
            });
        })
        .drop_without_applying_deltas();
    }
}
