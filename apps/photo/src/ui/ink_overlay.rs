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

//! Overlay transitoire de trait (Phase 6G.3P) : helpers PURS côté app.
//!
//! Le geste (pinceau/gomme) peint DEUX fois avec les MÊMES points et les
//! MÊMES paramètres : une fois en transitoire dans
//! [`photo_engine::interaction::InteractionOverlay`] (présenté chaque frame
//! du drag), une fois au commit via `PaintRequest` → worker (chemin
//! historique inchangé). Ce module garantit que les deux divergent jamais :
//! [`ink_brush_params`] construit EXACTEMENT les paramètres du worker (cf.
//! `engine_bridge`, bras `PaintStroke`), et [`ink_overlay_screen_rect`]
//! place la région overlay à l'écran via la transform snapshotée du calque.
//!
//! Aucun envoi worker ici : pendant le drag, l'app ne pousse AUCUNE action
//! (zéro trafic, miniatures différées au commit). `clear_ink` vide l'état
//! transitoire (commit, annulation Échap, changement d'outil).

use crate::state::PhotoUiState;
use crate::ui::viewport::{PhotoBrushSettings, PhotoCanvasTool};

/// Paramètres pinceau pour l'overlay : IDENTIQUES à ceux du commit worker
/// (`engine_bridge`, bras `PaintStroke` — même rayon, même couleur, même
/// opacité, même mode). Toute divergence casserait l'équivalence
/// transitoire == commit (test `params_identiques_au_commit`).
#[must_use]
pub fn ink_brush_params(
    brush: &PhotoBrushSettings,
    eraser: bool,
) -> photo_engine::paint::BrushParams {
    photo_engine::paint::BrushParams {
        radius: brush.radius,
        color: brush.color,
        opacity: brush.opacity.clamp(0.0, 1.0),
        mode: if eraser {
            photo_engine::paint::StrokeMode::Erase
        } else {
            photo_engine::paint::StrokeMode::Paint
        },
    }
}

/// Vrai si l'outil peint (overlay pertinent : pinceau/gomme uniquement,
/// jamais déplacement ni pipette).
#[must_use]
pub fn ink_tool_active(tool: PhotoCanvasTool) -> bool {
    matches!(tool, PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser)
}

/// Calque → document (translation/échelle UNIQUEMENT, cf.
/// [`photo_engine::interaction::overlay_displayable`]) : `None` si la
/// transform n'est pas plaçable (rotation/skew — présentation refusée,
/// moteur et commit exacts quand même).
#[must_use]
pub fn ink_layer_to_doc(
    x: f32,
    y: f32,
    transform: &photo_engine::Transform2D,
) -> Option<(f32, f32)> {
    if !photo_engine::interaction::overlay_displayable(transform) {
        return None;
    }
    let (x, y) = (
        x * transform.scale_x + transform.offset_x,
        y * transform.scale_y + transform.offset_y,
    );
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// Rectangle écran d'une frame overlay : région calque → document (via
/// `transform` snapshotée) → miniature (`origin`, `thumb_to_doc`) → écran
/// (`dest`, conventions identiques au canvas : `screen = dest.min +
/// (doc - origin) / thumb_to_doc * dest.width() / thumb_size_x`).
/// `None` = non plaçable (transform refusée ou géométrie dégénérée).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn ink_overlay_screen_rect(
    frame: &photo_engine::interaction::OverlayFrame,
    transform: &photo_engine::Transform2D,
    dest: egui::Rect,
    origin: egui::Vec2,
    thumb_to_doc: f32,
    thumb_size_x: f32,
) -> Option<egui::Rect> {
    if frame.width == 0 || frame.height == 0 {
        return None;
    }
    if !thumb_to_doc.is_finite()
        || thumb_to_doc <= f32::EPSILON
        || !thumb_size_x.is_finite()
        || thumb_size_x <= 0.0
        || !dest.width().is_finite()
        || dest.width() <= 0.0
    {
        return None;
    }
    let (x0, y0) = ink_layer_to_doc(frame.x as f32, frame.y as f32, transform)?;
    let (x1, y1) = ink_layer_to_doc(
        frame.x as f32 + frame.width as f32,
        frame.y as f32 + frame.height as f32,
        transform,
    )?;
    let to_screen = dest.width() / thumb_size_x / thumb_to_doc;
    let screen = |dx: f32, dy: f32| {
        egui::pos2(
            dest.min.x + (dx - origin.x) * to_screen,
            dest.min.y + (dy - origin.y) * to_screen,
        )
    };
    let (a, b) = (screen(x0, y0), screen(x1, y1));
    (a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()).then(|| {
        egui::Rect::from_min_max(
            egui::pos2(a.x.min(b.x), a.y.min(b.y)),
            egui::pos2(a.x.max(b.x), a.y.max(b.y)),
        )
    })
}

