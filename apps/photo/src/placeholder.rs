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

//! Placeholder content of the main screen: sample layers, curve, histogram,
//! history and tool settings from the mockup.
//!
//! None of this is connected to a document. The whole module is meant to be
//! deleted at milestone J3, when the document model, the commands and the
//! history exist.

use cygnus_ui_kit::i18n::{Key, tr, tr_format};
use cygnus_ui_kit::icons;
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::widgets;
use egui::epaint::CubicBezierShape;
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Ui, Vec2, pos2, vec2,
};

use crate::keys;

/// Blend modes of CAL-3, shown in the drop-downs.
pub const BLEND_MODES: [Key; 12] = [
    keys::BLEND_NORMAL,
    keys::BLEND_MULTIPLY,
    keys::BLEND_SCREEN,
    keys::BLEND_OVERLAY,
    keys::BLEND_SOFT_LIGHT,
    keys::BLEND_DARKEN,
    keys::BLEND_LIGHTEN,
    keys::BLEND_DIFFERENCE,
    keys::BLEND_HUE,
    keys::BLEND_SATURATION,
    keys::BLEND_COLOR,
    keys::BLEND_LUMINOSITY,
];

/// Placeholder document size, in pixels.
pub const DOCUMENT_SIZE: (u32, u32) = (6000, 3375);

/// Placeholder zoom factor, in percent.
pub const ZOOM_PERCENT: &str = "18,7";

/// Values the widgets of the placeholder content edit.
pub struct State {
    brush_size: u32,
    brush_hardness: u32,
    brush_opacity: u32,
    brush_flow: u32,
    brush_mode: usize,
    stabilizer: bool,
    strength: f32,
    mask_density: f32,
    layer_mode: usize,
    layer_opacity: u32,
    selected_layer: usize,
    visible: [bool; 4],
}

impl Default for State {
    fn default() -> Self {
        Self {
            brush_size: 120,
            brush_hardness: 0,
            brush_opacity: 35,
            brush_flow: 20,
            brush_mode: 0,
            stabilizer: false,
            strength: 75.0,
            mask_density: 100.0,
            layer_mode: 0,
            layer_opacity: 100,
            selected_layer: 0,
            visible: [true; 4],
        }
    }
}

fn px(value: u32) -> String {
    tr_format(keys::UNIT_PX, &[("value", &value.to_string())])
}

fn percent(value: impl ToString) -> String {
    tr_format(keys::UNIT_PERCENT, &[("value", &value.to_string())])
}

fn value_combo(
    ui: &mut Ui,
    id: &str,
    label: Key,
    value: &mut u32,
    choices: &[u32],
    unit: fn(u32) -> String,
) {
    widgets::labeled_combo(ui, id, tr(label), &unit(*value), 56.0, |ui| {
        for choice in choices {
            ui.selectable_value(value, *choice, unit(*choice));
        }
    });
}

fn blend_combo(ui: &mut Ui, id: &str, label: Option<Key>, mode: &mut usize, width: f32) {
    let label = label.map_or("", tr);
    widgets::labeled_combo(ui, id, label, tr(BLEND_MODES[*mode]), width, |ui| {
        for (index, key) in BLEND_MODES.iter().enumerate() {
            ui.selectable_value(mode, index, tr(*key));
        }
    });
}

