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

//! Dirty region (Phase 6D): WHICH zones were modified and need
//! rebuilding.
//!
//! [`DirtyRegion`] does NO compositing and knows no filter: a list of
//! rectangles in DOCUMENT SPACE (the same space as regions sent to
//! `TileGrid` / `composite_region_with`).
//!
//! Strict separation (central 6D rule):
//!
//! - `SpatialScope` (Phase 6C, `compositing.rs`): to produce R, which
//!   region D to compute (`D = R`, `D = R + halo`, `D = full`);
//! - [`DirtyRegion`] (here): which zones are modified;
//! - `TileCache` (`tile_cache.rs`): reuse the final result of R.
//!
//! The `D = R + halo` logic does NOT live here: the caller resolves it
//! via `scope_adjustment_window` (Phase 6C) and passes rectangles
//! already expressed as invalidation zones (see
//! [`TileCache::mark_dirty_scope`](crate::document::tile_cache::TileCache::mark_dirty_scope)).
//!
//! Contract (Phase 6G.2 §1): after producing the result, the caller
//! consumes EXACTLY the rendered zones via [`DirtyRegion::consume`] —
//! never before (dirty only clears once the result exists), never more
//! (unrendered zones stay dirty). Total `clear` remains for resets.
//! Without consumption, requests stay dirty (conservative re-render,
//! never wrong pixels).

use tiles::TileRegion;

/// Invalidated zones, in DOCUMENT SPACE.
///
/// Deliberately simple representation (rectangle list, no bitset /
/// quadtree / BVH): correctness first, optimization later. Rectangles
/// may overlap; `clear` starts over from zero.
#[derive(Debug, Clone, Default)]
pub struct DirtyRegion {
    rects: Vec<TileRegion>,
}

impl DirtyRegion {
    /// Empty region (nothing to rebuild).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an invalidated zone (empty rectangles are ignored).
    pub fn add(&mut self, rect: TileRegion) {
        if !rect.is_empty() {
            self.rects.push(rect);
        }
    }

    /// Forgets all zones (reset, perimeter change).
    pub fn clear(&mut self) {
        self.rects.clear();
    }

    /// Consumes the actually rendered zones (Phase 6G.2 §1): subtracts each
    /// rendered rect from the dirty zones (exact subtraction, ≤ 4 pieces
    /// per rect). Returns the number of fully settled dirty zones.
    ///
    /// Exactly what was produced disappears, nothing more: a zone
    /// straddling a rendered tile and another stays dirty on its unrendered
    /// part (targeted re-render next frame, never wrong pixels, never
    /// useless re-renders).
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

    /// True if no invalidated zone.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// Number of tracked rectangles (observability, no effect).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// True if `rect` intersects at least one invalidated zone. It is the
    ///
    /// only test used by `TileCache`: tile R is reusable iff `!dirty.intersects(R)`
    /// (AND content guard OK).
    #[must_use]
    pub fn intersects(&self, rect: TileRegion) -> bool {
        if rect.is_empty() {
            return false;
        }
        self.rects.iter().any(|d| !d.intersect(rect).is_empty())
    }

    /// Bounding union of the zones (`EMPTY` if empty) — to choose what to
    /// re-render in one pass.
    #[must_use]
    pub fn bounds(&self) -> TileRegion {
        self.rects
            .iter()
            .fold(TileRegion::EMPTY, |acc, r| acc.union(*r))
    }

    /// Iteration over zones (assembly, diagnostics).
    #[must_use]
    pub fn rects(&self) -> &[TileRegion] {
        &self.rects
    }
}

/// Subtracts `cut` from `rect` (exact integer geometry, ≤ 4 non-empty
/// pieces: left/right bands over the full height + top/bottom bands over
/// the remaining width). `cut` outside `rect` ⇒ `[rect]`; total overlap
/// ⇒ `[]`.
fn subtract_rect(rect: TileRegion, cut: TileRegion) -> Vec<TileRegion> {
    let cut = rect.intersect(cut);
    if cut.is_empty() {
        return vec![rect];
    }
    if cut == rect {
        return Vec::new();
    }
    // i64 bounds (no overflow on i32); saturated widths.
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
        // Partial render: a right band remains.
        assert_eq!(d.consume(&[TileRegion::new(0, 0, 40, 100)]), 0);
        assert_eq!(d.rects(), &[TileRegion::new(40, 0, 60, 100)]);
        // Total overlap: settled.
        assert_eq!(d.consume(&[TileRegion::new(0, 0, 100, 100)]), 1);
        assert!(d.is_empty());
        // Outside the zone: intact.
        d.add(TileRegion::new(0, 0, 10, 10));
        assert_eq!(d.consume(&[TileRegion::new(50, 50, 10, 10)]), 0);
        assert_eq!(d.len(), 1);
        // Center hole: 4 pieces, area preserved.
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
