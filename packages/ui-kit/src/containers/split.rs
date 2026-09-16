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

//! Zone redimensionnable à deux volets (séparateur draggable).
//!
//! Le ratio est persisté dans le `egui::Memory` via l'`id` fourni :
//! il survit aux frames sans état côté app. Pour la persistence
//! disque, voir [`crate::layout`].
//!
//! # Exemple
//! ```rust,no_run
//! # use ui_kit::containers::Split;
//! # use ui_kit::theme::CygnusTheme;
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

/// Ratio minimal / maximal d'un volet (5% .. 95%).
const MIN_RATIO: f32 = 0.05;
/// Largeur minimale d'un volet en points.
const MIN_PANE: f32 = 60.0;

/// Borne un ratio à la plage valide.
fn clamp_ratio(ratio: f32) -> f32 {
    ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO)
}

/// Largeur du premier volet pour `total` points disponibles.
fn pane_size(total: f32, separator: f32, min: f32, ratio: f32) -> f32 {
    if total <= 0.0 {
        return 0.0;
    }
    let max_first = (total - separator - min).max(0.0);
    (total * clamp_ratio(ratio)).clamp(min.min(max_first), max_first)
}

/// Zone à deux volets via API builder.
#[derive(Debug, Clone)]
pub struct Split {
    id: egui::Id,
    ratio: f32,
    min: f32,
    vertical: bool,
}

impl Split {
    /// Deux volets gauche / droite (séparateur vertical).
    pub fn horizontal(id: impl Into<egui::Id>) -> Self {
        Self {
            id: id.into(),
            ratio: 0.5,
            min: MIN_PANE,
            vertical: false,
        }
    }

    /// Deux volets haut / bas (séparateur horizontal).
    pub fn vertical(id: impl Into<egui::Id>) -> Self {
        Self {
            id: id.into(),
            ratio: 0.5,
            min: MIN_PANE,
            vertical: true,
        }
    }

    /// Ratio initial du premier volet (0.05 .. 0.95).
    #[must_use]
    pub fn initial_ratio(mut self, ratio: f32) -> Self {
        self.ratio = clamp_ratio(ratio);
        self
    }

    /// Taille minimale d'un volet en points.
    #[must_use]
    pub fn min_size(mut self, min: f32) -> Self {
        self.min = min.max(0.0);
        self
    }

    /// Affiche les deux volets et leur séparateur.
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

    /// Mémorise le ratio dans le `egui::Memory`.
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
        // Total trop petit : le premier volet est réduit au disponible.
        assert!(pane_size(80.0, 4.0, 60.0, 0.5) <= 80.0);
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
