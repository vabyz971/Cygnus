// Cygnus — Professional open-source creative suite
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

//! Compact labeled value for context bars ("Size [120 px v]").
//!
//! A subdued label followed by a recessed box holding the formatted
//! value and a chevron. Dragging the box horizontally scrubs the
//! value; a click opens a popup with a full-width
//! [`Slider`](super::Slider) for coarse moves.
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::components::ValueField;
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! let mut opacity = 0.35;
//! ValueField::new("Opacity", 0.0..=1.0).percent().show(ui, &theme, &mut opacity);
//! # });
//! ```

use super::Slider;
use super::menu::menu_style;
use crate::icons::{Icon, IconRegistry};
use crate::theme::CygnusTheme;
use std::ops::RangeInclusive;

/// Compact labeled numeric field via builder API.
#[derive(Debug, Clone)]
pub struct ValueField<'a> {
    label: &'a str,
    range: RangeInclusive<f32>,
    unit: &'a str,
    percent: bool,
    show_label: bool,
}

impl<'a> ValueField<'a> {
    /// Creates a field with label and range (value printed rounded).
    pub fn new(label: &'a str, range: RangeInclusive<f32>) -> Self {
        Self {
            label,
            range,
            unit: "",
            percent: false,
            show_label: true,
        }
    }

    /// Unit printed after the value ("px").
    #[must_use]
    pub fn unit(mut self, unit: &'a str) -> Self {
        self.unit = unit;
        self
    }

    /// `0..=1` value printed as a percentage.
    #[must_use]
    pub fn percent(mut self) -> Self {
        self.percent = true;
        self
    }

    /// Skips the inline label (callers place it themselves, e.g. in a
    /// right-to-left row); the popup slider keeps it.
    #[must_use]
    pub fn hide_label(mut self) -> Self {
        self.show_label = false;
        self
    }

    /// Text shown in the box.
    fn format(&self, value: f32) -> String {
        if self.percent {
            format!("{} %", (value * 100.0).round())
        } else if self.unit.is_empty() {
            format!("{}", value.round())
        } else {
            format!("{} {}", value.round(), self.unit)
        }
    }

    /// Displays the field, updates `value`, returns the box response
    /// (`changed()` when the value moved this frame).
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme, value: &mut f32) -> egui::Response {
        let (min, max) = (*self.range.start(), *self.range.end());
        if self.show_label {
            ui.label(
                egui::RichText::new(self.label)
                    .size(theme.typography.body_size)
                    .color(theme.colors.fg_secondary),
            );
        }
        let text = self.format(*value);
        let font = egui::FontId::proportional(theme.typography.body_size);
        let galley = ui
            .painter()
            .layout_no_wrap(text, font, theme.colors.fg_primary);
        let chevron = theme.typography.icon_size;
        let pad = theme.spacing.sm;
        let width = (galley.size().x + chevron + pad * 2.0 + theme.spacing.xs)
            .max(theme.sizes.preset_card * 0.7);
        let (rect, mut response) = ui.allocate_exact_size(
            egui::vec2(width, theme.sizes.input_height),
            egui::Sense::click_and_drag(),
        );
        let response_hover = response.hovered() || response.dragged();
        // Scrub: one range per ~200 points of travel.
        if response.dragged() && max > min {
            let delta = response.drag_delta().x;
            let next = (*value + delta * (max - min) / 200.0).clamp(min, max);
            if next != *value {
                *value = next;
                response.mark_changed();
            }
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let chevron_galley = egui::WidgetText::from(IconRegistry::new().colored(
            Icon::ExpandMore,
            chevron,
            theme.colors.fg_secondary,
        ))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let painter = ui.painter();
        painter.rect(
            rect,
            theme.radius.sm,
            theme.colors.bg_tertiary,
            egui::Stroke::new(
                theme.borders.thin,
                if response_hover {
                    theme.colors.fg_subdued
                } else {
                    theme.colors.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            egui::pos2(rect.left() + pad, rect.center().y - galley.size().y / 2.0),
            galley,
            theme.colors.fg_primary,
        );
        let chevron_size = chevron_galley.size();
        painter.galley(
            egui::pos2(
                rect.right() - pad * 0.5 - chevron_size.x,
                rect.center().y - chevron_size.y / 2.0,
            ),
            chevron_galley,
            theme.colors.fg_secondary,
        );
        let mut popup_changed = false;
        egui::Popup::menu(&response)
            .style(menu_style(theme))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(theme.sizes.preset_card * 2.0);
                let before = *value;
                Slider::new(self.label, self.range.clone())
                    .stacked()
                    .value(if self.percent {
                        super::SliderValue::Percent
                    } else {
                        super::SliderValue::Number
                    })
                    .show(ui, theme, value);
                popup_changed = *value != before;
            });
        if popup_changed {
            response.mark_changed();
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::setup_fonts;

    #[test]
    fn formats_units_and_percentages() {
        assert_eq!(ValueField::new("a", 0.0..=200.0).format(119.6), "120");
        assert_eq!(
            ValueField::new("a", 0.0..=200.0).unit("px").format(12.0),
            "12 px"
        );
        assert_eq!(
            ValueField::new("a", 0.0..=1.0).percent().format(0.35),
            "35 %"
        );
    }

    #[test]
    fn field_renders_idle_without_change() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        let mut value = 0.5;
        let mut changed = true;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.horizontal(|ui| {
                    changed = ValueField::new("Opacite", 0.0..=1.0)
                        .percent()
                        .show(ui, &theme, &mut value)
                        .changed();
                });
            });
        })
        .drop_without_applying_deltas();
        assert!(!changed);
        assert_eq!(value, 0.5);
    }
}
