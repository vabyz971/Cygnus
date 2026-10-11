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

//! Engine worker: [`EngineWorker`] (live document, undo/redo history,
//! command application), [`apply_command`] and
//! [`spawn_photo_engine_worker`], plus the bridge tests.

use super::assert_send;
use super::commands::{
    DirtyMark, Mutation, PhotoEngineCommand, RenderInvalidation, SubmitDecision, fold_batch,
    mark_for_node, render_routing,
};
use super::metrics::{OpMetrics, PreviewTimings, WorkerMetrics};
use super::preview::{AppearanceFrameCache, PreviewImage, render_preview};
use super::responses::PhotoEngineResponse;
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

/// Worker state: live document + undo/redo history.
pub struct EngineWorker {
    pub(crate) document: Document,
    /// History stacks: PRE-mutation snapshot + French label
    /// of the operation (History panel, O003). `undo` oldest
    /// to newest, `redo` in undo order.
    pub(crate) undo: Vec<HistoryStep>,
    pub(crate) redo: Vec<HistoryStep>,
    /// Operation name (`op_name`) of the command being
    /// mutated: consumed by [`Self::push_history`] to label
    /// the step (set by `apply`/`apply_batch`, never by the arms).
    pub(crate) pending_label: Option<&'static str>,
    /// Layer of the last `SetOpacity` (slider coalescing).
    pub(crate) coalesced_opacity_layer: Option<Uuid>,
    /// Preview clipped to the document (View menu, default: infinite plane).
    pub(crate) clip_to_doc: bool,
    /// Revision of the last produced composite (stale renders
    /// ignored UI-side).
    pub(crate) revision: RenderRevision,
    /// Tile OBSERVATION (vertical slice): plan computed during
    /// a stroke mutation, consumed by `respond` in the metrics.
    /// Never read by the renderer: pure observation, zero side effects.
    /// In a batch (`apply_batch`), only the batch's last stroke is observed.
    pub(crate) pending_tile_plan: Option<photo_engine::tiles::StrokeTilePlan>,
    /// Incremental rendering (Phase 6E): tile cache + tracked extent,
    /// fed by `Mutation::dirty` markings, consumed by the
    /// preview. Persists across commands: clean tiles are
    /// reused from frame to frame.
    pub(crate) render_worker: photo_engine::document::RenderWorker,
    /// Known thumbnails per layer (Phase 6G.3): `(RgbaBuf, version)`
    /// of the last INGESTED results (freshness guard at ingestion).
    /// The snapshot serves them (stamped with the live version); fresh
    /// thumbnails also go out as `ThumbnailUpdated`.
    pub(crate) thumb_store: HashMap<Uuid, (photo_engine::RgbaBuf, u64)>,
    /// Submitted, not-yet-settled intents: `(version, source)` per
    /// layer (send dedup + second factor of the freshness
    /// guard — see `ingest_thumb_done`).
    pub(crate) thumb_pending: HashMap<Uuid, (u64, Arc<image::DynamicImage>)>,
    /// Layers with freshly stored thumbnails since the last
    /// drain (`ThumbnailUpdated` extras of batch mode).
    pub(crate) thumb_fresh: Vec<Uuid>,
    /// Secondary thumbnail task (threaded in production, inline in
    /// tests: same code, determinism preserved).
    pub(crate) thumb_worker: crate::ui::thumb_worker::ThumbWorker,
    /// Cumulative surface-written pixels at the last response (Phase 6G):
    /// used to report the per-frame delta in `OpMetrics`.
    pub(crate) last_surface_pixels: u64,
    /// Cumulative surface time at the last response (Phase 6G.1): same.
    pub(crate) last_surface_us: u128,
    /// TEMPORARY INSTRUMENTATION (perf diagnostics).
    pub metrics: WorkerMetrics,
}

/// Worker tile-cache budget (8M px ≈ 32 MB RGBA8): covers
/// several 1080p documents in 256² tiles with margin for halos.
const TILE_CACHE_BUDGET_PX: usize = 8 << 20;

/// History bounds (shared snapshots, nearly free).
pub const HISTORY_LIMIT: usize = 100;

/// One history step: state to restore + label shown in the
/// History panel. The label travels with the snapshot (undo pushes
/// it to redo and back: redo reapplies the same title).
#[derive(Clone)]
pub(crate) struct HistoryStep {
    /// PRE-mutation state to restore.
    snapshot: photo_engine::history::Snapshot,
    /// French label of the operation (`history_label`).
    label: String,
}

/// "Crop to document" plan (O004): intersection of the
/// layer footprint (offset + dimensions) with the document,
/// converted back to layer space for [`Document::crop`](photo_engine::Document::crop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropPlan {
    /// Not a pixel layer: silent no-op (like `AddFilter`).
    NotPixel,
    /// Rotation/scale/skew: the world intersection would be
    /// wrong — rejected with a clear message.
    Transformed,
    /// No overlap: clean error, no history entry.
    Outside,
    /// Already contained: no-op (avoids an empty history and render,
    /// and any destructive shift).
    Noop,
    /// Rect to crop (layer space, guaranteed bounds).
    Rect { x: i32, y: i32, w: u32, h: u32 },
}

/// Computes the crop plan, pure and testable (no side effects).
fn crop_plan(document: &Document, id: Uuid) -> CropPlan {
    let Some(layer) = document.pixel_layer(id) else {
        return CropPlan::NotPixel;
    };
    let (w, h) = layer.dimensions();
    let Some(transform) = document.transform_of(id) else {
        return CropPlan::NotPixel;
    };
    if transform.rotation_deg != 0.0
        || transform.scale_x != 1.0
        || transform.scale_y != 1.0
        || transform.skew_x != 0.0
        || transform.skew_y != 0.0
    {
        return CropPlan::Transformed;
    }
    let (doc_w, doc_h) = (document.width as f32, document.height as f32);
    let left = transform.offset_x.max(0.0);
    let top = transform.offset_y.max(0.0);
    let right = (transform.offset_x + w as f32).min(doc_w);
    let bottom = (transform.offset_y + h as f32).min(doc_h);
    if right <= left || bottom <= top {
        return CropPlan::Outside;
    }
    // Into layer space (potentially fractional offsets:
    // rounded to the pixel).
    let x = (left - transform.offset_x).round() as i32;
    let y = (top - transform.offset_y).round() as i32;
    let cw = (right - left).round() as u32;
    let ch = (bottom - top).round() as u32;
    if x == 0 && y == 0 && cw == w && ch == h {
        CropPlan::Noop
    } else {
        CropPlan::Rect { x, y, w: cw, h: ch }
    }
}

/// French label of a history step from the operation name
/// (`op_name`). "Édition" fallback when unknown (never empty in UI).
fn history_label(op: &str) -> String {
    String::from(match op {
        "toggle_visibility" => "Visibilité",
        "reorder_nodes" => "Réordonner",
        "move_into_group" => "Déplacer dans le groupe",
        "toggle_collapsed" => "Replier / déplier",
        "set_opacity" => "Opacité",
        "set_blend_mode" => "Fusion",
        "paint_stroke" => "Coup de pinceau",
        "paint_mask" => "Peindre le masque",
        "move_layer" => "Déplacer",
        "open_image" => "Ouvrir une image",
        "add_empty_layer" => "Nouveau calque",
        "duplicate_layer" => "Dupliquer",
        "delete_layer" => "Supprimer",
        "add_filter" => "Ajouter un filtre",
        "rename_layer" => "Renommer",
        "add_mask" => "Ajouter un masque",
        "remove_mask" => "Supprimer le masque",
        "move_filter" => "Déplacer le filtre",
        "move_mask" => "Déplacer le masque",
        "remove_filter" => "Supprimer le filtre",
        "flip_horizontal" => "Miroir horizontal",
        "flip_vertical" => "Miroir vertical",
        "rotate_clockwise" => "Rotation horaire",
        "rotate_counterclockwise" => "Rotation antihoraire",
        "crop_to_document" => "Rogner au document",
        "toggle_filter" => "Basculer le filtre",
        "set_filter_param" => "Réglage du filtre",
        "toggle_mask" => "Basculer le masque",
        _ => "Édition",
    })
}

