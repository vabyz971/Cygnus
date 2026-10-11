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

//! Async thumbnails of [`EngineWorker`](super::worker::EngineWorker):
//! `impl EngineWorker` methods (store, submission, ingestion, drain).

use super::commands::{
    DirtyMark, Mutation, PhotoEngineCommand, RenderInvalidation, SubmitDecision,
};
use super::metrics::{OpMetrics, PreviewTimings, WorkerMetrics};
use super::preview::AppearanceFrameCache;
use super::responses::PhotoEngineResponse;
use super::worker::EngineWorker;
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

impl EngineWorker {
    /// Enables the threaded secondary task (production; the worker lives on
    /// its own thread anyway). Without a call: deterministic inline
    /// mode (tests, ephemeral `apply_command` — no leaked thread).
    pub fn enable_thumb_thread(&mut self) {
        if !self.thumb_worker.is_threaded() {
            self.thumb_worker = crate::ui::thumb_worker::ThumbWorker::spawned();
        }
    }

    /// Known thumbnail of a layer IF it matches the live version
    /// (else `None`: icon while waiting for the secondary task). Sole
    /// source of snapshot thumbnails — never the renderer directly.
    pub(crate) fn stored_thumb(&self, id: Uuid) -> Option<photo_engine::RgbaBuf> {
        let live = match self.document.find(id) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => return None,
        };
        let (thumb, version) = self.thumb_store.get(&id)?;
        (*version == live).then(|| thumb.clone())
    }

    /// Evicts gone layers from the store (structural, undo/redo…).
    pub(crate) fn prune_thumb_store(&mut self) {
        let live: std::collections::HashSet<Uuid> =
            self.document.all_pixel_ids().into_iter().collect();
        self.thumb_store.retain(|id, _| live.contains(id));
        self.thumb_pending.retain(|id, _| live.contains(id));
    }

    /// Pure send decision (testable): `Fresh` (nothing to do),
    /// `Coalesce` (identical intent already pending — absorbed),
    /// `Submit` (new intent to forward).
    pub(crate) fn submit_decision(known: bool, pending: Option<u64>, live: u64) -> SubmitDecision {
        if pending == Some(live) {
            SubmitDecision::Coalesce
        } else if known {
            SubmitDecision::Fresh
        } else {
            SubmitDecision::Submit
        }
    }

    /// Current collapsed state of a group (false when `id` is not a
    /// group): the worker toggle flips this value.
    pub(crate) fn is_collapsed(&self, id: Uuid) -> bool {
        matches!(
            self.document.find(id),
            Some(photo_engine::LayerNode::Group(groupe)) if groupe.collapsed
        )
    }

    /// Submits stale thumbnails (live version unknown to the store AND not
    /// already pending at that version). Inline mode: immediate compute +
    /// ingestion (deterministic). Threaded mode: non-blocking send
    /// (inline fallback when the thread died).
    /// Returns the submitted versions (diagnostics).
    pub(crate) fn submit_stale_thumbs(&mut self) -> u64 {
        use crate::ui::thumb_worker::ThumbJob;
        self.prune_thumb_store();
        let mut submitted = 0u64;
        let mut inline_done = Vec::new();
        for id in self.document.all_pixel_ids() {
            let Some(photo_engine::LayerNode::Pixel(layer)) = self.document.find(id) else {
                continue;
            };
            let live_version = layer.appearance_version;
            let known = self
                .thumb_store
                .get(&id)
                .is_some_and(|(_, v)| *v == live_version);
            let pending = self.thumb_pending.get(&id).map(|(v, _)| *v);
            match Self::submit_decision(known, pending, live_version) {
                SubmitDecision::Fresh => continue,
                // Redundant intent absorbed (sender-side coalescing).
                SubmitDecision::Coalesce => {
                    self.metrics.thumb.coalesced += 1;
                    continue;
                }
                SubmitDecision::Submit => {}
            }
            let job = ThumbJob {
                layer: id,
                version: live_version,
                source: Arc::clone(&layer.source_image),
                filters: layer.filter_layers.clone(),
                masks: layer.masks.clone(),
                key: photo_engine::ThumbnailKey::for_layer(layer),
            };
            self.metrics.thumb.submitted += 1;
            submitted += 1;
            self.thumb_pending
                .insert(id, (live_version, Arc::clone(&layer.source_image)));
            if self.thumb_worker.is_threaded() {
                if !self.thumb_worker.submit(job.clone()) {
                    // Dead thread: safe inline fallback (identical pixels,
                    // shared 6G.4 cache of the inline instance).
                    if let Some(done) = self.thumb_worker.compute_cached(&job) {
                        inline_done.push(done);
                    }
                }
            } else if let Some(done) = self.thumb_worker.compute_cached(&job) {
                inline_done.push(done);
            }
        }
        for done in inline_done {
            self.ingest_thumb_done(done);
        }
        submitted
    }

    /// Ingests a result (guard §6-§7): applicable iff version AND source
    /// still match the live state — the monotonic global counter never
    /// reuses a number for different content (snapshots
    /// preserving the `Arc`s), and `ptr_eq` locks the allocation.
    /// Returns true when stored (fresh). Used by the threaded poll AND
    /// inline mode (same code).
    pub(crate) fn ingest_thumb_done(&mut self, done: crate::ui::thumb_worker::ThumbDone) -> bool {
        // 6G.4 cache accounting (HIT/MISS measured here: covers inline AND
        // threaded, fresh or stale — a stale HIT is still an avoided
        // computation, rejected right after by the guard).
        if done.from_cache {
            self.metrics.thumb.cache_hits += 1;
        } else {
            self.metrics.thumb.cache_misses += 1;
        }
        let pending_source = self
            .thumb_pending
            .get(&done.layer)
            .map(|(_, src)| Arc::clone(src));
        if let Some((v, _)) = self.thumb_pending.get(&done.layer)
            && *v == done.version
        {
            self.thumb_pending.remove(&done.layer);
        }
        // Double factor: version (monotonic global counter, never reused
        // for different content — snapshots preserving the `Arc`s) AND
        // source allocation identity (kept at send time; version-only
        // fallback for an untracked external result, e.g. tests).
        let fresh = match self.document.find(done.layer) {
            Some(photo_engine::LayerNode::Pixel(layer)) => {
                let job_source = pending_source.as_ref().unwrap_or(&layer.source_image);
                crate::ui::thumb_worker::is_fresh(
                    &done,
                    layer.appearance_version,
                    &layer.source_image,
                    job_source,
                )
            }
            _ => false,
        };
        if fresh {
            self.metrics.thumb.completed += 1;
            self.metrics.thumb.thumb_us += done.render_us + done.thumb_us;
            self.thumb_store
                .insert(done.layer, (done.thumb, done.version));
            if !self.thumb_fresh.contains(&done.layer) {
                self.thumb_fresh.push(done.layer);
            }
            true
        } else {
            self.metrics.thumb.discarded += 1;
            false
        }
    }

    /// Drains available threaded results (never blocking).
    /// Returns the number of freshly ingested ones.
    pub(crate) fn poll_thumb_results(&mut self) -> u64 {
        let mut fresh = 0u64;
        // Disjoint borrow: drain first (borrow ends), ingestion next.
        let done: Vec<_> = self.thumb_worker.drain_results();
        for d in done {
            if self.ingest_thumb_done(d) {
                fresh += 1;
            }
        }
        fresh
    }

    /// `ThumbnailUpdated` extras for freshly stored thumbnails
    /// since the last drain (batch mode only — solo `apply` keeps
    /// the 1:1 contract). Skips layers whose main response already
    /// embedded the version (inline case: synchronous snapshot — no
    /// duplicates). Each extra carries the computed version; the UI ignores
    /// anything not newer than its cache (idempotent).
    pub(crate) fn drain_thumb_extras(
        &mut self,
        layers: &[PhotoLayerInfo],
    ) -> Vec<PhotoEngineResponse> {
        let mut out = Vec::new();
        for layer in std::mem::take(&mut self.thumb_fresh) {
            let embedded = layers
                .iter()
                .find(|l| l.id == layer)
                .and_then(|l| l.thumb.as_ref().map(|t| t.version));
            if let Some((thumb, version)) = self.thumb_store.get(&layer) {
                if embedded == Some(*version) {
                    continue;
                }
                // Check the live state still matches (else: gone stale
                // meanwhile — the next job will recompute it).
                let current = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(l)) => Some(l.appearance_version),
                    _ => None,
                };
                if current == Some(*version) {
                    out.push(PhotoEngineResponse::ThumbnailUpdated {
                        layer,
                        thumb: PhotoLayerThumb {
                            width: thumb.width,
                            height: thumb.height,
                            rgba: thumb.data.to_vec(),
                            version: *version,
                        },
                    });
                }
            }
        }
        out
    }
}
