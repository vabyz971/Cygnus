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

//! Vector shape generators (spec §5 item 1, first slice).
//!
//! Base shapes as closed segment paths in layer space:
//! resolution-independent by construction (SDF/flatten at draw,
//! never pre-triangulated). Freehand curves come from the pen
//! (see [`super::pen`]); the node editor mutates these paths.

use super::{PathSegment, VectorLayer, VectorPath};
use datatypes::Vec2;

/// Closed path from vertices in draw order (first = pen start).
fn closed_path(vertices: Vec<Vec2>) -> VectorLayer {
    let mut vertices = vertices;
    let start = vertices.remove(0);
    VectorLayer {
        paths: vec![VectorPath {
            start,
            segments: vertices
                .into_iter()
                .map(|to| PathSegment::Line { to })
                .collect(),
            closed: true,
        }],
        // Opaque black until the style slice (fills/gradients).
        fill: [0, 0, 0, 255],
        filter_layers: Vec::new(),
        masks: Vec::new(),
        appearance_version: 0,
    }
}

/// Rectangle path (4 corners, closed).
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> VectorLayer {
    closed_path(vec![
        Vec2::new(x, y),
        Vec2::new(x + w, y),
        Vec2::new(x + w, y + h),
        Vec2::new(x, y + h),
    ])
}

/// Ellipse path as line segments (`segments` straight edges).
///
/// `segments` is clamped to 8..=256: below 8 the shape stops
/// reading as an ellipse, above 256 the preview gains nothing.
pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, segments: u32) -> VectorLayer {
    let n = segments.clamp(8, 256);
    let mut vertices = Vec::with_capacity(n as usize);
    for i in 0..n {
        let a = 2.0 * std::f32::consts::PI * i as f32 / n as f32;
        vertices.push(Vec2::new(cx + rx * a.cos(), cy + ry * a.sin()));
    }
    closed_path(vertices)
}

/// Regular polygon path (`sides` >= 3, first vertex at the top).
pub fn polygon(cx: f32, cy: f32, radius: f32, sides: u32) -> VectorLayer {
    let n = sides.max(3);
    let mut vertices = Vec::with_capacity(n as usize);
    for i in 0..n {
        let a = -std::f32::consts::FRAC_PI_2 + 2.0 * std::f32::consts::PI * i as f32 / n as f32;
        vertices.push(Vec2::new(cx + radius * a.cos(), cy + radius * a.sin()));
    }
    closed_path(vertices)
}

/// Star path (alternating outer/inner radius, `points` >= 2).
pub fn star(cx: f32, cy: f32, outer: f32, inner: f32, points: u32) -> VectorLayer {
    let n = points.max(2);
    let mut vertices = Vec::with_capacity(2 * n as usize);
    for i in 0..2 * n {
        let r = if i % 2 == 0 { outer } else { inner };
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / n as f32;
        vertices.push(Vec2::new(cx + r * a.cos(), cy + r * a.sin()));
    }
    closed_path(vertices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_has_four_closed_corners() {
        let v = rect(10.0, 20.0, 30.0, 40.0);
        assert_eq!(v.paths.len(), 1);
        let p = &v.paths[0];
        assert!(p.closed);
        assert_eq!(p.start, Vec2::new(10.0, 20.0));
        assert_eq!(p.segments.len(), 3);
        assert_eq!(
            p.segments[1].endpoint(),
            Vec2::new(40.0, 60.0),
            "third corner"
        );
    }

    #[test]
    fn ellipse_segment_count_is_clamped() {
        // n vertices = 1 start + (n - 1) line segments.
        assert_eq!(ellipse(0.0, 0.0, 10.0, 5.0, 2).paths[0].segments.len(), 7);
        assert_eq!(ellipse(0.0, 0.0, 10.0, 5.0, 48).paths[0].segments.len(), 47);
        assert_eq!(
            ellipse(0.0, 0.0, 10.0, 5.0, 10_000).paths[0].segments.len(),
            255
        );
        // First vertex at angle 0 (right of center).
        let start = ellipse(0.0, 0.0, 10.0, 5.0, 4).paths[0].start;
        assert!((start.x - 10.0).abs() < 1e-4 && start.y.abs() < 1e-4);
    }

    #[test]
    fn polygon_starts_at_top_with_n_sides() {
        let v = polygon(0.0, 0.0, 10.0, 5);
        assert_eq!(v.paths[0].segments.len(), 4);
        let top = v.paths[0].start;
        assert!(top.x.abs() < 1e-4 && (top.y + 10.0).abs() < 1e-4);
        assert_eq!(polygon(0.0, 0.0, 10.0, 0).paths[0].segments.len(), 2);
    }

    #[test]
    fn star_alternates_radii() {
        let v = star(0.0, 0.0, 10.0, 4.0, 5);
        let path = &v.paths[0];
        let mut radii = vec![(path.start.x.hypot(path.start.y), 0)];
        for (i, seg) in path.segments.iter().enumerate() {
            let to = seg.endpoint();
            radii.push((to.x.hypot(to.y), i + 1));
        }
        assert_eq!(radii.len(), 10);
        for (r, i) in radii {
            let expected = if i % 2 == 0 { 10.0 } else { 4.0 };
            assert!((r - expected).abs() < 1e-3, "vertex {i}: {r}");
        }
    }
}