/// Vide l'état transitoire (commit, annulation, changement d'outil) :
/// overlay moteur + texture + points en vol. Idempotent, jamais vers
/// le worker (le commit est routé séparément, inchangé).
pub fn clear_ink(ui: &mut PhotoUiState) {
    ui.ink_overlay.clear();
    ui.ink_texture = None;
    ui.stroke.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_rect() -> (egui::Rect, egui::Vec2, f32, f32) {
        (
            egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 160.0)),
            egui::Vec2::ZERO,
            1.0,
            200.0,
        )
    }

    fn frame_at(x: i32, y: i32, w: u32, h: u32) -> photo_engine::interaction::OverlayFrame {
        photo_engine::interaction::OverlayFrame {
            generation: 1,
            x,
            y,
            width: w,
            height: h,
            rgba: vec![0; (w * h * 4) as usize],
        }
    }

    #[test]
    fn params_identiques_au_commit() {
        // Mêmes entrées que `PaintRequest` → mêmes `BrushParams` que le
        // worker (bras `PaintStroke`) : l'overlay et le commit peignent
        // pareil, par construction.
        let brush = PhotoBrushSettings {
            radius: 7.0,
            color: [10, 20, 30],
            opacity: 0.5,
        };
        let paint = ink_brush_params(&brush, false);
        assert_eq!(paint.radius, 7.0);
        assert_eq!(paint.color, [10, 20, 30]);
        assert_eq!(paint.opacity, 0.5);
        assert!(matches!(paint.mode, photo_engine::paint::StrokeMode::Paint));
        let erase = ink_brush_params(&brush, true);
        assert!(matches!(erase.mode, photo_engine::paint::StrokeMode::Erase));
        // Clamp identique au worker (hors 0..=1 ramené dedans).
        let wild = PhotoBrushSettings {
            radius: 3.0,
            color: [0, 0, 0],
            opacity: 9.0,
        };
        assert_eq!(ink_brush_params(&wild, false).opacity, 1.0);
    }

    #[test]
    fn seuls_pinceau_et_gomme_alimentent_l_overlay() {
        assert!(ink_tool_active(PhotoCanvasTool::Brush));
        assert!(ink_tool_active(PhotoCanvasTool::Eraser));
        assert!(!ink_tool_active(PhotoCanvasTool::Move));
        assert!(!ink_tool_active(PhotoCanvasTool::Eyedropper));
    }

    #[test]
    fn rect_identite_place_au_bon_endroit() {
        let (dest, origin, to_doc, thumb_w) = identity_rect();
        let rect = ink_overlay_screen_rect(
            &frame_at(10, 20, 30, 40),
            &photo_engine::Transform2D::default(),
            dest,
            origin,
            to_doc,
            thumb_w,
        )
        .expect("identité plaçable");
        assert_eq!(rect.min, egui::pos2(110.0, 70.0));
        assert_eq!(rect.max, egui::pos2(140.0, 110.0));
    }

    #[test]
    fn rect_applique_offset_et_echelle_du_calque() {
        let (dest, origin, to_doc, thumb_w) = identity_rect();
        let transform = photo_engine::Transform2D {
            offset_x: 5.0,
            offset_y: 7.0,
            scale_x: 2.0,
            scale_y: 2.0,
            ..photo_engine::Transform2D::default()
        };
        let rect = ink_overlay_screen_rect(
            &frame_at(10, 20, 4, 6),
            &transform,
            dest,
            origin,
            to_doc,
            thumb_w,
        )
        .expect("translation/échelle plaçable");
        // doc : (10*2+5, 20*2+7) = (25, 47) → écran dest.min + doc.
        assert_eq!(rect.min, egui::pos2(125.0, 97.0));
        assert_eq!(rect.max, egui::pos2(133.0, 109.0));
    }

    #[test]
    fn rotation_ou_skew_refuse_la_presentation() {
        let (dest, origin, to_doc, thumb_w) = identity_rect();
        for transform in [
            photo_engine::Transform2D {
                rotation_deg: 15.0,
                ..photo_engine::Transform2D::default()
            },
            photo_engine::Transform2D {
                skew_x: 5.0,
                ..photo_engine::Transform2D::default()
            },
        ] {
            assert!(
                ink_overlay_screen_rect(
                    &frame_at(0, 0, 8, 8),
                    &transform,
                    dest,
                    origin,
                    to_doc,
                    thumb_w
                )
                .is_none(),
                "rotation/skew : pas de présentation"
            );
            assert!(
                ink_layer_to_doc(1.0, 2.0, &transform).is_none(),
                "porte calque→document : même refus"
            );
        }
    }

    #[test]
    fn geometrie_degeneree_refusee() {
        let (dest, origin, _, thumb_w) = identity_rect();
        let identity = photo_engine::Transform2D::default();
        assert!(
            ink_overlay_screen_rect(&frame_at(0, 0, 0, 8), &identity, dest, origin, 1.0, thumb_w)
                .is_none()
        );
        assert!(
            ink_overlay_screen_rect(&frame_at(0, 0, 8, 8), &identity, dest, origin, 0.0, thumb_w)
                .is_none()
        );
        assert!(
            ink_overlay_screen_rect(&frame_at(0, 0, 8, 8), &identity, dest, origin, 1.0, 0.0)
                .is_none()
        );
    }

    #[test]
    fn metrics_remontent_dans_l_etat() {
        // Même séquence que `central_view` : les métriques stockées dans
        // l'état reflètent le geste (rapport §13, jamais affichées).
        let mut ui = PhotoUiState::default();
        ui.ink_overlay
            .begin_stroke(ink_brush_params(&PhotoBrushSettings::default(), false));
        ui.ink_overlay.add_points(&[(1.0, 2.0), (9.0, 9.0)]);
        let frame = ui.ink_overlay.render().expect("frame");
        assert!(ui.ink_overlay.is_current(frame.generation));
        ui.ink_metrics = ui.ink_overlay.metrics();
        assert_eq!(ui.ink_metrics.points_received, 2);
        assert_eq!(ui.ink_metrics.submissions, 1);
        assert_eq!(ui.ink_metrics.completions, 1);
        assert!(ui.ink_metrics.surface_pixels_written > 0);
    }

    #[test]
    fn clear_ink_vide_tout_sans_worker() {
        let mut ui = PhotoUiState::default();
        ui.stroke.push(egui::vec2(1.0, 2.0));
        ui.ink_overlay
            .begin_stroke(ink_brush_params(&PhotoBrushSettings::default(), false));
        ui.ink_overlay.add_points(&[(1.0, 2.0), (9.0, 9.0)]);
        let _ = ui.ink_overlay.render();
        clear_ink(&mut ui);
        assert!(ui.stroke.is_empty(), "points en vol vidés");
        assert!(
            ui.ink_overlay.render().is_none(),
            "overlay vide après clear"
        );
        assert!(ui.ink_texture.is_none(), "texture larguée");
    }
}
