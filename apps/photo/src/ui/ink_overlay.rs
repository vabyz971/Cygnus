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

//! Transient stroke overlay (Phase 6G.3P): PURE app-side helpers.
//!
//! The gesture (brush/eraser) paints TWICE with the SAME points and the
//! SAME parameters: once transiently in
//! [`photo_engine::interaction::InteractionOverlay`] (presented each frame
//! of the drag), once at commit via `PaintRequest` → worker (unchanged
//! historical path). Both go through the same `resample_stroke`
//! (subdivision + Chaikin): the transient shows the same curves as
//! the commit, only the inter-packet junction anchoring differs (gap
//! bounded to junction discs, replaced at `MouseUp` — see
//! `transient_proche_du_commit_avec_lissage`). This module also guarantees
//! identical parameters: [`ink_brush_params`] builds
//! EXACTLY the worker's parameters (see `engine_bridge`, `PaintStroke`
//! arm — same radius, same color, same opacity, same mode),
//! and [`ink_overlay_screen_rect`] places the overlay region on screen via
//! the layer's snapshotted transform.
//!
//! No worker send here: during the drag, the app pushes NO action
//! (zero traffic, thumbnails deferred to commit). `clear_ink` drains the
//! transient state (commit, Escape cancel, tool switch).

use crate::state::PhotoUiState;
use crate::ui::viewport::{PhotoBrushSettings, PhotoCanvasTool};

/// Brush parameters for the overlay: IDENTICAL to the worker commit's
/// (`engine_bridge`, `PaintStroke` arm — same radius, same color, same
/// opacity, same mode). Any divergence would break the
/// transient == commit equivalence (`params_identiques_au_commit` test).
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

/// True when the tool paints (relevant overlay: brush/eraser only,
/// never move or eyedropper).
#[must_use]
pub fn ink_tool_active(tool: PhotoCanvasTool) -> bool {
    matches!(tool, PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser)
}

/// Layer → document (translation/scale ONLY, see
/// [`photo_engine::interaction::overlay_displayable`]): `None` when the
/// transform is not placeable (rotation/skew — presentation refused,
/// engine and commit still exact).
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

/// Screen rect of an overlay frame: layer region → document (via
/// snapshotted `transform`) → thumbnail (`origin`, `thumb_to_doc`) → screen
/// (`dest`, same conventions as the canvas: the thumbnail fills `dest`
/// entirely, so `screen = dest.min + thumbnail * dest.width() /
/// thumb_size_x`, with `thumbnail = doc / thumb_to_doc + origin` —
/// `origin` in thumbnail pixels, `doc` in document pixels).
/// `None` = not placeable (rejected transform or degenerate geometry).
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

/// Same placement from stored geometry (persistent redraw without
/// re-rendering: only `dest` changes on zoom/pan — state-only).
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
    // Units: `origin` in thumbnail px, `(x0,y0)` in document px.
    // thumbnail = doc / k + origin, screen = dest.min + thumbnail *
    // (dest.width() / thumb_size_x). Never a mixed subtraction.
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

/// Overlay frame geometry cached with its texture: the
/// persistent redraw re-places the same image each frame (zoom/pan
/// state-only) without re-rendering or re-uploading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkFrameGeom {
    /// Region origin in absolute layer coords.
    pub x: i32,
    /// Region origin in absolute layer coords.
    pub y: i32,
    /// Region width (px).
    pub w: u32,
    /// Region height (px).
    pub h: u32,
    /// Gesture generation (`is_current`: stale rejection).
    pub generation: u64,
    /// Layer→document transform snapshotted at render.
    pub transform: photo_engine::Transform2D,
}

/// Ink frame input (one packet of points from the current drag).
pub struct InkFrameInput<'a> {
    /// THIS frame's points in document pixels (same values as at commit).
    pub points: &'a [(f32, f32)],
    /// True when the gesture starts this frame (empty `stroke` + points).
    pub gesture_started: bool,
    /// True = eraser, false = brush.
    pub eraser: bool,
    /// App brush/eraser settings.
    pub brush: PhotoBrushSettings,
    /// Target layer transform (panel snapshot).
    pub transform: photo_engine::Transform2D,
    /// Displayable feedback (placeable transform — engine snapshot).
    pub overlay_live: bool,
    /// Thumbnail screen rect (`None` = no image).
    pub dest: Option<egui::Rect>,
    /// Document top-left corner in thumbnail pixels.
    pub origin: egui::Vec2,
    /// Thumbnail→document-pixel factor.
    pub thumb_to_doc: f32,
    /// Thumbnail width in pixels.
    pub thumb_w: f32,
}

