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
#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.

//! PHOTO canvas: generic viewport + photo tools on top.
//!
//! Domain widget: displays the `photo-engine` texture via
//! `cygnus_ui::Viewport`, maps the photo tools (hand, zoom, brush,
//! eraser, move) and accumulates drawing strokes. At the end
//! of a stroke (release), the request is RETURNED in
//! [`PhotoCanvasOutcome::paint`]: the app sends it to the worker
//! (via [`PhotoAction`](crate::commands::PhotoAction)), never the
//! canvas. Pan/zoom stay 100% local (state-only).

use cygnus_ui::theme::UiThemeExt;
use cygnus_ui::viewport::{Viewport, ViewportAction, ViewportState, ViewportTool};
use uuid::Uuid;

/// Active photo canvas tool (egui-side, no iced dependency).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoCanvasTool {
    /// Move / select.
    #[default]
    Move,
    /// Node editor (vector paths: anchors + handles).
    Node,
    /// Marquee selection (rectangle / ellipse): drag selects, click
    /// deselects.
    Select,
    /// Freehand (lasso) selection: the drag path, closed on release.
    Lasso,
    /// Magic wand: click selects similar colors.
    Wand,
    /// Paint bucket: click fills the similar-color area.
    Bucket,
    /// Gradient: drag from the primary to the secondary color.
    Gradient,
    /// Clone stamp: Alt+click sets the source, dragging copies from it.
    Clone,
    /// Polygonal (pen) selection: clicks add corners, closing selects.
    PenSelect,
    /// Hand: dragging moves the view.
    Pan,
    /// Zoom: click = zoom in, right-click = out.
    Zoom,
    /// Brush: dragging accumulates a stroke sent to the engine.
    Brush,
    /// Eraser: like the brush, in erase mode.
    Eraser,
    /// Eyedropper: clicking samples the color (composite preview).
    Eyedropper,
}

impl PhotoCanvasTool {
    /// Matching generic viewport tool.
    pub fn viewport_tool(self) -> ViewportTool {
        match self {
            Self::Move | Self::Node => ViewportTool::Move,
            Self::Pan => ViewportTool::Pan,
            Self::Zoom => ViewportTool::Zoom,
            Self::Brush
            | Self::Eraser
            | Self::Eyedropper
            | Self::Select
            | Self::Lasso
            | Self::Wand
            | Self::Bucket
            | Self::Gradient
            | Self::Clone
            | Self::PenSelect => ViewportTool::Brush,
        }
    }

    /// True for stroke-accumulating tools (brush, eraser).
    pub fn is_painting(self) -> bool {
        matches!(self, Self::Brush | Self::Eraser | Self::Clone)
    }

    /// Contextual help shown in the status bar (Affinity-style).
    pub fn hint(self, texts: cygnus_i18n::PhotoCatalog) -> &'static str {
        use cygnus_i18n::PhotoTextKey;
        match self {
            Self::Move => texts.get(PhotoTextKey::MoveHintText),
            Self::Node => texts.get(PhotoTextKey::NodeHintText),
            Self::Select => texts.get(PhotoTextKey::SelectHintText),
            Self::Lasso => texts.get(PhotoTextKey::LassoHintText),
            Self::Wand => texts.get(PhotoTextKey::WandHintText),
            Self::Bucket => texts.get(PhotoTextKey::BucketHintText),
            Self::Gradient => texts.get(PhotoTextKey::GradientHintText),
            Self::Clone => texts.get(PhotoTextKey::CloneHintText),
            Self::PenSelect => texts.get(PhotoTextKey::PenSelectHintText),
            Self::Pan => texts.get(PhotoTextKey::PanHintText),
            Self::Zoom => texts.get(PhotoTextKey::ZoomHintText),
            Self::Brush => texts.get(PhotoTextKey::BrushHintText),
            Self::Eraser => texts.get(PhotoTextKey::EraserHintText),
            Self::Eyedropper => texts.get(PhotoTextKey::EyedropperHintText),
        }
    }
}

/// Brush/eraser settings (owned by the app).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotoBrushSettings {
    /// Radius in image pixels.
    pub radius: f32,
    /// RGB color (ignored in eraser mode).
    pub color: [u8; 3],
    /// Opacity 0..=1.
    pub opacity: f32,
    /// Secondary RGB color (rail swatch, swapped with `color`).
    pub secondary: [u8; 3],
    /// Tip hardness 0..=1 (1 = hard edge).
    pub hardness: f32,
    /// Per-dab flow 0..=1 (1 = no build-up).
    pub flow: f32,
    /// Dab spacing, fraction of the diameter.
    pub spacing: f32,
    /// Stabilizer strength 0..=1 (0 = raw pointer).
    pub stabilizer: f32,
}

/// Strongest stabilizer smoothing (a full 1.0 would freeze the stroke).
const MAX_STABILIZER: f32 = 0.95;

