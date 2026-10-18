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

//! Background-free titled section (groups components).
//!
//! Unlike [`Panel`](super::panel::Panel), the section draws
//! no surface: title + content, no chrome.
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::containers::Section;
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Section::new("Transform").show(ui, &theme, |ui| {
//!     ui.label("position, rotation, echelle");
//! });
//! # });
//! ```

use crate::primitives::{Text, divider};
use crate::theme::CygnusTheme;

/// Titled section via builder API.
#[derive(Debug, Clone)]
pub struct Section<'a> {
    title: &'a str,
}

impl<'a> Section<'a> {
    /// Creates a section with title.
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    /// Displays the section and its content.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        ui.vertical(|ui| {
            Text::heading(theme, self.title).show(ui);
            divider(ui, theme);
            add_contents(ui)
        })
    }
}

/// Panel section header row: strong title on the left, `trailing`
/// widgets laid out right-to-left (caption, overflow menu…), one
/// tab-bar height tall so it lines up with dock tab bars.
pub fn section_header<R>(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    title: &str,
    trailing: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(theme.sizes.tab_height);
        ui.label(
            egui::RichText::new(title)
                .size(theme.typography.body_size)
                .strong()
                .color(theme.colors.fg_strong),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing)
            .inner
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Section::new("Transform").show(ui, &theme, |ui| {
                    ui.label("contenu");
                });
                let trailing = section_header(ui, &theme, "Histogramme", |ui| {
                    ui.label("RVB");
                    7
                });
                assert_eq!(trailing, 7);
            });
        })
        .drop_without_applying_deltas();
    }
}
