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

//! Transient interaction surface (Phase 6G.3P): paint feedback during the
//! drag, BEFORE the final commit into the document.
//!
//! ```text
//! MouseDown → begin_stroke() → MouseMove* → update (overlay)
//!     → MouseUp → commit (existing permanent path) → clear()
//!     → cancel → clear() (document untouched)
//! ```
//!
//! Rules:
//!
//! - LAYER space, same points and parameters as the commit: the coverage
//!   mask is built with EXACTLY the shared `paint` primitives (`stroke_bbox`,
//!   `stamp_polyline`), and the mask union is idempotent — stamping in
//!   pieces gives the same mask as in one go;
//! - the overlay NEVER touches the document (no snapshots, no dirty, no
//!   worker): the commit goes through the existing `PaintStroke` path,
//!   which stays the only permanent write (test-proven equivalence);
//! - stroke-bounded region (not layer): memory and uploads in O(stroke),
//!   never O(document);
//! - Paint mode: exact pixels on opaque background (same source-over formula);
//!   Erase mode: indicative white veil (the real eraser needs the base;
//!   the commit stays exact, only the transient is indicative);
//! - display restricted to rigid identity/translation/scale transforms
//!   ([`overlay_displayable`]): rotation/skew disabled presentation-side
//!   (engine exact anyway, documented extension gate).

use crate::document::Transform2D;
use crate::paint::{
    BrushParams, StrokeMode, composite_coverage, resample_stroke, stamp_polyline, stroke_bbox,
};

/// Interaction metrics (Phase 6G.3P §13): event and work-volume counters,
/// never imposed timings (hardware varies).
#[derive(Debug, Default, Clone, Copy)]
pub struct InteractionMetrics {
    /// Received points (raw, before filtering).
    pub points_received: u64,
    /// Coalesced points (degenerate `< EPSILON` or non-finite — no effect).
    pub points_coalesced: u64,
    /// Render submissions (`render()` called).
    pub submissions: u64,
    /// Renders performed (== `submissions`, synchronous — documented for a
    /// future async version where they would diverge).
    pub completions: u64,
    /// Pixels (re)composited into the overlay image (cumulative, segment bounds).
    pub surface_pixels_written: u64,
    /// Wall time of the last `render()` (µs, indicative, never asserted in tests).
    pub last_render_us: u128,
}

/// Transient presentation image: region + pixels + generation.
#[derive(Debug, Clone)]
pub struct OverlayFrame {
    /// Generation of the session that produced it (see `is_current`).
    pub generation: u64,
    /// Region origin in absolute layer coords.
    pub x: i32,
    /// Region origin in absolute layer coords.
    pub y: i32,
    /// Region width (px).
    pub width: u32,
    /// Region height (px).
    pub height: u32,
    /// Region RGBA8 pixels (stroke-only, see module docs).
    pub rgba: Vec<u8>,
}

/// Transient stroke session (one gesture = one logical instance rearmed
/// by `begin_stroke`).
#[derive(Debug, Clone)]
pub struct InteractionOverlay {
    brush: BrushParams,
    rx: f32,
    ry: f32,
    mask: Vec<u8>,
    image: Vec<u8>,
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
    /// Allocated capacity (`mask`/`image` are `cap_w × cap_h`, stride =
    /// `cap_w`): the logical region (`w × h`) grows without copying while
    /// it fits in capacity (geometric growth on reallocation) — one sweep
    /// no longer copies the whole region every frame (amortized O(1)
    /// instead of O(region)).
    cap_w: u32,
    cap_h: u32,
    last: Option<(f32, f32)>,
    generation: u64,
    dirty: bool,
    metrics: InteractionMetrics,
    /// Selection limiting the feedback like the commit (document px).
    clip: Option<crate::Selection>,
    /// Tip dynamics (Brush v2), same as the commit.
    dynamics: crate::paint::BrushDynamics,
    /// Dab placement carried across `add_points` calls.
    walker: crate::paint::DabWalker,
}

impl InteractionOverlay {
    /// Empty overlay (generation 0, no pixels).
    #[must_use]
    pub fn new() -> Self {
        Self {
            brush: BrushParams {
                radius: 0.0,
                color: [0, 0, 0],
                opacity: 0.0,
                mode: StrokeMode::Paint,
            },
            rx: 0.0,
            ry: 0.0,
            mask: Vec::new(),
            image: Vec::new(),
            ox: 0,
            oy: 0,
            w: 0,
            h: 0,
            cap_w: 0,
            cap_h: 0,
            last: None,
            generation: 0,
            dirty: false,
            metrics: InteractionMetrics::default(),
            clip: None,
            dynamics: crate::paint::BrushDynamics::default(),
            walker: crate::paint::DabWalker::default(),
        }
    }

