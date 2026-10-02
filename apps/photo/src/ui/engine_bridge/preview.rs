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
#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.

//! Aperçus composites : [`PreviewImage`], cache d'apparences
//! ([`AppearanceFrameCache`]), legacy ([`render_preview`]) et pipeline
//! incrémental (`EngineWorker::render_preview_incremental`).

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

/// Plus grande dimension de l'aperçu canvas (pixels).
pub const PREVIEW_MAX_DIMENSION: u32 = 1600;

/// Aperçu composite pour le canvas (RGBA8, pur, thread-safe).
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewImage {
    /// Largeur en pixels (miniature).
    pub width: u32,
    /// Hauteur en pixels (miniature).
    pub height: u32,
    /// Pixels RGBA8 ligne par ligne (miniature).
    pub rgba: Vec<u8>,
    /// Dimensions du composite pleine résolution avant miniature.
    pub full_width: u32,
    /// Hauteur pleine résolution.
    pub full_height: u32,
    /// Coin haut-gauche du DOCUMENT en pixels pleine résolution
    /// (le composite couvre le plan infini : origine > 0 dès qu'un
    /// calque dépasse).
    pub origin_x: f32,
    /// Origine verticale pleine résolution.
    pub origin_y: f32,
    /// Dimensions du document (le cadre à dessiner par-dessus).
    pub doc_width: u32,
    /// Hauteur du document.
    pub doc_height: u32,
}

/// Partage des apparences pendant UNE réponse (`respond()`).
///
/// Le renderer persistant décide seul des recalculs pixels ; ce cache
/// local évite seulement de redemander le même résultat aux trois
/// consommateurs (snapshot, composite, géométrie) : chaque calque
/// pixels est résolu une fois, puis cloné en `Arc` partagés (aucune
/// copie de buffers, conformément à l'objectif 5).
pub(crate) struct AppearanceFrameCache {
    /// Images résolues, par calque (clones `Arc`, pas de pixels).
    /// Phase 6G.3 : UNIQUEMENT les images (composite + géométrie). Les
    /// miniatures partent en tâche secondaire (jamais sur ce chemin).
    pub(crate) map: HashMap<Uuid, Arc<image::DynamicImage>>,
    /// Appels au renderer (premières résolutions).
    pub(crate) resolves: u64,
    /// Réutilisations inter-passes (`Cell` : le résolveur est `Fn`).
    pub(crate) hits: std::cell::Cell<u64>,
}

impl AppearanceFrameCache {
    /// Résout chaque calque pixels UNE fois auprès du renderer
    /// (chemin image seule, sans miniature ni preview).
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

    /// Image partagée pour le compositing (clone d'`Arc`).
    pub(crate) fn image(&self, id: Uuid) -> Option<Arc<image::DynamicImage>> {
        let image = self.map.get(&id)?;
        self.hits.set(self.hits.get() + 1);
        Some(Arc::clone(image))
    }

    /// Compteur de réutilisations (lecture après la réponse).
    pub(crate) fn hits(&self) -> u64 {
        self.hits.get()
    }
}

/// Réduit un composite au plafond `PREVIEW_MAX_DIMENSION` SANS
/// jamais agrandir (`thumbnail` agrandit les petits composites, ce
/// qui gonfle la texture et fausse l'échelle miniature→document).
/// Miniature plafonnée + pixels d'entrée du rétrécissement (0 si pas de
/// shrink — compteur §8 6G.2, aucun effet sur les octets).
fn capped_preview(image: image::DynamicImage) -> (image::DynamicImage, u64) {
    let (w, h) = (image.width(), image.height());
    // Chemin rapide bit-exact (Phase 6G.2 §3.5D) : mêmes octets que
    // `thumbnail` (fuzz prouvé : dims + pixels), ~4× plus rapide, pool
    // dédié existant. Tout autre cas : legacy inchangé.
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

/// Calcule l'aperçu composite (plafonné à `PREVIEW_MAX_DIMENSION`).
/// Coûteux : à appeler uniquement sur thread background.
/// `clip_to_doc` = aperçu rogné au document (sinon plan infini avec
/// l'origine du document dans `origin_*`).
pub fn render_preview(document: &Document, clip_to_doc: bool) -> Option<PreviewImage> {
    render_preview_timed(document, clip_to_doc).0
}

/// Variante instrumentée de [`render_preview`] (diagnostic perf) :
/// sépare le temps du composite pleine résolution et celui de la
/// miniature. Comportement fonctionnel identique.
pub fn render_preview_timed(
    document: &Document,
    clip_to_doc: bool,
) -> (Option<PreviewImage>, PreviewTimings) {
    let frame = AppearanceFrameCache::build(document);
    render_preview_timed_with(document, clip_to_doc, &frame)
}

/// Variante de [`render_preview_timed`] avec cadre d'apparences déjà
/// résolu : composite et géométrie partagent les mêmes `Arc` (zéro
/// nouvelle résolution). Comportement identique à parité de contenu.
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
        // `composite()` centre le document (extents symétriques) : le
        // cadre vaut le buffer sauf contenu plus petit que le document.
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
    // Même miniature uniforme : la géométrie pleine résolution est
    // ramenée à l'échelle (demi-extents symétriques → origine exacte,
    // pas un recentrage entier approximatif).
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
    /// Aperçu composite via le pipeline incrémental (Phase 6E) : le périmètre
    /// (document rogné ou plan infini) est découpé en tuiles rendues par
    /// `RenderWorker` (hits réutilisés, miss ⇒ `composite_region_with`
    /// fenêtré 6C), puis assemblé. Post-traitement (plafond, géométrie)
    /// IDENTIQUE au legacy : à contenu égal, mêmes octets.
    ///
    /// Retourne aussi les stats d'orchestration (tuiles demandées/hits/miss)
    /// pour `OpMetrics`. Le cache persiste entre les appels : une petite
    /// mutation ne re-rend que ses tuiles sales.
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
        // Périmètre + requête selon le mode (mêmes espaces que le legacy) :
        // rogné ⇒ viewport document ; plan infini ⇒ requête couvrant le
        // scope (DOCUMENT SPACE via `scope_request`). Échelle 1.0 : le zoom
        // d'affichage reste state-only côté canvas (AGENTS.md).
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
        // L'assemblage démarre à l'origine du viewport (0,0 en pratique :
        // viewports pleins) ; le legacy raisonne de même sur son buffer.
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
