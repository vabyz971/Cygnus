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

//! Modèle spatial : coordonnées, rectangles, identités, padding.
//!
//! Tout est entier et `Copy` : les conversions rect ↔ tuiles sont exactes
//! et les clés de cache se comparent par valeur, sans allocation.

use ids::Revision;

/// Révision d'une tuile : le compteur partagé [`Revision`].
///
/// Même tuile + même révision = contenu identique (cache hit). Toute
/// salissure avance la révision du nœud source — jamais ici directement.
pub type TileRevision = Revision;

/// Coordonnée de tuile dans la grille (indices, signés : les calques
/// peuvent dépasser du document vers le négatif).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct TileCoord {
    /// Colonne (x).
    pub x: i32,
    /// Ligne (y).
    pub y: i32,
}

impl TileCoord {
    /// Nouvelle coordonnée.
    #[must_use]
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Dimensions d'une tuile en pixels (non carrées admises).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileExtent {
    /// Largeur en pixels (> 0).
    pub width: u32,
    /// Hauteur en pixels (> 0).
    pub height: u32,
}

impl TileExtent {
    /// Carré `size × size` (`size` borné à ≥ 1).
    #[must_use]
    pub fn square(size: u32) -> Self {
        let size = size.max(1);
        Self {
            width: size,
            height: size,
        }
    }
}

/// Identifiant stable d'un emplacement de tuile : niveau + coordonnée.
///
/// L'identité NE contient PAS la révision (voir [`TileKey`]) : le même
/// emplacement est réutilisé d'une révision à l'autre — c'est le principe
/// du cache.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct TileId {
    /// Niveau mip (0 = pleine résolution, +1 = divisé par 2).
    pub level: u8,
    /// Colonne.
    pub x: i32,
    /// Ligne.
    pub y: i32,
}

impl TileId {
    /// Nouvel identifiant.
    #[must_use]
    pub fn new(level: u8, x: i32, y: i32) -> Self {
        Self { level, x, y }
    }

    /// Coordonnée (sans le niveau).
    #[must_use]
    pub fn coord(self) -> TileCoord {
        TileCoord::new(self.x, self.y)
    }
}

impl std::fmt::Display for TileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "t{}:{}:{}", self.level, self.x, self.y)
    }
}

/// Clé de cache : emplacement + révision du contenu.
///
/// Même `TileId`, révision différente = contenus potentiellement
/// différents (miss). La révision vient du nœud source (render graph),
/// jamais devinée ici.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileKey {
    /// Emplacement.
    pub id: TileId,
    /// Révision du contenu attendu.
    pub revision: TileRevision,
}

impl TileKey {
    /// Nouvelle clé.
    #[must_use]
    pub fn new(id: TileId, revision: TileRevision) -> Self {
        Self { id, revision }
    }
}

/// Rectangle de pixels en coordonnées surface (origine signée, bornes
/// exclusives `[x, x+w) × [y, y+h)`). Vide ssi `width == 0 || height == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileRegion {
    /// Origine X (peut être négative : contenu hors document).
    pub x: i32,
    /// Origine Y.
    pub y: i32,
    /// Largeur en pixels.
    pub width: u32,
    /// Hauteur en pixels.
    pub height: u32,
}

impl TileRegion {
    /// Région vide (ne couvre aucun pixel, aucune tuile).
    pub const EMPTY: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// Nouveau rectangle.
    #[must_use]
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Vrai si aucun pixel couvert.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Aire en pixels (0 si vide).
    #[must_use]
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Borne droite exclusive (i64 : pas de dépassement).
    #[must_use]
    pub fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    /// Borne basse exclusive.
    #[must_use]
    pub fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    /// Construit depuis des bornes exclusives (vide si inversées).
    fn from_bounds(x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let x0 = x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        let y0 = y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        let x1 = x1.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        let y1 = y1.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        if x1 <= x0 || y1 <= y0 {
            return Self::EMPTY;
        }
        Self {
            x: x0 as i32,
            y: y0 as i32,
            width: (x1 - x0).min(i64::from(u32::MAX)) as u32,
            height: (y1 - y0).min(i64::from(u32::MAX)) as u32,
        }
    }