impl EngineWorker {
    /// Creates a worker on an existing document.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_label: None,
            coalesced_opacity_layer: None,
            clip_to_doc: false,
            revision: RenderRevision::default(),
            pending_tile_plan: None,
            render_worker: photo_engine::document::RenderWorker::new(
                TILE_CACHE_BUDGET_PX,
                photo_engine::tiles::VIEWPORT_TILE_PX,
            ),
            thumb_store: HashMap::new(),
            thumb_pending: HashMap::new(),
            thumb_fresh: Vec::new(),
            thumb_worker: crate::ui::thumb_worker::ThumbWorker::inline_(),
            last_surface_pixels: 0,
            last_surface_us: 0,
            metrics: WorkerMetrics::default(),
        }
    }

    /// TEMPORARY INSTRUMENTATION: read access to the live
    /// document (measurement probes only).
    pub fn test_document(&self) -> &Document {
        &self.document
    }

    /// Pushes the pre-mutation state (never after) and drains redo.
    /// Continuous gestures (sliders) are coalesced per layer.
    /// The step is labeled from [`Self::pending_label`] (operation
    /// name set by `apply`/`apply_batch`): a coalesced
    /// gesture (no push) drops its label with its duplicate.
    fn push_history(&mut self, coalesce_layer: Option<Uuid>) {
        let label = history_label(self.pending_label.take().unwrap_or("edit"));
        if coalesce_layer.is_some() && coalesce_layer == self.coalesced_opacity_layer {
            return;
        }
        if self.undo.len() >= HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(HistoryStep {
            snapshot: self.document.snapshot(),
            label,
        });
        self.redo.clear();
        self.coalesced_opacity_layer = coalesce_layer;
    }

    /// Destructive edit of a pixel layer (mirror, rotation):
    /// guards (pixels only, else silent no-op like
    /// `AddFilter`), PRE-mutation snapshot, application, clean error
    /// with snapshot removal (`ReorderNodes` pattern).
    fn mutate_pixels(
        &mut self,
        id: Uuid,
        invalidation: RenderInvalidation,
        apply: impl FnOnce(&mut Document) -> Result<(), String>,
    ) -> Mutation {
        if !matches!(
            self.document.find(id),
            Some(photo_engine::LayerNode::Pixel(_))
        ) {
            return Mutation::unchanged();
        }
        self.push_history(None);
        if let Err(error) = apply(&mut self.document) {
            self.undo.pop();
            return Mutation::immediate(PhotoEngineResponse::EngineError { message: error });
        }
        Mutation::changed(invalidation)
    }

    /// One step back: restores the snapshot, the undone step (with
    /// its label) goes to redo. False when the stack is empty.
    fn apply_undo_step(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        self.redo.push(HistoryStep {
            snapshot: self.document.snapshot(),
            label: step.label,
        });
        self.document.restore_snapshot(step.snapshot);
        self.coalesced_opacity_layer = None;
        true
    }

    /// One step forward, symmetric. False when the stack is empty.
    fn apply_redo_step(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        self.undo.push(HistoryStep {
            snapshot: self.document.snapshot(),
            label: step.label,
        });
        self.document.restore_snapshot(step.snapshot);
        self.coalesced_opacity_layer = None;
        true
    }

    /// Builds a mutation's response: new composite
    /// (`Composite`, bumped revision) or snapshot only
    /// (`StateOnly`/`Unchanged`, UI texture kept).
    ///
    /// In `Composite` mode, images are resolved ONCE in a local
    /// [`AppearanceFrameCache`] (images only, no thumbnails —
    /// Phase 6G.3) then shared between composite and geometry
    /// (single-pass, without touching the renderer's persistent cache). The
    /// thumbnails come from the secondary store (`stored_thumb`, possibly
    /// one frame late — §18); the snapshot never waits for their computation.
    /// TEMPORARY INSTRUMENTATION: fills `metrics.last` (except
    /// `op`, `total_us` and `mutation_us`, set by `apply`/`apply_batch`).
    fn respond(&mut self, invalidation: RenderInvalidation) -> PhotoEngineResponse {
        let stats_before = self.document.appearance_stats();
        // Previously finished secondary thumbnails: ingested BEFORE the
        // snapshot (the panel benefits from already-done work, no waiting).
        let thumb_completed_before = self.metrics.thumb.completed;
        let thumb_us_before = self.metrics.thumb.thumb_us;
        let thumb_hits_before = self.metrics.thumb.cache_hits;
        let thumb_misses_before = self.metrics.thumb.cache_misses;
        self.poll_thumb_results();
        let t_frame = std::time::Instant::now();
        let frame = (invalidation == RenderInvalidation::Composite)
            .then(|| AppearanceFrameCache::build(&self.document));
        let frame_us = t_frame.elapsed().as_micros();
        // Secondary task: submitted AFTER the frame (entries are
        // cloned, state frozen by the finished mutation) and BEFORE the
        // composite — in threaded mode, computation overlaps canvas rendering.
        if frame.is_some() {
            self.submit_stale_thumbs();
        }
        let t_snapshot = std::time::Instant::now();
        // Thumbnails ALWAYS from the secondary store (even in StateChanged:
        // never a synchronous rebuild for the panel — `None` = icon while
        // waiting for the task, §18).
        let layers = snapshot_layers_with(&self.document, &|id| self.stored_thumb(id));
        let snapshot_us = t_snapshot.elapsed().as_micros();
        let can_undo = !self.undo.is_empty();
        let can_redo = !self.redo.is_empty();
        // History panel labels (O003): stacks oldest
        // to newest, cloned per response (short strings, ≤100).
        let undo_labels = self.undo.iter().map(|step| step.label.clone()).collect();
        let redo_labels = self.redo.iter().map(|step| step.label.clone()).collect();
        if invalidation == RenderInvalidation::Composite {
            let frame = frame.as_ref().expect("cadre construit en mode Composite");
            // 6E incremental pipeline: dirty tiles re-evaluated, clean ones
            // reused; same bytes as legacy at equal content.
            let (preview, timings, worker_stats) = self.render_preview_incremental(frame);
            // Phase 6G.2 §1: dirty is settled only AFTER producing the
            // result (never before, never without rendering). `None` ⇒ nothing to
            // settle (conservative retry next frame).
            // Returns the number of fully settled zones (§8).
            let dirty_cleared = if preview.is_some() {
                let rendered = self.render_worker.take_rendered_tiles();
                self.render_worker.cache_mut().consume_rendered(&rendered)
            } else {
                0
            };
            let revision = self.revision.bump();
            self.metrics.renders += 1;
            let stats_after = self.document.appearance_stats();
            // Tile observation consumed here (zero when the command
            // was not a stroke): the renderer already ran above.
            let tiles = self.pending_tile_plan.take().unwrap_or_default();
            self.metrics.last = Some(OpMetrics {
                op: self.metrics.current_op,
                total_us: 0,
                mutation_us: 0,
                frame_us,
                snapshot_us,
                composite_us: timings.composite_us,
                thumb_us: timings.thumb_us,
                geometry_us: timings.geometry_us,
                appearance_resolves: frame.resolves,
                appearance_frame_hits: frame.hits(),
                preview_rebuilds: stats_after.preview_rebuilds - stats_before.preview_rebuilds,
                thumb_rebuilds: stats_after.thumb_rebuilds - stats_before.thumb_rebuilds,
                blend_us: timings.blend_us,
                layers_blended: timings.layers_blended,
                pixels_processed: timings.pixels_processed,
                scope_px: timings.scope_px,
                full_px: timings.full_px,
                preview_px: timings.preview_px,
                preview_resize_pixels: timings.preview_resize_pixels,
                dirty_region_px: tiles.dirty_region_px,
                dirty_tiles: tiles.dirty_tiles,
                scheduled_tiles: tiles.scheduled_tiles,
                cache_hits: worker_stats.cache_hits,
                cache_misses: worker_stats.cache_misses,
                renders_avoided: worker_stats.cache_hits,
                visible_tiles: worker_stats.visible_tiles,
                requested_tiles: worker_stats.requested_tiles,
                dirty_tiles_cleared: dirty_cleared,
                surface_pixels_written: self
                    .render_worker
                    .surface_stats()
                    .pixels_written
                    .saturating_sub(self.last_surface_pixels),
                schedule_us: worker_stats.schedule_us,
                tile_render_total_us: worker_stats.tile_render_total_us,
                assembly_us: worker_stats.assembly_us,
                surface_update_us: self
                    .render_worker
                    .surface_stats()
                    .update_us
                    .saturating_sub(self.last_surface_us),
                thumbnail_jobs_completed: self
                    .metrics
                    .thumb
                    .completed
                    .saturating_sub(thumb_completed_before),
                thumb_cache_hits: self
                    .metrics
                    .thumb
                    .cache_hits
                    .saturating_sub(thumb_hits_before),
                thumb_cache_misses: self
                    .metrics
                    .thumb
                    .cache_misses
                    .saturating_sub(thumb_misses_before),
                canvas_critical_us: frame_us
                    + timings.composite_us
                    + timings.thumb_us
                    + worker_stats.assembly_us,
                secondary_work_us: self.metrics.thumb.thumb_us.saturating_sub(thumb_us_before),
            });
            // Sliding window: the next response will report its delta.
            self.last_surface_pixels = self.render_worker.surface_stats().pixels_written;
            self.last_surface_us = self.render_worker.surface_stats().update_us;
            PhotoEngineResponse::LayersChanged {
                layers,
                preview,
                revision,
                can_undo,
                can_redo,
                undo_labels,
                redo_labels,
            }
        } else {
            self.metrics.last = Some(OpMetrics {
                op: self.metrics.current_op,
                total_us: 0,
                mutation_us: 0,
                frame_us,
                snapshot_us,
                composite_us: 0,
                thumb_us: 0,
                geometry_us: 0,
                appearance_resolves: 0,
                appearance_frame_hits: 0,
                preview_rebuilds: 0,
                thumb_rebuilds: 0,
                blend_us: 0,
                layers_blended: 0,
                pixels_processed: 0,
                scope_px: 0,
                full_px: 0,
                preview_px: 0,
                dirty_region_px: 0,
                dirty_tiles: 0,
                scheduled_tiles: 0,
                cache_hits: 0,
                cache_misses: 0,
                renders_avoided: 0,
                visible_tiles: 0,
                requested_tiles: 0,
                dirty_tiles_cleared: 0,
                surface_pixels_written: 0,
                schedule_us: 0,
                tile_render_total_us: 0,
                assembly_us: 0,
                surface_update_us: 0,
                preview_resize_pixels: 0,
                thumbnail_jobs_completed: 0,
                thumb_cache_hits: 0,
                thumb_cache_misses: 0,
                canvas_critical_us: 0,
                secondary_work_us: 0,
            });
            PhotoEngineResponse::StateChanged {
                layers,
                revision: self.revision,
                can_undo,
                can_redo,
                undo_labels,
                redo_labels,
            }
        }
    }

    /// Applies one command (1:1 contract: always one response).
    /// Never panics: invalid command (unknown id, out-of-range
    /// index) = no-op (with a state response to resynchronize
    /// the UI, no re-render).
    /// TEMPORARY INSTRUMENTATION: `metrics` is updated here.
    pub fn apply(&mut self, command: PhotoEngineCommand) -> PhotoEngineResponse {
        let t_total = std::time::Instant::now();
        self.metrics.current_op = command.op_name();
        self.pending_label = Some(command.op_name());
        let t_mutation = std::time::Instant::now();
        let mutation = self.mutate(command);
        let mutation_us = t_mutation.elapsed().as_micros();
        let response = match mutation.immediates.into_iter().next() {
            Some(immediate) => immediate,
            None => self.respond(if mutation.changed {
                mutation.invalidation
            } else {
                RenderInvalidation::Unchanged
            }),
        };
        let total_us = t_total.elapsed().as_micros();
        if let Some(last) = self.metrics.last.as_mut() {
            last.op = self.metrics.current_op;
            last.total_us = total_us;
            last.mutation_us = mutation_us;
        }
        self.metrics.responses += 1;
        if matches!(
            response,
            PhotoEngineResponse::LayersChanged {
                preview: Some(_),
                ..
            }
        ) {
            self.metrics.previews += 1;
        }
        response
    }

    /// Applies a batch of commands (worker loop) with coalescing
    /// render: latest-value-wins fallback ([`fold_batch`]), ONE render
    /// for the whole batch. History stays transactional per
    /// command (slider coalescing preserved): undo/redo unchanged.
    /// Returns 0..=N responses (none when the batch folds to empty).
    pub fn apply_batch(&mut self, commands: Vec<PhotoEngineCommand>) -> Vec<PhotoEngineResponse> {
        let t_total = std::time::Instant::now();
        self.metrics.current_op = "batch";
        let folded = fold_batch(commands);
        let had_commands = !folded.is_empty();
        let mut out = Vec::new();
        let mut max_invalidation = RenderInvalidation::Unchanged;
        let mut changed = false;
        // Acks/errors collected separately: they are emitted AFTER the
        // state response to have the last word UI-side (a
        // `LayersChanged` clears the status; the ack must survive).
        let mut immediates = Vec::new();
        let t_mutation = std::time::Instant::now();
        for command in folded {
            self.pending_label = Some(command.op_name());
            let mutation = self.mutate(command);
            immediates.extend(mutation.immediates);
            max_invalidation = max_invalidation.max(mutation.invalidation);
            changed |= mutation.changed;
        }
        let mutation_us = t_mutation.elapsed().as_micros();
        if changed {
            out.push(self.respond(max_invalidation));
        } else if had_commands {
            // Resync without rendering (only no-ops).
            out.push(self.respond(RenderInvalidation::Unchanged));
        }
        out.extend(immediates);
        // Secondary thumbnails finished during the batch (Phase 6G.3 §4A):
        // re-poll (results arrived DURING the composite) then pushed
        // AFTER the main response to never delay the canvas
        // (`drain_thumb_extras` skips the already-embedded ones).
        // Disjoint borrow: `out` is local, `self` is borrowed after.
        self.poll_thumb_results();
        let embedded: &[PhotoLayerInfo] = out
            .iter()
            .rev()
            .find_map(|response| match response {
                PhotoEngineResponse::LayersChanged { layers, .. }
                | PhotoEngineResponse::StateChanged { layers, .. } => Some(layers.as_slice()),
                _ => None,
            })
            .unwrap_or(&[]);
        let extras = self.drain_thumb_extras(embedded);
        out.extend(extras);
        let total_us = t_total.elapsed().as_micros();
        if let Some(last) = self.metrics.last.as_mut() {
            last.op = self.metrics.current_op;
            last.total_us = total_us;
            last.mutation_us = mutation_us;
        }
        self.metrics.responses += out.len() as u64;
        self.metrics.previews += out
            .iter()
            .filter(|response| {
                matches!(
                    response,
                    PhotoEngineResponse::LayersChanged {
                        preview: Some(_),
                        ..
                    }
                )
            })
            .count() as u64;
        out
    }

    /// Body of [`Self::apply`] (instrumentation in the wrapper).
    /// Never renders: applies the mutation, marks the incremental dirty
    /// (Phase 6E §1: centralized here, never at UI callers) and
    /// returns its invalidation (+ immediate export/error-type responses).
    /// Any rendering is produced by [`Self::respond`].
    fn mutate(&mut self, command: PhotoEngineCommand) -> Mutation {
        let mutation = self.mutate_inner(command);
        // Centralized marking: precise when the arm knows (`Region`), safe
        // global fallback on `Composite` without precision, nothing otherwise. A future
        // command at worst forgets precision (wide re-render), never
        // correctness (no stale tile).
        let mark = match mutation.dirty {
            DirtyMark::Auto
                if mutation.changed && mutation.invalidation == RenderInvalidation::Composite =>
            {
                DirtyMark::Global
            }
            DirtyMark::Auto | DirtyMark::None => DirtyMark::None,
            precise => precise,
        };
        match mark {
            DirtyMark::None | DirtyMark::Auto => {}
            DirtyMark::Region(region) => self.render_worker.cache_mut().mark_dirty(region),
            DirtyMark::Global => self
                .render_worker
                .cache_mut()
                .mark_global(self.document.width.max(1), self.document.height.max(1)),
        }
        mutation
    }

    /// Raw application of a command (no marking) — see [`Self::mutate`].
    fn mutate_inner(&mut self, command: PhotoEngineCommand) -> Mutation {
        let (_, invalidation) = render_routing(&command);
        match command {
            PhotoEngineCommand::ToggleLayerVisibility(id) => {
                if self.document.find_mut(id).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                if let Some(node) = self.document.find_mut(id) {
                    let visible = node.visible();
                    node.set_visible(!visible);
                }
                let dirty = self
                    .document
                    .find(id)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::ReorderNodes {
                dragged,
                target,
                before,
            } => {
                // Hierarchical before/after drop: the engine moves the
                // subtree (same parent or not — free nesting).
                if !self.document.can_reorder_before(dragged, target) {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                if !self.document.reorder_before(dragged, target, before) {
                    self.undo.pop();
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::MoveIntoGroup { layer, group } => {
                // Drop inside a group: insert at the head of the children.
                if !self.document.can_move_into(layer, group) {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                if !self.document.move_into(layer, group) {
                    self.undo.pop();
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::ToggleGroupCollapsed(id) => {
                // Display only: no pixel touched (snapshot only).
                // PRE-mutation snapshot (never after).
                if !matches!(
                    self.document.find(id),
                    Some(photo_engine::LayerNode::Group(_))
                ) {
                    return Mutation::unchanged();
                }
                let collapsed = !self.is_collapsed(id);
                self.push_history(None);
                self.document.set_collapsed(id, collapsed);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::SetOpacity { layer, opacity } => {
                if self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                // Coalescing: a single snapshot per slider gesture.
                self.push_history(Some(layer));
                if let Some(node) = self.document.find_mut(layer) {
                    node.set_opacity(opacity);
                }
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::SetBlendMode { layer, mode } => {
                if self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                // Discrete choice: one snapshot per change.
                self.push_history(None);
                if let Some(node) = self.document.find_mut(layer) {
                    node.set_blend_mode(mode);
                }
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::MoveLayer { layer, dx, dy } => {
                // Guards: finite non-zero deltas, pixel layer.
                if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
                    return Mutation::unchanged();
                }
                if self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                // Old transform BEFORE the move (old ∪ new next).
                let before: Option<(photo_engine::Transform2D, u32, u32)> =
                    match self.document.find(layer) {
                        Some(photo_engine::LayerNode::Pixel(pixels)) => Some((
                            pixels.transform,
                            pixels.dimensions().0,
                            pixels.dimensions().1,
                        )),
                        _ => None,
                    };
                if let Some(node) = self.document.find_mut(layer) {
                    // Groups/adjustments: no-op (snapshot already pushed
                    // out of caution, state unchanged).
                    node.translate_by(dx, dy);
                }
                // Precise marking: union of before/after footprints (+exact
                // spread). Without a pixel layer: safe global fallback.
                let dirty = match (before, self.document.find(layer)) {
                    (Some((old_t, w, h)), Some(photo_engine::LayerNode::Pixel(pixels))) => {
                        let spread = photo_engine::tiles::appearance_spread(&pixels.filter_layers);
                        DirtyMark::Region(photo_engine::tiles::layer_move_dirty_region(
                            &old_t,
                            &pixels.transform,
                            w,
                            h,
                            spread,
                        ))
                    }
                    _ => DirtyMark::Global,
                };
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::Undo => {
                if !self.apply_undo_step() {
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::Redo => {
                if !self.apply_redo_step() {
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::UndoSteps { steps } => {
                let mut applied = 0u32;
                for _ in 0..steps {
                    if !self.apply_undo_step() {
                        break;
                    }
                    applied += 1;
                }
                if applied == 0 {
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::RedoSteps { steps } => {
                let mut applied = 0u32;
                for _ in 0..steps {
                    if !self.apply_redo_step() {
                        break;
                    }
                    applied += 1;
                }
                if applied == 0 {
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::PaintStroke {
                layer,
                points,
                eraser,
                radius,
                color,
                opacity,
            } => {
                // Guards: non-finite (NaN/infinite) or zero radius → no-op.
                if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
                    return Mutation::unchanged();
                }
                // Gesture smoothing (1 point/frame): corners softened before
                // rasterization (same conservative bbox for the dirty).
                let points = photo_engine::paint::resample_stroke(&points, radius.max(2.0));
                let is_pixel = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                );
                if !is_pixel {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                let Some(photo_engine::LayerNode::Pixel(pixels)) = self.document.find_mut(layer)
                else {
                    return Mutation::unchanged();
                };
                let source = pixels.source_image.clone();
                let raster = source.to_rgba8();
                let (width, height) = (raster.width(), raster.height());
                let mut buffer = raster.into_raw();
                let brush = photo_engine::paint::BrushParams {
                    radius,
                    color,
                    opacity: opacity.clamp(0.0, 1.0),
                    mode: if eraser {
                        photo_engine::paint::StrokeMode::Erase
                    } else {
                        photo_engine::paint::StrokeMode::Paint
                    },
                };
                photo_engine::paint::paint_stroke_rgba(&mut buffer, width, height, &points, &brush);
                if let Some(image) = image::RgbaImage::from_raw(width, height, buffer) {
                    // Defensive re-read after the history push.
                    if let Some(photo_engine::LayerNode::Pixel(pixels)) =
                        self.document.find_mut(layer)
                    {
                        pixels.set_source_image(image::DynamicImage::ImageRgba8(image));
                        pixels.touch();
                    }
                }
                // Tile OBSERVATION (vertical slice): plan computed on the
                // command's validated inputs, after successful painting.
                // Points in LAYER SPACE → DOCUMENT SPACE region via the
                // layer transform + active-blur halo (Phase 6B).
                // Touches neither the document nor the renderer: the full-frame
                // composite is still produced as before by `respond`.
                // Rasterization above is unchanged (layer space).
                let (layer_transform, layer_w, layer_h, halo) = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => (
                        pixels.transform,
                        pixels.dimensions().0,
                        pixels.dimensions().1,
                        photo_engine::tiles::blur_halo_for_filters(&pixels.filter_layers),
                    ),
                    _ => (
                        photo_engine::Transform2D::default(),
                        self.document.width,
                        self.document.height,
                        photo_engine::tiles::Padding::ZERO,
                    ),
                };
                self.pending_tile_plan = Some(photo_engine::tiles::plan_stroke_tiles_layer_space(
                    &points,
                    radius,
                    &layer_transform,
                    layer_w,
                    layer_h,
                    self.document.width,
                    self.document.height,
                    halo,
                ));
                // Precise marking (cache validity): document footprint +
                // EXACT spread of the appearance chain (not the 6B
                // observation halo) — painting does not move the layer.
                let spread = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        photo_engine::tiles::appearance_spread(&pixels.filter_layers)
                    }
                    _ => photo_engine::tiles::Padding::ZERO,
                };
                let dirty = photo_engine::tiles::stroke_footprint_in_document(
                    &points,
                    radius,
                    &layer_transform,
                    layer_w,
                    layer_h,
                )
                .map(|footprint| DirtyMark::Region(footprint.pad(spread)))
                .unwrap_or(DirtyMark::Global);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::PaintMask {
                owner,
                mask,
                points,
                eraser,
                radius,
                opacity,
            } => {
                // Same guards + smoothing as `PaintStroke`.
                if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
                    return Mutation::unchanged();
                }
                let known = self
                    .document
                    .masks_of(owner)
                    .is_some_and(|masks| masks.iter().any(|m| m.id == mask));
                if !known {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                let points = photo_engine::paint::resample_stroke(&points, radius.max(2.0));
                if self
                    .document
                    .paint_mask(owner, mask, &points, radius, opacity, !eraser)
                    .is_err()
                {
                    self.undo.pop();
                    return Mutation::unchanged();
                }
                // Global marking (safe fallback: the precise dirty follows
                // the layer footprint, the mask shares its space).
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::OpenImage { path } => match image::open(&path) {
                Ok(image) => {
                    self.push_history(None);
                    let name = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("image")
                        .to_owned();
                    self.document.width = self.document.width.max(image.width());
                    self.document.height = self.document.height.max(image.height());
                    self.document.push_layer(photo_engine::LayerNode::Pixel(
                        photo_engine::PixelLayer::new(name, std::sync::Arc::new(image)),
                    ));
                    Mutation::changed(invalidation)
                }
                Err(error) => Mutation::immediate(PhotoEngineResponse::EngineError {
                    message: format!("Ouverture impossible : {}", error),
                }),
            },
            PhotoEngineCommand::AddEmptyLayer => {
                self.push_history(None);
                let (width, height) = (self.document.width.max(1), self.document.height.max(1));
                let blank = image::DynamicImage::new_rgba8(width, height);
                let name = format!("Calque {}", self.document.root.len() + 1);
                self.document.push_layer(photo_engine::LayerNode::Pixel(
                    photo_engine::PixelLayer::new(name, std::sync::Arc::new(blank)),
                ));
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::DuplicateLayer(id) => {
                if self.document.find(id).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.duplicate(id);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::DeleteLayer(id) => {
                // Never an empty document: the last layer is kept.
                if self.document.root.len() <= 1 || self.document.find(id).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.remove(id);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::FlipHorizontal(id) => self.mutate_pixels(id, invalidation, |doc| {
                doc.flip(id, true)
                    .map_err(|error| format!("Miroir impossible : {error}"))
            }),
            PhotoEngineCommand::FlipVertical(id) => self.mutate_pixels(id, invalidation, |doc| {
                doc.flip(id, false)
                    .map_err(|error| format!("Miroir impossible : {error}"))
            }),
            PhotoEngineCommand::RotateClockwise(id) => {
                self.mutate_pixels(id, invalidation, |doc| {
                    doc.rotate(id, true)
                        .map_err(|error| format!("Rotation impossible : {error}"))
                })
            }
            PhotoEngineCommand::RotateCounterclockwise(id) => {
                self.mutate_pixels(id, invalidation, |doc| {
                    doc.rotate(id, false)
                        .map_err(|error| format!("Rotation impossible : {error}"))
                })
            }
            PhotoEngineCommand::CropToDocument(id) => match crop_plan(&self.document, id) {
                CropPlan::NotPixel => Mutation::unchanged(),
                CropPlan::Transformed => Mutation::immediate(PhotoEngineResponse::EngineError {
                    message: String::from("Rogner impossible : redressez le calque d'abord"),
                }),
                CropPlan::Outside => Mutation::immediate(PhotoEngineResponse::EngineError {
                    message: String::from("Rogner impossible : calque hors document"),
                }),
                CropPlan::Noop => Mutation::unchanged(),
                CropPlan::Rect { x, y, w, h } => {
                    self.push_history(None);
                    if let Err(error) = self.document.crop(id, x, y, w, h) {
                        self.undo.pop();
                        return Mutation::immediate(PhotoEngineResponse::EngineError {
                            message: format!("Rogner impossible : {error}"),
                        });
                    }
                    Mutation::changed(invalidation)
                }
            },
            PhotoEngineCommand::AddFilter { layer, filter_type } => {
                let Some(filter) = photo_engine::new_filter_layer(&filter_type) else {
                    return Mutation::immediate(PhotoEngineResponse::EngineError {
                        message: format!("Filtre inconnu : {filter_type}"),
                    });
                };
                let is_pixel = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                );
                if !is_pixel {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                // Re-read after the history push (fresh borrow).
                if let Some(photo_engine::LayerNode::Pixel(pixels)) = self.document.find_mut(layer)
                {
                    pixels.filter_layers.push(filter);
                    pixels.touch();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::Export { path, quality } => {
                Mutation::immediate(match self.document.composite_preview() {
                    Some(image) => {
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_ascii_lowercase);
                        if ext.as_deref() == Some("gif") {
                            match image.save(&path) {
                                Ok(()) => PhotoEngineResponse::ExportDone { path },
                                Err(error) => PhotoEngineResponse::EngineError {
                                    message: format!("Export impossible : {error}"),
                                },
                            }
                        } else {
                            let format = match ext.as_deref() {
                                Some("jpg") | Some("jpeg") => photo_engine::ExportFormat::Jpeg {
                                    quality: quality.clamp(1, 100),
                                },
                                _ => photo_engine::ExportFormat::from_path(&path),
                            };
                            match photo_engine::export_image(&image, &path, format) {
                                Ok(()) => PhotoEngineResponse::ExportDone { path },
                                Err(error) => PhotoEngineResponse::EngineError {
                                    message: format!("Export impossible : {error}"),
                                },
                            }
                        }
                    }
                    None => PhotoEngineResponse::EngineError {
                        message: String::from("Rien a exporter (document vide)"),
                    },
                })
            }
            PhotoEngineCommand::SaveProject { path } => {
                Mutation::immediate(match photo_engine::project::save(&path, &self.document) {
                    Ok(()) => PhotoEngineResponse::ProjectSaved { path },
                    Err(error) => PhotoEngineResponse::EngineError {
                        message: format!("Enregistrement impossible : {error}"),
                    },
                })
            }
            PhotoEngineCommand::LoadProject { path } => {
                match photo_engine::project::load(&path) {
                    Ok(loaded) => {
                        // Total replacement: new editing session.
                        // Neither a pre-mutation snapshot (the old document
                        // goes away), nor caches (changed layer ids:
                        // old doc thumbnails and tiles invalid).
                        self.document = loaded.document;
                        self.undo.clear();
                        self.redo.clear();
                        self.coalesced_opacity_layer = None;
                        self.thumb_store.clear();
                        self.thumb_pending.clear();
                        self.thumb_fresh.clear();
                        Mutation::changed(invalidation)
                    }
                    Err(error) => Mutation::immediate(PhotoEngineResponse::EngineError {
                        message: format!("Ouverture impossible : {error}"),
                    }),
                }
            }
            PhotoEngineCommand::RenameLayer { layer, name } => {
                let name = name.trim().to_owned();
                if name.is_empty() || self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.set_name_any(layer, name);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::AddMask { layer } => {
                // Validation without a held borrow (the history push
                // borrows `self` mutably: no borrow held).
                // `masks_of_mut` (not `masks_of`): adjustments
                // expose an empty slice but refuse insertion.
                if self.document.masks_of_mut(layer).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                let (width, height) = (self.document.width.max(1), self.document.height.max(1));
                if let Some(masks) = self.document.masks_of_mut(layer) {
                    let mut mask = photo_engine::LayerMask::full(width, height);
                    mask.name = format!("Masque {}", masks.len() + 1);
                    masks.push(mask);
                }
                touch_mask_owner(&mut self.document, layer);
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::RemoveMask { owner, mask } => {
                let present = self
                    .document
                    .masks_of(owner)
                    .is_some_and(|masks| masks.iter().any(|m| m.id == mask));
                if !present {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                if let Some(masks) = self.document.masks_of_mut(owner) {
                    masks.retain(|m| m.id != mask);
                }
                touch_mask_owner(&mut self.document, owner);
                let dirty = self
                    .document
                    .find(owner)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::ToggleFilter { layer, filter } => {
                let Some(current) = self.document.find_filter_layer(filter).map(|f| f.enabled)
                else {
                    return Mutation::unchanged();
                };
                self.push_history(None);
                if !self.document.set_filter_enabled(layer, filter, !current) {
                    self.undo.pop();
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::SetFilterParam {
                layer,
                filter,
                key,
                value,
            } => {
                // Continuous gesture (sliders): guards then coalescing
                // per filter (one snapshot per gesture, like opacity
                // per layer — keyed `push_history(Some)`).
                if !value.is_finite() || key.is_empty() {
                    return Mutation::unchanged();
                }
                let known = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                ) && self.document.find_filter_layer(filter).is_some();
                if !known {
                    return Mutation::unchanged();
                }
                self.push_history(Some(filter));
                if !self.document.set_filter_param(
                    layer,
                    filter,
                    key.clone(),
                    datatypes::ParamValue::Float(value),
                ) {
                    self.undo.pop();
                    return Mutation::unchanged();
                }
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::ToggleMask { owner, mask } => {
                let Some(current) = self
                    .document
                    .masks_of(owner)
                    .and_then(|masks| masks.iter().find(|m| m.id == mask))
                    .map(|m| m.enabled)
                else {
                    return Mutation::unchanged();
                };
                self.push_history(None);
                if !self.document.set_mask_enabled(owner, mask, !current) {
                    self.undo.pop();
                    return Mutation::unchanged();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::MoveFilter { layer, filter, up } => {
                // `move_filter` mutates: first validate via an immutable
                // read (position + bounds), then record history, then mutate.
                let movable = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        match pixels.filter_layers.iter().position(|f| f.id == filter) {
                            Some(idx) => {
                                let other = if up {
                                    idx.checked_add(1)
                                } else {
                                    idx.checked_sub(1)
                                };
                                other.is_some_and(|o| o < pixels.filter_layers.len())
                            }
                            None => false,
                        }
                    }
                    _ => false,
                };
                if !movable {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.move_filter(layer, filter, up);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::MoveMask { owner, mask, up } => {
                let movable = match self.document.masks_of(owner) {
                    Some(masks) => match masks.iter().position(|m| m.id == mask) {
                        Some(idx) => {
                            let other = if up {
                                idx.checked_sub(1)
                            } else {
                                idx.checked_add(1)
                            };
                            other.is_some_and(|o| o < masks.len())
                        }
                        None => false,
                    },
                    None => false,
                };
                if !movable {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.move_mask(owner, mask, up);
                touch_mask_owner(&mut self.document, owner);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::RemoveFilter { layer, filter } => {
                let present = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        pixels.filter_layers.iter().any(|f| f.id == filter)
                    }
                    _ => false,
                };
                if !present {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.remove_filter(layer, filter);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::Refresh => Mutation::changed(invalidation),
            PhotoEngineCommand::SetPreviewClip { clip } => {
                // Display preference: no history snapshot.
                // Pixels unchanged (same R ⇒ same render in both modes):
                // no marking, scope detection covers the rest.
                self.clip_to_doc = clip;
                Mutation::changed(invalidation).with_dirty(DirtyMark::None)
            }
        }
    }
}

/// Invalidates a mask bearer's appearance after mutating its
/// masks (the engine's mask setters do not bump the version
/// themselves). Groups: on-the-fly composition, nothing to invalidate.
fn touch_mask_owner(document: &mut Document, owner: Uuid) {
    if let Some(photo_engine::LayerNode::Pixel(pixels)) = document.find_mut(owner) {
        pixels.touch();
    } else if let Some(parent) = document.find_filter_parent(owner)
        && let Some(photo_engine::LayerNode::Pixel(pixels)) = document.find_mut(parent)
    {
        pixels.touch();
    }
}

/// Applies one command to an ephemeral document (tests, no
/// persistent worker or history).
pub fn apply_command(document: &mut Document, command: PhotoEngineCommand) {
    let mut worker = EngineWorker::new(Document::new(0, 0));
    std::mem::swap(&mut worker.document, document);
    let _ = worker.apply(command);
    std::mem::swap(&mut worker.document, document);
}

/// Starts the engine worker on a background thread.
///
/// Blocking `recv` loop (allowed off the UI thread): applies
/// each command then responds. Dropping the UI-side `Sender`
/// cleanly stops the thread.
pub fn spawn_photo_engine_worker(
    document: Document,
    commands: Receiver<PhotoEngineCommand>,
    responses: Sender<PhotoEngineResponse>,
) -> JoinHandle<()> {
    assert_send::<Document>();
    std::thread::spawn(move || {
        let mut worker = EngineWorker::new(document);
        // Threaded secondary task (Phase 6G.3): without this call,
        // thumbnails would be computed inline on the worker thread,
        // hence on the canvas-critical path — exactly what
        // ThumbWorker exists to avoid.
        worker.enable_thumb_thread();
        // Batch loop: the first command blocks (`recv`), then
        // everything already queued is coalesced (`try_recv`) and
        // produces only ONE render (latest-value-wins). Structural
        // commands still apply one by one, in order.
        'worker: while let Ok(first) = commands.recv() {
            let mut batch = vec![first];
            while let Ok(next) = commands.try_recv() {
                batch.push(next);
            }
            for response in worker.apply_batch(batch) {
                if responses.send(response).is_err() {
                    break 'worker;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::{LayerNode, PixelLayer};
    use std::sync::Arc;
    use std::sync::mpsc::channel;

    fn test_image() -> Arc<image::DynamicImage> {
        Arc::new(image::DynamicImage::new_rgba8(4, 4))
    }

    fn three_layer_doc() -> Document {
        let mut doc = Document::new(8, 8);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("milieu", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("dessus", test_image())));
        doc
    }

    #[test]
    fn toggle_visibility_command() {
        let mut doc = three_layer_doc();
        let id = doc.root[0].id();
        assert!(doc.find(id).expect("present").visible());
        apply_command(&mut doc, PhotoEngineCommand::ToggleLayerVisibility(id));
        assert!(!doc.find(id).expect("present").visible());
        apply_command(&mut doc, PhotoEngineCommand::ToggleLayerVisibility(id));
        assert!(doc.find(id).expect("present").visible());
    }

    #[test]
    fn reorder_nodes_moves_by_ids() {
        // Display [top, middle, bottom]; "top" before "bottom"
        // (root order: just above the bottom = bottom of stack
        // displayed): [middle, bottom, top].
        let mut doc = three_layer_doc();
        let dragged = doc.root[2].id();
        let target = doc.root[0].id();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderNodes {
                dragged,
                target,
                before: true,
            },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);
    }

    #[test]
    fn reorder_nodes_bottom_to_top() {
        // "bottom" after "top" (root order: above = very
        // top displayed): [bottom, top, middle].
        let mut doc = three_layer_doc();
        let dragged = doc.root[0].id();
        let target = doc.root[2].id();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderNodes {
                dragged,
                target,
                before: false,
            },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["fond", "dessus", "milieu"]);
    }

    #[test]
    fn reorder_nodes_into_group_and_back() {
        // Free nesting: a layer enters a group then leaves it
        // (before a root layer) — undo/redo follow.
        use photo_engine::GroupLayer;
        let mut worker = EngineWorker::new({
            let mut doc = Document::new(8, 8);
            doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
            doc.push_layer(LayerNode::Group(GroupLayer::new(
                "groupe",
                vec![LayerNode::Pixel(PixelLayer::new("dedans", test_image()))],
            )));
            doc
        });
        let fond = worker.document.root[0].id();
        let groupe = worker.document.root[1].id();
        worker.apply(PhotoEngineCommand::MoveIntoGroup {
            layer: fond,
            group: groupe,
        });
        let layers = snapshot_layers(&worker.document);
        assert_eq!(layers.len(), 1, "tout est dans le groupe");
        assert_eq!(layers[0].children.len(), 2);
        // Exit: "bottom" after the group (root top of stack).
        worker.apply(PhotoEngineCommand::ReorderNodes {
            dragged: fond,
            target: groupe,
            before: false,
        });
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["fond", "groupe"]);
        // Undo x2: back to the initial state (bottom outside the group).
        worker.apply(PhotoEngineCommand::Undo);
        worker.apply(PhotoEngineCommand::Undo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["groupe", "fond"]);
        assert!(snapshot_layers(&worker.document)[1].children.is_empty());
    }

    #[test]
    fn toggle_collapsed_is_state_only_with_undo() {
        // Fallback: snapshot only (no composite), undo restores.
        use photo_engine::GroupLayer;
        let mut worker = EngineWorker::new({
            let mut doc = Document::new(8, 8);
            doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
            doc.push_layer(LayerNode::Group(GroupLayer::new(
                "groupe",
                vec![LayerNode::Pixel(PixelLayer::new("dedans", test_image()))],
            )));
            doc
        });
        let groupe = worker.document.root[1].id();
        let renders = worker.metrics.renders;
        assert_eq!(
            flattened_len(&snapshot_layers(&worker.document)),
            3,
            "groupe déplié : fond + groupe + enfant"
        );
        let response = worker.apply(PhotoEngineCommand::ToggleGroupCollapsed(groupe));
        assert!(
            matches!(response, PhotoEngineResponse::StateChanged { .. }),
            "repli = snapshot seul"
        );
        assert_eq!(worker.metrics.renders, renders, "aucun re-rendu");
        let layers = snapshot_layers(&worker.document);
        assert!(layers[0].collapsed, "groupe replié");
        assert_eq!(flattened_len(&layers), 2, "enfant masqué de la liste");
        // Unknown id: no-op.
        worker.apply(PhotoEngineCommand::ToggleGroupCollapsed(Uuid::new_v4()));
        // Undo: expanded again.
        worker.apply(PhotoEngineCommand::Undo);
        let layers = snapshot_layers(&worker.document);
        assert!(!layers[0].collapsed);
        assert_eq!(flattened_len(&layers), 3);
    }

    #[test]
    fn invalid_commands_are_noops() {
        let mut doc = three_layer_doc();
        let target = doc.root[0].id();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ToggleLayerVisibility(Uuid::new_v4()),
        );
        // Unknown target or onto itself: no-op.
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderNodes {
                dragged: Uuid::new_v4(),
                target,
                before: true,
            },
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderNodes {
                dragged: target,
                target,
                before: true,
            },
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::MoveIntoGroup {
                layer: target,
                group: Uuid::new_v4(),
            },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["dessus", "milieu", "fond"]);
        assert!(snapshot_layers(&doc).iter().all(|l| l.visible));
    }

    #[test]
    fn set_blend_mode_snapshots_and_undoes() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::SetBlendMode {
            layer: id,
            mode: BlendMode::Multiply,
        });
        assert_eq!(
            worker.document.find(id).expect("present").blend_mode(),
            Some(BlendMode::Multiply)
        );
        assert_eq!(worker.undo.len(), 1);
        // Unknown id: no effect, no snapshot.
        worker.apply(PhotoEngineCommand::SetBlendMode {
            layer: Uuid::new_v4(),
            mode: BlendMode::Screen,
        });
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(
            worker.document.find(id).expect("present").blend_mode(),
            Some(BlendMode::Normal)
        );
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        assert_eq!(
            layers
                .iter()
                .find(|l| l.id == id)
                .expect("present")
                .blend_mode,
            BlendMode::Multiply
        );
    }

    #[test]
    fn set_opacity_clamps_and_snapshots() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 250.0,
        });
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        // A single snapshot despite 3 calls (slider coalescing).
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 10.0,
        });
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 20.0,
        });
        assert_eq!(worker.undo.len(), 1);
        // Undo restores the original opacity (100 engine default).
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        assert_eq!(
            layers.iter().find(|l| l.id == id).expect("present").opacity,
            100.0
        );
    }

    #[test]
    fn undo_redo_cycle() {
        let mut worker = EngineWorker::new(three_layer_doc());
        // Undo with no history: state without rendering (no composite).
        let response = worker.apply(PhotoEngineCommand::Undo);
        assert!(matches!(
            response,
            PhotoEngineResponse::StateChanged {
                can_undo: false,
                can_redo: false,
                ..
            }
        ));
        assert_eq!(worker.metrics.renders, 0);
        // "top" before "bottom" (bottom of stack), like the old one (0 -> 3).
        let dragged = worker.document.root[2].id();
        let target = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::ReorderNodes {
            dragged,
            target,
            before: true,
        });
        worker.apply(PhotoEngineCommand::Undo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["dessus", "milieu", "fond"]);
        worker.apply(PhotoEngineCommand::Redo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);
    }

    #[test]
    fn open_missing_image_is_engine_error() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::OpenImage {
            path: PathBuf::from("/chemin/inexistant/photo.png"),
        });
        assert!(matches!(response, PhotoEngineResponse::EngineError { .. }));
        assert_eq!(worker.document.root.len(), 3);
    }

    #[test]
    fn preview_present_after_command() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::ToggleLayerVisibility(
            worker.document.root[0].id(),
        ));
        let PhotoEngineResponse::LayersChanged { preview, .. } = response else {
            panic!("réponse attendue");
        };
        let preview = preview.expect("apercu genere");
        assert_eq!(
            preview.rgba.len(),
            preview.width as usize * preview.height as usize * 4
        );
    }

    #[test]
    fn worker_roundtrip_over_channels() {
        let doc = three_layer_doc();
        let dragged = doc.root[2].id();
        let target = doc.root[0].id();
        let (cmd_tx, cmd_rx) = channel();
        let (resp_tx, resp_rx) = channel();
        let handle = spawn_photo_engine_worker(doc, cmd_rx, resp_tx);

        // The UI thread never blocks: immediate empty poll…
        assert!(resp_rx.try_recv().is_err());
        cmd_tx
            .send(PhotoEngineCommand::ReorderNodes {
                dragged,
                target,
                before: true,
            })
            .expect("envoi commande");
        // …then the worker response (heavy work off-UI).
        let response = resp_rx.recv().expect("reponse worker");
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        let names: Vec<String> = layers.iter().map(|l| l.name.clone()).collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);

        drop(cmd_tx);
        handle.join().expect("arret propre du worker");
    }

    #[test]
    fn rename_trims_and_rejects_empty_or_unknown() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("  fond clair  "),
        });
        assert_eq!(
            worker.document.find(id).expect("present").name(),
            "fond clair"
        );
        assert_eq!(worker.undo.len(), 1);
        // Empty or unknown: no-op without snapshot.
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("   "),
        });
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: Uuid::new_v4(),
            name: String::from("fantome"),
        });
        assert_eq!(
            worker.document.find(id).expect("present").name(),
            "fond clair"
        );
        assert_eq!(worker.undo.len(), 1);
        // Undo restores the original name.
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.find(id).expect("present").name(), "fond");
    }

    #[test]
    fn mask_add_remove_roundtrip_with_undo() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::AddMask { layer: id });
        let masks = worker.document.masks_of(id).expect("masques");
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0].name, "Masque 1");
        let mask_id = masks[0].id;
        // UI snapshot: the mask shows in the HUD.
        let layers = snapshot_layers(&worker.document);
        let info = layers.iter().find(|l| l.id == id).expect("present");
        assert!(info.has_masks);
        assert_eq!(info.masks.len(), 1);
        // Bounds: moving a lone mask = no-op.
        worker.apply(PhotoEngineCommand::MoveMask {
            owner: id,
            mask: mask_id,
            up: true,
        });
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
        // Delete then undo.
        worker.apply(PhotoEngineCommand::RemoveMask {
            owner: id,
            mask: mask_id,
        });
        assert!(worker.document.masks_of(id).expect("masques").is_empty());
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
        // Unknown owner: no-op.
        worker.apply(PhotoEngineCommand::AddMask {
            layer: Uuid::new_v4(),
        });
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
    }

    #[test]
    fn filter_move_and_remove() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        for type_id in ["brightness_contrast", "blur"] {
            let filter = photo_engine::new_filter_layer(type_id).expect("filtre connu");
            worker.document.add_filter(id, filter);
        }
        // Application order: [brightness, blur]; move the first one up.
        let first = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels.filter_layers[0].id,
            _ => panic!("pixels attendus"),
        };
        worker.apply(PhotoEngineCommand::MoveFilter {
            layer: id,
            filter: first,
            up: true,
        });
        let order: Vec<String> = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels
                .filter_layers
                .iter()
                .map(|f| f.type_id.clone())
                .collect(),
            _ => panic!("pixels attendus"),
        };
        assert_eq!(order, ["blur", "brightness_contrast"]);
        // Upper bound: no-op.
        worker.apply(PhotoEngineCommand::MoveFilter {
            layer: id,
            filter: first,
            up: true,
        });
        // Delete.
        worker.apply(PhotoEngineCommand::RemoveFilter {
            layer: id,
            filter: first,
        });
        let remaining = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels.filter_layers.len(),
            _ => panic!("pixels attendus"),
        };
        assert_eq!(remaining, 1);
    }

    #[test]
    fn set_filter_param_applies_coalesces_and_undoes() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let filter = photo_engine::new_filter_layer("brightness_contrast").expect("filtre connu");
        let fid = worker.document.add_filter(id, filter).expect("ajout");
        let param = |worker: &EngineWorker| {
            worker
                .document
                .find_filter_layer(fid)
                .expect("filtre present")
                .params
                .get("brightness")
                .and_then(|v| v.as_float())
                .expect("param flottant")
        };
        assert_eq!(param(&worker), 0.0);
        // Continuous gesture: 3 ticks coalesced into ONE history step.
        for value in [10.0, 20.0, 30.0] {
            let response = worker.apply(PhotoEngineCommand::SetFilterParam {
                layer: id,
                filter: fid,
                key: String::from("brightness"),
                value,
            });
            assert!(
                matches!(response, PhotoEngineResponse::LayersChanged { .. }),
                "re-rendu attendu"
            );
        }
        assert_eq!(param(&worker), 30.0);
        assert_eq!(worker.undo.len(), 1, "geste coalescé");
        assert_eq!(worker.undo[0].label, "Réglage du filtre");
        // Guards: NaN, empty key, unknown ids = no-op without snapshot.
        worker.apply(PhotoEngineCommand::SetFilterParam {
            layer: id,
            filter: fid,
            key: String::from("brightness"),
            value: f32::NAN,
        });
        worker.apply(PhotoEngineCommand::SetFilterParam {
            layer: id,
            filter: fid,
            key: String::new(),
            value: 5.0,
        });
        worker.apply(PhotoEngineCommand::SetFilterParam {
            layer: Uuid::new_v4(),
            filter: fid,
            key: String::from("brightness"),
            value: 5.0,
        });
        worker.apply(PhotoEngineCommand::SetFilterParam {
            layer: id,
            filter: Uuid::new_v4(),
            key: String::from("brightness"),
            value: 5.0,
        });
        assert_eq!(param(&worker), 30.0);
        assert_eq!(worker.undo.len(), 1);
        // Undo: back to the pre-gesture value.
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(param(&worker), 0.0);
    }

    #[test]
    fn move_layer_translates_pixel_transform_with_undo() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 12.0,
            dy: -5.0,
        });
        let moved = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => pixels.transform,
            _ => panic!("pixels attendus"),
        };
        assert_eq!((moved.offset_x, moved.offset_y), (12.0, -5.0));
        assert_eq!(worker.undo.len(), 1);
        // Zero delta or unknown id: no effect, no snapshot.
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 0.0,
            dy: 0.0,
        });
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: Uuid::new_v4(),
            dx: 3.0,
            dy: 3.0,
        });
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        let restored = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => pixels.transform,
            _ => panic!("pixels attendus"),
        };
        assert_eq!((restored.offset_x, restored.offset_y), (0.0, 0.0));
    }

    #[test]
    fn preview_clip_crops_to_document_without_history() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        // Off-frame layer: the infinite-plane preview extends past the document.
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 6.0,
            dy: 0.0,
        });
        let wide = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("apercu"),
            PhotoEngineResponse::EngineError { message } => panic!("{message}"),
            PhotoEngineResponse::ExportDone { .. } => panic!("export inattendu"),
            PhotoEngineResponse::ProjectSaved { .. } => panic!("sauvegarde inattendue"),
            PhotoEngineResponse::StateChanged { .. } => panic!("rendu attendu"),
            PhotoEngineResponse::ThumbnailUpdated { .. } => {
                panic!("pas de miniature seule sur apply solo")
            }
        };
        assert!(wide.full_width > wide.doc_width, "plan infini plus large");
        assert!((wide.origin_x - 2.0).abs() < 1.0, "origine exacte");
        let undo_len = worker.undo.len();
        // Clipping: preview at document dimensions, no history.
        worker.apply(PhotoEngineCommand::SetPreviewClip { clip: true });
        assert_eq!(worker.undo.len(), undo_len, "pas de snapshot");
        let clipped = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("apercu rogne"),
            other => panic!("reponse inattendue : {other:?}"),
        };
        assert_eq!(clipped.full_width, clipped.doc_width);
        assert_eq!(clipped.full_height, clipped.doc_height);
    }

    #[test]
    fn export_png_and_gif_write_files() {
        let dir = std::env::temp_dir().join(format!("cygnus-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let mut worker = EngineWorker::new(three_layer_doc());
        for ext in ["png", "jpg", "gif"] {
            let path = dir.join(format!("rendu.{ext}"));
            let response = worker.apply(PhotoEngineCommand::Export {
                path: path.clone(),
                quality: 80,
            });
            assert!(
                matches!(response, PhotoEngineResponse::ExportDone { .. }),
                "export {ext} attendu"
            );
            assert!(path.is_file(), "fichier {ext} écrit");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_routing_matches_semantics() {
        use RenderEvent::{FullInvalidation, NodeInvalidated};
        use RenderInvalidation::{Composite, StateOnly};
        let id = Uuid::new_v4();
        let other = Uuid::new_v4();
        // Structural: total composite invalidation.
        assert_eq!(
            render_routing(&PhotoEngineCommand::ReorderNodes {
                dragged: id,
                target: other,
                before: true,
            }),
            (FullInvalidation, Composite)
        );
        assert_eq!(
            render_routing(&PhotoEngineCommand::MoveIntoGroup {
                layer: id,
                group: other,
            }),
            (FullInvalidation, Composite)
        );
        // Fallback: only state changes, no pixels.
        assert_eq!(
            render_routing(&PhotoEngineCommand::ToggleGroupCollapsed(id)),
            (NodeInvalidated(id), StateOnly)
        );
        // Visibility: affects global blending (affects_composite).
        assert_eq!(
            render_routing(&PhotoEngineCommand::ToggleLayerVisibility(id)),
            (FullInvalidation, Composite)
        );
        // Opacity: isolated node in the RenderEvent sense, but the CPU
        // composite must be re-blended → Composite.
        assert_eq!(
            render_routing(&PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: 50.0
            }),
            (NodeInvalidated(id), Composite)
        );
        // Rename: only state changes, no pixels.
        assert_eq!(
            render_routing(&PhotoEngineCommand::RenameLayer {
                layer: id,
                name: String::from("x")
            }),
            (NodeInvalidated(id), StateOnly)
        );
    }

    #[test]
    fn batch_opacity_coalesces_to_single_render() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let gestures: Vec<PhotoEngineCommand> = (1..=20)
            .map(|i| PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: i as f32 * 4.0,
            })
            .collect();
        let responses = worker.apply_batch(gestures);
        // 20 intermediate events → 1 response, 1 render.
        assert_eq!(responses.len(), 1, "un seul rendu par geste");
        assert_eq!(worker.metrics.renders, 1);
        let revision = match &responses[0] {
            PhotoEngineResponse::LayersChanged {
                revision,
                preview: Some(_),
                ..
            } => *revision,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(revision, RenderRevision(1));
        // Last value wins, a single history entry.
        assert_eq!(worker.document.find(id).expect("present").opacity(), 80.0);
        assert_eq!(worker.undo.len(), 1);
    }

    #[test]
    fn batch_opacity_undo_restores_initial_value() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let gestures: Vec<PhotoEngineCommand> = (1..=20)
            .map(|i| PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: i as f32 * 4.0,
            })
            .collect();
        worker.apply_batch(gestures);
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(
            worker.document.find(id).expect("present").opacity(),
            100.0,
            "undo restaure la valeur initiale du geste"
        );
        worker.apply(PhotoEngineCommand::Redo);
        assert_eq!(
            worker.document.find(id).expect("present").opacity(),
            80.0,
            "redo réapplique la valeur finale"
        );
    }

    #[test]
    fn batch_move_sums_deltas_with_single_history_entry() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let responses = worker.apply_batch(vec![
            PhotoEngineCommand::MoveLayer {
                layer: id,
                dx: 5.0,
                dy: 0.0,
            },
            PhotoEngineCommand::MoveLayer {
                layer: id,
                dx: -2.0,
                dy: 1.0,
            },
        ]);
        assert_eq!(responses.len(), 1);
        assert_eq!(worker.metrics.renders, 1);
        let offset = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => {
                (pixels.transform.offset_x, pixels.transform.offset_y)
            }
            _ => panic!("pixels attendus"),
        };
        assert_eq!(offset, (3.0, 1.0));
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        let restored = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => {
                (pixels.transform.offset_x, pixels.transform.offset_y)
            }
            _ => panic!("pixels attendus"),
        };
        assert_eq!(restored, (0.0, 0.0));
    }

    #[test]
    fn rename_produces_state_without_render() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        // First one render to pin the revision at 1.
        worker.apply(PhotoEngineCommand::Refresh);
        assert_eq!(worker.metrics.renders, 1);
        let response = worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("renomme"),
        });
        match response {
            PhotoEngineResponse::StateChanged {
                layers, revision, ..
            } => {
                assert_eq!(revision, RenderRevision(1), "pas de nouveau rendu");
                assert!(
                    layers.iter().any(|l| l.id == id && l.name == "renomme"),
                    "snapshot à jour"
                );
            }
            other => panic!("StateChanged attendu, obtenu : {other:?}"),
        }
        assert_eq!(worker.metrics.renders, 1, "aucun rendu pour un renommage");
    }

    #[test]
    fn batch_keeps_export_and_error_responses() {
        let dir = std::env::temp_dir().join(format!("cygnus-batch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let path = dir.join("rendu.png");
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let responses = worker.apply_batch(vec![
            PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: 50.0,
            },
            PhotoEngineCommand::Export {
                path: path.clone(),
                quality: 80,
            },
            PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: 60.0,
            },
        ]);
        // State first, ack next (acks have the last
        // word: a `LayersChanged` clears the status), a single render
        // for the whole batch (folded opacities).
        assert_eq!(responses.len(), 2);
        assert!(matches!(
            responses[0],
            PhotoEngineResponse::LayersChanged { .. }
        ));
        assert!(matches!(
            responses[1],
            PhotoEngineResponse::ExportDone { .. }
        ));
        assert_eq!(worker.metrics.renders, 1);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 60.0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mutation_does_not_invalidate_other_layers_caches() {
        let mut worker = EngineWorker::new(three_layer_doc());
        worker.apply(PhotoEngineCommand::Refresh);
        let before: Vec<(uuid::Uuid, u64)> = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers
                .iter()
                .map(|l| (l.id, l.thumb.as_ref().map(|t| t.version).unwrap_or(0)))
                .collect(),
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let after = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        for (id, version) in before {
            if id == target {
                continue;
            }
            let current = after
                .iter()
                .find(|l| l.id == id)
                .and_then(|l| l.thumb.as_ref().map(|t| t.version));
            assert_eq!(
                current,
                Some(version),
                "le cache des autres calques est intact"
            );
        }
    }
}