impl PhotoBrushSettings {
    /// Engine tip dynamics of these settings.
    #[must_use]
    pub fn dynamics(&self) -> photo_engine::paint::BrushDynamics {
        photo_engine::paint::BrushDynamics {
            hardness: self.hardness,
            flow: self.flow,
            spacing: self.spacing,
        }
    }
}

/// Stabilizer ("lazy mouse"): each new point only moves part of the way
/// from the previous smoothed point, `strength` 0 (raw) to
/// [`MAX_STABILIZER`]. `previous` = last smoothed point of the gesture.
fn stabilize(points: &mut [egui::Vec2], previous: Option<egui::Vec2>, strength: f32) {
    let keep = strength.clamp(0.0, MAX_STABILIZER);
    if keep <= 0.0 {
        return;
    }
    let mut prev = previous;
    for point in points.iter_mut() {
        if let Some(q) = prev {
            *point = q + (*point - q) * (1.0 - keep);
        }
        prev = Some(*point);
    }
}

impl Default for PhotoBrushSettings {
    fn default() -> Self {
        Self {
            radius: 8.0,
            color: [255, 255, 255],
            opacity: 1.0,
            secondary: [0, 0, 0],
            hardness: 1.0,
            flow: 1.0,
            spacing: photo_engine::paint::LEGACY_SPACING,
            stabilizer: 0.0,
        }
    }
}

/// Stroke commit request (returned to the app, sent to the
/// worker via `PhotoAction::CommitStroke`).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintRequest {
    /// Target pixel layer.
    pub layer: Uuid,
    /// Stroke polyline (image pixels).
    pub points: Vec<(f32, f32)>,
    /// True = eraser (lowers alpha), false = brush.
    pub eraser: bool,
    /// Radius in image pixels.
    pub radius: f32,
    /// RGB color (ignored in eraser mode).
    pub color: [u8; 3],
    /// Stroke opacity 0..=1.
    pub opacity: f32,
}

/// Layer move committed on drag release (select
/// tool, returned to the app then sent to the worker via
/// `PhotoAction::MoveLayer`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveRequest {
    /// Target pixel layer.
    pub layer: Uuid,
    /// Horizontal offset in image pixels.
    pub dx: f32,
    /// Vertical offset in image pixels.
    pub dy: f32,
}

/// Thumbnail→document-pixel mapping for an infinite-plane
/// preview: the displayed thumbnail may be bigger than the
/// document (layer moved out of frame) and downscaled (1600 px cap).
/// Without a mapping (default), the thumbnail equals the document (identity).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CanvasMapping {
    /// Document top-left corner in thumbnail pixels.
    pub origin: egui::Vec2,
    /// Thumbnail→document-pixel factor (uniform thumbnail).
    pub thumb_to_doc: f32,
}

impl CanvasMapping {
    /// Identity mapping (thumbnail = document).
    pub fn identity() -> Self {
        Self {
            origin: egui::Vec2::ZERO,
            thumb_to_doc: 1.0,
        }
    }

    /// Converts a thumbnail point to document pixels.
    fn to_doc(self, thumb: egui::Vec2) -> egui::Vec2 {
        let k = if self.thumb_to_doc > 0.0 && self.thumb_to_doc.is_finite() {
            self.thumb_to_doc
        } else {
            1.0
        };
        egui::vec2((thumb.x - self.origin.x) * k, (thumb.y - self.origin.y) * k)
    }

    /// Converts a thumbnail delta to a document delta (origin
    /// cancels out on differences).
    fn delta_to_doc(self, delta_thumb: egui::Vec2) -> egui::Vec2 {
        let k = if self.thumb_to_doc > 0.0 && self.thumb_to_doc.is_finite() {
            self.thumb_to_doc
        } else {
            1.0
        };
        delta_thumb * k
    }
}

/// Photo canvas (builder): engine texture + tools on top.
///
/// `stroke` accumulates the in-progress gesture positions (brush
/// stroke or move drag, in image pixels), owned by
/// the app and drained on each commit.
pub struct PhotoCanvas<'a> {
    texture: Option<(egui::TextureId, egui::Vec2)>,
    tool: PhotoCanvasTool,
    show_grid: bool,
    active_layer: Option<Uuid>,
    brush: PhotoBrushSettings,
    mapping: CanvasMapping,
    stroke: &'a mut Vec<egui::Vec2>,
}

impl<'a> PhotoCanvas<'a> {
    /// Creates a photo canvas (app-owned stroke).
    pub fn new(stroke: &'a mut Vec<egui::Vec2>) -> Self {
        Self {
            texture: None,
            tool: PhotoCanvasTool::default(),
            show_grid: false,
            active_layer: None,
            brush: PhotoBrushSettings::default(),
            mapping: CanvasMapping::identity(),
            stroke,
        }
    }

    /// Engine texture + image size (pixels). `None` = empty state.
    #[must_use]
    pub fn texture(mut self, texture: Option<(egui::TextureId, egui::Vec2)>) -> Self {
        self.texture = texture;
        self
    }