/// Options bar of the Brush tool, with sample values.
pub fn brush_options(ui: &mut Ui, state: &mut State) {
    widgets::icon_label(ui, icons::PAINT_BRUSH, tr(keys::TOOL_BRUSH), palette::TEXT);
    ui.add_space(metrics::SPACING * 3.0);
    value_combo(
        ui,
        "brush-size",
        keys::OPTION_SIZE,
        &mut state.brush_size,
        &[20, 60, 120, 250, 500],
        px,
    );
    let percents = [0, 20, 35, 50, 75, 100];
    value_combo(
        ui,
        "brush-hardness",
        keys::OPTION_HARDNESS,
        &mut state.brush_hardness,
        &percents,
        percent,
    );
    value_combo(
        ui,
        "brush-opacity",
        keys::OPTION_OPACITY,
        &mut state.brush_opacity,
        &percents,
        percent,
    );
    value_combo(
        ui,
        "brush-flow",
        keys::OPTION_FLOW,
        &mut state.brush_flow,
        &percents,
        percent,
    );
    blend_combo(
        ui,
        "brush-mode",
        Some(keys::OPTION_MODE),
        &mut state.brush_mode,
        100.0,
    );
    ui.checkbox(&mut state.stabilizer, tr(keys::OPTION_STABILIZER));
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        widgets::icon_button(ui, icons::SLIDERS_HORIZONTAL, tr(keys::OPTION_MORE));
    });
}

/// Title of the placeholder document.
pub fn document_title() -> &'static str {
    tr(keys::PLACEHOLDER_DOCUMENT_TITLE)
}

/// Size and color format of the placeholder document, for the tab bar.
pub fn document_info(ui: &mut Ui) {
    widgets::icon_button(ui, icons::INFO, tr(keys::DOCUMENT_INFO));
    let mono = |text: &str| RichText::new(text).monospace().color(palette::TEXT_MUTED);
    ui.label(mono(tr(keys::PLACEHOLDER_DOCUMENT_FORMAT)));
    let (width, height) = DOCUMENT_SIZE;
    ui.label(mono(&tr_format(
        keys::DOCUMENT_DIMENSIONS,
        &[
            ("width", &width.to_string()),
            ("height", &height.to_string()),
        ],
    )));
}

/// Camera settings line under the canvas (AFF-3).
pub fn exif_line(ui: &mut Ui) {
    ui.label(
        RichText::new(tr(keys::PLACEHOLDER_EXIF))
            .monospace()
            .color(palette::TEXT_MUTED),
    );
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        // Right-to-left layout: the text is added before its icon.
        ui.label(RichText::new(tr(keys::PLACEHOLDER_MASK_TARGET)).color(palette::TEXT_MUTED));
        ui.label(widgets::icon(icons::PAINT_BRUSH, metrics::ICON - 4.0).color(palette::TEXT_MUTED));
    });
}

/// Action under way, shown at the left of the status bar.
pub fn status_action() -> &'static str {
    tr(keys::PLACEHOLDER_STATUS_ACTION)
}

fn caption(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(palette::TEXT_MUTED));
}

/// Sample histogram: three overlapping channels.
pub fn histogram(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 96.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(metrics::RADIUS), palette::INSET);

    const BINS: usize = 96;
    let bin_width = rect.width() / BINS as f32;
    for (channel, color) in palette::CHANNELS.iter().enumerate() {
        let shift = channel as f32 * 0.9;
        for bin in 0..BINS {
            let x = bin as f32 / BINS as f32;
            let level =
                (0.55 + 0.25 * (x * 9.0 + shift).sin() + 0.2 * (x * 23.0 + shift * 2.0).sin())
                    * (1.0 - (2.0 * x - 1.0).powi(6));
            let height = level.clamp(0.0, 1.0) * rect.height() * 0.85;
            let left = rect.left() + bin as f32 * bin_width;
            let bar = Rect::from_min_max(
                pos2(left, rect.bottom() - height),
                pos2(left + bin_width, rect.bottom()),
            );
            painter.rect_filled(bar, CornerRadius::ZERO, color.gamma_multiply(0.45));
        }
    }

    ui.horizontal(|ui| {
        caption(ui, "0");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            caption(ui, "255");
            ui.add_space((ui.available_width() - 60.0) / 2.0);
            caption(ui, tr(keys::HISTOGRAM_MIDTONES));
        });
    });
    ui.horizontal(|ui| {
        caption(ui, &tr_format(keys::HISTOGRAM_MEAN, &[("value", "112,8")]));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            caption(
                ui,
                &tr_format(keys::HISTOGRAM_STD_DEV, &[("value", "48,2")]),
            );
        });
    });
}

