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

//! Grille de tuilage : conversions rect ↔ tuiles, clipping, invalidation.
//!
//! [`TileGrid`] décrit une surface (dimensions + taille de tuile + niveaux
//! mip) ; [`TileGrid::invalidate`] est le point d'entrée
//! `rect → tuiles affectées` (région élargie du padding, clippée à la
//! surface) ; [`TileRange`] est le résultat SANS allocation (bornes
//! inclusives + itérateur paresseux) ; [`TileRange::to_region`] fait le
//! chemin inverse `tuiles → rectangle`.

use crate::{Padding, TileCoord, TileExtent, TileId, TileRegion};

/// Grille de tuilage d'une surface (un niveau = une grille `taille >> level`).
///
/// Niveau 0 = pleine résolution ; chaque niveau divise les dimensions par 2
/// (plancher 1 px). Les tuiles hors surface (coordonnées négatives ou
/// au-delà) existent pour les contenus débordants mais sont clippées à
/// l'invalidation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TileGrid {
    extent: TileExtent,
    width: u32,
    height: u32,
    levels: u8,
}

impl TileGrid {
    /// Nouvelle grille (`levels` borné à ≥ 1 ; taille de tuile ≥ 1).
    #[must_use]
    pub fn new(tile: u32, width: u32, height: u32, levels: u8) -> Self {
        Self::with_extent(TileExtent::square(tile), width, height, levels)
    }

    /// Grille à tuiles non carrées.
    #[must_use]
    pub fn with_extent(extent: TileExtent, width: u32, height: u32, levels: u8) -> Self {
        Self {
            extent: TileExtent::square(1).merge_max(extent),
            width,
            height,
            levels: levels.max(1),
        }
    }

    /// Taille de tuile (niveau 0).
    #[must_use]
    pub fn extent(&self) -> TileExtent {
        self.extent
    }

    /// Dimensions de la surface (niveau 0).
    #[must_use]
    pub fn surface(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Nombre de niveaux mip.
    #[must_use]
    pub fn level_count(&self) -> u8 {
        self.levels
    }

    /// Dimensions au niveau (`>> level`, plancher 1).
    #[must_use]
    pub fn level_dims(&self, level: u8) -> (u32, u32) {
        let shift = u32::from(level.min(31));
        ((self.width >> shift).max(1), (self.height >> shift).max(1))
    }

    /// Nombre de tuiles (colonnes, lignes) au niveau (plafond).
    #[must_use]
    pub fn tile_count(&self, level: u8) -> (u32, u32) {
        let (w, h) = self.level_dims(level);
        (
            w.div_ceil(self.extent.width),
            h.div_ceil(self.extent.height),
        )
    }

    /// La tuile couvre-t-elle au moins un pixel de la surface ?
    #[must_use]
    pub fn covers_surface(&self, id: TileId) -> bool {
        let (nx, ny) = self.tile_count(id.level);
        id.level < self.levels && id.x >= 0 && id.y >= 0 && (id.x as u32) < nx && (id.y as u32) < ny
    }

    /// Rectangle de pixels d'une tuile, clippé à la surface du niveau
    /// (vide si la tuile est entièrement hors surface).
    #[must_use]
    pub fn tile_rect(&self, id: TileId) -> TileRegion {
        let (w, h) = self.level_dims(id.level);
        let ew = i64::from(self.extent.width);
        let eh = i64::from(self.extent.height);
        let x0 = i64::from(id.x) * ew;
        let y0 = i64::from(id.y) * eh;
        TileRegion::new(0, 0, w, h).intersect(TileRegion::new(
            x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            ew as u32,
            eh as u32,
        ))
    }

    /// Rectangle d'une tuile exprimé en pixels niveau 0 (pour comparer au
    /// viewport, toujours défini au niveau 0).
    #[must_use]
    pub fn tile_rect_l0(&self, id: TileId) -> TileRegion {
        let rect = self.tile_rect(id);
        if rect.is_empty() {
            return rect;
        }
        let shift = u32::from(id.level.min(31));
        let (w, h) = self.level_dims(0);
        TileRegion::new(0, 0, w, h).intersect(TileRegion::new(
            rect.x.checked_shl(shift).unwrap_or(i32::MAX),
            rect.y.checked_shl(shift).unwrap_or(i32::MAX),
            rect.width.checked_shl(shift).unwrap_or(u32::MAX),
            rect.height.checked_shl(shift).unwrap_or(u32::MAX),
        ))
    }

    /// Tuiles couvrant `region` (pixels niveau 0) au niveau (clippées,
    /// sans allocation). Région vide ou niveau inconnu ⇒ plage vide.
    /// La couverture est conservative : tout pixel de la région touche
    /// une tuile retournée.
    #[must_use]
    pub fn tiles_for_rect(&self, region: &TileRegion, level: u8) -> TileRange {
        if region.is_empty() || level >= self.levels {
            return TileRange::empty(level);
        }
        let (w, h) = self.level_dims(level);
        let clipped = region
            .downscaled_cover(level)
            .intersect(TileRegion::new(0, 0, w, h));
        if clipped.is_empty() {
            return TileRange::empty(level);
        }
        let ew = i64::from(self.extent.width);
        let eh = i64::from(self.extent.height);
        let x0 = div_floor(i64::from(clipped.x), ew);
        let y0 = div_floor(i64::from(clipped.y), eh);
        let x1 = div_floor(clipped.right() - 1, ew);
        let y1 = div_floor(clipped.bottom() - 1, eh);
        TileRange::new(level, x0, y0, x1, y1)
    }

    /// `invalidate(rect)` : élargit du padding, clippe, convertit en tuiles.
    /// Point d'entrée du pipeline pinceau → bbox → expansion → tuiles.
    #[must_use]
    pub fn invalidate(&self, region: &TileRegion, level: u8, pad: Padding) -> TileRange {
        self.tiles_for_rect(&region.pad(pad), level)
    }
}

/// Division entière plancher (les coordonnées peuvent être négatives).
fn div_floor(a: i64, b: i64) -> i32 {
    debug_assert!(b > 0);
    let q = a / b;
    let r = a % b;
    let adj = i64::from((r != 0) && ((r < 0) != (b < 0)));
    (q - adj).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl TileExtent {
    /// Max par composante (bornes internes de la grille).
    fn merge_max(self, other: Self) -> Self {
        Self {
            width: self.width.max(other.width),
            height: self.height.max(other.height),
        }
    }
}

/// Plage de tuiles : bornes INCLUSIVES + niveau, SANS allocation.
///
/// Vide ssi `x0 > x1 || y0 > y1`. `iter()` énumère en lignes (déterministe),
/// `count()` est O(1), `to_region()` fait le chemin inverse vers le
/// rectangle englobant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileRange {
    /// Niveau mip.
    pub level: u8,
    /// Colonne min (inclusive).
    pub x0: i32,
    /// Ligne min (inclusive).
    pub y0: i32,
    /// Colonne max (inclusive).
    pub x1: i32,
    /// Ligne max (inclusive).
    pub y1: i32,
}

impl TileRange {
    /// Plage vide (aucune tuile, niveau conservé pour le typage).
    #[must_use]
    pub fn empty(level: u8) -> Self {
        Self {
            level,
            x0: 0,
            y0: 0,
            x1: -1,
            y1: -1,
        }
    }

