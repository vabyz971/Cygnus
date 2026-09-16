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

//! Bouton icône du design system (barres d'outils, panneaux).
//!
//! Prend un [`Icon`](crate::icons::Icon) sémantique : la résolution
//! du glyphe passe par [`IconRegistry`](crate::icons::IconRegistry),
//! jamais par une lib d'icônes directement.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::components::IconButton;
//! # use ui_kit::icons::Icon;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! if IconButton::new(Icon::Save)
//!     .tooltip("Enregistrer")
//!     .show(ui, &theme)
//!     .clicked()
//! {
//!     // action déclenchée via UiCommand côté app
//! }
//! # });
//! ```

use crate::icons::{Icon, IconRegistry};
use crate::theme::CygnusTheme;

/// Bouton icône via API builder.
#[derive(Debug, Clone)]
pub struct IconButton {
    icon: Icon,
    tooltip: Option<String>,
    selected: bool,
}

impl IconButton {
    /// Crée un bouton pour l'icône sémantique `icon`.
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            tooltip: None,
            selected: false,
        }
    }

    /// Infobulle au survol.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// État sélectionné (teinte d'accent).
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Affiche le bouton et retourne la réponse egui.
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme) -> egui::Response {
        let registry = IconRegistry::new();
        let color = if self.selected {
            theme.colors.accent
        } else {
            theme.colors.fg_secondary
        };
        let response = ui.add(
            egui::Button::new(registry.colored(self.icon, theme.typography.icon_size, color))
                .frame(false)
                .min_size(egui::vec2(theme.sizes.icon_button, theme.sizes.icon_button)),
        );
        match self.tooltip {
            Some(tip) => response.on_hover_text(tip),
            None => response,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::setup_fonts;

    #[test]
    fn icon_button_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = IconButton::new(Icon::Save)
                    .tooltip("Enregistrer")
                    .selected(true)
                    .show(ui, &theme);
                let _ = IconButton::new(Icon::Close).show(ui, &theme);
            });
        })
        .drop_without_applying_deltas();
    }
}
