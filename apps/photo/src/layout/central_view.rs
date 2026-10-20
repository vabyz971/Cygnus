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

//! "Canvas" dock tab content (one per open document).
//!
//! Each open document has its dock tab, titled with the
//! document name (see `super::dock`): activating the tab activates
//! the document, closing the tab closes the document. The canvas
//! (`PhotoCanvas`, generic viewport + photo tools) only returns
//! data: a stroke commit bubbles up as
//! [`PhotoAction::CommitStroke`](crate::commands::PhotoAction), a
//! move as [`PhotoAction::MoveLayer`](crate::commands::PhotoAction),
//! the eyedropper samples into local UI state. Document frame and
//! brush cursor on top.

use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::{OpenDocument, PreviewGeom, sample_preview_color};
use crate::ui::features::layers::find_layer_in;
use crate::ui::features::layers::types::PhotoLayerKind;
use crate::ui::{
    CanvasMapping, InkFrameInput, NodeDrag, NodePoint, NodeRef, PhotoCanvas, PhotoCanvasOutcome,
    PhotoCanvasTool, StabilityAction, StableView, TransformDrag, clear_ink, clear_ink_overlay,
    doc_to_screen, draw_brush_cursor, draw_cached_ink, draw_document_bounds, draw_gradient_line,
    draw_node_overlay, draw_selection_outline, draw_transform_overlay, feed_ink_frame,
    frame_corners, hit_test_topmost, ink_tool_active, path_nodes, point_in_quad, screen_to_doc,
    stability_action,
};
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use cygnus_ui::components::{menu_row, menu_style};
use cygnus_ui::prelude::UiThemeExt;

/// View-stability driver (all tools): keeps the document's
/// on-screen size across scope changes (see [`StableView`]). Pure
/// decision in [`stabilize_factor`]; here only the viewport actuation
/// (counter-zoom around the document center, which always coincides
/// with the symmetric scope's center).
fn stabilize_viewport(
    doc: &mut OpenDocument,
    outcome: &PhotoCanvasOutcome,
    geom: Option<PreviewGeom>,
    mapping: &CanvasMapping,
    pre_zoom: f32,
    pre_offset: egui::Vec2,
) {
    let Some(g) = geom else { return };
    let Some(dest) = outcome.dest_rect else {
        return;
    };
    if g.thumb_size.x <= 0.0 || mapping.thumb_to_doc <= f32::EPSILON {
        return;
    }
    let screen_per_doc = dest.width() / g.thumb_size.x / mapping.thumb_to_doc.max(f32::EPSILON);
    if !screen_per_doc.is_finite() || screen_per_doc <= 0.0 {
        return;
    }
    let full = (g.full_size.x as u32, g.full_size.y as u32);
    let doc_size = (g.doc_size.x as u32, g.doc_size.y as u32);
    let user_drove = doc.ui.viewport.zoom() != pre_zoom || doc.ui.viewport.offset() != pre_offset;
    let base = doc.ui.stable_view;
    // Document replaced (or first frame): re-baseline, never fight it.
    match stability_action(base, full, doc_size, screen_per_doc, user_drove) {
        StabilityAction::Rebaseline => {
            doc.ui.stable_view = Some(StableView {
                screen_per_doc,
                full,
                doc: doc_size,
            });
        }
        StabilityAction::Hold => (),
        StabilityAction::Compensate(factor) => {
            // Anchor: document center on screen (fixed point of the zoom).
            let Some(anchor) = doc_to_screen(
                (g.doc_size.x / 2.0, g.doc_size.y / 2.0),
                dest,
                g.origin,
                mapping.thumb_to_doc,
                g.thumb_size.x,
            ) else {
                // Degenerate mapping: accept the view as is.
                doc.ui.stable_view = Some(StableView {
                    screen_per_doc,
                    full,
                    doc: doc_size,
                });
                return;
            };
            // Centered widget frame (`screen = center + offset + world
            // * zoom`): `zoom_by` assumes the raw frame and drifts toward
            // the panel center (see `ViewportState::set_zoom_around`) —
            // every scope change would visibly kick the camera. Recover
            // the available center from the pre-interaction dest
            // (`dest.center = available.center + offset`) and zoom around
            // the document center instead: moving a layer never moves
            // the camera (Affinity parity).
            let available_center = dest.center() - pre_offset;
            let before = doc.ui.viewport.zoom();
            doc.ui
                .viewport
                .zoom_by_around(factor, anchor, available_center);
            if doc.ui.viewport.zoom() != before {
                // Settling takes effect next frame (repaint requested);
                // predict the baseline so float dust cannot loop.
                doc.ui.stable_view = Some(StableView {
                    screen_per_doc: screen_per_doc * factor,
                    full,
                    doc: doc_size,
                });
                doc.ui.needs_repaint = true;
            } else {
                // Clamped at the zoom bounds: accept the view as is.
                doc.ui.stable_view = Some(StableView {
                    screen_per_doc,
                    full,
                    doc: doc_size,
                });
            }
        }
    }
}
/// Node points mapped to document space (placement applied:
/// raw path points would miss the layer transform — the exact bug
/// where nodes float away from their shape).
fn placed_nodes(
    transform: &photo_engine::Transform2D,
    paths: &[photo_engine::VectorPath],
) -> Vec<NodePoint> {
    paths
        .iter()
        .enumerate()
        .flat_map(|(i, p)| path_nodes(i, p))
        .map(|mut point| {
            if let Some(doc) = photo_engine::place_vector_point(
                transform,
                paths,
                datatypes::Vec2::new(point.doc.0, point.doc.1),
            ) {
                point.doc = doc;
            }
            point
        })
        .collect()
}

