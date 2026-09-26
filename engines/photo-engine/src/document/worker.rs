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

//! Rendu incrémental par tuiles (Phase 6E) : orchestration, pas de calcul.
//!
//! [`RenderWorker`] enchaîne les fondations sans absorber leurs
//! responsabilités :
//!
//! ```text
//! DirtyRegion (dirty.rs)      = quelles zones sont modifiées
//! SpatialScope (compositing)  = pour R, quelle dépendance D calculer
//! TileCache (tile_cache.rs)   = réutiliser le résultat final de R
//! RenderWorker (ici)          = découper, regarder, assembler, mesurer
//! ```
//!
//! Le worker ne connaît ni blur ni blend : pour chaque tuile manquante il
//! délègue à [`get_or_render`](super::tile_cache::get_or_render), qui
//! délègue à `composite_region_with` (fenêtre 6C). Séquentiel (pas de
//! scheduler multithread en 6E), pur moteur (aucun egui/wgpu).
//!
//! Espaces : les requêtes de tuiles sont en DOCUMENT SPACE (rects entiers)
//! ; l'assemblage se fait en coordonnées buffer via l'origine portée par
//! chaque [`RegionalComposite`] — aucun calcul géométrique dupliqué ici.
//!
//! Sécurité de périmètre : le worker suit la géométrie du scope
//! ([`ScopeGeom`]) ; tout changement (resize, débordement, contenu déplacé)
//! vide le cache avant rendu — jamais de tuile d'un ancien périmètre
//! resservie sous une nouvelle géométrie.

use std::sync::Arc;

use image::DynamicImage;
use tiles::TileRegion;
use uuid::Uuid;

use super::compositing::{CompositeStats, scope_half_extents};
use super::preview_surface::{PreviewSurface, PreviewSurfaceStats};
use super::scheduler::{RenderPriorityContext, RenderRequest, TilePriority, schedule_viewport};
use super::tile_cache::{TILE_CACHE_FLAGS_NONE, TileCache, get_or_render};
use super::tree::Document;
use crate::gpu::GpuContext;
use crate::tile_key::{BackendTag, GPU_PIXEL_THRESHOLD, tile_content_signature};

/// Géométrie du périmètre de rendu (= grille d'assemblage) : dimensions du
/// buffer + origine monde (mêmes formules que `composite_scope_with`).
/// Mode rogné : `{doc, 0, 0}` ; plan infini : extents réels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScopeGeom {
    /// Largeur du buffer (px).
    pub w: u32,
    /// Hauteur du buffer (px).
    pub h: u32,
    /// Origine monde X (monde (0,0) = buffer − origine).
    pub origin_x: f32,
    /// Origine monde Y.
    pub origin_y: f32,
}

impl ScopeGeom {
    /// Périmètre document (mode rogné) : buffer == document, origine nulle.
    #[must_use]
    pub fn document(doc_w: u32, doc_h: u32) -> Self {
        Self {
            w: doc_w.max(1),
            h: doc_h.max(1),
            origin_x: 0.0,
            origin_y: 0.0,
        }
    }

    /// Rectangle buffer couvert.
    #[must_use]
    pub fn rect(self) -> TileRegion {
        TileRegion::new(0, 0, self.w, self.h)
    }
}

/// Vue assemblée : pixels + origine buffer (correspondance document-space
/// conservée comme `RegionalComposite`).
#[derive(Debug, Clone)]
pub struct AssembledView {
    /// Pixels assemblés (viewport clippé au scope).
    pub image: DynamicImage,
    /// Colonne buffer de l'origine.
    pub origin_x: u32,
    /// Ligne buffer de l'origine.
    pub origin_y: u32,
}

