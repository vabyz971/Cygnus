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

//! Mesures ciblées (harnais std : `criterion` indisponible hors-ligne).
//!
//! Pas d'assertions temporelles (flakiness CI) : ces tests IMPRIMENT le
//! débit (`-- --nocapture`) et verrouillent le COMPORTEMENT (comptes
//! exacts). Pour brancher `criterion` plus tard : ajouter la dev-dependency,
//! déplacer chaque `perf_*` en `benches/*.rs` avec `criterion::black_box`
//! (mêmes scénarios, mêmes invariants).

use super::*;
use std::hint::black_box;
use std::time::Instant;

fn grille_4k() -> TileGrid {
    TileGrid::new(256, 3840, 2160, 3)
}

#[test]
fn perf_invalidate_sans_allocation() {
    let grid = grille_4k();
    let rect = TileRegion::new(1000, 500, 300, 200);
    let pad = Padding::new(32);
    let t0 = Instant::now();
    let mut total = 0u64;
    for _ in 0..20_000 {
        let range = grid.invalidate(black_box(&rect), 0, black_box(pad));
        total += black_box(range.count());
    }
    let us = t0.elapsed().as_micros();
    eprintln!("perf invalidate: {us} µs / 20k (total {total})");
    assert_eq!(total, 20_000 * grid.invalidate(&rect, 0, pad).count());
}

#[test]
fn perf_mark_range_bitset() {
    let grid = grille_4k();
    let range = grid.invalidate(&TileRegion::new(0, 0, 3840, 2160), 0, Padding::ZERO);
    let t0 = Instant::now();
    for _ in 0..500 {
        let mut dirty = DirtyTiles::new(&grid);
        black_box(dirty.mark_range(black_box(range)));
        dirty.clear_all();
        black_box(dirty.dirty_count());
    }
    let us = t0.elapsed().as_micros();
    eprintln!(
        "perf mark_range 4k: {us} µs / 500 ({} tuiles)",
        range.count()
    );
    assert_eq!(range.count(), 15 * 9);
}

#[test]
fn perf_planification_4k_sale() {
    let grid = grille_4k();
    let mut dirty = DirtyTiles::new(&grid);
    dirty.mark_range(grid.invalidate(&TileRegion::new(0, 0, 3840, 2160), 0, Padding::ZERO));
    let view = Viewport::new(TileRegion::new(0, 0, 1920, 1080), (960, 540));
    let mut out = Vec::new();
    let t0 = Instant::now();
    for _ in 0..200 {
        TileScheduler.plan_into(black_box(&dirty), &grid, &view, None, &mut out);
        black_box(out.len());
    }
    let us = t0.elapsed().as_micros();
    eprintln!("perf plan 135 tuiles: {us} µs / 200");
    assert_eq!(out.len() as u64, dirty.dirty_count());
}
