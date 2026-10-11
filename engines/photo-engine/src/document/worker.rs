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

//! Incremental tile rendering (Phase 6E): orchestration, no computation.
//!
//! [`RenderWorker`] chains the foundations without absorbing their
//! responsibilities:
//!
//! ```text
//! DirtyRegion (dirty.rs)      = which zones were modified
//! SpatialScope (compositing)  = for R, which dependency D to compute
//! TileCache (tile_cache.rs)   = reuse the final result of R
//! RenderWorker (here)         = slice, look up, assemble, measure
//! ```
//!
//! The worker knows neither blur nor blend: for each missing tile it
//! delegates to [`get_or_render`](super::tile_cache::get_or_render),
//! which delegates to `composite_region_with` (6C window). Sequential (no
//! multithread scheduler in 6E), pure engine (no egui/wgpu).
//!
//! Spaces: tile requests are in DOCUMENT SPACE (integer rects); assembly
//! happens in buffer coordinates via the origin carried by each
//! [`RegionalComposite`] — no duplicated geometry math here.
//!
//! Perimeter safety: the worker tracks the scope geometry ([`ScopeGeom`]);
//! any change (resize, overflow, moved content) empties the cache
//! before rendering — never a tile from an old perimeter re-served
//! under a new geometry.

use std::sync::Arc;

use image::DynamicImage;
use tiles::TileRegion;
use uuid::Uuid;

use super::compositing::{CompositeStats, scope_half_extents};
use super::preview_surface::{PreviewSurface, PreviewSurfaceStats};
use super::scheduler::{RenderPriorityContext, RenderRequest, TilePriority, schedule_viewport};
use super::tile_cache::{TILE_CACHE_FLAGS_NONE, TileCache, get_or_render};
use super::tree::Document;
use crate::gpu::GpuContext;
use crate::tile_key::{BackendTag, GPU_PIXEL_THRESHOLD, tile_content_signature};

/// Render perimeter geometry (= assembly grid): buffer dimensions + world
/// origin (same formulas as `composite_scope_with`). Cropped mode: `{doc,
/// 0, 0}`; infinite plane: real extents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScopeGeom {
    /// Buffer width (px).
    pub w: u32,
    /// Buffer height (px).
    pub h: u32,
    /// World origin X (world (0,0) = buffer − origin).
    pub origin_x: f32,
    /// World origin Y.
    pub origin_y: f32,
}

impl ScopeGeom {
    /// Document perimeter (cropped mode): buffer == document, zero origin.
    #[must_use]
    pub fn document(doc_w: u32, doc_h: u32) -> Self {
        Self {
            w: doc_w.max(1),
            h: doc_h.max(1),
            origin_x: 0.0,
            origin_y: 0.0,
        }
    }

    /// Covered buffer rectangle.
    #[must_use]
    pub fn rect(self) -> TileRegion {
        TileRegion::new(0, 0, self.w, self.h)
    }
}

/// Assembled view: pixels + buffer origin (document-space mapping kept
/// like `RegionalComposite`).
#[derive(Debug, Clone)]
pub struct AssembledView {
    /// Assembled pixels (viewport clipped to scope).
    pub image: DynamicImage,
    /// Buffer column of the origin.
    pub origin_x: u32,
    /// Buffer row of the origin.
    pub origin_y: u32,
}

/// Orchestration counters (Phase 6E §15): pure counting, no fancy
/// telemetry. `rendered_tiles` = evaluated tiles (miss ⇒ one regional
/// evaluation); `dirty_tiles` = required tiles intersecting dirty BEFORE
/// rendering; `dependency_px` = cumulative dependency pixels (sum of
/// actually evaluated D windows — `CompositeStats::scope_px` being
/// overwritten per tile, this cumulative sum IS the §18/§22 measure).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderWorkerStats {
    /// Requested tiles (viewport ∩ scope).
    pub requested_tiles: u64,
    /// Evaluated tiles (miss).
    pub rendered_tiles: u64,
    /// Tiles re-served without computation.
    pub cache_hits: u64,
    /// Missed tiles.
    pub cache_misses: u64,
    /// Dirty tiles among required ones (dirty discovery).
    pub dirty_tiles: u64,
    /// Required visible tiles (== `requested_tiles` while there is no
    /// prefetch — Phase 6F §20).
    pub visible_tiles: u64,
    /// Evaluated dependency pixels (cumulative D).
    pub dependency_px: u64,
    /// Plan build time (pure scheduling, Phase 6G.1).
    pub schedule_us: u128,
    /// Total tile evaluation time (cache lookup + miss render).
    pub tile_render_total_us: u128,
    /// Smallest tile evaluation (µs, 0 if none).
    pub tile_render_min_us: u64,
    /// Largest tile evaluation (µs).
    pub tile_render_max_us: u64,
    /// Assembly crop time (memcpy O(viewport), Phase 6G.1 §7).
    pub assembly_us: u128,
}

