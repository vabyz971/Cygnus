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

//! Tile tests: model, invalidation, cache, scheduler.

use super::*;
use crate::{Padding, TileCoord, TileExtent, TileScheduler, Viewport};

fn grille_test() -> TileGrid {
    TileGrid::new(256, 1920, 1080, 3)
}

#[test]
fn rectangle_vers_tuiles_cas_nominal() {
    let grid = grille_test();
    // 1920/256 = 7.5 → 8 columns; 1080/256 = 4.22 → 5 rows.
    assert_eq!(grid.tile_count(0), (8, 5));
    // Top-left corner: a single tile.
    let range = grid.tiles_for_rect(&TileRegion::new(0, 0, 10, 10), 0);
    assert_eq!((range.x0, range.y0, range.x1, range.y1), (0, 0, 0, 0));
    assert_eq!(range.count(), 1);
    // Straddling 2×2 tiles (exclusive edge: 256 belongs to the next one).
    let range = grid.tiles_for_rect(&TileRegion::new(200, 200, 100, 100), 0);
    assert_eq!((range.x0, range.y0, range.x1, range.y1), (0, 0, 1, 1));
    assert_eq!(range.count(), 4);
}

#[test]
fn tuiles_vers_rectangle_englobant() {
    let grid = grille_test();
    let range = grid.tiles_for_rect(&TileRegion::new(200, 200, 100, 100), 0);
    let region = range.to_region(&grid);
    // Encloses the source and aligns to the grid.
    assert_eq!(region, TileRegion::new(0, 0, 512, 512));
    assert!(
        !region
            .intersect(TileRegion::new(200, 200, 100, 100))
            .is_empty()
    );
    // Empty range → empty region (safe round-trip).
    assert!(TileRange::empty(0).to_region(&grid).is_empty());
}

#[test]
fn clipping_bords_et_hors_surface() {
    let grid = grille_test();
    // Bottom-right overflow: clipped to the existing tiles.
    let range = grid.tiles_for_rect(&TileRegion::new(1800, 1000, 500, 500), 0);
    assert_eq!((range.x0, range.y0, range.x1, range.y1), (7, 3, 7, 4));
    // Fully off-surface: empty (never ghost tiles).
    assert!(
        grid.tiles_for_rect(&TileRegion::new(5000, 5000, 100, 100), 0)
            .is_empty()
    );
    assert!(grid.tiles_for_rect(&TileRegion::EMPTY, 0).is_empty());
    // Unknown level: empty.
    assert!(
        grid.tiles_for_rect(&TileRegion::new(0, 0, 100, 100), 9)
            .is_empty()
    );
    // Negative coordinates: correct floor (pixel -1 ⇒ tile -1).
    let range = grid.tiles_for_rect(&TileRegion::new(-300, -300, 100, 100), 0);
    assert!(range.is_empty(), "hors surface à gauche/haut");
    let rect = grid.tile_rect(TileId::new(0, -1, -1));
    assert!(rect.is_empty());
}

#[test]
fn invalidation_elargit_du_padding() {
    let grid = grille_test();
    let rect = TileRegion::new(100, 100, 50, 50);
    let sans = grid.invalidate(&rect, 0, Padding::ZERO);
    let avec = grid.invalidate(&rect, 0, Padding::new(300));
    assert!(avec.count() > sans.count());
    // Zero padding = rect alone.
    assert_eq!(sans, grid.tiles_for_rect(&rect, 0));
}

#[test]
fn padding_rayon_de_flou() {
    assert_eq!(Padding::for_blur_radius(20.0), Padding::new(20));
    assert_eq!(Padding::for_blur_radius(20.2), Padding::new(21));
    assert_eq!(Padding::for_blur_radius(0.0), Padding::ZERO);
    assert_eq!(Padding::for_blur_radius(-5.0), Padding::ZERO);
    assert_eq!(Padding::for_blur_radius(f32::NAN), Padding::ZERO);
    assert_eq!(Padding::for_blur_radius(f32::INFINITY), Padding::ZERO);
    // Exact symmetric expansion.
    let padded = TileRegion::new(100, 100, 50, 50).pad(Padding::new(20));
    assert_eq!(padded, TileRegion::new(80, 80, 90, 90));
}

#[test]
fn invalidations_chevauchantes_fusionnees_par_le_bitset() {
    let grid = grille_test();
    let mut dirty = DirtyTiles::new(&grid);
    let n1 = dirty.mark_range(grid.invalidate(&TileRegion::new(0, 0, 300, 300), 0, Padding::ZERO));
    let n2 =
        dirty.mark_range(grid.invalidate(&TileRegion::new(200, 200, 400, 400), 0, Padding::ZERO));
    assert!(n1 > 0 && n2 > 0);
    // Exact count: the overlap (2×2 tiles) is counted only once.
    assert_eq!((n1, n2), (4, 5));
    let count = dirty.iter_dirty().count() as u64;
    assert_eq!(dirty.dirty_count(), count);
    assert_eq!(count, 9);
    // Re-marking: zero new tiles.
    assert_eq!(
        dirty.mark_range(grid.invalidate(&TileRegion::new(0, 0, 300, 300), 0, Padding::ZERO)),
        0
    );
}