    /// Active tool.
    #[must_use]
    pub fn tool(mut self, tool: PhotoCanvasTool) -> Self {
        self.tool = tool;
        self
    }

    /// Shows/hides the grid.
    #[must_use]
    pub fn show_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    /// Pixel layer receiving strokes (`None` = drawing disabled).
    #[must_use]
    pub fn active_layer(mut self, layer: Option<Uuid>) -> Self {
        self.active_layer = layer;
        self
    }

    /// Brush/eraser settings.
    #[must_use]
    pub fn brush(mut self, brush: PhotoBrushSettings) -> Self {
        self.brush = brush;
        self
    }

    /// Thumbnail→document-pixel mapping (infinite plane +
    /// capped thumbnail). Identity by default.
    #[must_use]
    pub fn mapping(mut self, mapping: CanvasMapping) -> Self {
        self.mapping = mapping;
        self
    }

    /// Shows the canvas, applies pan/zoom, accumulates then commits
    /// strokes (brush/eraser) or moves (select). Reported
    /// positions are in IMAGE PIXELS (accounting for fit,
    /// zoom, and pan via `dest_rect`) — directly
    /// usable by the worker (identity transform).
    pub fn show(self, ui: &mut egui::Ui, state: &mut ViewportState) -> PhotoCanvasOutcome {
        let (texture_id, image_size) = self.texture.unzip();
        // Destination rect BEFORE interaction (same geometry
        // as drawing: current available + zoom/pan).
        let dest = image_size.and_then(|size| {
            (size.x > 0.0 && size.y > 0.0).then(|| {
                Viewport::fit_dest_with_margin(
                    ui.available_rect_before_wrap(),
                    size,
                    state.zoom(),
                    state.offset(),
                    ui.cygnus_theme().spacing.xl,
                )
            })?
        });
        let response = Viewport::new()
            .texture(texture_id)
            .image_size(image_size.unwrap_or(egui::Vec2::ZERO))
            .tool(self.tool.viewport_tool())
            .show_grid(self.show_grid)
            .show(ui, state);

        let mut outcome = PhotoCanvasOutcome {
            response: Some(response.response.clone()),
            dest_rect: dest,
            image_size: image_size.unwrap_or(egui::Vec2::ZERO),
            hover_pos: response.response.hover_pos(),
            ..Default::default()
        };
        // Screen → thumbnail → document pixels (infinite-plane origin
        // + thumbnail scale inverted). Without an image (dest None),
        // positions are ignored.
        if let Some(dest) = dest {
            let scale = dest.width() / image_size.map_or(1.0, |size| size.x.max(1.0));
            if scale > 0.0 {
                for action in &response.actions {
                    if let ViewportAction::PointerAtWorld(world) = action {
                        let screen = state.world_to_screen(*world);
                        let thumb = egui::vec2(
                            (screen.x - dest.min.x) / scale,
                            (screen.y - dest.min.y) / scale,
                        );
                        outcome.pointer_world.push(self.mapping.to_doc(thumb));
                    }
                }
            }
        }
        // Move tool: accumulates the drag in image pixels and
        // commits the total offset on release (or click: zero
        // offset, just clears — never leaves an orphaned stroke).
        if self.tool == PhotoCanvasTool::Move {
            self.stroke.extend(outcome.pointer_world.iter().copied());
            if (response.response.drag_stopped() || response.response.clicked())
                && !self.stroke.is_empty()
            {
                let first = self.stroke.first().copied().unwrap_or(egui::Vec2::ZERO);
                let last = self.stroke.last().copied().unwrap_or(first);
                self.stroke.clear();
                let (dx, dy) = (last.x - first.x, last.y - first.y);
                if let Some(layer) = self.active_layer
                    && dx.is_finite()
                    && dy.is_finite()
                    && (dx != 0.0 || dy != 0.0)
                {
                    outcome.move_layer = Some(MoveRequest { layer, dx, dy });
                }
            }
            return outcome;
        }
        // Selection tool: reports the marquee corners (document pixels)
        // every frame of the gesture; the app builds the shape.
        // Lasso: the whole drag path, reported every frame.
        if self.tool == PhotoCanvasTool::Lasso {
            if ui.input(|input| input.pointer.primary_pressed()) {
                self.stroke.clear();
            }
            self.stroke.extend(outcome.pointer_world.iter().copied());
            if !self.stroke.is_empty() {
                let finished = response.response.drag_stopped() || response.response.clicked();
                outcome.lasso = Some(LassoPath {
                    points: self.stroke.iter().map(|v| (v.x, v.y)).collect(),
                    finished,
                });
                if finished {
                    self.stroke.clear();
                }
            }
            return outcome;
        }
        // Pen selection: each click reports a corner (double-click
        // closes); the path itself lives in the document state.
        if self.tool == PhotoCanvasTool::PenSelect {
            self.stroke.clear();
            if response.response.double_clicked() {
                outcome.pen_click = outcome.pointer_world.last().copied().map(|p| (p, true));
            } else if response.response.clicked() {
                outcome.pen_click = outcome.pointer_world.last().copied().map(|p| (p, false));
            }
            return outcome;
        }
        // Wand / bucket: one click = one flood request.
        if matches!(self.tool, PhotoCanvasTool::Wand | PhotoCanvasTool::Bucket) {
            self.stroke.clear();
            if response.response.clicked() {
                let click = outcome.pointer_world.last().copied();
                if self.tool == PhotoCanvasTool::Wand {
                    outcome.wand_click = click;
                } else if self.active_layer.is_some() {
                    outcome.fill_click = click;
                }
            }
            return outcome;
        }
        // Gradient: start/end line, like the marquee.
        if self.tool == PhotoCanvasTool::Gradient {
            if ui.input(|input| input.pointer.primary_pressed()) {
                self.stroke.clear();
            }
            self.stroke.extend(outcome.pointer_world.iter().copied());
            if let (Some(start), Some(end)) = (self.stroke.first(), self.stroke.last()) {
                let finished = response.response.drag_stopped() || response.response.clicked();
                outcome.gradient_drag = Some(SelectionDrag {
                    start: *start,
                    end: *end,
                    finished,
                });
                if finished {
                    self.stroke.clear();
                }
            }
            return outcome;
        }
        if self.tool == PhotoCanvasTool::Select {
            // A new press starts a fresh marquee (never leftovers of
            // another tool's gesture).
            if ui.input(|input| input.pointer.primary_pressed()) {
                self.stroke.clear();
            }
            self.stroke.extend(outcome.pointer_world.iter().copied());
            if let (Some(start), Some(end)) = (self.stroke.first(), self.stroke.last()) {
                let finished = response.response.drag_stopped() || response.response.clicked();
                outcome.selection_drag = Some(SelectionDrag {
                    start: *start,
                    end: *end,
                    finished,
                });
                if finished {
                    self.stroke.clear();
                }
            }
            return outcome;
        }
        // Clone stamp: Alt+click picks the source instead of painting.
        if self.tool == PhotoCanvasTool::Clone && ui.input(|input| input.modifiers.alt) {
            self.stroke.clear();
            if response.response.clicked() {
                outcome.clone_source = outcome.pointer_world.last().copied();
            }
            return outcome;
        }
        if !self.tool.is_painting() {
            // Mid-gesture tool switch: clean abort.
            // (The eyedropper just reports the position.)
            self.stroke.clear();
            return outcome;
        }
        if self.active_layer.is_none() {
            // No target layer: no gesture possible — no overlay, no
            // commit, no orphaned point (clicking without a layer is a
            // silent no-op, never a one-frame flash).
            self.stroke.clear();
            outcome.pointer_world.clear();
            return outcome;
        }
        // Phase 6G.3P: the gesture is reported incrementally for
        // the transient overlay (the app feeds `InteractionOverlay`,
        // never the worker before commit — see `PhotoCanvasOutcome`).
        // Stabilizer: the smoothed points feed both the live overlay and
        // the commit (same gesture on screen and in the document).
        stabilize(
            &mut outcome.pointer_world,
            self.stroke.last().copied(),
            self.brush.stabilizer,
        );
        outcome.ink_gesture_started = self.stroke.is_empty() && !outcome.pointer_world.is_empty();
        outcome.ink_points = outcome.pointer_world.clone();
        outcome.ink_dragging = response.response.dragged_by(egui::PointerButton::Primary);
        self.stroke.extend(outcome.pointer_world.iter().copied());
        // A plain click (press + release without a drag) commits a
        // single dab: `drag_stopped()` alone is false without movement,
        // which used to flash the overlay for one frame and orphan the
        // point into the next stroke.
        if (response.response.drag_stopped() || response.response.clicked())
            && !self.stroke.is_empty()
        {
            let points: Vec<(f32, f32)> = self.stroke.iter().map(|v| (v.x, v.y)).collect();
            self.stroke.clear();
            if let Some(layer) = self.active_layer {
                outcome.paint = Some(PaintRequest {
                    layer,
                    points,
                    eraser: self.tool == PhotoCanvasTool::Eraser,
                    radius: self.brush.radius,
                    color: self.brush.color,
                    opacity: self.brush.opacity,
                });
            }
        }
        outcome
    }
}

