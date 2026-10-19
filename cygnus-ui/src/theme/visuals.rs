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

//! Font setup and Cygnus theme application on egui.
//!
//! Call EXACTLY ONCE at each app's startup, in this order:
//! 1. [`setup_fonts`]: UI font (Hanken Grotesk) + icon font.
//! 2. [`apply_cygnus_theme`]: colors, spacing and radii.

use super::CygnusTheme;
use super::context::install_theme;

/// Loads the UI font (Hanken Grotesk) THEN the Material icon font
/// Symbols via `egui_material_icons`.
///
/// Call once at each app's boot, BEFORE
/// [`apply_cygnus_theme`].
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 1. Hanken Grotesk UI font (versioned under assets/fonts/).
    fonts.font_data.insert(
        "hanken_grotesk".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/HankenGrotesk-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "hanken_grotesk".to_owned());

    ctx.set_fonts(fonts);

    // 2. Material Symbols icon font via the dedicated lib
    //    (never hand-managed codepoints).
    egui_material_icons::initialize(ctx);
}

/// Applies the Cygnus theme to the egui context.
///
/// Call EXACTLY ONCE at each app's startup, after
/// [`setup_fonts`]. Every egui widget (buttons, combos, sliders, menus,
/// windows, scroll bars) is restyled from the tokens so stock widgets
/// and Cygnus components look like one system — the Rerun approach.
pub fn apply_cygnus_theme(ctx: &egui::Context) {
    let theme = CygnusTheme::dark();
    install_theme(ctx, theme);
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| apply_to_style(&theme, style));
}

/// Writes the theme into an egui style (pure: unit-testable).
pub fn apply_to_style(theme: &CygnusTheme, style: &mut egui::Style) {
    let c = &theme.colors;
    let small = egui::CornerRadius::same(theme.radius.sm as u8);
    let divider = egui::Stroke::new(theme.borders.thin, c.border);

    // Text styles: one scale for every stock widget.
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(theme.typography.heading_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(theme.typography.body_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(theme.typography.body_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(theme.typography.caption_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(theme.typography.body_size, FontFamily::Monospace),
        ),
    ]
    .into();

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    // No global override: widgets pick default / strong text per state,
    // which gives the strong / default / subdued hierarchy.
    v.override_text_color = None;
    v.panel_fill = c.bg_primary;
    v.window_fill = c.bg_primary;
    v.window_stroke = divider;
    v.faint_bg_color = c.bg_secondary;
    v.extreme_bg_color = c.bg_tertiary;
    v.text_edit_bg_color = Some(c.bg_tertiary);
    v.code_bg_color = c.bg_tertiary;
    v.hyperlink_color = c.accent_hover;
    v.error_fg_color = c.error;
    v.warn_fg_color = c.warning;
    v.selection.bg_fill = c.item_selected;
    v.selection.stroke = egui::Stroke::new(theme.borders.medium, c.fg_strong);
    v.window_corner_radius = egui::CornerRadius::same(theme.radius.lg as u8);
    v.menu_corner_radius = egui::CornerRadius::same(theme.radius.md as u8);
    let shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: c.shadow,
    };
    v.popup_shadow = shadow;
    v.window_shadow = shadow;
    v.striped = false;
    v.indent_has_left_vline = false;

    // Widget states: flat (no outline), fill appears on hover/press,
    // text goes strong when interacted with.
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.bg_primary;
    w.noninteractive.weak_bg_fill = c.bg_primary;
    w.noninteractive.bg_stroke = divider;
    w.noninteractive.fg_stroke = egui::Stroke::new(theme.borders.thin, c.fg_primary);
    w.inactive.bg_fill = c.widget_bg;
    w.inactive.weak_bg_fill = c.widget_bg;
    w.inactive.fg_stroke = egui::Stroke::new(theme.borders.thin, c.fg_primary);
    w.hovered.bg_fill = c.item_hover;
    w.hovered.weak_bg_fill = c.item_hover;
    w.hovered.fg_stroke = egui::Stroke::new(theme.borders.thin, c.fg_strong);
    w.active.bg_fill = c.widget_bg;
    w.active.weak_bg_fill = c.widget_bg;
    w.active.fg_stroke = egui::Stroke::new(theme.borders.medium, c.fg_strong);
    w.open.bg_fill = c.item_hover;
    w.open.weak_bg_fill = c.item_hover;
    w.open.fg_stroke = egui::Stroke::new(theme.borders.thin, c.fg_strong);
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.corner_radius = small;
    }
    for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        state.bg_stroke = egui::Stroke::NONE;
    }
    for state in [&mut w.hovered, &mut w.active, &mut w.open] {
        state.expansion = theme.spacing.xxs;
    }

    // Spacing: 4 px grid, compact rows, thin flush scroll bars.
    let sp = &mut style.spacing;
    sp.item_spacing = egui::vec2(theme.spacing.sm, theme.spacing.sm);
    sp.button_padding = egui::vec2(theme.spacing.sm, theme.spacing.xxs);
    sp.menu_margin = egui::Margin::same(theme.spacing.sm as i8);
    sp.window_margin = egui::Margin::same(theme.spacing.md as i8);
    sp.menu_spacing = theme.spacing.xxs;
    sp.indent = theme.spacing.md + theme.spacing.xxs;
    sp.interact_size.y = theme.sizes.list_item - theme.spacing.sm;
    sp.combo_width = theme.spacing.sm;
    sp.scroll.bar_width = theme.spacing.xs + theme.spacing.xxs;
    sp.scroll.bar_outer_margin = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_follows_the_tokens() {
        let theme = CygnusTheme::dark();
        let mut style = egui::Style::default();
        apply_to_style(&theme, &mut style);
        assert_eq!(style.visuals.panel_fill, theme.colors.bg_primary);
        assert_eq!(style.visuals.override_text_color, None);
        assert_eq!(
            style.visuals.widgets.inactive.bg_stroke,
            egui::Stroke::NONE,
            "boutons sans contour"
        );
        assert_eq!(
            style.text_styles[&egui::TextStyle::Body].size,
            theme.typography.body_size
        );
        assert_eq!(style.visuals.selection.bg_fill, theme.colors.item_selected);
    }

    #[test]
    fn theme_applies_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        apply_cygnus_theme(&ctx);
        assert!(ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
    }
}