/// Compteurs d'orchestration (Phase 6E §15) : que du comptage, aucune
/// télémétrie complexe. `rendered_tiles` = tuiles évaluées (miss ⇒ une
/// évaluation régionale) ; `dirty_tiles` = tuiles requises intersectant le
/// dirty AVANT rendu ; `dependency_px` = pixels de dépendance cumulés
/// (somme des fenêtres D réellement évaluées — `CompositeStats::scope_px`
/// étant écrasé à chaque tuile, ce cumul est LA mesure §18/§22).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderWorkerStats {
    /// Tuiles demandées (viewport ∩ scope).
    pub requested_tiles: u64,
    /// Tuiles évaluées (miss).
    pub rendered_tiles: u64,
    /// Tuiles resservies sans calcul.
    pub cache_hits: u64,
    /// Tuiles manquées.
    pub cache_misses: u64,
    /// Tuiles sales parmi les requises (découverte dirty).
    pub dirty_tiles: u64,
    /// Tuiles visibles requises (== `requested_tiles` tant qu'il n'y a pas
    /// de prefetch — Phase 6F §20).
    pub visible_tiles: u64,
    /// Pixels de dépendance évalués (cumul des D).
    pub dependency_px: u64,
    /// Temps de construction du plan (ordonnancement pur, Phase 6G.1).
    pub schedule_us: u128,
    /// Temps total des évaluations de tuiles (lookup cache + rendu miss).
    pub tile_render_total_us: u128,
    /// Plus petite évaluation de tuile (µs, 0 si aucune).
    pub tile_render_min_us: u64,
    /// Plus grande évaluation de tuile (µs).
    pub tile_render_max_us: u64,
    /// Temps de découpe d'assemblage (memcpy O(viewport), Phase 6G.1 §7).
    pub assembly_us: u128,
}

/// Orchestrateur du rendu incrémental : cache + grain + périmètre suivi +
/// surface persistante.
///
/// Le `Clone` des hits recopie les pixels (fondation correctness-first,
/// cf. `TileCache` ; `Arc` documenté comme optimisation future). La surface
/// (`PreviewSurface`, Phase 6G) accumule les tuiles rendues : un hit
/// n'écrit RIEN (zéro réécriture), un miss n'écrit que sa tuile.
#[derive(Debug)]
pub struct RenderWorker {
    cache: TileCache,
    tile_px: u32,
    last_scope: Option<ScopeGeom>,
    surface: Option<PreviewSurface>,
    /// Rects document effectivement (re)rendus par le dernier `render`
    /// (miss avec contribution) — soldés via `take_rendered_tiles`.
    last_rendered: Vec<TileRegion>,
}

impl RenderWorker {
    /// Nouveau worker (budget cache en pixels, grain de tuile en px ;
    /// `VIEWPORT_TILE_PX` par défaut). Surface créée au premier rendu
    /// (chemin full explicite).
    #[must_use]
    pub fn new(max_pixels: usize, tile_px: u32) -> Self {
        Self {
            cache: TileCache::new(max_pixels),
            tile_px: tile_px.max(1),
            last_scope: None,
            surface: None,
            last_rendered: Vec::new(),
        }
    }

    /// Rects rendus par le dernier `render`, vidés à la lecture (Phase 6G.2
    /// §1) : l'appelant les transmet à `consume_rendered` APRÈS application
    /// du résultat — jamais avant, jamais sans rendu.
    pub fn take_rendered_tiles(&mut self) -> Vec<TileRegion> {
        std::mem::take(&mut self.last_rendered)
    }

    /// Statistiques cumulées de la surface persistante (zéro si jamais créée).
    #[must_use]
    pub fn surface_stats(&self) -> PreviewSurfaceStats {
        self.surface
            .as_ref()
            .map_or(PreviewSurfaceStats::default(), PreviewSurface::stats)
    }

    /// Oublie la surface (le cache est conservé) : le prochain `render` la
    /// recrée vide puis la repeuple par rendu, ou `repopulate_from_cache`
    /// la remplit SANS recomposer. Explicite (§8 : ni silencieux ni fréquent).
    pub fn reset_surface(&mut self) {
        self.surface = None;
    }

