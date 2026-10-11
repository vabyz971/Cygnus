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

//! Viewport scheduling (Phase 6F): WHAT to render, IN WHICH ORDER.
//!
//! The scheduler computes NO pixels and knows neither blur nor blend:
//!
//! ```text
//! Camera (screen ⇄ document) → document viewport → visible TileCoords →
//! dirty analysis → ordered plan → RenderWorker → TileCache
//! ```
//!
//! Rules:
//!
//! - compositing (`composite_region_with`) keeps working in
//!   document-space: never any screen coordinate downstream;
//! - halos stay the exclusive business of `SpatialScope` (the scheduler
//!   only knows already-expressed dirty rects);
//! - no prefetch in 6F: only visible tiles are scheduled;
//! - deterministic total order: priority, then INTEGER distance to the
//!   center, then `y`, then `x` (no floats in the order key).

use tiles::TileRegion;

use super::dirty::DirtyRegion;
use crate::tiles::{TileCoord, tile_coords_for_rect, tile_rect};

/// Minimal camera: zoom + pan (no rotation in 6F).
///
/// Convention: `screen = (doc − pan) × zoom + screen_center` where
/// `screen_center = (width/2, height/2)`; the inverse gives the document
/// viewport. Display zoom stays state-only canvas-side: the camera is
/// used here to DERIVE the document viewport (what to render), never to
/// re-render at zoom (render scale stays 1.0, see `RenderRequest`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Zoom factor (> 0; 1.0 = 100%).
    pub zoom: f32,
    /// Horizontal offset of the document center in screen px.
    pub pan_x: f32,
    /// Vertical offset of the document center in screen px.
    pub pan_y: f32,
}

impl Camera {
    /// Identity camera (100% zoom, centered).
    #[must_use]
    pub fn identity() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }

    /// New camera (non-finite `zoom` or ≤ 0 ⇒ 1.0).
    #[must_use]
    pub fn new(zoom: f32, pan_x: f32, pan_y: f32) -> Self {
        Self {
            zoom: if zoom.is_finite() && zoom > 0.0 {
                zoom
            } else {
                1.0
            },
            pan_x: if pan_x.is_finite() { pan_x } else { 0.0 },
            pan_y: if pan_y.is_finite() { pan_y } else { 0.0 },
        }
    }

    /// Screen → document (`screen_w/h` = canvas size in screen px).
    #[must_use]
    pub fn screen_to_document(self, sx: f32, sy: f32, screen_w: f32, screen_h: f32) -> (f32, f32) {
        let z = self.zoom.max(f32::EPSILON);
        (
            (sx - screen_w / 2.0 - self.pan_x) / z,
            (sy - screen_h / 2.0 - self.pan_y) / z,
        )
    }

    /// Document → screen.
    #[must_use]
    pub fn document_to_screen(self, dx: f32, dy: f32, screen_w: f32, screen_h: f32) -> (f32, f32) {
        (
            (dx) * self.zoom + screen_w / 2.0 + self.pan_x,
            (dy) * self.zoom + screen_h / 2.0 + self.pan_y,
        )
    }

    /// Visible document viewport for a `screen_w × screen_h` canvas
    /// (DOCUMENT SPACE, conservative integrality: floor/ceil).
    #[must_use]
    pub fn document_viewport(self, screen_w: f32, screen_h: f32) -> TileRegion {
        let (x0, y0) = self.screen_to_document(0.0, 0.0, screen_w, screen_h);
        let (x1, y1) = self.screen_to_document(screen_w, screen_h, screen_w, screen_h);
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
            return TileRegion::EMPTY;
        }
        TileRegion::new(
            x0.floor() as i32,
            y0.floor() as i32,
            (x1.ceil() - x0.floor()).max(1.0) as u32,
            (y1.ceil() - y0.floor()).max(1.0) as u32,
        )
    }
}

