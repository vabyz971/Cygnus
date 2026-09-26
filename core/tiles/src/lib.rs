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

//! Invalidation spatiale et Tile Scheduler — optimisation du rendu.
//!
//! ```text
//! Scene Graph → Render Graph → invalidate(rect) → DirtyTiles
//!                                          ↓
//!                              TileScheduler::plan (visible, proche,
//!                                basse-résolution d'abord, progressif)
//!                                          ↓
//!                              Backend (CPU aujourd'hui, GPU demain)
//! ```
//!
//! Règles de cette crate :
//!
//! - Les tuiles sont une STRATÉGIE D'EXÉCUTION, pas un modèle de document :
//!   aucune hiérarchie, aucune sémantique — seulement des rectangles
//!   entiers, des révisions et des priorités.
//! - Arithmétique entière EXACTE (`i32`/`u32`, pas de `f32`) : l'appartenance
//!   d'un pixel à une tuile ne tolère aucun flou d'arrondi (d'où un modèle
//!   propre au lieu de réutiliser `datatypes::Vec2`, géométrie UI flottante).
//! - Zéro allocation sur les chemins chauds : `invalidate` retourne une
//!   [`TileRange`] (bornes, pas de `Vec`), [`DirtyTiles`] est un bitset
//!   dense (pas de `HashMap`), la planification réutilise le `Vec`
//!   de l'appelant (`plan_into`).
//! - AUCUNE donnée GPU ici : [`GpuTile`] est un jeton opaque résolu par le
//!   backend (zéro readback, zéro synchronisation CPU/GPU dans cette crate).
//! - `tiles::TileId` (tuile de rendu) n'a aucun rapport avec
//!   `egui_tiles::TileId` (panneau dockable de l'app photo).
//!
//! # Examples
//!
//! ```
//! use tiles::{Padding, TileGrid, TileRegion};
//!
//! let grid = TileGrid::new(256, 1920, 1080, 1);
//! // Coup de pinceau : bbox + rayon, élargie du padding, puis tuiles.
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
