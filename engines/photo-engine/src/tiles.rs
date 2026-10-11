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

//! Tile integration point: a gesture's dirty region, without touching
//!
//! the renderer. [`stroke_dirty_region`] converts a stroke (points +
//! radius, image space) into a [`TileRegion`] ready for `TileGrid::invalidate`:
//! gesture bbox, grown by the brush radius THEN the effect [`Padding`] (active
//! filters' neighborhood — blur…).
//!
//! Target progressive pipeline (current renderer unchanged: it still
//! composites full-frame; tiles order and observe):
//!
//! ```text
//! stroke → stroke_dirty_region → grid.invalidate → DirtyTiles::mark_range
//!     → TileScheduler::plan_into → progressive execution → existing renderer
//! ```
//!
//! PURE function (no engine state read or mutated): the renderer
//! replacement will come later; the dirty geometry is available
//! right now.
//!
//! COORDINATE RULE (Phase 6B): any region sent to `TileGrid` / `DirtyTiles` /
//! `TileScheduler` / `tiles_for_rect` is in DOCUMENT SPACE. Paint-stroke
//! points stay in LAYER SPACE: explicit conversion goes through the layer's
//! [`Transform2D`] ([`stroke_footprint_in_document`],
//! [`plan_stroke_tiles_layer_space`]).
//! Rasterization (`paint_stroke_rgba`, layer space) is unchanged.

use tiles::{DirtyTiles, TileGrid, TileScheduler, Viewport};

pub use tiles::{Padding, TileRegion};

use crate::document::compositing::blur_support_px;
use crate::document::{FilterLayer, GroupLayer, LayerNode, Transform2D};

/// Dirty region of a brush/eraser stroke (`None` if unusable:
/// no points, non-finite or zero radius, non-finite coordinates).
///
/// `points` in image pixels, `radius` in pixels, `pad` = effect margin
/// (e.g. the active filter's [`Padding::for_blur_radius`] — 0 otherwise).
/// The region may exceed the surface: clipping belongs to the grid.
#[must_use]
pub fn stroke_dirty_region(points: &[(f32, f32)], radius: f32, pad: Padding) -> Option<TileRegion> {
    if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    // Conservative coverage: floor at min, ceil at max (+ radius).
    let region = TileRegion::new(
        (x0 - radius).floor() as i32,
        (y0 - radius).floor() as i32,
        ((x1 + radius).ceil() - (x0 - radius).floor()).max(1.0) as u32,
        ((y1 + radius).ceil() - (y0 - radius).floor()).max(1.0) as u32,
    );
    Some(region.pad(pad))
}

/// Incremental-pipeline tile size (Phase 6E, single level 0): centralized
/// here — observation (6B), viewport-render and assembly grids share the
/// same grain (256 px, like the foundation tests).
pub const VIEWPORT_TILE_PX: u32 = 256;

/// Transform clamped like the renderer (`prepare_top` / `extents_visit`:
/// 0.05..=8.0 scales): the invalidation footprint covers the same zone as
/// the render, never less.
fn clamped_transform(t: &Transform2D) -> Transform2D {
    Transform2D {
        scale_x: t.scale_x.clamp(0.05, 8.0),
        scale_y: t.scale_y.clamp(0.05, 8.0),
        ..*t
    }
}

