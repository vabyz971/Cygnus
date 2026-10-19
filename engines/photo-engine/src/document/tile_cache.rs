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

//! Document tile cache (Phase 6D): remember the final result R.
//!
//! Strict separation (central 6D rule):
//!
//! - `SpatialScope` / `ScopeWindow` (Phase 6C, `compositing.rs`): to
//!   produce R, which region D to compute;
//! - `DirtyRegion` (`dirty.rs`): which zones are modified;
//! - [`TileCache`] (here): may I reuse the final result of R? The cache
//!
//! computes NO spatial dependency and knows neither blur nor blend: it
//! stores [`RegionalComposite`] (the cropped R, never the dependency
//! window D) and decides hit/miss by key + dirty + guard.
//!
//! Key ([`DocTileKey`]): requested rect + backend + halo + flags. NO
//! global revision as validity: per Phase 6D §6, validity is determined
//! by dirty state; [`TileContentSignature`] is kept INSIDE the entry
//! as metadata (traceability + extension point), refreshed on every
//! hit.
//!
//! Accepted consequence (§17): after a mutation, clean tiles stay hits
//! — recomputation is limited to dirty zones. In return, EVERY mutation
//! must go through `mark_dirty*` with an expansion covering its real
//! dependencies (6C halos + appearance-chain spread): a forgotten or
//! underestimated mark would re-serve stale data. When in doubt:
//! `mark_global` (safe over-invalidation, never wrong pixels).
//!
//! Memory budget in pixels (not tile count) with LRU eviction: after
//! each completed insertion, `cached_pixels <= max_pixels`. A lone tile
//! bigger than the budget is not stored (rather than exceeding the
//! budget).

use std::collections::HashMap;
use std::sync::Arc;

use image::DynamicImage;
use tiles::{Padding, TileRegion};
use uuid::Uuid;

use super::compositing::{CompositeStats, ScopeWindow};
use super::dirty::DirtyRegion;
use super::tree::{Document, RegionalComposite};
use crate::tile_key::{BackendTag, TileContentSignature};

/// Render path flags (no option for now — any future pixel-affecting
/// option MUST appear here to avoid false hits).
pub const TILE_CACHE_FLAGS_NONE: u32 = 0;

/// Cache key: identity of the final result R. The rect is the
///
/// REQUESTED region in DOCUMENT SPACE (integer): at equal content, the
/// buffer projection is identical, so same rect ⇒ same pixels (when
/// clean). The `halo` is the one of the dependency window computed by
/// `scope_adjustment_window` — deterministic at equal content, redundant
/// for safety.
///
/// The render scale (`scale`, Phase 6F §14) is part of identity: a tile
/// computed at one scale is never served for another (conservative
/// miss, never a false hit). Rendering is 1.0-scale today (state-only
/// display zoom); the field reserves partitioning for future mipmaps
/// without implementing any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocTileKey {
    /// Requested region R (DOCUMENT SPACE).
    pub rect: TileRegion,
    /// Backend that produced the pixels (CPU/GPU diverge).
    pub backend: BackendTag,
    /// Dependency halo served to the render (6C, deterministic at equal content).
    pub halo: u32,
    /// Render options.
    pub flags: u32,
    /// Render scale, `f32` bits (1.0 today).
    pub scale_bits: u32,
}

impl DocTileKey {
    /// New key (non-finite `scale` or ≤ 0 ⇒ 1.0, never NaN in keys).
    #[must_use]
    pub fn new(rect: TileRegion, backend: BackendTag, halo: u32, flags: u32, scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        Self {
            rect,
            backend,
            halo,
            flags,
            scale_bits: scale.to_bits(),
        }
    }

    /// Render scale.
    #[must_use]
    pub fn scale(self) -> f32 {
        f32::from_bits(self.scale_bits)
    }
}