/// Sample Curves adjustment with its strength and mask density.
pub fn adjustments(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        widgets::icon_label(
            ui,
            icons::BEZIER_CURVE,
            tr(keys::ADJUSTMENT_CURVES),
            palette::TEXT,
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::labeled_combo(
                ui,
                "curve-channel",
                "",
                tr(keys::HISTOGRAM_RGB),
                48.0,
                |ui| {
                    let _ = ui.selectable_label(true, tr(keys::HISTOGRAM_RGB));
                },
            );
        });
    });

    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 120.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(metrics::RADIUS), palette::INSET);
    let grid = Stroke::new(1.0, palette::BORDER);
    for step in 1..4 {
        let t = step as f32 / 4.0;
        painter.vline(rect.left() + rect.width() * t, rect.y_range(), grid);
        painter.hline(rect.x_range(), rect.top() + rect.height() * t, grid);
    }
    painter.line_segment(
        [rect.left_bottom(), rect.right_top()],
        Stroke::new(1.0, palette::TEXT_DISABLED),
    );
    painter.text(
        rect.left_top() + vec2(8.0, 8.0),
        egui::Align2::LEFT_TOP,
        tr(keys::ADJUSTMENT_AXES),
        egui::FontId::monospace(9.5),
        palette::TEXT_MUTED,
    );
    let at = |x: f32, y: f32| -> Pos2 {
        pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let points = [at(0.0, 0.0), at(0.45, 0.0), at(0.4, 0.75), at(1.0, 0.95)];
    painter.add(Shape::CubicBezier(CubicBezierShape::from_points_stroke(
        points,
        false,
        Color32::TRANSPARENT,
        Stroke::new(1.5, palette::ACCENT),
    )));
    for handle in [at(0.28, 0.13), at(0.65, 0.78)] {
        painter.circle(
            handle,
            3.5,
            palette::INSET,
            Stroke::new(1.5, palette::ACCENT_TEXT),
        );
    }

    ui.add_space(metrics::SPACING);
    labeled_slider(ui, keys::ADJUSTMENT_STRENGTH, &mut state.strength);
    labeled_slider(ui, keys::ADJUSTMENT_MASK_DENSITY, &mut state.mask_density);
}

fn labeled_slider(ui: &mut Ui, label: Key, value: &mut f32) {
    ui.horizontal(|ui| {
        caption(ui, tr(label));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(percent(value.round() as u32))
                    .monospace()
                    .color(palette::TEXT),
            );
        });
    });
    widgets::slider(ui, value, 0.0..=100.0);
}

struct SampleLayer {
    name: Key,
    kind: Key,
    icon: &'static str,
    trailing: &'static str,
    trailing_hint: Key,
}

const LAYERS: [SampleLayer; 4] = [
    SampleLayer {
        name: keys::PLACEHOLDER_LAYER_SKY,
        kind: keys::PLACEHOLDER_LAYER_SKY_KIND,
        icon: icons::BEZIER_CURVE,
        trailing: icons::SQUARE_HALF_BOTTOM,
        trailing_hint: keys::LAYERS_MASK,
    },
    SampleLayer {
        name: keys::PLACEHOLDER_LAYER_CURVES,
        kind: keys::PLACEHOLDER_LAYER_CURVES_KIND,
        icon: icons::BEZIER_CURVE,
        trailing: icons::SLIDERS_HORIZONTAL,
        trailing_hint: keys::LAYERS_SETTINGS,
    },
    SampleLayer {
        name: keys::PLACEHOLDER_LAYER_BALANCE,
        kind: keys::PLACEHOLDER_LAYER_BALANCE_KIND,
        icon: icons::SLIDERS,
        trailing: icons::SLIDERS_HORIZONTAL,
        trailing_hint: keys::LAYERS_SETTINGS,
    },
    SampleLayer {
        name: keys::PLACEHOLDER_LAYER_SOURCE,
        kind: keys::PLACEHOLDER_LAYER_SOURCE_KIND,
        icon: icons::IMAGE,
        trailing: icons::LOCK_SIMPLE,
        trailing_hint: keys::LAYERS_LOCKED,
    },
];