/// Node editor driver (Node tool): draws anchors + handles of the
/// selected vector layer, runs node drags (live `SetVectorPaths`,
/// one history entry per drag, `Escape` = undo) and click-to-select.
#[allow(clippy::too_many_arguments)]
fn draw_node_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    response: Option<&egui::Response>,
    pointer_world: &[egui::Vec2],
    hover_pos: Option<egui::Pos2>,
    dest_rect: Option<egui::Rect>,
    geom: Option<PreviewGeom>,
    mapping: &CanvasMapping,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
    let origin = geom.map(|g| g.origin).unwrap_or(egui::Vec2::ZERO);
    let thumb_w = geom.map(|g| g.thumb_size.x).unwrap_or(0.0);
    let to_screen = |doc_pt: (f32, f32)| -> Option<egui::Pos2> {
        doc_to_screen(doc_pt, dest_rect?, origin, mapping.thumb_to_doc, thumb_w)
    };
    let press_doc = pointer_world.last().map(|v| (v.x, v.y));

    // --- Active gesture -------------------------------------------------
    if doc.ui.node_drag.is_some() {
        if escaped {
            if let Some(drag) = doc.ui.node_drag.take() {
                if drag.sent_paths != drag.base_paths {
                    actions.push(PhotoAction::Undo);
                }
                doc.ui.needs_repaint = true;
            }
            return actions;
        }
        let stopped = response.is_some_and(|r| r.drag_stopped() || r.clicked());
        if stopped {
            if let Some(drag) = doc.ui.node_drag.take() {
                actions.push(PhotoAction::SetVectorPaths {
                    layer: drag.layer,
                    paths: drag.sent_paths,
                    commit: true,
                });
                doc.ui.needs_repaint = true;
            }
            return actions;
        }
        ui.output_mut(|o| {
            o.cursor_icon = egui::CursorIcon::Grabbing;
        });
        if let Some(pointer) = press_doc
            && let Some(drag) = doc.ui.node_drag.as_mut()
        {
            let next = drag.resolve(pointer);
            // Live follow, throttled like transforms (outline tracks
            // every frame locally; worker re-renders at most ~25/s).
            let due = doc
                .ui
                .last_node_send
                .is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(40));
            if next != drag.sent_paths {
                drag.sent_paths = next.clone();
                doc.ui.needs_repaint = true;
                if due {
                    doc.ui.last_node_send = Some(std::time::Instant::now());
                    actions.push(PhotoAction::SetVectorPaths {
                        layer: drag.layer,
                        paths: next,
                        commit: false,
                    });
                }
            }
        }
        // Draw from the sent paths (no snapshot lag mid-gesture).
        if let Some(drag) = doc.ui.node_drag.as_ref() {
            let transform = find_layer_in(&doc.ui.layers, drag.layer)
                .map(|info| info.transform)
                .unwrap_or_default();
            let points = placed_nodes(&transform, &drag.sent_paths);
            let theme = ui.cygnus_theme();
            draw_node_overlay(ui.painter(), theme, &points, &to_screen, hover_pos);
        }
        return actions;
    }

    // --- Idle ------------------------------------------------------------
    // Selected vector layer with editable paths.
    let target = doc.ui.selected.and_then(|id| {
        find_layer_in(&doc.ui.layers, id).filter(|info| {
            info.kind == PhotoLayerKind::Vector && info.visible && info.paths.is_some()
        })
    });
    if let Some(info) = target {
        let paths = info.paths.clone().expect("filtered");
        let points = placed_nodes(&info.transform, &paths);
        let theme = ui.cygnus_theme();
        let hovered = draw_node_overlay(ui.painter(), theme, &points, &to_screen, hover_pos);
        if hovered.is_some() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::Grab;
            });
        }
        // Press on a node begins a drag (press-selects first, like
        // the transform overlay).
        if response.is_some_and(|r| r.drag_started())
            && let Some(grab) = press_doc
        {
            if let Some(node) = nearest_node(&points, &to_screen, grab) {
                if doc.ui.selected != Some(info.id) {
                    actions.push(PhotoAction::SelectLayer(info.id));
                }
                if let Some(drag) = NodeDrag::begin(info.id, &paths, node) {
                    doc.ui.node_drag = Some(drag);
                    doc.ui.needs_repaint = true;
                }
            } else if let Some(hit) = hit_test_topmost(&doc.ui.layers, grab)
                && doc.ui.selected != Some(hit)
            {
                actions.push(PhotoAction::SelectLayer(hit));
            }
        }
    }
    // Click (no drag): select topmost under the cursor, empty = deselect.
    if response.is_some_and(|r| r.clicked())
        && let Some(at) = hover_pos
            .and_then(|pos| screen_to_doc(pos, dest_rect?, origin, mapping.thumb_to_doc, thumb_w))
            .or(press_doc)
    {
        match hit_test_topmost(&doc.ui.layers, at) {
            Some(hit) => {
                if doc.ui.selected != Some(hit) {
                    actions.push(PhotoAction::SelectLayer(hit));
                }
            }
            None => {
                if doc.ui.selected.is_some() {
                    doc.ui.selected = None;
                    doc.ui.needs_repaint = true;
                }
            }
        }
    }
    actions
}