/// Entry: tile R + content metadata + LRU recency. Validity comes from
///
/// dirty (see [`TileCache::get`]); `content` is metadata refreshed on
/// every hit, never a miss cause.
#[derive(Debug, Clone)]
struct CacheEntry {
    tile: RegionalComposite,
    content: TileContentSignature,
    pixels: usize,
    last_used: u64,
}

/// Cache counters (observability, no render effect).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TileCacheStats {
    /// Reads re-served without recompute.
    pub hits: u64,
    /// Reads recomputed (absent, dirty or stale guard).
    pub misses: u64,
    /// Entries evicted by the budget (LRU).
    pub evictions: u64,
    /// Stored pixels.
    pub cached_pixels: usize,
}

impl TileCacheStats {
    /// Recomputes avoided via hits (== `hits`, dedicated field for future
    /// benchmark matrices).
    #[must_use]
    pub fn renders_avoided(self) -> u64 {
        self.hits
    }
}

/// Bounded (in pixels) LRU cache of document tiles, dirty-aware. Its
///
/// hit `Clone` copies pixels (`DynamicImage`): acceptable for the 6D
/// foundation (correctness first); `Arc` sharing will come if
/// measurements require it — no premature optimization.
#[derive(Debug)]
pub struct TileCache {
    max_pixels: usize,
    entries: HashMap<DocTileKey, CacheEntry>,
    dirty: DirtyRegion,
    /// Eviction-protected zones (visible viewport, Phase 6F): LRU eviction
    /// skips entries intersecting them. Call scope (set for one render,
    /// removed after) — never permanent state.
    protected: Vec<TileRegion>,
    clock: u64,
    current_pixels: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl TileCache {
    /// New cache (`max_pixels` = pixel budget; 0 = stores nothing, reads
    /// stay misses).
    #[must_use]
    pub fn new(max_pixels: usize) -> Self {
        Self {
            max_pixels,
            entries: HashMap::new(),
            dirty: DirtyRegion::new(),
            protected: Vec::new(),
            clock: 0,
            current_pixels: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Protects zones from eviction (visible viewport) until the next
    /// [`TileCache::clear_protected`]. Eviction prefers entries outside the
    /// zones; if EVERYTHING is protected, strict LRU fallback (budget wins —
    /// never exceeded). Changes neither hits nor validity.
    pub fn set_protected(&mut self, rects: &[TileRegion]) {
        self.protected.clear();
        self.protected
            .extend(rects.iter().copied().filter(|r| !r.is_empty()));
    }

    /// Removes the eviction protection.
    pub fn clear_protected(&mut self) {
        self.protected.clear();
    }

    /// Protected zones (diagnostics).
    #[must_use]
    pub fn protected(&self) -> &[TileRegion] {
        &self.protected
    }

    /// True if the tile intersects a protected zone.
    fn is_protected(&self, tile: &RegionalComposite) -> bool {
        let bounds = tile.tile_bounds();
        self.protected
            .iter()
            .any(|p| !p.intersect(bounds).is_empty())
    }

    /// Configured budget (pixels).
    #[must_use]
    pub fn max_pixels(&self) -> usize {
        self.max_pixels
    }

    /// Stored entries (clean or stale awaiting eviction).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Stored pixels (always ≤ budget after a completed insertion).
    #[must_use]
    pub fn cached_pixels(&self) -> usize {
        self.current_pixels
    }

    /// Dirty state (which zones must be rebuilt).
    #[must_use]
    pub fn dirty(&self) -> &DirtyRegion {
        &self.dirty
    }

    /// Content metadata stored for the key (`None` if absent) — refresh
    /// diagnostics, no validity effect.
    #[must_use]
    pub fn cached_content(&self, key: DocTileKey) -> Option<TileContentSignature> {
        self.entries.get(&key).map(|e| e.content)
    }

    /// Cached tiles whose pixels (buffer bounds) intersect `rect` at `scale`
    /// (Phase 6G): repopulate a fresh surface WITHOUT recompositing (zero
    /// computation, only clones). Intersection uses rendered pixels (`tile_bounds`,
    /// buffer-space), not keys (document-space): both spaces match at zero
    /// origin but diverge on the infinite plane. Dirty is NOT consulted
    /// here: the caller (`RenderWorker`) only repopulates after establishing
    /// validity.
    #[must_use]
    pub fn tiles_for_surface(&self, rect: TileRegion, scale: f32) -> Vec<RegionalComposite> {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let bits = scale.to_bits();
        let mut out: Vec<(u64, RegionalComposite)> = Vec::new();
        for (key, entry) in &self.entries {
            if key.scale_bits != bits {
                continue;
            }
            if entry.tile.tile_bounds().intersect(rect).is_empty() {
                continue;
            }
            out.push((entry.last_used, entry.tile.clone()));
        }
        // LRU order (newest first): deterministic; residual overlaps go to
        // the freshest (same bytes anyway).
        out.sort_by_key(|(used, _)| u64::MAX - used);
        out.into_iter().map(|(_, tile)| tile).collect()
    }

    /// Cumulative statistics.
    #[must_use]
    pub fn stats(&self) -> TileCacheStats {
        TileCacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            cached_pixels: self.current_pixels,
        }
    }

    /// Read: hit iff the entry exists AND its rect is clean (`!dirty.intersects`)
    /// — validity is determined by dirty state (Phase 6D §6), NOT by the
    /// content signature. Marks LRU recency and refreshes the content
    /// metadata.
    ///
    /// Usage discipline (correctness): every mutation MUST be marked
    /// (`mark_dirty*`, expansion covering real dependencies) BEFORE any
    /// re-read, else stale data would be re-served.
    #[must_use]
    pub fn get(
        &mut self,
        key: DocTileKey,
        content: TileContentSignature,
    ) -> Option<RegionalComposite> {
        self.clock = self.clock.wrapping_add(1);
        let tick = self.clock;
        let dirty_hit = match self.entries.get(&key) {
            None => {
                self.misses += 1;
                return None;
            }
            Some(entry) => self.dirty.intersects(entry.tile.tile_bounds()),
        };
        if dirty_hit {
            self.misses += 1;
            return None;
        }
        let entry = self.entries.get_mut(&key).expect("présent ci-dessus");
        entry.content = content;
        entry.last_used = tick;
        self.hits += 1;
        Some(entry.tile.clone())
    }

    /// Stores the final result R. Returns false (without storing) if the
    /// lone tile exceeds the budget — the budget is never exceeded after a
    /// completed insertion. Replaces the same-key entry without counting an
    /// eviction; each LRU victim counts one eviction.
    pub fn insert(
        &mut self,
        key: DocTileKey,
        tile: RegionalComposite,
        content: TileContentSignature,
    ) -> bool {
        let pixels = tile.pixels();
        if pixels > self.max_pixels {
            return false;
        }
        self.clock = self.clock.wrapping_add(1);
        let tick = self.clock;
        if let Some(old) = self.entries.remove(&key) {
            self.current_pixels = self.current_pixels.saturating_sub(old.pixels);
        }
        while self.current_pixels + pixels > self.max_pixels {
            // LRU victim outside protected zones; if all is protected, strict
            // LRU fallback (the budget is non-negotiable).
            let victim = self
                .entries
                .iter()
                .filter(|(_, e)| !self.is_protected(&e.tile))
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| *k)
                .or_else(|| {
                    self.entries
                        .iter()
                        .min_by_key(|(_, e)| e.last_used)
                        .map(|(k, _)| *k)
                });
            let Some(victim) = victim else {
                break;
            };
            if let Some(old) = self.entries.remove(&victim) {
                self.current_pixels = self.current_pixels.saturating_sub(old.pixels);
                self.evictions += 1;
            }
        }
        self.entries.insert(
            key,
            CacheEntry {
                tile,
                content,
                pixels,
                last_used: tick,
            },
        );
        self.current_pixels += pixels;
        true
    }

    /// Marks a modified zone (LOCAL dependency: only tiles intersecting the
    /// zone are concerned).
    pub fn mark_dirty(&mut self, region: TileRegion) {
        self.dirty.add(region);
    }

    /// Marks a modified zone with neighborhood dependency: the invalidation
    /// zone is grown by the halo (DOCUMENT SPACE, same values as
    /// `blur_support_px` passed by the caller via `ScopeWindow` — no halo
    /// (re)definition here).
    pub fn mark_dirty_halo(&mut self, region: TileRegion, halo_px: u32) {
        self.dirty.add(region.pad(Padding::new(halo_px)));
    }

    /// Marks per a 6C window: LOCAL ⇒ zone alone, NEIGHBORHOOD ⇒ zone +
    /// halo, GLOBAL ⇒ whole document (all tiles concerned).
    pub fn mark_dirty_scope(
        &mut self,
        region: TileRegion,
        window: ScopeWindow,
        doc_w: u32,
        doc_h: u32,
    ) {
        if window.global {
            self.mark_global(doc_w, doc_h);
        } else if window.halo_px > 0 {
            self.mark_dirty_halo(region, window.halo_px);
        } else {
            self.mark_dirty(region);
        }
    }

    /// Global invalidation: the whole document is dirty (GLOBAL op). Lazy
    /// (no entry removed): gets will miss via dirty until the next
    /// `clear_dirty` — never wrong pixels.
    pub fn mark_global(&mut self, doc_w: u32, doc_h: u32) {
        self.dirty
            .add(TileRegion::new(0, 0, doc_w.max(1), doc_h.max(1)));
    }

    /// Forgets dirty zones (after re-render + presentation of affected
    /// tiles). Usage contract, not a guess: without `clear`, requests stay
    /// dirty (conservative re-render).
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Settles EXACTLY the rendered zones (Phase 6G.2 §1): call after result
    /// production + application, never before. Returns the number of fully
    /// settled zones. Partially rendered zones are shrunk (exact subtraction),
    /// never erased.
    pub fn consume_rendered(&mut self, rendered: &[TileRegion]) -> u64 {
        self.dirty.consume(rendered)
    }

    /// Empties the cache and forgets dirty (structural change, resize…).
    /// Returns the number of removed entries.
    pub fn invalidate_all(&mut self) -> usize {
        let n = self.entries.len();
        self.entries.clear();
        self.current_pixels = 0;
        self.dirty.clear();
        self.protected.clear();
        n
    }
}

impl RegionalComposite {
    /// Tile buffer bounds (for the dirty test).
    fn tile_bounds(&self) -> TileRegion {
        TileRegion::new(
            self.origin_x as i32,
            self.origin_y as i32,
            self.image.width(),
            self.image.height(),
        )
    }

