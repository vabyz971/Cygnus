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

//! Tool settings of the options bar and the color swatches of the tool
//! rail (docs/decisions/0025). The tools paint from J5: until then their
//! settings are real state with default values, without effect.

use cygnus_photo_core::BlendMode;
use cygnus_ui_kit::i18n::{Key, tr, tr_format};
use cygnus_ui_kit::icons;
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::widgets;
use egui::{Align, Color32, CornerRadius, Layout, Rect, Sense, Stroke, Ui, Vec2, vec2};

use crate::keys;

/// Translation key of a blend mode (CAL-3).
pub fn blend_key(mode: BlendMode) -> Key {
    match mode {
        BlendMode::Normal => keys::BLEND_NORMAL,
        BlendMode::Multiply => keys::BLEND_MULTIPLY,
        BlendMode::Screen => keys::BLEND_SCREEN,
        BlendMode::Overlay => keys::BLEND_OVERLAY,
        BlendMode::SoftLight => keys::BLEND_SOFT_LIGHT,
        BlendMode::Darken => keys::BLEND_DARKEN,
        BlendMode::Lighten => keys::BLEND_LIGHTEN,
        BlendMode::Difference => keys::BLEND_DIFFERENCE,
        BlendMode::Hue => keys::BLEND_HUE,
        BlendMode::Saturation => keys::BLEND_SATURATION,
        BlendMode::Color => keys::BLEND_COLOR,
        BlendMode::Luminosity => keys::BLEND_LUMINOSITY,
    }
}

/// Drop-down of the twelve blend modes; returns the mode picked, if any.
pub fn blend_combo(
    ui: &mut Ui,
    id: &str,
    label: Option<Key>,
    mode: BlendMode,
    width: f32,
) -> Option<BlendMode> {
    let mut picked = None;
    widgets::labeled_combo(
        ui,
        id,
        label.map_or("", tr),
        tr(blend_key(mode)),
        width,
        |ui| {
            for candidate in BlendMode::ALL {
                if ui
                    .selectable_label(candidate == mode, tr(blend_key(candidate)))
                    .clicked()
                {
                    picked = Some(candidate);
                }
            }
        },
    );
    picked
}

/// `{value} px`.
pub fn px(value: u32) -> String {
    tr_format(keys::UNIT_PX, &[("value", &value.to_string())])
}

/// `{value} %`.
pub fn percent(value: impl ToString) -> String {
    tr_format(keys::UNIT_PERCENT, &[("value", &value.to_string())])
}

/// Settings of the painting tools (used from J5).
pub struct ToolSettings {
    brush_size: u32,
    brush_hardness: u32,
    brush_opacity: u32,
    brush_flow: u32,
    brush_mode: BlendMode,
    stabilizer: bool,
    /// Foreground and background colors, straight sRGB.
    pub colors: [Color32; 2],
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            brush_size: 60,
            brush_hardness: 100,
            brush_opacity: 100,
            brush_flow: 100,
            brush_mode: BlendMode::Normal,
            stabilizer: false,
            colors: [Color32::BLACK, Color32::WHITE],
        }
    }
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

/// Options bar of the brush.
pub fn brush_options(ui: &mut Ui, settings: &mut ToolSettings) {
    widgets::icon_label(ui, icons::PAINT_BRUSH, tr(keys::TOOL_BRUSH), palette::TEXT);
    ui.add_space(metrics::SPACING * 3.0);
    value_combo(
        ui,
        "brush-size",
        keys::OPTION_SIZE,
        &mut settings.brush_size,
        &[5, 20, 60, 120, 250, 500],
        px,
    );
    let percents = [0, 20, 35, 50, 75, 100];
    for (id, label, value) in [
        (
            "brush-hardness",
            keys::OPTION_HARDNESS,
            &mut settings.brush_hardness,
        ),
        (
            "brush-opacity",
            keys::OPTION_OPACITY,
            &mut settings.brush_opacity,
        ),
        ("brush-flow", keys::OPTION_FLOW, &mut settings.brush_flow),
    ] {
        value_combo(ui, id, label, value, &percents, percent);
    }
    if let Some(mode) = blend_combo(
        ui,
        "brush-mode",
        Some(keys::OPTION_MODE),
        settings.brush_mode,
        100.0,
    ) {
        settings.brush_mode = mode;
    }
    ui.checkbox(&mut settings.stabilizer, tr(keys::OPTION_STABILIZER));
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        widgets::icon_button(ui, icons::SLIDERS_HORIZONTAL, tr(keys::OPTION_MORE));
    });
}

/// Foreground and background swatches, with the swap button.
pub fn color_swatches(ui: &mut Ui, settings: &mut ToolSettings) {
    if widgets::icon_button(ui, icons::ARROWS_LEFT_RIGHT, tr(keys::TOOL_SWAP_COLORS)).clicked() {
        settings.colors.swap(0, 1);
    }
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let painter = ui.painter();
    let back = Rect::from_min_size(rect.min + vec2(12.0, 12.0), Vec2::splat(20.0));
    let front = Rect::from_min_size(rect.min + vec2(2.0, 2.0), Vec2::splat(20.0));
    for (area, color, width) in [
        (back, settings.colors[1], 1.5),
        (front, settings.colors[0], 1.0),
    ] {
        painter.rect(
            area,
            CornerRadius::same(3),
            color,
            Stroke::new(width, palette::BORDER),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(tr(keys::TOOL_COLORS));
}
