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

//! Vector geometry: Bézier paths and bounds.
//!
//! [`Path`] is a sequence of verbs (move/line/quad/cubic/close) in
//! `datatypes::Vec2`. [`Path::bounds`] returns the bounding box of the
//! control points: CONSERVATIVE and exact in practice (a Bézier curve lies
//! within the convex hull of its controls — the backend refines at
//! tessellation).

use datatypes::{Rect, Vec2};

/// A path verb (drawing-space coordinates).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathVerb {
    /// Moves the current point (starts a subpath).
    MoveTo(Vec2),
    /// Straight segment to the point.
    LineTo(Vec2),
    /// Quadratic Bézier (control, endpoint).
    QuadTo(Vec2, Vec2),
    /// Cubic Bézier (control 1, control 2, endpoint).
    CubicTo(Vec2, Vec2, Vec2),
    /// Closes the current subpath.
    Close,
}

impl PathVerb {
    /// Control points of the verb (for future bounds and flattening).
    #[must_use]
    pub fn points(self) -> Vec<Vec2> {
        match self {
            PathVerb::MoveTo(p) | PathVerb::LineTo(p) => vec![p],
            PathVerb::QuadTo(c, p) => vec![c, p],
            PathVerb::CubicTo(c1, c2, p) => vec![c1, c2, p],
            PathVerb::Close => Vec::new(),
        }
    }
}

/// Vector path: ordered sequence of verbs.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Path {
    verbs: Vec<PathVerb>,
}

impl Path {
    /// Empty path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of verbs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.verbs.len()
    }

    /// True if no verbs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Verbs in order.
    #[must_use]
    pub fn verbs(&self) -> &[PathVerb] {
        &self.verbs
    }

    /// Adds a verb (chainable constructor).
    pub fn push(&mut self, verb: PathVerb) {
        self.verbs.push(verb);
    }

    /// Bounding box of the control points (a lone point yields a degenerate
    /// — empty — rectangle positioned at the point; empty if no points).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for verb in &self.verbs {
            for p in verb.points() {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
        if min_x.is_infinite() {
            return Rect::EMPTY;
        }
        Rect::new(min_x, min_y, max_x, max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn bornes_segments_exactes() {
        let mut path = Path::new();
        assert!(path.is_empty());
        path.push(PathVerb::MoveTo(v(0.0, 0.0)));
        path.push(PathVerb::LineTo(v(10.0, 5.0)));
        assert_eq!(path.len(), 2);
        assert_eq!(path.bounds(), Rect::new(0.0, 0.0, 10.0, 5.0));
    }

    #[test]
    fn bornes_courbes_conservatives() {
        // Cubic rising to y=7.5 max but controls at y=30: the controls'
        // bounding box covers the curve (convex-hull property).
        let mut path = Path::new();
        path.push(PathVerb::MoveTo(v(0.0, 0.0)));
        path.push(PathVerb::CubicTo(v(0.0, 30.0), v(10.0, 30.0), v(10.0, 0.0)));
        let bounds = path.bounds();
        assert_eq!((bounds.x0, bounds.y1), (0.0, 30.0));
        // The real (sampled) curve fits inside.
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mt = 1.0 - t;
            let y = 3.0 * mt * mt * t * 30.0 + 3.0 * mt * t * t * 30.0;
            assert!(y <= bounds.y1 + 0.001, "y={y}");
        }
    }

    #[test]
    fn trace_vide_bornes_vides() {
        assert!(Path::new().bounds().is_empty());
    }
}
