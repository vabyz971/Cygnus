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

//! Transform monde : matrice affine 2D dérivée, jamais stockée.
//!
//! [`Affine2`] est la forme COMPOSABLE du [`Transform2D`](math_utils::Transform2D)
//! local. La scène ne stocke que des locaux ; le monde se calcule à la
//! demande par multiplication parent → enfant
//! ([`crate::Scene::world_affine`]).
//!
//! Convention (origine, pas de centre d'image) :
//! `M = T(offset) · R(rotation) · K(skew) · S(scale)` avec
//! `K·S = [[sx, kx·sy], [ky·sx, sy]]` — même décomposition que
//! [`Transform2D::shear_scale_matrix`](math_utils::Transform2D::shear_scale_matrix),
//! mêmes formules de rotation que
//! [`Transform2D::local_to_doc`](math_utils::Transform2D::local_to_doc),
//! mais sans les termes de centre `(cx·sx, cy·sy)` qui dépendent des
//! dimensions d'un contenu raster et n'ont pas de sens pour un groupe,
//! une forme ou un cadre de mise en page.

use math_utils::Transform2D;

/// Matrice affine 2D : `x' = m00·x + m01·y + tx`, `y' = m10·x + m11·y + ty`.
///
/// Valeur pure dérivée des [`Transform2D`] locaux — jamais source de vérité.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Affine2 {
    /// Ligne X : `(m00, m01, tx)`.
    pub m00: f32,
    /// Ligne X : `(m00, m01, tx)`.
    pub m01: f32,
    /// Translation X.
    pub tx: f32,
    /// Ligne Y : `(m10, m11, ty)`.
    pub m10: f32,
    /// Ligne Y : `(m10, m11, ty)`.
    pub m11: f32,
    /// Translation Y.
    pub ty: f32,
}

impl Affine2 {
    /// Identité.
    pub const IDENTITY: Self = Self {
        m00: 1.0,
        m01: 0.0,
        tx: 0.0,
        m10: 0.0,
        m11: 1.0,
        ty: 0.0,
    };

    /// Matrice identité.
    #[must_use]
    pub fn identity() -> Self {
        Self::IDENTITY
    }

    /// Convertit un transform local en matrice (convention du module).
    #[must_use]
    pub fn from_transform(t: &Transform2D) -> Self {
        // K·S partagé avec math-utils : [[sx, kx·sy], [ky·sx, sy]].
        let (s00, s01, s10, s11) = t.shear_scale_matrix();
        let rad = t.rotation_deg.to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        // R·(K·S), puis translation par l'offset (origine, pas de centre).
        Self {
            m00: cos * s00 - sin * s10,
            m01: cos * s01 - sin * s11,
            tx: t.offset_x,
            m10: sin * s00 + cos * s10,
            m11: sin * s01 + cos * s11,
            ty: t.offset_y,
        }
    }

    /// Compose : `self` (parent) suivi de `child` — appliquer `child`
    /// d'abord, puis `self`. C'est l'ordre de la propagation monde.
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

    /// Applique la matrice à un point.
    #[must_use]
    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.m00 * x + self.m01 * y + self.tx,
            self.m10 * x + self.m11 * y + self.ty,
        )
    }
}
