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

//! Shared menu and context-popup style (rerun-style).
//!
//! Compact rows, tight margins, theme corners: apply via
//! `egui::Popup::context_menu(...).style(menu_style(theme))`.
//! A `menu_bar_style` variant exists for the top menu bar
//! (square corners, no rounding on title buttons).
//! The rows themselves use [`menu_row`](super::button::menu_row).
//! 100% tokens, no hardcoded value.

use crate::theme::CygnusTheme;

/// Menu-popup style, derived from the theme.
pub fn menu_style(theme: &CygnusTheme) -> egui::style::StyleModifier {
    let row_height = theme.sizes.button_sm;
    let margin = theme.spacing.xs;
    let padding_x = theme.spacing.xs;
    let corner = theme.radius.sm;
    egui::style::StyleModifier::new(move |style| {
        egui::containers::menu::menu_style(style);
        style.spacing.interact_size.y = row_height;
        style.spacing.menu_margin = margin.into();
        style.spacing.button_padding.x = padding_x;
        style.spacing.item_spacing.y = 0.0;
        for visual in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.open,
            &mut style.visuals.widgets.noninteractive,
        ] {
            visual.expansion = 0.0;
            visual.corner_radius = corner.into();
        }
    })
}

/// Top menu-bar style.
pub fn menu_bar_style(theme: &CygnusTheme) -> egui::style::StyleModifier {
    let row_height = theme.sizes.button_sm;
    let margin = theme.spacing.xs;
    let padding_x = theme.spacing.xs;
    let corner = theme.radius.sm;
    egui::style::StyleModifier::new(move |style| {
        style.spacing.interact_size.y = row_height;
        style.spacing.menu_margin = margin.into();
        style.spacing.button_padding.x = padding_x;
        style.spacing.item_spacing.y = 0.0;
        for visual in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.open,
            &mut style.visuals.widgets.noninteractive,
        ] {
            visual.expansion = 0.0;
            visual.corner_radius = corner.into();
        }
    })
}

/// Menu item: full-width ghost button; `true` when clicked
/// (and menu closed).
pub fn menu_item(ui: &mut egui::Ui, theme: &CygnusTheme, label: &str) -> bool {
    use crate::components::Button;
    use crate::components::ButtonSize;
    use crate::components::ButtonVariant;
    let clicked = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .show(ui, theme)
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Disablable menu item (replaces `add_enabled_ui` + raw button).
///
/// Same height as [`menu_item`] (`Small`, calibrated on the menu
/// style's `interact_size`): mixing sizes in one popup paints
/// overflowing highlights (glitched squares).
pub fn menu_item_enabled(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    label: &str,
    enabled: bool,
) -> bool {
    use crate::components::Button;
    use crate::components::ButtonSize;
    use crate::components::ButtonVariant;
    let clicked = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .enabled(enabled)
        .show(ui, theme)
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Menu rows: see [`menu_row`](super::button::menu_row),
/// re-exported here for a single import.
pub use super::button::menu_row;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_style_comes_from_theme_tokens() {
        let theme = CygnusTheme::dark();
        let mut style = egui::Style::default();
        menu_style(&theme).apply(&mut style);
        assert_eq!(style.spacing.interact_size.y, theme.sizes.button_sm);
        assert_eq!(style.spacing.item_spacing.y, 0.0);
        assert_eq!(
            style.visuals.widgets.hovered.corner_radius,
            theme.radius.sm.into()
        );
        // Kept egui base: no outline on rows.
        assert_eq!(style.visuals.widgets.inactive.bg_stroke, egui::Stroke::NONE);
    }

    #[test]
    fn reexported_menu_row_renders_idle() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        let mut clicked = true;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                clicked = menu_row(ui, &theme, "Dupliquer");
            });
        })
        .drop_without_applying_deltas();
        assert!(!clicked, "aucun clic sans interaction");
    }

    /// Both item variants reserve the same height: mixed in one popup,
    /// no highlight overflows (glitched squares).
    #[test]
    fn menu_items_share_row_height() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        let mut heights = Vec::new();
        fn measure(ui: &mut egui::Ui, heights: &mut Vec<f32>, draw: impl FnOnce(&mut egui::Ui)) {
            let before = ui.min_rect().max.y;
            draw(ui);
            heights.push(ui.min_rect().max.y - before);
        }
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                measure(ui, &mut heights, |ui| {
                    menu_item(ui, &theme, "Toujours");
                });
                measure(ui, &mut heights, |ui| {
                    menu_item_enabled(ui, &theme, "Conditionnel", true);
                });
                measure(ui, &mut heights, |ui| {
                    menu_item_enabled(ui, &theme, "Grisé", false);
                });
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(heights.len(), 3);
        assert!(
            heights
                .windows(2)
                .all(|w| (w[0] - w[1]).abs() < f32::EPSILON),
            "rangées uniformes : {heights:?}"
        );
    }
}