/// Viewport render request (Phase 6F §7): what the canvas looks at. The
///
/// worker never knows UI widgets: it receives this pure descriptor
/// (document viewport + render scale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
    /// Zone to produce (DOCUMENT SPACE, integer).
    pub viewport: TileRegion,
    /// Render scale (1.0: display zoom is state-only; any other value
    /// partitions keys for future mipmaps without implementing any —
    /// never a cross-scale false hit).
    pub scale: f32,
}

impl RenderRequest {
    /// New request (non-finite `scale` or ≤ 0 ⇒ 1.0).
    #[must_use]
    pub fn new(viewport: TileRegion, scale: f32) -> Self {
        Self {
            viewport,
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
        }
    }
}

/// Priority context (Phase 6F §10): slot reserved to distinguish active /
/// inactive interaction. In 6F the ordering policy is identical in both
/// cases (dirty first, then distance) — the field is accepted and
/// recorded for later phases (prefetch gating…), without changing
/// today's plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderPriorityContext {
    /// True during a gesture (pan/scroll/paint in progress).
    pub interaction_active: bool,
}

impl RenderPriorityContext {
    /// Inactive context (completion).
    #[must_use]
    pub fn idle() -> Self {
        Self {
            interaction_active: false,
        }
    }

    /// Active gesture context.
    #[must_use]
    pub fn active() -> Self {
        Self {
            interaction_active: true,
        }
    }
}

/// A tile's priority (ascending order = rendered first).
///
/// 6F: `visible + dirty` then `visible` (no prefetch — "visible first"
/// suffices). The worker skips computation on hits anyway; order matters
/// for perceived latency and future LRU pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TilePriority {
    /// Visible and dirty: stale content to re-render first.
    VisibleDirty,
    /// Visible and clean (likely hit): completion.
    Visible,
}

/// Scheduled unit request: canonical coordinate + rect + priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRenderRequest {
    /// Canonical identity (`tile_px` grid).
    pub coord: TileCoord,
    /// Clipped document rectangle (what the worker asks the cache for).
    pub rect: TileRegion,
    /// Priority (primary order).
    pub priority: TilePriority,
}

