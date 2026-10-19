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

//! Resizable two-pane area (draggable separator).
//!
//! The ratio persists in `egui::Memory` via the provided `id`:
//! it survives frames with no app-side state. For on-disk
//! persistence, see [`crate::layout`].
//!
//! # Example
//! ```rust,no_run
//! # use cygnus_ui::containers::Split;
//! # use cygnus_ui::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! Split::horizontal("studio_split").show(
//!     ui,
//!     &theme,
//!     |ui| {
//!         ui.label("proprietes");
//!     },
//!     |ui| {
//!         ui.label("calques");
//!     },
//! );
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Minimum / maximum ratio of a pane (5% .. 95%).
const MIN_RATIO: f32 = 0.05;
/// Minimum pane width in points.
const MIN_PANE: f32 = 60.0;

/// Clamps a ratio to the valid range.
fn clamp_ratio(ratio: f32) -> f32 {
    ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO)
}

/// Width of the first pane for `total` available points.
fn pane_size(total: f32, separator: f32, min: f32, ratio: f32) -> f32 {
    if total <= 0.0 {
        return 0.0;
    }
    let max_first = (total - separator - min).max(0.0);
    (total * clamp_ratio(ratio)).clamp(min.min(max_first), max_first)
}

/// Two-pane area via builder API.
#[derive(Debug, Clone)]
pub struct Split {
    id: egui::Id,
    ratio: f32,
    min: f32,
    vertical: bool,
}

impl Split {
    /// Left / right panes (vertical separator).
    pub fn horizontal(id: impl Into<egui::Id>) -> Self {
        Self {
            id: id.into(),
            ratio: 0.5,
            min: MIN_PANE,
            vertical: false,
        }
    }

    /// Top / bottom panes (horizontal separator).
    pub fn vertical(id: impl Into<egui::Id>) -> Self {
        Self {
            id: id.into(),
            ratio: 0.5,
            min: MIN_PANE,
            vertical: true,
        }
    }

    /// Initial ratio of the first pane (0.05 .. 0.95).
    #[must_use]
    pub fn initial_ratio(mut self, ratio: f32) -> Self {
        self.ratio = clamp_ratio(ratio);
        self
    }

    /// Minimum size of a pane in points.
    #[must_use]
    pub fn min_size(mut self, min: f32) -> Self {
        self.min = min.max(0.0);
        self
    }

    /// Displays both panes and their separator.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        first: impl FnOnce(&mut egui::Ui),
        second: impl FnOnce(&mut egui::Ui),
    ) {
        let mut ratio = self.ratio;
        ui.data_mut(|data| {
            ratio = *data.get_temp_mut_or(self.id, ratio);
        });
        ratio = clamp_ratio(ratio);
        if self.vertical {
            self.show_vertical(ui, theme, ratio, first, second);
        } else {
            self.show_horizontal(ui, theme, ratio, first, second);
        }
    }

    /// Remembers the ratio in `egui::Memory`.
    fn store_ratio(ui: &mut egui::Ui, id: egui::Id, ratio: f32) {
        ui.data_mut(|data| {
            data.insert_temp(id, clamp_ratio(ratio));
        });
    }

    fn show_horizontal(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        ratio: f32,
        first: impl FnOnce(&mut egui::Ui),
        second: impl FnOnce(&mut egui::Ui),
    ) {
        let avail = ui.available_size();
        let separator = theme.spacing.xs.max(2.0);
        let first_w = pane_size(avail.x, separator, self.min, ratio);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.allocate_ui_with_layout(
                egui::vec2(first_w, avail.y),
                egui::Layout::top_down(egui::Align::Min),
                first,
            );
            let (rect, response) = ui
                .allocate_exact_size(egui::vec2(separator, avail.y.max(0.0)), egui::Sense::drag());
            let active = response.dragged() || response.hovered();
            ui.painter().line_segment(
                [rect.center_top(), rect.center_bottom()],
                egui::Stroke::new(
                    theme.borders.thin,
                    if active {
                        theme.colors.drop_indicator
                    } else {
                        theme.colors.border
                    },
                ),
            );
            let cursor = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if cursor.dragged() && avail.x > 0.0 {
                Self::store_ratio(ui, self.id, ratio + cursor.drag_delta().x / avail.x);
            }
            let rest = (avail.x - first_w - separator).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(rest, avail.y),
                egui::Layout::top_down(egui::Align::Min),
                second,
            );
        });
    }

    fn show_vertical(
        self,
        ui: &mut egui::Ui,
        theme: &CygnusTheme,
        ratio: f32,
        first: impl FnOnce(&mut egui::Ui),
        second: impl FnOnce(&mut egui::Ui),
    ) {
        let avail = ui.available_size();
        let separator = theme.spacing.xs.max(2.0);
        let first_h = pane_size(avail.y, separator, self.min, ratio);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.allocate_ui_with_layout(
                egui::vec2(avail.x, first_h),
                egui::Layout::top_down(egui::Align::Min),
                first,
            );
            let (rect, response) = ui
                .allocate_exact_size(egui::vec2(avail.x.max(0.0), separator), egui::Sense::drag());
            let active = response.dragged() || response.hovered();
            ui.painter().line_segment(
                [rect.left_center(), rect.right_center()],
                egui::Stroke::new(
                    theme.borders.thin,
                    if active {
                        theme.colors.drop_indicator
                    } else {
                        theme.colors.border
                    },
                ),
            );
            let cursor = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
            if cursor.dragged() && avail.y > 0.0 {
                Self::store_ratio(ui, self.id, ratio + cursor.drag_delta().y / avail.y);
            }
            let rest = (avail.y - first_h - separator).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(avail.x, rest),
                egui::Layout::top_down(egui::Align::Min),
                second,
            );
        });
    }
}