/// Conservative integer bounds of a float quad (`floor` at min, `ceil` at
/// max — same convention as tiles). `EMPTY` if non-finite.
fn quad_to_region(quad: &[(f32, f32); 4]) -> TileRegion {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &(x, y) in quad {
        if !x.is_finite() || !y.is_finite() {
            return TileRegion::EMPTY;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if max_x <= min_x || max_y <= min_y {
        return TileRegion::EMPTY;
    }
    TileRegion::new(
        min_x.floor() as i32,
        min_y.floor() as i32,
        (max_x.ceil() - min_x.floor()).max(1.0) as u32,
        (max_y.ceil() - min_y.floor()).max(1.0) as u32,
    )
}

/// A whole layer's footprint in DOCUMENT SPACE: the image's 4 corners
/// (`w0 × h0`, layer space) via [`Transform2D::doc_corners`] — exactly the
/// same computation as renderer-side `extents_visit` — then bounds.
///
/// `EMPTY` if null dimensions (no pixels to invalidate).
#[must_use]
pub fn layer_footprint_in_document(transform: &Transform2D, w0: u32, h0: u32) -> TileRegion {
    if w0 == 0 || h0 == 0 {
        return TileRegion::EMPTY;
    }
    let clamped = clamped_transform(transform);
    quad_to_region(&clamped.doc_corners(w0 as f32, h0 as f32))
}

/// A stroke's footprint (LAYER SPACE) in DOCUMENT SPACE: the stroke bbox
/// grown by the radius **in layer space**, then corners transformed by
/// the `Transform2D` (translation, rotation, scale, skew — no plain offset
/// addition) and conservative bounds.
///
/// `None` if unusable (no points, non-finite or zero radius, non-finite
/// coordinate). `w0`/`h0` = layer image dimensions (transform center, like
/// `local_to_doc`). Without halo: apply
/// [`expand_region_by_halo`] after.
#[must_use]
pub fn stroke_footprint_in_document(
    points: &[(f32, f32)],
    radius: f32,
    transform: &Transform2D,
    w0: u32,
    h0: u32,
) -> Option<TileRegion> {
    if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    // Radius kept BEFORE conversion: the grown transformed box covers the
    // true dilated stroke (T(true) ⊆ T(box) ⊆ bounds).
    let clamped = clamped_transform(transform);
    let (w0f, h0f) = (w0 as f32, h0 as f32);
    let quad = [
        clamped.local_to_doc(w0f, h0f, x0 - radius, y0 - radius),
        clamped.local_to_doc(w0f, h0f, x1 + radius, y0 - radius),
        clamped.local_to_doc(w0f, h0f, x1 + radius, y1 + radius),
        clamped.local_to_doc(w0f, h0f, x0 - radius, y1 + radius),
    ];
    let region = quad_to_region(&quad);
    if region.is_empty() {
        return None;
    }
    Some(region)
}

/// Expansion of a DOCUMENT SPACE region by a halo (effect neighborhood).
/// Pure `pad`: document clipping belongs to the grid (`TileGrid::invalidate`),
/// not here.
#[must_use]
pub fn expand_region_by_halo(region: &TileRegion, halo: Padding) -> TileRegion {
    region.pad(halo)
}

/// Dirty region of a geometric move: `old ∪ new` (both footprints, then
/// halo). Any mutation moving the produced zone (transform, resize,
/// rotation) must invalidate the union, never the new position
/// alone.
#[must_use]
pub fn layer_move_dirty_region(
    old_transform: &Transform2D,
    new_transform: &Transform2D,
    w0: u32,
    h0: u32,
    halo: Padding,
) -> TileRegion {
    expand_region_by_halo(
        &layer_footprint_in_document(old_transform, w0, h0).union(layer_footprint_in_document(
            new_transform,
            w0,
            h0,
        )),
        halo,
    )
}

/// Conservative halo of a filter sub-layer chain: sum of ACTIVE blur radii
/// (`Blur(r1) → Blur(r2)` ⇒ `r1 + r2`, sequential chain). Only the `blur`
/// effect (float `radius` param) is neighborhood; everything else (and any
/// empty chain) ⇒ zero halo.
///
/// Local reads of existing structures: no effect refactor, no new
/// system — `Padding::for_blur_radius` per radius.
#[must_use]
pub fn blur_halo_for_filters(filters: &[FilterLayer]) -> Padding {
    let mut total = 0.0f32;
    for f in filters {
        if !f.enabled || f.type_id != "blur" {
            continue;
        }
        if let Some(datatypes::ParamValue::Float(r)) = f.params.get("radius")
            && r.is_finite()
            && *r > 0.0
        {
            total += *r;
        }
    }
    Padding::for_blur_radius(total)
}

/// Observable tile plan for a stroke: pure counters + bounding region,
/// no pixels touched, no render driven. The worker records these counters
/// in its metrics; the current renderer runs unchanged
/// alongside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeTilePlan {
    /// Dirty region area in pixels (0 if unusable gesture).
    pub dirty_region_px: u64,
    /// Tiles marked dirty at level 0 (after document clipping).
    pub dirty_tiles: u64,
    /// Tiles kept by the scheduler (open budget: == dirty_tiles).
    pub scheduled_tiles: u64,
    /// Bounding region of dirty tiles, in DOCUMENT coordinates, clamped to
    /// the document (`TileRegion::EMPTY` if no tiles). It alone — nothing
    /// else — is consumed by a regional render.
    pub bounds: TileRegion,
}

impl Default for StrokeTilePlan {
    fn default() -> Self {
        Self {
            dirty_region_px: 0,
            dirty_tiles: 0,
            scheduled_tiles: 0,
            bounds: TileRegion::EMPTY,
        }
    }
}

/// Computes a stroke's observable plan: region → document grid → dirty →
/// scheduler. Pure and deterministic (same inputs ⇒ same plan).
/// `doc_w`/`doc_h` = document dimensions (composite space); null values
/// clamped to 1 px. Never `None`: an unusable gesture yields a null plan
/// (zeros), not an error.
#[must_use]
pub fn plan_stroke_tiles(
    points: &[(f32, f32)],
    radius: f32,
    doc_w: u32,
    doc_h: u32,
) -> StrokeTilePlan {
    let mut plan = StrokeTilePlan::default();
    let Some(region) = stroke_dirty_region(points, radius, Padding::ZERO) else {
        return plan;
    };
    plan_region_tiles(&mut plan, &region, doc_w, doc_h);
    plan
}

/// A LAYER SPACE stroke's plan with layer transform: document footprint
/// ([`stroke_footprint_in_document`]) grown by `halo` (document space,
/// e.g. [`blur_halo_for_filters`]), then `tiles_for_rect` / `DirtyTiles`
/// via the same plan as [`plan_stroke_tiles`].
///
/// `layer_w`/`layer_h` = layer image dimensions. Pure and deterministic.
/// Identity transform ⇒ region equivalent to [`plan_stroke_tiles`] (the
/// `identity` test below).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn plan_stroke_tiles_layer_space(
    points: &[(f32, f32)],
    radius: f32,
    transform: &Transform2D,
    layer_w: u32,
    layer_h: u32,
    doc_w: u32,
    doc_h: u32,
    halo: Padding,
) -> StrokeTilePlan {
    let mut plan = StrokeTilePlan::default();
    let Some(footprint) = stroke_footprint_in_document(points, radius, transform, layer_w, layer_h)
    else {
        return plan;
    };
    let region = expand_region_by_halo(&footprint, halo);
    plan_region_tiles(&mut plan, &region, doc_w, doc_h);
    plan
}

/// Plans' shared core: DOCUMENT SPACE region → grid → dirty → scheduler.
/// `region` may exceed the surface: clipping belongs to the grid
/// (`invalidate` + `to_region` already clamped to the document).
fn plan_region_tiles(plan: &mut StrokeTilePlan, region: &TileRegion, doc_w: u32, doc_h: u32) {
    plan.dirty_region_px = region.area();
    let (doc_w, doc_h) = (doc_w.max(1), doc_h.max(1));
    let grid = TileGrid::new(VIEWPORT_TILE_PX, doc_w, doc_h, 1);
    let range = grid.invalidate(region, 0, Padding::ZERO);
    let mut dirty = DirtyTiles::new(&grid);
    plan.dirty_tiles = dirty.mark_range(range);
    // Bounding region of dirty tiles (union already clamped to the document
    // by `to_region`): regional render input, no tile list.
    plan.bounds = range.to_region(&grid);
    // Observation viewport: whole document visible, focus at the gesture
    // center — deterministic, no UI state threaded down here.
    let visible = TileRegion::new(0, 0, doc_w, doc_h);
    let focus = (
        region
            .x
            .saturating_add((region.width / 2).min(i32::MAX as u32) as i32),
        region
            .y
            .saturating_add((region.height / 2).min(i32::MAX as u32) as i32),
    );
    let view = Viewport::new(visible, focus);
    plan.scheduled_tiles = TileScheduler.plan(&dirty, &grid, &view, None).len() as u64;
}