/// Feeds the overlay with THIS frame's points and displays the transient
/// stroke: `begin_stroke` when the gesture opens, `add_points` +
/// `render` per packet, upload (in-place update at equal
/// dimensions, recreate on growth) then immediate draw.
/// Returns the drawn screen rect (`None` = nothing to show this
/// frame — the persistent redraw takes over).
///
/// NO worker send here: the worker only sees the commit at `MouseUp`.
/// Never re-renders the full document (stroke-bounded region) and never
/// touches the composite texture (zoom/pan state-only intact).
pub fn feed_ink_frame(
    ui: &mut egui::Ui,
    state: &mut PhotoUiState,
    input: InkFrameInput<'_>,
) -> Option<egui::Rect> {
    if !input.overlay_live {
        return None;
    }
    if input.gesture_started {
        // Same selection clip as the worker commit.
        state.ink_overlay.set_clip(state.selection);
        state.ink_overlay.set_dynamics(input.brush.dynamics());
        state
            .ink_overlay
            .begin_stroke(ink_brush_params(&input.brush, input.eraser));
        // New generation: the old texture must never be reused.
        state.ink_texture = None;
        state.ink_frame = None;
    }
    if input.points.is_empty() {
        return None;
    }
    state.ink_overlay.add_points(input.points);
    let frame = state.ink_overlay.render()?;
    state.ink_metrics = state.ink_overlay.metrics();
    // Stale rejection: a frame from an earlier generation (gesture
    // cancelled/committed meanwhile) never displays.
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
    // Same dimensions: in-place update (stable id, zero GPU
    // realloc); region growth: clean recreate.
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

/// Redraws the cached overlay WITHOUT new points or re-render: same
/// texture, rect re-placed at the current `dest` (drag plateaus,
/// mid-gesture zoom/pan — state-only). Returns false when nothing
/// is displayable (no gesture in flight, stale generation).
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

/// Drains the transient state (commit, cancel, tool switch):
/// engine overlay + texture + geometry + in-flight points. Idempotent,
/// never to the worker (the commit is routed separately, unchanged).
pub fn clear_ink(ui: &mut PhotoUiState) {
    clear_ink_overlay(ui);
    ui.stroke.clear();
}

/// Drops the transient overlay only, keeping `stroke`: the selection
/// tool stores its marquee anchor there across frames.
pub fn clear_ink_overlay(ui: &mut PhotoUiState) {
    ui.ink_overlay.clear();
    ui.ink_texture = None;
    ui.ink_frame = None;
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
        // Same inputs as `PaintRequest` → same `BrushParams` as the
        // worker (`PaintStroke` arm): overlay and commit paint
        // alike, by construction.
        let brush = PhotoBrushSettings {
            radius: 7.0,
            color: [10, 20, 30],
            opacity: 0.5,
            ..PhotoBrushSettings::default()
        };
        let paint = ink_brush_params(&brush, false);
        assert_eq!(paint.radius, 7.0);
        assert_eq!(paint.color, [10, 20, 30]);
        assert_eq!(paint.opacity, 0.5);
        assert!(matches!(paint.mode, photo_engine::paint::StrokeMode::Paint));
        let erase = ink_brush_params(&brush, true);
        assert!(matches!(erase.mode, photo_engine::paint::StrokeMode::Erase));
        // Same clamp as the worker (out-of-0..=1 brought inside).
        let wild = PhotoBrushSettings {
            radius: 3.0,
            color: [0, 0, 0],
            opacity: 9.0,
            ..PhotoBrushSettings::default()
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
        // doc: (10*2+5, 20*2+7) = (25, 47) → screen dest.min + doc.
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
        // Same sequence as `central_view`: the metrics stored in
        // state reflect the gesture (§13 report, never displayed).
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

    /// Headless feed context: dest + identity thumbnail (no
    /// worker, no document — same contract as `central_view`).
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
        // Infinite plane + capped thumbnail: the origin (thumbnail px) is
        // NEVER subtracted from document coords — regression of the
        // mixed `(doc - origin)` formula.
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
        // thumbnail = doc/2 + origin: (10/2+20, 20/2+10) = (25, 20);
        // screen = thumbnail * 2 => (50, 40)..(80, 80).
        assert_eq!(rect.min, egui::pos2(50.0, 40.0));
        assert_eq!(rect.max, egui::pos2(80.0, 80.0));
    }

    #[test]
    fn feed_donne_feedback_avant_commit_en_paquets() {
        // Core contract: 3 packets (3 drag frames) ⇒ texture
        // visible BEFORE any MouseUp, no worker (no action
        // produced here by construction — `feed_ink_frame` only returns
        // a rect, never a command).
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
        // Drag plateau (button held, no points this frame): the cached
        // texture redraws, same id (zero GPU realloc).
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
                // Second packet fully coalesced (duplicate): no
                // new pixels, but the frame stays displayable.
                let _ = feed_ink_frame(
                    ui,
                    &mut state,
                    feed_input(&[(40.0, 40.0), (40.0, 40.0)], false, Some(dest)),
                );
                assert_eq!(state.ink_metrics.points_coalesced, 2);
                // Plateau: no points, persistent redraw.
                assert!(
                    draw_cached_ink(ui, &state, Some(dest), egui::Vec2::ZERO, 1.0, 200.0),
                    "overlay persistant sans nouveau point"
                );
            });
        })
        .drop_without_applying_deltas();
        let id = state.ink_texture.as_ref().expect("texture").id();
        // Other frame, same gesture: the id does not move (in-place update).
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
        // State-only: 3 placements (zoom x2, pan) of the SAME frame ⇒
        // same texture id, scaled/shifted rects, zero
        // re-render (unchanged submissions).
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
        // Zoom x2: dest twice as wide, same thumbnail.
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
        // Pan: shifted dest, rect shifted by as much.
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
        // Unplaceable layer (rotation): no feedback, no
        // texture — and commit stays possible otherwise (exact
        // engine, presentation alone refused).
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
        // Cancel after a real gesture: everything drained, the redraw
        // revives nothing (generation kept, empty content).
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
        // Deterministic counters of a scripted drag (10 frames x 8 pts,
        // with deliberate duplicates): interactive frames, coalesced
        // points, written pixels. No asserted walls (variable hardware;
        // see the ignored `perf_*` tests for walls).
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
                    // Coalesced duplicate each even frame.
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
        // The transient stroke REALLY produces egui shapes (not
        // just internal state): non-empty shapes on the frame.
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