    /// Vrai si le point (px) est couvert.
    #[must_use]
    pub fn contains(self, x: i32, y: i32) -> bool {
        !self.is_empty()
            && x >= self.x
            && y >= self.y
            && i64::from(x) < self.right()
            && i64::from(y) < self.bottom()
    }

    /// Intersection (clipping) — vide si disjoints.
    #[must_use]
    pub fn intersect(self, bounds: Self) -> Self {
        Self::from_bounds(
            i64::from(self.x).max(i64::from(bounds.x)),
            i64::from(self.y).max(i64::from(bounds.y)),
            self.right().min(bounds.right()),
            self.bottom().min(bounds.bottom()),
        )
    }

    /// Union (englobant) — vide + r = r.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self::from_bounds(
            i64::from(self.x).min(i64::from(other.x)),
            i64::from(self.y).min(i64::from(other.y)),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    /// Élargit de `pad` pixels dans chaque direction (saturé i32).
    #[must_use]
    pub fn pad(self, pad: Padding) -> Self {
        if self.is_empty() || pad.0 == 0 {
            return self;
        }
        let p = i64::from(pad.0);
        Self::from_bounds(
            i64::from(self.x) - p,
            i64::from(self.y) - p,
            self.right() + p,
            self.bottom() + p,
        )
    }

    /// Réduit à l'échelle du niveau mip de façon CONSERVATIVE : le
    /// rectangle englobant couvre tous les pixels sources (`floor` en min,
    /// `ceil` en max). Un pixel sale reste couvert à tout niveau — à
    /// utiliser pour invalidation et visibilité, jamais l'inverse.
    #[must_use]
    pub fn downscaled_cover(self, level: u8) -> Self {
        if self.is_empty() || level == 0 {
            return self;
        }
        let shift = u32::from(level.min(31));
        let step = 1i64 << shift;
        let x0 = (i64::from(self.x)) >> shift;
        let y0 = (i64::from(self.y)) >> shift;
        let x1 = div_ceil(self.right(), step);
        let y1 = div_ceil(self.bottom(), step);
        Self {
            x: x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y: y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            width: (x1 - x0).max(0) as u32,
            height: (y1 - y0).max(0) as u32,
        }
    }
}

/// Plafond de division entière (diviseur > 0).
fn div_ceil(a: i64, b: i64) -> i64 {
    debug_assert!(b > 0);
    if a >= 0 { (a + b - 1) / b } else { a / b }
}

/// Marge d'invalidation en pixels : les effets à voisinage (flou, bloom…)
/// lisent AU-DELÀ de la zone modifiée — on invalide donc plus large que le
/// rectangle source (voir [`TileRegion::pad`]).
///
/// Prépare la logique sans implémenter les filtres : chaque effet déclarera
/// son rayon, converti ici en marge entière (plafond).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Padding(pub u32);

impl Padding {
    /// Aucune marge.
    pub const ZERO: Self = Self(0);

    /// Marge explicite en pixels.
    #[must_use]
    pub fn new(px: u32) -> Self {
        Self(px)
    }

    /// Marge pour un rayon de flou en pixels (plafond ; ≤ 0 ou NaN → 0).
    ///
    /// Exemple : `radius = 20.0` ⇒ `Padding(20)` — une retouche dans une
    /// zone invalide la zone élargie de 20 px (le noyau échantillonne le
    /// voisinage modifié).
    #[must_use]
    pub fn for_blur_radius(radius: f32) -> Self {
        if !radius.is_finite() || radius <= 0.0 {
            return Self::ZERO;
        }
        Self(radius.ceil() as u32)
    }

    /// Marge pour un rayon générique d'effet à voisinage (même règle).
    #[must_use]
    pub fn for_effect_radius(radius: f32) -> Self {
        Self::for_blur_radius(radius)
    }
}
