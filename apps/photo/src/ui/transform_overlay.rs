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

//! Canvas transform overlay (Move tool, Affinity-style): bounding box
//! of the selected pixel layer with 8 scale handles + 1 rotation
//! handle, drawn in screen space over the viewport.
//!
//! All math is document/screen pixels, never layer pixels: drags
//! resolve against the gesture-start transform frozen in
//! [`TransformDrag`], so zoom/pan mid-gesture stay exact
//! (state-only). Rotation/skew need no placement gate (exact
//! [`Transform2D::doc_corners`] both ways).
//!
//! Hit-testing and drag resolution are pure functions (unit-tested
//! without egui); only [`draw_transform_overlay`] paints.

use crate::ui::features::layers::PhotoLayerInfo;
use photo_engine::Transform2D;
use uuid::Uuid;

/// Scale range accepted from handles (same clamp as the engine
/// render: avoids degenerate/collapsed layers mid-drag).
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 8.0;

/// Corner scale handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

/// Edge scale handle (single axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Overlay handle under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformHandle {
    /// One of the 4 corners (uniform scale about the opposite corner).
    Corner(Corner),
    /// One of the 4 edge midpoints (single-axis scale).
    Edge(Edge),
    /// Rotation knob above the top edge (about the center).
    Rotate,
}

impl Corner {
    /// All four, in doc_corners order (tl, tr, br, bl).
    const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];

    /// Index into [`frame_corners`].
    fn index(self) -> usize {
        match self {
            Self::TopLeft => 0,
            Self::TopRight => 1,
            Self::BottomRight => 2,
            Self::BottomLeft => 3,
        }
    }

    /// Opposite corner (scale anchor).
    fn opposite(self) -> Self {
        match self {
            Self::TopLeft => Self::BottomRight,
            Self::TopRight => Self::BottomLeft,
            Self::BottomRight => Self::TopLeft,
            Self::BottomLeft => Self::TopRight,
        }
    }
}

impl Edge {
    const ALL: [Self; 4] = [Self::Top, Self::Right, Self::Bottom, Self::Left];

    /// Corner indices bounding this edge (doc_corners order).
    fn ends(self) -> (usize, usize) {
        match self {
            Self::Top => (0, 1),
            Self::Right => (1, 2),
            Self::Bottom => (2, 3),
            Self::Left => (3, 0),
        }
    }

    /// Corner indices of the opposite edge (scale anchor side).
    fn anchor_ends(self) -> (usize, usize) {
        match self {
            Self::Top => (2, 3),
            Self::Right => (3, 0),
            Self::Bottom => (0, 1),
            Self::Left => (1, 2),
        }
    }

    /// Screen cursor for this axis.
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Self::Top | Self::Bottom => egui::CursorIcon::ResizeVertical,
            Self::Left | Self::Right => egui::CursorIcon::ResizeHorizontal,
        }
    }
}

impl TransformHandle {
    /// Screen cursor for the handle.
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Self::Corner(Corner::TopLeft) | Self::Corner(Corner::BottomRight) => {
                egui::CursorIcon::ResizeNwSe
            }
            Self::Corner(_) => egui::CursorIcon::ResizeNeSw,
            Self::Edge(edge) => edge.cursor(),
            Self::Rotate => egui::CursorIcon::PointingHand,
        }
    }
}

/// Four layer corners (tl, tr, br, bl) in document pixels.
pub fn frame_corners(transform: &Transform2D, dims: (u32, u32)) -> [(f32, f32); 4] {
    transform.doc_corners(dims.0 as f32, dims.1 as f32)
}

/// Frame center (rotation pivot) in document pixels.
pub fn frame_center(corners: [(f32, f32); 4]) -> (f32, f32) {
    (
        (corners[0].0 + corners[2].0) / 2.0,
        (corners[0].1 + corners[2].1) / 2.0,
    )
}

