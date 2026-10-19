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

//! World transform: derived 2D affine matrix, never stored.
//!
//! [`Affine2`] is the COMPOSABLE form of the local [`Transform2D`](math_utils::Transform2D).
//! The scene only stores locals; the world is computed on
//! demand by parent → child multiplication
//! ([`crate::Scene::world_affine`]).
//!
//! Convention (origin, no image center):
//! `M = T(offset) · R(rotation) · K(skew) · S(scale)` with
//! `K·S = [[sx, kx·sy], [ky·sx, sy]]` — same decomposition as
//! [`Transform2D::shear_scale_matrix`](math_utils::Transform2D::shear_scale_matrix),
//! same rotation formulas as
//! [`Transform2D::local_to_doc`](math_utils::Transform2D::local_to_doc),
//! but without the center terms `(cx·sx, cy·sy)` which depend on
//! a raster content's dimensions and are meaningless for a group,
//! a shape or a layout frame.

use math_utils::Transform2D;

/// 2D affine matrix: `x' = m00·x + m01·y + tx`, `y' = m10·x + m11·y + ty`.
///
/// Pure value derived from local [`Transform2D`]s — never the source of truth.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Affine2 {
    /// X row: `(m00, m01, tx)`.
    pub m00: f32,
    /// X row: `(m00, m01, tx)`.
    pub m01: f32,
    /// X translation.
    pub tx: f32,
    /// Y row: `(m10, m11, ty)`.
    pub m10: f32,
    /// Y row: `(m10, m11, ty)`.
    pub m11: f32,
    /// Y translation.
    pub ty: f32,
}

impl Affine2 {
    /// Identity.
    pub const IDENTITY: Self = Self {
        m00: 1.0,
        m01: 0.0,
        tx: 0.0,
        m10: 0.0,
        m11: 1.0,
        ty: 0.0,
    };

    /// Identity matrix.
    #[must_use]
    pub fn identity() -> Self {
        Self::IDENTITY
    }

    /// Converts a local transform into a matrix (module convention).
    #[must_use]
    pub fn from_transform(t: &Transform2D) -> Self {
        // K·S shared with math-utils: [[sx, kx·sy], [ky·sx, sy]].
        let (s00, s01, s10, s11) = t.shear_scale_matrix();
        let rad = t.rotation_deg.to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        // R·(K·S), then translation by the offset (origin, no center).
        Self {
            m00: cos * s00 - sin * s10,
            m01: cos * s01 - sin * s11,
            tx: t.offset_x,
            m10: sin * s00 + cos * s10,
            m11: sin * s01 + cos * s11,
            ty: t.offset_y,
        }
    }

    /// Composes: `self` (parent) followed by `child` — apply `child`
    /// first, then `self`. This is the world-propagation order.
    #[must_use]
    pub fn concat(self, child: Self) -> Self {
        Self {
            m00: self.m00 * child.m00 + self.m01 * child.m10,
            m01: self.m00 * child.m01 + self.m01 * child.m11,
            tx: self.m00 * child.tx + self.m01 * child.ty + self.tx,
            m10: self.m10 * child.m00 + self.m11 * child.m10,
            m11: self.m10 * child.m01 + self.m11 * child.m11,
            ty: self.m10 * child.tx + self.m11 * child.ty + self.ty,
        }
    }

    /// Applies the matrix to a point.
    #[must_use]
    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.m00 * x + self.m01 * y + self.tx,
            self.m10 * x + self.m11 * y + self.ty,
        )
    }
}
