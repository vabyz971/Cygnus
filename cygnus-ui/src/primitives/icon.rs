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

//! Icon: display primitive for a semantic icon.
//!
//! Thin wrapper around [`crate::icons::Icon`]: the primitive
//! resolves no glyph itself, it delegates to the registry.
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::primitives::Icon;
//! # use cygnus_ui::icons::Icon as IconKind;
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Icon::new(IconKind::Save)
//!     .size(theme.typography.icon_size)
//!     .color(theme.colors.fg_secondary)
//!     .show(ui);
//! # });
//! ```

use crate::icons::{Icon as IconKind, IconRegistry};

/// Glyph displayable via a builder API.
#[derive(Debug, Clone, Copy)]
pub struct Icon {
    kind: IconKind,
    size: Option<f32>,
    color: Option<egui::Color32>,
}

impl Icon {
    /// Creates an icon from a semantic icon
    /// ([`crate::icons::Icon`]).
    pub fn new(kind: IconKind) -> Self {
        Self {
            kind,
            size: None,
            color: None,
        }
    }

    /// Glyph size (prefer `theme.typography.icon_size`).
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Glyph tint.
    #[must_use]
    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Displays the icon and returns the egui response.
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