    /// Plage bornée (vide si bornes inversées).
    #[must_use]
    pub fn new(level: u8, x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self {
            level,
            x0,
            y0,
            x1,
            y1,
        }
    }

    /// Une seule tuile.
    #[must_use]
    pub fn single(id: TileId) -> Self {
        Self::new(id.level, id.x, id.y, id.x, id.y)
    }

    /// Vrai si aucune tuile.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.x0 > self.x1 || self.y0 > self.y1
    }

    /// Nombre de tuiles (0 si vide).
    #[must_use]
    pub fn count(self) -> u64 {
        if self.is_empty() {
            return 0;
        }
        (self.x1 as i64 - self.x0 as i64 + 1) as u64 * (self.y1 as i64 - self.y0 as i64 + 1) as u64
    }

    /// La plage contient-elle la coordonnée (même niveau requis) ?
    #[must_use]
    pub fn contains(self, coord: TileCoord, level: u8) -> bool {
        self.level == level
            && !self.is_empty()
            && coord.x >= self.x0
            && coord.x <= self.x1
            && coord.y >= self.y0
            && coord.y <= self.y1
    }

    /// Itérateur paresseux en lignes (aucune allocation).
    #[must_use]
    pub fn iter(self) -> TileRangeIter {
        TileRangeIter {
            range: self,
            x: self.x0,
            y: self.y0,
        }
    }

    /// Chemin inverse : rectangle englobant des tuiles (pixels niveau 0).
    #[must_use]
    pub fn to_region(self, grid: &TileGrid) -> TileRegion {
        if self.is_empty() {
            return TileRegion::EMPTY;
        }
        let mut region = TileRegion::EMPTY;
        for coord in self.iter() {
            region = region.union(grid.tile_rect_l0(TileId::new(self.level, coord.x, coord.y)));
        }
        region
    }
}

/// Itérateur de [`TileRange`] (lignes, sans allocation).
#[derive(Debug, Clone, Copy)]
pub struct TileRangeIter {
    range: TileRange,
    x: i32,
    y: i32,
}

impl Iterator for TileRangeIter {
    type Item = TileCoord;

    fn next(&mut self) -> Option<TileCoord> {
        if self.y > self.range.y1 {
            return None;
        }
        let coord = TileCoord::new(self.x, self.y);
        self.x += 1;
        if self.x > self.range.x1 {
            self.x = self.range.x0;
            self.y += 1;
        }
        Some(coord)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.y > self.range.y1 {
            return (0, Some(0));
        }
        let rows_left = (self.range.y1 - self.y) as usize;
        let in_row = (self.range.x1 - self.x + 1).max(0) as usize;
        let rest_rows = rows_left * (self.range.x1 - self.range.x0 + 1).max(0) as usize;
        let n = in_row + rest_rows;
        (n, Some(n))
    }
}

impl ExactSizeIterator for TileRangeIter {}