    /// Repeuple la surface depuis le cache seul (zéro compositing) puis
    /// retourne la découpe `viewport` (coords buffer du `scope`).
    /// `None` si scope inconnu/incohérent (passer par `render`, qui détecte
    /// et invalide) ou si aucune tuile ne couvre la zone.
    pub fn repopulate_from_cache(
        &mut self,
        scope: ScopeGeom,
        viewport: TileRegion,
        scale: f32,
    ) -> Option<AssembledView> {
        if self.last_scope != Some(scope) {
            return None;
        }
        let clip = viewport.intersect(scope.rect());
        if clip.is_empty() {
            return None;
        }
        self.surface_for(scope);
        let tiles = self.cache.tiles_for_surface(clip, scale);
        if tiles.is_empty() {
            return None;
        }
        let surface = self.surface.as_mut().expect("créée ci-dessus");
        surface.update_tiles(&tiles);
        Self::crop_surface(surface, &clip)
    }

    /// Surface garantie aux dims du scope (créée si besoin : full explicite).
    fn surface_for(&mut self, scope: ScopeGeom) -> &mut PreviewSurface {
        if self.surface.is_none() {
            self.surface = Some(PreviewSurface::new(scope.w, scope.h));
        }
        let surface = self.surface.as_mut().expect("créée ci-dessus");
        surface.ensure(scope.w, scope.h);
        surface
    }

    /// Découpe la surface au clip (coords buffer).
    fn crop_surface(surface: &PreviewSurface, clip: &TileRegion) -> Option<AssembledView> {
        let (sw, sh) = surface.dimensions();
        let rect = TileRegion::new(0, 0, sw, sh).intersect(*clip);
        if rect.is_empty() {
            return None;
        }
        let cropped = image::imageops::crop_imm(
            surface.image(),
            rect.x as u32,
            rect.y as u32,
            rect.width,
            rect.height,
        )
        .to_image();
        Some(AssembledView {
            image: DynamicImage::ImageRgba8(cropped),
            origin_x: rect.x.max(0) as u32,
            origin_y: rect.y.max(0) as u32,
        })
    }

    /// Accès au cache (marquage dirty par l'appelant — voir le worker applicatif).
    pub fn cache_mut(&mut self) -> &mut TileCache {
        &mut self.cache
    }

    /// Accès lecture au cache (découverte dirty, diagnostics).
    #[must_use]
    pub fn cache(&self) -> &TileCache {
        &self.cache
    }

    /// Grain de tuile (px).
    #[must_use]
    pub fn tile_px(&self) -> u32 {
        self.tile_px
    }

