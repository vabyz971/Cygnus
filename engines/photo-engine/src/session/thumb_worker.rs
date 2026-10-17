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

//! Secondary thumbnail task (Phase 6G.3): canvas first.
//!
//! The canvas-critical path (`PreviewSurface` → `PreviewImage`) must
//! never wait for layer thumbnail generation (`thumb_buf` ≈
//! 16 ms at 2048², measured 6G.3.1). This module isolates that secondary work:
//!
//! STUDY NOTE §9-§10: no incremental thumbnails. A partial resample
//! would require replaying the appearance chain's halo/neighborhood
//! accounting per thumbnail tile — a correctness risk for an uncertain
//! gain, while the remaining cost (full resample) is ALREADY off the
//! critical path. Revisit only on future measurements demanding it.
//!
//! ```text
//! EngineWorker (main thread)
//!   │ submit ThumbJob (light: cloned Arc + params, never a Document)
//!   ▼
//! ThumbWorker — inline (tests, deterministic) or dedicated thread (production)
//!   │ compute: fresh Renderer + appearance_frame (SAME function as the
//!   │   frame path → bit-identical bytes by construction)
//!   ▼
//! ThumbDone ──► versioned ingestion (stale rejected, never displayed)
//! ```
//!
//! Rules (spec 6G.3 §5-§8):
//!
//! - the main thread stays the SOLE owner of the `Document` (no
//!   mutable sharing): only immutable `Arc`s and params transit;
//! - structural coalescing: per-layer intent queue `BTreeMap` (newest
//!   wins) + drain-replace thread-side (superseded intermediate
//!   states are never computed);
//! - NO stale result displayed: `(version, source Arc)` guard;
//! - NO new appearance cache: the app store reuses the existing
//!   validity (`appearance_version`), computation reuses
//!   `Renderer::appearance_frame`.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use image::DynamicImage;
use uuid::Uuid;

use crate::{FilterLayer, LayerMask, RgbaBuf, ThumbnailKey};

/// Thumbnail request: everything needed to derive image + thumbnail,
/// cloned from the live layer (light: `Arc` + params, never copied
/// pixels, never a `Document`).
#[derive(Debug, Clone)]
pub struct ThumbJob {
    /// Target layer.
    pub layer: Uuid,
    /// Captured `appearance_version` (freshness guard).
    pub version: u64,
    /// Source image (shared, immutable).
    pub source: Arc<DynamicImage>,
    /// Filter chain (cloned params).
    pub filters: Vec<FilterLayer>,
    /// Masks (shared coverages).
    pub masks: Vec<LayerMask>,
    /// Unique cache key (source + filters + masks).
    pub key: ThumbnailKey,
}

/// Computed thumbnail + compute-side measured costs.
#[derive(Clone)]
pub struct ThumbDone {
    /// Target layer.
    pub layer: Uuid,
    /// Computed version (to compare against the live one).
    pub version: u64,
    /// Thumbnail (48×32, same bytes as the frame path).
    pub thumb: RgbaBuf,
    /// Appearance image: diagnostic payload checked by tests
    /// (never stored or displayed — only `thumb` goes into the response).
    #[allow(dead_code)]
    pub image: Arc<DynamicImage>,
    /// Chain + bake time (µs).
    pub render_us: u128,
    /// Resample-only time (µs).
    pub thumb_us: u128,
    /// `true` when the result comes from the cache (Phase 6G.4).
    pub from_cache: bool,
}

impl std::fmt::Debug for ThumbDone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbDone")
            .field("layer", &self.layer)
            .field("version", &self.version)
            .field("thumb_dims", &(self.thumb.width, self.thumb.height))
            .field("render_us", &self.render_us)
            .field("thumb_us", &self.thumb_us)
            .field("from_cache", &self.from_cache)
            .finish_non_exhaustive()
    }
}

/// Cumulative counters (§14 6G.3).
#[derive(Debug, Default, Clone, Copy)]
pub struct ThumbStats {
    /// Submitted requests (queue or inline).
    pub submitted: u64,
    /// Requests skipped as already pending (sender-side coalescing).
    pub coalesced: u64,
    /// Applied results (fresh).
    pub completed: u64,
    /// Rejected results (stale).
    pub discarded: u64,
    /// Cumulative compute time of applied results (µs).
    pub thumb_us: u128,
    /// Cache hits (result served from cache without computing).
    pub cache_hits: u64,
    /// Cache misses (computation needed).
    pub cache_misses: u64,
    /// Recompute time avoided by hits (µs).
    #[allow(dead_code)]
    pub cache_us_saved: u128,
}

