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

//! Interactive tone-curve editor (Curves filter, O005).
//!
//! Square plot on the 8-bit scale: click on empty space adds a point,
//! dragging moves it (x bounded by its neighbors, so the order never
//! flips), double-click removes an interior point. The drawn curve is
//! the engine's own interpolation (`photo_engine::nodes::curves`), so
//! what the user sees is what the filter applies. Returns the edited
//! point list as the filter's text value; the worker coalesces a drag
//! into one history step.

use cygnus_ui::theme::CygnusTheme;
use photo_engine::nodes::curves;

/// Minimum x gap kept between neighbors while dragging (8-bit units).
const MIN_GAP: f32 = 1.0;

/// Index of the point under `pos` (8-bit space) within `radius`.
fn hit_point(points: &[[f32; 2]], pos: [f32; 2], radius: f32) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| (i, (p[0] - pos[0]).hypot(p[1] - pos[1])))
        .filter(|(_, d)| *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Inserts a point keeping the x order; returns its index (`None` when
/// the list is full or a point already sits on that x).
fn insert_point(points: &mut Vec<[f32; 2]>, pos: [f32; 2]) -> Option<usize> {
    if points.len() >= curves::MAX_POINTS {
        return None;
    }
    let at = points.partition_point(|p| p[0] < pos[0]);
    let too_close = |i: usize| {
        points
            .get(i)
            .is_some_and(|p| (p[0] - pos[0]).abs() < MIN_GAP)
    };
    if too_close(at) || (at > 0 && too_close(at - 1)) {
        return None;
    }
    points.insert(at, [pos[0].clamp(0.0, 255.0), pos[1].clamp(0.0, 255.0)]);
    Some(at)
}

/// Moves point `index`, x clamped strictly between its neighbors.
fn move_point(points: &mut [[f32; 2]], index: usize, pos: [f32; 2]) {
    let Some(len) = points.len().checked_sub(1) else {
        return;
    };
    let lo = if index == 0 {
        0.0
    } else {
        points[index - 1][0] + MIN_GAP
    };
    let hi = if index >= len {
        255.0
    } else {
        points[index + 1][0] - MIN_GAP
    };
    if let Some(p) = points.get_mut(index) {
        p[0] = pos[0].clamp(lo.min(hi), hi.max(lo));
        p[1] = pos[1].clamp(0.0, 255.0);
    }
}

/// Removes an interior point (end points stay: a curve needs two).
fn remove_point(points: &mut Vec<[f32; 2]>, index: usize) -> bool {
    if index == 0 || index + 1 >= points.len() {
        return false;
    }
    points.remove(index);
    true
}

/// Draws the editor for `text` (the filter's points value) and returns
/// the new value when the user edited the curve.
pub fn curve_editor(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    salt: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
) -> Option<String> {
    // Wide plot (about 2:1) so the curve fits the side column without
    // pushing the sliders below the fold; the mapping is per axis.
    let width = ui
        .available_width()
        .clamp(theme.sizes.preset_card, 4.0 * theme.sizes.preset_card);
    let height = (width * 0.48).max(theme.sizes.preset_card);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    let drag_id = ui.id().with(("curve_drag", salt));
    let to_screen = |p: [f32; 2]| {
        egui::pos2(
            rect.left() + p[0] / 255.0 * rect.width(),
            rect.bottom() - p[1] / 255.0 * rect.height(),
        )
    };
    let from_screen = |pos: egui::Pos2| {
        [
            ((pos.x - rect.left()) / rect.width() * 255.0).clamp(0.0, 255.0),
            ((rect.bottom() - pos.y) / rect.height() * 255.0).clamp(0.0, 255.0),
        ]
    };
    let hit_radius = theme.spacing.sm / rect.width().max(1.0) * 255.0;

    let mut points = curves::parse_points(text);
    let mut changed = false;
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    if response.double_clicked()
        && let Some(pos) = pointer
        && let Some(i) = hit_point(&points, from_screen(pos), hit_radius)
    {
        changed |= remove_point(&mut points, i);
    } else if response.clicked()
        && let Some(pos) = pointer
        && hit_point(&points, from_screen(pos), hit_radius).is_none()
    {
        changed |= insert_point(&mut points, from_screen(pos)).is_some();
    }
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        let at = from_screen(origin);
        let index = hit_point(&points, at, hit_radius).or_else(|| {
            let inserted = insert_point(&mut points, at);
            changed |= inserted.is_some();
            inserted
        });
        ui.data_mut(|d| match index {
            Some(i) => {
                d.insert_temp(drag_id, i);
            }
            None => d.remove::<usize>(drag_id),
        });
    }
    let dragging = ui.data(|d| d.get_temp::<usize>(drag_id));
    if response.dragged()
        && let (Some(i), Some(pos)) = (dragging, response.interact_pointer_pos())
    {
        let before = points.get(i).copied();
        move_point(&mut points, i, from_screen(pos));
        changed |= before != points.get(i).copied();
    }
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<usize>(drag_id));
    }

    // Plot: background, quarter grid, identity diagonal, curve, points.
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme.radius.sm, theme.colors.bg_tertiary);
    let grid = egui::Stroke::new(theme.borders.thin, theme.colors.border);
    for q in 1..4 {
        let t = q as f32 / 4.0;
        let x = rect.left() + t * rect.width();
        let y = rect.top() + t * rect.height();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            grid,
        );
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            grid,
        );
    }
    painter.line_segment([rect.left_bottom(), rect.right_top()], grid);
    let curve = curves::evaluate(&points);
    let line: Vec<egui::Pos2> = curve
        .iter()
        .enumerate()
        .map(|(x, y)| to_screen([x as f32, *y]))
        .collect();
    painter.add(egui::Shape::line(
        line,
        egui::Stroke::new(theme.borders.medium, theme.colors.accent),
    ));
    let hovered = pointer.and_then(|pos| hit_point(&points, from_screen(pos), hit_radius));
    for (i, p) in points.iter().enumerate() {
        let active = dragging == Some(i) || hovered == Some(i);
        painter.circle(
            to_screen(*p),
            theme.spacing.xs,
            if active {
                theme.colors.accent_hover
            } else {
                theme.colors.fg_primary
            },
            egui::Stroke::new(theme.borders.thin, theme.colors.bg_primary),
        );
    }
    painter.rect_stroke(rect, theme.radius.sm, grid, egui::StrokeKind::Inside);

    changed.then(|| curves::format_points(&points))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_keeps_order_and_rejects_duplicates() {
        let mut points = vec![[0.0, 0.0], [255.0, 255.0]];
        assert_eq!(insert_point(&mut points, [128.0, 100.0]), Some(1));
        assert_eq!(insert_point(&mut points, [64.0, 40.0]), Some(1));
        assert_eq!(points[1], [64.0, 40.0]);
        assert_eq!(insert_point(&mut points, [64.3, 90.0]), None);
        assert_eq!(points.len(), 4);
    }

    #[test]
    fn insert_respects_capacity() {
        let mut points: Vec<[f32; 2]> = (0..curves::MAX_POINTS)
            .map(|i| [i as f32 * 10.0, 0.0])
            .collect();
        assert_eq!(insert_point(&mut points, [251.0, 0.0]), None);
    }

    #[test]
    fn move_stays_between_neighbors() {
        let mut points = vec![[0.0, 0.0], [128.0, 128.0], [255.0, 255.0]];
        move_point(&mut points, 1, [300.0, -10.0]);
        assert_eq!(points[1], [254.0, 0.0]);
        move_point(&mut points, 1, [-5.0, 300.0]);
        assert_eq!(points[1], [1.0, 255.0]);
        move_point(&mut points, 0, [50.0, 20.0]);
        assert_eq!(points[0], [0.0, 20.0], "x bornée par le voisin à 1");
    }

    #[test]
    fn end_points_cannot_be_removed() {
        let mut points = vec![[0.0, 0.0], [128.0, 128.0], [255.0, 255.0]];
        assert!(!remove_point(&mut points, 0));
        assert!(!remove_point(&mut points, 2));
        assert!(remove_point(&mut points, 1));
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn hit_picks_nearest_within_radius() {
        let points = vec![[0.0, 0.0], [100.0, 100.0], [106.0, 106.0]];
        assert_eq!(hit_point(&points, [105.0, 105.0], 10.0), Some(2));
        assert_eq!(hit_point(&points, [50.0, 10.0], 10.0), None);
    }

    #[test]
    fn editor_renders_without_edit() {
        let ctx = egui::Context::default();
        let theme = cygnus_ui::theme::UiThemeExt::cygnus_theme(&ctx);
        let mut out = Some(String::new());
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                out = curve_editor(ui, &theme, 1u8, curves::IDENTITY);
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(out, None);
    }
}
