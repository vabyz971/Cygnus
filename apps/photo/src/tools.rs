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
use cygnus_photo_core::brush::{Brush, MAX_DIAMETER, MIN_DIAMETER};
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

impl ToolSettings {
    /// The brush of these settings, painting the foreground color.
    pub fn brush(&self) -> Brush {
        let [r, g, b, _] = self.colors[0].to_srgba_unmultiplied();
        let unit = |v: u8| f32::from(v) / 255.0;
        Brush {
            diameter: self.brush_size as f32,
            hardness: self.brush_hardness as f32 / 100.0,
            flow: self.brush_flow as f32 / 100.0,
            opacity: self.brush_opacity as f32 / 100.0,
            mode: self.brush_mode,
            ..Brush::new([unit(r), unit(g), unit(b), 1.0])
        }
    }

    /// Diameter of the brush, in document pixels.
    pub fn diameter(&self) -> f32 {
        self.brush_size as f32
    }

    /// Whether the stabilizer smooths the strokes.
    pub fn stabilizer(&self) -> bool {
        self.stabilizer
    }

    /// `[` and `]` (PIN-5): the diameter by steps of 10 %, at least one
    /// pixel.
    pub fn step_size(&mut self, larger: bool) {
        let size = self.brush_size as f32;
        let next = if larger {
            (size * 1.1).round().max(size + 1.0)
        } else {
            (size / 1.1).round().min(size - 1.0)
        };
        self.brush_size = next.clamp(MIN_DIAMETER, MAX_DIAMETER) as u32;
    }

    /// Shift + `[` and `]`: the hardness by steps of 10 %.
    pub fn step_hardness(&mut self, harder: bool) {
        self.brush_hardness = if harder {
            (self.brush_hardness + 10).min(100)
        } else {
            self.brush_hardness.saturating_sub(10)
        };
    }

    /// Sets the foreground color from a straight 16-bit sample (the
    /// eyedropper), opaque.
    pub fn set_foreground(&mut self, rgba: [u16; 4]) {
        let narrow = |v: u16| ((u32::from(v) * 255 + 32767) / 65535) as u8;
        self.colors[0] = Color32::from_rgb(narrow(rgba[0]), narrow(rgba[1]), narrow(rgba[2]));
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

#[cfg(test)]
mod brush_settings_tests {
    use super::*;

    #[test]
    fn the_brush_follows_the_settings_and_the_foreground_color() {
        let mut settings = ToolSettings::default();
        settings.colors[0] = Color32::from_rgb(255, 0, 51);
        settings.brush_hardness = 35;
        let brush = settings.brush();
        assert_eq!(brush.diameter, 60.0);
        assert!((brush.hardness - 0.35).abs() < 1e-6);
        assert_eq!(brush.color, [1.0, 0.0, 0.2, 1.0]);
        assert!(brush.validate().is_ok());
    }

    #[test]
    fn brackets_step_the_size_by_ten_percent_within_bounds() {
        let mut settings = ToolSettings::default();
        settings.step_size(true);
        assert_eq!(settings.diameter(), 66.0);
        settings.step_size(false);
        assert_eq!(settings.diameter(), 60.0);
        settings.brush_size = 1;
        settings.step_size(false);
        assert_eq!(settings.diameter(), 1.0);
        settings.brush_size = 3;
        settings.step_size(true);
        assert_eq!(settings.diameter(), 4.0, "at least one pixel per step");
        settings.brush_size = 5000;
        settings.step_size(true);
        assert_eq!(settings.diameter(), 5000.0);
        settings.step_hardness(true);
        assert_eq!(settings.brush_hardness, 100);
        settings.step_hardness(false);
        assert_eq!(settings.brush_hardness, 90);
    }

    #[test]
    fn the_eyedropper_sets_an_opaque_foreground() {
        let mut settings = ToolSettings::default();
        settings.set_foreground([65535, 32896, 0, 1000]);
        assert_eq!(settings.colors[0], Color32::from_rgb(255, 128, 0));
    }
}