#[cfg(test)]
mod fold_batch_tests {
    use super::*;

    fn opacity(id: Uuid, value: f32) -> PhotoEngineCommand {
        PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: value,
        }
    }

    fn shifted(id: Uuid, dx: f32, dy: f32) -> PhotoEngineCommand {
        PhotoEngineCommand::MoveLayer { layer: id, dx, dy }
    }

    #[test]
    fn consecutive_same_layer_opacity_keeps_last() {
        let id = Uuid::new_v4();
        let folded = fold_batch(vec![
            opacity(id, 10.0),
            opacity(id, 20.0),
            opacity(id, 30.0),
        ]);
        assert_eq!(folded, vec![opacity(id, 30.0)]);
    }

    #[test]
    fn opacity_does_not_fold_across_other_commands() {
        let id = Uuid::new_v4();
        let undo = PhotoEngineCommand::Undo;
        // Undo breaks the fold: all three commands survive.
        let folded = fold_batch(vec![opacity(id, 10.0), undo, opacity(id, 30.0)]);
        assert_eq!(folded.len(), 3);
        // Two different layers do not fold either.
        let other = Uuid::new_v4();
        let folded = fold_batch(vec![opacity(id, 10.0), opacity(other, 20.0)]);
        assert_eq!(folded.len(), 2);
    }

    #[test]
    fn consecutive_moves_sum_and_zero_sum_is_dropped() {
        let id = Uuid::new_v4();
        let folded = fold_batch(vec![shifted(id, 5.0, 0.0), shifted(id, -2.0, 1.0)]);
        assert_eq!(folded, vec![shifted(id, 3.0, 1.0)]);
        // Zero sum: command dropped (no render to produce).
        let folded = fold_batch(vec![shifted(id, 5.0, 0.0), shifted(id, -5.0, 0.0)]);
        assert!(folded.is_empty());
    }

    #[test]
    fn noop_batch_returns_single_state_response() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        // Only no-ops: a single resync, no render.
        let responses = worker.apply_batch(vec![
            PhotoEngineCommand::ToggleLayerVisibility(Uuid::new_v4()),
            PhotoEngineCommand::DeleteLayer(Uuid::new_v4()),
        ]);
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            PhotoEngineResponse::StateChanged { .. }
        ));
        assert_eq!(worker.metrics.renders, 0);
    }

    #[test]
    fn batch_with_undo_inside_preserves_order() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let id = worker.document.root[0].id();
        // Opacity applied THEN cancelled in the same batch: back to 100.
        let responses = worker.apply_batch(vec![
            opacity(id, 10.0),
            opacity(id, 20.0),
            PhotoEngineCommand::Undo,
        ]);
        assert_eq!(responses.len(), 1);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        // History keeps the trace: redo replays the folded value.
        worker.apply(PhotoEngineCommand::Redo);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 20.0);
    }

    fn three_layer_doc_for_fold() -> Document {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        let mut doc = Document::new(8, 8);
        for name in ["fond", "milieu", "dessus"] {
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(name, image.clone())));
        }
        doc
    }
}

