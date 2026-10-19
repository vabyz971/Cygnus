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

//! Tile Scheduler: progressive execution order of dirty tiles.
//!
//! Priorities (in this order):
//!
//! 1. visible tiles;
//! 2. near the interaction point (focus);
//! 3. low resolution (coarse mips first — fast preview);
//! 4. high resolution (refinement);
//! 5. off-screen (last).
//!
//! The scheduler is STATELESS and pure: it sorts dirty tiles into the caller's
//! `Vec` ([`TileScheduler::plan_into`], allocation reused across
//! frames) then truncates to the budget. Progressive execution belongs to
//! the caller: schedule → execute → clean the done ones → repeat
//! until the set is empty.

use crate::{DirtyTiles, TileGrid, TileId, TileRegion};

/// Window of interest: visible area + interaction point, in level-0
/// pixels (same space as the surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    /// Visible area (level-0 pixels).
    pub visible: TileRegion,
    /// Interaction point (cursor, gesture center — level-0 pixels).
    pub focus: (i32, i32),
}

impl Viewport {
    /// New window.
    #[must_use]
    pub fn new(visible: TileRegion, focus: (i32, i32)) -> Self {
        Self { visible, focus }
    }
}

/// Dirty-tile scheduler (pure, stateless).
pub struct TileScheduler;

impl TileScheduler {
    /// Sorts dirty tiles into `out` (cleared first) then truncates to the budget
    /// (`None` = all). The `Vec` is reused across calls:
    /// no per-frame allocation at steady state.
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

    /// Fresh plan (allocates — prefer `plan_into` in the render loop).
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

    /// Priority comparator (visible, near, coarse first).
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

/// Do two regions overlap (empty ones = never)?
fn overlaps(a: &TileRegion, b: &TileRegion) -> bool {
    !a.intersect(*b).is_empty()
}

/// Squared distance (integer, no NaN) from the rectangle center to the focus.
fn dist2(rect: TileRegion, focus: (i32, i32)) -> i64 {
    let cx = i64::from(rect.x) + i64::from(rect.width) / 2;
    let cy = i64::from(rect.y) + i64::from(rect.height) / 2;
    let dx = cx - i64::from(focus.0);
    let dy = cy - i64::from(focus.1);
    dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
}
