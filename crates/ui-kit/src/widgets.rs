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

//! Building blocks of an application main screen: menu bar, persona tabs,
//! options bar, tool rail, document tabs and status bar.
//!
//! Widgets take already translated texts: they never hold strings of their
//! own.

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Panel, Response, RichText, Sense,
    Stroke, Ui, Vec2,
};

use crate::theme::{heading_family, icon_family, metrics, palette};

/// An icon glyph (from [`crate::icons`]) at the given size, in the icon font.
pub fn icon(glyph: &str, size: f32) -> RichText {
    RichText::new(glyph).family(icon_family()).size(size)
}

/// An icon followed by a text, on one line.
pub fn icon_label(ui: &mut Ui, glyph: &str, text: &str, color: Color32) -> Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = metrics::SPACING;
        ui.label(icon(glyph, metrics::ICON - 2.0).color(color));
        ui.label(RichText::new(text).color(color));
    })
    .response
}

fn bar_frame(fill: Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(metrics::PADDING as i8, 0))
}

fn bottom_border(ui: &Ui, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, palette::BORDER),
    );
}

/// Shows a horizontal bar docked at the top of `ui`, with a bottom border.
fn top_bar(
    ui: &mut Ui,
    id: &'static str,
    height: f32,
    fill: Color32,
    contents: impl FnOnce(&mut Ui),
) -> Response {
    let response = Panel::top(id)
        .exact_size(height)
        .resizable(false)
        .frame(bar_frame(fill))
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), contents);
        })
        .response;
    bottom_border(ui, response.rect);
    response
}

/// Top menu bar: application name followed by the menus added by `menus`.
pub fn menu_bar(ui: &mut Ui, app_name: &str, menus: impl FnOnce(&mut Ui)) {
    top_bar(
        ui,
        "cygnus-menu-bar",
        metrics::MENU_BAR_HEIGHT,
        palette::CHROME,
        |ui| {
            ui.label(
                RichText::new(app_name)
                    .family(heading_family())
                    .size(metrics::FONT_HEADING + 1.0)
                    .color(palette::TEXT),
            );
            ui.add_space(metrics::PADDING);
            egui::MenuBar::new().ui(ui, |ui| {
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.fg_stroke.color = palette::TEXT_MUTED;
                menus(ui);
            });
        },
    );
}

/// A persona tab (a mode of the application, such as Photo or Export).
pub struct Persona<'a> {
    /// Icon glyph, from [`crate::icons`].
    pub icon: &'a str,
    /// Translated label.
    pub label: &'a str,
    /// Disabled personas are shown greyed and cannot be selected.
    pub enabled: bool,
    /// Tooltip shown when hovering a disabled persona.
    pub disabled_hint: &'a str,
}

/// Persona bar: logo, persona tabs, then `trailing` contents aligned right.
pub fn persona_bar(
    ui: &mut Ui,
    logo: &str,
    personas: &[Persona<'_>],
    selected: &mut usize,
    trailing: impl FnOnce(&mut Ui),
) {
    top_bar(
        ui,
        "cygnus-persona-bar",
        metrics::PERSONA_BAR_HEIGHT,
        palette::CHROME,
        |ui| {
            ui.label(icon(logo, metrics::ICON + 6.0).color(palette::TEXT));
            ui.add_space(metrics::SPACING);
            vertical_separator(ui, 24.0);
            ui.add_space(metrics::SPACING);
            for (index, persona) in personas.iter().enumerate() {
                if persona_tab(ui, persona, index == *selected).clicked() && persona.enabled {
                    *selected = index;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), trailing);
        },
    );
}

fn persona_tab(ui: &mut Ui, persona: &Persona<'_>, selected: bool) -> Response {
    let color = match (persona.enabled, selected) {
        (false, _) => palette::TEXT_DISABLED,
        (true, true) => palette::TEXT,
        (true, false) => palette::TEXT_MUTED,
    };
    let button = egui::Button::new((
        icon(persona.icon, metrics::ICON - 2.0).color(color),
        RichText::new(persona.label).color(color),
    ))
    .min_size(Vec2::new(0.0, 40.0))
    .corner_radius(CornerRadius::same(metrics::RADIUS_LARGE))
    .fill(if selected {
        palette::SELECTED
    } else {
        Color32::TRANSPARENT
    })
    .stroke(Stroke::NONE);
    let response = ui.add_enabled(persona.enabled, button);
    if persona.enabled {
        response
    } else {
        response.on_disabled_hover_text(persona.disabled_hint)
    }
}

/// Thin vertical line between groups of widgets.
pub fn vertical_separator(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, palette::BORDER),
    );
}

/// Contextual options bar of the active tool.
pub fn options_bar(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    top_bar(
        ui,
        "cygnus-options-bar",
        metrics::OPTIONS_BAR_HEIGHT,
        palette::PANEL,
        contents,
    );
}

/// A label followed by a drop-down showing `value`, as in the options bar.
/// `options` fills the drop-down.
pub fn labeled_combo(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: &str,
    width: f32,
    options: impl FnOnce(&mut Ui),
) {
    ui.label(RichText::new(label).color(palette::TEXT_MUTED));
    egui::ComboBox::from_id_salt(id)
        .selected_text(RichText::new(value).color(palette::TEXT))
        .width(width)
        .show_ui(ui, options);
    ui.add_space(metrics::SPACING);
}

/// A tool of the tool rail.
pub struct Tool<'a> {
    /// Icon glyph, from [`crate::icons`].
    pub icon: &'a str,
    /// Translated name, shown as a tooltip.
    pub name: &'a str,
}

