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

//! Per-layer appearance rendering with a TARGETED CACHE.
//!
//! [`Renderer`] is the single orchestrator of appearance computation
//! (source × live filters). Per layer:
//!
//! 1. validity key = (layer identity, ORDERED signature of its filter
//!    chain, physical source identity);
//! 2. HIT → the cached appearance is returned as is: ZERO recompute,
//!    zero compute dispatch, zero pixel allocation;
//! 3. MISS → the chain runs via [`crate::filters::render_chain`], which
//!    routes each effect to [`crate::gpu`] COMPUTE SHADERS when an adapter
//!    is available (rayon CPU fallback otherwise), then the result
//!    (full-resolution image + preview + thumbnail) is inserted
//!    into the cache.
//!
//! Source identity is validated by Arc pointer comparison (with entry
//! keep-alive to prevent any address reuse — same ABA defense as the UI
//! texture cache).
//!
//! The [`Renderer::hits`]/[`Renderer::misses`] counters make the
//! behavior observable: editing layer N must cause a single MISS on N,
//! never on its neighbors — the LayerTree "targeted GPU cache"
//! promise.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgba};
use uuid::Uuid;

use crate::document::{Appearance, Document, FilterLayer, LayerMask, PixelLayer, RgbaBuf};

/// Cache entry pre-computed OFF the UI thread, transferable to the
/// live document.
///
/// Use case: toggling a filter (shader) or a mask changes the signature →
/// `appearance_hit` MISS → `appearance()` would run `render_chain` + bake
/// + preview/thumb at FULL RESOLUTION on the UI thread (multi-hundred-ms
/// freeze on large images). Instead, the app clones the document, applies
/// the toggle on the clone in `spawn_blocking`, exports the warm entry
/// and inserts it into the live document on receipt:
/// `PreviewCache::sync()` then HITs without ever touching the pool
/// from the UI.
///
/// All fields are `Arc`s (zero pixel copy): inter-thread transfer is
/// cheap. `Send` required for `Task::perform`, manual `Debug` (never
/// pixel dumps in logs).
#[derive(Clone)]
pub struct WarmedAppearance {
    /// Ordered signature of the filter chain AFTER toggle.
    pub filter_signature: u64,
    /// Source kept alive (pointer-identity validation).
    pub source: Arc<DynamicImage>,
    /// Unmasked image (filter chain alone).
    pub unmasked: Arc<DynamicImage>,
    /// Mask-coverage signature AFTER toggle.
    pub mask_signature: u64,
    /// Combined coverage of active masks (`None` = none).
    pub mask_cover: Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
    /// Full derived appearance (image + preview + thumbnail).
    pub appearance: Appearance,
}

impl std::fmt::Debug for WarmedAppearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarmedAppearance")
            .field("filter_signature", &self.filter_signature)
            .field("mask_signature", &self.mask_signature)
            .field(
                "preview_dims",
                &(
                    self.appearance.preview.width,
                    self.appearance.preview.height,
                ),
            )
            .finish()
    }
}

/// Cache entry: derived appearance + validity proofs.
///
/// The UNMASKED image (source × filters) and the mask coverage are cached
/// SEPARATELY: painting a mask never re-runs the filter chain, it only
/// recombines the coverage with the already-warm unmasked image. The
/// separate coverage is also the data a future shader path will sample
/// directly at draw time.
struct CacheEntry {
    /// Ordered signature of the filter chain at compute time.
    filter_signature: u64,
    /// Source kept alive: pointer-identity validation (a paint/replacement
    /// yields a new Arc → guaranteed miss).
    source: Arc<DynamicImage>,
    /// Unmasked image (filter chain alone), reused when only masks
    /// change.
    unmasked: Arc<DynamicImage>,
    /// Mask-coverage signature at compute time.
    mask_signature: u64,
    /// Combined coverage of active masks (`None` = no active mask).
    mask_cover: Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
    appearance: Appearance,
}

/// Per-layer appearance cache, fed by the render chain (GPU compute
/// shaders when available, rayon CPU otherwise).
#[derive(Default)]
pub struct Renderer {
    entries: HashMap<Uuid, CacheEntry>,
    hits: u64,
    misses: u64,
    /// Regenerated `preview` buffers (reduced full resolution).
    preview_rebuilds: u64,
    /// Regenerated `thumb` thumbnails.
    thumb_rebuilds: u64,
}