/// Sample layer list of the mockup.
pub fn layers(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        blend_combo(ui, "layer-mode", None, &mut state.layer_mode, 110.0);
        let percents = [0, 25, 50, 75, 100];
        value_combo(
            ui,
            "layer-opacity",
            keys::LAYERS_OPACITY,
            &mut state.layer_opacity,
            &percents,
            percent,
        );
    });
    ui.add_space(4.0);

    for (index, layer) in LAYERS.iter().enumerate() {
        let selected = index == state.selected_layer;
        let row = Frame::new()
            .fill(if selected {
                palette::SELECTED
            } else {
                Color32::TRANSPARENT
            })
            .corner_radius(CornerRadius::same(metrics::RADIUS))
            .inner_margin(Margin::symmetric(6, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let eye = if state.visible[index] {
                        icons::EYE
                    } else {
                        icons::EYE_SLASH
                    };
                    if widgets::icon_button(ui, eye, tr(keys::LAYERS_VISIBILITY)).clicked() {
                        state.visible[index] = !state.visible[index];
                    }
                    Frame::new()
                        .stroke(Stroke::new(1.0, palette::BORDER))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(5))
                        .show(ui, |ui| {
                            ui.label(
                                widgets::icon(layer.icon, metrics::ICON - 2.0).color(palette::TEXT),
                            );
                        });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(tr(layer.name)).color(palette::TEXT));
                        caption(ui, tr(layer.kind));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::icon_button(ui, layer.trailing, tr(layer.trailing_hint));
                    });
                });
            });
        if row.response.interact(Sense::click()).clicked() {
            state.selected_layer = index;
        }
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = metrics::SPACING * 1.5;
            for (icon, hint) in [
                (icons::PLUS, keys::LAYERS_ADD),
                (icons::FOLDER_SIMPLE_PLUS, keys::LAYERS_ADD_GROUP),
                (icons::CIRCLE_HALF, keys::LAYERS_ADD_ADJUSTMENT),
                (icons::SELECTION, keys::LAYERS_ADD_MASK),
                (icons::SPARKLE, keys::LAYERS_ADD_FILTER),
            ] {
                widgets::icon_button(ui, icon, tr(hint));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::icon_button(ui, icons::TRASH, tr(keys::LAYERS_DELETE));
            });
        });
    });
}

/// Sample history with a single entry.
pub fn history(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(
            widgets::icon(icons::CLOCK_COUNTER_CLOCKWISE, metrics::ICON).color(palette::TEXT_MUTED),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            caption(ui, tr(keys::PANEL_HISTORY));
            ui.label(RichText::new(tr(keys::PLACEHOLDER_HISTORY_ENTRY)).color(palette::TEXT));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(tr(keys::PLACEHOLDER_HISTORY_TIME))
                    .monospace()
                    .color(palette::TEXT_MUTED),
            );
        });
    });
}

/// Foreground and background color swatches at the bottom of the tool rail.
pub fn color_swatches(ui: &mut Ui) {
    widgets::icon_button(ui, icons::ARROWS_LEFT_RIGHT, tr(keys::TOOL_SWAP_COLORS));
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let painter = ui.painter();
    let back = Rect::from_min_size(rect.min + vec2(12.0, 12.0), Vec2::splat(20.0));
    let front = Rect::from_min_size(rect.min + vec2(2.0, 2.0), Vec2::splat(20.0));
    painter.rect(
        back,
        CornerRadius::same(3),
        palette::CANVAS,
        Stroke::new(1.5, palette::TEXT),
        egui::StrokeKind::Inside,
    );
    painter.rect(
        front,
        CornerRadius::same(3),
        Color32::WHITE,
        Stroke::new(1.0, palette::BORDER),
        egui::StrokeKind::Inside,
    );
    response.on_hover_text(tr(keys::TOOL_COLORS));
}