#[test]
fn revisions_distinguent_les_contenus() {
    let mut cache = CpuTileCache::new(8);
    let id = TileId::new(0, 1, 1);
    let r1 = Revision(7);
    let r2 = Revision(8);
    let tile = CpuTile {
        data: vec![0u8; 16].into(),
        width: 2,
        height: 2,
    };
    assert!(cache.insert(id, r1, tile.clone()).is_none());
    assert!(cache.get(id, r1).is_some(), "même révision = hit");
    assert!(cache.get(id, r2).is_none(), "autre révision = miss");
    assert_eq!(cache.revision_of(id), Some(r1));
    // Distinct composite keys.
    assert_ne!(TileKey::new(id, r1), TileKey::new(id, r2));
    assert_eq!(TileKey::new(id, r1), TileKey::new(id, r1));
}

#[test]
fn cles_cache_stables_et_hachables() {
    use std::collections::HashSet;
    let id = TileId::new(1, 3, 4);
    assert_eq!(format!("{id}"), "t1:3:4");
    let mut set = HashSet::new();
    set.insert(TileKey::new(id, Revision(1)));
    set.insert(TileKey::new(id, Revision(1)));
    set.insert(TileKey::new(id, Revision(2)));
    assert_eq!(set.len(), 2);
}

#[test]
fn cache_lru_evict_et_rend_le_handle() {
    let mut cache = CpuTileCache::new(2);
    let mk = |v: u8| CpuTile {
        data: vec![v; 4].into(),
        width: 1,
        height: 1,
    };
    let (a, b, c) = (
        TileId::new(0, 0, 0),
        TileId::new(0, 1, 0),
        TileId::new(0, 2, 0),
    );
    assert!(cache.insert(a, Revision(1), mk(1)).is_none());
    assert!(cache.insert(b, Revision(1), mk(2)).is_none());
    // Touch A (recent), insert C: B, the least recent, is handed back.
    assert!(cache.get(a, Revision(1)).is_some());
    let evicted = cache.insert(c, Revision(1), mk(3)).expect("évincé");
    assert_eq!(&evicted.data[..], &[2, 2, 2, 2]);
    assert_eq!(cache.len(), 2);
    assert!(cache.get(b, Revision(1)).is_none());
    assert_eq!(cache.stats().evicted, 1);
    // Invalidation hands the handle back (the backend frees its texture).
    let dropped = cache.invalidate(a).expect("présent");
    assert_eq!(&dropped.data[..], &[1, 1, 1, 1]);
    assert!(!cache.invalidate(a).is_some());
}

#[test]
fn poignees_cpu_gpu_separees_par_construction() {
    // The GPU only handles tokens: no textures here.
    let mut gpu = GpuTileCache::new(4);
    let id = TileId::new(0, 0, 0);
    assert!(
        gpu.insert(
            id,
            Revision(3),
            GpuTile {
                token: 42,
                generation: 1
            }
        )
        .is_none()
    );
    assert_eq!(gpu.get(id, Revision(3)).expect("hit").token, 42);
    // Stale generation = miss (backend-side re-upload).
    assert!(
        gpu.insert(
            id,
            Revision(4),
            GpuTile {
                token: 43,
                generation: 2
            }
        )
        .is_none()
    );
    assert!(gpu.get(id, Revision(3)).is_none());
    assert!(gpu.get(id, Revision(4)).is_some());
}

#[test]
fn scheduler_visibles_puis_proches_puis_grossieres() {
    let grid = TileGrid::new(256, 1024, 1024, 3);
    let mut dirty = DirtyTiles::new(&grid);
    // Dirty everywhere at levels 0 and 2.
    dirty.mark_range(grid.tiles_for_rect(&TileRegion::new(0, 0, 1024, 1024), 0));
    dirty.mark_range(grid.tiles_for_rect(&TileRegion::new(0, 0, 1024, 1024), 2));
    let view = Viewport::new(TileRegion::new(0, 0, 256, 256), (128, 128));
    let plan = TileScheduler.plan(&dirty, &grid, &view, None);
    assert_eq!(plan.len() as u64, dirty.dirty_count());
    // 1) Visible (0,0 at level 0) comes before anything off-screen.
    let pos = |id| plan.iter().position(|&n| n == id).expect("planifié");
    let visible = TileId::new(0, 0, 0);
    let offscreen = TileId::new(0, 3, 3);
    assert!(pos(visible) < pos(offscreen));
    // 2) At equal visibility, nearest the focus first.
    let near = TileId::new(0, 1, 0);
    let far = TileId::new(0, 3, 0);
    assert!(pos(near) < pos(far));
    // 3) Low resolution before high, at equal visibility and distance:
    // all visible, focus equidistant from (0,0)@L0 and (0,0)@L2.
    let wide = Viewport::new(TileRegion::new(0, 0, 1024, 1024), (320, 320));
    let plan = TileScheduler.plan(&dirty, &grid, &wide, None);
    let pos = |id| plan.iter().position(|&n| n == id).expect("planifié");
    assert!(pos(TileId::new(2, 0, 0)) < pos(TileId::new(0, 0, 0)));
}

