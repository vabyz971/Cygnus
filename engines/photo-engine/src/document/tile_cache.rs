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

//! Cache de tuiles document (Phase 6D) : mémoriser le résultat final R.
//!
//! Séparation stricte (règle centrale 6D) :
//!
//! - `SpatialScope` / `ScopeWindow` (Phase 6C, `compositing.rs`) : pour
//!   produire R, quelle région D calculer ;
//! - `DirtyRegion` (`dirty.rs`) : quelles zones sont modifiées ;
//! - [`TileCache`] (ici) : puis-je réutiliser le résultat final de R ?
//!
//! Le cache ne calcule AUCUNE dépendance spatiale et ne connaît ni
//! blur ni blend : il stocke des [`RegionalComposite`] (le R rogné, jamais
//! la fenêtre de dépendance D) et décide hit/miss par clé + dirty + garde.
//!
//! Clé ([`DocTileKey`]) : rect demandé + backend + halo + flags. PAS de
//! révision globale comme validité : conformément à la Phase 6D §6, la
//! validité est déterminée par l'état dirty ; la [`TileContentSignature`]
//! est conservée DANS l'entrée comme métadonnée (traçabilité + point
//! d'extension), rafraîchie à chaque hit.
//!
//! Conséquence assumée (§17) : après une mutation, les tuiles propres
//! restent des hits — le recalcul se limite aux zones sales. En contrepartie,
//! TOUTE mutation doit passer par `mark_dirty*` avec une expansion couvrant
//! ses dépendances réelles (halos 6C + diffusion des chaînes d'apparence) :
//! une marque oubliée ou sous-évaluée resservirait du périmé. En cas de
//! doute : `mark_global` (sur-invalidation sûre, jamais de pixel faux).
//!
//! Budget mémoire en pixels (pas en nombre de tuiles) avec éviction LRU :
//! après chaque insertion terminée, `cached_pixels <= max_pixels`. Une tuile
//! seule plus grosse que le budget n'est pas stockée (plutôt que de
//! dépasser le budget).

use std::collections::HashMap;
use std::sync::Arc;

use image::DynamicImage;
use tiles::{Padding, TileRegion};
use uuid::Uuid;

use super::compositing::{CompositeStats, ScopeWindow};
use super::dirty::DirtyRegion;
use super::tree::{Document, RegionalComposite};
use crate::tile_key::{BackendTag, TileContentSignature};

/// Flags de chemin de rendu (aucune option pour l'instant — toute option
/// pixellisée future DOIT y figurer pour éviter les faux hits).
pub const TILE_CACHE_FLAGS_NONE: u32 = 0;

/// Clé de cache : identité du résultat final R.
///
/// Le rect est la région DEMANDÉE en DOCUMENT SPACE (entière) : à contenu
/// égal, la projection buffer est identique, donc même rect ⇒ mêmes pixels
/// (si propre). Le `halo` est celui de la fenêtre de dépendance calculée
/// par `scope_adjustment_window` — déterministe à contenu égal, redondant
/// par sécurité.
///
/// L'échelle de rendu (`scale`, Phase 6F §14) fait partie de l'identité :
/// une tuile calculée à une échelle n'est jamais servie pour une autre
/// (miss conservateur, jamais de faux hit). Le rendu est aujourd'hui en
/// échelle 1.0 (zoom d'affichage state-only) ; le champ réserve le
/// partitionnement pour de futurs mipmaps, sans en implémenter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocTileKey {
    /// Région demandée R (DOCUMENT SPACE).
    pub rect: TileRegion,
    /// Backend ayant produit les pixels (CPU/GPU divergent).
    pub backend: BackendTag,
    /// Halo de dépendance servi au rendu (6C, déterminsite à contenu égal).
    pub halo: u32,
    /// Options de rendu.
    pub flags: u32,
    /// Échelle de rendu, bits `f32` (1.0 aujourd'hui).
    pub scale_bits: u32,
}

impl DocTileKey {
    /// Nouvelle clé (`scale` non fini ou ≤ 0 ⇒ 1.0, jamais de NaN en clé).
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

    /// Échelle de rendu.
    #[must_use]
    pub fn scale(self) -> f32 {
        f32::from_bits(self.scale_bits)
    }
}

/// Entrée : tuile R + métadonnée de contenu + récence LRU.
///
/// La validité vient du dirty (voir [`TileCache::get`]) ; `content` est une
/// métadonnée rafraîchie à chaque hit, jamais une cause de miss.
#[derive(Debug, Clone)]
struct CacheEntry {
    tile: RegionalComposite,
    content: TileContentSignature,
    pixels: usize,
    last_used: u64,
}

/// Compteurs du cache (observabilité, sans effet sur le rendu).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TileCacheStats {
    /// Lectures resservies sans recalcul.
    pub hits: u64,
    /// Lectures recalculées (absent, sale ou garde périmée).
    pub misses: u64,
    /// Entrées évincées par le budget (LRU).
    pub evictions: u64,
    /// Pixels stockés.
    pub cached_pixels: usize,
}