/// Nearest node to a document position (10 px screen radius).
fn nearest_node(
    points: &[NodePoint],
    to_screen: &dyn Fn((f32, f32)) -> Option<egui::Pos2>,
    grab: (f32, f32),
) -> Option<NodeRef> {
    let grab_screen = to_screen(grab)?;
    let mut best: Option<(f32, NodeRef)> = None;
    for point in points {
        let Some(at) = to_screen(point.doc) else {
            continue;
        };
        let dist = at.distance(grab_screen);
        if dist <= 10.0 && best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, point.node));
        }
    }
    best.map(|(_, node)| node)
}

/// Transform overlay driver (Move tool): draws the selected pixel
/// layer's frame + handles, runs handle gestures (live
/// `SetTransform`, one history entry per drag, `Escape` = undo) and
/// click-to-select (topmost layer, empty click deselects). Body drags
/// are left to the canvas legacy path (`MoveLayer` on release).
#[allow(clippy::too_many_arguments)]
fn draw_transform_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    response: Option<&egui::Response>,
    pointer_world: &[egui::Vec2],
    hover_pos: Option<egui::Pos2>,
    dest_rect: Option<egui::Rect>,
    geom: Option<PreviewGeom>,
    mapping: &CanvasMapping,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
    let origin = geom.map(|g| g.origin).unwrap_or(egui::Vec2::ZERO);
    let thumb_w = geom.map(|g| g.thumb_size.x).unwrap_or(0.0);
    let to_screen = |doc_pt: (f32, f32)| -> Option<egui::Pos2> {
        doc_to_screen(doc_pt, dest_rect?, origin, mapping.thumb_to_doc, thumb_w)
    };
    // Hover in document pixels (screen fallback: the drag packets
    // only flow while dragging).
    let hover_doc = hover_pos
        .and_then(|pos| screen_to_doc(pos, dest_rect?, origin, mapping.thumb_to_doc, thumb_w));
    let press_doc = pointer_world.last().map(|v| (v.x, v.y));

    // --- Active gesture -------------------------------------------------
    if doc.ui.transform_drag.is_some() {
        if escaped {
            // Cancel: the coalesced frames form a single entry —
            // undo only when at least one frame was applied.
            if let Some(drag) = doc.ui.transform_drag.take() {
                // Proxy-driven frames never reached the worker: only
                // live frames need undoing.
                if drag.applied {
                    actions.push(PhotoAction::Undo);
                }
                doc.ui.drag_proxy = None;
                doc.ui.stroke.clear();
                doc.ui.needs_repaint = true;
            }
            return actions;
        }
        // Release ends the drag (`drag_stopped`); a plain click can
        // never be in flight here (gestures only begin on drag).
        let stopped = response.is_some_and(|r| r.drag_stopped() || r.clicked());
        if stopped {
            if let Some(drag) = doc.ui.transform_drag.take() {
                // The overlay owns this drag end to end: drop the
                // legacy body's release commit (same gesture, one
                // history entry via the transform commit).
                actions.retain(|a| !matches!(a, PhotoAction::MoveLayer { .. }));
                actions.push(PhotoAction::SetTransform {
                    layer: drag.layer,
                    transform: drag.sent,
                    commit: true,
                });
                end_drag_proxy(doc, &drag);
                doc.ui.needs_repaint = true;
            }
            return actions;
        }
        ui.output_mut(|o| {
            o.cursor_icon = egui::CursorIcon::Grabbing;
        });
        if let Some(pointer) = press_doc
            && let Some(drag) = doc.ui.transform_drag.as_mut()
        {
            let next = drag.resolve(pointer);
            // Live follow, throttled: the outline tracks every
            // frame locally (60 fps feedback); the worker
            // re-renders at most ~25/s (its queue folds anyway, but
            // a saturated worker would lag the preview seconds
            // behind the pointer on big documents). Release always
            // sends.
            let due = doc
                .ui
                .last_transform_send
                .is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(40));
            // GPU drag proxy ready: the canvas draws the layer at
            // `drag.sent` every frame, the worker only sees the release.
            let proxied = doc
                .ui
                .drag_proxy
                .as_ref()
                .is_some_and(|proxy| proxy.drives(drag.layer));
            if next != drag.sent {
                drag.sent = next;
                doc.ui.needs_repaint = true;
                if due && !proxied {
                    doc.ui.last_transform_send = Some(std::time::Instant::now());
                    drag.applied = true;
                    actions.push(PhotoAction::SetTransform {
                        layer: drag.layer,
                        transform: next,
                        commit: false,
                    });
                }
            }
        }
        // Draw from the sent transform (no snapshot lag mid-gesture).
        if let Some(drag) = doc.ui.transform_drag.as_ref() {
            let corners = frame_corners(&drag.sent, drag.dims);
            let mut quad = [egui::Pos2::ZERO; 4];
            let mut ok = true;
            for (i, screen) in quad.iter_mut().enumerate() {
                match to_screen(corners[i]) {
                    Some(p) => *screen = p,
                    None => ok = false,
                }
            }
            if ok {
                let theme = ui.cygnus_theme();
                draw_transform_overlay(ui.painter(), theme, quad, None, true);
            }
        }
        return actions;
    }

    // --- Idle ------------------------------------------------------------
    // Overlay target: visible pixel, vector or group layer with
    // carrier dims (vectors origin-normalized, groups on a virtual
    // bounds placement — same frame visuals everywhere).
    let target = doc.ui.selected.and_then(|id| {
        find_layer_in(&doc.ui.layers, id).filter(|info| {
            matches!(
                info.kind,
                PhotoLayerKind::Pixel | PhotoLayerKind::Vector | PhotoLayerKind::Group
            ) && info.visible
                && info.dims.is_some()
        })
    });
    if let Some(info) = target {
        let transform = info.transform;
        let dims = info.dims.expect("filtered");
        let layer = info.id;
        let corners = frame_corners(&transform, dims);
        let quad = corners.map(to_screen);
        // Groups show the frame with no handles (body moves only —
        // scale/rotate of a group is a follow-up).
        let hover_for_handles = (info.kind != PhotoLayerKind::Group)
            .then_some(hover_pos)
            .flatten();
        if let [Some(a), Some(b), Some(c), Some(d)] = quad {
            let theme = ui.cygnus_theme();
            let hovered =
                draw_transform_overlay(ui.painter(), theme, [a, b, c, d], hover_for_handles, false);
            if let Some(handle) = hovered {
                ui.output_mut(|o| {
                    o.cursor_icon = handle.cursor();
                });
            } else if let Some(at) = hover_doc
                && point_in_quad(at, corners)
            {
                ui.output_mut(|o| {
                    o.cursor_icon = egui::CursorIcon::Grab;
                });
            }
            // Press: handles begin a transform gesture; anywhere
            // else on a layer begins a body move (press-selects
            // first, so the drag always owns the layer under the
            // pointer — the legacy release commit is dropped).
            if response.is_some_and(|r| r.drag_started())
                && let Some(grab) = press_doc
            {
                if let Some(handle) = hovered {
                    if let Some(drag) =
                        TransformDrag::begin(layer, dims, transform, Some(handle), grab)
                    {
                        doc.ui.transform_drag = Some(drag);
                        doc.ui.needs_repaint = true;
                    }
                } else if let Some(hit) = hit_test_topmost(&doc.ui.layers, grab) {
                    if doc.ui.selected != Some(hit) {
                        actions.push(PhotoAction::SelectLayer(hit));
                    }
                    let space = find_layer_in(&doc.ui.layers, hit).and_then(|info| {
                        (matches!(
                            info.kind,
                            PhotoLayerKind::Pixel | PhotoLayerKind::Vector | PhotoLayerKind::Group
                        ) && info.visible)
                            .then(|| info.dims.map(|d| (d, info.transform)))
                            .flatten()
                    });
                    if let Some((hdims, htransform)) = space
                        && let Some(drag) = TransformDrag::begin(hit, hdims, htransform, None, grab)
                    {
                        doc.ui.transform_drag = Some(drag);
                        doc.ui.needs_repaint = true;
                    }
                } else if info.visible
                    && let Some(drag) = TransformDrag::begin(layer, dims, transform, None, grab)
                {
                    // Empty press: legacy parity — the drag moves the
                    // selected layer (owned here, single commit).
                    doc.ui.transform_drag = Some(drag);
                    doc.ui.needs_repaint = true;
                }
            }
        }
    }
    // Click (no drag): select topmost under the cursor, empty = deselect.
    if response.is_some_and(|r| r.clicked())
        && let Some(at) = hover_doc.or(press_doc)
    {
        match hit_test_topmost(&doc.ui.layers, at) {
            Some(hit) => {
                if doc.ui.selected != Some(hit) {
                    actions.push(PhotoAction::SelectLayer(hit));
                }
            }
            None => {
                if doc.ui.selected.is_some() {
                    doc.ui.selected = None;
                    doc.ui.needs_repaint = true;
                }
            }
        }
    }
    if escaped && !doc.ui.stroke.is_empty() {
        // Cancel the uncommitted legacy body drag.
        doc.ui.stroke.clear();
        doc.ui.needs_repaint = true;
    }
    actions
}