/// In-progress marquee of the selection tool (document pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionDrag {
    /// Press position.
    pub start: egui::Vec2,
    /// Current (or release) position.
    pub end: egui::Vec2,
    /// True on release: the marquee becomes the selection.
    pub finished: bool,
}

/// Lasso path of the current gesture (document pixels).
#[derive(Debug, Clone, PartialEq)]
pub struct LassoPath {
    /// Path so far (closed implicitly).
    pub points: Vec<(f32, f32)>,
    /// True on release: the path becomes the selection.
    pub finished: bool,
}

/// Result of [`PhotoCanvas::show`].
#[derive(Debug, Clone, Default)]
pub struct PhotoCanvasOutcome {
    /// Lasso gesture (`None` = no lasso this frame).
    pub lasso: Option<LassoPath>,
    /// Magic wand click in document pixels.
    pub wand_click: Option<egui::Vec2>,
    /// Paint bucket click in document pixels (active layer only).
    pub fill_click: Option<egui::Vec2>,
    /// Gradient line of the current gesture (document pixels).
    pub gradient_drag: Option<SelectionDrag>,
    /// Clone stamp source picked this frame (Alt+click, document pixels).
    pub clone_source: Option<egui::Vec2>,
    /// Pen selection click (document pixels, true = double-click).
    pub pen_click: Option<(egui::Vec2, bool)>,
    /// Selection tool gesture (`None` = no marquee this frame).
    pub selection_drag: Option<SelectionDrag>,
    /// Stroke to commit (`None` = nothing this frame; the app sends it).
    pub paint: Option<PaintRequest>,
    /// Move to commit (`None` = nothing this frame; the app sends it).
    pub move_layer: Option<MoveRequest>,
    /// Pointer positions in image pixels (active-tool click/drag).
    pub pointer_world: Vec<egui::Vec2>,
    /// Phase 6G.3P: points added THIS frame for the transient overlay
    /// (== `pointer_world` for paint tools, empty otherwise).
    /// The app pours them into `InteractionOverlay`; the worker only sees
    /// the commit (`paint`, unchanged).
    pub ink_points: Vec<egui::Vec2>,
    /// True while a primary drag is in progress (uncommitted gesture).
    pub ink_dragging: bool,
    /// True when the gesture starts this frame (empty `stroke` + points).
    pub ink_gesture_started: bool,
    /// Document screen rect (`None` = no image).
    pub dest_rect: Option<egui::Rect>,
    /// Image size in pixels (zero = no image).
    pub image_size: egui::Vec2,
    /// Cursor screen position (`None` = off-canvas).
    pub hover_pos: Option<egui::Pos2>,
    /// Viewport egui response (context menus, hover).
    pub response: Option<egui::Response>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::PhotoEngineCommand;

