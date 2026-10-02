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

//! Icône : primitive d'affichage d'une icône sémantique.
//!
//! Fin wrapper autour de [`crate::icons::Icon`] : la primitive ne
//! résout aucun glyphe elle-même, elle délègue au registre.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::primitives::Icon;
//! # use ui_kit::icons::Icon as IconKind;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Icon::new(IconKind::Save)
//!     .size(theme.typography.icon_size)
//!     .color(theme.colors.fg_secondary)
//!     .show(ui);
//! # });
//! ```

use crate::icons::{Icon as IconKind, IconRegistry};

/// Glyphe affichable via une API builder.
#[derive(Debug, Clone, Copy)]
pub struct Icon {
    kind: IconKind,
    size: Option<f32>,
    color: Option<egui::Color32>,
}

impl Icon {
    /// Crée une icône depuis une icône sémantique
    /// ([`crate::icons::Icon`]).
    pub fn new(kind: IconKind) -> Self {
        Self {
            kind,
            size: None,
            color: None,
        }
    }

    /// Taille du glyphe (préférer `theme.typography.icon_size`).
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Teinte du glyphe.
    #[must_use]
    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Affiche l'icône et retourne la réponse egui.
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let registry = IconRegistry::new();
        let mut glyph = match self.size {
            Some(size) => registry.sized(self.kind, size),
            None => registry.text(self.kind),
        };
        if let Some(color) = self.color {
            glyph = glyph.color(color);
        }
        ui.label(glyph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::Icon as IconKind;
    use crate::theme::{CygnusTheme, setup_fonts};

    #[test]
    fn icon_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Icon::new(IconKind::Save)
                    .size(theme.typography.icon_size)
                    .color(theme.colors.fg_secondary)
                    .show(ui);
            });
        })
        .drop_without_applying_deltas();
    }
}