impl TileCacheStats {
    /// Recalculs évités par les hits (== `hits`, champ dédié pour les
    /// futures matrices de benchmark).
    #[must_use]
    pub fn renders_avoided(self) -> u64 {
        self.hits
    }
}

/// Cache LRU borné (en pixels) de tuiles document, dirty-aware.
///
/// Le `Clone` du hit recopie les pixels (`DynamicImage`) : acceptable pour
/// la fondation 6D (correction d'abord) ; un partage `Arc` viendra si les
/// mesures l'exigent — pas d'optimisation prématurée.
#[derive(Debug)]
pub struct TileCache {
    max_pixels: usize,
    entries: HashMap<DocTileKey, CacheEntry>,
    dirty: DirtyRegion,
    /// Zones protégées d'éviction (viewport visible, Phase 6F) : l'éviction
    /// LRU saute les entrées qui les intersectent. Portée d'appel (posée
    /// pour un rendu, retirée après) — jamais un état permanent.
    protected: Vec<TileRegion>,
    clock: u64,
    current_pixels: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl TileCache {
    /// Nouveau cache (`max_pixels` = budget pixels ; 0 = ne stocke rien,
    /// les lectures restent des miss).
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

    /// Protège des zones d'éviction (viewport visible) jusqu'au prochain
    /// [`TileCache::clear_protected`]. L'éviction préfère les entrées hors
    /// zones ; si TOUT est protégé, repli LRU strict (le budget prime —
    /// jamais dépassé). Ne change ni les hits ni la validité.
    pub fn set_protected(&mut self, rects: &[TileRegion]) {
        self.protected.clear();
        self.protected
            .extend(rects.iter().copied().filter(|r| !r.is_empty()));
    }

    /// Retire la protection d'éviction.
    pub fn clear_protected(&mut self) {
        self.protected.clear();
    }

    /// Zones protégées (diagnostic).
    #[must_use]
    pub fn protected(&self) -> &[TileRegion] {
        &self.protected
    }

    /// Vrai si la tuile intersecte une zone protégée.
    fn is_protected(&self, tile: &RegionalComposite) -> bool {
        let bounds = tile.tile_bounds();
        self.protected
            .iter()
            .any(|p| !p.intersect(bounds).is_empty())
    }

    /// Budget configuré (pixels).
    #[must_use]
    pub fn max_pixels(&self) -> usize {
        self.max_pixels
    }

    /// Entrées stockées (propres ou périmées en attente d'éviction).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Vrai si aucune entrée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Pixels stockés (toujours ≤ budget après insertion terminée).
    #[must_use]
    pub fn cached_pixels(&self) -> usize {
        self.current_pixels
    }

    /// État dirty (quelles zones doivent être reconstruites).
    #[must_use]
    pub fn dirty(&self) -> &DirtyRegion {
        &self.dirty
    }

    /// Métadonnée de contenu stockée pour la clé (`None` si absente) —
    /// diagnostic du rafraîchissement, sans effet sur la validité.
    #[must_use]
    pub fn cached_content(&self, key: DocTileKey) -> Option<TileContentSignature> {
        self.entries.get(&key).map(|e| e.content)
    }

    /// Tuiles en cache dont les pixels (bornes buffer) intersectent `rect`
    /// à `scale` (Phase 6G) : repeupler une surface neuve SANS recomposer
    /// (zéro calcul, que des clones). L'intersection se fait sur les pixels
    /// rendus (`tile_bounds`, buffer-space), pas sur les clés (document-space) :
    /// les deux espaces coïncident à origine nulle mais divergent en plan
    /// infini. Le dirty n'est PAS consulté ici : l'appelant (`RenderWorker`)
    /// ne repeuple qu'après avoir établi la validité.
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
        // Ordre LRU (plus récent d'abord) : déterministe ; recouvrements
        // résiduels au plus frais (mêmes octets de toute façon).
        out.sort_by_key(|(used, _)| u64::MAX - used);
        out.into_iter().map(|(_, tile)| tile).collect()
    }