    #[test]
    fn select_tool_reports_the_marquee_then_finishes_on_release() {
        use std::cell::RefCell;

        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let mut state = ViewportState::default();
        let mut stroke: Vec<egui::Vec2> = Vec::new();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let last = RefCell::new(PhotoCanvasOutcome::default());
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 600.0))))
                        .tool(PhotoCanvasTool::Select)
                        .show(ui, &mut state);
                    *last.borrow_mut() = outcome;
                });
            })
            .drop_without_applying_deltas();
            last.borrow().clone()
        };
        let out = frame(vec![]);
        assert!(out.selection_drag.is_none(), "no gesture, no marquee");
        let center = out.dest_rect.expect("dest").center();
        let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        let a = center;
        let b = center + egui::vec2(60.0, 40.0);
        frame(vec![egui::Event::PointerMoved(a), button(a, true)]);
        let mut dragging = None;
        for step in 1..=4 {
            let p = a + (b - a) * (step as f32 / 4.0);
            if let Some(drag) = frame(vec![egui::Event::PointerMoved(p)]).selection_drag {
                dragging = Some(drag);
            }
        }
        let dragging = dragging.expect("marquee while dragging");
        assert!(!dragging.finished);
        let released = frame(vec![egui::Event::PointerMoved(b), button(b, false)])
            .selection_drag
            .expect("marquee on release");
        assert!(released.finished);
        assert!(released.end.x > released.start.x && released.end.y > released.start.y);
        assert!(stroke.is_empty(), "gesture drained on release");
    }

    #[test]
    fn stabilizer_pulls_points_toward_the_previous_one() {
        let mut points = vec![egui::vec2(10.0, 0.0), egui::vec2(20.0, 0.0)];
        stabilize(&mut points, Some(egui::Vec2::ZERO), 0.5);
        assert_eq!(points[0], egui::vec2(5.0, 0.0));
        assert_eq!(points[1], egui::vec2(12.5, 0.0));
        let mut raw = vec![egui::vec2(10.0, 0.0)];
        stabilize(&mut raw, Some(egui::Vec2::ZERO), 0.0);
        assert_eq!(raw[0], egui::vec2(10.0, 0.0), "0 = raw pointer");
        let mut first = vec![egui::vec2(3.0, 4.0)];
        stabilize(&mut first, None, 0.9);
        assert_eq!(
            first[0],
            egui::vec2(3.0, 4.0),
            "the gesture starts at the cursor"
        );
    }

    #[test]
    fn pointer_maps_through_capped_thumbnail() {
        // Mismatched import case: 1920×1080 document shown through an
        // 800×450 capped thumbnail (k = 2.4). Clicking the dest center
        // must report the document center (960, 540) — not thumbnail
        // pixels, not a shifted position.
        use std::cell::RefCell;

        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let mut state = ViewportState::default();
        let mut stroke: Vec<egui::Vec2> = Vec::new();
        let layer = uuid::Uuid::new_v4();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let last = RefCell::new(PhotoCanvasOutcome::default());
        let mapping = CanvasMapping {
            origin: egui::Vec2::ZERO,
            thumb_to_doc: 2.4,
        };
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 450.0))))
                        .tool(PhotoCanvasTool::Brush)
                        .active_layer(Some(layer))
                        .mapping(mapping)
                        .show(ui, &mut state);
                    *last.borrow_mut() = outcome;
                });
            })
            .drop_without_applying_deltas();
            last.borrow().clone()
        };
        let out = frame(vec![]);
        let dest = out.dest_rect.expect("dest");
        let center = dest.center();
        let press = egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let _ = frame(vec![press]);
        // Move beyond the click threshold so egui counts a drag.
        let at = center + egui::vec2(30.0, 20.0);
        let out = frame(vec![egui::Event::PointerMoved(at)]);
        assert!(!out.pointer_world.is_empty(), "drag reports a position");
        // Hand-derived expectation (independent arithmetic): dest is
        // 784 wide for the 800 px thumbnail (scale 0.98), k = 2.4.
        // thumb = ((430-8)/0.98, (320-79.5)/0.98) ≈ (430.6, 245.4),
        // doc = thumb × 2.4 ≈ (1033, 589).
        for p in &out.pointer_world {
            assert!(
                (p.x - 1033.0).abs() < 6.0 && (p.y - 589.0).abs() < 6.0,
                "mapped near the aimed document point, got ({}, {})",
                p.x,
                p.y
            );
        }
    }

    #[test]
    fn move_drag_commits_offset_on_release() {
        // Legacy body path: press, drag, release with the Move tool
        // commits the accumulated document-space offset (owned by
        // the overlay at release, same values).
        use std::cell::RefCell;

        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let mut state = ViewportState::default();
        let mut stroke: Vec<egui::Vec2> = Vec::new();
        let layer = uuid::Uuid::new_v4();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let last = RefCell::new(PhotoCanvasOutcome::default());
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 450.0))))
                        .tool(PhotoCanvasTool::Move)
                        .active_layer(Some(layer))
                        .mapping(CanvasMapping::identity())
                        .show(ui, &mut state);
                    *last.borrow_mut() = outcome;
                });
            })
            .drop_without_applying_deltas();
            last.borrow().clone()
        };
        let out = frame(vec![]);
        let center = out.dest_rect.expect("dest").center();
        let press = egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let _ = frame(vec![press]);
        // Drag beyond the click threshold, then release.
        let at = center + egui::vec2(30.0, 20.0);
        // Multi-frame drag (production-like gesture packets).
        let mid = frame(vec![egui::Event::PointerMoved(at)]);
        assert!(mid.move_layer.is_none(), "no commit mid-drag");
        assert!(!mid.pointer_world.is_empty(), "drag reports positions");
        let at2 = center + egui::vec2(60.0, 40.0);
        let mid2 = frame(vec![egui::Event::PointerMoved(at2)]);
        assert!(mid2.move_layer.is_none(), "no commit mid-drag");
        let release = egui::Event::PointerButton {
            pos: at2,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };
        let out = frame(vec![release]);
        let commit = out.move_layer.expect("release commits the move");
        assert_eq!(commit.layer, layer);
        // Identity mapping, dest scale 0.98: doc delta ≈ screen delta.
        // (The press packet carries no position: the stroke spans the
        // moved packets only.)
        assert!(
            (commit.dx - 30.0 / 0.98).abs() < 2.0 && (commit.dy - 20.0 / 0.98).abs() < 2.0,
            "offset follows the drag: ({}, {})",
            commit.dx,
            commit.dy
        );
    }

    #[test]
    fn canvas_mapping_miniature_vers_document() {
        // 2x thumbnail (downscaled infinite plane) with a (20, 10) thumbnail-px origin.
        let mapping = CanvasMapping {
            origin: egui::vec2(20.0, 10.0),
            thumb_to_doc: 2.0,
        };
        let doc = mapping.to_doc(egui::vec2(24.0, 15.0));
        assert!((doc.x - 8.0).abs() < 1e-4);
        assert!((doc.y - 10.0).abs() < 1e-4);
        // Deltas ignore the origin.
        let delta = mapping.delta_to_doc(egui::vec2(3.0, -4.0));
        assert!((delta.x - 6.0).abs() < 1e-4);
        assert!((delta.y + 8.0).abs() < 1e-4);
        // Identity by default.
        let id = CanvasMapping::identity();
        let same = id.to_doc(egui::vec2(5.0, 7.0));
        assert!((same.x - 5.0).abs() < 1e-4);
        assert!((same.y - 7.0).abs() < 1e-4);
    }

    #[test]
    fn tool_mapping_and_painting_flags() {
        assert_eq!(PhotoCanvasTool::Move.viewport_tool(), ViewportTool::Move);
        assert_eq!(PhotoCanvasTool::Pan.viewport_tool(), ViewportTool::Pan);
        assert_eq!(PhotoCanvasTool::Zoom.viewport_tool(), ViewportTool::Zoom);
        assert_eq!(PhotoCanvasTool::Brush.viewport_tool(), ViewportTool::Brush);
        assert_eq!(PhotoCanvasTool::Eraser.viewport_tool(), ViewportTool::Brush);
        assert_eq!(
            PhotoCanvasTool::Eyedropper.viewport_tool(),
            ViewportTool::Brush
        );
        assert!(PhotoCanvasTool::Brush.is_painting());
        assert!(PhotoCanvasTool::Eraser.is_painting());
        assert!(!PhotoCanvasTool::Eyedropper.is_painting());
        assert!(!PhotoCanvasTool::Move.is_painting());
        assert!(!PhotoCanvasTool::Pan.is_painting());
        assert!(!PhotoCanvasTool::Zoom.is_painting());
    }

    #[test]
    fn drag_rapporte_ink_avant_commit_puis_commit_au_relachement() {
        // MouseMove→MouseUp contract: during the drag, `paint` is None
        // (NO commit, NO worker) and `ink_points` is non-empty
        // (live feedback); on release, `paint` carries the whole gesture.
        use std::cell::{Cell, RefCell};

        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let mut state = ViewportState::default();
        let mut stroke: Vec<egui::Vec2> = Vec::new();
        let layer = uuid::Uuid::new_v4();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let center = Cell::new(egui::pos2(400.0, 300.0));
        let last = RefCell::new(PhotoCanvasOutcome::default());

        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 600.0))))
                        .tool(PhotoCanvasTool::Brush)
                        .active_layer(Some(layer))
                        .show(ui, &mut state);
                    *last.borrow_mut() = outcome;
                });
            })
            .drop_without_applying_deltas();
            last.borrow().clone()
        };

        // Frame 0: no interaction — geometry learned, nothing else.
        let out = frame(vec![]);
        assert!(out.paint.is_none());
        assert!(out.ink_points.is_empty());
        if let Some(dest) = out.dest_rect {
            center.set(dest.center());
        }
        let c = center.get();
        // Frames 1..=4: pressed then moved — feedback, never a commit.
        let moved = |p: egui::Pos2| egui::Event::PointerMoved(p);
        let press = |p: egui::Pos2| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let release = |p: egui::Pos2| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };
        let p0 = c;
        let pts = [
            p0,
            c + egui::vec2(20.0, 5.0),
            c + egui::vec2(40.0, 15.0),
            c + egui::vec2(60.0, 10.0),
        ];
        let out = frame(vec![moved(p0), press(p0)]);
        assert!(out.paint.is_none(), "press seul : aucun commit");
        let mut saw_ink = false;
        for p in pts.iter().skip(1) {
            let out = frame(vec![moved(*p)]);
            assert!(out.paint.is_none(), "pendant le drag : aucun commit");
            if !out.ink_points.is_empty() {
                saw_ink = true;
            }
        }
        assert!(saw_ink, "feedback visible AVANT le MouseUp");
        // Release: the full gesture is committed in ONE go.
        let out = frame(vec![moved(pts[3]), release(pts[3])]);
        let paint = out.paint.expect("commit au MouseUp");
        assert_eq!(paint.layer, layer);
        assert!(paint.points.len() >= 3, "tout le drag commis");
        assert!(stroke.is_empty(), "points vidés au commit");
        assert!(!paint.eraser);
    }

    #[test]
    fn click_simple_commit_un_point_sans_orphelin() {
        // Clic sans drag (appui + relâchement sans mouvement) : commet
        // un point unique (dab), sans laisser de point orphelin — avant,
        // `drag_stopped()` seul restait faux, l'overlay flashait une
        // frame et le point polluait le geste suivant.
        use std::cell::RefCell;

        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let mut state = ViewportState::default();
        let mut stroke: Vec<egui::Vec2> = Vec::new();
        let layer = uuid::Uuid::new_v4();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let last = RefCell::new(PhotoCanvasOutcome::default());
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 600.0))))
                        .tool(PhotoCanvasTool::Brush)
                        .active_layer(Some(layer))
                        .show(ui, &mut state);
                    *last.borrow_mut() = outcome;
                });
            })
            .drop_without_applying_deltas();
            last.borrow().clone()
        };
        let click_at = egui::pos2(400.0, 300.0);
        let press = egui::Event::PointerButton {
            pos: click_at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let release = egui::Event::PointerButton {
            pos: click_at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };
        let _ = frame(vec![]);
        let out = frame(vec![press]);
        assert!(out.paint.is_none(), "appui seul : aucun commit");
        let out = frame(vec![release]);
        let paint = out.paint.expect("le clic commet un point");
        assert_eq!(paint.layer, layer);
        assert!(!paint.points.is_empty(), "au moins le point cliqué");
        // Le geste suivant repart à zéro (pas de segment parasite vers
        // l'ancien clic) : premier mouvement = nouveau geste.
        let elsewhere = egui::pos2(100.0, 100.0);
        let press_there = egui::Event::PointerButton {
            pos: elsewhere,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let moved = egui::Event::PointerMoved(elsewhere + egui::vec2(30.0, 10.0));
        let _ = frame(vec![press_there]);
        let out = frame(vec![moved]);
        let fresh_gesture = out.ink_gesture_started;
        let release_there = egui::Event::PointerButton {
            pos: elsewhere + egui::vec2(30.0, 10.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };
        let out = frame(vec![release_there]);
        assert!(out.paint.is_some(), "le drag suivant commet");
        assert!(stroke.is_empty(), "aucun point orphelin");
        assert!(fresh_gesture, "nouveau geste après le clic (stroke vidé)");
    }

    #[test]
    fn click_sans_calque_reste_silencieux() {
        // Pinceau sans calque sélectionné : ni overlay, ni commit, ni
        // orphelin (pas de flash d'une frame).
        use std::cell::RefCell;

        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let mut state = ViewportState::default();
        let mut stroke: Vec<egui::Vec2> = Vec::new();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let last = RefCell::new(PhotoCanvasOutcome::default());
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 600.0))))
                        .tool(PhotoCanvasTool::Brush)
                        .active_layer(None)
                        .show(ui, &mut state);
                    *last.borrow_mut() = outcome;
                });
            })
            .drop_without_applying_deltas();
            last.borrow().clone()
        };
        let at = egui::pos2(400.0, 300.0);
        let press = egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let release = egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };
        let _ = frame(vec![]);
        for events in [vec![press], vec![release], vec![]] {
            let out = frame(events);
            assert!(out.paint.is_none(), "jamais de commit sans calque");
            assert!(out.pointer_world.is_empty(), "aucun point retenu");
            assert!(out.ink_points.is_empty(), "aucun overlay");
        }
        assert!(stroke.is_empty(), "aucun orphelin");
    }

    #[test]
    fn canvas_renders_with_every_tool_without_panic() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            let mut state = ViewportState::default();
            let mut stroke = Vec::new();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    // With a dummy texture then empty state.
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 600.0))))
                        .tool(tool)
                        .show_grid(true)
                        .active_layer(None)
                        .show(ui, &mut state);
                    assert!(outcome.paint.is_none(), "aucun trait sans interaction");
                    assert!(outcome.pointer_world.is_empty());
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .tool(tool)
                        .show(ui, &mut state);
                    assert!(outcome.paint.is_none());
                });
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn worker_paint_stroke_changes_pixels() {
        use crate::ui::engine_bridge::apply_command;
        use photo_engine::{Document, LayerNode, PixelLayer};
        use std::sync::Arc;

        let mut doc = Document::new(8, 8);
        let red = image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 40, 40, 255]));
        let id = {
            let layer = PixelLayer::new("fond", Arc::new(image::DynamicImage::ImageRgba8(red)));
            let id = layer.id;
            doc.push_layer(LayerNode::Pixel(layer));
            id
        };
        let points: Vec<(f32, f32)> = (0..8).map(|i| (i as f32, i as f32)).collect();
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: id,
                points,
                eraser: false,
                radius: 3.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        let painted = doc
            .pixel_layer(id)
            .expect("calque present")
            .source_image
            .to_rgba8();
        // The diagonal stroke center is repainted opaque blue.
        assert_eq!(painted.get_pixel(4, 4).0[0..3], [0, 0, 255]);
        // Off-stroke corners are intact.
        assert_eq!(painted.get_pixel(0, 7).0[0..3], [200, 40, 40]);
    }

    #[test]
    fn worker_paint_invalid_is_noop() {
        use crate::ui::engine_bridge::apply_command;
        use photo_engine::{Document, LayerNode, PixelLayer};
        use std::sync::Arc;
        use uuid::Uuid;

        let mut doc = Document::new(4, 4);
        let red = image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 40, 40, 255]));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new(
            "fond",
            Arc::new(image::DynamicImage::ImageRgba8(red)),
        )));
        // Unknown id, zero radius, no points: no panic, no change.
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: Uuid::new_v4(),
                points: vec![(1.0, 1.0)],
                eraser: false,
                radius: 2.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        let id = doc.root[0].id();
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: id,
                points: vec![],
                eraser: false,
                radius: 2.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: id,
                points: vec![(1.0, 1.0)],
                eraser: false,
                radius: 0.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        let untouched = doc
            .pixel_layer(id)
            .expect("calque present")
            .source_image
            .to_rgba8();
        assert!(untouched.pixels().all(|p| p.0[0..3] == [200, 40, 40]));
    }
}