/// Appearance counters (observability, no render effect).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceStats {
    /// Entries re-served without chain recompute.
    pub hits: u64,
    /// Recomputed entries (chain re-run).
    pub misses: u64,
    /// Regenerated `preview` buffers (resample included).
    pub preview_rebuilds: u64,
    /// Regenerated `thumb` thumbnails.
    pub thumb_rebuilds: u64,
}

impl Renderer {
    /// Renderer detached from the GPU backend: effects stay on their CPU
    /// path even if an adapter appears later. Reserved for deterministic
    /// tests; prefer [`Renderer::new`] in production.
    pub fn without_gpu() -> Self {
        Self::default()
    }

    /// Layer derived appearance, from cache when still valid — else
    /// recomputed (GPU compute when available) and inserted.
    pub fn appearance(&mut self, layer: &PixelLayer) -> Appearance {
        // Le chemin MISS (render_chain + preview/thumb) est un calcul image
        // The MISS path (render_chain + preview/thumb) is heavy image
        // computation: runs on the DEDICATED render pool (never global) even
        // when triggered by UI sync. When warm, install overhead is negligible.
        crate::render_pool::run_parallel(|| self.appearance_locked(layer))
    }

    /// [`Renderer::appearance`] body — never called directly.
    ///
    /// The unmasked image and the coverage are reused separately: a mask
    /// edit never re-runs the filter chain. Rendered pixels stay identical
    /// to the legacy baked path.
    ///
    /// If the entry is still valid ([`appearance_hit`](Self::appearance_hit)),
    /// the remembered appearance is re-served as is: no `bake`, no
    /// `preview`/`thumb` resample — same shared buffers.
    fn appearance_locked(&mut self, layer: &PixelLayer) -> Appearance {
        if let Some(cached) = self.appearance_hit(layer) {
            self.hits += 1;
            return cached;
        }
        let unmasked = self.unmasked_image(layer);
        let mask_signature = mask_signature(&layer.masks);
        let mask_cover = match self.entries.get_mut(&layer.id) {
            Some(entry) if entry.mask_signature == mask_signature => entry.mask_cover.clone(),
            Some(entry) => {
                let cover = Self::fresh_cover(&layer.masks);
                entry.mask_signature = mask_signature;
                entry.mask_cover = cover.clone();
                cover
            }
            // Unreachable in practice: `unmasked_image` creates the entry.
            None => Self::fresh_cover(&layer.masks),
        };
        let baked = Self::bake(&unmasked, &mask_cover);
        let appearance = Appearance {
            preview: crate::document::preview_buf(&baked),
            thumb: crate::document::thumb_buf(&baked),
            image: baked,
        };
        self.preview_rebuilds += 1;
        self.thumb_rebuilds += 1;
        if let Some(entry) = self.entries.get_mut(&layer.id) {
            entry.appearance = appearance.clone();
        }
        appearance
    }

    /// Image + thumbnail for the response frame (Phase 6G.2 §2), WITHOUT the
    /// `preview` buffer: `appearance.preview` is read by NO production path
    /// (verified: only Debug/tests touch it) while its full-size resample
    /// dominates the rebuild (≈ 1/3, measured).
    ///
    /// Validity STRICTLY identical to [`Self::appearance_hit`] (filters +
    /// masks + source identity): hit ⇒ same shared `Arc`s/buffers, shared
    /// `hits` counters. Miss ⇒ chain via [`Self::unmasked_image`] (shared
    /// `misses` counter) + coverage + bake + thumbnail (`thumb_rebuilds` not
    /// `preview_rebuilds`), entry updated with an EMPTY `preview` marker
    /// (documented, never read in production). Rendered pixels bit-identical
    /// to the full path.
    pub fn appearance_frame(&mut self, layer: &PixelLayer) -> Option<(Arc<DynamicImage>, RgbaBuf)> {
        crate::render_pool::run_parallel(|| self.appearance_frame_locked(layer))
    }

