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

//! Pile horizontale ou verticale à espacement calibré.
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::containers::Stack;
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Stack::horizontal(&theme).show(ui, |ui| {
//!     ui.label("a");
//!     ui.label("b");
//! });
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Direction d'une pile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Horizontal,
    Vertical,
}

/// Pile de widgets via API builder.
#[derive(Debug, Clone, Copy)]
pub struct Stack {
    direction: Direction,
    spacing: f32,
}

impl Stack {
    /// Pile horizontale (espacement `sm` du thème).
    pub fn horizontal(theme: &CygnusTheme) -> Self {
        Self {
            direction: Direction::Horizontal,
            spacing: theme.spacing.sm,
        }
    }

    /// Pile verticale (espacement `sm` du thème).
    pub fn vertical(theme: &CygnusTheme) -> Self {
        Self {
            direction: Direction::Vertical,
            spacing: theme.spacing.sm,
        }
    }

    /// Espacement entre enfants (préférer un token `theme.spacing.*`).
    #[must_use]
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Affiche la pile et son contenu.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        match self.direction {
            Direction::Horizontal => ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = self.spacing;
                add_contents(ui)
            }),
            Direction::Vertical => ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = self.spacing;
                add_contents(ui)
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Stack::horizontal(&theme)
                    .spacing(theme.spacing.md)
                    .show(ui, |ui| {
                        ui.label("a");
                    });
                Stack::vertical(&theme).show(ui, |ui| {
                    ui.label("b");
                });
            });
        })
        .drop_without_applying_deltas();
    }
}