    /// Starts a gesture: rearms state, bumps the generation (renders tagged
    /// with an older generation become stale).
    /// Returns the new generation.
    pub fn begin_stroke(&mut self, brush: BrushParams) -> u64 {
        self.brush = brush;
        self.rx = brush.radius;
        self.ry = brush.radius;
        self.mask.clear();
        self.image.clear();
        self.ox = 0;
        self.oy = 0;
        self.w = 0;
        self.h = 0;
        self.last = None;
        self.walker = crate::paint::DabWalker::default();
        self.generation = self.generation.wrapping_add(1);
        self.dirty = false;
        self.generation
    }

    /// Selection applied to the following renders (the commit clips the
    /// same way worker-side). Set before `begin_stroke`.
    pub fn set_clip(&mut self, clip: Option<crate::Selection>) {
        self.clip = clip;
    }

    /// Tip dynamics of the following gesture (same values the worker
    /// gets). Set before `begin_stroke`.
    pub fn set_dynamics(&mut self, dynamics: crate::paint::BrushDynamics) {
        self.dynamics = dynamics.sanitized();
    }

    /// Current generation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// True if `generation` is current (stale result ⇒ discard).
    #[must_use]
    pub fn is_current(&self, generation: u64) -> bool {
        generation == self.generation
    }

    /// True if no dabs (empty region).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Cumulative session metrics.
    #[must_use]
    pub fn metrics(&self) -> InteractionMetrics {
        self.metrics
    }

    /// Clears state (cancel or after commit): document untouched by
    /// construction (the overlay never touched it). The generation is kept
    /// (only `begin_stroke` bumps it).
    pub fn clear(&mut self) {
        self.mask.clear();
        self.image.clear();
        self.w = 0;
        self.h = 0;
        self.cap_w = 0;
        self.cap_h = 0;
        self.last = None;
        self.dirty = false;
    }

    /// Adds points (absolute layer coords, SAME values as at commit).
    /// Only stamps new segments since the last point (idempotent union:
    /// overlaps and duplicates have no effect). Returns true if the mask
    /// changed (presentation to refresh). Non-finite points are ignored one
    /// by one (the legacy commit yields implementation-defined results on
    /// hostile input — outside supported cases, documented); degenerate
    /// segments (`< EPSILON`, like the permanent path) are coalesced and
    /// counted.
    ///
    /// Smoothing: `fresh` ([last known] + new) goes through the SAME
    /// `resample_stroke` as the worker commit (subdivision + Chaikin) — the
    /// transient shows curves, not segments, even on fast sweeps (sparse
    /// points). Transient == commit equivalence is preserved: same inputs,
    /// same function (micro-gaps at packet junctions, invisible and replaced
    /// at `MouseUp`).
    pub fn add_points(&mut self, points: &[(f32, f32)]) -> bool {
        if points.is_empty()
            || !self.brush.radius.is_finite()
            || self.brush.radius <= 0.0
            || self.brush.opacity <= 0.0
        {
            return false;
        }
        self.metrics.points_received += points.len() as u64;
        // Filtering: finite and non-degenerate (same EPSILON rule as the
        // permanent path — a null segment stamps nothing anyway).
        let mut fresh: Vec<(f32, f32)> = Vec::with_capacity(points.len() + 1);
        if let Some(last) = self.last {
            fresh.push(last);
        }
        for &(x, y) in points {
            if !x.is_finite() || !y.is_finite() {
                self.metrics.points_coalesced += 1;
                continue;
            }
            let skip = match fresh.last() {
                None => false,
                Some(&(px, py)) => {
                    let dx = x - px;
                    let dy = y - py;
                    (dx * dx + dy * dy).sqrt() < f32::EPSILON
                }
            };
            if skip {
                self.metrics.points_coalesced += 1;
                continue;
            }
            fresh.push((x, y));
        }
        // First point of an empty gesture: primed as an isolated disc (like
        // the permanent path stamping `points[0]`).
        if fresh.is_empty() {
            return false;
        }
        // Same smoothing as the commit (subdivision + Chaikin on [last known]
        // + new): endpoints are preserved, so inter-packet continuity
        // (`self.last`) is exact.
        let fresh = resample_stroke(&fresh, self.brush.radius.max(2.0));
        let changed = self.stamp_fresh(&fresh);
        if let Some(&p) = fresh.last() {
            self.last = Some(p);
        }
        changed
    }

