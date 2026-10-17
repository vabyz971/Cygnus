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

//! Single-line text input (rename, search…) and
//! numeric field (dimensions, zoom…).
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::components::{NumberInput, TextInput};
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut name = String::from("Calque 1");
//! TextInput::new()
//!     .placeholder("Nom du calque")
//!     .show(ui, &theme, &mut name);
//! let mut width = 1920.0;
//! NumberInput::new("Largeur")
//!     .range(1.0..=8192.0)
//!     .show(ui, &theme, &mut width);
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Text field via builder API.
#[derive(Debug, Clone, Default)]
pub struct TextInput<'a> {
    placeholder: Option<&'a str>,
}

impl<'a> TextInput<'a> {
    /// Creates an empty field.
    pub fn new() -> Self {
        Self::default()
    }

    /// Help text shown when empty.
    #[must_use]
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Displays the field, updates `text`, returns the egui response.
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme, text: &mut String) -> egui::Response {
        let mut edit = egui::TextEdit::singleline(text)
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Body)
            .margin(egui::vec2(theme.spacing.sm, theme.spacing.xs));
        if let Some(hint) = self.placeholder {
            edit = edit.hint_text(
                egui::RichText::new(hint)
                    .size(theme.typography.body_size)
                    .color(theme.colors.fg_subdued),
            );
        }
        // Recessed field with a hairline (buttons stay outline-free):
        // the frame reads the inactive/hovered strokes.
        ui.scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.bg_stroke = egui::Stroke::new(theme.borders.thin, theme.colors.border);
            widgets.hovered.bg_stroke =
                egui::Stroke::new(theme.borders.thin, theme.colors.fg_subdued);
            ui.add(edit.background_color(theme.colors.bg_tertiary))
        })
        .inner
    }
}

/// Numeric field (label + `DragValue`) via builder API.
#[derive(Debug, Clone, Default)]
pub struct NumberInput<'a> {
    label: &'a str,
    range: Option<std::ops::RangeInclusive<f64>>,
    /// True = label above the field (vertical column).
    stacked: bool,
}

impl<'a> NumberInput<'a> {
    /// Creates a numeric field with label.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            range: None,
            stacked: false,
        }
    }

    /// Allowed value range.
    #[must_use]
    pub fn range(mut self, range: std::ops::RangeInclusive<f64>) -> Self {
        self.range = Some(range);
        self
    }

    /// Stacks the label above the field (instead of side by side).
    #[must_use]
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    /// Displays the field, updates `value`, returns the egui response.
    /// In [`NumberInput::stacked`] mode, the label sits above the field.
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme, value: &mut f64) -> egui::Response {
        let label = egui::RichText::new(self.label)
            .size(theme.typography.body_size)
            .color(theme.colors.fg_secondary);
        if self.stacked {
            ui.vertical(|ui| {
                ui.label(label);
                let mut edit = egui::DragValue::new(value).speed(0.1);
                if let Some(range) = self.range {
                    edit = edit.range(range);
                }
                ui.add(edit)
            })
            .inner
        } else {
            ui.horizontal(|ui| {
                ui.label(label);
                let mut edit = egui::DragValue::new(value).speed(0.1);
                if let Some(range) = self.range {
                    edit = edit.range(range);
                }
                ui.add(edit)
            })
            .inner
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut name = String::from("Calque 1");
                let _ = TextInput::new()
                    .placeholder("Nom")
                    .show(ui, &theme, &mut name);
                let mut width = 1920.0;
                let _ = NumberInput::new("Largeur")
                    .range(1.0..=8192.0)
                    .show(ui, &theme, &mut width);
            });
        })
        .drop_without_applying_deltas();
    }
}