    /// Stored pixels (budget).
    fn pixels(&self) -> usize {
        self.image.width() as usize * self.image.height() as usize
    }
}

/// Renders R through the cache: hit ⇒ re-served tile; miss ⇒
/// `composite_region_with(R)` (6C windowed: `D = R + halo` or global
/// fallback), insertion of the cropped R, return. `content` =
///
/// `tile_content_signature(doc)` (6A guard); `backend` = real backend
/// (CPU/GPU diverge); `scale` = render scale (1.0: display zoom stays
/// state-only, the field partitions for future mipmaps); `render_stats`
/// receives the render cost on a miss (nothing measured on a hit —
/// no computation).
///
/// `None` (empty/out-of-frame region, nothing to contribute) is
/// counted as nothing but a (read) miss, and not inserted.
#[allow(clippy::too_many_arguments)]
pub fn get_or_render(
    cache: &mut TileCache,
    doc: &Document,
    resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    render_stats: &mut CompositeStats,
    region: &TileRegion,
    backend: BackendTag,
    flags: u32,
    scale: f32,
    content: TileContentSignature,
) -> Option<RegionalComposite> {
    if region.is_empty() {
        return None;
    }
    // Deterministic halo at equal content (6C): same key on every request.
    let halo = super::compositing::scope_adjustment_window(&doc.root).halo_px;
    let key = DocTileKey::new(*region, backend, halo, flags, scale);
    if let Some(tile) = cache.get(key, content) {
        return Some(tile);
    }
    let rendered = doc.composite_region_with(resolve, render_stats, region)?;
    cache.insert(key, rendered.clone(), content);
    Some(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile_key::TILE_FLAGS_NONE;
    use image::{ImageBuffer, Rgba};

    fn tile_at(x: u32, y: u32, w: u32, h: u32, v: u8) -> RegionalComposite {
        RegionalComposite {
            image: DynamicImage::ImageRgba8(ImageBuffer::from_pixel(w, h, Rgba([v, v, v, 255]))),
            origin_x: x,
            origin_y: y,
        }
    }

    fn key_at(x: i32, y: i32, s: u32) -> DocTileKey {
        DocTileKey::new(
            TileRegion::new(x, y, s, s),
            BackendTag::cpu(),
            0,
            TILE_FLAGS_NONE,
            1.0,
        )
    }

    const C1: TileContentSignature = TileContentSignature(1);
    const C2: TileContentSignature = TileContentSignature(2);

    #[test]
    fn miss_puis_hit() {
        let mut cache = TileCache::new(1 << 20);
        let key = key_at(0, 0, 64);
        assert!(cache.get(key, C1).is_none());
        assert!(cache.insert(key, tile_at(0, 0, 64, 64, 7), C1));
        let hit = cache.get(key, C1).expect("hit");
        assert_eq!(hit.image.to_rgba8().get_pixel(0, 0)[0], 7);
        assert_eq!(cache.stats().renders_avoided(), 1);
        assert_eq!((cache.stats().hits, cache.stats().misses), (1, 1));
    }

    #[test]
    fn contenu_nouveau_rafraichit_si_propre() {
        // §6: validity comes from dirty, not the revision. Changed content +
        // clean tile ⇒ hit (partial recompute possible, §17), refreshed
        // metadata.
        let mut cache = TileCache::new(1 << 20);
        let key = key_at(0, 0, 64);
        assert!(cache.insert(key, tile_at(0, 0, 64, 64, 7), C1));
        assert!(cache.get(key, C2).is_some(), "propre ⇒ hit malgré C1→C2");
        assert_eq!(cache.cached_content(key), Some(C2), "métadonnée rafraîchie");
        assert!(cache.get(key, C1).is_some(), "pas de miss");
        assert_eq!((cache.stats().hits, cache.stats().misses), (2, 0));
    }

    #[test]
    fn sale_prime_sur_contenu() {
        let mut cache = TileCache::new(1 << 20);
        let key = key_at(0, 0, 64);
        assert!(cache.insert(key, tile_at(0, 0, 64, 64, 7), C1));
        cache.mark_dirty(TileRegion::new(0, 0, 64, 64));
        assert!(cache.get(key, C1).is_none(), "sale ⇒ miss à contenu égal");
        assert!(cache.get(key, C2).is_none(), "sale ⇒ miss à contenu changé");
        cache.clear_dirty();
        assert!(cache.get(key, C2).is_some(), "après clear ⇒ hit");
    }

    #[test]
    fn backends_distincts() {
        let mut cache = TileCache::new(1 << 20);
        let cpu = key_at(0, 0, 64);
        let mut gpu = cpu;
        gpu.backend = BackendTag::gpu();
        assert!(cache.insert(cpu, tile_at(0, 0, 64, 64, 1), C1));
        assert!(cache.get(gpu, C1).is_none(), "jamais de hit croisé CPU/GPU");
        assert!(cache.get(cpu, C1).is_some());
    }

    #[test]
    fn lru_evict_le_moins_recent() {
        // Budget: two 64² tiles.
        let mut cache = TileCache::new(2 * 64 * 64);
        let (a, b, c) = (key_at(0, 0, 64), key_at(64, 0, 64), key_at(128, 0, 64));
        assert!(cache.insert(a, tile_at(0, 0, 64, 64, 1), C1));
        assert!(cache.insert(b, tile_at(64, 0, 64, 64, 2), C1));
        assert!(cache.get(a, C1).is_some(), "A rafraîchi");
        assert!(cache.insert(c, tile_at(128, 0, 64, 64, 3), C1));
        assert_eq!(cache.stats().evictions, 1);
        assert_eq!(cache.cached_pixels(), 2 * 64 * 64, "budget tenu");
        assert!(cache.get(b, C1).is_none(), "B (LRU) évincé");
        assert!(cache.get(a, C1).is_some());
        assert!(cache.get(c, C1).is_some());
    }

    #[test]
    fn tuile_surdimensionnee_refusee_budget_tenu() {
        let mut cache = TileCache::new(64 * 64);
        assert!(!cache.insert(key_at(0, 0, 128), tile_at(0, 0, 128, 128, 1), C1));
        assert!(cache.is_empty());
        assert_eq!(cache.cached_pixels(), 0);
    }

    #[test]
    fn protection_viewport_preferee() {
        // 2-tile budget: the protected one survives, others fall to LRU.
        let mut cache = TileCache::new(2 * 64 * 64);
        let (a, b, c) = (key_at(0, 0, 64), key_at(64, 0, 64), key_at(128, 0, 64));
        assert!(cache.insert(a, tile_at(0, 0, 64, 64, 1), C1));
        assert!(cache.insert(b, tile_at(64, 0, 64, 64, 2), C1));
        cache.set_protected(&[TileRegion::new(0, 0, 64, 64)]);
        assert!(cache.insert(c, tile_at(128, 0, 64, 64, 3), C1));
        assert_eq!(cache.stats().evictions, 1);
        assert!(cache.get(a, C1).is_some(), "protégée conservée");
        assert!(cache.get(b, C1).is_none(), "non protégée évincée");
        assert!(cache.get(c, C1).is_some());
        // All protected: strict LRU fallback (budget wins).
        cache.set_protected(&[TileRegion::new(0, 0, 512, 512)]);
        let d = key_at(192, 0, 64);
        assert!(cache.insert(d, tile_at(192, 0, 64, 64, 4), C1));
        assert_eq!(cache.cached_pixels(), 2 * 64 * 64, "budget tenu quand même");
        cache.clear_protected();
        assert!(cache.protected().is_empty());
    }

    #[test]
    fn echelles_distinctes() {
        // Same rect, scales 1.0 vs 2.0: never a cross hit; back to 1.0 ⇒
        // hit (kept). Same pixels (1.0 render, no 6F mipmaps).
        let mut cache = TileCache::new(1 << 20);
        let r = TileRegion::new(0, 0, 64, 64);
        let k1 = DocTileKey::new(r, BackendTag::cpu(), 0, TILE_FLAGS_NONE, 1.0);
        let k2 = DocTileKey::new(r, BackendTag::cpu(), 0, TILE_FLAGS_NONE, 2.0);
        assert_ne!(k1, k2);
        assert_eq!(k2.scale(), 2.0);
        assert_eq!(
            DocTileKey::new(r, BackendTag::cpu(), 0, TILE_FLAGS_NONE, f32::NAN).scale(),
            1.0,
            "NaN normalisé"
        );
        assert!(cache.insert(k1, tile_at(0, 0, 64, 64, 9), C1));
        assert!(cache.get(k2, C1).is_none(), "échelle distincte ⇒ miss");
        assert!(cache.get(k1, C1).is_some(), "échelle d'origine ⇒ hit");
        assert_eq!((cache.stats().hits, cache.stats().misses), (1, 1));
    }

    #[test]
    fn budget_zero_ne_stocke_rien() {
        let mut cache = TileCache::new(0);
        assert!(!cache.insert(key_at(0, 0, 64), tile_at(0, 0, 64, 64, 1), C1));
        assert!(cache.get(key_at(0, 0, 64), C1).is_none());
        assert_eq!((cache.stats().hits, cache.stats().misses), (0, 1));
    }
}