/// Computes (image, thumbnail) from the cloned parts — SAME function as
/// the frame path (`Renderer::appearance_frame` on a rebuilt layer):
/// bit-identical bytes by construction (same code, same inputs).
/// `None` on zero dimensions (guard, never in practice).
pub fn compute_thumb(job: &ThumbJob) -> Option<(Arc<DynamicImage>, RgbaBuf, u128, u128)> {
    let mut layer = crate::PixelLayer::new("thumb-job", Arc::clone(&job.source));
    layer.id = job.layer;
    layer.filter_layers = job.filters.clone();
    layer.masks = job.masks.clone();
    let mut renderer = crate::renderer::Renderer::default();
    let t_render = std::time::Instant::now();
    let (image, _) = renderer.appearance_frame(&layer)?;
    let render_us = t_render.elapsed().as_micros();
    // Resample isolated and measured separately (§1: it dominates) — same inputs,
    // same function as the frame path.
    let t_thumb = std::time::Instant::now();
    let thumb = crate::document::compositing::thumb_buf(&image);
    let thumb_us = t_thumb.elapsed().as_micros();
    Some((image, thumb, render_us, thumb_us))
}

/// Intent queue: one entry per layer, newest wins.
/// Pure and deterministic (tested): 10 fast paints ⇒ 1 computation.
pub fn select_latest(jobs: Vec<ThumbJob>) -> BTreeMap<Uuid, ThumbJob> {
    let mut pending = BTreeMap::new();
    for job in jobs {
        pending.insert(job.layer, job);
    }
    pending
}

/// Secondary task: inline (tests, deterministic direct calls) or dedicated
/// thread (production, drain-replace + computations).
pub struct ThumbWorker {
    tx: Option<mpsc::Sender<ThumbJob>>,
    rx: Option<mpsc::Receiver<ThumbDone>>,
    // `None` in inline mode. Not joined on drop (same pattern as the
    // engine worker: the thread exits alone when the sender is dropped).
    _handle: Option<JoinHandle<()>>,
    /// Thumbnail cache: key (source + filters + masks) → thumbnail.
    cache: HashMap<ThumbnailKey, RgbaBuf>,
}

impl ThumbWorker {
    /// Inline mode: `submit` computes immediately (deterministic, no
    /// thread — tests and ephemeral `apply_command`).
    #[must_use]
    pub fn inline_() -> Self {
        Self {
            tx: None,
            rx: None,
            _handle: None,
            cache: HashMap::new(),
        }
    }

    /// Threaded mode (production): a dedicated thread, drain-replace queue.
    #[must_use]
    pub fn spawned() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<ThumbJob>();
        let (done_tx, done_rx) = mpsc::channel::<ThumbDone>();
        let handle = std::thread::Builder::new()
            .name("cygnus-thumb".into())
            .spawn(move || Self::thread_loop(job_rx, done_tx))
            .ok();
        Self {
            tx: Some(job_tx),
            rx: Some(done_rx),
            _handle: handle,
            cache: HashMap::new(),
        }
    }

    /// True when a dedicated thread runs.
    #[must_use]
    pub fn is_threaded(&self) -> bool {
        self.tx.is_some()
    }

    /// Submits a request (non-blocking). In threaded mode, computation may
    /// be superseded before starting (coalescing). Returns false when the
    /// thread died (the caller degrades to safe inline).
    pub fn submit(&self, job: ThumbJob) -> bool {
        match &self.tx {
            Some(tx) => tx.send(job).is_ok(),
            None => false,
        }
    }

    /// Computes immediately (inline mode AND synchronous fallback).
    /// Returns the full result (to ingest via the version guard).
    #[must_use]
    pub fn compute_now(job: &ThumbJob) -> Option<ThumbDone> {
        let (image, thumb, render_us, thumb_us) = compute_thumb(job)?;
        Some(ThumbDone {
            layer: job.layer,
            version: job.version,
            thumb,
            image,
            render_us,
            thumb_us,
            from_cache: false,
        })
    }

    /// Computes or fetches from cache (Phase 6G.4) — inline only.
    /// On a cache key hit: returns the stored thumbnail (HIT).
    /// Else: computes, stores in cache, returns (MISS).
    #[must_use]
    pub fn compute_cached(&mut self, job: &ThumbJob) -> Option<ThumbDone> {
        if let Some(thumb) = self.cache.get(&job.key) {
            // HIT: rebuild the minimal ThumbDone (no recompute).
            Some(ThumbDone {
                layer: job.layer,
                version: job.version,
                thumb: thumb.clone(),
                image: job.source.clone(),
                render_us: 0,
                thumb_us: 0,
                from_cache: true,
            })
        } else {
            // MISS: full computation + store.
            let (image, thumb, render_us, thumb_us) = compute_thumb(job)?;
            self.cache.insert(job.key.clone(), thumb.clone());
            Some(ThumbDone {
                layer: job.layer,
                version: job.version,
                thumb,
                image,
                render_us,
                thumb_us,
                from_cache: false,
            })
        }
    }

    /// Drains available results (never blocking).
    pub fn drain_results(&mut self) -> Vec<ThumbDone> {
        let mut out = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(done) = rx.try_recv() {
                out.push(done);
            }
        }
        out
    }

    /// Dedicated thread loop: blocks on the first request, drains the
    /// rest via [`select_latest`] (superseded intermediate versions
    /// are NEVER computed), processes the newest per layer (id
    /// order, deterministic), publishes. Clean exit on disconnect.
    fn thread_loop(job_rx: mpsc::Receiver<ThumbJob>, done_tx: mpsc::Sender<ThumbDone>) {
        while let Ok(first) = job_rx.recv() {
            let mut batch = vec![first];
            while let Ok(job) = job_rx.try_recv() {
                batch.push(job);
            }
            let mut dead = false;
            for (_, job) in select_latest(batch) {
                let Some(done) = Self::compute_now(&job) else {
                    continue;
                };
                if done_tx.send(done).is_err() {
                    dead = true;
                    break;
                }
            }
            if dead {
                break;
            }
        }
    }
}