    /// Statistiques cumulées.
    #[must_use]
    pub fn stats(&self) -> TileCacheStats {
        TileCacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            cached_pixels: self.current_pixels,
        }
    }

    /// Lecture : hit ssi l'entrée existe ET son rect est propre
    /// (`!dirty.intersects`) — la validité est déterminée par l'état dirty
    /// (Phase 6D §6), PAS par la signature de contenu. Marque la récence
    /// LRU et rafraîchit la métadonnée de contenu.
    ///
    /// Discipline d'usage (correction) : chaque mutation DOIT être marquée
    /// (`mark_dirty*`, expansion couvrant les dépendances réelles) AVANT
    /// toute relecture, sinon du périmé serait resservi.
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

    /// Stocke le résultat final R. Retourne faux (sans stocker) si la tuile
    /// seule dépasse le budget — le budget n'est jamais dépassé après une
    /// insertion terminée. Remplace l'entrée de même clé sans compter
    /// d'éviction ; chaque victime LRU compte une éviction.
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
            // Victime LRU hors zones protégées ; si tout est protégé, repli
            // LRU strict (le budget ne se négocie pas).
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

    /// Marque une zone modifiée (dépendance LOCALE : seules les tuiles
    /// intersectant la zone sont concernées).
    pub fn mark_dirty(&mut self, region: TileRegion) {
        self.dirty.add(region);
    }

    /// Marque une zone modifiée avec dépendance de voisinage : la zone
    /// d'invalidation est élargie du halo (DOCUMENT SPACE, mêmes valeurs
    /// que `blur_support_px` transmises par l'appelant via `ScopeWindow` —
    /// aucune (re)définition de halo ici).
    pub fn mark_dirty_halo(&mut self, region: TileRegion, halo_px: u32) {
        self.dirty.add(region.pad(Padding::new(halo_px)));
    }

    /// Marque selon une fenêtre 6C : LOCAL ⇒ zone seule, NEIGHBORHOOD ⇒
    /// zone + halo, GLOBAL ⇒ document entier (toutes les tuiles concernées).
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

    /// Invalidation globale : tout le document est sale (opération GLOBAL).
    /// Paresseux (aucune entrée retirée) : les gets manqueront via le dirty
    /// jusqu'au prochain `clear_dirty` — jamais de pixel faux.
    pub fn mark_global(&mut self, doc_w: u32, doc_h: u32) {
        self.dirty
            .add(TileRegion::new(0, 0, doc_w.max(1), doc_h.max(1)));
    }

    /// Oublie les zones sales (après re-rendu + présentation des tuiles
    /// affectées). Contrat d'usage, pas une devinette : sans `clear`, les
    /// requêtes restent sales (re-rendu conservateur).
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Solde EXACTEMENT les zones rendues (Phase 6G.2 §1) : à appeler après
    /// production+application du résultat, jamais avant. Retourne le nombre
    /// de zones entièrement soldées. Les zones partiellement rendues sont
    /// réduites (soustraction exacte), jamais effacées.
    pub fn consume_rendered(&mut self, rendered: &[TileRegion]) -> u64 {
        self.dirty.consume(rendered)
    }

    /// Vide le cache et oublie le dirty (changement structurel, resize…).
    /// Rend le nombre d'entrées retirées.
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
    /// Bornes buffer de la tuile (pour le test dirty).
    fn tile_bounds(&self) -> TileRegion {
        TileRegion::new(
            self.origin_x as i32,
            self.origin_y as i32,
            self.image.width(),
            self.image.height(),
        )
    }

    /// Pixels stockés (budget).
    fn pixels(&self) -> usize {
        self.image.width() as usize * self.image.height() as usize
    }
}

/// Rend R via le cache : hit ⇒ tuile resservie ; miss ⇒
/// `composite_region_with(R)` (fenêtré 6C : `D = R + halo` ou repli
/// global), insertion du R rogné, retour.
///
/// `content` = `tile_content_signature(doc)` (garde 6A) ; `backend` = backend
/// réel (CPU/GPU divergent) ; `scale` = échelle de rendu (1.0 : le zoom
/// d'affichage reste state-only, le champ partitionne pour de futurs
/// mipmaps) ; `render_stats` reçoit le coût du rendu en cas de miss (rien
/// n'est mesuré sur un hit — aucun calcul).
///
/// `None` (région vide/hors cadre, rien à contribuer) n'est ni compté
/// autrement qu'en miss (lecture) ni inséré.
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
    // Halo déterministe à contenu égal (6C) : même clé à chaque demande.
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
        // §6 : la validité vient du dirty, pas de la révision. Contenu
        // changé + tuile propre ⇒ hit (recalcul partiel possible, §17),
        // métadonnée rafraîchie.
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
        // Budget : 2 tuiles de 64².
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
        // Budget 2 tuiles : la protégée survit, les autres tombent en LRU.
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
        // Tout protégé : repli LRU strict (le budget prime).
        cache.set_protected(&[TileRegion::new(0, 0, 512, 512)]);
        let d = key_at(192, 0, 64);
        assert!(cache.insert(d, tile_at(192, 0, 64, 64, 4), C1));
        assert_eq!(cache.cached_pixels(), 2 * 64 * 64, "budget tenu quand même");
        cache.clear_protected();
        assert!(cache.protected().is_empty());
    }

    #[test]
    fn echelles_distinctes() {
        // Même rect, échelles 1.0 vs 2.0 : jamais de hit croisé ; retour 1.0
        // : hit (conservé). Mêmes pixels (rendu 1.0, pas de mipmaps 6F).
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
