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

//! Horizontal or vertical stack with calibrated spacing.
//!
//! # Example
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

/// A stack's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Horizontal,
    Vertical,
}

/// Widget stack via builder API.
#[derive(Debug, Clone, Copy)]
pub struct Stack {
    direction: Direction,
    spacing: f32,
}

impl Stack {
    /// Horizontal stack (theme `sm` spacing).
    pub fn horizontal(theme: &CygnusTheme) -> Self {
        Self {
            direction: Direction::Horizontal,
            spacing: theme.spacing.sm,
        }
    }

    /// Vertical stack (theme `sm` spacing).
    pub fn vertical(theme: &CygnusTheme) -> Self {
        Self {
            direction: Direction::Vertical,
            spacing: theme.spacing.sm,
        }
    }

    /// Spacing between children (prefer a `theme.spacing.*` token).
    #[must_use]
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Displays the stack and its content.
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
