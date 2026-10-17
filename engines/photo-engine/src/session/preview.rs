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
use crate::snapshot::{PhotoLayerInfo, PhotoLayerThumb, snapshot_layers, snapshot_layers_with};
use crate::tiles::TileRegion;
use crate::{BlendMode, Document, RenderEvent, RenderRevision};
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
/// consumers (snapshot, composite, geometry): each pixel AND vector
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
    /// (image-only path, no thumbnail or preview), then each vector
    /// layer from its rasterized appearance (vectors never enter the
    /// pixel cache — see `appearance_vector_image`).
    pub(crate) fn build(document: &Document) -> Self {
        let mut ids: Vec<Uuid> = document.all_pixel_ids();
        ids.extend(document.all_vector_ids());
        let mut map = HashMap::with_capacity(ids.len());
        let mut resolves = 0;
        for id in ids {
            if let Some(image) = document
                .appearance_image_only(id)
                .or_else(|| document.appearance_vector_image(id))
            {
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

/// Zeroes pixels outside the document frame (clip-to-doc
/// display): same plane dimensions and origin as the infinite
/// mode, so toggling the clip never refits the view — out-of-frame
/// content is masked, not cropped away.
fn mask_outside_doc(
    raster: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    origin_x: f32,
    origin_y: f32,
    doc_w: u32,
    doc_h: u32,
) {
    let (w, h) = (raster.width(), raster.height());
    let x0 = origin_x.ceil().max(0.0) as u32;
    let y0 = origin_y.ceil().max(0.0) as u32;
    let x1 = (origin_x + doc_w as f32).floor().min(w as f32).max(0.0) as u32;
    let y1 = (origin_y + doc_h as f32).floor().min(h as f32).max(0.0) as u32;
    for (x, y, px) in raster.enumerate_pixels_mut() {
        if x < x0 || x >= x1 || y < y0 || y >= y1 {
            *px = image::Rgba([0, 0, 0, 0]);
        }
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
            crate::document::compositing::capped_thumbnail_dims(w, h, PREVIEW_MAX_DIMENSION)
        {
            let fast = crate::document::compositing::fast_thumbnail_rgba8(buf, nw, nh);
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
/// `clip_to_doc` masks out-of-document pixels (same framing as the
/// infinite plane — no refit jump when toggling).
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
        let mut composite_stats = crate::document::compositing::CompositeStats::default();
        let composite =
            document.composite_preview_with_stats(&|id| frame.image(id), &mut composite_stats);
        let composite_us = t_composite.elapsed().as_micros();
        let Some(composite) = composite else {
            return (None, PreviewTimings::default());
        };
        let (full_w, full_h) = (composite.width(), composite.height());
        // Same plane geometry as the infinite mode (framing
        // identical — toggling the clip never refits the view).
        let (origin_x, origin_y) = document
            .preview_geometry_with(&|id| frame.image(id))
            .map(|(_, _, ox, oy)| (ox, oy))
            .unwrap_or((0.0, 0.0));
        // Mask (not crop): pixels outside the document frame go
        // transparent, dimensions and origin stay plane-sized.
        let mut raster = composite.to_rgba8();
        mask_outside_doc(&mut raster, origin_x, origin_y, doc_width, doc_height);
        let t_thumb = std::time::Instant::now();
        let (capped, _) = capped_preview(image::DynamicImage::ImageRgba8(raster));
        let raster = capped.to_rgba8();
        let (w, h) = (raster.width(), raster.height());
        let thumb_us = t_thumb.elapsed().as_micros();
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
        return (
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
                geometry_us: 0,
                blend_us: composite_stats.blend_us,
                layers_blended: composite_stats.layers_blended,
                pixels_processed: composite_stats.pixels_processed,
                scope_px: composite_stats.scope_px,
                full_px: u64::from(full_w) * u64::from(full_h),
                preview_px: u64::from(w) * u64::from(h),
                preview_resize_pixels: 0,
            },
        );
    }
    let t_composite = std::time::Instant::now();
    let mut composite_stats = crate::document::compositing::CompositeStats::default();
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
    /// Display-scale composite (see `crate::document::scaled`):
    /// `None` when the document fits the cap or holds canvas-space data
    /// the scaled view cannot resample (the caller then runs the
    /// full-resolution pipeline). Same geometry contract as every preview
    /// (origin in thumbnail pixels, `full / thumb` = document scale).
    pub(crate) fn render_preview_scaled(
        &mut self,
    ) -> Option<(
        Option<PreviewImage>,
        PreviewTimings,
        crate::document::RenderWorkerStats,
    )> {
        let scale = self.document.display_scale(PREVIEW_MAX_DIMENSION)?;
        let view = self.document.scaled_view(scale)?;
        let t_composite = std::time::Instant::now();
        // Appearances evaluated at the display scale (filters and masks
        // included): the full-resolution chain never runs on this path.
        let mut small: HashMap<Uuid, Arc<image::DynamicImage>> = HashMap::new();
        for id in self.document.all_pixel_ids() {
            if let Some(layer) = self.document.pixel_layer(id) {
                small.insert(id, self.scaled_appearances.pixel_appearance(layer, scale));
            }
        }
        for id in self.document.all_vector_ids() {
            if let Some(full) = self.document.appearance_vector_image(id) {
                small.insert(id, self.scaled_appearances.get(id, &full, scale));
            }
        }
        self.scaled_appearances.retain(|id| small.contains_key(&id));
        let mut stats = crate::document::compositing::CompositeStats::default();
        let resolve = |id: Uuid| small.get(&id).cloned();
        let part = view.composite_root_range_with(0..view.root.len(), &resolve, &mut stats);
        let composite_us = t_composite.elapsed().as_micros();
        // Tiles are not consulted on this path: nothing stays pending.
        self.render_worker.cache_mut().clear_dirty();
        let worker_stats = crate::document::RenderWorkerStats::default();
        let Some(part) = part else {
            return Some((None, PreviewTimings::default(), worker_stats));
        };
        let t_thumb = std::time::Instant::now();
        let mut raster = part.image.to_rgba8();
        if self.clip_to_doc {
            mask_outside_doc(
                &mut raster,
                part.origin_x,
                part.origin_y,
                view.width,
                view.height,
            );
        }
        let (scaled_w, scaled_h) = (raster.width(), raster.height());
        // A plane far larger than the document can still exceed the cap.
        let (capped, resize_px) = capped_preview(image::DynamicImage::ImageRgba8(raster));
        let raster = capped.to_rgba8();
        let (w, h) = (raster.width(), raster.height());
        let k = w as f32 / scaled_w.max(1) as f32;
        let thumb_to_doc = 1.0 / (scale * k);
        let thumb_us = t_thumb.elapsed().as_micros();
        Some((
            Some(PreviewImage {
                width: w,
                height: h,
                rgba: raster.into_raw(),
                full_width: (w as f32 * thumb_to_doc).round() as u32,
                full_height: (h as f32 * thumb_to_doc).round() as u32,
                origin_x: part.origin_x * k,
                origin_y: part.origin_y * k,
                doc_width: self.document.width.max(1),
                doc_height: self.document.height.max(1),
            }),
            PreviewTimings {
                composite_us,
                thumb_us,
                geometry_us: 0,
                blend_us: stats.blend_us,
                layers_blended: stats.layers_blended,
                pixels_processed: stats.pixels_processed,
                scope_px: stats.scope_px,
                full_px: u64::from(scaled_w) * u64::from(scaled_h),
                preview_px: u64::from(w) * u64::from(h),
                preview_resize_pixels: resize_px,
            },
            worker_stats,
        ))
    }

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
        crate::document::RenderWorkerStats,
    ) {
        let doc_width = self.document.width.max(1);
        let doc_height = self.document.height.max(1);
        let resolve = |id: Uuid| frame.image(id);
        // Clipped and infinite share the plane scope (identical
        // framing — the clip only masks, never refits). Scale 1.0:
        // display zoom stays state-only canvas-side (AGENTS.md).
        let scope = self.render_worker.scope_for(&self.document, &resolve);
        let (scope, request) = (
            scope,
            crate::document::RenderWorker::scope_request(scope, 1.0),
        );
        // Full-frame bypass: when the marked-dirty coverage dominates
        // a multi-tile scope, per-tile windowed re-renders redo the
        // same whole-image work per tile (a rotated layer spans every
        // dependency window: prepare_top rotates the full image per
        // tile). One full-frame legacy composite is ~tiles× cheaper
        // with bit-identical bytes. Small scopes always stay tiled
        // (exact tile-metric tests live there).
        let scope_px = u64::from(scope.w.max(1)) * u64::from(scope.h.max(1));
        let dirty_px = self.render_worker.cache().dirty().bounds().area();
        if scope_px > 4 * 256 * 256 && dirty_px * 2 >= scope_px {
            let (preview, timings) =
                render_preview_timed_with(&self.document, self.clip_to_doc, frame);
            self.render_worker.cache_mut().clear_dirty();
            return (
                preview,
                timings,
                crate::document::RenderWorkerStats::default(),
            );
        }
        let t_composite = std::time::Instant::now();
        let mut composite_stats = crate::document::compositing::CompositeStats::default();
        let mut worker_stats = crate::document::RenderWorkerStats::default();
        let assembled = self.render_worker.render(
            &self.document,
            &resolve,
            &mut composite_stats,
            scope,
            &request,
            &crate::document::RenderPriorityContext::idle(),
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
            // Mask (not crop): same plane framing as the infinite
            // mode — toggling the clip never refits the view.
            let (full_w, full_h) = (composite.width(), composite.height());
            let (origin_x, origin_y) = self
                .document
                .preview_geometry_with(&|id| frame.image(id))
                .map(|(_, _, ox, oy)| (ox, oy))
                .unwrap_or((0.0, 0.0));
            let mut raster = composite.to_rgba8();
            mask_outside_doc(&mut raster, origin_x, origin_y, doc_width, doc_height);
            let t_thumb = std::time::Instant::now();
            let (preview, resize_px) = capped_preview(image::DynamicImage::ImageRgba8(raster));
            let raster = preview.to_rgba8();
            let (w, h) = (raster.width(), raster.height());
            let thumb_us = t_thumb.elapsed().as_micros();
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
            return (
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

/// Largest dimension of a dragged layer's proxy texture (GPU-side
/// quad during a move; the exact composite lands on release).
pub const PROXY_LAYER_MAX_DIMENSION: u32 = 2048;

/// Pixels of the dragged layer for the drag proxy (carrier space,
/// downscaled to [`PROXY_LAYER_MAX_DIMENSION`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyLayerImage {
    /// Texture width (pixels).
    pub width: u32,
    /// Texture height (pixels).
    pub height: u32,
    /// RGBA8 pixels row by row.
    pub rgba: Vec<u8>,
    /// Carrier dimensions before downscaling (must match the drag).
    pub source_width: u32,
    /// Carrier height before downscaling.
    pub source_height: u32,
    /// Layer opacity (0..=100), applied as a texture tint.
    pub opacity: f32,
}

/// One flattened part of the split stack as a canvas preview (same
/// geometry contract as the main preview: origin in thumbnail pixels).
pub(crate) fn proxy_part_preview(
    part: crate::document::ProxyPart,
    scale: f32,
    doc_width: u32,
    doc_height: u32,
) -> PreviewImage {
    let buffer_w = part.image.width().max(1);
    let (capped, _) = capped_preview(image::DynamicImage::ImageRgba8(part.image.to_rgba8()));
    let raster = capped.to_rgba8();
    let (w, h) = (raster.width(), raster.height());
    // `scale` = buffer pixels per document pixel (1 on the full path).
    let k = w as f32 / buffer_w as f32;
    let thumb_to_doc = 1.0 / (scale * k);
    PreviewImage {
        width: w,
        height: h,
        rgba: raster.into_raw(),
        full_width: (w as f32 * thumb_to_doc).round() as u32,
        full_height: (h as f32 * thumb_to_doc).round() as u32,
        origin_x: part.origin_x * k,
        origin_y: part.origin_y * k,
        doc_width,
        doc_height,
    }
}

/// Dragged layer appearance as a proxy texture.
/// `source_dims` = carrier dimensions the quad spans (the image may be
/// a display-scale appearance, smaller than the carrier).
pub(crate) fn proxy_layer_image(
    image: &image::DynamicImage,
    source_dims: (u32, u32),
    opacity: f32,
) -> ProxyLayerImage {
    let (sw, sh) = (image.width(), image.height());
    let rgba = image.to_rgba8();
    let raster = match crate::document::compositing::capped_thumbnail_dims(
        sw,
        sh,
        PROXY_LAYER_MAX_DIMENSION,
    ) {
        Some((nw, nh)) => crate::document::compositing::fast_thumbnail_rgba8(&rgba, nw, nh),
        None => rgba,
    };
    ProxyLayerImage {
        width: raster.width(),
        height: raster.height(),
        rgba: raster.into_raw(),
        source_width: source_dims.0,
        source_height: source_dims.1,
        opacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_layer_reaches_preview_pixels() {
        // Regression: the frame cache only resolved pixels, so vector
        // shapes never reached the canvas.
        let mut document = Document::new(32, 32);
        document.push_layer(crate::LayerNode::Vector(crate::VectorLayer::new(
            "forme",
            crate::rect(4.0, 4.0, 24.0, 24.0),
        )));
        let frame = AppearanceFrameCache::build(&document);
        assert_eq!(frame.resolves, 1, "vector appearance resolved");
        let preview = render_preview(&document, false).expect("preview composite");
        assert!(
            preview.rgba.iter().skip(3).step_by(4).any(|a| *a > 0),
            "vector pixels visible in preview"
        );
    }
}