    /// [`Self::appearance_frame`] body — never called directly.
    fn appearance_frame_locked(
        &mut self,
        layer: &PixelLayer,
    ) -> Option<(Arc<DynamicImage>, RgbaBuf)> {
        if let Some(cached) = self.appearance_hit(layer) {
            self.hits += 1;
            return Some((cached.image, cached.thumb));
        }
        let unmasked = self.unmasked_image(layer);
        let mask_signature = mask_signature(&layer.masks);
        let mask_cover = match self.entries.get_mut(&layer.id) {
            Some(entry) if entry.mask_signature == mask_signature => entry.mask_cover.clone(),
            Some(entry) => {
                let cover = Self::fresh_cover(&layer.masks);
                entry.mask_signature = mask_signature;
                entry.mask_cover = cover.clone();
                cover
            }
            // Unreachable in practice: `unmasked_image` creates the entry.
            None => Self::fresh_cover(&layer.masks),
        };
        let baked = Self::bake(&unmasked, &mask_cover);
        let thumb = crate::document::thumb_buf(&baked);
        self.thumb_rebuilds += 1;
        if let Some(entry) = self.entries.get_mut(&layer.id) {
            entry.appearance.image = Arc::clone(&baked);
            entry.appearance.thumb = thumb.clone();
            // EMPTY marker: the full-size `preview` is not derived here (never read
            // in production — see module docs).
            entry.appearance.preview = RgbaBuf {
                width: 0,
                height: 0,
                data: Arc::from([]),
            };
        }
        Some((baked, thumb))
    }

    /// Image NOT masked by the LAYER's masks (source × filters; sub-layer
    /// masks stay baked into the chain since they apply before each filter's
    /// transform/blend).
    ///
    /// Painting a layer mask never re-runs the chain: the next call finds
    /// the same image (same `Arc`) while source and filters are unchanged.
    /// It is also the image a future shader path will combine with coverage
    /// at draw time.
    pub fn unmasked_image(&mut self, layer: &PixelLayer) -> Arc<DynamicImage> {
        let filter_signature = filters_signature(&layer.filter_layers);
        // perf-entry-api: use Entry to avoid double hashing on miss path
        use std::collections::hash_map::Entry;
        match self.entries.entry(layer.id) {
            Entry::Occupied(entry)
                if entry.get().filter_signature == filter_signature
                    && Arc::ptr_eq(&entry.get().source, &layer.source_image) =>
            {
                self.hits += 1;
                Arc::clone(&entry.get().unmasked)
            }
            Entry::Occupied(entry) => {
                self.misses += 1;
                let unmasked =
                    crate::filters::render_chain(&layer.source_image, &layer.filter_layers);
                let slot = entry.into_mut();
                slot.filter_signature = filter_signature;
                slot.source = Arc::clone(&layer.source_image);
                slot.unmasked = Arc::clone(&unmasked);
                unmasked
            }
            Entry::Vacant(entry) => {
                self.misses += 1;
                let unmasked =
                    crate::filters::render_chain(&layer.source_image, &layer.filter_layers);
                entry.insert(CacheEntry {
                    filter_signature,
                    source: Arc::clone(&layer.source_image),
                    unmasked: Arc::clone(&unmasked),
                    // Forced stale: the coverage is (re)computed by the caller
                    // (`appearance_locked`) right after.
                    mask_signature: mask_signature(&layer.masks).wrapping_add(1),
                    mask_cover: None,
                    appearance: Appearance {
                        preview: crate::document::preview_buf(&unmasked),
                        thumb: crate::document::thumb_buf(&unmasked),
                        image: Arc::clone(&unmasked),
                    },
                });
                unmasked
            }
        }
    }

    /// Combined coverage of the layer's ACTIVE masks (`None` = none),
    /// separately cacheable. Pure data meant to be sampled directly by a
    /// shader at draw time (no UI dependency here).
    pub fn mask_coverage(
        &mut self,
        layer: &PixelLayer,
    ) -> Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>> {
        let signature = mask_signature(&layer.masks);
        if let Some(entry) = self.entries.get_mut(&layer.id) {
            if entry.mask_signature == signature {
                return entry.mask_cover.clone();
            }
            let cover = Self::fresh_cover(&layer.masks);
            entry.mask_signature = signature;
            entry.mask_cover = cover.clone();
            return cover;
        }
        Self::fresh_cover(&layer.masks)
    }

    fn fresh_cover(masks: &[LayerMask]) -> Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>> {
        crate::document::compositing::combined_mask_coverage(masks).map(Arc::new)
    }

    /// Combines an unmasked image with a coverage: pixels identical to the
    /// old baked path (`apply_layer_masks`).
    fn bake(
        unmasked: &Arc<DynamicImage>,
        cover: &Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
    ) -> Arc<DynamicImage> {
        match cover {
            None => Arc::clone(unmasked),
            Some(cover) => {
                crate::document::compositing::apply_coverage(Arc::clone(unmasked), cover)
            }
        }
    }