/// Builds the ordered plan for `request.viewport` at `tile_px` step:
/// visible dirty tiles marked `VisibleDirty`, deterministic total order
/// `(priority, integer distance² to center, y, x)`. No clipping to the
///
/// document: overflowing tiles (infinite plane) are legitimate — the
/// render clamps them and returns `None` if nothing contributes. The
/// scheduler recomputes no halo: `dirty` is already expressed as
/// invalidation zones (caller: 6D `mark_dirty_scope`). No tile outside
/// the viewport (no prefetch in 6F).
#[must_use]
pub fn schedule_viewport(
    request: &RenderRequest,
    dirty: &DirtyRegion,
    tile_px: u32,
    _ctx: &RenderPriorityContext,
) -> Vec<TileRenderRequest> {
    let visible = request.viewport;
    if visible.is_empty() {
        return Vec::new();
    }
    // Viewport center (integers: no floats in the order).
    let cx = i64::from(visible.x) + i64::from(visible.width) / 2;
    let cy = i64::from(visible.y) + i64::from(visible.height) / 2;
    let mut plan: Vec<(TileRenderRequest, u64)> = Vec::new();
    for coord in tile_coords_for_rect(&visible, tile_px) {
        // FULL canonical rect (unclipped): same tile ⇒ same key whatever
        // the request (§4); the render clamps and returns None if empty.
        let rect = tile_rect(coord, tile_px);
        let priority = if dirty.intersects(rect) {
            TilePriority::VisibleDirty
        } else {
            TilePriority::Visible
        };
        let (ccx, ccy) = coord.center(tile_px.max(1));
        let dx = ccx - cx;
        let dy = ccy - cy;
        let dist2 = (dx * dx + dy * dy) as u64;
        plan.push((
            TileRenderRequest {
                coord,
                rect,
                priority,
            },
            dist2,
        ));
    }
    // Deterministic total order: priority, distance², y, x.
    plan.sort_by(|a, b| {
        (a.0.priority, a.1, a.0.coord.y, a.0.coord.x).cmp(&(
            b.0.priority,
            b.1,
            b.0.coord.y,
            b.0.coord.x,
        ))
    });
    plan.into_iter().map(|(r, _)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_round_trip() {
        let cam = Camera::new(2.0, 30.0, -15.0);
        let (dx, dy) = cam.screen_to_document(400.0, 300.0, 800.0, 600.0);
        let (sx, sy) = cam.document_to_screen(dx, dy, 800.0, 600.0);
        assert!((sx - 400.0).abs() < 0.01, "round-trip x");
        assert!((sy - 300.0).abs() < 0.01, "round-trip y");
        // Identity: screen center == document center.
        let (dx, dy) = Camera::identity().screen_to_document(400.0, 300.0, 800.0, 600.0);
        assert!((dx - 0.0).abs() < 0.01 && (dy - 0.0).abs() < 0.01);
        // Zoom 2: the document viewport covers half the screen (height 301:
        // floor(-142.5) = -143, conservative).
        let view = cam.document_viewport(800.0, 600.0);
        assert_eq!(view.width, 400);
        assert_eq!(view.height, 301);
        // Guard rails.
        assert_eq!(Camera::new(0.0, 0.0, 0.0).zoom, 1.0);
        assert_eq!(Camera::new(f32::NAN, 0.0, 0.0).zoom, 1.0);
    }

    #[test]
    fn plan_visible_dirty_d_abord_deterministe() {
        let req = RenderRequest::new(TileRegion::new(0, 0, 512, 512), 1.0);
        let mut dirty = DirtyRegion::new();
        dirty.add(TileRegion::new(300, 300, 10, 10));
        let idle = RenderPriorityContext::idle();
        let plan = schedule_viewport(&req, &dirty, 256, &idle);
        assert_eq!(plan.len(), 4);
        // Dirty first.
        assert_eq!(plan[0].coord, TileCoord::new(1, 1));
        assert_eq!(plan[0].priority, TilePriority::VisibleDirty);
        assert!(
            plan[1..]
                .iter()
                .all(|r| r.priority == TilePriority::Visible)
        );
        // Determinism: two identical requests, same plan.
        let plan2 = schedule_viewport(&req, &dirty, 256, &idle);
        assert_eq!(plan, plan2);
        // Active context: same set (6F policy), identical order.
        let active = RenderPriorityContext::active();
        let plan3 = schedule_viewport(&req, &dirty, 256, &active);
        assert_eq!(plan, plan3);
    }

    #[test]
    fn plan_vide_et_hors_document() {
        let dirty = DirtyRegion::new();
        let idle = RenderPriorityContext::idle();
        let req = RenderRequest::new(TileRegion::EMPTY, 1.0);
        assert!(schedule_viewport(&req, &dirty, 256, &idle).is_empty());
        // Outside the document: the covering tile is scheduled (visibility), the
        // render clamps it and returns None (no pixels outside the document —
        // see the worker tests) — never a gap in coverage.
        let req = RenderRequest::new(TileRegion::new(900, 900, 10, 10), 1.0);
        let plan = schedule_viewport(&req, &dirty, 256, &idle);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].coord, TileCoord::new(3, 3));
    }

    #[test]
    fn plan_echelles_distinguees() {
        // Same viewport, different scales: distinct requests (anti-false-hit
        // partitioning, unchanged 1.0 render).
        let a = RenderRequest::new(TileRegion::new(0, 0, 256, 256), 1.0);
        let b = RenderRequest::new(TileRegion::new(0, 0, 256, 256), 2.0);
        assert_ne!(a, b);
        assert_eq!(RenderRequest::new(a.viewport, f32::NAN).scale, 1.0);
    }
}