/// FULL_FRAME ↔ REGIONAL fallback policy (vertical slice):
/// deliberately trivial — no complex heuristic threshold.
///
/// * empty plan / invalid region (`dirty_px == 0`) → full frame;
/// * region covering all or more than the frame (`dirty_px >= scope_px`)
///   → full frame (no gain in cropping);
/// * else → regional rendering is supported, use it.
///
/// `scope_px` = full-frame accumulator pixels (see `CompositeStats::scope_px`).
/// No incompatible-GPU / unsupported-renderer cases here: regional reuses
/// exactly the same CPU pass (same full-size appearances, no added readback)
/// — documented
/// in `Document::composite_region_with`.
#[must_use]
pub fn use_regional_render(dirty_px: u64, scope_px: u64) -> bool {
    dirty_px > 0 && dirty_px < scope_px
}

// Phase 6E — viewport grid, node footprints, appearance spread.
// Phase 6E — viewport grid, node footprints, appearance spread.
// ---------------------------------------------------------------------------

/// EXACT spatial spread of a filter sub-layer chain (appearances):
/// summed real supports (`blur_support_px` 6C) of ACTIVE blurs.
/// Distinct from [`blur_halo_for_filters`] (6B `ceil(r)` invalidation
/// convention, enough for observation but not for cache validity):
/// under-marking would yield stale pixels here, so the kernel's exact
/// support is required. Reuses no halo computation — per-effect
/// `blur_support_px`.
#[must_use]
pub fn appearance_spread(filters: &[FilterLayer]) -> Padding {
    let mut total = 0u32;
    for f in filters {
        if !f.enabled || f.type_id != "blur" {
            continue;
        }
        if let Some(datatypes::ParamValue::Float(r)) = f.params.get("radius") {
            total = total.saturating_add(blur_support_px(*r));
        }
    }
    Padding::new(total)
}

/// A node's DOCUMENT SPACE footprint: pixel layer via its transform
/// (same corners as the renderer), UNION with its clipped children (same
/// space); group = recursive union of children (child transforms in
/// canvas coordinates, like compositing).
/// Adjustment: `EMPTY` — an adjustment applies to the whole accumulator,
/// the caller must use a global fallback (never an empty region).
#[must_use]
pub fn node_footprint(node: &LayerNode) -> TileRegion {
    match node {
        LayerNode::Pixel(l) => {
            let (w, h) = l.dimensions();
            let own = layer_footprint_in_document(&l.transform, w, h);
            own.union(children_footprint(&l.children))
        }
        LayerNode::Group(g) => group_footprint(g),
        LayerNode::Adjustment(_) => TileRegion::EMPTY,
    }
}

/// Union of a siblinghood's footprints (recursive, all carriers).
#[must_use]
fn children_footprint(nodes: &[LayerNode]) -> TileRegion {
    let mut region = TileRegion::EMPTY;
    for node in nodes {
        region = region.union(node_footprint(node));
    }
    region
}

/// Union of a group's children footprints (recursive).
#[must_use]
pub fn group_footprint(group: &GroupLayer) -> TileRegion {
    let mut region = TileRegion::EMPTY;
    group_footprint_into(&group.children, &mut region);
    region
}

/// A node's invalidation zone (Phase 6E): footprint grown by the EXACT
/// spread of its appearance chain (`appearance_spread`).
/// Pixels: union with clipped-children zones. Groups: recursive union of
/// child zones, grown by their nested filters' spread.
/// Adjustment: `EMPTY` — it applies to the whole accumulator; the caller
/// uses a global fallback.
#[must_use]
pub fn node_mark_region(node: &LayerNode) -> TileRegion {
    match node {
        LayerNode::Pixel(l) => {
            let (w, h) = l.dimensions();
            let own = layer_footprint_in_document(&l.transform, w, h)
                .pad(appearance_spread(&l.filter_layers));
            own.union(children_mark_region(&l.children))
        }
        LayerNode::Group(g) => {
            let mut region = TileRegion::EMPTY;
            group_mark_into(&g.children, &mut region);
            region.pad(appearance_spread(&g.filter_layers))
        }
        LayerNode::Adjustment(_) => TileRegion::EMPTY,
    }
}

/// Union of a siblinghood's invalidation zones (recursive).
#[must_use]
fn children_mark_region(nodes: &[LayerNode]) -> TileRegion {
    let mut region = TileRegion::EMPTY;
    for node in nodes {
        region = region.union(node_mark_region(node));
    }
    region
}

fn group_mark_into(nodes: &[LayerNode], region: &mut TileRegion) {
    for node in nodes {
        match node {
            LayerNode::Pixel(_) | LayerNode::Group(_) => {
                *region = region.union(node_mark_region(node));
            }
            LayerNode::Adjustment(_) => {}
        }
    }
}

fn group_footprint_into(nodes: &[LayerNode], region: &mut TileRegion) {
    for node in nodes {
        match node {
            LayerNode::Pixel(l) => {
                let (w, h) = l.dimensions();
                let own = layer_footprint_in_document(&l.transform, w, h);
                *region = region.union(own.union(children_footprint(&l.children)));
            }
            LayerNode::Group(g) => group_footprint_into(&g.children, region),
            // Nested adjustment: global fallback decided by the caller
            // (scope_adjustment_window), not a silent empty footprint.
            LayerNode::Adjustment(_) => {}
        }
    }
}