    /// STRICT cache lookup — no execution, neither render pool nor
    /// `render_chain`. Returns the appearance only if the entry is still
    /// VALID (same filter signature, same source by Arc identity), `None`
    /// otherwise. Reserved for UI sync (`PreviewCache::sync`): during a
    /// gesture (drag, slider…) where no pixel changes, each message thus
    /// revalidates every layer without ever touching the pool — the "view
    /// reload" disappears.
    ///
    /// `None` is not an error: the caller must then fall back to
    /// [`Self::appearance`] (the only path running the chain).
    pub fn appearance_hit(&mut self, layer: &PixelLayer) -> Option<Appearance> {
        let filter_signature = filters_signature(&layer.filter_layers);
        let mask_signature = mask_signature(&layer.masks);
        match self.entries.get(&layer.id) {
            Some(e)
                if e.filter_signature == filter_signature
                    && e.mask_signature == mask_signature
                    && Arc::ptr_eq(&e.source, &layer.source_image) =>
            {
                Some(e.appearance.clone())
            }
            _ => None,
        }
    }

    /// Exports a layer's warm entry for inter-thread transfer (see
    /// [`WarmedAppearance`]). Returns `None` if the layer has no entry
    /// (never computed) — the caller must first go through [`Self::appearance`]
    /// on the warmed clone.
    pub fn export_warmed(&self, layer_id: Uuid) -> Option<WarmedAppearance> {
        self.entries.get(&layer_id).map(|e| WarmedAppearance {
            filter_signature: e.filter_signature,
            source: Arc::clone(&e.source),
            unmasked: Arc::clone(&e.unmasked),
            mask_signature: e.mask_signature,
            mask_cover: e.mask_cover.clone(),
            appearance: e.appearance.clone(),
        })
    }

    /// Inserts an entry pre-computed off the UI thread. Insertion is
    /// structurally safe: a future `appearance_hit` HITs only if signatures
    /// + source identity match — a stale entry (concurrent edit) simply
    /// goes unused, never wrong.
    pub fn insert_warmed(&mut self, layer_id: Uuid, warmed: WarmedAppearance) {
        self.entries.insert(
            layer_id,
            CacheEntry {
                filter_signature: warmed.filter_signature,
                source: warmed.source,
                unmasked: warmed.unmasked,
                mask_signature: warmed.mask_signature,
                mask_cover: warmed.mask_cover,
                appearance: warmed.appearance,
            },
        );
    }

    /// Pre-fills this cache with another cache's CURRENTLY warm entries.
    /// Used to warm a clone `Document` (cold cache) from the LIVE document
    /// (warm cache) before a background task: the clone's composite then
    /// HITs instead of re-running the full render chain.
    ///
    /// No recompute: `Appearance`s are cloned by `Arc` (zero pixel copy).
    /// Entries no longer valid (changed signature/source) are simply
    /// revalidated on demand and overwritten.
    pub fn import_from(&mut self, src: &Renderer) {
        for (id, e) in &src.entries {
            self.entries.insert(
                *id,
                CacheEntry {
                    filter_signature: e.filter_signature,
                    source: Arc::clone(&e.source),
                    unmasked: Arc::clone(&e.unmasked),
                    mask_signature: e.mask_signature,
                    mask_cover: e.mask_cover.clone(),
                    appearance: e.appearance.clone(),
                },
            );
        }
    }

    /// Invalidates ONLY the given layer's appearance (manually forced
    /// micro-edit; normally useless: the signature suffices).
    pub fn invalidate_layer(&mut self, layer_id: Uuid) {
        self.entries.remove(&layer_id);
    }

    /// Invalidates the whole cache (structural op, restore…).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Prunes entries of layers no longer in the tree. Call after a
    /// deletion/grouping/duplication or a structural undo — frees the
    /// VRAM/RAM of vanished subtrees.
    pub fn sync_tree(&mut self, doc: &Document) {
        let live: HashSet<Uuid> = doc.iter_pixels().into_iter().map(|l| l.id).collect();
        self.entries.retain(|id, _| live.contains(id));
    }

    /// Number of appearances currently cached.
    pub fn cached_len(&self) -> usize {
        self.entries.len()
    }

    /// Appearances served WITHOUT recompute since construction (or the last
    /// counter reset) — the direct targeted-cache indicator.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Recomputed appearances.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Regenerated `preview` buffers since construction (or the last reset):
    /// resample included, even at unchanged signatures before the
    /// [`appearance_hit`](Self::appearance_hit) fast path.
    pub fn preview_rebuilds(&self) -> u64 {
        self.preview_rebuilds
    }