/// Paint slots of the drag proxy: under / layer (document-clipped when
/// the preview is clipped) / over, plus the placement they share.
struct ProxySlots {
    under: egui::layers::ShapeIdx,
    layer: egui::layers::ShapeIdx,
    layer_clip: egui::Rect,
    over: egui::layers::ShapeIdx,
    mapping: crate::ui::drag_proxy::ProxyMapping,
}

/// Reserves the slots when a proxy may draw this frame (cheap no-op
/// shapes otherwise never filled).
fn reserve_proxy_slots(
    ui: &egui::Ui,
    doc: &OpenDocument,
    geom: Option<PreviewGeom>,
    dest: Option<egui::Rect>,
) -> Option<ProxySlots> {
    use crate::ui::drag_proxy::DragProxyState;
    if !matches!(
        doc.ui.drag_proxy,
        Some(DragProxyState::Ready(_) | DragProxyState::Ending(..))
    ) {
        return None;
    }
    let (geom, dest) = (geom?, dest?);
    let layer_clip = if doc.ui.preview_clip {
        geom.doc_rect(dest)
            .map_or(ui.clip_rect(), |r| r.intersect(ui.clip_rect()))
    } else {
        ui.clip_rect()
    };
    Some(ProxySlots {
        under: ui.painter().add(egui::Shape::Noop),
        layer: ui
            .painter()
            .with_clip_rect(layer_clip)
            .add(egui::Shape::Noop),
        layer_clip,
        over: ui.painter().add(egui::Shape::Noop),
        mapping: crate::ui::drag_proxy::ProxyMapping { dest, geom },
    })
}

