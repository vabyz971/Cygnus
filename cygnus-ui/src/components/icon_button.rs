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

//! Design-system icon button (toolbars, panels).
//!
//! Takes a semantic [`Icon`](crate::icons::Icon): glyph resolution
//! goes through [`IconRegistry`](crate::icons::IconRegistry),
//! never through an icon lib directly.
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::components::IconButton;
//! # use cygnus_ui::icons::Icon;
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! if IconButton::new(Icon::Save)
//!     .tooltip("Enregistrer")
//!     .show(ui, &theme)
//!     .clicked()
//! {
//!     // action triggered via UiCommand on the app side
//! }
//! # });
//! ```

use crate::icons::{Icon, IconRegistry};
use crate::theme::CygnusTheme;

/// Icon button via builder API.
#[derive(Debug, Clone)]
pub struct IconButton {
    icon: Icon,
    tooltip: Option<String>,
    selected: bool,
    size: Option<f32>,
}

impl IconButton {
    /// Creates a button for the semantic `icon`.
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            tooltip: None,
            selected: false,
            size: None,
        }
    }

    /// Hover tooltip.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Selected state (accent tint).
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Square button side (defaults to `sizes.icon_button`).
    #[must_use]
    pub fn size(mut self, side: f32) -> Self {
        self.size = Some(side);
        self
    }

    /// Displays the button and returns the egui response.
    ///
    /// Hover/pressed fills come from the theme (scoped visuals, like
    /// [`Button`](super::button::Button)): idle stays transparent,
    /// hover shows `item_hover`, pressed darkens it.
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme) -> egui::Response {
        let registry = IconRegistry::new();
        let color = if self.selected {
            theme.colors.accent
        } else {
            theme.colors.fg_secondary
        };
        let response = ui
            .scope(|ui| {
                let widgets = &mut ui.visuals_mut().widgets;
                widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                widgets.hovered.bg_fill = theme.colors.item_hover;
                widgets.hovered.fg_stroke = egui::Stroke::new(1.0, theme.colors.fg_primary);
                widgets.active.bg_fill = theme.colors.item_selected;
                let mut button = egui::Button::new(registry.colored(
                    self.icon,
                    theme.typography.icon_size,
                    color,
                ))
                .frame(true)
                .frame_when_inactive(self.selected)
                .corner_radius(theme.radius.sm)
                .min_size(egui::Vec2::splat(
                    self.size.unwrap_or(theme.sizes.icon_button),
                ));
                // Selected: resting pill fill behind the accent glyph.
                if self.selected {
                    button = button.fill(theme.colors.item_selected);
                }
                ui.add(button)
            })
            .inner;
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