    /// Regenerated `thumb` thumbnails.
    pub fn thumb_rebuilds(&self) -> u64 {
        self.thumb_rebuilds
    }

    /// Grouped counters (windowed observability).
    pub fn stats(&self) -> AppearanceStats {
        AppearanceStats {
            hits: self.hits,
            misses: self.misses,
            preview_rebuilds: self.preview_rebuilds,
            thumb_rebuilds: self.thumb_rebuilds,
        }
    }

    /// Resets counters to zero (windowed observability).
    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.preview_rebuilds = 0;
        self.thumb_rebuilds = 0;
    }
}

/// A mask list's signature: count, id, active state, inversion and version
/// (paint replaces the buffer AND touches the version → guaranteed MISS).
/// A mask's name is EXCLUDED (renaming changes no pixels). Used for the
/// separately-cacheable coverage. `pub(crate)`: reused by `tile_key`
/// (tile identity) without duplication.
pub(crate) fn mask_signature(masks: &[LayerMask]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write_usize(masks.len());
    for m in masks {
        h.write(m.id.as_bytes());
        h.write_u8(u8::from(m.enabled));
        h.write_u8(u8::from(m.inverted));
        h.write_u64(m.version);
    }
    h.finish()
}

/// ORDERED signature of a filter sub-layer chain: two chains share it
/// iff same sub-layers (id, type, active state, opacity, blend,
/// transform, masks) with same parameters in the same order. Order is
/// significant — this is a processing CHAIN,
/// not a set.
///
/// Sub-layer masks are PART of it: they are baked into the chain by
/// `composite_filter_layer`, so any edit (paint, toggle, inversion) must
/// invalidate the cache — the mask version ensures it. (The LAYER's
/// own masks are tracked separately
/// by [`mask_signature`].)
///
/// Deterministic across processes (`DefaultHasher::new()` = fixed keys),
/// independent of `HashMap`'s unordered iteration (sorted keys).
#[must_use]
pub fn filters_signature(filters: &[FilterLayer]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let mut h = DefaultHasher::new();
    h.write_usize(filters.len());
    for f in filters {
        h.write(f.id.as_bytes());
        h.write(f.type_id.as_bytes());
        h.write_u8(u8::from(f.enabled));
        h.write_u32(f.opacity.to_bits());
        h.write_u32(f.blend_mode.id());
        for v in [
            f.transform.offset_x,
            f.transform.offset_y,
            f.transform.rotation_deg,
            f.transform.scale_x,
            f.transform.scale_y,
            f.transform.skew_x,
            f.transform.skew_y,
        ] {
            h.write_u32(v.to_bits());
        }
        // Masks baked into appearance: id + state + version (paint replaces the
        // buffer AND touches the version → guaranteed MISS).
        h.write_usize(f.masks.len());
        for m in &f.masks {
            h.write(m.id.as_bytes());
            h.write_u8(u8::from(m.enabled));
            h.write_u8(u8::from(m.inverted));
            h.write_u64(m.version);
        }
        // Params sorted by key: same content => same signature
        let mut keys: Vec<&str> = Vec::with_capacity(f.params.len());
        keys.extend(f.params.keys().map(String::as_str));
        keys.sort_unstable();
        h.write_usize(keys.len());
        for key in keys {
            h.write(key.as_bytes());
            hash_param_value(&mut h, &f.params[key]);
        }
    }
    h.finish()
}

