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

//! Worker metrics and timings: [`PreviewTimings`], [`OpMetrics`],
//! [`WorkerMetrics`].

use crate::ui::features::layers::{
    PhotoLayerInfo, PhotoLayerThumb, flattened_len, snapshot_layers, snapshot_layers_with,
};
use photo_engine::tiles::TileRegion;
use photo_engine::{BlendMode, Document, RenderEvent, RenderRevision};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use uuid::Uuid;

/// Durations of one `render_preview` pass (microseconds).
#[derive(Debug, Default, Clone, Copy)]
pub struct PreviewTimings {
    /// Preview downscale input pixels §8 (0 when not downscaled).
    pub preview_resize_pixels: u64,
    /// Full-resolution composite (`composite_preview` / `composite`).
    pub composite_us: u128,
    /// Thumbnail (`capped_preview` + `into_raw`).
    pub thumb_us: u128,
    /// Document geometry (`preview_geometry`, 3rd appearance pass).
    pub geometry_us: u128,
    /// Prepare + blend + mix sweeps (excl. extents, excl. alloc).
    pub blend_us: u128,
    /// Pixel layers and groups actually blended.
    pub layers_blended: u64,
    /// Pixels traversed (accumulator sweep sums).
    pub pixels_processed: u64,
    /// Accumulator pixels (scope).
    pub scope_px: u64,
    /// Full-resolution composite pixels.
    pub full_px: u64,
    /// Sent thumbnail pixels.
    pub preview_px: u64,
}

/// Metrics of one worker command (filled by `apply`).
#[derive(Debug, Default, Clone)]
pub struct OpMetrics {
    /// Command name (see `PhotoEngineCommand::op_name`).
    pub op: &'static str,
    /// Total `apply` duration (mutation + snapshot + response).
    pub total_us: u128,
    /// Pure mutation (`mutate`: setters + history + painting).
    pub mutation_us: u128,
    /// Layer snapshot construction (`snapshot_layers`).
    pub snapshot_us: u128,
    /// Full-resolution composite.
    pub composite_us: u128,
    /// Thumbnail + buffering.
    pub thumb_us: u128,
    /// Document geometry (infinite-plane origin).
    pub geometry_us: u128,
    /// Inter-pass sharing construction (renderer resolutions).
    pub frame_us: u128,
    /// Appearances resolved from the renderer during the response.
    pub appearance_resolves: u64,
    /// Inter-pass reuses (zero new resolutions).
    pub appearance_frame_hits: u64,
    /// Regenerated `preview` buffers (renderer counter delta).
    pub preview_rebuilds: u64,
    /// Regenerated `thumb` thumbnails (renderer counter delta).
    pub thumb_rebuilds: u64,
    /// Tile OBSERVATION (vertical slice, no render effect):
    /// dirty-region area of the last stroke in pixels.
    pub dirty_region_px: u64,
    /// Tile OBSERVATION: level-0 tiles marked dirty.
    pub dirty_tiles: u64,
    /// Tile OBSERVATION: tiles kept by the scheduler.
    pub scheduled_tiles: u64,
    /// Phase 6E: tiles re-served from the incremental cache (0 compute).
    pub cache_hits: u64,
    /// Phase 6E: evaluated tiles (windowed `composite_region_with`).
    pub cache_misses: u64,
    /// Phase 6E: renders avoided by hits (== `cache_hits`).
    pub renders_avoided: u64,
    /// Phase 6F: visible tiles required by the rendered viewport.
    pub visible_tiles: u64,
    /// Phase 6F: tiles requested from the worker (== visible, no prefetch).
    pub requested_tiles: u64,
    /// Phase 6G.2 §8: dirty zones fully settled by the frame
    /// (post-render consumption — the F1 anti-drift guard).
    pub dirty_tiles_cleared: u64,
    /// Phase 6G: pixels written to the persistent surface (0 when warm —
    /// hits write nothing; partial ≈ dirty tiles).
    pub surface_pixels_written: u64,
    /// Phase 6G.1: plan scheduling time (pure scheduler).
    pub schedule_us: u128,
    /// Phase 6G.1: total tile evaluation time (lookup + renders).
    pub tile_render_total_us: u128,
    /// Phase 6G.1: assembly slicing time (memcpy O(viewport)).
    pub assembly_us: u128,
    /// Phase 6G.1: this frame's surface write time (delta).
    pub surface_update_us: u128,
    /// Phase 6G.2 §8: preview downscale input pixels (0 when no
    /// downscale — same dimensions kept, §3.7).
    pub preview_resize_pixels: u64,
    /// Phase 6G.3 §14: secondary thumbnails finished this frame.
    pub thumbnail_jobs_completed: u64,
    /// Phase 6G.4: thumbnail results re-served by the cache without
    /// computation this frame (HIT).
    pub thumb_cache_hits: u64,
    /// Phase 6G.4: thumbnail results that required computation.
    pub thumb_cache_misses: u64,
    /// Phase 6G.3 §14: canvas-critical time (frame images + composite +
    /// downscale + assembly — everything producing the canvas pixels,
    /// without thumbnails).
    pub canvas_critical_us: u128,
    /// Phase 6G.3 §14: secondary work absorbed this frame (finished
    /// thumbnails, off the canvas path).
    pub secondary_work_us: u128,
    /// Prepare + blend + mix sweeps (excl. extents, excl. alloc).
    pub blend_us: u128,
    /// Pixel layers and groups actually blended.
    pub layers_blended: u64,
    /// Pixels traversed (accumulator sweep sums).
    pub pixels_processed: u64,
    /// Accumulator pixels (scope).
    pub scope_px: u64,
    /// Full-resolution composite pixels.
    pub full_px: u64,
    /// Sent thumbnail pixels.
    pub preview_px: u64,
}

/// Cumulative worker counters (zeroed at creation).
#[derive(Debug, Default)]
pub struct WorkerMetrics {
    /// Metrics of the last processed command.
    pub last: Option<OpMetrics>,
    /// Command in flight (set by `apply`/`apply_batch`).
    pub(crate) current_op: &'static str,
    /// Number of produced responses.
    pub responses: u64,
    /// Number of responses with a preview.
    pub previews: u64,
    /// Number of composites actually produced (non-coalesced renders).
    pub renders: u64,
    /// Secondary thumbnails (Phase 6G.3 §14, cumulative).
    pub thumb: crate::ui::thumb_worker::ThumbStats,
}
