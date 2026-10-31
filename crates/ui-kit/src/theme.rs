// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Dark theme of the Cygnus applications.
//!
//! Every color, radius, spacing and font size of the interface is defined
//! here and nowhere else. Values are taken from the main screen mockup
//! (`docs/views/ecran-principal.png`).

use std::sync::Arc;

use egui::epaint::Shadow;
use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    Style, TextStyle, Vec2, Visuals,
};

/// Colors of the dark theme.
pub mod palette {
    use egui::Color32;

    /// Menu bar, persona bar, tool rail and status bar.
    pub const CHROME: Color32 = Color32::from_rgb(0x1B, 0x1E, 0x25);
    /// Options bar and dockable panels.
    pub const PANEL: Color32 = Color32::from_rgb(0x20, 0x24, 0x2C);
    /// Recessed areas: input fields, document tab bar, plots.
    pub const INSET: Color32 = Color32::from_rgb(0x17, 0x1B, 0x22);
    /// Background around the canvas.
    pub const CANVAS: Color32 = Color32::from_rgb(0x14, 0x17, 0x1C);
    /// Separators and widget outlines.
    pub const BORDER: Color32 = Color32::from_rgb(0x30, 0x35, 0x3F);
    /// Background of the selected item (persona, tool, layer).
    pub const SELECTED: Color32 = Color32::from_rgb(0x35, 0x38, 0x4E);
    /// Background of a hovered item.
    pub const HOVERED: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x38);
    /// Accent color: active marks, curves, slider fill.
    pub const ACCENT: Color32 = Color32::from_rgb(0xB5, 0xB8, 0xF8);
    /// Text and icons on a selected background.
    pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x9D, 0xA0, 0xD9);
    /// Main text.
    pub const TEXT: Color32 = Color32::from_rgb(0xE4, 0xE6, 0xEC);
    /// Secondary text: labels, captions.
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8A, 0x90, 0x9C);
    /// Disabled text and icons.
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x55, 0x5B, 0x66);
    /// Success marks (saved state, GPU ready).
    pub const SUCCESS: Color32 = Color32::from_rgb(0x8F, 0xD1, 0x9E);
    /// Red, green and blue channels in plots.
    pub const CHANNELS: [Color32; 3] = [
        Color32::from_rgb(0xC0, 0x6C, 0x84),
        Color32::from_rgb(0x6C, 0xA0, 0x8C),
        Color32::from_rgb(0x6C, 0x84, 0xC0),
    ];
}

/// Sizes of the dark theme, in points.
pub mod metrics {
    /// Corner radius of buttons and fields.
    pub const RADIUS: u8 = 6;
    /// Corner radius of large items (persona tabs, windows).
    pub const RADIUS_LARGE: u8 = 8;
    /// Height of the menu bar.
    pub const MENU_BAR_HEIGHT: f32 = 40.0;
    /// Height of the persona bar.
    pub const PERSONA_BAR_HEIGHT: f32 = 62.0;
    /// Height of the tool options bar.
    pub const OPTIONS_BAR_HEIGHT: f32 = 44.0;
    /// Height of the document tab bar.
    pub const DOCUMENT_TABS_HEIGHT: f32 = 40.0;
    /// Height of the status bar.
    pub const STATUS_BAR_HEIGHT: f32 = 34.0;
    /// Width of the tool rail.
    pub const TOOL_RAIL_WIDTH: f32 = 56.0;
    /// Side of a tool rail button.
    pub const TOOL_BUTTON_SIZE: f32 = 36.0;
    /// Default width of the dockable panel column.
    pub const DOCK_DEFAULT_WIDTH: f32 = 320.0;
    /// Minimum width of the dockable panel column.
    pub const DOCK_MIN_WIDTH: f32 = 240.0;
    /// Horizontal padding inside bars and panels.
    pub const PADDING: f32 = 12.0;
    /// Space between two widgets.
    pub const SPACING: f32 = 8.0;
    /// Body text size.
    pub const FONT_BODY: f32 = 13.0;
    /// Small text size (captions, status bar).
    pub const FONT_SMALL: f32 = 11.5;
    /// Heading size (panel titles, application name).
    pub const FONT_HEADING: f32 = 13.5;
    /// Monospace text size (numbers, rulers).
    pub const FONT_MONO: f32 = 11.5;
    /// Icon size in bars and rails.
    pub const ICON: f32 = 18.0;
}

/// Font family of headings (Inter SemiBold).
pub fn heading_family() -> FontFamily {
    FontFamily::Name(HEADING_FAMILY.into())
}

/// Font family of icons (Phosphor). Use it for every icon glyph, through
/// [`crate::widgets::icon`].
pub fn icon_family() -> FontFamily {
    FontFamily::Name(ICON_FAMILY.into())
}

const HEADING_FAMILY: &str = "cygnus-heading";
const ICON_FAMILY: &str = "cygnus-icons";
const INTER_REGULAR: &str = "inter-regular";
const INTER_SEMIBOLD: &str = "inter-semibold";
const MONO: &str = "jetbrains-mono";
const ICONS: &str = "phosphor";

