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

//! Région sale (Phase 6D) : QUELLES zones ont été modifiées et nécessitent
//! une reconstruction.
//!
//! [`DirtyRegion`] ne fait AUCUN compositing et ne connaît aucun filtre :
//! une liste de rectangles en DOCUMENT SPACE (le même espace que les
//! régions envoyées à `TileGrid` / `composite_region_with`).
//!
//! Séparation stricte (règle centrale 6D) :
//!
//! - `SpatialScope` (Phase 6C, `compositing.rs`) : pour produire R, quelle
//!   région D calculer (`D = R`, `D = R + halo`, `D = full`) ;
//! - [`DirtyRegion`] (ici) : quelles zones sont modifiées ;
//! - `TileCache` (`tile_cache.rs`) : réutiliser le résultat final de R.
//!
//! La logique `D = R + halo` ne vit PAS ici : l'appelant la résout via
//! `scope_adjustment_window` (Phase 6C) et transmet des rectangles déjà
//! exprimés en zones d'invalidation (voir
//! [`TileCache::mark_dirty_scope`](crate::document::tile_cache::TileCache::mark_dirty_scope)).
//!
//! Contrat (Phase 6G.2 §1) : après production du résultat, l'appelant
//! consomme EXACTEMENT les zones rendues via [`DirtyRegion::consume`] —
//! jamais avant (le dirty ne part que si le résultat existe), jamais plus
//! large (les zones non rendues restent sales). `clear` total reste pour
//! les réinitialisations. Sans consommation, les requêtes restent sales
//! (re-rendu conservateur, jamais de pixel faux).

use tiles::TileRegion;

/// Zones invalidées, en DOCUMENT SPACE.
///
/// Représentation volontairement simple (liste de rectangles, pas de
/// bitset/quadtree/BVH) : la correction d'abord, l'optimisation plus tard.
/// Les rectangles peuvent se chevaucher ; `clear` repart de zéro.
#[derive(Debug, Clone, Default)]
pub struct DirtyRegion {
    rects: Vec<TileRegion>,
}

impl DirtyRegion {
    /// Région vide (rien à reconstruire).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute une zone invalidée (les rectangles vides sont ignorés).
    pub fn add(&mut self, rect: TileRegion) {
        if !rect.is_empty() {
            self.rects.push(rect);
        }
    }

    /// Oublie toutes les zones (réinitialisation, changement de périmètre).
    pub fn clear(&mut self) {
        self.rects.clear();
    }

    /// Consomme les zones effectivement rendues (Phase 6G.2 §1) : soustrait
    /// chaque rect rendu des zones sales (soustraction exacte, ≤ 4 morceaux
    /// par rect). Retourne le nombre de zones sales entièrement soldées.
    ///
    /// Précisément ce qui a été produit disparaît, rien de plus : une zone
    /// sale à cheval sur une tuile rendue et une autre reste sale sur la
    /// partie non rendue (re-rendu ciblé à la prochaine frame, jamais de
    /// pixel faux, jamais de re-rendu inutile).
    pub fn consume(&mut self, rendered: &[TileRegion]) -> u64 {
        if rendered.is_empty() || self.rects.is_empty() {
            return 0;
        }
        let mut cleared = 0u64;
        let mut kept: Vec<TileRegion> = Vec::with_capacity(self.rects.len());
        for rect in self.rects.drain(..) {
            let mut pieces = vec![rect];
            for cut in rendered {
                let mut next = Vec::with_capacity(pieces.len() * 2);
                for piece in pieces.drain(..) {
                    next.extend(subtract_rect(piece, *cut));
                }
                pieces = next;
                if pieces.is_empty() {
                    break;
                }
            }
            if pieces.is_empty() {
                cleared += 1;
            } else {
                kept.extend(pieces);
            }
        }
        self.rects = kept;
        cleared
    }

    /// Vrai si aucune zone invalidée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// Nombre de rectangles suivis (observabilité, sans effet).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// Vrai si `rect` intersecte au moins une zone invalidée.
    ///
    /// C'est le seul test utilisé par `TileCache` : une tuile R est
    /// réutilisable ssi `!dirty.intersects(R)` (ET garde de contenu OK).
    #[must_use]
    pub fn intersects(&self, rect: TileRegion) -> bool {
        if rect.is_empty() {
            return false;
        }
        self.rects.iter().any(|d| !d.intersect(rect).is_empty())
    }

    /// Union englobante des zones (`EMPTY` si vide) — pour choisir quoi
    /// re-rendre en un passage.
    #[must_use]
    pub fn bounds(&self) -> TileRegion {
        self.rects
            .iter()
            .fold(TileRegion::EMPTY, |acc, r| acc.union(*r))
    }