#[test]
fn scheduler_hors_ecran_en_dernier_et_budget_progressif() {
    let grid = TileGrid::new(256, 1024, 1024, 1);
    let mut dirty = DirtyTiles::new(&grid);
    dirty.mark_range(grid.tiles_for_rect(&TileRegion::new(0, 0, 1024, 1024), 0));
    // Window on the first tile only.
    let view = Viewport::new(TileRegion::new(0, 0, 256, 256), (0, 0));
    let total = dirty.dirty_count();
    assert_eq!(total, 16);
    // Progressive execution in budgets of 5: 5 + 5 + 5 + 1.
    let mut out = Vec::new();
    let mut rounds = 0;
    while !dirty.is_empty() {
        TileScheduler.plan_into(&dirty, &grid, &view, Some(5), &mut out);
        assert!(!out.is_empty() && out.len() <= 5);
        for id in out.drain(..) {
            assert!(dirty.clear(id), "chaque planifiée est sale");
        }
        rounds += 1;
    }
    assert_eq!(rounds, 4);
    // The first scheduled tile is the visible one.
    let mut dirty2 = DirtyTiles::new(&grid);
    dirty2.mark_range(grid.tiles_for_rect(&TileRegion::new(0, 0, 1024, 1024), 0));
    let first = TileScheduler.plan(&dirty2, &grid, &view, Some(1));
    assert_eq!(first, vec![TileId::new(0, 0, 0)]);
}

#[test]
fn tuile_mip_couvre_le_bon_rectangle() {
    let grid = TileGrid::new(256, 1024, 1024, 3);
    // Level 1: 512×512 surface → tile (1,0) = x ∈ [256,512), y ∈ [0,256).
    assert_eq!(grid.tile_count(1), (2, 2));
    assert_eq!(
        grid.tile_rect(TileId::new(1, 1, 0)),
        TileRegion::new(256, 0, 256, 256)
    );
    // Level 1 → L0: ×2.
    assert_eq!(
        grid.tile_rect_l0(TileId::new(1, 1, 0)),
        TileRegion::new(512, 0, 512, 512)
    );
    // Conservative mip invalidation: 1 level-0 px touches one level-2 tile.
    let range = grid.tiles_for_rect(&TileRegion::new(900, 900, 1, 1), 2);
    assert_eq!(range.count(), 1);
    assert!(range.to_region(&grid).contains(900, 900));
}

#[test]
fn etendues_non_carrees_et_coordonnees() {
    let grid = TileGrid::with_extent(
        TileExtent {
            width: 128,
            height: 64,
        },
        256,
        128,
        1,
    );
    assert_eq!(grid.tile_count(0), (2, 2));
    assert_eq!(
        grid.tile_rect(TileId::new(0, 1, 1)),
        TileRegion::new(128, 64, 128, 64)
    );
    assert_eq!(TileId::new(0, 1, 1).coord(), TileCoord::new(1, 1));
    let range = TileRange::single(TileId::new(0, 0, 0));
    assert!(range.contains(TileCoord::new(0, 0), 0));
    assert!(!range.contains(TileCoord::new(1, 0), 0));
    assert!(!range.contains(TileCoord::new(0, 0), 1));
}

#[test]
fn serialisation_modele_aller_retour() {
    let id = TileId::new(2, -3, 7);
    let json = serde_json::to_string(&id).expect("serializable");
    assert_eq!(serde_json::from_str::<TileId>(&json).expect("ok"), id);
    let key = TileKey::new(id, Revision(9));
    let json = serde_json::to_string(&key).expect("serializable");
    assert_eq!(serde_json::from_str::<TileKey>(&json).expect("ok"), key);
    let region = TileRegion::new(-10, 5, 100, 200);
    let json = serde_json::to_string(&region).expect("serializable");
    assert_eq!(
        serde_json::from_str::<TileRegion>(&json).expect("ok"),
        region
    );
}