/// Pure state and logic of a split panel (no egui).
///
/// `fraction` is the share (0..=1) of the total space allocated to
/// the first area. Minimum sizes guarantee no area collapses below
/// its threshold. Ported from the old `panels::CygnusSplitState`:
/// same logic, same behavior.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitState {
    fraction: f32,
    min_first: f32,
    min_second: f32,
}

impl SplitState {
    /// Creates a split state. `fraction` is clamped to 0..=1.
    pub fn new(fraction: f32, min_first: f32, min_second: f32) -> Self {
        let mut state = Self {
            fraction: 0.5,
            min_first: min_first.max(0.0),
            min_second: min_second.max(0.0),
        };
        state.set_fraction(fraction);
        state
    }

    /// Updates the fraction (clamped to 0..=1).
    pub fn set_fraction(&mut self, fraction: f32) {
        self.fraction = fraction.clamp(0.0, 1.0);
    }

    /// Current fraction (0..=1).
    pub fn fraction(self) -> f32 {
        self.fraction
    }

    /// Minimum size of the first area.
    pub fn min_first(self) -> f32 {
        self.min_first
    }

    /// Minimum size of the second area.
    pub fn min_second(self) -> f32 {
        self.min_second
    }

    /// Clamps a requested size for the first area against both
    /// minimums. Never panics, even when `total` is smaller than the
    /// minimums' sum (degenerate case: the first area takes all
    /// available space).
    pub fn clamp_first_size(total: f32, requested: f32, min_first: f32, min_second: f32) -> f32 {
        let total = total.max(0.0);
        let max_first = (total - min_second.max(0.0)).max(0.0);
        let min_first = min_first.max(0.0).min(total);
        if min_first >= max_first {
            return min_first.min(total);
        }
        requested.clamp(min_first, max_first)
    }

    /// Size of the first area for a given total space.
    pub fn first_size(self, total: f32) -> f32 {
        Self::clamp_first_size(
            total,
            self.fraction * total,
            self.min_first,
            self.min_second,
        )
    }

    /// Size of the second area for a given total space.
    pub fn second_size(self, total: f32) -> f32 {
        (total.max(0.0) - self.first_size(total)).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_is_clamped() {
        assert_eq!(clamp_ratio(-1.0), MIN_RATIO);
        assert_eq!(clamp_ratio(2.0), 1.0 - MIN_RATIO);
        assert_eq!(clamp_ratio(0.3), 0.3);
    }

    #[test]
    fn pane_size_respects_min_and_total() {
        assert_eq!(pane_size(0.0, 4.0, 60.0, 0.5), 0.0);
        assert_eq!(pane_size(1000.0, 4.0, 60.0, 0.5), 500.0);
        // Total too small: the first pane shrinks to what's available.
        assert!(pane_size(80.0, 4.0, 60.0, 0.5) <= 80.0);
    }

    #[test]
    fn split_respects_min_sizes() {
        // Below-minimum request -> minimum applied.
        assert_eq!(
            SplitState::clamp_first_size(1000.0, 10.0, 160.0, 200.0),
            160.0
        );
        // Request crushing the second area -> second area preserved.
        assert_eq!(
            SplitState::clamp_first_size(1000.0, 900.0, 160.0, 200.0),
            800.0
        );
        // Valid request -> unchanged.
        assert_eq!(
            SplitState::clamp_first_size(1000.0, 300.0, 160.0, 200.0),
            300.0
        );
    }

    #[test]
    fn split_degenerate_total_never_panics() {
        // Total space below the minimums' sum: no panic, sizes kept
        // within [0, total].
        for total in [0.0, 50.0, 200.0, 359.0] {
            let state = SplitState::new(0.5, 160.0, 200.0);
            let first = state.first_size(total);
            let second = state.second_size(total);
            assert!(
                (0.0..=total).contains(&first),
                "first={first} total={total}"
            );
            assert!(
                (0.0..=total).contains(&second),
                "second={second} total={total}"
            );
        }
    }

    #[test]
    fn split_fraction_is_clamped() {
        let mut state = SplitState::new(2.0, 100.0, 100.0);
        assert_eq!(state.fraction(), 1.0);
        state.set_fraction(-1.0);
        assert_eq!(state.fraction(), 0.0);
    }

    #[test]
    fn split_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                Split::horizontal("test_h").show(
                    ui,
                    &theme,
                    |ui| {
                        ui.label("gauche");
                    },
                    |ui| {
                        ui.label("droite");
                    },
                );
                Split::vertical("test_v").show(
                    ui,
                    &theme,
                    |ui| {
                        ui.label("haut");
                    },
                    |ui| {
                        ui.label("bas");
                    },
                );
            });
        })
        .drop_without_applying_deltas();
    }
}
