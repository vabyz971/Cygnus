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

//! Spatial model: coordinates, rectangles, identities, padding.
//!
//! Everything is integer and `Copy`: rect ↔ tile conversions are exact
//! and cache keys compare by value, allocation-free.

use ids::Revision;

/// Revision of a tile: the shared [`Revision`] counter.
///
/// Same tile + same revision = identical content (cache hit). Any
/// dirtying advances the source node's revision — never here directly.
pub type TileRevision = Revision;

/// Tile coordinate in the grid (signed indices: layers
/// may overflow the document toward the negative).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct TileCoord {
    /// Column (x).
    pub x: i32,
    /// Row (y).
    pub y: i32,
}

impl TileCoord {
    /// New coordinate.
    #[must_use]
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Tile dimensions in pixels (non-square allowed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileExtent {
    /// Largeur en pixels (> 0).
    pub width: u32,
    /// Hauteur en pixels (> 0).
    pub height: u32,
}

impl TileExtent {
    /// `size × size` square (`size` clamped to ≥ 1).
    #[must_use]
    pub fn square(size: u32) -> Self {
        let size = size.max(1);
        Self {
            width: size,
            height: size,
        }
    }
}

/// Stable identifier of a tile slot: level + coordinate.
///
/// Identity does NOT hold the revision (see [`TileKey`]): the same
/// slot is reused across revisions — the very point
/// of the cache.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct TileId {
    /// Mip level (0 = full resolution, +1 = halved).
    pub level: u8,
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
}

impl TileId {
    /// New identifier.
    #[must_use]
    pub fn new(level: u8, x: i32, y: i32) -> Self {
        Self { level, x, y }
    }

    /// Coordinate (without the level).
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

/// Cache key: slot + content revision.
///
/// Same `TileId`, different revision = potentially different
/// contents (miss). The revision comes from the source node (render graph),
/// never guessed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileKey {
    /// Slot.
    pub id: TileId,
    /// Expected content revision.
    pub revision: TileRevision,
}

impl TileKey {
    /// New key.
    #[must_use]
    pub fn new(id: TileId, revision: TileRevision) -> Self {
        Self { id, revision }
    }
}

/// Pixel rectangle in surface coordinates (signed origin,
/// exclusive `[x, x+w) × [y, y+h)` bounds). Empty iff `width == 0 || height == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileRegion {
    /// X origin (may be negative: off-document content).
    pub x: i32,
    /// Y origin.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl TileRegion {
    /// Empty region (covers no pixels, no tiles).
    pub const EMPTY: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// New rectangle.
    #[must_use]
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// True if no pixel covered.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Area in pixels (0 if empty).
    #[must_use]
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Exclusive right bound (i64: no overflow).
    #[must_use]
    pub fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    /// Exclusive bottom bound.
    #[must_use]
    pub fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    /// Built from exclusive bounds (empty if inverted).
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

    /// True if the (px) point is covered.
    #[must_use]
    pub fn contains(self, x: i32, y: i32) -> bool {
        !self.is_empty()
            && x >= self.x
            && y >= self.y
            && i64::from(x) < self.right()
            && i64::from(y) < self.bottom()
    }

    /// Intersection (clipping) — empty if disjoint.
    #[must_use]
    pub fn intersect(self, bounds: Self) -> Self {
        Self::from_bounds(
            i64::from(self.x).max(i64::from(bounds.x)),
            i64::from(self.y).max(i64::from(bounds.y)),
            self.right().min(bounds.right()),
            self.bottom().min(bounds.bottom()),
        )
    }

    /// Union (bounding) — empty + r = r.
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

    /// Expands by `pad` pixels in each direction (i32-saturating).
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

    /// Downscales to the mip level CONSERVATIVELY: the
    /// bounding rectangle covers all source pixels (`floor` on min,
    /// `ceil` on max). A dirty pixel stays covered at every level — for
    /// invalidation and visibility use, never the reverse.
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

/// Ceiling integer division (divisor > 0).
fn div_ceil(a: i64, b: i64) -> i64 {
    debug_assert!(b > 0);
    if a >= 0 { (a + b - 1) / b } else { a / b }
}

/// Invalidation margin in pixels: neighborhood effects (blur, bloom…)
/// read BEYOND the modified area — so we invalidate wider than the
/// source rectangle (see [`TileRegion::pad`]).
///
/// Prepares the logic without implementing filters: each effect will declare
/// its radius, converted here into an integer margin (ceiling).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Padding(pub u32);

impl Padding {
    /// No margin.
    pub const ZERO: Self = Self(0);

    /// Explicit margin in pixels.
    #[must_use]
    pub fn new(px: u32) -> Self {
        Self(px)
    }

    /// Margin for a blur radius in pixels (ceiling; ≤ 0 or NaN → 0).
    ///
    /// Example: `radius = 20.0` ⇒ `Padding(20)` — an edit inside an
    /// area invalidates the 20 px-expanded area (the kernel samples the
    /// modified neighborhood).
    #[must_use]
    pub fn for_blur_radius(radius: f32) -> Self {
        if !radius.is_finite() || radius <= 0.0 {
            return Self::ZERO;
        }
        Self(radius.ceil() as u32)
    }

    /// Margin for a generic neighborhood-effect radius (same rule).
    #[must_use]
    pub fn for_effect_radius(radius: f32) -> Self {
        Self::for_blur_radius(radius)
    }
}