/// Canonical tile coordinate (Phase 6F): positive-or-zero grid indices
/// in a `(0,0)`-origin paving at `tile_px` step.
///
/// Distinct from `tiles::TileCoord` (foundation, signed `i32` for
/// overflowing content): here the Positive identity of rendered tiles,
/// stable whatever the requesting viewport.
///
/// A tile's canonical identity is its coordinate (+ step): the rectangle
/// derives ([`tile_rect`]), never the reverse. Two viewports covering the
/// same tile designate the same coordinate — hence the same cache key
/// (§4 6F: no double computation along the request path).
/// Outside-document zones (negative indices) have no coordinate: they
/// are ignored (never rendered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    /// Column (x / tile_px).
    pub x: u32,
    /// Row (y / tile_px).
    pub y: u32,
}

impl TileCoord {
    /// New coordinate.
    #[must_use]
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// Center in document pixels (for distance sorts, integers).
    #[must_use]
    pub fn center(self, tile_px: u32) -> (i64, i64) {
        let t = i64::from(tile_px.max(1));
        (i64::from(self.x) * t + t / 2, i64::from(self.y) * t + t / 2)
    }
}

/// Canonical (full, unclipped) rectangle of a coordinate at `tile_px` step.
#[must_use]
pub fn tile_rect(coord: TileCoord, tile_px: u32) -> TileRegion {
    let t = tile_px.max(1);
    TileRegion::new(
        (i64::from(coord.x) * i64::from(t)).min(i64::from(i32::MAX)) as i32,
        (i64::from(coord.y) * i64::from(t)).min(i64::from(i32::MAX)) as i32,
        t,
        t,
    )
}

/// Coordinates covering `rect` (DOCUMENT SPACE) at `tile_px` step, in rows.
/// Outside-document (negative) parts are ignored; empty if nothing is
/// covered. Deterministic.
#[must_use]
pub fn tile_coords_for_rect(rect: &TileRegion, tile_px: u32) -> Vec<TileCoord> {
    let t = i64::from(tile_px.max(1));
    if rect.is_empty() {
        return Vec::new();
    }
    // Last covered pixel (exclusive bound − 1), floored at 0.
    let x1 = i64::from(rect.x) + i64::from(rect.width) - 1;
    let y1 = i64::from(rect.y) + i64::from(rect.height) - 1;
    if x1 < 0 || y1 < 0 {
        return Vec::new();
    }
    let tx0 = i64::from(rect.x).div_euclid(t).max(0);
    let ty0 = i64::from(rect.y).div_euclid(t).max(0);
    let tx1 = x1.div_euclid(t);
    let ty1 = y1.div_euclid(t);
    let mut out = Vec::new();
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            // u32: tx/ty ≥ 0 here; guard rail against absurd documents.
            if tx <= i64::from(u32::MAX) && ty <= i64::from(u32::MAX) {
                out.push(TileCoord::new(tx as u32, ty as u32));
            }
        }
    }
    out
}