    /// Itération sur les zones (assemblage, diagnostics).
    #[must_use]
    pub fn rects(&self) -> &[TileRegion] {
        &self.rects
    }
}

/// Soustrait `cut` de `rect` (géométrie entière exacte, ≤ 4 morceaux non
/// vides : bandes gauche/droite sur toute la hauteur + bandes haut/bas sur
/// la largeur restante). `cut` hors `rect` ⇒ `[rect]` ; recouvrement total
/// ⇒ `[]`.
fn subtract_rect(rect: TileRegion, cut: TileRegion) -> Vec<TileRegion> {
    let cut = rect.intersect(cut);
    if cut.is_empty() {
        return vec![rect];
    }
    if cut == rect {
        return Vec::new();
    }
    // Bornes en i64 (aucun dépassement sur des i32) ; largeurs saturées.
    let (rx0, ry0) = (i64::from(rect.x), i64::from(rect.y));
    let (rx1, ry1) = (rx0 + i64::from(rect.width), ry0 + i64::from(rect.height));
    let (cx0, cy0) = (i64::from(cut.x), i64::from(cut.y));
    let (cx1, cy1) = (cx0 + i64::from(cut.width), cy0 + i64::from(cut.height));
    let mut out = Vec::with_capacity(4);
    let mut push = |x0: i64, y0: i64, x1: i64, y1: i64| {
        let (w, h) = (x1 - x0, y1 - y0);
        if w > 0 && h > 0 {
            let x0 = x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            let y0 = y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            out.push(TileRegion::new(
                x0,
                y0,
                w.min(i64::from(u32::MAX)) as u32,
                h.min(i64::from(u32::MAX)) as u32,
            ));
        }
    };
    push(rx0, ry0, cx0, ry1);
    push(cx1, ry0, rx1, ry1);
    push(cx0, ry0, cx1, cy0);
    push(cx0, cy1, cx1, ry1);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vide_puis_ajout_intersection() {
        let mut d = DirtyRegion::new();
        assert!(d.is_empty());
        assert!(!d.intersects(TileRegion::new(0, 0, 64, 64)));
        d.add(TileRegion::EMPTY);
        assert!(d.is_empty(), "les vides sont ignorés");
        d.add(TileRegion::new(10, 10, 20, 20));
        assert!(!d.is_empty());
        assert_eq!(d.len(), 1);
        assert!(d.intersects(TileRegion::new(0, 0, 64, 64)));
        assert!(
            d.intersects(TileRegion::new(25, 25, 64, 64)),
            "chevauchement"
        );
        assert!(
            !d.intersects(TileRegion::new(30, 10, 10, 10)),
            "adjacent exclu"
        );
        assert!(!d.intersects(TileRegion::new(100, 100, 10, 10)));
        d.clear();
        assert!(d.is_empty());
    }

    #[test]
    fn union_englobante() {
        let mut d = DirtyRegion::new();
        assert_eq!(d.bounds(), TileRegion::EMPTY);
        d.add(TileRegion::new(0, 0, 10, 10));
        d.add(TileRegion::new(20, 20, 10, 10));
        assert_eq!(d.bounds(), TileRegion::new(0, 0, 30, 30));
    }

    #[test]
    fn consume_exact() {
        let mut d = DirtyRegion::new();
        d.add(TileRegion::new(0, 0, 100, 100));
        // Rendu partiel : reste une bande droite.
        assert_eq!(d.consume(&[TileRegion::new(0, 0, 40, 100)]), 0);
        assert_eq!(d.rects(), &[TileRegion::new(40, 0, 60, 100)]);
        // Recouvrement total : soldé.
        assert_eq!(d.consume(&[TileRegion::new(0, 0, 100, 100)]), 1);
        assert!(d.is_empty());
        // Hors zone : intact.
        d.add(TileRegion::new(0, 0, 10, 10));
        assert_eq!(d.consume(&[TileRegion::new(50, 50, 10, 10)]), 0);
        assert_eq!(d.len(), 1);
        // Trou central : 4 morceaux, aire conservée.
        let mut d = DirtyRegion::new();
        d.add(TileRegion::new(0, 0, 100, 100));
        assert_eq!(d.consume(&[TileRegion::new(40, 40, 20, 20)]), 0);
        assert_eq!(d.len(), 4);
        let area: u64 = d.rects().iter().map(|r| r.area()).sum();
        assert_eq!(area, 100 * 100 - 20 * 20);
        assert!(!d.intersects(TileRegion::new(40, 40, 20, 20)));
        assert!(d.intersects(TileRegion::new(0, 0, 100, 100)));
    }
}