/// Vertical tool rail on the left. `footer` is drawn at the bottom of the
/// rail (for example the color swatches).
pub fn tool_rail(
    ui: &mut Ui,
    tools: &[Tool<'_>],
    selected: &mut usize,
    footer: impl FnOnce(&mut Ui),
) {
    let response = Panel::left("cygnus-tool-rail")
        .exact_size(metrics::TOOL_RAIL_WIDTH)
        .resizable(false)
        .frame(
            Frame::new()
                .fill(palette::CHROME)
                .inner_margin(Margin::symmetric(0, 10)),
        )
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.y = 7.0;
                for (index, tool) in tools.iter().enumerate() {
                    if tool_button(ui, tool, index == *selected).clicked() {
                        *selected = index;
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::Center), footer);
            });
        })
        .response;
    ui.painter().vline(
        response.rect.right() - 0.5,
        response.rect.y_range(),
        Stroke::new(1.0, palette::BORDER),
    );
}

fn tool_button(ui: &mut Ui, tool: &Tool<'_>, selected: bool) -> Response {
    let color = if selected {
        palette::ACCENT_TEXT
    } else {
        palette::TEXT_MUTED
    };
    let button = egui::Button::new(icon(tool.icon, metrics::ICON).color(color))
        .min_size(Vec2::splat(metrics::TOOL_BUTTON_SIZE))
        .corner_radius(CornerRadius::same(metrics::RADIUS))
        .fill(if selected {
            palette::SELECTED
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE);
    ui.add(button).on_hover_text(tool.name)
}

/// What happened in the document tab bar this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTabsResponse {
    /// Index of the tab whose close button was clicked.
    pub closed: Option<usize>,
}

/// A document tab.
pub struct DocumentTab<'a> {
    /// Translated title (usually the file name).
    pub title: &'a str,
    /// Tooltip of the close button.
    pub close_hint: &'a str,
}

/// Document tab bar above the canvas; `trailing` is aligned right (document
/// size, color format...).
pub fn document_tabs(
    ui: &mut Ui,
    tabs: &[DocumentTab<'_>],
    active: &mut usize,
    trailing: impl FnOnce(&mut Ui),
) -> DocumentTabsResponse {
    let mut result = DocumentTabsResponse::default();
    let response = Panel::top("cygnus-document-tabs")
        .exact_size(metrics::DOCUMENT_TABS_HEIGHT)
        .resizable(false)
        .frame(Frame::new().fill(palette::INSET))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (index, tab) in tabs.iter().enumerate() {
                    let (clicked, closed) = document_tab(ui, tab, index == *active);
                    if clicked {
                        *active = index;
                    }
                    if closed {
                        result.closed = Some(index);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(metrics::PADDING);
                    ui.spacing_mut().item_spacing.x = metrics::SPACING * 1.5;
                    trailing(ui);
                });
            });
        })
        .response;
    bottom_border(ui, response.rect);
    result
}

fn document_tab(ui: &mut Ui, tab: &DocumentTab<'_>, active: bool) -> (bool, bool) {
    let fill = if active {
        palette::CANVAS
    } else {
        palette::INSET
    };
    let inner = Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(metrics::PADDING as i8 + 4, 0))
        .show(ui, |ui| {
            ui.set_height(metrics::DOCUMENT_TABS_HEIGHT);
            ui.spacing_mut().item_spacing.x = metrics::SPACING;
            let color = if active {
                palette::TEXT
            } else {
                palette::TEXT_MUTED
            };
            let title =
                icon_label(ui, crate::icons::IMAGE, tab.title, color).interact(Sense::click());
            ui.add_space(metrics::PADDING * 3.0);
            let close = ui
                .add(
                    egui::Button::new(
                        icon(crate::icons::X, metrics::ICON - 4.0).color(palette::TEXT_MUTED),
                    )
                    .frame(false),
                )
                .on_hover_text(tab.close_hint);
            (title.clicked(), close.clicked())
        });
    if active {
        let rect = inner.response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.top() + 1.0,
            Stroke::new(2.0, palette::ACCENT),
        );
    }
    inner.inner
}

/// Status bar at the bottom of the window.
pub fn status_bar(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    let response = Panel::bottom("cygnus-status-bar")
        .exact_size(metrics::STATUS_BAR_HEIGHT)
        .resizable(false)
        .frame(bar_frame(palette::CHROME))
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.style_mut().override_font_id = Some(FontId::proportional(metrics::FONT_SMALL));
                contents(ui);
            });
        })
        .response;
    ui.painter().hline(
        response.rect.x_range(),
        response.rect.top() + 0.5,
        Stroke::new(1.0, palette::BORDER),
    );
}

/// Icon-only button without frame, for toolbars and panel footers.
pub fn icon_button(ui: &mut Ui, glyph: &str, hint: &str) -> Response {
    ui.add(
        egui::Button::new(self::icon(glyph, metrics::ICON - 2.0).color(palette::TEXT_MUTED))
            .frame(false),
    )
    .on_hover_text(hint)
}

/// Horizontal slider filling the available width, with the accent-colored
/// track of the mockup and no value text.
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> Response {
    ui.scope(|ui| {
        let visuals = ui.visuals_mut();
        // The rail and the resting handle share `inactive.bg_fill`; the
        // handle is told apart by its accent outline.
        visuals.selection.bg_fill = palette::ACCENT;
        for state in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            state.bg_fill = palette::BORDER;
            state.fg_stroke = Stroke::new(1.5, palette::ACCENT);
        }
        let width = ui.available_width();
        let spacing = ui.spacing_mut();
        spacing.interact_size.y = 14.0;
        spacing.slider_width = width;
        ui.add(egui::Slider::new(value, range).show_value(false))
    })
    .inner
}