/// Freshness guard (§6-§7): the result only applies when the live
/// layer still carries EXACTLY this version AND this source
/// (`Arc::ptr_eq`: same allocation = same pixels, history
/// snapshots preserving the `Arc`s). Else: reject (never displayed).
#[must_use]
pub fn is_fresh(
    done: &ThumbDone,
    live_version: u64,
    live_source: &Arc<DynamicImage>,
    job_source: &Arc<DynamicImage>,
) -> bool {
    done.version == live_version && Arc::ptr_eq(live_source, job_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_job(value: u8, version: u64) -> ThumbJob {
        use image::{ImageBuffer, Rgba};
        let source = Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            64,
            64,
            Rgba([value, value, value, 255]),
        )));
        let layer = crate::PixelLayer::new("test", source.clone());
        let key = ThumbnailKey::for_layer(&layer);
        ThumbJob {
            layer: Uuid::new_v4(),
            version,
            source,
            filters: Vec::new(),
            masks: Vec::new(),
            key,
        }
    }
    #[test]
    fn calcul_egale_chemin_cadre() {
        // Same function, same inputs: the job produces EXACTLY what the
        // frame path would produce (pixels + thumbnail).
        let job = solid_job(77, 3);
        let done = ThumbWorker::compute_now(&job).expect("calcul");
        assert_eq!(done.version, 3);
        let mut renderer = crate::renderer::Renderer::default();
        let mut layer = crate::PixelLayer::new("x", Arc::clone(&job.source));
        layer.id = job.layer;
        let (image, thumb) = renderer.appearance_frame(&layer).expect("cadre");
        assert_eq!(
            done.image.to_rgba8().into_raw(),
            image.to_rgba8().into_raw()
        );
        assert_eq!(
            (
                done.thumb.width,
                done.thumb.height,
                done.thumb.data.as_ref()
            ),
            (thumb.width, thumb.height, thumb.data.as_ref())
        );
    }

    #[test]
    fn coalescing_ne_garde_que_le_dernier() {
        // 10 fast versions of the same layer ⇒ 1 intent.
        let id = Uuid::new_v4();
        let jobs: Vec<ThumbJob> = (0..10)
            .map(|v| ThumbJob {
                layer: id,
                version: v,
                ..solid_job(1, v)
            })
            .collect();
        let pending = select_latest(jobs);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[&id].version, 9, "la plus récente gagne");
    }

    #[test]
    fn garde_rejette_perime() {
        let job = solid_job(5, 10);
        let done = ThumbWorker::compute_now(&job).expect("calcul");
        // Identical live state: fresh.
        assert!(is_fresh(&done, 10, &job.source, &job.source));
        // Advanced version: stale.
        assert!(!is_fresh(&done, 11, &job.source, &job.source));
        // Replaced source (same theoretical version): stale.
        let other = solid_job(5, 10);
        assert!(!is_fresh(&done, 10, &other.source, &job.source));
    }

    #[test]
    fn inline_sans_thread() {
        let worker = ThumbWorker::inline_();
        assert!(!worker.is_threaded());
        let job = solid_job(9, 1);
        // Without a thread: no sending possible → inline fallback by the caller.
        assert!(!worker.submit(job));
    }
}
