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
/// (`dest`, conventions identiques au canvas : la miniature occupe `dest`
/// en entier, donc `écran = dest.min + miniature * dest.width() /
/// thumb_size_x`, avec `miniature = doc / thumb_to_doc + origin` —
/// `origin` est en pixels miniature, `doc` en pixels document).
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
    ink_geom_screen_rect(
        frame.x,
        frame.y,
        frame.width,
        frame.height,
        transform,
        dest,
        origin,
        thumb_to_doc,
        thumb_size_x,
    )
}

/// Même placement depuis une géométrie stockée (redraw persistant sans
/// re-rendre : seul `dest` change au zoom/pan — state-only).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn ink_geom_screen_rect(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    transform: &photo_engine::Transform2D,
    dest: egui::Rect,
    origin: egui::Vec2,
    thumb_to_doc: f32,
    thumb_size_x: f32,
) -> Option<egui::Rect> {
    if w == 0 || h == 0 {
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
    let (x0, y0) = ink_layer_to_doc(x as f32, y as f32, transform)?;
    let (x1, y1) = ink_layer_to_doc(x as f32 + w as f32, y as f32 + h as f32, transform)?;
    // Unités : `origin` en px miniature, `(x0,y0)` en px document.
    // miniature = doc / k + origin, écran = dest.min + miniature *
    // (dest.width() / thumb_size_x). Jamais de soustraction mixte.
    let screen_per_thumb = dest.width() / thumb_size_x;
    let to_screen = screen_per_thumb / thumb_to_doc;
    let screen = |dx: f32, dy: f32| {
        egui::pos2(
            dest.min.x + dx * to_screen + origin.x * screen_per_thumb,
            dest.min.y + dy * to_screen + origin.y * screen_per_thumb,
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

/// Géométrie de la frame overlay mise en cache avec sa texture : le
/// redraw persistant replace la même image à chaque frame (zoom/pan
/// state-only) sans re-rendre ni re-téléverser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkFrameGeom {
    /// Origine région en coords calque absolues.
    pub x: i32,
    /// Origine région en coords calque absolues.
    pub y: i32,
    /// Largeur région (px).
    pub w: u32,
    /// Hauteur région (px).
    pub h: u32,
    /// Génération du geste (`is_current` : rejet périmé).
    pub generation: u64,
    /// Transform calque→document snapshotée au rendu.
    pub transform: photo_engine::Transform2D,
}

/// Entrée d'une frame d'encre (un paquet de points du drag courant).
pub struct InkFrameInput<'a> {
    /// Points de CE frame en pixels document (mêmes valeurs qu'au commit).
    pub points: &'a [(f32, f32)],
    /// Vrai si le geste commence ce frame (`stroke` vide + points).
    pub gesture_started: bool,
    /// Vrai = gomme, faux = pinceau.
    pub eraser: bool,
    /// Réglages pinceau/gomme de l'app.
    pub brush: PhotoBrushSettings,
    /// Transform du calque cible (snapshot panneau).
    pub transform: photo_engine::Transform2D,
    /// Feedback affichable (transform plaçable — snapshot moteur).
    pub overlay_live: bool,
    /// Rectangle écran de la miniature (`None` = pas d'image).
    pub dest: Option<egui::Rect>,
    /// Coin haut-gauche du document en pixels miniature.
    pub origin: egui::Vec2,
    /// Facteur miniature→pixels document.
    pub thumb_to_doc: f32,
    /// Largeur miniature en pixels.
    pub thumb_w: f32,
}

/// Alimente l'overlay avec les points de CE frame et affiche le trait
/// transitoire : `begin_stroke` à l'ouverture du geste, `add_points` +
/// `render` à chaque paquet, téléversement (mise à jour en place à
/// dimensions égales, recréation à la croissance) puis dessin immédiat.
/// Retourne le rectangle écran dessiné (`None` = rien à afficher ce
/// frame — le redraw persistant prend le relais).
///
/// AUCUN envoi worker ici : le worker ne voit que le commit au `MouseUp`.
/// Ne re-rend jamais le document complet (région bornée au trait) et ne
/// touche jamais à la texture du composite (zoom/pan state-only intacts).
pub fn feed_ink_frame(
    ui: &mut egui::Ui,
    state: &mut PhotoUiState,
    input: InkFrameInput<'_>,
) -> Option<egui::Rect> {
    if !input.overlay_live {
        return None;
    }
    if input.gesture_started {
        state
            .ink_overlay
            .begin_stroke(ink_brush_params(&input.brush, input.eraser));
        // Nouvelle génération : l'ancienne texture ne doit jamais resservir.
        state.ink_texture = None;
        state.ink_frame = None;
    }
    if input.points.is_empty() {
        return None;
    }
    state.ink_overlay.add_points(input.points);
    let frame = state.ink_overlay.render()?;
    state.ink_metrics = state.ink_overlay.metrics();
    // Rejet périmé : une frame d'une génération antérieure (geste
    // annulé/commis entre-temps) ne s'affiche jamais.
    if !state.ink_overlay.is_current(frame.generation) {
        return None;
    }
    let dest = input.dest?;
    let rect = ink_overlay_screen_rect(
        &frame,
        &input.transform,
        dest,
        input.origin,
        input.thumb_to_doc,
        input.thumb_w,
    )?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    );
    // Même dimensions : mise à jour en place (id stable, zéro realloc
    // GPU) ; croissance de la région : recréation propre.
    let same_size = state
        .ink_frame
        .is_some_and(|g| g.w == frame.width && g.h == frame.height);
    if same_size {
        if let Some(handle) = state.ink_texture.as_mut() {
            handle.set(image, egui::TextureOptions::NEAREST);
        }
    } else {
        state.ink_texture = Some(ui.ctx().load_texture(
            "photo_ink_overlay",
            image,
            egui::TextureOptions::NEAREST,
        ));
    }
    state.ink_frame = Some(InkFrameGeom {
        x: frame.x,
        y: frame.y,
        w: frame.width,
        h: frame.height,
        generation: frame.generation,
        transform: input.transform,
    });
    if let Some(texture) = &state.ink_texture {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    Some(rect)
}

/// Redessine l'overlay en cache SANS nouveau point ni re-rendu : même
/// texture, rectangle replacé au `dest` courant (paliers du drag,
/// zoom/pan en cours de geste — state-only). Retourne faux si rien
/// n'est affichable (pas de geste en vol, génération périmée).
pub fn draw_cached_ink(
    ui: &mut egui::Ui,
    state: &PhotoUiState,
    dest: Option<egui::Rect>,
    origin: egui::Vec2,
    thumb_to_doc: f32,
    thumb_w: f32,
) -> bool {
    let (Some(geom), Some(texture), Some(dest)) =
        (state.ink_frame, state.ink_texture.as_ref(), dest)
    else {
        return false;
    };
    if !state.ink_overlay.is_current(geom.generation) {
        return false;
    }
    let Some(rect) = ink_geom_screen_rect(
        geom.x,
        geom.y,
        geom.w,
        geom.h,
        &geom.transform,
        dest,
        origin,
        thumb_to_doc,
        thumb_w,
    ) else {
        return false;
    };
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    true
}

/// Vide l'état transitoire (commit, annulation, changement d'outil) :
/// overlay moteur + texture + géométrie + points en vol. Idempotent,
/// jamais vers le worker (le commit est routé séparément, inchangé).
pub fn clear_ink(ui: &mut PhotoUiState) {
    ui.ink_overlay.clear();
    ui.ink_texture = None;
    ui.ink_frame = None;
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
        assert!(ui.ink_frame.is_none(), "géométrie larguée");
    }

    /// Contexte de feed headless : dest + miniature identité (pas de
    /// worker, pas de document — même contrat que `central_view`).
    fn feed_input<'a>(
        points: &'a [(f32, f32)],
        gesture_started: bool,
        dest: Option<egui::Rect>,
    ) -> InkFrameInput<'a> {
        InkFrameInput {
            points,
            gesture_started,
            eraser: false,
            brush: PhotoBrushSettings::default(),
            transform: photo_engine::Transform2D::default(),
            overlay_live: true,
            dest,
            origin: egui::Vec2::ZERO,
            thumb_to_doc: 1.0,
            thumb_w: 200.0,
        }
    }

    fn feed_dest() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 160.0))
    }

    #[test]
    fn rect_origine_non_nulle_et_miniature_reduite() {
        // Plan infini + miniature plafonnée : l'origine (px miniature) ne
        // se soustrait JAMAIS aux coords document — régression de la
        // formule mixte `(doc - origin)`.
        let dest = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 100.0));
        let origin = egui::vec2(20.0, 10.0);
        let rect = ink_overlay_screen_rect(
            &frame_at(10, 20, 30, 40),
            &photo_engine::Transform2D::default(),
            dest,
            origin,
            2.0,
            100.0,
        )
        .expect("plaçable");
        // miniature = doc/2 + origin : (10/2+20, 20/2+10) = (25, 20) ;
        // écran = miniature * 2 => (50, 40)..(80, 80).
        assert_eq!(rect.min, egui::pos2(50.0, 40.0));
        assert_eq!(rect.max, egui::pos2(80.0, 80.0));
    }

    #[test]
    fn feed_donne_feedback_avant_commit_en_paquets() {
        // Contrat central : 3 paquets (3 frames de drag) ⇒ texture
        // visible AVANT tout MouseUp, sans worker (aucune action
        // produite ici par construction — `feed_ink_frame` ne retourne
        // qu'un rectangle, jamais de commande).
        let ctx = egui::Context::default();
        let mut state = PhotoUiState::default();
        let dest = feed_dest();
        let packets: [&[(f32, f32)]; 3] = [
            &[(10.0, 10.0), (20.0, 20.0)],
            &[(30.0, 25.0), (40.0, 30.0)],
            &[(50.0, 10.0), (55.0, 40.0)],
        ];
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for (i, packet) in packets.iter().enumerate() {
                    let rect =
                        feed_ink_frame(ui, &mut state, feed_input(packet, i == 0, Some(dest)))
                            .expect("feedback visible chaque frame du drag");
                    assert!(rect.width() > 0.0 && rect.height() > 0.0);
                }
            });
        })
        .drop_without_applying_deltas();
        assert!(state.ink_texture.is_some(), "texture avant MouseUp");
        assert!(state.ink_frame.is_some(), "géométrie avant MouseUp");
        assert_eq!(state.ink_metrics.points_received, 6);
        assert_eq!(state.ink_metrics.submissions, 3);
        assert_eq!(state.ink_metrics.completions, 3);
        assert!(state.ink_metrics.surface_pixels_written > 0);
    }

    #[test]
    fn redraw_persistant_sans_nouveau_point_et_id_stable() {
        // Palier du drag (bouton tenu, aucun point ce frame) : la texture
        // en cache se redessine, même id (zéro realloc GPU).
        let ctx = egui::Context::default();
        let mut state = PhotoUiState::default();
        let dest = feed_dest();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = feed_ink_frame(
                    ui,
                    &mut state,
                    feed_input(&[(10.0, 10.0), (40.0, 40.0)], true, Some(dest)),
                )
                .expect("premier paquet");
                // Second paquet entièrement coalescé (doublon) : aucun
                // nouveau pixel, mais la frame reste affichable.
                let _ = feed_ink_frame(
                    ui,
                    &mut state,
                    feed_input(&[(40.0, 40.0), (40.0, 40.0)], false, Some(dest)),
                );
                assert_eq!(state.ink_metrics.points_coalesced, 2);
                // Palier : aucun point, redraw persistant.
                assert!(
                    draw_cached_ink(ui, &state, Some(dest), egui::Vec2::ZERO, 1.0, 200.0),
                    "overlay persistant sans nouveau point"
                );
            });
        })
        .drop_without_applying_deltas();
        let id = state.ink_texture.as_ref().expect("texture").id();
        // Autre frame, même geste : l'id ne bouge pas (update en place).
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert!(draw_cached_ink(
                    ui,
                    &state,
                    Some(dest),
                    egui::Vec2::ZERO,
                    1.0,
                    200.0
                ));
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(state.ink_texture.as_ref().expect("texture").id(), id);
    }

    #[test]
    fn zoom_pan_ne_regenere_rien_meme_texture() {
        // State-only : 3 placements (zoom x2, pan) de la MÊME frame ⇒
        // même id de texture, rectangles mis à l'échelle/décalés, zéro
        // re-rendu (submissions inchangées).
        let ctx = egui::Context::default();
        let mut state = PhotoUiState::default();
        let dest = feed_dest();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = feed_ink_frame(
                    ui,
                    &mut state,
                    feed_input(&[(10.0, 10.0), (40.0, 40.0)], true, Some(dest)),
                )
                .expect("premier paquet");
            });
        })
        .drop_without_applying_deltas();
        let submissions = state.ink_metrics.submissions;
        let id = state.ink_texture.as_ref().expect("texture").id();
        let geom = state.ink_frame.expect("géométrie");
        // Zoom x2 : dest deux fois plus large, même miniature.
        let zoomed = egui::Rect::from_min_size(dest.min, dest.size() * 2.0);
        let plain = super::ink_geom_screen_rect(
            geom.x,
            geom.y,
            geom.w,
            geom.h,
            &geom.transform,
            dest,
            egui::Vec2::ZERO,
            1.0,
            200.0,
        )
        .expect("rect de référence");
        let scaled = super::ink_geom_screen_rect(
            geom.x,
            geom.y,
            geom.w,
            geom.h,
            &geom.transform,
            zoomed,
            egui::Vec2::ZERO,
            1.0,
            200.0,
        )
        .expect("rect zoomé");
        assert!((scaled.width() - plain.width() * 2.0).abs() < 1e-3);
        assert!((scaled.height() - plain.height() * 2.0).abs() < 1e-3);
        // Pan : dest décalée, rectangle décalé d'autant.
        let panned = dest.translate(egui::vec2(30.0, -12.0));
        let moved = super::ink_geom_screen_rect(
            geom.x,
            geom.y,
            geom.w,
            geom.h,
            &geom.transform,
            panned,
            egui::Vec2::ZERO,
            1.0,
            200.0,
        )
        .expect("rect pané");
        assert!((moved.min.x - plain.min.x - 30.0).abs() < 1e-3);
        assert!((moved.min.y - plain.min.y + 12.0).abs() < 1e-3);
        assert_eq!(state.ink_metrics.submissions, submissions, "zéro re-rendu");
        assert_eq!(state.ink_texture.as_ref().expect("texture").id(), id);
    }

    #[test]
    fn gate_overlay_live_et_cancel() {
        // Calque non plaçable (rotation) : aucun feedback, aucune
        // texture — et le commit reste possible par ailleurs (moteur
        // exact, présentation seule refusée).
        let ctx = egui::Context::default();
        let mut state = PhotoUiState::default();
        let dest = feed_dest();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut blocked = feed_input(&[(10.0, 10.0)], true, Some(dest));
                blocked.overlay_live = false;
                assert!(feed_ink_frame(ui, &mut state, blocked).is_none());
            });
        })
        .drop_without_applying_deltas();
        assert!(state.ink_texture.is_none());
        assert!(state.ink_frame.is_none());
        // Cancel après un vrai geste : tout est vidé, le redraw ne
        // ressuscite rien (génération conservée, contenu vide).
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = feed_ink_frame(
                    ui,
                    &mut state,
                    feed_input(&[(10.0, 10.0), (40.0, 40.0)], true, Some(dest)),
                )
                .expect("geste");
                clear_ink(&mut state);
                assert!(!draw_cached_ink(
                    ui,
                    &state,
                    Some(dest),
                    egui::Vec2::ZERO,
                    1.0,
                    200.0
                ));
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn mesures_geste_scripte_dix_frames() {
        // Compteurs déterministes d'un drag scripté (10 frames x 8 pts,
        // avec doublons volontaires) : frames interactives, points
        // coalescés, pixels écrits. Aucun mur asserté (matériel variable ;
        // voir les tests `perf_*` ignorés pour les murs).
        let ctx = egui::Context::default();
        let mut state = PhotoUiState::default();
        let dest = feed_dest();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for frame in 0..10 {
                    let t0 = 100.0 + frame as f32 * 24.0;
                    let mut packet: Vec<(f32, f32)> = (0..8)
                        .map(|i| (t0 + i as f32 * 3.0, 260.0 + i as f32))
                        .collect();
                    // Doublon coalescé chaque frame paire.
                    if frame % 2 == 0 {
                        packet.push(packet[7]);
                    }
                    let _ =
                        feed_ink_frame(ui, &mut state, feed_input(&packet, frame == 0, Some(dest)));
                }
            });
        })
        .drop_without_applying_deltas();
        let m = state.ink_metrics;
        assert_eq!(m.points_received, 85, "80 + 5 doublons");
        assert_eq!(m.points_coalesced, 5, "doublons comptés");
        assert_eq!(m.submissions, 10, "10 frames interactives");
        assert_eq!(
            m.completions, 10,
            "10 rendus (synchrone : 1 frame ⇒ visible)"
        );
        assert!(m.surface_pixels_written > 0, "pixels du trait");
        eprintln!(
            "mesures-drag : frames={} coalesces={} pixels={} dernier_render_us={}",
            m.completions, m.points_coalesced, m.surface_pixels_written, m.last_render_us
        );
    }

    #[test]
    fn feed_emet_des_primitives_dessinees() {
        // Le trait transitoire produit VRAIMENT des formes egui (pas
        // seulement un état interne) : shapes non vides sur la frame.
        let ctx = egui::Context::default();
        let mut state = PhotoUiState::default();
        let dest = feed_dest();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = feed_ink_frame(
                    ui,
                    &mut state,
                    feed_input(&[(10.0, 10.0), (40.0, 40.0)], true, Some(dest)),
                )
                .expect("geste");
            });
        });
        assert!(!output.shapes.is_empty(), "formes émises pendant le drag");
        output.drop_without_applying_deltas();
    }
}
