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

//! Composite previews: [`PreviewImage`], appearance cache
//! ([`AppearanceFrameCache`]), legacy ([`render_preview`]) and incremental
//! pipeline (`EngineWorker::render_preview_incremental`).

use super::commands::{DirtyMark, Mutation, PhotoEngineCommand, RenderInvalidation};
use super::metrics::{OpMetrics, PreviewTimings, WorkerMetrics};
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

/// Largest canvas preview dimension (pixels).
pub const PREVIEW_MAX_DIMENSION: u32 = 1600;

/// Composite preview for the canvas (RGBA8, pure, thread-safe).
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewImage {
    /// Width in pixels (thumbnail).
    pub width: u32,
    /// Height in pixels (thumbnail).
    pub height: u32,
    /// RGBA8 pixels row by row (thumbnail).
    pub rgba: Vec<u8>,
    /// Full-resolution composite dimensions before thumbnailing.
    pub full_width: u32,
    /// Full-resolution height.
    pub full_height: u32,
    /// Top-left corner of the DOCUMENT in full-resolution pixels
    /// (the composite covers the infinite plane: origin > 0 as soon as
    /// a layer sticks out).
    pub origin_x: f32,
    /// Full-resolution vertical origin.
    pub origin_y: f32,
    /// Document dimensions (the frame to draw on top).
    pub doc_width: u32,
    /// Document height.
    pub doc_height: u32,
}

/// Appearance sharing during ONE response (`respond()`).
///
/// The persistent renderer alone decides pixel recomputes; this local
/// cache only avoids re-requesting the same result from the three
/// consumers (snapshot, composite, geometry): each pixel
/// layer is resolved once, then cloned into shared `Arc`s (no
/// buffer copies, per goal 5).
pub(crate) struct AppearanceFrameCache {
    /// Resolved images, per layer (`Arc` clones, no pixels).
    /// Phase 6G.3: images ONLY (composite + geometry). Thumbnails
    /// go to the secondary task (never on this path).
    pub(crate) map: HashMap<Uuid, Arc<image::DynamicImage>>,
    /// Renderer calls (first resolutions).
    pub(crate) resolves: u64,
    /// Inter-pass reuses (`Cell`: the resolver is `Fn`).
    pub(crate) hits: std::cell::Cell<u64>,
}

impl AppearanceFrameCache {
    /// Resolves each pixel layer ONCE from the renderer
    /// (image-only path, no thumbnail or preview).
    pub(crate) fn build(document: &Document) -> Self {
        let ids: Vec<Uuid> = document.all_pixel_ids();
        let mut map = HashMap::with_capacity(ids.len());
        let mut resolves = 0;
        for id in ids {
            if let Some(image) = document.appearance_image_only(id) {
                map.insert(id, image);
                resolves += 1;
            }
        }
        Self {
            map,
            resolves,
            hits: std::cell::Cell::new(0),
        }
    }

    /// Shared image for compositing (`Arc` clone).
    pub(crate) fn image(&self, id: Uuid) -> Option<Arc<image::DynamicImage>> {
        let image = self.map.get(&id)?;
        self.hits.set(self.hits.get() + 1);
        Some(Arc::clone(image))
    }

    /// Reuse counter (read after the response).
    pub(crate) fn hits(&self) -> u64 {
        self.hits.get()
    }
}

/// Downscales a composite to the `PREVIEW_MAX_DIMENSION` cap WITHOUT
/// ever upscaling (`thumbnail` upscales small composites, which
/// inflates the texture and skews the thumbnail→document scale).
/// Capped thumbnail + shrink input pixels (0 when no
/// shrink — §8 6G.2 counter, no effect on bytes).
fn capped_preview(image: image::DynamicImage) -> (image::DynamicImage, u64) {
    let (w, h) = (image.width(), image.height());
    // Bit-exact fast path (Phase 6G.2 §3.5D): same bytes as
    // `thumbnail` (proven by fuzzing: dims + pixels), ~4× faster, existing
    // dedicated pool. Every other case: unchanged legacy.
    if let image::DynamicImage::ImageRgba8(buf) = &image {
        if let Some((nw, nh)) =
            photo_engine::document::compositing::capped_thumbnail_dims(w, h, PREVIEW_MAX_DIMENSION)
        {
            let fast = photo_engine::document::compositing::fast_thumbnail_rgba8(buf, nw, nh);
            debug_assert_eq!((fast.width(), fast.height()), (nw, nh));
            return (
                image::DynamicImage::ImageRgba8(fast),
                u64::from(w) * u64::from(h),
            );
        }
        return (image, 0);
    }
    if w.max(h) <= PREVIEW_MAX_DIMENSION {
        (image, 0)
    } else {
        (
            image.thumbnail(PREVIEW_MAX_DIMENSION, PREVIEW_MAX_DIMENSION),
            u64::from(w) * u64::from(h),
        )
    }
}