    /// Stamps `fresh` ([last known] + new), growing the region if needed.
    /// Returns true if any pixels changed.
    fn stamp_fresh(&mut self, fresh: &[(f32, f32)]) -> bool {
        let rx = self.rx.max(0.5);
        let ry = self.ry.max(0.5);
        let Some((x0f, y0f, x1f, y1f)) = stroke_bbox(fresh, rx, ry) else {
            return false;
        };
        let (nx0, ny0) = (x0f.floor() as i32, y0f.floor() as i32);
        let (nx1, ny1) = (x1f.ceil() as i32, y1f.ceil() as i32);
        if nx1 <= nx0 || ny1 <= ny0 {
            return false;
        }
        self.grow_to(nx0, ny0, nx1, ny1);
        // Stride = capacity (the logical `w × h` region lives at the top-left
        // of the origin); clips stay the logical region — padding beyond is
        // never written nor read.
        let stride = self.cap_w as usize;
        if self.dynamics.is_legacy() {
            stamp_polyline(
                &mut self.mask,
                stride,
                self.ox as i64,
                self.oy as i64,
                self.ox as i64,
                self.oy as i64,
                (self.ox + self.w as i32) as i64,
                (self.oy + self.h as i32) as i64,
                fresh,
                rx,
                ry,
            );
        } else {
            // Build-up dabs: the walker carries the spacing across
            // packets, so dabs land where the commit puts them.
            let dynamics = self.dynamics;
            let (ox, oy) = (self.ox as i64, self.oy as i64);
            let clip = (
                ox,
                oy,
                (self.ox + self.w as i32) as i64,
                (self.oy + self.h as i32) as i64,
            );
            let step = dynamics.spacing * 2.0 * rx.max(ry);
            let mask = &mut self.mask;
            self.walker.walk(fresh, step, |cx, cy| {
                crate::paint::stamp_soft_disc(
                    mask, stride, ox, oy, clip, cx, cy, rx, ry, &dynamics,
                );
            });
        }
        // Recomposites the grown zone from the FULL mask (not from the image:
        // each final pixel only depends on the final mask — history-independent
        // exactness).
        let (dx0, dy0) = ((nx0 - self.ox).max(0), (ny0 - self.oy).max(0));
        let dx1 = (nx1 - self.ox).min(self.w as i32);
        let dy1 = (ny1 - self.oy).min(self.h as i32);
        if dx1 > dx0 && dy1 > dy0 {
            let rw = self.cap_w as usize;
            let mask = &self.mask;
            if self.brush.mode == StrokeMode::Erase {
                // No base here: indicative white veil over the coverage (the commit
                // applies destination-out on the real pixels — exact, documented
                // at module level).
                for y in dy0..dy1 {
                    for x in dx0..dx1 {
                        let mi = (y as usize) * stride + (x as usize);
                        if mask[mi] > 0 {
                            let di = ((y as usize) * rw + (x as usize)) * 4;
                            let a = mask[mi];
                            self.image[di] = 255;
                            self.image[di + 1] = 255;
                            self.image[di + 2] = 255;
                            self.image[di + 3] = a;
                        }
                    }
                }
            } else {
                let region_mask: Vec<u8> = (dy0..dy1)
                    .flat_map(|y| {
                        let base = (y as usize) * stride;
                        (dx0..dx1).map(move |x| mask[base + x as usize])
                    })
                    .collect();
                let mut region_img = vec![0u8; ((dx1 - dx0) as usize) * ((dy1 - dy0) as usize) * 4];
                composite_coverage(
                    &mut region_img,
                    (dx1 - dx0) as usize,
                    0,
                    0,
                    (dx1 - dx0) as u32,
                    (dy1 - dy0) as u32,
                    &region_mask,
                    (dx1 - dx0) as usize,
                    &self.brush,
                );
                for y in dy0..dy1 {
                    for x in dx0..dx1 {
                        let si = (((y - dy0) as usize) * ((dx1 - dx0) as usize)
                            + ((x - dx0) as usize))
                            * 4;
                        let di = ((y as usize) * rw + (x as usize)) * 4;
                        self.image[di..di + 4].copy_from_slice(&region_img[si..si + 4]);
                    }
                }
            }
            let area = ((dx1 - dx0) as u64) * ((dy1 - dy0) as u64);
            self.metrics.surface_pixels_written += area;
        }
        self.dirty = true;
        true
    }