/// Document → screen through the thumbnail mapping (same conventions
/// as the canvas and the ink overlay: `thumbnail = doc / k + origin`,
/// `screen = dest.min + thumbnail * dest.width() / thumb_w`).
pub fn doc_to_screen(
    doc: (f32, f32),
    dest: egui::Rect,
    origin: egui::Vec2,
    thumb_to_doc: f32,
    thumb_w: f32,
) -> Option<egui::Pos2> {
    if !thumb_to_doc.is_finite()
        || thumb_to_doc <= f32::EPSILON
        || !thumb_w.is_finite()
        || thumb_w <= 0.0
        || !dest.width().is_finite()
        || dest.width() <= 0.0
    {
        return None;
    }
    let screen_per_thumb = dest.width() / thumb_w;
    let to_screen = screen_per_thumb / thumb_to_doc;
    let (x, y) = (
        dest.min.x + doc.0 * to_screen + origin.x * screen_per_thumb,
        dest.min.y + doc.1 * to_screen + origin.y * screen_per_thumb,
    );
    (x.is_finite() && y.is_finite()).then(|| egui::pos2(x, y))
}

/// Screen → document (inverse of [`doc_to_screen`]).
pub fn screen_to_doc(
    screen: egui::Pos2,
    dest: egui::Rect,
    origin: egui::Vec2,
    thumb_to_doc: f32,
    thumb_w: f32,
) -> Option<(f32, f32)> {
    if !thumb_to_doc.is_finite()
        || thumb_to_doc <= f32::EPSILON
        || !thumb_w.is_finite()
        || thumb_w <= 0.0
        || !dest.width().is_finite()
        || dest.width() <= 0.0
    {
        return None;
    }
    let screen_per_thumb = dest.width() / thumb_w;
    let thumb = (
        (screen.x - dest.min.x) / screen_per_thumb,
        (screen.y - dest.min.y) / screen_per_thumb,
    );
    let doc = (
        (thumb.0 - origin.x) * thumb_to_doc,
        (thumb.1 - origin.y) * thumb_to_doc,
    );
    (doc.0.is_finite() && doc.1.is_finite()).then_some(doc)
}

/// Rotation knob in screen space: top-edge midpoint pushed outward
/// along the edge normal by `lift` screen pixels.
pub fn rotate_knob(quad: [egui::Pos2; 4], lift: f32) -> Option<(egui::Pos2, egui::Vec2)> {
    let mid = egui::pos2((quad[0].x + quad[1].x) / 2.0, (quad[0].y + quad[1].y) / 2.0);
    let mut edge = quad[1] - quad[0];
    if edge.length() < f32::EPSILON {
        return None;
    }
    edge /= edge.length();
    // Outward normal (away from the center).
    let center = egui::pos2((quad[0].x + quad[2].x) / 2.0, (quad[0].y + quad[2].y) / 2.0);
    let mut normal = egui::vec2(-edge.y, edge.x);
    if (mid + normal - center).length() < (mid - normal - center).length() {
        normal = -normal;
    }
    Some((mid + normal * lift, normal))
}

/// Handle at a screen point (corners, edges, then knob).
pub fn hit_handle(
    quad: [egui::Pos2; 4],
    knob: egui::Pos2,
    at: egui::Pos2,
    radius: f32,
) -> Option<TransformHandle> {
    for corner in Corner::ALL {
        if quad[corner.index()].distance(at) <= radius {
            return Some(TransformHandle::Corner(corner));
        }
    }
    for edge in Edge::ALL {
        let (a, b) = edge.ends();
        let mid = egui::pos2((quad[a].x + quad[b].x) / 2.0, (quad[a].y + quad[b].y) / 2.0);
        if mid.distance(at) <= radius {
            return Some(TransformHandle::Edge(edge));
        }
    }
    if knob.distance(at) <= radius {
        return Some(TransformHandle::Rotate);
    }
    None
}

