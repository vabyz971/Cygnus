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
use crate::i18n::{PhotoCatalog, PhotoTextKey};
use crate::state::{OpenDocument, sample_preview_color};
use crate::ui::{
    CanvasMapping, InkFrameInput, PhotoCanvas, PhotoCanvasTool, clear_ink, draw_brush_cursor,
    draw_cached_ink, draw_document_bounds, feed_ink_frame, ink_tool_active,
};
use ui_kit::components::{menu_row, menu_style};

/// Document canvas + overlays + eyedropper (dock tab content,
/// no `CentralPanel`).
pub fn draw_canvas_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let texture = doc.ui.texture_cache.texture();
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
    let outcome = PhotoCanvas::new(&mut doc.ui.stroke)
        .texture(texture)
        .tool(doc.ui.tool)
        .show_grid(doc.ui.show_grid)
        .active_layer(doc.ui.selected)
        .brush(doc.ui.brush)
        .mapping(mapping)
        .show(ui, &mut doc.ui.viewport);
    // Stroke commit: routed to the worker by the app.
    let committed = outcome.paint.is_some();
    if let Some(paint) = outcome.paint {
        actions.push(PhotoAction::CommitStroke(paint));
    }
    // Phase 6G.3P: interactive feedback during the drag (NO worker
    // action here — the worker only sees the commit above, unchanged;
    // thumbnails are therefore deferred to commit, §11).
    if !ink_tool_active(doc.ui.tool) {
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
                if menu_row(ui, theme, catalog.get(ui_kit::i18n::TextKey::ZoomIn)) {
                    actions.push(PhotoAction::ZoomIn);
                    ui.close();
                }
                if menu_row(ui, theme, catalog.get(ui_kit::i18n::TextKey::ZoomOut)) {
                    actions.push(PhotoAction::ZoomOut);
                    ui.close();
                }
                if menu_row(ui, theme, texts.get(PhotoTextKey::Zoom100)) {
                    actions.push(PhotoAction::ZoomReset);
                    ui.close();
                }
                ui.separator();
                if menu_row(ui, theme, catalog.get(ui_kit::i18n::TextKey::Grid)) {
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
        let origin = geom.map(|g| g.origin).unwrap_or(egui::Vec2::ZERO);
        let to_thumb = 1.0 / mapping.thumb_to_doc.max(f32::EPSILON);
        let sampled = sample_preview_color(
            preview,
            origin.x + world.x * to_thumb,
            origin.y + world.y * to_thumb,
        );
        if let Some(color) = sampled {
            doc.ui.brush.color = color;
        }
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