/// Incremental render orchestrator: cache + grain + tracked perimeter +
/// persistent surface.
///
/// Hit `Clone`s copy pixels (correctness-first foundation, see
/// `TileCache`; `Arc` documented as a future optimization). The
/// (`PreviewSurface`, Phase 6G) surface accumulates rendered tiles: a
/// hit writes NOTHING (zero rewrite), a miss only writes its own tile.
#[derive(Debug)]
pub struct RenderWorker {
    cache: TileCache,
    tile_px: u32,
    last_scope: Option<ScopeGeom>,
    surface: Option<PreviewSurface>,
    /// Document rects actually (re)rendered by the last `render` (miss
    /// with contribution) — settled via `take_rendered_tiles`.
    last_rendered: Vec<TileRegion>,
}

impl RenderWorker {
    /// New worker (cache budget in pixels, tile grain in px; default
    /// `VIEWPORT_TILE_PX`). Surface created on first render (explicit
    /// full path).
    #[must_use]
    pub fn new(max_pixels: usize, tile_px: u32) -> Self {
        Self {
            cache: TileCache::new(max_pixels),
            tile_px: tile_px.max(1),
            last_scope: None,
            surface: None,
            last_rendered: Vec::new(),
        }
    }

    /// Rects rendered by the last `render`, drained on read (Phase 6G.2 §1):
    /// the caller passes them to `consume_rendered` AFTER applying the
    /// result — never before, never without rendering.
    pub fn take_rendered_tiles(&mut self) -> Vec<TileRegion> {
        std::mem::take(&mut self.last_rendered)
    }

    /// Cumulative persistent-surface statistics (zero if never created).
    #[must_use]
    pub fn surface_stats(&self) -> PreviewSurfaceStats {
        self.surface
            .as_ref()
            .map_or(PreviewSurfaceStats::default(), PreviewSurface::stats)
    }

    /// Forgets the surface (cache kept): the next `render` recreates it
    /// empty then repopulates it by rendering, or `repopulate_from_cache`
    /// fills it WITHOUT recompositing. Explicit (§8: neither silent nor frequent).
    pub fn reset_surface(&mut self) {
        self.surface = None;
    }

    /// Repopulates the surface from cache alone (zero compositing) then
    /// returns the `viewport` crop (`scope` buffer coords). `None` if the
    /// scope is unknown/inconsistent (go through `render`, which detects and
    /// invalidates) or if no tile covers the zone.
    pub fn repopulate_from_cache(
        &mut self,
        scope: ScopeGeom,
        viewport: TileRegion,
        scale: f32,
    ) -> Option<AssembledView> {
        if self.last_scope != Some(scope) {
            return None;
        }
        let clip = viewport.intersect(scope.rect());
        if clip.is_empty() {
            return None;
        }
        self.surface_for(scope);
        let tiles = self.cache.tiles_for_surface(clip, scale);
        if tiles.is_empty() {
            return None;
        }
        let surface = self.surface.as_mut().expect("créée ci-dessus");
        surface.update_tiles(&tiles);
        Self::crop_surface(surface, &clip)
    }

    /// Surface ensured at scope dims (created if needed: explicit full).
    fn surface_for(&mut self, scope: ScopeGeom) -> &mut PreviewSurface {
        if self.surface.is_none() {
            self.surface = Some(PreviewSurface::new(scope.w, scope.h));
        }
        let surface = self.surface.as_mut().expect("créée ci-dessus");
        surface.ensure(scope.w, scope.h);
        surface
    }

    /// Crops the surface to the clip (buffer coords).
    fn crop_surface(surface: &PreviewSurface, clip: &TileRegion) -> Option<AssembledView> {
        let (sw, sh) = surface.dimensions();
        let rect = TileRegion::new(0, 0, sw, sh).intersect(*clip);
        if rect.is_empty() {
            return None;
        }
        let cropped = image::imageops::crop_imm(
            surface.image(),
            rect.x as u32,
            rect.y as u32,
            rect.width,
            rect.height,
        )
        .to_image();
        Some(AssembledView {
            image: DynamicImage::ImageRgba8(cropped),
            origin_x: rect.x.max(0) as u32,
            origin_y: rect.y.max(0) as u32,
        })
    }