/// Point in convex quad (either winding): the frame interior.
pub fn point_in_quad(p: (f32, f32), quad: [(f32, f32); 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (ax, ay) = quad[i];
        let (bx, by) = quad[(i + 1) % 4];
        let cross = (bx - ax) * (p.1 - ay) - (by - ay) * (p.0 - ax);
        if cross.abs() < f32::EPSILON {
            continue;
        }
        let s = cross.signum();
        if sign == 0.0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}

/// Topmost pixel layer under a document point (display order:
/// `layers` is top-of-stack first, children above their parent).
/// Groups/adjustments are skipped (no canvas transform).
pub fn hit_test_topmost(layers: &[PhotoLayerInfo], doc: (f32, f32)) -> Option<Uuid> {
    for layer in layers {
        if let Some(hit) = hit_test_in(layer, doc) {
            return Some(hit);
        }
    }
    None
}

fn hit_test_in(layer: &PhotoLayerInfo, doc: (f32, f32)) -> Option<Uuid> {
    if !layer.visible {
        return None;
    }
    for child in &layer.children {
        if let Some(hit) = hit_test_in(child, doc) {
            return Some(hit);
        }
    }
    let (w, h) = layer.dims?;
    let (x, y) = layer
        .transform
        .doc_to_local(w as f32, h as f32, doc.0, doc.1)?;
    ((0.0..w as f32).contains(&x) && (0.0..h as f32).contains(&y)).then_some(layer.id)
}

/// In-progress transform gesture (anchors frozen at grab time,
/// document pixels — immune to zoom/pan mid-drag).
#[derive(Debug, Clone)]
pub struct TransformDrag {
    /// Dragged layer.
    pub layer: Uuid,
    /// Carrier dimensions (source pixels).
    pub dims: (u32, u32),
    /// Transform at grab time (skew and friends carried through).
    pub start: Transform2D,
    /// Last transform sent to the worker (change-gated sends).
    pub sent: Transform2D,
    /// Active handle (`None` = body move).
    pub handle: Option<TransformHandle>,
    /// Pointer at grab time (document pixels).
    pub grab_doc: (f32, f32),
    /// Scale anchor: opposite corner / edge (document pixels).
    pub anchor_doc: (f32, f32),
    /// Rotation pivot (document pixels).
    pub center_doc: (f32, f32),
    /// `|grab − anchor|` at grab time (uniform scale).
    pub grab_dist: f32,
    /// Edge-axis projection at grab time (edge scale).
    pub grab_proj: f32,
    /// Pointer angle about the center at grab time (rotation).
    pub grab_angle: f32,
    /// Start angles kept for normalization context.
    pub start_rotation: f32,
}

impl TransformDrag {
    /// Begins a gesture: freezes the start transform and derives
    /// anchors from its frame.
    pub fn begin(
        layer: Uuid,
        dims: (u32, u32),
        start: Transform2D,
        handle: Option<TransformHandle>,
        grab_doc: (f32, f32),
    ) -> Option<Self> {
        let corners = frame_corners(&start, dims);
        let center = frame_center(corners);
        let anchor_doc = match handle {
            None => (0.0, 0.0),
            Some(TransformHandle::Corner(c)) => corners[c.opposite().index()],
            Some(TransformHandle::Edge(e)) => {
                let (a, b) = e.anchor_ends();
                (
                    (corners[a].0 + corners[b].0) / 2.0,
                    (corners[a].1 + corners[b].1) / 2.0,
                )
            }
            Some(TransformHandle::Rotate) => center,
        };
        let axis = axis_of(handle, corners).unwrap_or((1.0, 0.0));
        let along = (grab_doc.0 - anchor_doc.0, grab_doc.1 - anchor_doc.1);
        Some(Self {
            layer,
            dims,
            start,
            sent: start,
            handle,
            grab_doc,
            anchor_doc,
            center_doc: center,
            grab_dist: (along.0 * along.0 + along.1 * along.1).sqrt(),
            grab_proj: along.0 * axis.0 + along.1 * axis.1,
            grab_angle: (grab_doc.1 - center.1).atan2(grab_doc.0 - center.0),
            start_rotation: start.rotation_deg,
        })
    }

    /// Resolves the pointer to a new transform (pure: frozen start +
    /// current pointer, no UI state).
    pub fn resolve(&self, pointer_doc: (f32, f32)) -> Transform2D {
        match self.handle {
            None => self.resolve_move(pointer_doc),
            Some(TransformHandle::Corner(_)) => self.resolve_uniform(pointer_doc),
            Some(TransformHandle::Edge(_)) => self.resolve_edge(pointer_doc),
            Some(TransformHandle::Rotate) => self.resolve_rotate(pointer_doc),
        }
    }

    /// Body drag: pure translation.
    fn resolve_move(&self, pointer_doc: (f32, f32)) -> Transform2D {
        let mut t = self.start;
        t.offset_x += pointer_doc.0 - self.grab_doc.0;
        t.offset_y += pointer_doc.1 - self.grab_doc.1;
        t
    }

    /// Corner drag: uniform scale about the opposite corner (grab
    /// distance ratio, clamped like the engine).
    fn resolve_uniform(&self, pointer_doc: (f32, f32)) -> Transform2D {
        if self.grab_dist < f32::EPSILON {
            return self.start;
        }
        let along = (
            pointer_doc.0 - self.anchor_doc.0,
            pointer_doc.1 - self.anchor_doc.1,
        );
        let dist = (along.0 * along.0 + along.1 * along.1).sqrt();
        let k = (dist / self.grab_dist).clamp(MIN_SCALE, MAX_SCALE);
        self.scaled_about(k, k)
    }

    /// Edge drag: single-axis scale about the opposite edge, projected
    /// on the edge axis (frozen at grab time).
    fn resolve_edge(&self, pointer_doc: (f32, f32)) -> Transform2D {
        let Some(axis) = axis_of(self.handle, frame_corners(&self.start, self.dims)) else {
            return self.start;
        };
        if self.grab_proj.abs() < f32::EPSILON {
            return self.start;
        }
        let along = (
            pointer_doc.0 - self.anchor_doc.0,
            pointer_doc.1 - self.anchor_doc.1,
        );
        let proj = along.0 * axis.0 + along.1 * axis.1;
        let k = (proj / self.grab_proj).clamp(MIN_SCALE, MAX_SCALE);
        // Axis in layer space: horizontal edge (top/bottom) scales Y,
        // vertical edge scales X. The frozen axis is a doc-space
        // direction; pick the dominant layer axis by un-rotating it.
        let rad = self.start.rotation_deg.to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        let local = (axis.0 * cos + axis.1 * sin, -axis.0 * sin + axis.1 * cos);
        if local.0.abs() >= local.1.abs() {
            self.scaled_about(k, 1.0)
        } else {
            self.scaled_about(1.0, k)
        }
    }

    /// Rotation drag: angle about the center, normalized to
    /// (-180°, 180°].
    fn resolve_rotate(&self, pointer_doc: (f32, f32)) -> Transform2D {
        let angle = (pointer_doc.1 - self.center_doc.1).atan2(pointer_doc.0 - self.center_doc.0);
        let mut delta = (angle - self.grab_angle).to_degrees();
        while delta <= -180.0 {
            delta += 360.0;
        }
        while delta > 180.0 {
            delta -= 360.0;
        }
        let mut t = self.start;
        t.rotation_deg = self.start_rotation + delta;
        t
    }

    /// Scales about the frozen anchor, keeping the anchor fixed in
    /// document space (offset compensates the scaled geometry).
    /// Exact under rotation/skew: the frame point sitting at the
    /// anchor under the start transform is located (inverse map)
    /// and re-placed at the anchor under the new scales.
    fn scaled_about(&self, kx: f32, ky: f32) -> Transform2D {
        let mut t = self.start;
        t.scale_x = (self.start.scale_x * kx).clamp(MIN_SCALE, MAX_SCALE);
        t.scale_y = (self.start.scale_y * ky).clamp(MIN_SCALE, MAX_SCALE);
        let (w, h) = (self.dims.0 as f32, self.dims.1 as f32);
        // Frame point sitting at the anchor under the start
        // transform → re-placed at the anchor under the new scales.
        let Some(local) = self
            .start
            .doc_to_local(w, h, self.anchor_doc.0, self.anchor_doc.1)
        else {
            return t;
        };
        let placed = t.local_to_doc(w, h, local.0, local.1);
        t.offset_x += self.anchor_doc.0 - placed.0;
        t.offset_y += self.anchor_doc.1 - placed.1;
        t
    }
}

/// Unit edge-axis direction in document space for an edge handle
/// (`None` unless `handle` is an edge).
fn axis_of(handle: Option<TransformHandle>, corners: [(f32, f32); 4]) -> Option<(f32, f32)> {
    let edge = match handle {
        Some(TransformHandle::Edge(e)) => e,
        _ => return None,
    };
    let (a, b) = edge.ends();
    // Axis across the edge (anchor → grabbed side).
    let (ax, ay) = corners[a];
    let (bx, by) = corners[b];
    let mid = ((ax + bx) / 2.0, (ay + by) / 2.0);
    // Opposite edge midpoint as direction reference.
    let (c, d) = edge.anchor_ends();
    let other = (
        (corners[c].0 + corners[d].0) / 2.0,
        (corners[c].1 + corners[d].1) / 2.0,
    );
    let dx = mid.0 - other.0;
    let dy = mid.1 - other.1;
    let len = (dx * dx + dy * dy).sqrt();
    (len > f32::EPSILON).then_some((dx / len, dy / len))
}

/// Draws the overlay (frame + 8 handles + knob + center) and reports
/// the handle under `hover` for cursor feedback. All styling from
/// the theme (no hardcoded color or size beyond the knob lift).
pub fn draw_transform_overlay(
    painter: &egui::Painter,
    theme: ui_kit::theme::CygnusTheme,
    quad: [egui::Pos2; 4],
    hover: Option<egui::Pos2>,
    quiet: bool,
) -> Option<TransformHandle> {
    let half = theme.spacing.sm;
    let stroke = egui::Stroke::new(theme.borders.medium, theme.colors.accent);
    painter.add(egui::Shape::closed_line(quad.to_vec(), stroke));
    // Center pivot (always drawn: the rotation reference).
    let center = egui::pos2((quad[0].x + quad[2].x) / 2.0, (quad[0].y + quad[2].y) / 2.0);
    painter.line_segment(
        [
            center + egui::vec2(-half, 0.0),
            center + egui::vec2(half, 0.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(0.0, -half),
            center + egui::vec2(0.0, half),
        ],
        stroke,
    );
    if quiet {
        // Active drag: frame + pivot only — adornments stay out of
        // the way while the content moves under the pointer.
        return None;
    }
    // Edge midpoints.
    for edge in Edge::ALL {
        let (a, b) = edge.ends();
        let mid = egui::pos2((quad[a].x + quad[b].x) / 2.0, (quad[a].y + quad[b].y) / 2.0);
        painter.rect_filled(
            egui::Rect::from_center_size(mid, egui::vec2(half * 2.0, half * 2.0)),
            theme.radius.xs,
            theme.colors.bg_primary,
        );
        painter.rect_stroke(
            egui::Rect::from_center_size(mid, egui::vec2(half * 2.0, half * 2.0)),
            theme.radius.xs,
            stroke,
            egui::StrokeKind::Middle,
        );
    }
    for corner in Corner::ALL {
        let rect =
            egui::Rect::from_center_size(quad[corner.index()], egui::vec2(half * 2.0, half * 2.0));
        painter.rect_filled(rect, theme.radius.xs, theme.colors.bg_primary);
        painter.rect_stroke(rect, theme.radius.xs, stroke, egui::StrokeKind::Middle);
    }
    // Rotation knob + stem.
    let mid_top = egui::pos2((quad[0].x + quad[1].x) / 2.0, (quad[0].y + quad[1].y) / 2.0);
    if let Some((knob, _)) = rotate_knob(quad, half * 6.0) {
        painter.line_segment([mid_top, knob], stroke);
        painter.circle_filled(knob, half, theme.colors.bg_primary);
        painter.circle_stroke(knob, half, stroke);
    }
    let knob = rotate_knob(quad, half * 6.0).map(|(k, _)| k)?;
    hover.and_then(|at| hit_handle(quad, knob, at, half * 2.5))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::features::layers::types::snapshot_layers;
    use photo_engine::{Document, LayerNode, PixelLayer};
    use std::sync::Arc;

    fn identity_drag(handle: Option<TransformHandle>, grab: (f32, f32)) -> TransformDrag {
        TransformDrag::begin(
            Uuid::new_v4(),
            (100, 80),
            Transform2D::default(),
            handle,
            grab,
        )
        .expect("drag")
    }

    #[test]
    fn body_drag_translates_only() {
        let drag = identity_drag(None, (10.0, 10.0));
        let t = drag.resolve((35.0, 50.0));
        assert_eq!((t.offset_x, t.offset_y), (25.0, 40.0));
        assert_eq!((t.scale_x, t.scale_y), (1.0, 1.0));
        assert_eq!(t.rotation_deg, 0.0);
    }

    #[test]
    fn corner_drag_scales_uniformly_about_opposite() {
        // Bottom-right grabbed (anchor = top-left (0,0)): doubling
        // the grab distance doubles both scales, anchor fixed.
        let drag = identity_drag(
            Some(TransformHandle::Corner(Corner::BottomRight)),
            (100.0, 80.0),
        );
        let t = drag.resolve((200.0, 160.0));
        assert!((t.scale_x - 2.0).abs() < 1e-4, "sx {}", t.scale_x);
        assert!((t.scale_y - 2.0).abs() < 1e-4, "sy {}", t.scale_y);
        // Anchor Celui-ci: local (0,0) still maps to document (0,0).
        let placed = t.local_to_doc(100.0, 80.0, 0.0, 0.0);
        assert!(placed.0.abs() < 1e-3 && placed.1.abs() < 1e-3, "{placed:?}");
    }

    #[test]
    fn edge_drag_scales_single_axis_about_opposite() {
        // Top edge grabbed (anchor = bottom midpoint (50,80)):
        // pulling up 1.5× stretches Y only, anchor fixed.
        let drag = identity_drag(Some(TransformHandle::Edge(Edge::Top)), (50.0, 0.0));
        let t = drag.resolve((50.0, -40.0));
        assert!((t.scale_x - 1.0).abs() < 1e-4, "sx {}", t.scale_x);
        assert!((t.scale_y - 1.5).abs() < 1e-4, "sy {}", t.scale_y);
        let placed = t.local_to_doc(100.0, 80.0, 50.0, 80.0);
        assert!(
            (placed.0 - 50.0).abs() < 1e-3 && (placed.1 - 80.0).abs() < 1e-3,
            "{placed:?}"
        );
    }

    #[test]
    fn rotate_drag_angles_about_center() {
        // Grab east of the center (50,40), drag north: −90°.
        let drag = identity_drag(Some(TransformHandle::Rotate), (150.0, 40.0));
        let t = drag.resolve((50.0, -60.0));
        assert!(
            (t.rotation_deg + 90.0).abs() < 1e-4,
            "rot {}",
            t.rotation_deg
        );
        assert_eq!((t.offset_x, t.offset_y), (0.0, 0.0));
    }

    #[test]
    fn screen_mapping_roundtrips() {
        let dest = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        // 400×300 thumbnail of an 800×600 plane, document at (20,10).
        let back = screen_to_doc(
            doc_to_screen((100.0, 200.0), dest, egui::vec2(20.0, 10.0), 2.0, 400.0).expect("fwd"),
            dest,
            egui::vec2(20.0, 10.0),
            2.0,
            400.0,
        )
        .expect("back");
        assert!(
            (back.0 - 100.0).abs() < 1e-3 && (back.1 - 200.0).abs() < 1e-3,
            "{back:?}"
        );
    }

    #[test]
    fn handles_hit_corners_edges_knob() {
        let quad = [
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 80.0),
            egui::pos2(0.0, 80.0),
        ];
        let knob = rotate_knob(quad, 20.0).expect("knob").0;
        assert_eq!(
            hit_handle(quad, knob, egui::pos2(2.0, 1.0), 10.0),
            Some(TransformHandle::Corner(Corner::TopLeft))
        );
        assert_eq!(
            hit_handle(quad, knob, egui::pos2(50.0, 2.0), 10.0),
            Some(TransformHandle::Edge(Edge::Top))
        );
        assert_eq!(
            hit_handle(quad, knob, knob, 10.0),
            Some(TransformHandle::Rotate)
        );
        assert_eq!(hit_handle(quad, knob, egui::pos2(50.0, 40.0), 10.0), None);
    }

    #[test]
    fn hit_test_topmost_prefers_top_and_children() {
        let mut doc = Document::new(200, 200);
        let fond = PixelLayer::new("fond", Arc::new(image::DynamicImage::new_rgba8(200, 200)));
        let mut dessus =
            PixelLayer::new("dessus", Arc::new(image::DynamicImage::new_rgba8(60, 60)));
        dessus.transform.offset_x = 20.0;
        dessus.transform.offset_y = 10.0;
        let dessus_id = dessus.id;
        doc.push_layer(LayerNode::Pixel(fond));
        doc.push_layer(LayerNode::Pixel(dessus));
        let layers = snapshot_layers(&doc);
        // Overlap zone (doc (30,20) = dessus pixel (10,10)): top wins.
        assert_eq!(hit_test_topmost(&layers, (30.0, 20.0)), Some(dessus_id));
        // Outside dessus (doc (150,150)): fond.
        let fond_id = layers.iter().find(|l| l.name == "fond").expect("fond").id;
        assert_eq!(hit_test_topmost(&layers, (150.0, 150.0)), Some(fond_id));
        // Outside everything: none.
        assert_eq!(hit_test_topmost(&layers, (500.0, 500.0)), None);
    }
}