fn hash_param_value(
    h: &mut std::collections::hash_map::DefaultHasher,
    value: &datatypes::ParamValue,
) {
    use std::hash::Hasher;
    // Discriminant + raw bits: f32 excluded from std Hash (NaN), hash their
    // bits — two PartialEq-equal values share the same bits.
    match value {
        datatypes::ParamValue::Float(v) => {
            h.write_u8(0);
            h.write_u32(v.to_bits());
        }
        datatypes::ParamValue::Int(v) => {
            h.write_u8(1);
            h.write_i32(*v);
        }
        datatypes::ParamValue::Bool(v) => {
            h.write_u8(2);
            h.write_u8(u8::from(*v));
        }
        datatypes::ParamValue::Color(c) => {
            h.write_u8(3);
            for channel in c {
                h.write_u32(channel.to_bits());
            }
        }
        datatypes::ParamValue::Text(s) => {
            h.write_u8(4);
            h.write(s.as_bytes());
        }
        datatypes::ParamValue::Enum(s) => {
            h.write_u8(5);
            h.write(s.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datatypes::ParamValue;
    use image::ImageBuffer;
    use image::Rgba;

    fn solid(value: u8) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            2,
            2,
            Rgba([value, value, value, 255]),
        )))
    }

    fn layer_with_filter(brightness: f32) -> PixelLayer {
        let mut l = PixelLayer::new("test", solid(100));
        let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
        f.params
            .insert("brightness".to_string(), ParamValue::Float(brightness));
        l.filter_layers.push(f);
        l
    }

    #[test]
    fn premier_acces_miss_puis_hits_sans_recalcul() {
        let layer = layer_with_filter(10.0);
        let mut r = Renderer::default();

        let a1 = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 0));

        let a2 = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert_eq!(r.cached_len(), 1);
        // Same image Arc: no pixel copying
        assert!(Arc::ptr_eq(&a1.image, &a2.image));
    }

    #[test]
    fn seconde_lecture_ne_reconstruit_pas_preview_thumb() {
        // The appearance_hit fast path re-serves the remembered appearance: no
        // resample, unchanged rebuild counters.
        let layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let a1 = r.appearance(&layer);
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (1, 1));
        let a2 = r.appearance(&layer);
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (1, 1));
        assert_eq!((r.misses(), r.hits()), (1, 1));
        // Same shared buffers, no pixel clones.
        assert!(Arc::ptr_eq(&a1.image, &a2.image));
        assert!(Arc::ptr_eq(&a1.thumb.data, &a2.thumb.data));
        assert!(Arc::ptr_eq(&a1.preview.data, &a2.preview.data));
    }

    #[test]
    fn changement_de_masque_rebake_sans_reexecuter_la_chaine() {
        use crate::document::LayerMask;
        let mut layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let _ = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 0));
        // Adding a mask: chain spared (unmasked HIT), but coverage + bake +
        // preview/thumb to redo once.
        layer.masks.push(LayerMask::full(2, 2));
        let _ = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (2, 2));
        // Re-read: all stable, no rebuild.
        let _ = r.appearance(&layer);
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (2, 2));
    }

    #[test]
    fn changer_un_parametre_provoque_un_seul_nouveau_miss() {
        let mut layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let _ = r.appearance(&layer);

        // Slider setting: same filter id, new value
        let fid = layer.filter_layers[0].id;
        layer.filter_layers[0]
            .params
            .insert("brightness".to_string(), ParamValue::Float(20.0));
        let _ = r.appearance(&layer);
        assert_eq!(r.misses(), 2, "signature changée → recalcul");
        assert_eq!(r.hits(), 0);

        // Re-read with no change: back to HIT
        let _ = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (2, 1));
        let _ = fid;
    }

    #[test]
    fn remplacer_la_source_invalide_memes_sans_filtres() {
        // Empty chain: the signature never moves — only source identity can
        // detect a paint/crop.
        let mut layer = PixelLayer::new("peinture", solid(50));
        let mut r = Renderer::default();
        let avant = r.appearance(&layer);
        assert_eq!(r.misses(), 1);

        layer.set_source_image((*solid(90)).clone());
        let apres = r.appearance(&layer);
        assert_eq!(r.misses(), 2, "nouvel Arc source → recalcul garanti");
        assert!(!Arc::ptr_eq(&avant.image, &apres.image));
    }

    #[test]
    fn desactiver_un_filtre_change_la_signature() {
        let mut layer = layer_with_filter(30.0);
        let mut r = Renderer::default();
        let _ = r.appearance(&layer);

        layer.filter_layers[0].enabled = false;
        let _ = r.appearance(&layer);
        assert_eq!(r.misses(), 2);

        // And the result is pure source again (short-circuited filter)
        let out = r.appearance(&layer);
        let rgba = out.image.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        assert_eq!(p[0], 100);
    }

    #[test]
    fn cadre_partage_sans_preview() {
        // 6G.2 frame path: shared image + thumbnail, NO derived preview
        // (documented empty marker), honest counters.
        let layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let (img1, thumb1) = r.appearance_frame(&layer).expect("cadre");
        assert_eq!((r.misses(), r.hits()), (1, 0));
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (0, 1));
        let (img2, thumb2) = r.appearance_frame(&layer).expect("cadre");
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert!(Arc::ptr_eq(&img1, &img2), "image partagée");
        assert_eq!(
            (thumb1.width, thumb1.height),
            (thumb2.width, thumb2.height),
            "miniature stable"
        );
        // The full path stays consistent (HIT, shared image + thumbnail).
        let a = r.appearance(&layer);
        assert!(Arc::ptr_eq(&a.image, &img2));
        assert_eq!(
            (a.thumb.width, a.thumb.height),
            (thumb2.width, thumb2.height)
        );
        assert_eq!((a.preview.width, a.preview.height), (0, 0), "marqueur vide");
    }

    #[test]
    fn apparence_chauffee_inseree_hit_sans_recalcul() {
        // Simulates the async toggle: the worker computes on its own renderer,
        // the live one inserts the entry and HITs without running.
        let mut layer = layer_with_filter(30.0);
        let mut worker = Renderer::default();
        let _ = worker.appearance(&layer);

        // Worker-side toggle (filter disabling).
        layer.filter_layers[0].enabled = false;
        let _ = worker.appearance(&layer);
        let warmed = worker.export_warmed(layer.id).expect("entrée chaude");

        // Live: cold cache, insertion then strict HIT with no MISS.
        let mut live = Renderer::default();
        live.insert_warmed(layer.id, warmed);
        let hit = live.appearance_hit(&layer);
        assert!(hit.is_some(), "l'entrée insérée doit HITER");
        assert_eq!(
            (live.misses(), live.hits()),
            (0, 0),
            "aucune exécution côté vivant"
        );

        // Stale entry (concurrent edit): no false HIT.
        layer.filter_layers[0].enabled = true;
        assert!(live.appearance_hit(&layer).is_none());
    }

    #[test]
    fn toggle_masque_sans_touch_hit_apres_insertion() {
        // Mirror of `apply_toggle_flag`: worker and live flip the same bit
        // WITHOUT `touch()` → same version → same signature → HIT.
        let mut layer = PixelLayer::new("m", solid(100));
        layer.masks.push(LayerMask::full(2, 2));
        // Live = structural clone (shared Arcs, copied version), like a
        // document snapshot.
        let mut live = layer.clone();
        let mut worker = Renderer::default();
        let _ = worker.appearance(&layer);

        layer.masks[0].enabled = false;
        let _ = worker.appearance(&layer);
        let warmed = worker.export_warmed(layer.id).expect("entrée chaude");

        live.masks[0].enabled = false;
        let mut live_r = Renderer::default();
        live_r.insert_warmed(live.id, warmed);
        assert!(
            live_r.appearance_hit(&live).is_some(),
            "même version des deux côtés → HIT"
        );

        // Control: an interleaved `touch()` (paint) diverges → healthy MISS,
        // never a false HIT on stale pixels.
        live.masks[0].touch();
        assert!(live_r.appearance_hit(&live).is_none());
    }

    #[test]
    fn sync_tree_elague_les_calques_supprimes() {
        let mut doc = Document::new(4, 4);
        let l1 = PixelLayer::new("a", solid(1));
        let l2 = PixelLayer::new("b", solid(2));
        let id1 = l1.id;
        let _ = l2.id;
        doc.push_layer(crate::document::LayerNode::Pixel(l1));
        doc.push_layer(crate::document::LayerNode::Pixel(l2));

        let mut r = Renderer::default();
        for l in doc.iter_pixels() {
            let _ = r.appearance(l);
        }
        assert_eq!(r.cached_len(), 2);

        // Layer 1 removal then pruning
        let _removed = doc.remove(id1);
        r.sync_tree(&doc);
        assert_eq!(r.cached_len(), 1, "entrée orpheline supprimée");
    }

    #[test]
    fn deux_calques_sont_cachees_independamment() {
        // The heart of the LayerTree promise: touching layer A NEVER
        // re-runs layer B.
        let la = layer_with_filter(5.0);
        let lb = layer_with_filter(7.0);
        let mut r = Renderer::default();

        let _ = r.appearance(&la);
        let _ = r.appearance(&lb);
        assert_eq!((r.misses(), r.hits()), (2, 0));

        let mut la_mod = la.clone();
        if let Some(f) = la_mod.filter_layers.first_mut() {
            f.params
                .insert("brightness".to_string(), ParamValue::Float(6.0));
        }
        let _ = r.appearance(&la_mod);
        let _ = r.appearance(&lb);
        // lb generated NO new miss
        assert_eq!((r.misses(), r.hits()), (3, 1));

        // Signature tests (pure part)
        let s = filters_signature(&la.filter_layers);
        assert_eq!(s, filters_signature(&la.filter_layers));
    }

    #[test]
    fn ordre_et_etat_actif_comptent_dans_la_signature() {
        let mut a = vec![
            filter_of("blur", 3.0),
            filter_of("brightness_contrast", 12.0),
        ];
        let b = vec![
            filter_of("brightness_contrast", 12.0),
            filter_of("blur", 3.0),
        ];
        assert_ne!(filters_signature(&a), filters_signature(&b));

        a[0].enabled = false;
        let disabled_first = vec![a[0].clone(), a[1].clone()];
        let both_on = vec![
            filter_of("blur", 3.0),
            filter_of("brightness_contrast", 12.0),
        ];
        assert_ne!(
            filters_signature(&disabled_first),
            filters_signature(&both_on)
        );
    }

    fn filter_of(name: &str, brightness: f32) -> FilterLayer {
        let mut f = FilterLayer::neutral(name, Default::default());
        f.params
            .insert("brightness".to_string(), ParamValue::Float(brightness));
        f
    }

    #[test]
    fn opacite_et_fusion_comptent_dans_la_signature() {
        let base = vec![filter_of("brightness_contrast", 12.0)];
        let mut attenuated = base.clone();
        attenuated[0].opacity = 50.0;
        assert_ne!(filters_signature(&base), filters_signature(&attenuated));

        let mut multiplied = base.clone();
        multiplied[0].blend_mode = crate::document::BlendMode::Multiply;
        assert_ne!(filters_signature(&base), filters_signature(&multiplied));
    }

    #[test]
    fn masque_sous_calque_invalide_le_cache() {
        use crate::document::LayerMask;
        let base = vec![filter_of("brightness_contrast", 12.0)];
        // Adding a mask → different signature
        let mut masked = base.clone();
        masked[0].masks.push(LayerMask::full(2, 2));
        assert_ne!(filters_signature(&base), filters_signature(&masked));

        // Paint (touch = new version) → different signature
        let mut touched = masked.clone();
        touched[0].masks[0].touch();
        assert_ne!(filters_signature(&masked), filters_signature(&touched));
    }

    #[test]
    fn couverture_masque_combinee_doree() {
        use crate::document::LayerMask;
        use crate::document::compositing::combined_mask_coverage;
        use image::ImageBuffer;

        // No active mask → no coverage.
        let mut vide = LayerMask::full(2, 2);
        vide.enabled = false;
        assert!(combined_mask_coverage(&[vide]).is_none());
        assert!(combined_mask_coverage(&[]).is_none());

        // 128 × inverted(64→191) = 128*191/255 = 95 (truncated).
        let mut a = LayerMask::full(2, 2);
        a.image = Arc::new(ImageBuffer::from_pixel(2, 2, Rgba([128, 0, 0, 255])));
        let mut b = LayerMask::full(2, 2);
        b.image = Arc::new(ImageBuffer::from_pixel(2, 2, Rgba([64, 0, 0, 255])));
        b.inverted = true;
        let cover = combined_mask_coverage(&[a, b]).expect("couverture");
        assert_eq!(cover.dimensions(), (2, 2));
        assert_eq!(cover.get_pixel(0, 0)[0], 95);
        assert_eq!(cover.get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn peinture_masque_ne_reexecute_pas_la_chaine() {
        use crate::document::LayerMask;
        let mut layer = layer_with_filter(10.0);
        layer.masks.push(LayerMask::full(2, 2));
        let mut r = Renderer::default();

        let u1 = r.unmasked_image(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 0));

        // Mask paint: only the coverage changes, the chain does NOT.
        layer.masks[0].touch();
        let u2 = r.unmasked_image(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert!(Arc::ptr_eq(&u1, &u2), "chaîne non ré-exécutée");

        // The coverage itself is properly recomputed and cached.
        let c1 = r.mask_coverage(&layer).expect("couverture");
        let c2 = r.mask_coverage(&layer).expect("couverture");
        assert!(Arc::ptr_eq(&c1, &c2));

        // The baked appearance stays correct: source 100 + filter 10,
        // full white mask → alpha 255.
        let a = r.appearance(&layer);
        let rgba = a.image.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0)[3], 255);
    }
}