    /// Cache access (caller-side dirty marking — see the app worker).
    pub fn cache_mut(&mut self) -> &mut TileCache {
        &mut self.cache
    }

    /// Read access to the cache (dirty discovery, diagnostics).
    #[must_use]
    pub fn cache(&self) -> &TileCache {
        &self.cache
    }

    /// Tile grain (px).
    #[must_use]
    pub fn tile_px(&self) -> u32 {
        self.tile_px
    }

    /// Real document perimeter (infinite plane): same formulas as
    /// `composite_scope_with` (extents + clamp), with zero pixels computed.
    #[must_use]
    pub fn scope_for(
        &self,
        doc: &Document,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> ScopeGeom {
        let (half_w, half_h) = scope_half_extents(&doc.root, doc.width, doc.height, resolve);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        ScopeGeom {
            w: w.max(1),
            h: h.max(1),
            origin_x: half_w - doc.width as f32 / 2.0,
            origin_y: half_h - doc.height as f32 / 2.0,
        }
    }

    /// Request covering a whole scope (DOCUMENT SPACE): the plan from such a
    /// request assembles exactly the scope buffer (assembly uses the tiles'
    /// authoritative buffer origins).
    #[must_use]
    pub fn scope_request(scope: ScopeGeom, scale: f32) -> RenderRequest {
        RenderRequest::new(
            TileRegion::new(
                (-scope.origin_x).floor() as i32,
                (-scope.origin_y).floor() as i32,
                ((scope.w as f32 - scope.origin_x).ceil() - (-scope.origin_x).floor()).max(1.0)
                    as u32,
                ((scope.h as f32 - scope.origin_y).ceil() - (-scope.origin_y).floor()).max(1.0)
                    as u32,
            ),
            scale,
        )
    }

    /// Renders a viewport request (DOCUMENT SPACE): ordered plan (dirty
    /// first, then distance — [`schedule_viewport`]), tiles via the cache,
    /// exact assembly in buffer coords, orchestration
    /// statistics.
    ///
    /// Only missing tiles are evaluated (windowed `composite_region_with`).
    /// `None` if the request is empty, out of scope or nothing contributes.
    /// Plan order is execution order (visible+dirty first).
    ///
    /// HONEST per-tile backend (CPU/GPU kernels diverge — wrong keys
    /// forbidden): estimated from the D window (request + 6C halo) via
    /// [`BackendTag::select`]. Below the threshold, no `is_available` is
    /// called (no GPU wake-up for small documents); deterministic at equal
    /// content, hence stable hits.
    ///
    /// Eight explicit parameters (no artificial `RenderJob`): each caller
    /// already provides these pieces, see `get_or_render` / 6B).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        doc: &Document,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        render_stats: &mut CompositeStats,
        scope: ScopeGeom,
        request: &RenderRequest,
        ctx: &RenderPriorityContext,
        stats: &mut RenderWorkerStats,
    ) -> Option<AssembledView> {
        if scope.w == 0 || scope.h == 0 {
            return None;
        }
        // Tracked perimeter: any change empties the cache (safety, not
        // optimization: old-perimeter keys must never be re-served under a
        // new geometry).
        if self.last_scope != Some(scope) {
            self.cache.invalidate_all();
            self.last_scope = Some(scope);
        }
        // Valid document range (scope brought back into document): the plan
        // never requests outside this zone (overflows have no addressable
        // content and would return None anyway).
        let doc_valid = TileRegion::new(
            (-scope.origin_x).floor() as i32,
            (-scope.origin_y).floor() as i32,
            ((scope.w as f32 - scope.origin_x).ceil() - (-scope.origin_x).floor()).max(1.0) as u32,
            ((scope.h as f32 - scope.origin_y).ceil() - (-scope.origin_y).floor()).max(1.0) as u32,
        );
        let viewport = request.viewport.intersect(doc_valid);
        if viewport.is_empty() {
            return None;
        }
        // Assembly buffer window: mapped viewport (+ origin), clipped to
        // the scope. Rendered tiles carry their authoritative buffer origin
        // (`RegionalComposite`): assembly recomputes
        // no geometry.
        let clip = TileRegion::new(
            (viewport.x as f32 + scope.origin_x).floor() as i32,
            (viewport.y as f32 + scope.origin_y).floor() as i32,
            (((viewport.x as f32 + viewport.width as f32) + scope.origin_x).ceil()
                - (viewport.x as f32 + scope.origin_x).floor())
            .max(1.0) as u32,
            (((viewport.y as f32 + viewport.height as f32) + scope.origin_y).ceil()
                - (viewport.y as f32 + scope.origin_y).floor())
            .max(1.0) as u32,
        )
        .intersect(scope.rect());
        if clip.is_empty() {
            return None;
        }
        let sub_request = RenderRequest::new(viewport, request.scale);
        // 6G.1 instrumentation (no render effect): plan timer.
        let t_schedule = std::time::Instant::now();
        let plan = schedule_viewport(&sub_request, self.cache.dirty(), self.tile_px, ctx);
        stats.schedule_us += t_schedule.elapsed().as_micros();
        if plan.is_empty() {
            return None;
        }
        // Persistent surface at scope dims (created/recreated here: explicit
        // full path — never a silent rebuild elsewhere).
        self.surface_for(scope);
        // Viewport protection during render: evictions prefer off-screen tiles
        // (strict LRU fallback if all is protected).
        let protected: Vec<TileRegion> = plan.iter().map(|item| item.rect).collect();
        self.cache.set_protected(&protected);
        let content = tile_content_signature(doc);
        let halo = super::compositing::scope_adjustment_window(&doc.root).halo_px;
        let mut contributed = false;
        // Disjoint borrows: the surface writes, the cache reads, rendered-rect
        // bookkeeping accumulates for `take_rendered_tiles`.
        let Self {
            surface,
            cache,
            last_rendered,
            ..
        } = self;
        last_rendered.clear();
        let surface = surface.as_mut().expect("créée ci-dessus");
        for item in &plan {
            stats.requested_tiles += 1;
            stats.visible_tiles += 1;
            if item.priority == TilePriority::VisibleDirty {
                stats.dirty_tiles += 1;
            }
            // Estimated D window (request + halo): real backend (CPU below the
            // threshold with no GPU query, `select` beyond).
            let est = (u64::from(item.rect.width) + 2 * u64::from(halo))
                * (u64::from(item.rect.height) + 2 * u64::from(halo));
            let backend = if est < GPU_PIXEL_THRESHOLD {
                BackendTag::cpu()
            } else {
                BackendTag::select(GpuContext::is_available(), est)
            };
            let (hits_avant, misses_avant) = (cache.stats().hits, cache.stats().misses);
            // A zone with no contribution (empty accumulator) returns None: the
            // tile is skipped, neighbors still contribute.
            // 6G.1 instrumentation: per-tile timer (lookup + render).
            let t_tile = std::time::Instant::now();
            let tile = get_or_render(
                cache,
                doc,
                resolve,
                render_stats,
                &item.rect,
                backend,
                TILE_CACHE_FLAGS_NONE,
                request.scale,
                content,
            );
            let tile_us = t_tile.elapsed().as_micros();
            stats.tile_render_total_us += tile_us;
            // Min/max in µs (u64): initialized on the first tile.
            let tile_us64 = tile_us.min(u128::from(u64::MAX)) as u64;
            if stats.requested_tiles <= 1 {
                stats.tile_render_min_us = tile_us64;
                stats.tile_render_max_us = tile_us64;
            } else {
                stats.tile_render_min_us = stats.tile_render_min_us.min(tile_us64);
                stats.tile_render_max_us = stats.tile_render_max_us.max(tile_us64);
            }
            let Some(tile) = tile else {
                stats.cache_misses += cache.stats().misses - misses_avant;
                continue;
            };
            stats.cache_hits += cache.stats().hits - hits_avant;
            let misses = cache.stats().misses - misses_avant;
            stats.cache_misses += misses;
            if misses > 0 {
                // Only misses write the persistent surface: a hit proves the surface
                // already holds these bytes (zero rewrite — the heart of 6G). The
                // rendered rect is recorded for dirty consumption AFTER application
                // (§1 6G.2).
                surface.update_tile(&tile);
                last_rendered.push(item.rect);
                // `composite_region_with` sets scope_px = evaluated D window: worker
                // cumulative sum (per-tile assignment allows no total read from
                // CompositeStats).
                stats.rendered_tiles += misses;
                stats.dependency_px += render_stats.scope_px;
            }
            contributed = true;
        }
        self.cache.clear_protected();
        if !contributed {
            return None;
        }
        // 6G.1 instrumentation: the crop (memcpy O(viewport)) is explicitly
        // measured (§7: it must stay visible in metrics).
        let t_assembly = std::time::Instant::now();
        let out = Self::crop_surface(self.surface.as_ref().expect("créée ci-dessus"), &clip);
        stats.assembly_us += t_assembly.elapsed().as_micros();
        out
    }
}