/// Proxy lifecycle against the gesture: request the split when a drag
/// starts on a new layer, drop pending/ready states once no drag runs
/// (`Ending` lives on until the commit render, see `state.rs`).
fn sync_drag_proxy(doc: &mut OpenDocument, actions: &mut Vec<PhotoAction>) {
    use crate::ui::drag_proxy::DragProxyState;
    match (&doc.ui.transform_drag, &doc.ui.drag_proxy) {
        (Some(drag), state) => {
            let current = state
                .as_ref()
                .filter(|s| !matches!(s, DragProxyState::Ending(..)))
                .map(DragProxyState::layer);
            if current != Some(drag.layer) {
                doc.ui.drag_proxy = Some(DragProxyState::Requested(drag.layer));
                actions.push(PhotoAction::PrepareDragProxy { layer: drag.layer });
            } else if let Some(DragProxyState::Ready(proxy)) = state
                && proxy.source_dims != drag.dims
            {
                // Carrier mismatch (resampled layer): live renders.
                doc.ui.drag_proxy = Some(DragProxyState::Unavailable(drag.layer));
            }
        }
        (None, Some(DragProxyState::Ending(..))) | (None, None) => {}
        (None, Some(_)) => doc.ui.drag_proxy = None,
    }
}

/// Release: a proxy-driven drag keeps drawing its final transform until
/// the commit render lands; anything else is dropped.
fn end_drag_proxy(doc: &mut OpenDocument, drag: &crate::ui::TransformDrag) {
    use crate::ui::drag_proxy::DragProxyState;
    doc.ui.drag_proxy = match doc.ui.drag_proxy.take() {
        Some(DragProxyState::Ready(proxy))
            if proxy.layer == drag.layer && drag.sent != drag.start =>
        {
            Some(DragProxyState::Ending(proxy, drag.sent))
        }
        _ => None,
    };
}

/// Fills the reserved slots with this frame's proxy shapes.
fn paint_drag_proxy(ui: &egui::Ui, doc: &OpenDocument, ctx: &PhotoUiContext, slots: ProxySlots) {
    use crate::ui::drag_proxy::{DragProxyState, proxy_shapes};
    let (proxy, transform) = match (&doc.ui.drag_proxy, &doc.ui.transform_drag) {
        (Some(DragProxyState::Ready(proxy)), Some(drag)) if drag.layer == proxy.layer => {
            (proxy, drag.sent)
        }
        (Some(DragProxyState::Ending(proxy, transform)), _) => (proxy, *transform),
        _ => return,
    };
    let backdrop = ctx.shared.theme().colors.bg_canvas;
    let shapes = proxy_shapes(proxy, &transform, slots.mapping, backdrop);
    let painter = ui.painter();
    painter.set(slots.under, egui::Shape::Vec(shapes.under));
    painter
        .with_clip_rect(slots.layer_clip)
        .set(slots.layer, egui::Shape::Vec(shapes.layer));
    painter.set(slots.over, egui::Shape::Vec(shapes.over));
}

/// Marching ants of the gesture in progress (marquee, lasso path),
/// else of the active selection (cached outline), mapped document →
/// screen through the exact document rect. Keeps a slow repaint going
/// for the animation.
fn draw_selection(
    ui: &egui::Ui,
    doc: &OpenDocument,
    ctx: &PhotoUiContext,
    doc_rect: Option<egui::Rect>,
    geom: Option<crate::state::PreviewGeom>,
) {
    let (Some(rect), Some(geom)) = (doc_rect, geom) else {
        return;
    };
    if geom.doc_size.x <= 0.0 {
        return;
    }
    let polylines: Vec<Vec<(f32, f32)>> = if doc.ui.lasso_preview.len() >= 2 {
        let mut path = doc.ui.lasso_preview.clone();
        path.push(path[0]);
        vec![path]
    } else if let Some(marquee) = &doc.ui.selection_preview {
        marquee.outline(MARQUEE_SEGMENTS)
    } else {
        doc.ui.selection_outline.clone()
    };
    if polylines.is_empty() {
        return;
    }
    let scale = rect.width() / geom.doc_size.x;
    let screen: Vec<Vec<egui::Pos2>> = polylines
        .iter()
        .map(|line| {
            line.iter()
                .map(|(x, y)| rect.min + egui::vec2(*x, *y) * scale)
                .collect()
        })
        .collect();
    let phase = (ui.input(|input| input.time) * ANTS_SPEED) as f32;
    draw_selection_outline(ui.painter(), ctx, &screen, phase);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(ANTS_FRAME_MS));
}