    /// Grows the logical region (never shrunk during a gesture). Zero copy
    /// while the union fits the allocated capacity; else geometric
    /// reallocation (×1.5 rounded to 64 px) with a single copy of the old
    /// content. Padding beyond the logical region stays zero (never written:
    /// stamping clips are the logical region)
    /// and is never read (`render` only exports
    /// the logical region).
    fn grow_to(&mut self, nx0: i32, ny0: i32, nx1: i32, ny1: i32) {
        if self.w == 0 || self.h == 0 {
            self.ox = nx0;
            self.oy = ny0;
            self.w = (nx1 - nx0).max(1) as u32;
            self.h = (ny1 - ny0).max(1) as u32;
            self.cap_w = self.w;
            self.cap_h = self.h;
            self.mask = vec![0u8; self.w as usize * self.h as usize];
            self.image = vec![0u8; self.w as usize * self.h as usize * 4];
            return;
        }
        let gx0 = self.ox.min(nx0);
        let gy0 = self.oy.min(ny0);
        let gx1 = (self.ox + self.w as i32).max(nx1);
        let gy1 = (self.oy + self.h as i32).max(ny1);
        if gx0 == self.ox
            && gy0 == self.oy
            && gx1 == self.ox + self.w as i32
            && gy1 == self.oy + self.h as i32
        {
            return;
        }
        // Fits in capacity? Extended logical region, zero copy (indexing
        // already uses `ox`/`oy` + capacity stride).
        if gx0 >= self.ox
            && gy0 >= self.oy
            && gx1 <= self.ox + self.cap_w as i32
            && gy1 <= self.oy + self.cap_h as i32
        {
            self.w = (gx1 - gx0) as u32;
            self.h = (gy1 - gy0) as u32;
            return;
        }
        // Geometric reallocation: amortization makes copies rare (a monotone
        // sweep only reallocates in O(log)).
        let grow = |need: i32| ((need as f32 * 1.5).ceil() as i32).max(need).max(64);
        let gw = grow(gx1 - gx0) as u32;
        let gh = grow(gy1 - gy0) as u32;
        let mut mask = vec![0u8; gw as usize * gh as usize];
        let mut image = vec![0u8; gw as usize * gh as usize * 4];
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                let si = (y as usize) * self.cap_w as usize + x as usize;
                let dx = self.ox + x - gx0;
                let dy = self.oy + y - gy0;
                let di = (dy as usize) * gw as usize + dx as usize;
                mask[di] = self.mask[si];
                image[di * 4..di * 4 + 4].copy_from_slice(&self.image[si * 4..si * 4 + 4]);
            }
        }
        self.ox = gx0;
        self.oy = gy0;
        self.w = (gx1 - gx0) as u32;
        self.h = (gy1 - gy0) as u32;
        self.cap_w = gw;
        self.cap_h = gh;
        self.mask = mask;
        self.image = image;
    }

    /// Renders the transient frame (whole logical region, stroke-bounded
    /// clone). `None` if empty. Tagged with the current generation (see
    /// `is_current`). Touches neither the document nor the worker.
    pub fn render(&mut self) -> Option<OverlayFrame> {
        self.metrics.submissions += 1;
        if self.is_empty() {
            return None;
        }
        let t = std::time::Instant::now();
        // Contiguous export of the logical region (storage follows the
        // capacity stride): allocated padding never travels.
        let (w, h) = (self.w as usize, self.h as usize);
        let stride = self.cap_w as usize;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = (y * stride) * 4;
            let dst = (y * w) * 4;
            rgba[dst..dst + w * 4].copy_from_slice(&self.image[src..src + w * 4]);
        }
        // Selection: alpha scaled by the selected fraction (doc pixels).
        if let Some(clip) = self.clip {
            for y in 0..h {
                for x in 0..w {
                    let alpha = &mut rgba[(y * w + x) * 4 + 3];
                    if *alpha == 0 {
                        continue;
                    }
                    let selected = clip.coverage(
                        self.ox as f32 + x as f32 + 0.5,
                        self.oy as f32 + y as f32 + 0.5,
                    );
                    *alpha = (f32::from(*alpha) * selected).round() as u8;
                }
            }
        }
        let frame = OverlayFrame {
            generation: self.generation,
            x: self.ox,
            y: self.oy,
            width: self.w,
            height: self.h,
            rgba,
        };
        self.metrics.last_render_us = t.elapsed().as_micros();
        self.metrics.completions += 1;
        self.dirty = false;
        Some(frame)
    }

    /// True if fresh content awaits presentation.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty && !self.is_empty()
    }

    /// Current frame geometry `(x, y, w, h, generation)` WITHOUT cloning
    /// pixels: lets the app re-place its texture in cache every frame
    /// (state-only zoom/pan) without re-rendering. `None` if empty
    /// (nothing to display).
    #[must_use]
    pub fn frame_geom(&self) -> Option<(i32, i32, u32, u32, u64)> {
        if self.is_empty() {
            None
        } else {
            Some((self.ox, self.oy, self.w, self.h, self.generation))
        }
    }
}