/// Computes the composite preview (capped at `PREVIEW_MAX_DIMENSION`).
/// Expensive: call only on a background thread.
/// `clip_to_doc` = preview clipped to the document (else infinite plane with
/// the document origin in `origin_*`).
pub fn render_preview(document: &Document, clip_to_doc: bool) -> Option<PreviewImage> {
    render_preview_timed(document, clip_to_doc).0
}

/// Instrumented variant of [`render_preview`] (perf diagnostics):
/// separates full-resolution composite time from thumbnail
/// time. Identical functional behavior.
pub fn render_preview_timed(
    document: &Document,
    clip_to_doc: bool,
) -> (Option<PreviewImage>, PreviewTimings) {
    let frame = AppearanceFrameCache::build(document);
    render_preview_timed_with(document, clip_to_doc, &frame)
}

/// Variant of [`render_preview_timed`] with an already-resolved
/// appearance frame: composite and geometry share the same `Arc`s (zero
/// new resolutions). Identical behavior at equal content.
fn render_preview_timed_with(
    document: &Document,
    clip_to_doc: bool,
    frame: &AppearanceFrameCache,
) -> (Option<PreviewImage>, PreviewTimings) {
    let doc_width = document.width.max(1);
    let doc_height = document.height.max(1);
    if clip_to_doc {
        let t_composite = std::time::Instant::now();
        let cropped = document.composite();
        let composite_us = t_composite.elapsed().as_micros();
        let Some(cropped) = cropped else {
            return (None, PreviewTimings::default());
        };
        let t_thumb = std::time::Instant::now();
        let (capped, _) = capped_preview(cropped);
        let raster = capped.to_rgba8();
        let (w, h) = (raster.width(), raster.height());
        let thumb_us = t_thumb.elapsed().as_micros();
        // `composite()` centers the document (symmetric extents): the
        // frame equals the buffer except when content is smaller than the document.
        return (
            Some(PreviewImage {
                width: w,
                height: h,
                rgba: raster.into_raw(),
                full_width: w,
                full_height: h,
                origin_x: (w as f32 - doc_width as f32) / 2.0,
                origin_y: (h as f32 - doc_height as f32) / 2.0,
                doc_width,
                doc_height,
            }),
            PreviewTimings {
                composite_us,
                thumb_us,
                geometry_us: 0,
                blend_us: 0,
                layers_blended: 0,
                pixels_processed: 0,
                scope_px: u64::from(w) * u64::from(h),
                full_px: u64::from(w) * u64::from(h),
                preview_px: u64::from(w) * u64::from(h),
                preview_resize_pixels: 0,
            },
        );
    }
    let t_composite = std::time::Instant::now();
    let mut composite_stats = photo_engine::document::compositing::CompositeStats::default();
    let composite =
        document.composite_preview_with_stats(&|id| frame.image(id), &mut composite_stats);
    let composite_us = t_composite.elapsed().as_micros();
    let Some(composite) = composite else {
        return (None, PreviewTimings::default());
    };
    let (full_w, full_h) = (composite.width(), composite.height());
    let t_thumb = std::time::Instant::now();
    let (preview, _) = capped_preview(composite);
    let raster = preview.to_rgba8();
    let (w, h) = (raster.width(), raster.height());
    let thumb_us = t_thumb.elapsed().as_micros();
    // Same uniform thumbnail: full-resolution geometry is
    // scaled down (symmetric half-extents → exact origin,
    // not an approximate integer re-centering).
    let t_geometry = std::time::Instant::now();
    let (origin_x, origin_y) = document
        .preview_geometry_with(&|id| frame.image(id))
        .map(|(_, _, ox, oy)| (ox, oy))
        .unwrap_or((
            (full_w.saturating_sub(doc_width)) as f32 / 2.0,
            (full_h.saturating_sub(doc_height)) as f32 / 2.0,
        ));
    let geometry_us = t_geometry.elapsed().as_micros();
    let kx = if full_w > 0 {
        w as f32 / full_w as f32
    } else {
        1.0
    };
    let ky = if full_h > 0 {
        h as f32 / full_h as f32
    } else {
        1.0
    };
    (
        Some(PreviewImage {
            width: w,
            height: h,
            rgba: raster.into_raw(),
            full_width: full_w,
            full_height: full_h,
            origin_x: origin_x * kx,
            origin_y: origin_y * ky,
            doc_width,
            doc_height,
        }),
        PreviewTimings {
            composite_us,
            thumb_us,
            geometry_us,
            blend_us: composite_stats.blend_us,
            layers_blended: composite_stats.layers_blended,
            pixels_processed: composite_stats.pixels_processed,
            scope_px: composite_stats.scope_px,
            full_px: u64::from(full_w) * u64::from(full_h),
            preview_px: u64::from(w) * u64::from(h),
            preview_resize_pixels: 0,
        },
    )
}

