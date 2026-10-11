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

//! Spatial invalidation and Tile Scheduler — render optimization.
//!
//! ```text
//! Scene Graph → Render Graph → invalidate(rect) → DirtyTiles
//!                                          ↓
//!                              TileScheduler::plan (visible, near,
//!                                low-resolution first, progressive)
//!                                          ↓
//!                              Backend (CPU today, GPU tomorrow)
//! ```
//!
//! Rules of this crate:
//!
//! - Tiles are an EXECUTION STRATEGY, not a document model:
//!   no hierarchy, no semantics — only integer
//!   rectangles, revisions and priorities.
//! - EXACT integer arithmetic (`i32`/`u32`, no `f32`): a pixel's tile
//!   membership tolerates no rounding fuzz (hence a dedicated
//!   model instead of reusing `datatypes::Vec2`, floating UI geometry).
//! - Zero allocation on hot paths: `invalidate` returns a
//!   [`TileRange`] (bounds, no `Vec`), [`DirtyTiles`] is a dense
//!   bitset (no `HashMap`), scheduling reuses the caller's
//!   `Vec` (`plan_into`).
//! - NO GPU data here: [`GpuTile`] is an opaque token resolved by the
//!   backend (zero readback, zero CPU/GPU sync in this crate).
//! - `tiles::TileId` (render tile) is unrelated to
//!   `egui_tiles::TileId` (photo app dock panel).
//!
//! # Examples
//!
//! ```
//! use tiles::{Padding, TileGrid, TileRegion};
//!
//! let grid = TileGrid::new(256, 1920, 1080, 1);
//! // Brush stroke: bbox + radius, padded, then tiles.
//! let stroke = TileRegion::new(100, 100, 50, 50).pad(Padding::new(8));
//! let range = grid.invalidate(&stroke, 0, Padding::new(0));
//! assert!(!range.is_empty());
//! ```

pub mod cache;
pub mod dirty;
pub mod grid;
pub mod model;
pub mod scheduler;

#[cfg(test)]
mod perf;
#[cfg(test)]
mod tests;

pub use cache::{CacheStats, CpuTile, CpuTileCache, GpuTile, GpuTileCache, TileCache};
pub use dirty::DirtyTiles;
pub use grid::{TileGrid, TileRange};
pub use ids::Revision;
pub use model::{Padding, TileCoord, TileExtent, TileId, TileKey, TileRegion, TileRevision};
pub use scheduler::{TileScheduler, Viewport};
