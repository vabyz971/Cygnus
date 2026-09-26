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

//! Styles de peinture : couleurs, remplissages, contours, booléens.
//!
//! Données pures (aucune rasterisation) : le backend convertit ces
//! descriptions vers ses propres pinceaux (Vello ou autre).

use datatypes::Vec2;

/// Couleur RVBA flottante 0..=1 (bornée à la construction).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Color {
    /// Rouge.
    pub r: f32,
    /// Vert.
    pub g: f32,
    /// Bleu.
    pub b: f32,
    /// Alpha (1 = opaque).
    pub a: f32,
}

impl Color {
    /// Noir opaque.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Blanc opaque.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// Transparent.
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Nouvelle couleur (composantes bornées à 0..=1).
    #[must_use]
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Opaque (`a = 1`).
    #[must_use]
    pub fn opaque(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }
}

/// Arrêt de dégradé (offset 0..=1 borné).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradientStop {
    /// Position 0..=1.
    pub offset: f32,
    /// Couleur.
    pub color: Color,
}

impl GradientStop {
    /// Nouvel arrêt (offset borné).
    #[must_use]
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

/// Dégradé linéaire entre deux points.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LinearGradient {
    /// Départ (offset 0).
    pub start: Vec2,
    /// Arrivée (offset 1).
    pub end: Vec2,
    /// Arrêts (ordre d'application).
    pub stops: Vec<GradientStop>,
}

/// Dégradé radial (centre + rayon).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RadialGradient {
    /// Centre (offset 0).
    pub center: Vec2,
    /// Rayon (> 0 attendu, non vérifié ici — affaire du backend).
    pub radius: f32,
    /// Arrêts (ordre d'application).
    pub stops: Vec<GradientStop>,
}

/// Remplissage d'une forme.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    /// Aplat.
    Solid(Color),
    /// Dégradé linéaire.
    Linear(LinearGradient),
    /// Dégradé radial.
    Radial(RadialGradient),
}

/// Terminaison de contour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCap {
    /// Coupé net.
    Butt,
    /// Demi-cercle.
    Round,
    /// Carré dépassant.
    Square,
}

/// Jointure de contour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineJoin {
    /// Onglet (biseau extérieur).
    Miter,
    /// Arrondi.
    Round,
    /// Biseauté.
    Bevel,
}

/// Contour d'une forme.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stroke {
    /// Couleur du trait.
    pub color: Color,
    /// Épaisseur en unités dessin (≥ 0).
    pub width: f32,
    /// Terminaison.
    pub cap: LineCap,
    /// Jointure.
    pub join: LineJoin,
}

impl Stroke {
    /// Nouveau contour (épaisseur négative ramenée à 0).
    #[must_use]
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width: width.max(0.0),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
        }
    }
}

/// Opération booléenne entre formes (modèle : l'ÉVALUATION — tessellation
/// d'intersection — appartient au backend, pas au modèle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanOp {
    /// Union.
    Union,
    /// Intersection.
    Intersection,
    /// A moins B.
    Difference,
    /// Ou exclusif.
    Xor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleurs_bornees() {
        let c = Color::new(2.0, -1.0, 0.5, 1.0);
        assert_eq!((c.r, c.g, c.b, c.a), (1.0, 0.0, 0.5, 1.0));
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn contour_epaisseur_positive() {
        assert_eq!(Stroke::new(Color::BLACK, -3.0).width, 0.0);
        assert_eq!(GradientStop::new(9.0, Color::WHITE).offset, 1.0);
    }
}