/// Ellipse sampling of the marquee being dragged.
const MARQUEE_SEGMENTS: usize = 96;
/// Marching ants speed (screen points per second).
const ANTS_SPEED: f64 = 12.0;
/// Repaint period of the ants animation.
const ANTS_FRAME_MS: u64 = 120;

/// Document canvas + overlays + eyedropper (dock tab content,
/// no `CentralPanel`).
pub fn draw_canvas_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    // Tool switch mid-gesture: finalize the transform (keeps user
    // work, single history entry) instead of orphaning the drag.
    if doc.ui.tool != PhotoCanvasTool::Move
        && let Some(drag) = doc.ui.transform_drag.take()
    {
        actions.push(PhotoAction::SetTransform {
            layer: drag.layer,
            transform: drag.sent,
            commit: true,
        });
        end_drag_proxy(doc, &drag);
        doc.ui.stroke.clear();
        doc.ui.needs_repaint = true;
    }
    // Same for an in-progress node drag leaving the Node tool.
    if doc.ui.tool != PhotoCanvasTool::Node
        && let Some(drag) = doc.ui.node_drag.take()
    {
        actions.push(PhotoAction::SetVectorPaths {
            layer: drag.layer,
            paths: drag.sent_paths,
            commit: true,
        });
        doc.ui.needs_repaint = true;
    }
    // GPU frame when available (full resolution, zero-copy), else the
    // CPU preview texture.
    let texture = doc
        .ui
        .gpu_frame
        .as_ref()
        .and_then(|frame| crate::ui::gpu_display::texture_for(doc.ui.gpu_display.as_ref(), frame))
        .or_else(|| doc.ui.texture_cache.texture());
    // Infinite plane: the thumbnail may extend past the document (layer
    // moved out of frame) and be downscaled (1600 px cap) — the
    // mapping brings every gesture back to exact document pixels.
    let geom = doc.ui.preview_geom;
    let mapping = geom
        .map(|g| CanvasMapping {
            origin: g.origin,
            thumb_to_doc: g.thumb_to_doc(),
        })
        .unwrap_or_else(CanvasMapping::identity);
    // Viewport state before the canvas mutates it (user-driven
    // zoom/pan detector for the stabilizer below).
    let pre_zoom = doc.ui.viewport.zoom();
    let pre_offset = doc.ui.viewport.offset();
    let outcome = PhotoCanvas::new(&mut doc.ui.stroke)
        .texture(texture)
        .tool(doc.ui.tool)
        .show_grid(doc.ui.show_grid)
        .active_layer(doc.ui.selected)
        .brush(doc.ui.brush)
        .mapping(mapping)
        .show(ui, &mut doc.ui.viewport);
    // View stability: a scope change (layer moved/rotated out of
    // frame grows the infinite plane and refits the thumbnail) must
    // never rescale the document on screen — counter-zoom around the
    // document center instead. Skipped when the user drove zoom/pan
    // (re-baseline) or the document itself was replaced.
    stabilize_viewport(doc, &outcome, geom, &mapping, pre_zoom, pre_offset);
    // Drag proxy paint slots, reserved UNDER every overlay drawn below
    // (filled at the end of the frame, once the gesture resolved).
    let proxy_slots = reserve_proxy_slots(ui, doc, geom, outcome.dest_rect);
    doc.ui.last_dest = outcome.dest_rect;
    // Stroke commit: routed to the worker by the app.
    let committed = outcome.paint.is_some();
    if let Some(paint) = outcome.paint {
        actions.push(PhotoAction::CommitStroke(paint));
    }
    // Phase 6G.3P: interactive feedback during the drag (NO worker
    // action here — the worker only sees the commit above, unchanged;
    // thumbnails are therefore deferred to commit, §11).
    if matches!(
        doc.ui.tool,
        PhotoCanvasTool::Select | PhotoCanvasTool::Lasso | PhotoCanvasTool::Gradient
    ) {
        // The marquee anchor / lasso path lives in `stroke` across frames: only the
        // paint overlay goes (clearing `stroke` here collapsed every
        // marquee to the current frame's points).
        clear_ink_overlay(&mut doc.ui);
    } else if !ink_tool_active(doc.ui.tool) {
        // Tool switch mid-gesture: clean abort (the
        // canvas already drained `stroke`).
        clear_ink(&mut doc.ui);
    } else if committed {
        // The commit covers the gesture: the overlay did its job.
        clear_ink(&mut doc.ui);
    } else if ui.input(|input| input.key_pressed(egui::Key::Escape)) && !doc.ui.stroke.is_empty() {
        // Escape: cancels the gesture (NO commit — history and
        // thumbnails stay intact, §10).
        clear_ink(&mut doc.ui);
        doc.ui.needs_repaint = true;
    } else {
        let origin = geom.map(|g| g.origin).unwrap_or(egui::Vec2::ZERO);
        let thumb_w = geom.map(|g| g.thumb_size.x).unwrap_or(0.0);
        // Panel snapshot copied BEFORE the mutable borrow (transform +
        // gate: never guessed by the UI).
        let live = doc.ui.selected.and_then(|id| {
            doc.ui
                .layers
                .iter()
                .find(|layer| layer.id == id)
                .map(|info| (info.transform, info.overlay_live))
        });
        let mut fed = false;
        if !outcome.ink_points.is_empty()
            && let Some((transform, true)) = live
        {
            let eraser = doc.ui.tool == PhotoCanvasTool::Eraser;
            let brush = doc.ui.brush;
            let points: Vec<(f32, f32)> = outcome
                .ink_points
                .iter()
                .map(|point| (point.x, point.y))
                .collect();
            fed = feed_ink_frame(
                ui,
                &mut doc.ui,
                InkFrameInput {
                    points: &points,
                    gesture_started: outcome.ink_gesture_started,
                    eraser,
                    brush,
                    transform,
                    overlay_live: true,
                    dest: outcome.dest_rect,
                    origin,
                    thumb_to_doc: mapping.thumb_to_doc,
                    thumb_w,
                },
            )
            .is_some();
        }
        if fed {
            // New segment displayed this frame: the drag continues.
            doc.ui.needs_repaint = true;
        } else if outcome.ink_dragging
            && draw_cached_ink(
                ui,
                &doc.ui,
                outcome.dest_rect,
                origin,
                mapping.thumb_to_doc,
                thumb_w,
            )
        {
            // Drag plateau (button held, no points this frame): the
            // cached texture is re-placed at the current `dest` — visible
            // without waiting for `MouseUp`, without re-rendering (zoom/pan
            // state-only: same texture id).
            doc.ui.needs_repaint = true;
        }
    }
    // Move commit (select tool): routed to the worker.
    if let Some(moving) = outcome.move_layer {
        actions.push(PhotoAction::MoveLayer {
            layer: moving.layer,
            dx: moving.dx,
            dy: moving.dy,
        });
    }
    // Right click: view context menu (zoom, grid), rerun-style sections.
    if let Some(response) = &outcome.response {
        let theme = ctx.shared.theme();
        let catalog = ctx.shared.translator();
        let texts = PhotoCatalog::new(catalog.language());
        egui::Popup::context_menu(response)
            .style(menu_style(theme))
            .show(|ui| {
                if menu_row(ui, theme, catalog.get(cygnus_i18n::TextKey::ZoomIn)) {
                    actions.push(PhotoAction::ZoomIn);
                    ui.close();
                }
                if menu_row(ui, theme, catalog.get(cygnus_i18n::TextKey::ZoomOut)) {
                    actions.push(PhotoAction::ZoomOut);
                    ui.close();
                }
                if menu_row(ui, theme, texts.get(PhotoTextKey::Zoom100)) {
                    actions.push(PhotoAction::ZoomReset);
                    ui.close();
                }
                ui.separator();
                if menu_row(ui, theme, catalog.get(cygnus_i18n::TextKey::Grid)) {
                    actions.push(PhotoAction::ToggleGrid);
                    ui.close();
                }
                // Selected layer: Transformations submenu.
                if doc.ui.selected.is_some() {
                    ui.separator();
                    ui.menu_button(texts.get(PhotoTextKey::Transform), |ui| {
                        if menu_row(ui, theme, texts.get(PhotoTextKey::FlipHorizontal)) {
                            actions.push(PhotoAction::FlipHorizontalSelected);
                            ui.close();
                        }
                        if menu_row(ui, theme, texts.get(PhotoTextKey::FlipVertical)) {
                            actions.push(PhotoAction::FlipVerticalSelected);
                            ui.close();
                        }
                        if menu_row(ui, theme, texts.get(PhotoTextKey::RotateClockwise)) {
                            actions.push(PhotoAction::RotateClockwiseSelected);
                            ui.close();
                        }
                        if menu_row(ui, theme, texts.get(PhotoTextKey::RotateCounterclockwise)) {
                            actions.push(PhotoAction::RotateCounterclockwiseSelected);
                            ui.close();
                        }
                        if menu_row(ui, theme, texts.get(PhotoTextKey::CropToDocument)) {
                            actions.push(PhotoAction::CropSelectedToDocument);
                            ui.close();
                        }
                    });
                }
            });
    }
    // Eyedropper: the gesture is in document pixels, the sample in
    // thumbnail pixels — converted back via origin and scale.
    if doc.ui.tool == PhotoCanvasTool::Eyedropper
        && let (Some(world), Some(preview)) =
            (outcome.pointer_world.last(), doc.ui.last_preview.as_ref())
    {
        // The CPU preview's OWN geometry (the canvas may show a GPU frame
        // with another scale and origin).
        let to_thumb = if preview.full_width > 0 {
            preview.width as f32 / preview.full_width as f32
        } else {
            1.0
        };
        let sampled = sample_preview_color(
            preview,
            preview.origin_x + world.x * to_thumb,
            preview.origin_y + world.y * to_thumb,
        );
        if let Some(color) = sampled {
            doc.ui.brush.color = color;
        }
    }
    // Transform overlay (Move tool): bounding box + scale/rotate
    // handles of the selected pixel layer (handles drive live
    // `SetTransform`; body drags stay owned by the canvas legacy
    // path). Click selects the topmost layer under the cursor.
    if doc.ui.tool == PhotoCanvasTool::Move {
        actions.extend(draw_transform_content(
            ui,
            doc,
            outcome.response.as_ref(),
            &outcome.pointer_world,
            outcome.hover_pos,
            outcome.dest_rect,
            geom,
            &mapping,
        ));
    }
    sync_drag_proxy(doc, &mut actions);
    if let Some(slots) = proxy_slots {
        paint_drag_proxy(ui, doc, ctx, slots);
    }
    // Node editor (Node tool): anchors + handles of the selected
    // vector layer (body moves stay on the Move tool).
    if doc.ui.tool == PhotoCanvasTool::Node {
        actions.extend(draw_node_content(
            ui,
            doc,
            outcome.response.as_ref(),
            &outcome.pointer_world,
            outcome.hover_pos,
            outcome.dest_rect,
            geom,
            &mapping,
        ));
    }
    // Overlays: EXACT document frame inside the thumbnail (stable
    // when a layer leaves the frame or the thumbnail downscales) with NO
    // label — the resolution lives in the canvas tab title.
    // Then the brush/eraser circle.
    let doc_rect = match (geom, outcome.dest_rect) {
        (Some(g), Some(dest)) => g.doc_rect(dest),
        _ => outcome.dest_rect,
    };
    draw_document_bounds(ui.painter(), ctx, doc_rect, None);
    // Selection tool: the marquee follows the drag, its release
    // becomes the selection (a plain click deselects).
    if let Some(drag) = outcome.selection_drag {
        let marquee = photo_engine::Selection::from_drag(
            doc.ui.selection_kind,
            (drag.start.x, drag.start.y),
            (drag.end.x, drag.end.y),
        );
        if drag.finished {
            doc.ui.selection_preview = None;
            actions.push(PhotoAction::SetSelection(marquee));
        } else {
            doc.ui.selection_preview = marquee;
        }
        doc.ui.needs_repaint = true;
    } else if doc.ui.tool != PhotoCanvasTool::Select {
        doc.ui.selection_preview = None;
    }
    // Lasso: live path, closed into a selection on release.
    if let Some(lasso) = outcome.lasso {
        if lasso.finished {
            doc.ui.lasso_preview.clear();
            let size = geom.map(|g| (g.doc_size.x.max(1.0) as u32, g.doc_size.y.max(1.0) as u32));
            let selection = size.and_then(|(width, height)| {
                photo_engine::Selection::from_polygon(&lasso.points, width, height)
            });
            actions.push(PhotoAction::SetSelection(selection));
        } else {
            doc.ui.lasso_preview = lasso.points;
        }
        doc.ui.needs_repaint = true;
    } else if doc.ui.tool != PhotoCanvasTool::Lasso {
        doc.ui.lasso_preview.clear();
    }
    // Paint bucket: worker fill of the target buffer.
    if let Some(click) = outcome.fill_click {
        actions.push(PhotoAction::FillAt {
            x: click.x,
            y: click.y,
        });
    }
    // Gradient: live line, filled on release (a click without drag
    // fills nothing).
    if let Some(drag) = outcome.gradient_drag {
        let (start, end) = ((drag.start.x, drag.start.y), (drag.end.x, drag.end.y));
        if drag.finished {
            doc.ui.gradient_preview = None;
            if (drag.end - drag.start).length() >= 1.0 {
                actions.push(PhotoAction::FillGradient { start, end });
            }
        } else {
            doc.ui.gradient_preview = Some((start, end));
        }
        doc.ui.needs_repaint = true;
    } else if doc.ui.tool != PhotoCanvasTool::Gradient {
        doc.ui.gradient_preview = None;
    }
    if let (Some((start, end)), Some(rect), Some(g)) = (doc.ui.gradient_preview, doc_rect, geom)
        && g.doc_size.x > 0.0
    {
        let scale = rect.width() / g.doc_size.x;
        let to_screen = |(x, y): (f32, f32)| rect.min + egui::vec2(x, y) * scale;
        draw_gradient_line(ui.painter(), ctx, to_screen(start), to_screen(end));
    }
    // Wand: worker flood on the exact composite.
    if let Some(click) = outcome.wand_click {
        actions.push(PhotoAction::MagicWand {
            x: click.x,
            y: click.y,
        });
    }
    draw_selection(ui, doc, ctx, doc_rect, geom);
    if matches!(
        doc.ui.tool,
        PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser
    ) {
        // OS cursor kept (viewport crosshair) + circle at the
        // brush/eraser diameter on top.
        if let (Some(dest), Some(geom)) = (outcome.dest_rect, geom) {
            let to_screen = if geom.thumb_size.x > 0.0 {
                dest.width() / geom.thumb_size.x / mapping.thumb_to_doc.max(f32::EPSILON)
            } else {
                0.0
            };
            draw_brush_cursor(
                ui.painter(),
                ctx,
                Some(dest),
                outcome.hover_pos,
                doc.ui.brush.radius,
                to_screen,
            );
        }
    }
    actions
}
