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

//! Labeled numeric slider (opacity, size, zoom…).
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::components::Slider;
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut opacity = 1.0;
//! Slider::new("Opacite", 0.0..=1.0).show(ui, &theme, &mut opacity);
//! # });
//! ```

use crate::theme::CygnusTheme;
use std::ops::RangeInclusive;

/// How the slider prints its value next to the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliderValue {
    /// Value hidden (callers add their own field).
    #[default]
    Hidden,
    /// Raw number (decimals picked from the range span).
    Number,
    /// `0..=1` shown as a rounded percentage ("75 %").
    Percent,
}

/// `f32` slider via builder API.
///
/// Two layouts: inline (label then egui slider, the default) and
/// [`stacked`](Self::stacked) — label and value on one row, a thin
/// accent track underneath spanning the full width.
#[derive(Debug, Clone)]
pub struct Slider<'a> {
    label: &'a str,
    range: RangeInclusive<f32>,
    step: Option<f64>,
    stacked: bool,
    value: SliderValue,
}

impl<'a> Slider<'a> {
    /// Creates a slider with label and range.
    pub fn new(label: &'a str, range: RangeInclusive<f32>) -> Self {
        Self {
            label,
            range,
            step: None,
            stacked: false,
            value: SliderValue::Hidden,
        }
    }

    /// Discretization step (optional).
    #[must_use]
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Stacked layout: "label .... value" row above a full-width track.
    /// Shows the value as a number unless [`value`](Self::value) says otherwise.
    #[must_use]
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        if self.value == SliderValue::Hidden {
            self.value = SliderValue::Number;
        }
        self
    }

    /// Value display next to the label.
    #[must_use]
    pub fn value(mut self, value: SliderValue) -> Self {
        self.value = value;
        self
    }

    /// Displays the slider, updates `value`, returns the egui response.
    ///
    /// Inline layout: the raw egui value box is hidden
    /// (`show_value(false)`), callers that need precise entry add
    /// their own compact field (single value display, never duplicated).
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme, value: &mut f32) -> egui::Response {
        if self.stacked {
            return self.show_stacked(ui, theme, value);
        }
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(self.label)
                    .size(theme.typography.body_size)
                    .color(theme.colors.fg_secondary),
            );
            let mut slider = egui::Slider::new(value, self.range).show_value(false);
            if let Some(step) = self.step {
                slider = slider.step_by(step);
            }
            ui.add(slider)
        })
        .inner
    }

    /// Stacked layout (label/value row + painted track).
    fn show_stacked(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        value: &mut f32,
    ) -> egui::Response {
        let (min, max) = (*self.range.start(), *self.range.end());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing.xxs;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(self.label)
                        .size(theme.typography.body_size)
                        .color(theme.colors.fg_secondary),
                );
                if let Some(text) = format_value(self.value, *value, max - min) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(text)
                                .size(theme.typography.caption_size)
                                .color(theme.colors.fg_primary),
                        );
                    });
                }
            });
            let knob = theme.spacing.xs + theme.spacing.xxs;
            let size = egui::vec2(ui.available_width(), knob * 2.0 + theme.spacing.xxs);
            let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            let track = rect.shrink2(egui::vec2(knob, 0.0));
            if let Some(pointer) = response.interact_pointer_pos()
                && track.width() > 0.0
                && max > min
            {
                let t = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
                let mut next = min + t * (max - min);
                if let Some(step) = self.step
                    && step > 0.0
                {
                    let step = step as f32;
                    next = (min + ((next - min) / step).round() * step).clamp(min, max);
                }
                if next != *value {
                    *value = next;
                    response.mark_changed();
                }
            }
            let t = if max > min {
                ((*value - min) / (max - min)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let painter = ui.painter_at(rect.expand(knob));
            let y = rect.center().y;
            let rail = theme.borders.medium;
            let x = track.left() + t * track.width();
            painter.line_segment(
                [egui::pos2(track.left(), y), egui::pos2(track.right(), y)],
                egui::Stroke::new(rail, theme.colors.widget_bg),
            );
            painter.line_segment(
                [egui::pos2(track.left(), y), egui::pos2(x, y)],
                egui::Stroke::new(rail, theme.colors.accent),
            );
            let hot = response.hovered() || response.dragged();
            let fill = if hot {
                theme.colors.accent_hover
            } else {
                theme.colors.accent
            };
            painter.circle_filled(egui::pos2(x, y), knob * 0.8, fill);
            response
        })
        .inner
    }
}

/// Printed value for `display` (`None` = hidden).
fn format_value(display: SliderValue, value: f32, span: f32) -> Option<String> {
    match display {
        SliderValue::Hidden => None,
        SliderValue::Percent => Some(format!("{} %", (value * 100.0).round())),
        SliderValue::Number if span <= 10.0 => Some(format!("{value:.2}")),
        SliderValue::Number => Some(format!("{}", value.round())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut value = 0.5;
                let _ = Slider::new("Opacite", 0.0..=1.0)
                    .step(0.01)
                    .show(ui, &theme, &mut value);
                let _ = Slider::new("Densite", 0.0..=1.0)
                    .stacked()
                    .value(SliderValue::Percent)
                    .show(ui, &theme, &mut value);
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn values_format_per_display_mode() {
        assert_eq!(format_value(SliderValue::Hidden, 0.5, 1.0), None);
        assert_eq!(
            format_value(SliderValue::Percent, 0.75, 1.0).as_deref(),
            Some("75 %")
        );
        assert_eq!(
            format_value(SliderValue::Number, 1.5, 3.0).as_deref(),
            Some("1.50")
        );
        assert_eq!(
            format_value(SliderValue::Number, 42.4, 200.0).as_deref(),
            Some("42")
        );
    }
}