/// Fonts of the theme: Inter and JetBrains Mono (SIL OFL 1.1, see
/// docs/decisions/0006), plus Phosphor icons as a fallback of every family.
pub fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    for (name, bytes) in [
        (
            INTER_REGULAR,
            &include_bytes!("../assets/fonts/Inter-Regular.ttf")[..],
        ),
        (
            INTER_SEMIBOLD,
            &include_bytes!("../assets/fonts/Inter-SemiBold.ttf")[..],
        ),
        (
            MONO,
            &include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    fonts
        .families
        .insert(FontFamily::Proportional, vec![INTER_REGULAR.to_owned()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec![MONO.to_owned()]);
    fonts
        .families
        .insert(heading_family(), vec![INTER_SEMIBOLD.to_owned()]);

    // Icons have a family of their own, where Phosphor comes first: Inter
    // has glyphs at some of the private-use code points Phosphor uses, and
    // Phosphor has glyphs for some letters, so neither font can come first
    // for both text and icons. Text families keep Phosphor as a fallback.
    fonts.font_data.insert(
        ICONS.to_owned(),
        Arc::new(FontData::from_static(
            egui_phosphor::Variant::Regular.font_bytes(),
        )),
    );
    for list in fonts.families.values_mut() {
        list.push(ICONS.to_owned());
    }
    fonts.families.insert(
        icon_family(),
        vec![ICONS.to_owned(), INTER_REGULAR.to_owned()],
    );
    fonts
}

/// egui style of the dark theme.
pub fn style() -> Style {
    use metrics::*;
    use palette::*;

    let mut visuals = Visuals::dark();
    visuals.override_text_color = None;
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(RADIUS_LARGE);
    visuals.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    visuals.popup_shadow = visuals.window_shadow;
    visuals.menu_corner_radius = CornerRadius::same(RADIUS);
    visuals.extreme_bg_color = INSET;
    visuals.faint_bg_color = INSET;
    visuals.code_bg_color = INSET;
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;

    let radius = CornerRadius::same(RADIUS);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = PANEL;
    widgets.noninteractive.weak_bg_fill = PANEL;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    widgets.noninteractive.corner_radius = radius;

    widgets.inactive.bg_fill = INSET;
    widgets.inactive.weak_bg_fill = INSET;
    widgets.inactive.bg_stroke = Stroke::NONE;
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.inactive.corner_radius = radius;

    widgets.hovered.bg_fill = HOVERED;
    widgets.hovered.weak_bg_fill = HOVERED;
    widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;

    widgets.active.bg_fill = SELECTED;
    widgets.active.weak_bg_fill = SELECTED;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;

    widgets.open = widgets.active;

    let mut style = Style {
        visuals,
        ..Style::default()
    };
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(FONT_SMALL)),
        (TextStyle::Body, FontId::proportional(FONT_BODY)),
        (TextStyle::Button, FontId::proportional(FONT_BODY)),
        (
            TextStyle::Heading,
            FontId::new(FONT_HEADING, heading_family()),
        ),
        (TextStyle::Monospace, FontId::monospace(FONT_MONO)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(SPACING, 6.0);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.interact_size = Vec2::new(40.0, 24.0);
    style.spacing.window_margin = Margin::same(10);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.slider_rail_height = 3.0;
    style.spacing.combo_height = 300.0;
    style
}

/// Style of the dockable panels, derived from the egui style.
pub fn dock_style(egui_style: &Style) -> egui_dock::Style {
    use palette::*;

    let mut style = egui_dock::Style::from_egui(egui_style);
    style.dock_area_padding = None;
    style.main_surface_border_stroke = Stroke::NONE;
    style.main_surface_border_rounding = CornerRadius::ZERO;

    style.separator.width = 1.0;
    style.separator.extra = 0.0;
    style.separator.color_idle = BORDER;
    style.separator.color_hovered = ACCENT_TEXT;
    style.separator.color_dragged = ACCENT;

    style.tab_bar.bg_fill = PANEL;
    style.tab_bar.height = 34.0;
    style.tab_bar.hline_color = BORDER;
    style.tab_bar.corner_radius = CornerRadius::ZERO;

    style.tab.tab_body.bg_fill = PANEL;
    style.tab.tab_body.stroke = Stroke::NONE;
    style.tab.tab_body.inner_margin = Margin::symmetric(metrics::PADDING as i8, 8);
    style.tab.hline_below_active_tab_name = false;
    for (state, text) in [
        (&mut style.tab.active, TEXT),
        (&mut style.tab.focused, TEXT),
        (&mut style.tab.active_with_kb_focus, TEXT),
        (&mut style.tab.focused_with_kb_focus, TEXT),
        (&mut style.tab.inactive, TEXT_MUTED),
        (&mut style.tab.inactive_with_kb_focus, TEXT_MUTED),
        (&mut style.tab.hovered, TEXT),
    ] {
        state.bg_fill = PANEL;
        state.outline_color = PANEL;
        state.text_color = text;
        state.corner_radius = CornerRadius::ZERO;
    }

    style.buttons.close_tab_color = TEXT_MUTED;
    style.buttons.close_tab_active_color = TEXT;
    style.buttons.close_tab_bg_fill = HOVERED;

    style.overlay.selection_color = ACCENT.gamma_multiply(0.35);
    style.overlay.button_color = ACCENT;
    style.overlay.button_border_stroke = Stroke::new(1.0, BORDER);
    style
}

/// Installs the fonts and the style of the theme on `ctx`.
pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_global_style(style());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_style_uses_a_registered_family() {
        let fonts = fonts();
        for font in style().text_styles.values() {
            let family = fonts.families.get(&font.family);
            assert!(family.is_some_and(|list| !list.is_empty()));
        }
    }

    #[test]
    fn icon_family_starts_with_icons_and_text_families_do_not() {
        let fonts = fonts();
        for (family, list) in &fonts.families {
            let first_is_icons = list.first().map(String::as_str) == Some(ICONS);
            assert_eq!(first_is_icons, *family == icon_family(), "{family:?}");
            assert!(list.iter().any(|name| name == ICONS));
        }
    }
}