#[cfg(test)]
mod single_pass_tests {
    use super::*;

    fn layers_of(response: &PhotoEngineResponse) -> Vec<PhotoLayerInfo> {
        match response {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers.clone(),
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
    }

    fn thumb_version(layer: &PhotoLayerInfo) -> u64 {
        layer.thumb.as_ref().map(|t| t.version).unwrap_or(0)
    }

    #[test]
    fn opacity_shares_one_resolution_per_layer_without_rebuilds() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 50.0,
        });
        let m = worker.metrics.last.as_ref().expect("metriques");
        // 3 layers → 3 image resolutions, shared across consumers:
        // scope extents (3, Phase 6E) + composite extents (3) + fold (3) +
        // geometry extents (3) + contributes (1, early exit at the 1st
        // layer). The snapshot no longer resolves thumbnails (Phase 6G.3:
        // secondary store) — hence 13, not 16.
        assert_eq!(m.appearance_resolves, 3);
        assert_eq!(m.appearance_frame_hits, 13);
        // No pixel recomputed: neither preview nor thumb rebuilt.
        assert_eq!(m.preview_rebuilds, 0);
        assert_eq!(m.thumb_rebuilds, 0);
        // The composite itself is still produced (re-blend, revision +1).
        assert_eq!(worker.metrics.renders, 2);
    }

    #[test]
    fn paint_rebuilds_only_touched_layer() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let before = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        let after = layers_of(&worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        }));
        // Painted layer: new version; the others: intact.
        for layer in &after {
            let old = before
                .iter()
                .find(|l| l.id == layer.id)
                .map(thumb_version)
                .unwrap_or(0);
            if layer.id == target {
                assert_ne!(thumb_version(layer), old, "peinture invalide le calque");
            } else {
                assert_eq!(thumb_version(layer), old, "autres calques intacts");
            }
        }
        // Secondary task (inline here): only the painted layer's thumbnail
        // is recomputed — live versions otherwise intact, 6G.3 frame
        // without `preview` (6G.2) or synchronous `thumb`.
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.preview_rebuilds, 0);
        assert_eq!(m.thumb_rebuilds, 0);
        assert_eq!(m.thumbnail_jobs_completed, 1, "seul le calque peint");
    }

    /// Large document (640x480) for multi-case tile tests
    /// (observation tiles: 256 px, single level 0).
    fn wide_doc() -> Document {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let mut doc = Document::new(640, 480);
        let blank = Arc::new(image::DynamicImage::new_rgba8(640, 480));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", blank)));
        doc
    }

    #[test]
    fn paint_stroke_observes_tile_plan() {
        // Case 1+5: small stroke → exact region + 1 tile + 1-plan.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        // Rendering unchanged: composite produced, revision bumped as before.
        match response {
            PhotoEngineResponse::LayersChanged { revision, .. } => {
                assert_eq!(revision, RenderRevision(1));
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        // Observation: bbox [−2,5)² → 49 px², 8x8 doc → single (0,0) tile.
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.dirty_region_px, 49);
        assert_eq!(m.dirty_tiles, 1);
        assert_eq!(m.scheduled_tiles, 1);
    }

    #[test]
    fn paint_stroke_multi_tile_plan() {
        // Case 2: stroke straddling 2x2 256-px tiles.
        let mut worker = EngineWorker::new(wide_doc());
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(10.0, 10.0), (500.0, 400.0)],
            eraser: false,
            radius: 8.0,
            color: [255, 0, 0],
            opacity: 1.0,
        });
        assert!(
            matches!(response, PhotoEngineResponse::LayersChanged { .. }),
            "rendu produit comme avant"
        );
        // x ∈ [2,508) → tiles 0,1; y ∈ [2,408) → tiles 0,1.
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.dirty_region_px, 506 * 406);
        assert_eq!(m.dirty_tiles, 4);
        assert_eq!(m.scheduled_tiles, 4, "budget ouvert : tout planifié");
    }

    #[test]
    fn stroke_outside_document_marks_no_tiles() {
        // Case 3: real region but off-surface → grid clipping,
        // no invalid tiles (and above all no ghost tiles).
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(5000.0, 5000.0)],
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        assert!(
            matches!(response, PhotoEngineResponse::LayersChanged { .. }),
            "peinture commise (pixels inchangés hors calque)"
        );
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert!(m.dirty_region_px > 0, "région calculée");
        assert_eq!(m.dirty_tiles, 0);
        assert_eq!(m.scheduled_tiles, 0);
    }

    #[test]
    fn invalid_stroke_observes_nothing() {
        // Case 4: unusable gesture → no-op, no tile work.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![],
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        assert!(
            matches!(response, PhotoEngineResponse::StateChanged { .. }),
            "no-op sans rendu"
        );
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.dirty_region_px, 0);
        assert_eq!(m.dirty_tiles, 0);
        assert_eq!(m.scheduled_tiles, 0);
    }

    #[test]
    fn paint_commit_undo_redo_geste_complet() {
        // Full gesture: ONE PaintStroke at MouseUp (the whole
        // canvas-coalesced drag) ⇒ ONE history entry; Undo
        // restores the pixels, Redo replays the stroke.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let pixel = |worker: &EngineWorker| {
            worker
                .test_document()
                .pixel_layer(target)
                .expect("calque present")
                .source_image
                .to_rgba8()
                .get_pixel(1, 1)
                .0
        };
        assert_eq!(pixel(&worker)[3], 0, "fond transparent au départ");
        // Points as the canvas accumulates them during the drag.
        let points: Vec<(f32, f32)> = (0..8).map(|i| (i as f32, i as f32)).collect();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        match response {
            PhotoEngineResponse::LayersChanged {
                revision,
                can_undo,
                can_redo,
                ..
            } => {
                assert_eq!(revision, RenderRevision(1));
                assert!(can_undo);
                assert!(!can_redo);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        assert_eq!(worker.undo.len(), 1, "un geste = une entrée");
        assert_eq!(&pixel(&worker)[0..3], &[0, 0, 255]);
        assert_eq!(pixel(&worker)[3], 255);
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(pixel(&worker)[3], 0, "undo restaure le transparent");
        assert_eq!(worker.undo.len(), 0);
        worker.apply(PhotoEngineCommand::Redo);
        assert_eq!(&pixel(&worker)[0..3], &[0, 0, 255], "redo rejoue le trait");
        assert_eq!(worker.undo.len(), 1);
    }

    /// Bench 6G.4: thumbnail cost BEFORE/AFTER the versioned cache.
    /// Exact counters (submitted/completed/thumb_us) + indicative walls.
    /// Same bench on both sides: phases A (N distinct paints —
    /// needed computations), B (add/remove filter cycle: same source
    /// Arc, restored signature), C (toggle/opacity/undo/redo:
    /// zero rebuild expected). Release recommended for walls.
    #[test]
    #[ignore = "bench 6G.4 (murs indicatifs, compteurs exacts)"]
    fn bench_thumb_6g4() {
        use photo_engine::{LayerNode, PixelLayer};
        use std::time::Instant;
        fn blank(w: u32, h: u32) -> Document {
            let mut doc = Document::new(w, h);
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(
                "fond",
                Arc::new(image::DynamicImage::new_rgba8(w, h)),
            )));
            doc
        }
        fn counters(worker: &EngineWorker) -> (u64, u64, u128, u64) {
            let t = &worker.metrics.thumb;
            (t.submitted, t.completed, t.thumb_us, t.cache_hits)
        }
        eprintln!("BENCH6G4|phase|doc|n|wall_ms|submitted|completed|cache_hits|thumb_us|renders");
        // Phase A: distinct paints (all needed).
        for (w, h, n) in [
            (640u32, 480u32, 100usize),
            (1920, 1080, 20),
            (2048, 2048, 10),
            (4096, 4096, 5),
        ] {
            let mut worker = EngineWorker::new(blank(w, h));
            worker.apply(PhotoEngineCommand::Refresh);
            let target = worker.document.root[0].id();
            let before = counters(&worker);
            let start = Instant::now();
            for i in 0..n {
                let x = 10.0 + (i as f32 * 37.0) % (w as f32 - 20.0);
                let y = 10.0 + (i as f32 * 53.0) % (h as f32 - 20.0);
                worker.apply(PhotoEngineCommand::PaintStroke {
                    layer: target,
                    points: vec![(x, y), (x + 8.0, y + 5.0)],
                    eraser: false,
                    radius: 8.0,
                    color: [200, 30, 30],
                    opacity: 1.0,
                });
            }
            let wall_ms = start.elapsed().as_millis();
            let after = counters(&worker);
            eprintln!(
                "BENCH6G4|A|{w}x{h}|{n}|{wall_ms}|{}|{}|{}|{}|{}",
                after.0 - before.0,
                after.1 - before.1,
                after.3 - before.3,
                after.2 - before.2,
                worker.metrics.renders,
            );
        }
        // Phase B: filter cycle (same source, restored signature).
        {
            let mut worker = EngineWorker::new(blank(640, 480));
            worker.apply(PhotoEngineCommand::Refresh);
            let target = worker.document.root[0].id();
            worker.apply(PhotoEngineCommand::AddFilter {
                layer: target,
                filter_type: String::from("brightness_contrast"),
            });
            let filter = match worker.document.find(target).expect("calque") {
                LayerNode::Pixel(p) => p.filter_layers[0].id,
                _ => panic!("pixels attendus"),
            };
            let before = counters(&worker);
            let start = Instant::now();
            worker.apply(PhotoEngineCommand::RemoveFilter {
                layer: target,
                filter,
            });
            let wall_ms = start.elapsed().as_millis();
            let after = counters(&worker);
            eprintln!(
                "BENCH6G4|B|640x480|1|{wall_ms}|{}|{}|{}|{}|{}",
                after.0 - before.0,
                after.1 - before.1,
                after.3 - before.3,
                after.2 - before.2,
                worker.metrics.renders,
            );
        }
        // Phase C: toggle/opacity/undo/redo (zero rebuild expected).
        {
            let mut worker = EngineWorker::new(blank(640, 480));
            worker.apply(PhotoEngineCommand::Refresh);
            let target = worker.document.root[0].id();
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: target,
                points: vec![(10.0, 10.0), (30.0, 25.0)],
                eraser: false,
                radius: 8.0,
                color: [200, 30, 30],
                opacity: 1.0,
            });
            let before = counters(&worker);
            worker.apply(PhotoEngineCommand::ToggleLayerVisibility(target));
            worker.apply(PhotoEngineCommand::SetOpacity {
                layer: target,
                opacity: 50.0,
            });
            worker.apply(PhotoEngineCommand::Undo);
            worker.apply(PhotoEngineCommand::Undo);
            worker.apply(PhotoEngineCommand::Redo);
            worker.apply(PhotoEngineCommand::Redo);
            let after = counters(&worker);
            eprintln!(
                "BENCH6G4|C|640x480|6|0|{}|{}|{}|{}|{}",
                after.0 - before.0,
                after.1 - before.1,
                after.3 - before.3,
                after.2 - before.2,
                worker.metrics.renders,
            );
        }
    }

    #[test]
    #[ignore = "mesure commit (mur indicatif, jamais asserté)"]
    fn perf_commit_2048() {
        // Commit-to-MouseUp time on 2048²: mutation + composite +
        // secondary thumbnail (inline here). Indicative wall only.
        let mut worker = EngineWorker::new({
            use photo_engine::{LayerNode, PixelLayer};
            use std::sync::Arc;
            let mut doc = Document::new(2048, 2048);
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(
                "fond",
                Arc::new(image::DynamicImage::new_rgba8(2048, 2048)),
            )));
            doc
        });
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..64)
            .map(|i| (100.0 + i as f32 * 24.0, 260.0 + i as f32 * 3.0))
            .collect();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 8.0,
            color: [200, 30, 30],
            opacity: 1.0,
        });
        let m = worker.metrics.last.as_ref().expect("metriques");
        eprintln!(
            "commit-2048 : total={}µs mutation={}µs composite={}µs canvas_critical={}µs dirty_px={} dirty_tiles={}",
            m.total_us,
            m.mutation_us,
            m.composite_us,
            m.canvas_critical_us,
            m.dirty_region_px,
            m.dirty_tiles,
        );
    }

    #[test]
    fn paint_commit_chemin_canvas_critical_avant_miniatures() {
        // After MouseUp: commit ⇒ observed DirtyRegion + canvas render
        // (bumped revision, preview present) + secondary thumbnails
        // (1 job, never during the drag — the drag sends nothing).
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let submitted_before = worker.metrics.thumb.submitted;
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(1.0, 1.0), (2.0, 2.0)],
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert!(m.dirty_region_px > 0, "DirtyRegion calculée au commit");
        assert!(
            m.canvas_critical_us >= m.composite_us,
            "canvas-critical mesuré"
        );
        assert_eq!(
            worker.metrics.thumb.submitted,
            submitted_before + 1,
            "miniature soumise AU commit, jamais pendant le drag"
        );
        assert_eq!(m.thumbnail_jobs_completed, 1);
        match response {
            PhotoEngineResponse::LayersChanged { preview, .. } => {
                assert!(preview.is_some(), "CanvasCritical produit le preview");
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
    }

    #[test]
    fn paint_grand_document_1920_reste_borne() {
        // 1920x1080: committing a small stroke ⇒ small dirty region
        // next to the document, marked tiles < total, revision +1.
        let mut worker = EngineWorker::new({
            use photo_engine::{LayerNode, PixelLayer};
            use std::sync::Arc;
            let mut doc = Document::new(1920, 1080);
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(
                "fond",
                Arc::new(image::DynamicImage::new_rgba8(1920, 1080)),
            )));
            doc
        });
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(100.0, 100.0), (300.0, 250.0)],
            eraser: false,
            radius: 8.0,
            color: [200, 30, 30],
            opacity: 1.0,
        });
        match response {
            PhotoEngineResponse::LayersChanged { revision, .. } => {
                assert_eq!(revision, RenderRevision(1));
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        let m = worker.metrics.last.as_ref().expect("metriques");
        let doc_px = 1920u64 * 1080;
        assert!(m.dirty_region_px < doc_px / 10, "région bornée au trait");
        // 256-px grid over 1920x1080 ⇒ 8x5 = 40 tiles at most.
        assert!(m.dirty_tiles < 40, "tuiles marquées < total");
        assert!(m.dirty_tiles >= 1);
    }

    /// Cumulative thumbnail counters (submissions, completions, HIT/MISS).
    fn thumb_counters(worker: &EngineWorker) -> (u64, u64, u64, u64) {
        let t = &worker.metrics.thumb;
        (t.submitted, t.completed, t.cache_hits, t.cache_misses)
    }

    #[test]
    fn cache_cycle_filtre_ressert_sans_recalcul() {
        use photo_engine::LayerNode;
        // 6G.4: paint (MISS) → add filter (MISS, same Arc) →
        // remove filter (same Arc + restored signature ⇒ HIT).
        // Identical bytes before/after the cycle, zero recompute.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        let paint = |worker: &mut EngineWorker| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: target,
                points: vec![(1.0, 1.0), (2.0, 2.0)],
                eraser: false,
                radius: 2.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        paint(&mut worker);
        let thumb_painted = worker
            .thumb_store
            .get(&target)
            .expect("miniature peinte")
            .0
            .clone();
        worker.apply(PhotoEngineCommand::AddFilter {
            layer: target,
            filter_type: String::from("brightness_contrast"),
        });
        let filter = match worker.document.find(target).expect("calque") {
            LayerNode::Pixel(p) => p.filter_layers[0].id,
            _ => panic!("pixels attendus"),
        };
        let before = thumb_counters(&worker);
        worker.apply(PhotoEngineCommand::RemoveFilter {
            layer: target,
            filter,
        });
        let after = thumb_counters(&worker);
        assert_eq!(after.0 - before.0, 1, "retrait soumis (nouvelle version)");
        assert_eq!(after.1 - before.1, 1, "résultat ingéré");
        assert_eq!(after.2 - before.2, 1, "signature restaurée ⇒ HIT");
        assert_eq!(after.3 - before.3, 0, "aucun calcul");
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.thumb_cache_hits, 1);
        assert_eq!(m.thumb_cache_misses, 0);
        let thumb_restored = worker
            .thumb_store
            .get(&target)
            .expect("miniature restaurée")
            .0
            .clone();
        assert_eq!(
            thumb_restored.data.as_ref(),
            thumb_painted.data.as_ref(),
            "HIT byte-for-byte avec l'état peint"
        );
    }

    #[test]
    fn cache_hit_perime_rejete() {
        use photo_engine::LayerNode;
        // 6G.4 + 6G.3: a cache HIT tagged with an outdated version
        // is rejected by the guard (never displayed, never stored —
        // but counted as HIT: no computation happened).
        use crate::ui::thumb_worker::ThumbDone;
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        let paint = |worker: &mut EngineWorker, x: f32| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: target,
                points: vec![(x, x), (x + 1.0, x + 1.0)],
                eraser: false,
                radius: 2.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        paint(&mut worker, 1.0);
        let (thumb_v1, source_v1, version1) = {
            let layer = match worker.document.find(target).expect("calque") {
                LayerNode::Pixel(p) => p,
                _ => panic!("pixels attendus"),
            };
            (
                worker.thumb_store.get(&target).expect("store v1").0.clone(),
                Arc::clone(&layer.source_image),
                layer.appearance_version,
            )
        };
        // New paint: version 2, new source Arc.
        paint(&mut worker, 3.0);
        let version2 = match worker.document.find(target).expect("calque") {
            LayerNode::Pixel(p) => p.appearance_version,
            _ => panic!("pixels attendus"),
        };
        assert_ne!(version1, version2);
        // Replay of the v1 result via the cache (content HIT, stale version).
        let replay = ThumbDone {
            layer: target,
            version: version1,
            thumb: thumb_v1,
            image: source_v1,
            render_us: 0,
            thumb_us: 0,
            from_cache: true,
        };
        let hits_before = worker.metrics.thumb.cache_hits;
        assert!(!worker.ingest_thumb_done(replay), "v1 périmé rejeté");
        assert_eq!(worker.metrics.thumb.cache_hits, hits_before + 1);
        assert!(
            worker
                .thumb_store
                .get(&target)
                .is_some_and(|(_, v)| *v == version2),
            "store v2 intact"
        );
    }

    #[test]
    fn dix_paints_distincts_minimum_calculs() {
        // 6G.4: 10 distinct states ⇒ 10 needed computations, no more
        // (no parasitic resubmission) and no undue HIT (all contents
        // different).
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        let before = thumb_counters(&worker);
        for i in 0..10 {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: target,
                points: vec![(i as f32, 0.0), (i as f32 + 0.5, 1.0)],
                eraser: false,
                radius: 1.0,
                color: [i as u8 * 25, 0, 255],
                opacity: 1.0,
            });
        }
        let after = thumb_counters(&worker);
        assert_eq!(after.0 - before.0, 10, "10 soumissions");
        assert_eq!(after.1 - before.1, 10, "10 complétions");
        assert_eq!(after.2 - before.2, 0, "aucun faux HIT");
        assert_eq!(after.3 - before.3, 10, "10 MISS nécessaires");
    }

    #[test]
    fn toggle_opacity_undo_redo_sans_rebuild() {
        // Preserved 6G.3: toggle/opacity (unchanged version) then
        // undo/redo (restored versions, intact store) ⇒ zero
        // submission, zero computation, zero HIT.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(1.0, 1.0), (2.0, 2.0)],
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let before = thumb_counters(&worker);
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(target));
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: target,
            opacity: 50.0,
        });
        worker.apply(PhotoEngineCommand::Undo);
        worker.apply(PhotoEngineCommand::Undo);
        worker.apply(PhotoEngineCommand::Redo);
        worker.apply(PhotoEngineCommand::Redo);
        let after = thumb_counters(&worker);
        assert_eq!(after, before, "aucun travail miniature");
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!((m.thumb_cache_hits, m.thumb_cache_misses), (0, 0));
    }

    #[test]
    fn filter_change_rebuilds_only_concerned_layer() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::AddFilter {
            layer: target,
            filter_type: String::from("brightness_contrast"),
        });
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.preview_rebuilds, 0, "cadre 6G.2 : pas de preview dérivé");
        assert_eq!(m.thumb_rebuilds, 0, "cadre 6G.3 : miniature en tâche");
        assert_eq!(
            m.thumbnail_jobs_completed, 1,
            "seule la chaîne du calque rejoue (secondaire)"
        );
    }

    #[test]
    fn undo_redo_revalidates_versions() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let pristine = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        let target = worker.document.root[0].id();
        let pristine_thumb = pristine
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        let painted = layers_of(&worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        }));
        let painted_thumb = painted
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        assert_ne!(painted_thumb.version, pristine_thumb.version);
        // Undo: original pixels and version restored (targeted rebuild).
        let undone = layers_of(&worker.apply(PhotoEngineCommand::Undo));
        let undone_thumb = undone
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        assert_eq!(undone_thumb.version, pristine_thumb.version);
        assert_eq!(undone_thumb.rgba, pristine_thumb.rgba);
        // Redo: exact return to the painted state.
        let redone = layers_of(&worker.apply(PhotoEngineCommand::Redo));
        let redone_thumb = redone
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        assert_eq!(redone_thumb.version, painted_thumb.version);
        assert_eq!(redone_thumb.rgba, painted_thumb.rgba);
    }

    #[test]
    fn export_neither_renders_nor_bumps_revision() {
        let dir = std::env::temp_dir().join(format!("cygnus-sp-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        assert_eq!(worker.metrics.renders, 1);
        let path = dir.join("rendu.png");
        let response = worker.apply(PhotoEngineCommand::Export {
            path: path.clone(),
            quality: 80,
        });
        assert!(matches!(response, PhotoEngineResponse::ExportDone { .. }));
        assert!(path.is_file(), "fichier écrit");
        assert_eq!(worker.metrics.renders, 1, "export sans rendu");
        // The next refresh bumps by exactly one revision.
        match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { revision, .. } => {
                assert_eq!(revision, RenderRevision(2));
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_then_load_project_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cygnus-sp-projet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let path = dir.join("projet.cygp");
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let before = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        let renders = worker.metrics.renders;
        let response = worker.apply(PhotoEngineCommand::SaveProject { path: path.clone() });
        assert!(
            matches!(response, PhotoEngineResponse::ProjectSaved { .. }),
            "accusé de sauvegarde"
        );
        assert!(path.is_file(), "fichier écrit");
        assert_eq!(worker.metrics.renders, renders, "sauvegarde sans rendu");

        // Fresh worker: loading restores the full tree.
        let mut fresh = EngineWorker::new(Document::new(8, 8));
        let loaded = match fresh.apply(PhotoEngineCommand::LoadProject { path }) {
            PhotoEngineResponse::LayersChanged {
                layers, can_undo, ..
            } => {
                assert!(!can_undo, "historique vierge après chargement");
                layers
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(loaded.len(), before.len(), "même nombre de calques");
        for (restaure, origine) in loaded.iter().zip(before.iter()) {
            assert_eq!(restaure.name, origine.name);
            assert_eq!(restaure.opacity, origine.opacity);
            assert_eq!(restaure.visible, origine.visible);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_project_unknown_version_is_clean_error() {
        let dir = std::env::temp_dir().join(format!("cygnus-sp-projet-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let path = dir.join("vieux.cygp");
        std::fs::write(&path, r#"{"version":1,"width":4,"height":4,"root":[]}"#)
            .expect("fixture v1");
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        match worker.apply(PhotoEngineCommand::LoadProject { path }) {
            PhotoEngineResponse::EngineError { message } => {
                assert!(message.contains("non supportée"), "{message}");
            }
            other => panic!("EngineError attendue, obtenue : {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn history_labels_follow_operations_in_order() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::AddEmptyLayer);
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(target));
        let (undo_labels, redo_labels) = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged {
                undo_labels,
                redo_labels,
                can_undo,
                ..
            } => {
                assert!(can_undo);
                (undo_labels, redo_labels)
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(undo_labels, vec!["Nouveau calque", "Visibilité"]);
        assert!(redo_labels.is_empty());
        // One undo: the label travels to redo.
        match worker.apply(PhotoEngineCommand::Undo) {
            PhotoEngineResponse::LayersChanged {
                undo_labels,
                redo_labels,
                ..
            } => {
                assert_eq!(undo_labels, vec!["Nouveau calque"]);
                assert_eq!(redo_labels, vec!["Visibilité"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
    }

    #[test]
    fn history_steps_jump_beyond_stack_in_one_render() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::AddEmptyLayer);
        worker.apply(PhotoEngineCommand::AddEmptyLayer);
        let renders = worker.metrics.renders;
        // Backward jump past the stack: everything undone, one render.
        match worker.apply(PhotoEngineCommand::UndoSteps { steps: 5 }) {
            PhotoEngineResponse::LayersChanged {
                undo_labels,
                redo_labels,
                can_undo,
                can_redo,
                ..
            } => {
                assert!(!can_undo);
                assert!(can_redo);
                assert!(undo_labels.is_empty());
                assert_eq!(redo_labels, vec!["Nouveau calque", "Nouveau calque"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        assert_eq!(worker.metrics.renders, renders + 1);
        // Forward jump of 1: reapplies the oldest.
        match worker.apply(PhotoEngineCommand::RedoSteps { steps: 1 }) {
            PhotoEngineResponse::LayersChanged {
                undo_labels,
                redo_labels,
                ..
            } => {
                assert_eq!(undo_labels, vec!["Nouveau calque"]);
                assert_eq!(redo_labels, vec!["Nouveau calque"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        // Zero jump: no-op without rendering.
        assert!(matches!(
            worker.apply(PhotoEngineCommand::UndoSteps { steps: 0 }),
            PhotoEngineResponse::StateChanged { .. }
        ));
    }

    /// Asymmetric 4×2 document: left half red, right half black
    /// (mirror/rotation visible in the preview).
    fn asymmetric_doc() -> Document {
        use image::{DynamicImage, ImageBuffer, Rgba};
        use std::sync::Arc;
        let mut buf = ImageBuffer::from_pixel(4, 2, Rgba([0, 0, 0, 255]));
        buf.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        buf.put_pixel(0, 1, Rgba([255, 0, 0, 255]));
        let mut doc = Document::new(4, 2);
        doc.push_layer(photo_engine::LayerNode::Pixel(
            photo_engine::PixelLayer::new("asym", Arc::new(DynamicImage::ImageRgba8(buf))),
        ));
        doc
    }

    fn preview_rgba(response: &PhotoEngineResponse) -> Vec<u8> {
        match response {
            PhotoEngineResponse::LayersChanged {
                preview: Some(preview),
                ..
            } => preview.rgba.clone(),
            other => panic!("aperçu attendu, obtenu : {other:?}"),
        }
    }

    #[test]
    fn flip_miroir_change_l_apercu_libelle_et_annule() {
        let mut worker = EngineWorker::new(asymmetric_doc());
        let id = worker.test_document().root[0].id();
        let avant = preview_rgba(&worker.apply(PhotoEngineCommand::Refresh));
        match worker.apply(PhotoEngineCommand::FlipHorizontal(id)) {
            PhotoEngineResponse::LayersChanged {
                preview: Some(apercu),
                undo_labels,
                ..
            } => {
                assert_ne!(apercu.rgba, avant, "miroir visible");
                assert_eq!(undo_labels, vec!["Miroir horizontal"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        // Undo: original pixels restored byte-for-byte.
        let restaure = preview_rgba(&worker.apply(PhotoEngineCommand::Undo));
        assert_eq!(restaure, avant);
    }

    #[test]
    fn rotate_et_crop_travaillent_et_sont_libelles() {
        let mut worker = EngineWorker::new(asymmetric_doc());
        let id = worker.test_document().root[0].id();
        let avant = preview_rgba(&worker.apply(PhotoEngineCommand::Refresh));
        match worker.apply(PhotoEngineCommand::RotateClockwise(id)) {
            PhotoEngineResponse::LayersChanged { undo_labels, .. } => {
                assert_eq!(undo_labels, vec!["Rotation horaire"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        let tourne = preview_rgba(&worker.apply(PhotoEngineCommand::Refresh));
        assert_ne!(tourne, avant, "rotation visible");
        // Group: silent no-op (like AddFilter), no history.
        let mut groupe = Document::new(4, 4);
        groupe.push_layer(photo_engine::LayerNode::Group(
            photo_engine::GroupLayer::new("g", vec![]),
        ));
        let gid = groupe.root[0].id();
        let mut pivot = EngineWorker::new(groupe);
        assert!(matches!(
            pivot.apply(PhotoEngineCommand::FlipHorizontal(gid)),
            PhotoEngineResponse::StateChanged { .. }
        ));
    }

    #[test]
    fn crop_plan_couvre_tous_les_cas() {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let mut doc = Document::new(10, 10);
        let img = Arc::new(image::DynamicImage::new_rgba8(6, 6));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("carre", img)));
        let id = doc.root[0].id();
        // Content: no-op (already in the document).
        assert_eq!(crop_plan(&doc, id), CropPlan::Noop);
        // Shifted half out of frame: non-null clamped rect.
        doc.set_transform_any(
            id,
            photo_engine::Transform2D {
                offset_x: 7.0,
                ..photo_engine::Transform2D::default()
            },
        );
        assert_eq!(
            crop_plan(&doc, id),
            CropPlan::Rect {
                x: 0,
                y: 0,
                w: 3,
                h: 6
            }
        );
        // Fully out of frame: clean error.
        doc.set_transform_any(
            id,
            photo_engine::Transform2D {
                offset_x: 20.0,
                ..photo_engine::Transform2D::default()
            },
        );
        assert_eq!(crop_plan(&doc, id), CropPlan::Outside);
        // Rotation: rejected (wrong world intersection).
        doc.set_transform_any(
            id,
            photo_engine::Transform2D {
                offset_x: 0.0,
                rotation_deg: 45.0,
                ..photo_engine::Transform2D::default()
            },
        );
        assert_eq!(crop_plan(&doc, id), CropPlan::Transformed);
    }

    #[test]
    fn toggle_bascule_filtre_et_masque_avec_historique() {
        use photo_engine::FilterLayer;
        use std::collections::HashMap;
        let mut doc = Document::new(8, 8);
        let mut pixels = photo_engine::PixelLayer::new(
            "fond",
            std::sync::Arc::new(image::DynamicImage::new_rgba8(4, 4)),
        );
        let filtre = FilterLayer::neutral("brightness_contrast", HashMap::new());
        let fid = filtre.id;
        pixels.filter_layers.push(filtre);
        let masque = photo_engine::LayerMask::full(4, 4);
        let mid = masque.id;
        pixels.masks.push(masque);
        let id = pixels.id;
        doc.push_layer(photo_engine::LayerNode::Pixel(pixels));
        let mut worker = EngineWorker::new(doc);
        // Active filter → inactive.
        match worker.apply(PhotoEngineCommand::ToggleFilter {
            layer: id,
            filter: fid,
        }) {
            PhotoEngineResponse::LayersChanged { undo_labels, .. } => {
                assert_eq!(undo_labels, vec!["Basculer le filtre"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        let info = worker.test_document().pixel_layer(id).expect("calque");
        assert!(!info.filter_layers[0].enabled);
        // Active mask → inactive.
        worker.apply(PhotoEngineCommand::ToggleMask {
            owner: id,
            mask: mid,
        });
        assert!(
            !worker
                .test_document()
                .pixel_layer(id)
                .expect("calque")
                .masks[0]
                .enabled
        );
        // Undo ×2: both are reactivated.
        worker.apply(PhotoEngineCommand::UndoSteps { steps: 2 });
        let info = worker.test_document().pixel_layer(id).expect("calque");
        assert!(info.filter_layers[0].enabled);
        assert!(info.masks[0].enabled);
        // Unknown: no-op.
        assert!(matches!(
            worker.apply(PhotoEngineCommand::ToggleFilter {
                layer: id,
                filter: uuid::Uuid::new_v4()
            }),
            PhotoEngineResponse::StateChanged { .. }
        ));
    }

    #[test]
    fn paint_mask_noircit_libelle_et_annule() {
        use photo_engine::LayerMask;
        let mut doc = Document::new(8, 8);
        let mut pixels = photo_engine::PixelLayer::new(
            "fond",
            std::sync::Arc::new(image::DynamicImage::new_rgba8(4, 4)),
        );
        let masque = LayerMask::full(4, 4);
        let mid = masque.id;
        pixels.masks.push(masque);
        let id = pixels.id;
        doc.push_layer(photo_engine::LayerNode::Pixel(pixels));
        let mut worker = EngineWorker::new(doc);
        // Eraser on white mask: digs (R channel).
        match worker.apply(PhotoEngineCommand::PaintMask {
            owner: id,
            mask: mid,
            points: vec![(1.0, 1.0), (2.0, 2.0)],
            eraser: true,
            radius: 1.5,
            opacity: 1.0,
        }) {
            PhotoEngineResponse::LayersChanged { undo_labels, .. } => {
                assert_eq!(undo_labels, vec!["Peindre le masque"]);
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        let valeur = worker
            .test_document()
            .pixel_layer(id)
            .expect("calque")
            .masks[0]
            .image
            .get_pixel(1, 1)[0];
        assert!(valeur < 128, "masque creusé : {valeur}");
        // Undo: white restored.
        worker.apply(PhotoEngineCommand::Undo);
        let valeur = worker
            .test_document()
            .pixel_layer(id)
            .expect("calque")
            .masks[0]
            .image
            .get_pixel(1, 1)[0];
        assert_eq!(valeur, 255);
        // Unknown mask: no-op.
        assert!(matches!(
            worker.apply(PhotoEngineCommand::PaintMask {
                owner: id,
                mask: uuid::Uuid::new_v4(),
                points: vec![(1.0, 1.0)],
                eraser: true,
                radius: 1.5,
                opacity: 1.0,
            }),
            PhotoEngineResponse::StateChanged { .. }
        ));
    }

    #[test]
    fn hidden_layer_keeps_thumb_and_resolves_once() {
        // Hidden layers do not contribute to the composite but
        // keep their thumbnail (parity with the old thumb() path).
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let hidden = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(hidden));
        let layers = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        let info = layers.iter().find(|l| l.id == hidden).expect("calque");
        assert!(!info.visible);
        assert!(info.thumb.is_some(), "miniature conservée même masqué");
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(
            m.appearance_resolves, 3,
            "tous les calques résolus une fois"
        );
        assert_eq!(m.preview_rebuilds, 0);
        assert_eq!(m.thumb_rebuilds, 0);
    }

    fn three_layer_doc_for_fold() -> Document {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        let mut doc = Document::new(8, 8);
        for name in ["fond", "milieu", "dessus"] {
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(name, image.clone())));
        }
        doc
    }

    // ------------------------------------------------------------------
    // Phase 6E — incremental pipeline actually used by the worker.
    // ------------------------------------------------------------------

    fn painted_doc() -> (Document, Uuid) {
        use photo_engine::{LayerNode, PixelLayer};
        // 64² document with contrasted content (hard edges): opaque background +
        // offset square.
        let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            64,
            64,
            image::Rgba([200, 40, 40, 255]),
        ));
        let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            24,
            24,
            image::Rgba([40, 200, 40, 255]),
        ));
        let mut doc = Document::new(64, 64);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
        let mut vert = PixelLayer::new("vert", Arc::new(carre));
        vert.transform.offset_x = 20.0;
        vert.transform.offset_y = 12.0;
        let id = vert.id;
        doc.push_layer(LayerNode::Pixel(vert));
        (doc, id)
    }

    #[test]
    fn incremental_preview_matches_legacy_clip_et_infini() {
        // Both modes: incremental (worker) == legacy (full frame).
        for clip in [true, false] {
            let (doc, _) = painted_doc();
            let mut worker = EngineWorker::new(doc);
            worker.apply(PhotoEngineCommand::SetPreviewClip { clip });
            // Legacy reference on the same document state.
            let legacy = render_preview(worker.test_document(), clip)
                .expect("legacy")
                .rgba;
            // Force the incremental re-render (the clip already rendered once).
            worker.render_worker.cache_mut().invalidate_all();
            let frame = AppearanceFrameCache::build(worker.test_document());
            let (preview, _, _) = worker.render_preview_incremental(&frame);
            assert_eq!(
                preview.expect("incrémental").rgba,
                legacy,
                "clip={clip} : mêmes octets"
            );
        }
    }

    fn painted_big_doc() -> (Document, Uuid) {
        use photo_engine::{LayerNode, PixelLayer};
        // 300² document (4 tiles at 256 px): identity background + square.
        let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            300,
            300,
            image::Rgba([200, 40, 40, 255]),
        ));
        let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            60,
            60,
            image::Rgba([40, 200, 40, 255]),
        ));
        let mut doc = Document::new(300, 300);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
        let mut vert = PixelLayer::new("vert", Arc::new(carre));
        vert.transform.offset_x = 200.0;
        vert.transform.offset_y = 200.0;
        doc.push_layer(LayerNode::Pixel(vert));
        let fond_id = doc.root[0].id();
        (doc, fond_id)
    }

    #[test]
    fn incremental_warm_reuse_apres_second_stroke() {
        // Second paint far away: clean tiles reused (hits > 0),
        // preview identical to full-frame legacy.
        let (doc, fond) = painted_big_doc();
        let mut worker = EngineWorker::new(doc);
        let stroke = |worker: &mut EngineWorker, points: Vec<(f32, f32)>| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points,
                eraser: false,
                radius: 3.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        stroke(&mut worker, vec![(10.0, 10.0), (20.0, 20.0)]);
        let first = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(first.cache_misses, 4, "premier rendu froid : 4 tuiles");
        stroke(&mut worker, vec![(270.0, 270.0), (280.0, 280.0)]);
        let second = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(second.cache_misses, 1, "seule la tuile sale est réévaluée");
        assert_eq!(second.cache_hits, 3, "tuiles propres réutilisées");
        let legacy = render_preview(worker.test_document(), false)
            .expect("legacy")
            .rgba;
        // Last incremental preview == legacy (via a forced re-read).
        worker.render_worker.cache_mut().invalidate_all();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        assert_eq!(preview.expect("incrémental").rgba, legacy);
    }

    #[test]
    fn incremental_scope_change_reste_correct() {
        // Layer moved out of the document: changed scope ⇒ total invalidation,
        // preview still equal to legacy.
        let (doc, id) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 500.0,
            dy: 0.0,
        });
        let legacy = render_preview(worker.test_document(), false)
            .expect("legacy")
            .rgba;
        worker.render_worker.cache_mut().invalidate_all();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        assert_eq!(preview.expect("incrémental").rgba, legacy);
    }

    #[test]
    fn surface_pixels_written_partiel_de_bout_en_bout() {
        // Phase 6G: first stroke ⇒ full surface (300²); second far away ⇒
        // only tile (256,256,44,44) is rewritten.
        let (doc, fond) = painted_big_doc();
        let mut worker = EngineWorker::new(doc);
        let stroke = |worker: &mut EngineWorker, points: Vec<(f32, f32)>| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points,
                eraser: false,
                radius: 3.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        stroke(&mut worker, vec![(10.0, 10.0), (20.0, 20.0)]);
        let first = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(first.surface_pixels_written, 300 * 300);
        stroke(&mut worker, vec![(270.0, 270.0), (280.0, 280.0)]);
        let second = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(second.surface_pixels_written, 44 * 44);
        assert_eq!(second.cache_hits, 3);
    }

    #[test]
    fn appearance_frame_rebuild_seulement_touche() {
        // §2.7 6G.2/6G.3: paint A → secondary thumbnail of A only (B hit),
        // frame without preview or synchronous thumb; SetOpacity A (source
        // unchanged) → 0 rebuilds. Pixels == legacy in all cases.
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(20.0, 20.0), (36.0, 20.0)],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let m = last_metrics(&worker);
        assert_eq!(m.thumb_rebuilds, 0, "cadre 6G.3 : miniature en tâche");
        assert_eq!(m.preview_rebuilds, 0, "cadre sans preview");
        assert_eq!(m.thumbnail_jobs_completed, 1, "seul A en tâche");
        assert_eq!(m.appearance_resolves, 2, "A + B images résolues une fois");
        let legacy = render_preview(worker.test_document(), false)
            .expect("legacy")
            .rgba;
        worker.render_worker.cache_mut().invalidate_all();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        assert_eq!(preview.expect("incrémental").rgba, legacy);
        // Opacity: the source does not change ⇒ hit, no rebuild.
        let carre = worker.test_document().root[1].id();
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: carre,
            opacity: 50.0,
        });
        let m = last_metrics(&worker);
        assert_eq!(m.thumb_rebuilds, 0, "aucune chaîne rejouée");
        assert_eq!(m.preview_rebuilds, 0);
    }

    #[test]
    fn marquage_paint_precis_pas_global() {
        // Painting marks a bounded region, not the whole document.
        let (doc, id) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        // `mutate` alone (without `respond`): the marking is observable before
        // the first render's scope detection absorbs it.
        let _ = worker.mutate(PhotoEngineCommand::PaintStroke {
            layer: id,
            points: vec![(8.0, 8.0)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let dirty = worker.render_worker.cache().dirty();
        assert!(!dirty.is_empty());
        let bounds = dirty.bounds();
        assert!(
            (bounds.area()) < (64 * 64),
            "marquage précis, pas global : {bounds:?}"
        );
        // Footprint: box [5,11]² + offset (20,12) = [25,31]×[17,23].
        assert!(dirty.intersects(TileRegion::new(20, 12, 16, 16)));
        assert!(!dirty.intersects(TileRegion::new(48, 48, 16, 16)));
    }

    #[test]
    fn marquage_structurel_global() {
        // Structural operation ⇒ safe global fallback.
        let (doc, _) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        let _ = worker.mutate(PhotoEngineCommand::AddEmptyLayer);
        let dirty = worker.render_worker.cache().dirty();
        assert!(!dirty.is_empty());
        assert!(dirty.intersects(TileRegion::new(0, 0, 64, 64)));
    }

    fn live_version(worker: &EngineWorker, id: Uuid) -> u64 {
        match worker.test_document().find(id) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => panic!("calque pixels attendu"),
        }
    }

    fn paint_fond(worker: &mut EngineWorker, id: Uuid, x: f32, y: f32) {
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: id,
            points: vec![(x, y)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
    }

    #[test]
    fn undo_rewind_ne_sert_jamais_un_thumb_perime() {
        // §6 review: undo restores the old (version, Arc). The versioned store
        // must never serve another content's thumbnail, and pending
        // must not get stuck (no stale intent surviving
        // and polluting later submissions).
        let (doc, _) = painted_doc();
        let fond = doc.root[0].id();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        paint_fond(&mut worker, fond, 8.0, 8.0);
        let version1 = live_version(&worker, fond);
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == version1),
            "store alimenté en v1 (inline)"
        );
        // Undo: the live state becomes the before-state again (same Arc).
        let layers = layers_of(&worker.apply(PhotoEngineCommand::Undo));
        let version0 = live_version(&worker, fond);
        assert_ne!(version0, version1, "version restaurée en arrière");
        // After undo, the served thumbnail (recomputed inline for v0, or
        // absent) ALWAYS carries the live version — never v1 content
        // stamped v0.
        let thumb_apres_undo = layers
            .iter()
            .find(|l| l.id == fond)
            .and_then(|l| l.thumb.clone());
        if let Some(thumb) = thumb_apres_undo {
            assert_eq!(
                thumb.version, version0,
                "miniature servie == version live (pas de v1 périmée)"
            );
        }
        // New paint: new global number (never reused),
        // normal submission, store up to date, pending empty.
        paint_fond(&mut worker, fond, 40.0, 40.0);
        let version2 = live_version(&worker, fond);
        assert_ne!(version2, version1);
        assert_ne!(version2, version0);
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == version2),
            "store à jour en v2"
        );
        assert!(
            worker.thumb_pending.is_empty(),
            "aucune intention coincée (inline)"
        );
        let layers = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        assert!(
            layers
                .iter()
                .find(|l| l.id == fond)
                .and_then(|l| l.thumb.clone())
                .is_some_and(|t| t.version == version2),
            "miniature v2 servie"
        );
    }

    // ------------------------------------------------------------------
    // Phase 6G.3 — canvas first, thumbnails next (deterministic).
    // ------------------------------------------------------------------

    #[test]
    fn submit_decision_branches() {
        // Pure: store up to date ⇒ nothing; identical intent ⇒ coalesced;
        // otherwise ⇒ submit.
        assert_eq!(
            EngineWorker::submit_decision(true, None, 7),
            SubmitDecision::Fresh
        );
        assert_eq!(
            EngineWorker::submit_decision(false, None, 7),
            SubmitDecision::Submit
        );
        assert_eq!(
            EngineWorker::submit_decision(false, Some(7), 7),
            SubmitDecision::Coalesce
        );
        assert_eq!(
            EngineWorker::submit_decision(false, Some(6), 7),
            SubmitDecision::Submit
        );
        // Store up to date but residual intent: freshness wins.
        assert_eq!(
            EngineWorker::submit_decision(true, Some(7), 7),
            SubmitDecision::Coalesce
        );
    }

    #[test]
    fn ingest_rejette_perime_ordre_inverse() {
        // §16: B computed then A (older) → A rejected, B kept.
        // No thread: synthesized results, guard alone.
        use crate::ui::thumb_worker::ThumbDone;
        let (doc, fond) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        // Live version after a paint.
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(8.0, 8.0)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let live_version = match worker.test_document().find(fond) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => panic!("calque"),
        };
        // (Inline mode already stored the fresh thumbnail.)
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == live_version)
        );
        // Fake "old" result (version - 1, arbitrary content): rejected.
        let stale = ThumbDone {
            layer: fond,
            version: live_version.wrapping_sub(1),
            thumb: photo_engine::RgbaBuf::from_vec(48, 32, vec![0u8; 48 * 32 * 4]),
            image: std::sync::Arc::new(image::DynamicImage::new_rgba8(1, 1)),
            render_us: 0,
            thumb_us: 0,
            from_cache: false,
        };
        assert!(!worker.ingest_thumb_done(stale), "périmé rejeté");
        // The fresh one is intact.
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == live_version)
        );
    }

    #[test]
    fn canvas_first_puis_thumbnail_async() {
        // §18: in threaded mode, the canvas response leaves WITHOUT waiting for
        // the thumbnail (observable pending); the result eventually arrives and
        // matches the direct computation byte-for-byte.
        let (doc, fond) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        worker.enable_thumb_thread();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(8.0, 8.0)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let live_version = match worker.test_document().find(fond) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => panic!("calque"),
        };
        // The response already came back (canvas) with the intent pending —
        // deterministic: no thread wait.
        assert!(
            worker
                .thumb_pending
                .get(&fond)
                .is_some_and(|(v, _)| *v == live_version),
            "intention en attente pendant que le canvas est servi"
        );
        // Convergence: the result eventually arrives (wide timeout =
        // robustness, not a race: failure would mean a dead thread).
        let start = std::time::Instant::now();
        loop {
            worker.poll_thumb_results();
            if worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == live_version)
            {
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(10),
                "la tâche secondaire doit aboutir"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Bytes == direct computation (same function, same inputs).
        let stored = worker.thumb_store.get(&fond).expect("stockée").0.clone();
        let layer = match worker.test_document().find(fond) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.clone(),
            _ => panic!("calque"),
        };
        let mut renderer = photo_engine::renderer::Renderer::default();
        let (_, expected) = renderer.appearance_frame(&layer).expect("cadre");
        assert_eq!(
            (stored.width, stored.height, stored.data.as_ref()),
            (expected.width, expected.height, expected.data.as_ref()),
            "miniature async == cadre direct"
        );
    }

    #[test]
    fn drain_extras_sauf_si_deja_embarquee() {
        // §4A: a NON-embedded freshness goes out as an extra; already embedded
        // ⇒ nothing (no duplicate). Deterministic, no thread (real responses).
        let (doc, fond) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        let layers = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        assert!(
            layers.iter().any(|l| l.id == fond && l.thumb.is_some()),
            "inline : snapshot embarque les miniatures"
        );
        // Inline already marked both layers as fresh: the drain
        // without a main response emits both (async case).
        let extras = worker.drain_thumb_extras(&[]);
        assert_eq!(extras.len(), 2, "miniatures secondaires après canvas");
        assert!(
            extras
                .iter()
                .all(|r| matches!(r, PhotoEngineResponse::ThumbnailUpdated { .. })),
            "que des extras"
        );
        // Same freshnesses, main response embedding them: nothing.
        worker.thumb_fresh.push(fond);
        assert!(
            worker.drain_thumb_extras(&layers).is_empty(),
            "pas de doublon avec le snapshot"
        );
    }

    // ------------------------------------------------------------------
    // Phase 6G.1 — end-to-end profiling (diagnostics, no optimization).
    //
    // Fast tests (512²) lock VOLUMES (deterministic);
    // timings are reported by `#[ignore]` benches in release.
    // Rule: never assert on a duration (variable hardware), only
    // on events and work volumes.
    // ------------------------------------------------------------------

    fn bench_doc(size: u32) -> (Document, Uuid) {
        use photo_engine::{LayerNode, PixelLayer};
        let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            size,
            size,
            image::Rgba([200, 40, 40, 255]),
        ));
        let klein = size / 4;
        let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            klein,
            klein,
            image::Rgba([40, 200, 40, 255]),
        ));
        let mut doc = Document::new(size, size);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
        let mut vert = PixelLayer::new("carre", Arc::new(carre));
        vert.transform.offset_x = (size / 3) as f32;
        vert.transform.offset_y = (size / 4) as f32;
        let id = vert.id;
        doc.push_layer(LayerNode::Pixel(vert));
        let fond_id = doc.root[0].id();
        let _ = id;
        (doc, fond_id)
    }

    fn last_metrics(worker: &EngineWorker) -> OpMetrics {
        worker.metrics.last.clone().expect("metriques")
    }

    #[test]
    fn profile_volumes_paint_512() {
        // Deterministic volumes of a small paint on 512².
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let paint = |worker: &mut EngineWorker, x: f32, y: f32| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points: vec![(x, y), (x + 16.0, y + 16.0)],
                eraser: false,
                radius: 5.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        paint(&mut worker, 20.0, 20.0);
        let cold = last_metrics(&worker);
        assert_eq!(cold.cache_misses, 4, "froid : 4 tuiles 256²");
        assert_eq!(cold.cache_hits, 0);
        assert_eq!(cold.surface_pixels_written, 512 * 512);
        paint(&mut worker, 400.0, 400.0);
        let warm = last_metrics(&worker);
        assert_eq!(warm.cache_misses, 1, "seule la tuile sale");
        assert_eq!(warm.cache_hits, 3, "le reste réutilisé");
        assert_eq!(warm.surface_pixels_written, 256 * 256);
    }

    #[test]
    fn profile_volumes_toggle_512() {
        // ON/OFF toggle: symmetric volumes, no surprises.
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
        let off = last_metrics(&worker);
        assert_eq!((off.cache_misses, off.cache_hits), (4, 0), "froid complet");
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
        let on = last_metrics(&worker);
        // 128² footprint at (170,128): straddling x=256 ⇒ 2 tiles.
        assert_eq!((on.cache_misses, on.cache_hits), (2, 2));
        assert_eq!(on.surface_pixels_written, 2 * 256 * 256);
        // §2.5/§13: neither source nor filters touched ⇒ no synchronous rebuild.
        assert_eq!(on.thumb_rebuilds, 0);
        assert_eq!(on.preview_rebuilds, 0);
        // §2.5: visibility touches neither source nor filters ⇒ chains
        // do not replay (state-only compositing).
        assert_eq!(on.thumb_rebuilds, 0, "aucune apparence recalculée");
        assert_eq!(on.preview_rebuilds, 0);
    }

    #[test]
    fn profile_volumes_opacity_512() {
        // Two settings: each pass recomputes the footprint (history-coalesced).
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        for opacity in [30.0, 70.0] {
            worker.apply(PhotoEngineCommand::SetOpacity { layer: id, opacity });
        }
        // Second setting (coalesced): only the footprint is re-evaluated
        // (128² straddling x=256 ⇒ 2 tiles).
        let m = last_metrics(&worker);
        assert_eq!((m.cache_misses, m.cache_hits), (2, 2));
        assert_eq!(m.surface_pixels_written, 2 * 256 * 256);
    }

    #[test]
    fn profile_legacy_vs_incremental_512() {
        // Same document: legacy (full frame) == incremental (worker), bytes
        // AND dimensions; wall timings recorded (report only).
        let (doc, _) = bench_doc(512);
        let t_legacy = std::time::Instant::now();
        let legacy = render_preview(&doc, false).expect("legacy");
        let legacy_us = t_legacy.elapsed().as_micros();
        let mut worker = EngineWorker::new(doc);
        let t_incr = std::time::Instant::now();
        let response = worker.apply(PhotoEngineCommand::Refresh);
        let incr_us = t_incr.elapsed().as_micros();
        let m = last_metrics(&worker);
        let bytes = match response {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("aperçu").rgba,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(bytes, legacy.rgba, "mêmes octets");
        assert_eq!(m.cache_misses, 4, "froid complet");
        eprintln!("BENCH legacy_vs_incremental_512 legacy_us={legacy_us} incr_us={incr_us}");
    }

    #[test]
    fn profile_warm_ecrit_rien() {
        // §26: hot frame (same request, presented dirty hence clear) ⇒
        // 0 renders + 0 surface writes. `clear_dirty` simulates the 6D
        // contract's presentation step (no production call does it
        // today — see diagnostic_dirty_accumulates).
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        worker.render_worker.cache_mut().clear_dirty();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        preview.expect("froid");
        let written = worker.render_worker.surface_stats().pixels_written;
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, worker_stats) = worker.render_preview_incremental(&frame);
        preview.expect("chaud");
        assert_eq!(
            (worker_stats.rendered_tiles, worker_stats.cache_hits),
            (0, 4),
            "chaud : aucun rendu"
        );
        assert_eq!(
            worker.render_worker.surface_stats().pixels_written,
            written,
            "chaud : aucune écriture"
        );
    }

    #[test]
    fn dirty_consumed_after_render_stable() {
        // §1.3 6G.2 (ex-6G.1 finding, fixed): paint A → dirty=A → render A
        // → dirty settled; paint B (same tile) → dirty=A → render A only.
        // No more monotonic drift: dirty is consumed after production.
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let stroke = |worker: &mut EngineWorker, x: f32, y: f32| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points: vec![(x, y), (x + 16.0, y + 16.0)],
                eraser: false,
                radius: 5.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        stroke(&mut worker, 20.0, 20.0);
        let first = last_metrics(&worker);
        assert_eq!((first.cache_misses, first.cache_hits), (4, 0), "froid");
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "dirty soldé après rendu"
        );
        // First render: scope init drains the cache AND the dirty
        // (invalidate_all) — nothing to settle per tile.
        assert_eq!(first.dirty_tiles_cleared, 0);
        stroke(&mut worker, 24.0, 24.0);
        let second = last_metrics(&worker);
        assert_eq!(
            (second.cache_misses, second.cache_hits),
            (1, 3),
            "même tuile : 1 seul rendu"
        );
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "toujours soldé"
        );
        assert_eq!(second.dirty_tiles_cleared, 1, "la zone marquée est soldée");
        stroke(&mut worker, 400.0, 400.0);
        let third = last_metrics(&worker);
        assert_eq!((third.cache_misses, third.cache_hits), (1, 3));
        assert_eq!(third.dirty_tiles_cleared, 1);
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "jamais d'accumulation"
        );
    }

    #[test]
    fn long_session_no_drift_512() {
        // §1.4 + §10 (fast version, 50 mutations): same zone, 1 render per
        // mutation, empty dirty at the end. The 2048²×100 version is an ignored
        // release bench (bench_long_session_2048).
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        // Cold start (scope init: everything rendered once).
        worker.apply(PhotoEngineCommand::Refresh);
        let mut total_renders = 0u64;
        for i in 0..50 {
            if i % 10 == 9 {
                // Varied but local operations: toggle + re-toggle
                // (2-tile footprint straddling x=256, both ways).
                worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
                assert_eq!(last_metrics(&worker).cache_misses, 2);
                worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
                let m = last_metrics(&worker);
                assert_eq!(m.cache_misses, 2);
                total_renders += 4;
            } else {
                worker.apply(PhotoEngineCommand::PaintStroke {
                    layer: fond,
                    points: vec![(20.0, 20.0), (36.0, 20.0)],
                    eraser: false,
                    radius: 5.0,
                    color: [(i % 256) as u8, 0, 255],
                    opacity: 1.0,
                });
                let m = last_metrics(&worker);
                assert_eq!(m.cache_misses, 1, "itération {i} : 1 seul rendu");
                total_renders += m.cache_misses;
            }
        }
        assert_eq!(total_renders, 45 + 5 * 4, "stable : 45×1 + 5×(2+2)");
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "aucune dérive après 50+ mutations"
        );
    }

    #[test]
    #[ignore]
    fn bench_long_session_2048() {
        // §10 6G.2: 100 mixed mutations on 2048² — dirty settled each
        // frame (no drift), targeted appearances, targeted previews.
        // Release recommended: ~10-20 s.
        let (doc, fond) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        let id = worker.test_document().root[1].id();
        let mut samples = Vec::with_capacity(100);
        let mut total_renders = 0u64;
        let mut total_cleared = 0u64;
        for i in 0..100 {
            let t = std::time::Instant::now();
            match i % 10 {
                9 => {
                    worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
                }
                8 => {
                    worker.apply(PhotoEngineCommand::SetOpacity {
                        layer: id,
                        opacity: 20.0 + (i % 60) as f32,
                    });
                }
                _ => {
                    bench_paint_apply(&mut worker, fond, 40.0, 40.0);
                }
            }
            samples.push(t.elapsed().as_micros());
            let m = last_metrics(&worker);
            total_renders += m.cache_misses;
            total_cleared += m.dirty_tiles_cleared;
            assert!(
                m.cache_misses <= 8,
                "itération {i} : jamais de dérive vers le full (64)"
            );
        }
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "dirty vide après 100 mutations"
        );
        let m = last_metrics(&worker);
        let mut sorted = samples.clone();
        let (min, median, p95) = stats_us(&mut sorted);
        eprintln!(
            "BENCH op=long_session size=2048 n=100 min={min} median={median} p95={p95} \
             total_renders={total_renders} total_cleared={total_cleared}"
        );
        bench_row(
            "long_session",
            2048,
            "mixed",
            &samples,
            &m,
            &worker.metrics.thumb,
        );
    }

    /// Min/median/p95 over microseconds (release benchmarks).
    fn stats_us(samples: &mut [u128]) -> (u128, u128, u128) {
        samples.sort_unstable();
        let min = samples[0];
        let median = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        (min, median, p95)
    }

    /// Benchmark result line (read with `--nocapture` in release).
    fn bench_row(
        op: &str,
        size: u32,
        label: &str,
        samples: &[u128],
        m: &OpMetrics,
        thumb: &crate::ui::thumb_worker::ThumbStats,
    ) {
        let mut sorted = samples.to_vec();
        let (min, median, p95) = stats_us(&mut sorted);
        eprintln!(
            "BENCH op={op} size={size} {label} n={} min_us={min} median_us={median} p95_us={p95} \
             mutation_us={} snapshot_us={} frame_us={} thumb_us={} geometry_us={} composite_us={} \
             tile_render_total_us={} surface_us={} assembly_us={} \
             blend_us={} layers_blended={} pixels_processed={} \
             appearance_resolves={} appearance_frame_hits={} preview_rebuilds={} thumb_rebuilds={} \
             hits={} misses={} avoided={} surface_px={} scope_px={} full_px={} preview_px={} \
             resize_px={} cleared={} thumb_done={} canvas_us={} secondary_us={} \
             t_submitted={} t_completed={} t_discarded={} t_coalesced={} t_us={}",
            samples.len(),
            m.mutation_us,
            m.snapshot_us,
            m.frame_us,
            m.thumb_us,
            m.geometry_us,
            m.composite_us,
            m.tile_render_total_us,
            m.surface_update_us,
            m.assembly_us,
            m.blend_us,
            m.layers_blended,
            m.pixels_processed,
            m.appearance_resolves,
            m.appearance_frame_hits,
            m.preview_rebuilds,
            m.thumb_rebuilds,
            m.cache_hits,
            m.cache_misses,
            m.renders_avoided,
            m.surface_pixels_written,
            m.scope_px,
            m.full_px,
            m.preview_px,
            m.preview_resize_pixels,
            m.dirty_tiles_cleared,
            m.thumbnail_jobs_completed,
            m.canvas_critical_us,
            m.secondary_work_us,
            thumb.submitted,
            thumb.completed,
            thumb.discarded,
            thumb.coalesced,
            thumb.thumb_us,
        );
    }

    fn bench_paint_apply(worker: &mut EngineWorker, fond: Uuid, x: f32, y: f32) -> u128 {
        let t = std::time::Instant::now();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(x, y), (x + 16.0, y + 16.0)],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        t.elapsed().as_micros()
    }

    #[test]
    #[ignore]
    fn bench_512_all() {
        // 512² cold+hot BENCH lines: paint, toggle, opacity (5 reps).
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        // Cold paint then hot.
        let cold = vec![bench_paint_apply(&mut worker, fond, 20.0, 20.0)];
        let m_cold = last_metrics(&worker);
        bench_row("paint", 512, "cold", &cold, &m_cold, &worker.metrics.thumb);
        let mut warm = Vec::new();
        for _ in 0..5 {
            warm.push(bench_paint_apply(&mut worker, fond, 400.0, 400.0));
        }
        let m_warm = last_metrics(&worker);
        bench_row("paint", 512, "warm", &warm, &m_warm, &worker.metrics.thumb);
        // Cold toggle (fresh tile): fresh worker for a true cold start.
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let t = std::time::Instant::now();
            worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
            samples.push(t.elapsed().as_micros());
        }
        bench_row(
            "toggle",
            512,
            "on_off",
            &samples,
            &last_metrics(&worker),
            &worker.metrics.thumb,
        );
        // Opacity ×2.
        let mut samples = Vec::new();
        for opacity in [30.0, 70.0, 30.0, 70.0, 30.0] {
            let t = std::time::Instant::now();
            worker.apply(PhotoEngineCommand::SetOpacity { layer: id, opacity });
            samples.push(t.elapsed().as_micros());
        }
        bench_row(
            "opacity",
            512,
            "slider",
            &samples,
            &last_metrics(&worker),
            &worker.metrics.thumb,
        );
    }

    #[test]
    #[ignore]
    fn bench_paint_2048() {
        // Recommended: `cargo test --release -p photo bench_paint_2048 -- --ignored --nocapture`.
        // Sub-64×64 stroke on 2048², cold then hot (7 reps).
        let (doc, fond) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        let mut cold = Vec::new();
        for i in 0..3 {
            cold.push(bench_paint_apply(
                &mut worker,
                fond,
                20.0 + i as f32 * 300.0,
                20.0,
            ));
        }
        let m_cold = last_metrics(&worker);
        let mut warm = Vec::new();
        for _ in 0..7 {
            // Same zone repainted: tile already hot after the 1st.
            warm.push(bench_paint_apply(&mut worker, fond, 20.0, 20.0));
        }
        let m_warm = last_metrics(&worker);
        bench_row("paint", 2048, "cold", &cold, &m_cold, &worker.metrics.thumb);
        bench_row("paint", 2048, "warm", &warm, &m_warm, &worker.metrics.thumb);
    }

    #[test]
    #[ignore]
    fn bench_paint_4096() {
        // Sub-64×64 stroke on 4096², cold then hot (5 reps).
        let (doc, fond) = bench_doc(4096);
        let mut worker = EngineWorker::new(doc);
        let mut cold = Vec::new();
        for i in 0..2 {
            cold.push(bench_paint_apply(
                &mut worker,
                fond,
                20.0 + i as f32 * 900.0,
                20.0,
            ));
        }
        let m_cold = last_metrics(&worker);
        let mut warm = Vec::new();
        for _ in 0..5 {
            warm.push(bench_paint_apply(&mut worker, fond, 20.0, 20.0));
        }
        let m_warm = last_metrics(&worker);
        bench_row("paint", 4096, "cold", &cold, &m_cold, &worker.metrics.thumb);
        bench_row("paint", 4096, "warm", &warm, &m_warm, &worker.metrics.thumb);
    }

    #[test]
    #[ignore]
    fn bench_toggle_2048() {
        let (doc, _) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let t = std::time::Instant::now();
            worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
            samples.push(t.elapsed().as_micros());
        }
        let m = last_metrics(&worker);
        bench_row(
            "toggle",
            2048,
            "on_off",
            &samples,
            &m,
            &worker.metrics.thumb,
        );
    }

    #[test]
    #[ignore]
    fn bench_open_2048() {
        // Real PNG file (generated): decoding included, then composite.
        let dir = std::env::temp_dir().join("cygnus-bench-open");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("bench-2048.png");
        if !path.exists() {
            let img = image::RgbaImage::from_pixel(2048, 2048, image::Rgba([120, 130, 140, 255]));
            img.save(&path).expect("png bench");
        }
        let t_decode = std::time::Instant::now();
        let decoded = image::open(&path).expect("décodage bench");
        let decode_us = t_decode.elapsed().as_micros();
        let (w, h) = (decoded.width(), decoded.height());
        let mut worker = EngineWorker::new(Document::new(8, 8));
        let t = std::time::Instant::now();
        worker.apply(PhotoEngineCommand::OpenImage { path });
        let total_us = t.elapsed().as_micros();
        let m = last_metrics(&worker);
        eprintln!("BENCH op=open size=2048 decode_us={decode_us} dims={w}x{h} total_us={total_us}");
        bench_row(
            "open",
            2048,
            "apply",
            &[total_us],
            &m,
            &worker.metrics.thumb,
        );
    }

    #[test]
    #[ignore]
    fn bench_presentation_ui_512() {
        // Release recommended: worker paint → response → apply_response with
        // headless Context. Measures ColorImage + texture.set + thumbs + total.
        use crate::state::{PhotoUiState, apply_response};
        let ctx = egui::Context::default();
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(20.0, 20.0), (36.0, 20.0)],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        // Replays the same response N times on fresh states (uploads + bytes).
        let mut samples = Vec::new();
        for _ in 0..7 {
            let response = worker.apply(PhotoEngineCommand::Refresh);
            let preview_bytes = match &response {
                PhotoEngineResponse::LayersChanged { preview, .. } => preview
                    .as_ref()
                    .map(|p| p.width as u64 * p.height as u64 * 4),
                _ => None,
            };
            let mut ui = PhotoUiState::default();
            let t = std::time::Instant::now();
            apply_response(&ctx, &mut ui, response);
            let total_us = t.elapsed().as_micros();
            let p = ui.last_presentation;
            samples.push(total_us);
            eprintln!(
                "BENCHUI apply_us={total_us} thumb_us={} colorimage_us={} texture_us={} \
                 preview_us={} uploads={} upload_bytes={} preview_bytes={:?}",
                p.thumb_sync_us,
                p.colorimage_us,
                p.texture_set_us,
                p.preview_apply_us,
                ui.texture_uploads,
                ui.texture_upload_bytes,
                preview_bytes,
            );
        }
        let mut sorted = samples.clone();
        let (min, median, p95) = stats_us(&mut sorted);
        eprintln!(
            "BENCHUI op=apply_response size=512 n={} min={min} median={median} p95={p95}",
            samples.len()
        );
    }

    #[test]
    #[ignore]
    fn bench_presentation_ui_2048() {
        // 2048² UI: 3 applies (partial cold), millis per step.
        use crate::state::{PhotoUiState, apply_response};
        let ctx = egui::Context::default();
        let (doc, fond) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        for i in 0..3 {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points: vec![
                    (20.0 + i as f32 * 300.0, 20.0),
                    (36.0 + i as f32 * 300.0, 20.0),
                ],
                eraser: false,
                radius: 5.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
            let response = worker.apply(PhotoEngineCommand::Refresh);
            let mut ui = PhotoUiState::default();
            let t = std::time::Instant::now();
            apply_response(&ctx, &mut ui, response);
            let total_us = t.elapsed().as_micros();
            let p = ui.last_presentation;
            eprintln!(
                "BENCHUI op=apply size=2048 apply_us={total_us} thumb_us={} \
                 colorimage_us={} texture_us={} preview_us={} uploads={} upload_bytes={}",
                p.thumb_sync_us,
                p.colorimage_us,
                p.texture_set_us,
                p.preview_apply_us,
                ui.texture_uploads,
                ui.texture_upload_bytes,
            );
        }
    }

    #[test]
    #[ignore]
    fn bench_legacy_vs_incremental_2048() {
        // Full-frame legacy vs incremental worker, same 2048² document.
        let (doc, _) = bench_doc(2048);
        let t_legacy = std::time::Instant::now();
        let legacy = render_preview(&doc, false).expect("legacy");
        let legacy_us = t_legacy.elapsed().as_micros();
        let mut worker = EngineWorker::new(doc);
        let t_incr = std::time::Instant::now();
        let response = worker.apply(PhotoEngineCommand::Refresh);
        let incr_us = t_incr.elapsed().as_micros();
        let bytes = match response {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("aperçu").rgba,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(bytes, legacy.rgba, "mêmes octets");
        let m = last_metrics(&worker);
        eprintln!(
            "BENCH op=legacy_vs_incremental size=2048 legacy_us={legacy_us} incr_us={incr_us}"
        );
        bench_row(
            "incremental",
            2048,
            "cold",
            &[incr_us],
            &m,
            &worker.metrics.thumb,
        );
    }
}
