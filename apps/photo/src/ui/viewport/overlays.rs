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

//! Overlays photo : cadre du document + curseur du pinceau
//! par-dessus le viewport générique.
//!
//! Couche de composition : la géométrie vient de l'état UI (rectangle
//! écran du document), le dessin des helpers agnostiques
//! ([`draw_crosshair`](ui_kit::viewport::draw_crosshair)), le style du
//! thème (aucune couleur ni taille en dur).

use crate::commands::PhotoUiContext;
use ui_kit::viewport::draw_crosshair;

/// Délimitation du document : cadre accentué autour du rectangle
/// écran du DOCUMENT (`None` = pas d'image, rien à dessiner), avec
/// ses dimensions en haut à gauche (`label`, ex. `"800 x 600"`).
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
        egui::Stroke::new(theme.borders.medium, theme.colors.accent),
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

/// Curseur du pinceau/gomme : cercle au diamètre du pinceau + point
/// central (rendu `epaint` via le `Painter` egui — aucune dépendance
/// supplémentaire requise). `radius_image` en pixels image, converti
/// en pixels écran via `scale` (écran/image). Rien si hors image,
/// rayon invalide ou curseur inconnu.
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
        ui_kit::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let dest =
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 150.0));
                // Bornes : avec et sans image, avec et sans libellé.
                draw_document_bounds(ui.painter(), &photo_ctx, Some(dest), Some("800 x 600"));
                draw_document_bounds(ui.painter(), &photo_ctx, Some(dest), None);
                draw_document_bounds(ui.painter(), &photo_ctx, None, None);
                // Curseur : dedans, dehors, invalides — jamais de panic.
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
