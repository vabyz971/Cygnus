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

//! Photo overlays: document frame + brush cursor
//! over the generic viewport.
//!
//! Composition layer: geometry comes from UI state (document
//! screen rect), drawing from agnostic helpers
//! ([`draw_crosshair`](cygnus_ui::viewport::draw_crosshair)), styling from
//! the theme (no hardcoded color or size).

use crate::commands::PhotoUiContext;
use cygnus_ui::viewport::draw_crosshair;

/// Document boundary: hairline frame around the DOCUMENT's
/// screen rect (`None` = no image, nothing to draw), with
/// its dimensions at top left (`label`, e.g. `"800 x 600"`).
pub fn draw_document_bounds(
    painter: &egui::Painter,
    ctx: &PhotoUiContext,
    dest: Option<egui::Rect>,
    label: Option<&str>,
) {
    let Some(dest) = dest else {
        return;
    };
    let theme = ctx.shared.theme();
    painter.rect_stroke(
        dest,
        0.0,
        egui::Stroke::new(theme.borders.thin, theme.colors.border),
        egui::StrokeKind::Outside,
    );
    if let Some(label) = label {
        painter.text(
            dest.min + egui::vec2(theme.spacing.xs, theme.spacing.xs),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(theme.typography.caption_size),
            theme.colors.fg_secondary,
        );
    }
}

/// Gradient tool feedback: line from `start` to `end` (screen points)
/// with a ring at each end.
pub fn draw_gradient_line(
    painter: &egui::Painter,
    ctx: &PhotoUiContext,
    start: egui::Pos2,
    end: egui::Pos2,
) {
    let theme = ctx.shared.theme();
    let stroke = egui::Stroke::new(theme.borders.medium, theme.colors.accent);
    painter.line_segment([start, end], stroke);
    for point in [start, end] {
        painter.circle(point, theme.spacing.xs, theme.colors.bg_canvas, stroke);
    }
}

/// Clone stamp source marker: a small crosshair ring at `at`.
pub fn draw_clone_source(painter: &egui::Painter, ctx: &PhotoUiContext, at: egui::Pos2) {
    let theme = ctx.shared.theme();
    let r = theme.spacing.sm;
    let dark = egui::Stroke::new(theme.borders.medium, theme.colors.bg_canvas);
    let light = egui::Stroke::new(theme.borders.thin, theme.colors.fg_strong);
    for stroke in [dark, light] {
        painter.circle_stroke(at, r, stroke);
        painter.line_segment(
            [at - egui::vec2(r * 1.6, 0.0), at - egui::vec2(r * 0.6, 0.0)],
            stroke,
        );
        painter.line_segment(
            [at + egui::vec2(r * 0.6, 0.0), at + egui::vec2(r * 1.6, 0.0)],
            stroke,
        );
        painter.line_segment(
            [at - egui::vec2(0.0, r * 1.6), at - egui::vec2(0.0, r * 0.6)],
            stroke,
        );
        painter.line_segment(
            [at + egui::vec2(0.0, r * 0.6), at + egui::vec2(0.0, r * 1.6)],
            stroke,
        );
    }
}

/// Dash and gap length of the marching ants, in screen points.
const ANTS_DASH: f32 = 4.0;

/// Selection outline ("marching ants"): `polylines` (screen points,
/// closed loops repeat their first point) drawn as a dark hairline
/// under light dashes shifted by `phase` (animated by the caller).
pub fn draw_selection_outline(
    painter: &egui::Painter,
    ctx: &PhotoUiContext,
    polylines: &[Vec<egui::Pos2>],
    phase: f32,
) {
    let theme = ctx.shared.theme();
    let dark = egui::Stroke::new(theme.borders.thin, theme.colors.bg_canvas);
    let light = egui::Stroke::new(theme.borders.thin, theme.colors.fg_strong);
    for line in polylines.iter().filter(|line| line.len() >= 2) {
        painter.add(egui::Shape::line(line.clone(), dark));
        painter.extend(egui::Shape::dashed_line_with_offset(
            line,
            light,
            &[ANTS_DASH],
            &[ANTS_DASH],
            phase.rem_euclid(ANTS_DASH * 2.0),
        ));
    }
}

/// Brush/eraser cursor: circle at the brush diameter + central
/// dot (`epaint` rendering via the egui `Painter` — no extra
/// dependency required). `radius_image` in image pixels, converted
/// to screen pixels via `scale` (screen/image). Nothing when off-image,
/// invalid radius, or unknown cursor.
pub fn draw_brush_cursor(
    painter: &egui::Painter,
    ctx: &PhotoUiContext,
    dest: Option<egui::Rect>,
    hover: Option<egui::Pos2>,
    radius_image: f32,
    scale: f32,
) {
    let (Some(dest), Some(center)) = (dest, hover) else {
        return;
    };
    if !radius_image.is_finite() || !scale.is_finite() || radius_image <= 0.0 || scale <= 0.0 {
        return;
    }
    if !dest.contains(center) {
        return;
    }
    let theme = ctx.shared.theme();
    let radius = (radius_image * scale).max(1.0);
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(theme.borders.thin, theme.colors.fg_primary),
    );
    draw_crosshair(
        painter,
        center,
        theme.spacing.xs,
        egui::Stroke::new(theme.borders.thin, theme.colors.fg_primary),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_bounds_and_brush_cursor_render_without_panic() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let dest =
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 150.0));
                // Bounds: with and without image, with and without label.
                draw_document_bounds(ui.painter(), &photo_ctx, Some(dest), Some("800 x 600"));
                draw_document_bounds(ui.painter(), &photo_ctx, Some(dest), None);
                draw_document_bounds(ui.painter(), &photo_ctx, None, None);
                // Cursor: inside, outside, invalid — never a panic.
                draw_brush_cursor(
                    ui.painter(),
                    &photo_ctx,
                    Some(dest),
                    Some(egui::pos2(50.0, 50.0)),
                    8.0,
                    1.5,
                );
                draw_brush_cursor(
                    ui.painter(),
                    &photo_ctx,
                    Some(dest),
                    Some(egui::pos2(500.0, 500.0)),
                    8.0,
                    1.5,
                );
                draw_brush_cursor(ui.painter(), &photo_ctx, None, None, 8.0, 1.5);
                draw_brush_cursor(
                    ui.painter(),
                    &photo_ctx,
                    Some(dest),
                    Some(egui::pos2(50.0, 50.0)),
                    0.0,
                    1.5,
                );
                draw_brush_cursor(
                    ui.painter(),
                    &photo_ctx,
                    Some(dest),
                    Some(egui::pos2(50.0, 50.0)),
                    f32::NAN,
                    1.5,
                );
            });
        })
        .drop_without_applying_deltas();
    }
}