    /// Périmètre réel du document (plan infini) : mêmes formules que
    /// `composite_scope_with` (extents + clamp), sans aucun pixel calculé.
    #[must_use]
    pub fn scope_for(
        &self,
        doc: &Document,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> ScopeGeom {
        let (half_w, half_h) = scope_half_extents(&doc.root, doc.width, doc.height, resolve);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        ScopeGeom {
            w: w.max(1),
            h: h.max(1),
            origin_x: half_w - doc.width as f32 / 2.0,
            origin_y: half_h - doc.height as f32 / 2.0,
        }
    }

    /// Requête couvrant tout un scope (DOCUMENT SPACE) : le plan issu de
    /// cette requête assemble exactement le buffer du scope (l'assemblage
    /// utilise les origines buffer authoritatives des tuiles).
    #[must_use]
    pub fn scope_request(scope: ScopeGeom, scale: f32) -> RenderRequest {
        RenderRequest::new(
            TileRegion::new(
                (-scope.origin_x).floor() as i32,
                (-scope.origin_y).floor() as i32,
                ((scope.w as f32 - scope.origin_x).ceil() - (-scope.origin_x).floor()).max(1.0)
                    as u32,
                ((scope.h as f32 - scope.origin_y).ceil() - (-scope.origin_y).floor()).max(1.0)
                    as u32,
            ),
            scale,
        )
    }

    /// Rend une requête viewport (DOCUMENT SPACE) : plan ordonnancé
    /// (sales d'abord, puis distance — [`schedule_viewport`]), tuiles via
    /// le cache, assemblage exact en coords buffer, statistiques
    /// d'orchestration.
    ///
    /// Seules les tuiles manquantes sont évaluées (`composite_region_with`
    /// fenêtré). `None` si requête vide, hors scope ou rien à contribuer.
    /// L'ordre du plan est l'ordre d'exécution (visible+dirty d'abord).
    ///
    /// Backend HONNÊTE par tuile (les noyaux CPU/GPU divergent — clé fausse
    /// interdite) : estimé depuis la fenêtre D (requête + halo 6C) via
    /// [`BackendTag::select`]. Sous le seuil, aucun `is_available` n'est
    /// appelé (pas de réveil GPU pour les petits documents) ; déterministe
    /// à contenu égal, donc hits stables.
    ///
    /// Huit paramètres explicites (pas de `RenderJob` artificiel : chaque
    /// appelant fournit déjà ces pièces, cf. `get_or_render` / 6B).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        doc: &Document,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        render_stats: &mut CompositeStats,
        scope: ScopeGeom,
        request: &RenderRequest,
        ctx: &RenderPriorityContext,
        stats: &mut RenderWorkerStats,
    ) -> Option<AssembledView> {
        if scope.w == 0 || scope.h == 0 {
            return None;
        }
        // Périmètre suivi : tout changement vide le cache (sécurité, pas
        // d'optimisation : les clés d'un ancien périmètre ne doivent jamais
        // resservir sous une nouvelle géométrie).
        if self.last_scope != Some(scope) {
            self.cache.invalidate_all();
            self.last_scope = Some(scope);
        }
        // Plage document valide (scope ramené en document) : le plan ne
        // demande jamais hors de cette zone (les débordements n'ont pas de
        // contenu adressable et rendraient None de toute façon).
        let doc_valid = TileRegion::new(
            (-scope.origin_x).floor() as i32,
            (-scope.origin_y).floor() as i32,
            ((scope.w as f32 - scope.origin_x).ceil() - (-scope.origin_x).floor()).max(1.0) as u32,
            ((scope.h as f32 - scope.origin_y).ceil() - (-scope.origin_y).floor()).max(1.0) as u32,
        );
        let viewport = request.viewport.intersect(doc_valid);
        if viewport.is_empty() {
            return None;
        }
        // Fenêtre buffer d'assemblage : viewport mappé (+ origine), clippé
        // au scope. Les tuiles rendues portent leur origine buffer
        // authoritative (`RegionalComposite`) : l'assemblage ne recalcule
        // aucune géométrie.
        let clip = TileRegion::new(
            (viewport.x as f32 + scope.origin_x).floor() as i32,
            (viewport.y as f32 + scope.origin_y).floor() as i32,
            (((viewport.x as f32 + viewport.width as f32) + scope.origin_x).ceil()
                - (viewport.x as f32 + scope.origin_x).floor())
            .max(1.0) as u32,
            (((viewport.y as f32 + viewport.height as f32) + scope.origin_y).ceil()
                - (viewport.y as f32 + scope.origin_y).floor())
            .max(1.0) as u32,
        )
        .intersect(scope.rect());
        if clip.is_empty() {
            return None;
        }
        let sub_request = RenderRequest::new(viewport, request.scale);
        // Instrumentation 6G.1 (aucun effet sur le rendu) : chrono du plan.
        let t_schedule = std::time::Instant::now();
        let plan = schedule_viewport(&sub_request, self.cache.dirty(), self.tile_px, ctx);
        stats.schedule_us += t_schedule.elapsed().as_micros();
        if plan.is_empty() {
            return None;
        }
        // Surface persistante aux dims du scope (créée/recréée ici : chemin
        // full explicite — jamais de reconstruction silencieuse ailleurs).
        self.surface_for(scope);
        // Protection viewport pendant le rendu : les évictions préfèrent les
        // tuiles hors champ (repli LRU strict si tout est protégé).
        let protected: Vec<TileRegion> = plan.iter().map(|item| item.rect).collect();
        self.cache.set_protected(&protected);
        let content = tile_content_signature(doc);
        let halo = super::compositing::scope_adjustment_window(&doc.root).halo_px;
        let mut contributed = false;
        // Emprunts disjoints : la surface écrit, le cache lit, le relevé
        // des rects rendus s'accumule pour `take_rendered_tiles`.
        let Self {
            surface,
            cache,
            last_rendered,
            ..
        } = self;
        last_rendered.clear();
        let surface = surface.as_mut().expect("créée ci-dessus");
        for item in &plan {
            stats.requested_tiles += 1;
            stats.visible_tiles += 1;
            if item.priority == TilePriority::VisibleDirty {
                stats.dirty_tiles += 1;
            }
            // Fenêtre D estimée (requête + halo) : backend réel (CPU sous
            // le seuil sans requête GPU, `select` au-delà).
            let est = (u64::from(item.rect.width) + 2 * u64::from(halo))
                * (u64::from(item.rect.height) + 2 * u64::from(halo));
            let backend = if est < GPU_PIXEL_THRESHOLD {
                BackendTag::cpu()
            } else {
                BackendTag::select(GpuContext::is_available(), est)
            };
            let (hits_avant, misses_avant) = (cache.stats().hits, cache.stats().misses);
            // Une zone sans contribution (accumulateur vide) rend None :
            // on saute la tuile, les voisines contribuent quand même.
            // Instrumentation 6G.1 : chrono par tuile (lookup + rendu).
            let t_tile = std::time::Instant::now();
            let tile = get_or_render(
                cache,
                doc,
                resolve,
                render_stats,
                &item.rect,
                backend,
                TILE_CACHE_FLAGS_NONE,
                request.scale,
                content,
            );
            let tile_us = t_tile.elapsed().as_micros();
            stats.tile_render_total_us += tile_us;
            // Min/max en µs (u64) : initialisés à la première tuile.
            let tile_us64 = tile_us.min(u128::from(u64::MAX)) as u64;
            if stats.requested_tiles <= 1 {
                stats.tile_render_min_us = tile_us64;
                stats.tile_render_max_us = tile_us64;
            } else {
                stats.tile_render_min_us = stats.tile_render_min_us.min(tile_us64);
                stats.tile_render_max_us = stats.tile_render_max_us.max(tile_us64);
            }
            let Some(tile) = tile else {
                stats.cache_misses += cache.stats().misses - misses_avant;
                continue;
            };
            stats.cache_hits += cache.stats().hits - hits_avant;
            let misses = cache.stats().misses - misses_avant;
            stats.cache_misses += misses;
            if misses > 0 {
                // Seuls les miss écrivent la surface persistante : un hit
                // prouve que la surface détient déjà ces octets (zéro
                // réécriture — le cœur de 6G). Le rect rendu est relevé
                // pour consommation dirty APRÈS application (§1 6G.2).
                surface.update_tile(&tile);
                last_rendered.push(item.rect);
                // `composite_region_with` pose scope_px = fenêtre D
                // évaluée : cumul worker (l'assignation par tuile ne
                // permet pas de lire un total dans CompositeStats).
                stats.rendered_tiles += misses;
                stats.dependency_px += render_stats.scope_px;
            }
            contributed = true;
        }
        self.cache.clear_protected();
        if !contributed {
            return None;
        }
        // Instrumentation 6G.1 : la découpe (memcpy O(viewport)) est mesurée
        // explicitement (§7 : elle doit rester visible dans les métriques).
        let t_assembly = std::time::Instant::now();
        let out = Self::crop_surface(self.surface.as_ref().expect("créée ci-dessus"), &clip);
        stats.assembly_us += t_assembly.elapsed().as_micros();
        out
    }
}