impl Default for InteractionOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// Is the overlay presentable for this transform? Axis-aligned
/// placements only (identity, translation, scale): rotation and shear
/// would require oriented rasterization presentation-side (extension
/// gate — the engine stays exact in all cases, and the commit never
/// depends on this gate).
#[must_use]
pub fn overlay_displayable(t: &Transform2D) -> bool {
    if t.has_skew() {
        return false;
    }
    // Same epsilon as the render path (freely rounded rotations = visual
    // error).
    let rot = t.rotation_deg.rem_euclid(360.0);
    !(0.01..=359.99).contains(&rot)
}

/// Screen placement of an overlay frame (pure, tested math): the overlay
/// (doc px, origin included) follows exactly the same scaling as the
/// preview, anchored to the preview dest corner.
///
/// * `dest_min`: screen corner of the drawn preview rect;
/// * `screen_per_doc`: screen px per document px (same factor as preview);
/// * `ox/oy/w/h`: overlay region in document px.
///
/// Returns `(x, y, w, h)` screen.
#[must_use]
pub fn overlay_screen_rect(
    dest_min_x: f32,
    dest_min_y: f32,
    screen_per_doc: f32,
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
) -> (f32, f32, f32, f32) {
    (
        dest_min_x + ox as f32 * screen_per_doc,
        dest_min_y + oy as f32 * screen_per_doc,
        w as f32 * screen_per_doc,
        h as f32 * screen_per_doc,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::paint_stroke_rgba;

    fn brush_paint() -> BrushParams {
        BrushParams {
            radius: 4.0,
            color: [255, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        }
    }

    /// Source-over blit of an overlay frame onto a doc buffer (mirrors the
    /// display composition: stroke-only over the composite).
    fn blit_over(dst: &mut [u8], dst_w: u32, frame: &OverlayFrame) {
        for y in 0..frame.height {
            for x in 0..frame.width {
                let si = ((y as usize) * frame.width as usize + x as usize) * 4;
                let dx = frame.x + x as i32;
                let dy = frame.y + y as i32;
                if dx < 0 || dy < 0 {
                    continue;
                }
                let di = ((dy as usize) * dst_w as usize + dx as usize) * 4;
                if di + 3 >= dst.len() || si + 3 >= frame.rgba.len() {
                    continue;
                }
                let sa = frame.rgba[si + 3] as f32 / 255.0;
                if sa <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    let v = frame.rgba[si + c] as f32 * sa + dst[di + c] as f32 * (1.0 - sa);
                    dst[di + c] = v.round().clamp(0.0, 255.0) as u8;
                }
                let da = dst[di + 3] as f32 / 255.0;
                dst[di + 3] = ((sa + da * (1.0 - sa)) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    #[test]
    fn feedback_non_vide_avant_commit() {
        // §15: MouseDown + MouseMove ⇒ non-empty overlay BEFORE any MouseUp
        // (no worker, no document touched here — pure engine).
        let mut overlay = InteractionOverlay::new();
        assert!(overlay.is_empty());
        overlay.begin_stroke(brush_paint());
        assert!(overlay.is_empty(), "rien tamponné tant qu'aucun point");
        assert!(overlay.add_points(&[(10.0, 10.0), (20.0, 20.0)]));
        let frame = overlay.render().expect("overlay non vide avant commit");
        assert!(!frame.rgba.iter().all(|&v| v == 0), "pixels visibles");
        assert!(!overlay.is_dirty(), "render consomme le dirty");
        assert_eq!(overlay.metrics().points_received, 2);
    }

    #[test]
    fn morceaux_equivalent_appel_unique() {
        // §5/§15: 10 points at once == in 3 pieces (continuity via last point,
        // idempotent union) — including with overlap.
        let points: Vec<(f32, f32)> = (0..10).map(|i| (8.0 + i as f32 * 3.0, 30.0)).collect();
        let brush = brush_paint();
        let mut whole = InteractionOverlay::new();
        whole.begin_stroke(brush);
        whole.add_points(&points);
        let whole = whole.render().expect("frame");
        let mut chunked = InteractionOverlay::new();
        chunked.begin_stroke(brush);
        chunked.add_points(&points[0..4]);
        chunked.add_points(&points[3..7]);
        chunked.add_points(&points[5..10]);
        // Deliberate overlap ([3..7] then [5..10]): idempotent.
        chunked.add_points(&points[4..6]);
        let chunked = chunked.render().expect("frame");
        assert_eq!(
            (whole.x, whole.y, whole.width, whole.height),
            (chunked.x, chunked.y, chunked.width, chunked.height)
        );
        assert_eq!(whole.rgba, chunked.rgba, "morceaux == unique");
    }

    #[test]
    fn transient_proche_du_commit_avec_lissage() {
        // §5: overlay (transient) + permanent commit == reference,
        // modulo the packet junction. Both go through the SAME
        // `resample_stroke` as the worker (`PaintStroke`): the transient
        // shows the same curves as the commit. Only structural difference:
        // global Chaikin drops interior points (cut corners) while packet
        // splitting anchors each junction (disc stamped at the raw point).
        // The gap is therefore bounded by the discs (brush radius) around
        // interior points — invisible, and replaced by the exact commit
        // at `MouseUp`.
        use crate::paint::resample_stroke;
        for (brush, w, h) in [
            (brush_paint(), 64u32, 64u32),
            (
                BrushParams {
                    radius: 6.0,
                    color: [0, 255, 0],
                    opacity: 0.5,
                    mode: StrokeMode::Paint,
                },
                96u32,
                64u32,
            ),
        ] {
            let points: Vec<(f32, f32)> =
                vec![(10.0, 10.0), (30.0, 25.0), (50.0, 10.0), (55.0, 40.0)];
            // Reference: direct commit on opaque background (resample included,
            // like the worker's `PaintStroke` arm).
            let mut base = vec![0u8; w as usize * h as usize * 4];
            for px in base.as_chunks_mut::<4>().0 {
                px[0] = 200;
                px[1] = 40;
                px[2] = 40;
                px[3] = 255;
            }
            let mut reference = base.clone();
            let smooth = resample_stroke(&points, brush.radius.max(2.0));
            paint_stroke_rgba(&mut reference, w, h, &smooth, &brush);
            // Fragmented overlay (2 calls, junction at `points[1]`).
            let mut overlay = InteractionOverlay::new();
            overlay.begin_stroke(brush);
            overlay.add_points(&points[0..2]);
            overlay.add_points(&points[2..4]);
            let frame = overlay.render().expect("frame");
            // Recomposite overlay over the base (same op as transient display)
            // and compare to the reference.
            let mut shown = base.clone();
            blit_over(&mut shown, w, &frame);
            let mut ndiff = 0usize;
            for (i, (a, b)) in shown.iter().zip(reference.iter()).enumerate() {
                if a == b {
                    continue;
                }
                let px = i / 4;
                let (x, y) = (px % w as usize, px / w as usize);
                let near_junction = points[1..points.len() - 1].iter().any(|&(jx, jy)| {
                    let dx = x as f32 - jx;
                    let dy = y as f32 - jy;
                    (dx * dx + dy * dy).sqrt() <= brush.radius + 1.0
                });
                assert!(
                    near_junction,
                    "pixel ({x},{y}) loin de toute jonction : divergence non bornée"
                );
                ndiff += 1;
            }
            let bound =
                (std::f32::consts::PI * (brush.radius + 2.0).powi(2) * (points.len() - 2) as f32)
                    .ceil() as usize;
            assert!(
                ndiff <= bound,
                "écart {ndiff} > aire des disques de jonction ({bound})"
            );
        }
    }

    #[test]
    fn cancel_ne_touche_rien() {
        // §15: cancel ⇒ empty overlay, no other trace (the document is never
        // touched by construction — here: rearmed internal state).
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(10.0, 10.0), (40.0, 40.0)]);
        assert!(!overlay.is_empty());
        let generation = overlay.generation();
        overlay.clear();
        assert!(overlay.is_empty());
        assert!(overlay.render().is_none());
        assert_eq!(
            overlay.generation(),
            generation,
            "clear ne touche pas la génération"
        );
        assert!(!overlay.is_dirty());
    }

    #[test]
    fn generation_rejette_perime() {
        // §14: completion A arriving after B ⇒ A discarded.
        let mut overlay = InteractionOverlay::new();
        let generation_a = overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(5.0, 5.0)]);
        let frame_a = overlay.render().expect("frame A");
        assert!(overlay.is_current(frame_a.generation));
        assert_eq!(frame_a.generation, generation_a);
        let generation_b = overlay.begin_stroke(brush_paint());
        assert_ne!(generation_a, generation_b);
        assert!(!overlay.is_current(frame_a.generation), "A périmée après B");
        // New session restarts empty (no residue).
        assert!(overlay.is_empty());
    }

    #[test]
    fn gate_display_transforms() {
        // §11: identity/translation/scale OK; rotation/skew refused
        // presentation-side (engine exact anyway).
        use crate::document::Transform2D;
        assert!(overlay_displayable(&Transform2D::default()));
        assert!(overlay_displayable(&Transform2D {
            offset_x: 30.0,
            offset_y: -12.0,
            ..Transform2D::default()
        }));
        assert!(overlay_displayable(&Transform2D {
            scale_x: 2.0,
            scale_y: 0.5,
            ..Transform2D::default()
        }));
        assert!(!overlay_displayable(&Transform2D {
            rotation_deg: 90.0,
            ..Transform2D::default()
        }));
        assert!(!overlay_displayable(&Transform2D {
            rotation_deg: 0.02,
            ..Transform2D::default()
        }));
        assert!(!overlay_displayable(&Transform2D {
            skew_x: 5.0,
            ..Transform2D::default()
        }));
    }

    #[test]
    fn placement_ecran_suit_preview() {
        // The overlay follows the same scale as the preview, anchored at the corner.
        assert_eq!(
            overlay_screen_rect(100.0, 50.0, 2.0, 10, 20, 30, 40),
            (120.0, 90.0, 60.0, 80.0)
        );
        assert_eq!(
            overlay_screen_rect(0.0, 0.0, 0.5, 0, 0, 64, 64),
            (0.0, 0.0, 32.0, 32.0)
        );
    }

    #[test]
    fn points_degeneres_coalesces() {
        // §8/§13: duplicated and non-finite points counted, no effect.
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(10.0, 10.0), (10.0, 10.0), (f32::NAN, 0.0), (20.0, 20.0)]);
        let m = overlay.metrics();
        assert_eq!(m.points_received, 4);
        assert_eq!(m.points_coalesced, 2, "doublon + NaN");
        assert!(!overlay.is_empty());
        // Invalid brush: total no-op.
        let mut bad = InteractionOverlay::new();
        bad.begin_stroke(BrushParams {
            radius: 0.0,
            color: [0, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        assert!(!bad.add_points(&[(5.0, 5.0)]));
        assert!(bad.is_empty());
    }

    #[test]
    fn frame_geom_reexpose_sans_recloner() {
        // Persistent-redraw support app-side: geometry stays available after
        // `render()` (which consumed the dirty) without touching pixels, and
        // disappears at `clear()`.
        let mut overlay = InteractionOverlay::new();
        assert!(overlay.frame_geom().is_none());
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(10.0, 10.0), (20.0, 20.0)]);
        let frame = overlay.render().expect("frame");
        let geom = overlay.frame_geom().expect("géométrie conservée");
        assert_eq!(
            (geom.0, geom.1, geom.2, geom.3),
            (frame.x, frame.y, frame.width, frame.height)
        );
        assert_eq!(geom.4, frame.generation);
        assert!(overlay.is_current(geom.4));
        overlay.clear();
        assert!(overlay.frame_geom().is_none());
    }

    #[test]
    fn region_bornee_au_trait_grands_documents() {
        // 1920x1080, 2048², 4096²: a SMALL stroke stays small — the overlay
        // region is O(stroke), never O(document).
        for (doc_w, doc_h) in [(1920.0, 1080.0), (2048.0, 2048.0), (4096.0, 4096.0)] {
            let points = ligne(100.0, 100.0, 300.0, 250.0, 16);
            let mut overlay = InteractionOverlay::new();
            overlay.begin_stroke(BrushParams {
                radius: 8.0,
                color: [200, 30, 30],
                opacity: 1.0,
                mode: StrokeMode::Paint,
            });
            for chunk in points.chunks(8) {
                overlay.add_points(chunk);
            }
            let frame = overlay.render().expect("frame non vide");
            let area = u64::from(frame.width) * u64::from(frame.height);
            let doc_area = (doc_w as u64) * (doc_h as u64);
            assert!(
                area * 20 < doc_area,
                "région {w}x{h} bornée au trait (< 5 % du document {doc_w}x{doc_h})",
                w = frame.width,
                h = frame.height,
            );
            assert!(
                frame.x >= 0 && frame.y >= 0,
                "région dans l'espace document {doc_w}x{doc_h}"
            );
            // Bounded memory: region-only mask + image.
            assert_eq!(
                frame.rgba.len(),
                frame.width as usize * frame.height as usize * 4
            );
        }
    }
    /// Measures a gesture: incremental `add_points` (8-point packets, like
    /// a drag) + final `render()`. Returns total wall time (µs) and the
    /// overlay region size. NEVER assert on wall time (hardware varies).
    fn mesure_geste(points: &[(f32, f32)], radius: f32) -> (u128, u32, u32) {
        use std::time::Instant;
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(BrushParams {
            radius,
            color: [200, 30, 30],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        let start = Instant::now();
        for chunk in points.chunks(8) {
            overlay.add_points(chunk);
        }
        let frame = overlay.render().expect("frame non vide");
        (start.elapsed().as_micros(), frame.width, frame.height)
    }

    fn ligne(x0: f32, y0: f32, x1: f32, y1: f32, n: usize) -> Vec<(f32, f32)> {
        (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1).max(1) as f32;
                (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)
            })
            .collect()
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_geste_court_1920() {
        // Small brush, 1920x1080 diagonal: the common case.
        let points = ligne(100.0, 100.0, 1800.0, 950.0, 64);
        let (us, w, h) = mesure_geste(&points, 8.0);
        eprintln!("geste-court-1920 : {us} µs pour 64 pts r=8 (région {w}x{h})");
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_geste_long_gros_pinceau() {
        // Big brush, 2048² crossing: region upper bound.
        let points = ligne(0.0, 0.0, 2048.0, 2048.0, 512);
        let (us, w, h) = mesure_geste(&points, 64.0);
        eprintln!("geste-long-2048 : {us} µs pour 512 pts r=64 (région {w}x{h})");
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_frame_drag_courant() {
        // True interactive regime: 8 pts + render PER FRAME (10 frames), average
        // wall vs the 16 ms budget. The overlay is pre-warmed (region already
        // allocated — like mid-gesture).
        use std::time::Instant;
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(BrushParams {
            radius: 8.0,
            color: [200, 30, 30],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        overlay.add_points(&ligne(100.0, 100.0, 300.0, 250.0, 16));
        let _ = overlay.render();
        let start = Instant::now();
        for frame in 0..10 {
            let t0 = 320.0 + frame as f32 * 24.0;
            overlay.add_points(&ligne(t0, 260.0, t0 + 24.0, 280.0, 8));
            let rendered = overlay.render().expect("frame");
            assert!(overlay.is_current(rendered.generation));
        }
        let mean_us = start.elapsed().as_micros() / 10;
        eprintln!("frame-drag : {mean_us} µs/frame (8 pts + render, r=8)");
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_geste_coords_4096() {
        // 4096² space coordinates (exact scaled placement, §10).
        let points = ligne(10.0, 10.0, 4086.0, 4086.0, 256);
        let (us, w, h) = mesure_geste(&points, 12.0);
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&points);
        let frame = overlay.render().expect("frame");
        assert!(
            frame.x >= 0 && frame.y >= 0,
            "région dans l'espace 4096², pas de débordement"
        );
        eprintln!("geste-coords-4096 : {us} µs pour 256 pts r=12 (région {w}x{h})");
    }

    #[test]
    fn gomme_voile_indicatif() {
        // Erase: white veil over the coverage (indicator, exact commit
        // elsewhere) — never an invented black pixel.
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(BrushParams {
            radius: 4.0,
            color: [0, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Erase,
        });
        overlay.add_points(&[(10.0, 10.0), (20.0, 20.0)]);
        let frame = overlay.render().expect("frame");
        assert!(!frame.rgba.iter().all(|&v| v == 0), "voile visible");
        for px in frame.rgba.as_chunks::<4>().0 {
            if px[3] > 0 {
                assert_eq!(&px[0..3], &[255, 255, 255], "que du blanc + alpha");
            }
        }
    }

    #[test]
    fn overlay_feedback_follows_the_selection() {
        let mut overlay = InteractionOverlay::new();
        overlay.set_clip(crate::Selection::from_drag(
            crate::SelectionKind::Rect,
            (0.0, 0.0),
            (20.0, 40.0),
        ));
        overlay.begin_stroke(BrushParams {
            radius: 3.0,
            color: [255, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        overlay.add_points(&[(10.0, 10.0), (30.0, 10.0)]);
        let frame = overlay.render().expect("frame");
        let alpha = |x: i32, y: i32| {
            let (lx, ly) = ((x - frame.x) as usize, (y - frame.y) as usize);
            frame.rgba[(ly * frame.width as usize + lx) * 4 + 3]
        };
        assert!(alpha(12, 10) > 0, "inside: visible");
        assert_eq!(alpha(28, 10), 0, "outside: clipped");
    }
}
