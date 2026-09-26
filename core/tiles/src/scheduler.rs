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

//! Tile Scheduler : ordre progressif d'exécution des tuiles sales.
//!
//! Priorités (dans cet ordre) :
//!
//! 1. tuiles visibles ;
//! 2. proches du point d'interaction (focus) ;
//! 3. basse résolution (mips grossiers d'abord — aperçu rapide) ;
//! 4. haute résolution (raffinement) ;
//! 5. hors écran (en dernier).
//!
//! Le scheduler est APATRIDE et pur : il trie les sales dans le `Vec` de
//! l'appelant ([`TileScheduler::plan_into`], allocation réutilisée entre
//! frames) puis tronque au budget. L'exécution progressive appartient à
//! l'appelant : planifier → exécuter → nettoyer les faites → recommencer
//! jusqu'à ensemble vide.

use crate::{DirtyTiles, TileGrid, TileId, TileRegion};

/// Fenêtre d'intérêt : zone visible + point d'interaction, en pixels
/// niveau 0 (même espace que la surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    /// Zone visible (pixels niveau 0).
    pub visible: TileRegion,
    /// Point d'interaction (curseur, centre du geste — pixels niveau 0).
    pub focus: (i32, i32),
}

impl Viewport {
    /// Nouvelle fenêtre.
    #[must_use]
    pub fn new(visible: TileRegion, focus: (i32, i32)) -> Self {
        Self { visible, focus }
    }
}

/// Ordonnanceur de tuiles sales (pur, sans état).
pub struct TileScheduler;

impl TileScheduler {
    /// Trie les sales dans `out` (vidé d'abord) puis tronque au budget
    /// (`None` = tout). Le `Vec` est réutilisé d'un appel à l'autre :
    /// aucune allocation par frame en régime établi.
    pub fn plan_into(
        &self,
        dirty: &DirtyTiles,
        grid: &TileGrid,
        view: &Viewport,
        budget: Option<usize>,
        out: &mut Vec<TileId>,
    ) {
        out.clear();
        out.extend(dirty.iter_dirty());
        out.sort_by(|a, b| Self::compare(grid, view, *a, *b));
        if let Some(max) = budget {
            out.truncate(max);
        }
    }

    /// Plan neuf (alloue — préférer `plan_into` en boucle de rendu).
    #[must_use]
    pub fn plan(
        &self,
        dirty: &DirtyTiles,
        grid: &TileGrid,
        view: &Viewport,
        budget: Option<usize>,
    ) -> Vec<TileId> {
        let mut out = Vec::new();
        self.plan_into(dirty, grid, view, budget, &mut out);
        out
    }

    /// Comparateur de priorités (visible, proche, grossier d'abord).
    fn compare(grid: &TileGrid, view: &Viewport, a: TileId, b: TileId) -> std::cmp::Ordering {
        let rect_a = grid.tile_rect_l0(a);
        let rect_b = grid.tile_rect_l0(b);
        let vis_a = u8::from(!overlaps(&rect_a, &view.visible));
        let vis_b = u8::from(!overlaps(&rect_b, &view.visible));
        vis_a
            .cmp(&vis_b)
            .then_with(|| dist2(rect_a, view.focus).cmp(&dist2(rect_b, view.focus)))
            .then_with(|| b.level.cmp(&a.level))
            .then_with(|| (a.x, a.y).cmp(&(b.x, b.y)))
    }
}

/// Deux régions se recouvrent-elles (vides = jamais) ?
fn overlaps(a: &TileRegion, b: &TileRegion) -> bool {
    !a.intersect(*b).is_empty()
}

/// Distance² (entière, sans NaN) du centre du rectangle au focus.
fn dist2(rect: TileRegion, focus: (i32, i32)) -> i64 {
    let cx = i64::from(rect.x) + i64::from(rect.width) / 2;
    let cy = i64::from(rect.y) + i64::from(rect.height) / 2;
    let dx = cx - i64::from(focus.0);
    let dy = cy - i64::from(focus.1);
    dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
}
