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

//! Paint styles: colors, fills, strokes, booleans.
//!
//! Pure data (no rasterization): the backend converts these
//! descriptions into its own brushes (Vello or other).

use datatypes::Vec2;

/// Floating-point RVBA color 0..=1 (clamped at construction).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha (1 = opaque).
    pub a: f32,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque white.
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

    /// New color (components clamped to 0..=1).
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

/// Gradient stop (offset 0..=1, clamped).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradientStop {
    /// Position 0..=1.
    pub offset: f32,
    /// Color.
    pub color: Color,
}

impl GradientStop {
    /// New stop (clamped offset).
    #[must_use]
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

/// Linear gradient between two points.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LinearGradient {
    /// Start (offset 0).
    pub start: Vec2,
    /// End (offset 1).
    pub end: Vec2,
    /// Stops (in application order).
    pub stops: Vec<GradientStop>,
}

/// Radial gradient (center + radius).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RadialGradient {
    /// Center (offset 0).
    pub center: Vec2,
    /// Radius (> 0 expected, not verified here — backend's business).
    pub radius: f32,
    /// Stops (in application order).
    pub stops: Vec<GradientStop>,
}

/// Fill of a shape.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    /// Solid color.
    Solid(Color),
    /// Linear gradient.
    Linear(LinearGradient),
    /// Radial gradient.
    Radial(RadialGradient),
}

/// Stroke cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCap {
    /// Butt.
    Butt,
    /// Round.
    Round,
    /// Square.
    Square,
}

/// Stroke join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineJoin {
    /// Miter (outer bevel).
    Miter,
    /// Round.
    Round,
    /// Beveled.
    Bevel,
}

/// Stroke of a shape.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stroke {
    /// Stroke color.
    pub color: Color,
    /// Thickness in drawing units (≥ 0).
    pub width: f32,
    /// Cap.
    pub cap: LineCap,
    /// Join.
    pub join: LineJoin,
}

impl Stroke {
    /// New stroke (negative thickness clamped to 0).
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

/// Boolean operation between shapes (model: EVALUATION — intersection
/// tessellation — belongs to the backend, not the model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanOp {
    /// Union.
    Union,
    /// Intersection.
    Intersection,
    /// A minus B.
    Difference,
    /// Exclusive or.
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