/// Cuts a viewport (DOCUMENT SPACE) into `tile_px` tiles (default
/// [`VIEWPORT_TILE_PX`), clipped to the `doc_w × doc_h` document, in rows.
/// Empty if the viewport is empty or outside the document. Tiles partition
/// the covered zone with no gap or overlap (exact integers).
///
/// Implemented over [`tile_coords_for_rect`]: same canonical paving as the
/// scheduler — a tile keeps the same identity whatever the viewport.
#[must_use]
pub fn viewport_tiles(
    viewport: &TileRegion,
    doc_w: u32,
    doc_h: u32,
    tile_px: u32,
) -> Vec<TileRegion> {
    let doc = TileRegion::new(0, 0, doc_w.max(1), doc_h.max(1));
    let visible = viewport.intersect(doc);
    if visible.is_empty() {
        return Vec::new();
    }
    tile_coords_for_rect(&visible, tile_px)
        .into_iter()
        .map(|c| tile_rect(c, tile_px).intersect(doc))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Tiles (same cut as [`viewport_tiles`]) intersecting at least one dirty
/// zone: the to-re-render tile discovery (§4 6E). Row order, deterministic.
/// Empty if nothing dirty. `PixelLayer` not required: pure geometry over
/// `DirtyRegion`.
#[must_use]
pub fn dirty_tile_rects(
    dirty: &crate::document::DirtyRegion,
    doc_w: u32,
    doc_h: u32,
    tile_px: u32,
) -> Vec<TileRegion> {
    if dirty.is_empty() {
        return Vec::new();
    }
    // `viewport_tiles` already clips to the document: only the dirty test remains.
    viewport_tiles(&dirty.bounds(), doc_w, doc_h, tile_px)
        .into_iter()
        .filter(|t| dirty.intersects(*t))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiles::{TileGrid, TileRange};

    #[test]
    fn trait_couvre_points_plus_rayon() {
        let region = stroke_dirty_region(&[(10.0, 10.0), (20.0, 30.0)], 5.0, Padding::ZERO)
            .expect("geste valide");
        assert_eq!(region, TileRegion::new(5, 5, 20, 30));
    }

    #[test]
    fn padding_elargit_pour_le_voisinage() {
        let base = stroke_dirty_region(&[(100.0, 100.0)], 4.0, Padding::ZERO).expect("ok");
        let padded = stroke_dirty_region(&[(100.0, 100.0)], 4.0, Padding::for_blur_radius(20.0))
            .expect("ok");
        assert_eq!(base, TileRegion::new(96, 96, 8, 8));
        assert_eq!(padded, TileRegion::new(76, 76, 48, 48));
    }

    #[test]
    fn pipeline_geste_vers_tuiles() {
        // Full chain: gesture → region → grid → tiles → dirty set.
        let grid = TileGrid::new(256, 1920, 1080, 1);
        let region =
            stroke_dirty_region(&[(10.0, 10.0), (300.0, 10.0)], 8.0, Padding::ZERO).expect("ok");
        let range: TileRange = grid.invalidate(&region, 0, Padding::ZERO);
        assert_eq!(range.count(), 2, "le trait chevauche 2 tuiles");
        let mut dirty = tiles::DirtyTiles::new(&grid);
        assert_eq!(dirty.mark_range(range), 2);
    }

    #[test]
    fn gestes_invalides_refuses() {
        assert!(stroke_dirty_region(&[], 5.0, Padding::ZERO).is_none());
        assert!(stroke_dirty_region(&[(0.0, 0.0)], 0.0, Padding::ZERO).is_none());
        assert!(stroke_dirty_region(&[(0.0, 0.0)], f32::NAN, Padding::ZERO).is_none());
        assert!(stroke_dirty_region(&[(f32::INFINITY, 0.0)], 5.0, Padding::ZERO).is_none());
    }

    #[test]
    fn plan_petit_stroke_une_tuile() {
        // (5,5,20,30) region on 1920x1080 doc: fully inside tile (0,0).
        let plan = plan_stroke_tiles(&[(10.0, 10.0), (20.0, 30.0)], 5.0, 1920, 1080);
        assert_eq!(plan.dirty_region_px, 20 * 30);
        assert_eq!(plan.dirty_tiles, 1);
        assert_eq!(plan.scheduled_tiles, 1);
    }

    #[test]
    fn plan_stroke_large_plusieurs_tuiles() {
        // x ∈ [2,308] → tiles 0 and 1; y ∈ [2,18] → tile 0: 2 tiles.
        let plan = plan_stroke_tiles(&[(10.0, 10.0), (300.0, 10.0)], 8.0, 1920, 1080);
        assert_eq!(plan.dirty_tiles, 2);
        assert_eq!(plan.scheduled_tiles, plan.dirty_tiles);
    }

    #[test]
    fn plan_hors_document_aucune_tuile() {
        // Real region but fully outside the 8x8 surface: grid clipping.
        let plan = plan_stroke_tiles(&[(5000.0, 5000.0)], 2.0, 8, 8);
        assert!(plan.dirty_region_px > 0, "région calculée");
        assert_eq!(plan.dirty_tiles, 0, "clippé, pas de tuile invalide");
        assert_eq!(plan.scheduled_tiles, 0);
    }

    #[test]
    fn plan_geste_invalide_nul() {
        assert_eq!(
            plan_stroke_tiles(&[], 5.0, 1920, 1080),
            StrokeTilePlan::default()
        );
        assert_eq!(
            plan_stroke_tiles(&[(0.0, 0.0)], 0.0, 1920, 1080),
            StrokeTilePlan::default()
        );
    }

    #[test]
    fn plan_deterministe() {
        let points = [(10.0, 10.0), (300.0, 10.0)];
        assert_eq!(
            plan_stroke_tiles(&points, 8.0, 1920, 1080),
            plan_stroke_tiles(&points, 8.0, 1920, 1080)
        );
    }

    #[test]
    fn plan_bounds_englobe_exactement_les_tuiles() {
        // (4,5),(5,5),(4,6),(5,6) tiles at 256 px on 2048² doc: x ∈ [1090,1460)
        // → tiles 4,5; y ∈ [1340,1710) → tiles 5,6.
        let plan = plan_stroke_tiles(&[(1100.0, 1350.0), (1450.0, 1700.0)], 10.0, 2048, 2048);
        assert_eq!(plan.dirty_tiles, 4);
        assert_eq!(plan.bounds, TileRegion::new(1024, 1280, 512, 512));
    }

    #[test]
    fn plan_bounds_vide_sans_tuile() {
        let plan = plan_stroke_tiles(&[(5000.0, 5000.0)], 2.0, 8, 8);
        assert_eq!(plan.bounds, TileRegion::EMPTY);
        let nul = plan_stroke_tiles(&[], 5.0, 1920, 1080);
        assert_eq!(nul.bounds, TileRegion::EMPTY);
    }

    #[test]
    fn fallback_regional_trivial() {
        assert!(!use_regional_render(0, 1000), "plan vide → pleine image");
        assert!(
            !use_regional_render(1000, 1000),
            "couverture totale → pleine image"
        );
        assert!(
            !use_regional_render(2000, 1000),
            "débordement → pleine image"
        );
        assert!(use_regional_render(1, 1000), "région partielle → régional");
        assert!(use_regional_render(999, 1000));
    }

    use crate::document::Transform2D;

    fn ident() -> Transform2D {
        Transform2D::default()
    }

    #[test]
    fn identity_equivalent_a_l_ancien_chemin() {
        // Identity transform: same region as before the phase.
        let points = [(100.0f32, 100.0f32)];
        let fp =
            stroke_footprint_in_document(&points, 10.0, &ident(), 512, 512).expect("geste valide");
        assert_eq!(fp, TileRegion::new(90, 90, 20, 20));
        assert_eq!(
            fp,
            stroke_dirty_region(&points, 10.0, Padding::ZERO).expect("ancien chemin")
        );
        let old = plan_stroke_tiles(&points, 10.0, 1920, 1080);
        let new = plan_stroke_tiles_layer_space(
            &points,
            10.0,
            &ident(),
            512,
            512,
            1920,
            1080,
            Padding::ZERO,
        );
        assert_eq!(old, new);
    }

    #[test]
    fn translation_deplace_la_region_document() {
        let t = Transform2D {
            offset_x: 100.0,
            offset_y: 50.0,
            ..ident()
        };
        let fp =
            stroke_footprint_in_document(&[(10.0, 20.0)], 5.0, &t, 512, 512).expect("geste valide");
        // Layer box [5,15]×[15,25] offset by (+100,+50), via the real APIs.
        assert_eq!(fp, TileRegion::new(105, 65, 10, 10));
        let expect = TileRegion::new(5, 15, 10, 10);
        let corner = t.local_to_doc(512.0, 512.0, expect.x as f32, expect.y as f32);
        assert_eq!((corner.0, corner.1), (105.0, 65.0));
    }

    #[test]
    fn rotation_90_bbox_reelle() {
        // 100×50 layer, 90° rotation: exact math x[25,75]×y[-25,75]
        // (corners (0,0)→(75,-25), (100,0)→(75,75), (100,50)→(25,75),
        // (0,50)→(25,-25)). The float cos(90°) (~-4e-8) widens by at most
        // 1 px: check exact content + an overestimation bound.
        let t = Transform2D {
            rotation_deg: 90.0,
            ..ident()
        };
        let full = layer_footprint_in_document(&t, 100, 50);
        assert_ne!(full, TileRegion::new(0, 0, 100, 50), "transformée");
        assert!(full.x <= 25 && full.y <= -25, "couvre le min exact");
        assert!(
            full.right() >= 75 && full.bottom() >= 75,
            "couvre le max exact"
        );
        assert!(full.area() <= 52 * 102, "surestimation ≤ 1 px par bord");
        // Asymmetric stroke: the footprint is NOT the untransformed bbox and
        // covers the exact x[65,75]×y[-25,-5] bbox.
        let fp = stroke_footprint_in_document(&[(0.0, 0.0), (20.0, 10.0)], 1.0, &t, 100, 50)
            .expect("geste valide");
        assert_ne!(fp, TileRegion::new(-1, -1, 22, 12));
        assert!(fp.x <= 65 && fp.y <= -25);
        assert!(fp.right() >= 75 && fp.bottom() >= -5);
        assert!(fp.area() <= 14 * 24, "surestimation ≤ 1 px par bord");
    }

    #[test]
    fn scale_uniforme_et_non_uniforme_conservateurs() {
        let t2 = Transform2D {
            scale_x: 2.0,
            scale_y: 2.0,
            ..ident()
        };
        // 64×64 layer, point (10,10) r=2: [8,12]² box → doc [16,24]².
        let fp =
            stroke_footprint_in_document(&[(10.0, 10.0)], 2.0, &t2, 64, 64).expect("geste valide");
        assert_eq!(fp, TileRegion::new(16, 16, 8, 8));

        let tnu = Transform2D {
            scale_x: 2.0,
            scale_y: 0.5,
            ..ident()
        };
        let fp_nu =
            stroke_footprint_in_document(&[(10.0, 10.0)], 2.0, &tnu, 64, 64).expect("geste valide");
        // ux∈[-48,-40]→x[16,24] ; uy∈[-12,-10]→y[4,6].
        assert_eq!(fp_nu, TileRegion::new(16, 4, 8, 2));
    }

    #[test]
    fn skew_non_nul_modifie_l_empreinte() {
        let t = Transform2D {
            skew_x: 45.0,
            ..ident()
        };
        let fp =
            stroke_footprint_in_document(&[(10.0, 20.0)], 2.0, &t, 100, 100).expect("geste valide");
        // Untransformed [8,12]×[18,22] box; X skew (tan=1) shifts left.
        assert_ne!(fp, TileRegion::new(8, 18, 4, 4));
        assert_eq!(fp, TileRegion::new(-24, 18, 8, 4));
    }

    #[test]
    fn old_union_new_couvre_les_deux_positions() {
        let a = ident();
        let b = Transform2D {
            offset_x: 200.0,
            ..ident()
        };
        // 64×64 layer: [0,64]² ∪ [200,264]×[0,64] = [0,264]×[0,64].
        assert_eq!(
            layer_move_dirty_region(&a, &b, 64, 64, Padding::ZERO),
            TileRegion::new(0, 0, 264, 64)
        );
        // With halo 10: grown 10 px everywhere.
        assert_eq!(
            layer_move_dirty_region(&a, &b, 64, 64, Padding::new(10)),
            TileRegion::new(-10, -10, 284, 84)
        );
    }

    #[test]
    fn halo_elargit_et_clipping_grille() {
        assert_eq!(
            expand_region_by_halo(&TileRegion::new(100, 100, 100, 100), Padding::new(10)),
            TileRegion::new(90, 90, 120, 120)
        );
        assert_eq!(
            expand_region_by_halo(&TileRegion::EMPTY, Padding::new(10)),
            TileRegion::EMPTY
        );
        // Clipping: 64×64 doc, 256 tile ⇒ single tile, clamped bounds.
        let grid = TileGrid::new(256, 64, 64, 1);
        let grown = expand_region_by_halo(&TileRegion::new(0, 0, 64, 64), Padding::new(10));
        let range = grid.invalidate(&grown, 0, Padding::ZERO);
        assert_eq!(range.count(), 1);
        assert_eq!(range.to_region(&grid), TileRegion::new(0, 0, 64, 64));
    }

    fn blur_filter(radius: f32) -> crate::document::FilterLayer {
        let mut f = crate::document::FilterLayer::neutral("blur", Default::default());
        f.params
            .insert("radius".to_string(), datatypes::ParamValue::Float(radius));
        f
    }

    #[test]
    fn halo_blur_somme_sequentielle() {
        assert_eq!(blur_halo_for_filters(&[]), Padding::ZERO);
        assert_eq!(
            blur_halo_for_filters(&[blur_filter(10.0)]),
            Padding::new(10)
        );
        assert_eq!(
            blur_halo_for_filters(&[blur_filter(10.0), blur_filter(20.0)]),
            Padding::new(30)
        );
        // Disabled or non-blur: ignored.
        let mut off = blur_filter(50.0);
        off.enabled = false;
        let mut bc =
            crate::document::FilterLayer::neutral("brightness_contrast", Default::default());
        bc.params
            .insert("brightness".to_string(), datatypes::ParamValue::Float(5.0));
        assert_eq!(blur_halo_for_filters(&[off, bc]), Padding::ZERO);
    }

    #[test]
    fn plan_blur_etend_vers_la_tuile_voisine() {
        // (255,100) point r=1, 1024 doc: [254,256] box ⇒ tile x=0 alone.
        let plain = plan_stroke_tiles_layer_space(
            &[(255.0, 100.0)],
            1.0,
            &ident(),
            1024,
            1024,
            1024,
            1024,
            Padding::ZERO,
        );
        assert_eq!(plain.dirty_tiles, 1);
        // Halo 10 (Blur 10): [244,266] ⇒ tiles x=0 and x=1.
        let halo = blur_halo_for_filters(&[blur_filter(10.0)]);
        let grown = plan_stroke_tiles_layer_space(
            &[(255.0, 100.0)],
            1.0,
            &ident(),
            1024,
            1024,
            1024,
            1024,
            halo,
        );
        assert_eq!(grown.dirty_tiles, 2);
        assert!(grown.dirty_region_px > plain.dirty_region_px);
    }

    #[test]
    fn frontiere_tuile_avec_et_sans_halo() {
        let grid = TileGrid::new(256, 1920, 1080, 1);
        // x = 250 → 270: overlaps tiles 0 and 1.
        let range = grid.invalidate(&TileRegion::new(250, 10, 20, 10), 0, Padding::ZERO);
        assert_eq!(range.count(), 2);
        // Contained region + halo ⇒ dirty neighbor.
        let tight = grid.invalidate(&TileRegion::new(250, 10, 5, 10), 0, Padding::ZERO);
        assert_eq!(tight.count(), 1);
        let grown = grid.invalidate(&TileRegion::new(250, 10, 5, 10), 0, Padding::new(10));
        assert_eq!(grown.count(), 2);
    }

    #[test]
    fn bornes_document_clampees_aucune_tuile_hors_doc() {
        // Fully outside the document: no tiles, empty bounds.
        let plan = plan_stroke_tiles_layer_space(
            &[(-500.0, -500.0)],
            2.0,
            &ident(),
            64,
            64,
            8,
            8,
            Padding::ZERO,
        );
        assert_eq!(plan.dirty_tiles, 0);
        assert_eq!(plan.bounds, TileRegion::EMPTY);
        // Partially out of frame: clipped to the surface, no negative tile.
        let plan = plan_stroke_tiles_layer_space(
            &[(2.0, 2.0)],
            10.0,
            &ident(),
            64,
            64,
            8,
            8,
            Padding::ZERO,
        );
        assert_eq!(plan.dirty_tiles, 1);
        assert_eq!(plan.bounds, TileRegion::new(0, 0, 8, 8));
        // y < 0 and x > width: same via the bare grid.
        let grid = TileGrid::new(256, 100, 100, 1);
        let range = grid.invalidate(&TileRegion::new(-50, -50, 300, 300), 0, Padding::ZERO);
        assert_eq!(range.to_region(&grid), TileRegion::new(0, 0, 100, 100));
    }

    #[test]
    fn integration_transform_paint_tiles() {
        // 1024 document, 256 tiles; 200×200 layer, translation + 90° rotation.
        let t = Transform2D {
            offset_x: 100.0,
            offset_y: 50.0,
            rotation_deg: 90.0,
            ..ident()
        };
        let plan = plan_stroke_tiles_layer_space(
            &[(10.0, 20.0)],
            2.0,
            &t,
            200,
            200,
            1024,
            1024,
            Padding::ZERO,
        );
        // Layer box [8,12]×[18,22] → doc x[278,282]×y[58,62] (90° rotation
        // around center (100,100), then + scaled center + offset (100,50),
        // see `local_to_doc`): single (1,0) tile.
        let fp =
            stroke_footprint_in_document(&[(10.0, 20.0)], 2.0, &t, 200, 200).expect("geste valide");
        assert_eq!(fp, TileRegion::new(278, 58, 4, 4));
        assert_eq!(plan.dirty_tiles, 1);
        assert_eq!(plan.scheduled_tiles, 1);
        assert_eq!(plan.bounds, TileRegion::new(256, 0, 256, 256));
        // Equivalence: the same gesture expressed directly in document-space
        // yields the same tiles.
        let direct = plan_stroke_tiles(&[(280.0, 60.0)], 2.0, 1024, 1024);
        assert_eq!(direct.dirty_tiles, plan.dirty_tiles);
        assert_eq!(direct.bounds, plan.bounds);
    }

    #[test]
    fn geste_invalide_plan_nul_transforme() {
        assert_eq!(
            plan_stroke_tiles_layer_space(&[], 5.0, &ident(), 64, 64, 64, 64, Padding::ZERO),
            StrokeTilePlan::default()
        );
        assert_eq!(
            plan_stroke_tiles_layer_space(
                &[(0.0, 0.0)],
                0.0,
                &ident(),
                64,
                64,
                64,
                64,
                Padding::ZERO
            ),
            StrokeTilePlan::default()
        );
        assert!(stroke_footprint_in_document(&[], 5.0, &ident(), 64, 64).is_none());
        assert_eq!(
            layer_footprint_in_document(&ident(), 0, 64),
            TileRegion::EMPTY
        );
    }

    #[test]
    fn coordonnees_canoniques_stables() {
        // Same tile via two viewports: same coordinate, same rect.
        let a = tile_coords_for_rect(&TileRegion::new(0, 0, 256, 256), 256);
        let b = tile_coords_for_rect(&TileRegion::new(200, 200, 200, 200), 256);
        assert_eq!(a, vec![TileCoord::new(0, 0)]);
        assert!(b.contains(&TileCoord::new(0, 0)));
        assert!(b.contains(&TileCoord::new(1, 1)));
        assert_eq!(
            tile_rect(TileCoord::new(1, 0), 256),
            TileRegion::new(256, 0, 256, 256)
        );
        // 128 step: doubled indices, same covered zone.
        let c = tile_coords_for_rect(&TileRegion::new(0, 0, 256, 256), 128);
        assert_eq!(c.len(), 4);
        // Outside document (negative): ignored, never a negative index.
        assert!(tile_coords_for_rect(&TileRegion::new(-300, -300, 100, 100), 256).is_empty());
        assert!(tile_coords_for_rect(&TileRegion::EMPTY, 256).is_empty());
        // Row order, deterministic.
        let d = tile_coords_for_rect(&TileRegion::new(0, 0, 512, 256), 256);
        assert_eq!(d, vec![TileCoord::new(0, 0), TileCoord::new(1, 0)]);
    }

    #[test]
    fn viewport_tiles_partition_3x3() {
        // 384² document, 128 grain: 9 tiles in rows, no gap.
        let tiles = viewport_tiles(&TileRegion::new(0, 0, 384, 384), 384, 384, 128);
        assert_eq!(tiles.len(), 9);
        assert_eq!(tiles[0], TileRegion::new(0, 0, 128, 128));
        assert_eq!(tiles[8], TileRegion::new(256, 256, 128, 128));
        let area: u64 = tiles.iter().map(|t| t.area()).sum();
        assert_eq!(area, 384 * 384);
    }

    #[test]
    fn viewport_tiles_clip_et_hors_doc() {
        // 200² doc, 256 grain: single tile clipped to the document.
        assert_eq!(
            viewport_tiles(&TileRegion::new(0, 0, 200, 200), 200, 200, 256),
            vec![TileRegion::new(0, 0, 200, 200)]
        );
        // Partial: the covering grid tile, clipped to the doc.
        assert_eq!(
            viewport_tiles(&TileRegion::new(150, 150, 100, 100), 200, 200, 256),
            vec![TileRegion::new(0, 0, 200, 200)]
        );
        // Outside document / empty: nothing.
        assert!(viewport_tiles(&TileRegion::new(600, 600, 10, 10), 200, 200, 256).is_empty());
        assert!(viewport_tiles(&TileRegion::EMPTY, 200, 200, 256).is_empty());
    }

    #[test]
    fn dirty_tile_rects_decouverte() {
        use crate::document::DirtyRegion;
        let mut dirty = DirtyRegion::new();
        assert!(dirty_tile_rects(&dirty, 512, 512, 256).is_empty());
        // Dirty over x[300,400)×y[100,300): tiles (1,0),(1,1) — not neighbors.
        dirty.add(TileRegion::new(300, 100, 100, 200));
        let tiles = dirty_tile_rects(&dirty, 512, 512, 256);
        assert_eq!(
            tiles,
            vec![
                TileRegion::new(256, 0, 256, 256),
                TileRegion::new(256, 256, 256, 256),
            ]
        );
    }

    #[test]
    fn node_footprint_pixel_groupe_ajustement() {
        use crate::document::{GroupLayer, LayerNode, PixelLayer};
        use image::{ImageBuffer, Rgba};
        use std::sync::Arc;
        let img = Arc::new(image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            40,
            40,
            Rgba([1, 2, 3, 255]),
        )));
        let mut l = PixelLayer::new("a", Arc::clone(&img));
        l.transform.offset_x = 10.0;
        l.transform.offset_y = 20.0;
        assert_eq!(
            node_footprint(&LayerNode::Pixel(l.clone())),
            TileRegion::new(10, 20, 40, 40)
        );
        let mut l2 = PixelLayer::new("b", img);
        l2.transform.offset_x = 100.0;
        let g = GroupLayer::new("g", vec![LayerNode::Pixel(l), LayerNode::Pixel(l2)]);
        // l : [10,50)×[20,60) ; l2 : [100,140)×[0,40) ⇒ union (10,0,130,60).
        assert_eq!(
            node_footprint(&LayerNode::Group(g)),
            TileRegion::new(10, 0, 130, 60)
        );
        // Adjustment: no local footprint — caller-side global fallback.
        let adj = LayerNode::Adjustment(crate::document::AdjustmentLayer::new("adj", Vec::new()));
        assert_eq!(node_footprint(&adj), TileRegion::EMPTY);
    }

    #[test]
    fn node_mark_region_footprint_plus_spread() {
        use crate::document::{GroupLayer, LayerNode, PixelLayer};
        use datatypes::ParamValue;
        use image::{ImageBuffer, Rgba};
        use std::sync::Arc;
        let img = Arc::new(image::DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            40,
            40,
            Rgba([1, 2, 3, 255]),
        )));
        // No filter: footprint alone.
        let l = PixelLayer::new("a", Arc::clone(&img));
        assert_eq!(
            node_mark_region(&LayerNode::Pixel(l)),
            TileRegion::new(0, 0, 40, 40)
        );
        // σ=2 blur (support 5): footprint grown by 5.
        let mut lf = PixelLayer::new("b", img);
        let mut blur = FilterLayer::neutral("blur", Default::default());
        blur.params
            .insert("radius".to_string(), ParamValue::Float(2.0));
        lf.filter_layers.push(blur);
        assert_eq!(
            node_mark_region(&LayerNode::Pixel(lf)),
            TileRegion::new(-5, -5, 50, 50)
        );
        // Adjustment: caller-decided global fallback.
        let adj = LayerNode::Adjustment(crate::document::AdjustmentLayer::new("adj", Vec::new()));
        assert_eq!(node_mark_region(&adj), TileRegion::EMPTY);
        let _ = GroupLayer::new("g", Vec::new());
    }

    #[test]
    fn appearance_spread_supports_exacts() {
        use datatypes::ParamValue;
        assert_eq!(appearance_spread(&[]), Padding::ZERO);
        let mut blur = FilterLayer::neutral("blur", Default::default());
        blur.params
            .insert("radius".to_string(), ParamValue::Float(3.0));
        assert_eq!(
            appearance_spread(&[blur.clone()]),
            Padding::new(crate::document::compositing::blur_support_px(3.0))
        );
        let mut blur2 = FilterLayer::neutral("blur", Default::default());
        blur2
            .params
            .insert("radius".to_string(), ParamValue::Float(2.0));
        let mut off = blur2.clone();
        off.enabled = false;
        // Sequential sum, disabled and non-blur ignored.
        let mut bc = FilterLayer::neutral("brightness_contrast", Default::default());
        bc.params
            .insert("brightness".to_string(), ParamValue::Float(5.0));
        assert_eq!(
            appearance_spread(&[blur, blur2, off, bc]),
            Padding::new(
                crate::document::compositing::blur_support_px(3.0)
                    + crate::document::compositing::blur_support_px(2.0)
            )
        );
    }
}