impl EngineWorker {
    /// Composite preview via the incremental pipeline (Phase 6E): the extent
    /// (clipped document or infinite plane) is cut into tiles rendered by
    /// `RenderWorker` (hits reused, miss ⇒ windowed 6C
    /// `composite_region_with`), then assembled. Post-processing (cap, geometry)
    /// IDENTICAL to legacy: at equal content, same bytes.
    ///
    /// Also returns orchestration stats (requested tiles/hits/miss)
    /// for `OpMetrics`. The cache persists across calls: a small
    /// mutation only re-renders its dirty tiles.
    pub(crate) fn render_preview_incremental(
        &mut self,
        frame: &AppearanceFrameCache,
    ) -> (
        Option<PreviewImage>,
        PreviewTimings,
        photo_engine::document::RenderWorkerStats,
    ) {
        let doc_width = self.document.width.max(1);
        let doc_height = self.document.height.max(1);
        let resolve = |id: Uuid| frame.image(id);
        // Extent + request per mode (same spaces as legacy):
        // clipped ⇒ document viewport; infinite plane ⇒ request covering the
        // scope (DOCUMENT SPACE via `scope_request`). Scale 1.0: display
        // zoom stays state-only canvas-side (AGENTS.md).
        let (scope, request) = if self.clip_to_doc {
            let scope = photo_engine::document::ScopeGeom::document(doc_width, doc_height);
            let request = photo_engine::document::RenderRequest::new(
                TileRegion::new(0, 0, doc_width, doc_height),
                1.0,
            );
            (scope, request)
        } else {
            let scope = self.render_worker.scope_for(&self.document, &resolve);
            (
                scope,
                photo_engine::document::RenderWorker::scope_request(scope, 1.0),
            )
        };
        let t_composite = std::time::Instant::now();
        let mut composite_stats = photo_engine::document::compositing::CompositeStats::default();
        let mut worker_stats = photo_engine::document::RenderWorkerStats::default();
        let assembled = self.render_worker.render(
            &self.document,
            &resolve,
            &mut composite_stats,
            scope,
            &request,
            &photo_engine::document::RenderPriorityContext::idle(),
            &mut worker_stats,
        );
        let composite_us = t_composite.elapsed().as_micros();
        let Some(assembled) = assembled else {
            return (None, PreviewTimings::default(), worker_stats);
        };
        // Assembly starts at the viewport origin (0,0 in practice:
        // full viewports); legacy reasons the same way over its buffer.
        let composite = assembled.image;
        if self.clip_to_doc {
            let t_thumb = std::time::Instant::now();
            let (preview, resize_px) = capped_preview(composite);
            let raster = preview.to_rgba8();
            let (w, h) = (raster.width(), raster.height());
            let thumb_us = t_thumb.elapsed().as_micros();
            return (
                Some(PreviewImage {
                    width: w,
                    height: h,
                    rgba: raster.into_raw(),
                    full_width: w,
                    full_height: h,
                    origin_x: (w as f32 - doc_width as f32) / 2.0,
                    origin_y: (h as f32 - doc_height as f32) / 2.0,
                    doc_width,
                    doc_height,
                }),
                PreviewTimings {
                    composite_us,
                    thumb_us,
                    geometry_us: 0,
                    blend_us: composite_stats.blend_us,
                    layers_blended: composite_stats.layers_blended,
                    pixels_processed: composite_stats.pixels_processed,
                    scope_px: worker_stats.dependency_px,
                    full_px: u64::from(w) * u64::from(h),
                    preview_px: u64::from(w) * u64::from(h),
                    preview_resize_pixels: resize_px,
                },
                worker_stats,
            );
        }
        let (full_w, full_h) = (composite.width(), composite.height());
        let t_thumb = std::time::Instant::now();
        let (preview, resize_px) = capped_preview(composite);
        let raster = preview.to_rgba8();
        let (w, h) = (raster.width(), raster.height());
        let thumb_us = t_thumb.elapsed().as_micros();
        let t_geometry = std::time::Instant::now();
        let (origin_x, origin_y) = self
            .document
            .preview_geometry_with(&|id| frame.image(id))
            .map(|(_, _, ox, oy)| (ox, oy))
            .unwrap_or((
                (full_w.saturating_sub(doc_width)) as f32 / 2.0,
                (full_h.saturating_sub(doc_height)) as f32 / 2.0,
            ));
        let geometry_us = t_geometry.elapsed().as_micros();
        let kx = if full_w > 0 {
            w as f32 / full_w as f32
        } else {
            1.0
        };
        let ky = if full_h > 0 {
            h as f32 / full_h as f32
        } else {
            1.0
        };
        (
            Some(PreviewImage {
                width: w,
                height: h,
                rgba: raster.into_raw(),
                full_width: full_w,
                full_height: full_h,
                origin_x: origin_x * kx,
                origin_y: origin_y * ky,
                doc_width,
                doc_height,
            }),
            PreviewTimings {
                composite_us,
                thumb_us,
                geometry_us,
                blend_us: composite_stats.blend_us,
                layers_blended: composite_stats.layers_blended,
                pixels_processed: composite_stats.pixels_processed,
                scope_px: worker_stats.dependency_px,
                full_px: u64::from(full_w) * u64::from(full_h),
                preview_px: u64::from(w) * u64::from(h),
                preview_resize_pixels: resize_px,
            },
            worker_stats,
        )
    }
}
