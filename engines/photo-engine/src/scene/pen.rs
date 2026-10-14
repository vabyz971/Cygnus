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

//! Bézier pen + node editing (spec §5 item 1).
//!
//! The pen builds [`VectorPath`] verb by verb (click = anchor,
//! drag = handles, like Affinity); node operations are pure path
//! → path functions so each manipulation maps to one invertible
//! history command. Flattening (fixed subdivisions per curve)
//! feeds the CPU preview rasterizer; tolerance-adaptive flattening
//! is a follow-up.

use super::{PathSegment, VectorPath};
use datatypes::Vec2;

/// Cubic Bézier point at `t` (0..=1, de Casteljau form).
pub fn cubic_point(p0: Vec2, c1: Vec2, c2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    Vec2::new(
        u * u * u * p0.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * p3.x,
        u * u * u * p0.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * p3.y,
    )
}

/// Quadratic Bézier point at `t` (0..=1).
pub fn quad_point(p0: Vec2, c: Vec2, p1: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    Vec2::new(
        u * u * p0.x + 2.0 * u * t * c.x + t * t * p1.x,
        u * u * p0.y + 2.0 * u * t * c.y + t * t * p1.y,
    )
}

/// Pen builder: accumulates one subpath verb by verb.
///
/// Segments before the first `move_to` are ignored (no silent
/// origin anchor). `build` returns `None` without a start point.
#[derive(Debug, Clone, Default)]
pub struct PenPath {
    start: Option<Vec2>,
    segments: Vec<PathSegment>,
    closed: bool,
}

impl PenPath {
    /// New empty pen.
    pub fn new() -> Self {
        Self::default()
    }

    /// Anchor the subpath start.
    pub fn move_to(&mut self, at: Vec2) -> &mut Self {
        self.start = Some(at);
        self
    }

    /// Straight segment to `to` (ignored before `move_to`).
    pub fn line_to(&mut self, to: Vec2) -> &mut Self {
        if self.start.is_some() {
            self.segments.push(PathSegment::Line { to });
        }
        self
    }

    /// Quadratic curve to `to` (ignored before `move_to`).
    pub fn quad_to(&mut self, ctrl: Vec2, to: Vec2) -> &mut Self {
        if self.start.is_some() {
            self.segments.push(PathSegment::Quad { ctrl, to });
        }
        self
    }

    /// Cubic curve to `to` (ignored before `move_to`).
    pub fn cubic_to(&mut self, ctrl1: Vec2, ctrl2: Vec2, to: Vec2) -> &mut Self {
        if self.start.is_some() {
            self.segments.push(PathSegment::Cubic { ctrl1, ctrl2, to });
        }
        self
    }

    /// Close the subpath on build.
    pub fn close(&mut self) -> &mut Self {
        self.closed = true;
        self
    }

    /// Finish the path (`None` without a start point).
    pub fn build(&self) -> Option<VectorPath> {
        Some(VectorPath {
            start: self.start?,
            segments: self.segments.clone(),
            closed: self.closed,
        })
    }
}

/// Anchors of a path: start point then every segment endpoint.
pub fn anchors(path: &VectorPath) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(path.segments.len() + 1);
    out.push(path.start);
    out.extend(path.segments.iter().map(|seg| seg.endpoint()));
    out
}

/// Move anchor `index` (0 = start) to `to`; control handles stay
/// absolute (curve shape follows). Out-of-range index is a no-op.
pub fn move_anchor(path: &VectorPath, index: usize, to: Vec2) -> VectorPath {
    let mut next = path.clone();
    if index == 0 {
        next.start = to;
    } else if let Some(seg) = next.segments.get_mut(index - 1) {
        match seg {
            PathSegment::Line { to: end }
            | PathSegment::Quad { to: end, .. }
            | PathSegment::Cubic { to: end, .. } => *end = to,
        }
    }
    next
}

/// Move control handle `control` of segment `seg` to `to`.
///
/// `control`: quad = 0 only; cubic = 0 (`ctrl1`) or 1 (`ctrl2`).
/// Anything else is a no-op (lines have no handles).
pub fn move_control(path: &VectorPath, seg: usize, control: usize, to: Vec2) -> VectorPath {
    let mut next = path.clone();
    match next.segments.get_mut(seg) {
        Some(PathSegment::Quad { ctrl, .. }) if control == 0 => *ctrl = to,
        Some(PathSegment::Cubic { ctrl1, ctrl2, .. }) => match control {
            0 => *ctrl1 = to,
            1 => *ctrl2 = to,
            _ => {}
        },
        _ => {}
    }
    next
}

/// Flatten a path to a polyline (open: closing edge excluded).
///
/// Lines contribute their endpoint; curves contribute `subdiv`
/// interior+end samples (`subdiv` clamped to 1..=64).
pub fn flatten(path: &VectorPath, subdiv: u32) -> Vec<Vec2> {
    let n = subdiv.clamp(1, 64);
    let mut out = Vec::new();
    out.push(path.start);
    let mut cursor = path.start;
    for seg in &path.segments {
        match *seg {
            PathSegment::Line { to } => {
                out.push(to);
                cursor = to;
            }
            PathSegment::Quad { ctrl, to } => {
                for i in 1..=n {
                    out.push(quad_point(cursor, ctrl, to, i as f32 / n as f32));
                }
                cursor = to;
            }
            PathSegment::Cubic { ctrl1, ctrl2, to } => {
                for i in 1..=n {
                    out.push(cubic_point(cursor, ctrl1, ctrl2, to, i as f32 / n as f32));
                }
                cursor = to;
            }
        }
    }
    out
}

/// Translate a path by `(dx, dy)` (anchors + handles).
pub fn translate(path: &VectorPath, dx: f32, dy: f32) -> VectorPath {
    let shift = |p: Vec2| Vec2::new(p.x + dx, p.y + dy);
    VectorPath {
        start: shift(path.start),
        segments: path
            .segments
            .iter()
            .map(|seg| match *seg {
                PathSegment::Line { to } => PathSegment::Line { to: shift(to) },
                PathSegment::Quad { ctrl, to } => PathSegment::Quad {
                    ctrl: shift(ctrl),
                    to: shift(to),
                },
                PathSegment::Cubic { ctrl1, ctrl2, to } => PathSegment::Cubic {
                    ctrl1: shift(ctrl1),
                    ctrl2: shift(ctrl2),
                    to: shift(to),
                },
            })
            .collect(),
        closed: path.closed,
    }
}

/// Axis-aligned bounds of a path (flattened, all points incl. handles).
///
/// `None` for an empty path (start-only, non-finite coords excluded).
pub fn path_bounds(path: &VectorPath) -> Option<datatypes::Rect> {
    let flat = flatten(path, 8);
    let mut points = flat;
    for seg in &path.segments {
        match *seg {
            PathSegment::Line { .. } => {}
            PathSegment::Quad { ctrl, .. } => points.push(ctrl),
            PathSegment::Cubic { ctrl1, ctrl2, .. } => {
                points.push(ctrl1);
                points.push(ctrl2);
            }
        }
    }
    let mut iter = points.iter().filter(|p| p.x.is_finite() && p.y.is_finite());
    let first = *iter.next()?;
    let mut bounds = datatypes::Rect::new(first.x, first.y, first.x, first.y);
    for p in iter {
        bounds.x0 = bounds.x0.min(p.x);
        bounds.y0 = bounds.y0.min(p.y);
        bounds.x1 = bounds.x1.max(p.x);
        bounds.y1 = bounds.y1.max(p.y);
    }
    Some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pen_sample() -> VectorPath {
        let mut pen = PenPath::new();
        pen.move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(10.0, 0.0))
            .quad_to(Vec2::new(15.0, 0.0), Vec2::new(15.0, 5.0))
            .cubic_to(
                Vec2::new(15.0, 8.0),
                Vec2::new(12.0, 10.0),
                Vec2::new(10.0, 10.0),
            );
        pen.build().expect("pen with start")
    }

    #[test]
    fn build_needs_a_start_point() {
        let mut pen = PenPath::new();
        pen.line_to(Vec2::new(1.0, 1.0));
        assert!(pen.build().is_none());
        pen.move_to(Vec2::new(0.0, 0.0));
        let path = pen.build().expect("start set");
        assert!(path.segments.is_empty(), "pre-start verbs ignored");
    }

    #[test]
    fn builder_keeps_verb_order() {
        let path = pen_sample();
        assert_eq!(path.segments.len(), 3);
        assert!(matches!(path.segments[0], PathSegment::Line { .. }));
        assert!(matches!(path.segments[1], PathSegment::Quad { .. }));
        assert!(matches!(path.segments[2], PathSegment::Cubic { .. }));
        assert!(!path.closed);
    }

    #[test]
    fn cubic_and_quad_midpoints() {
        let q = quad_point(
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 10.0),
            Vec2::new(10.0, 0.0),
            0.5,
        );
        assert!((q.x - 5.0).abs() < 1e-4 && (q.y - 5.0).abs() < 1e-4);
        let c = cubic_point(
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            0.5,
        );
        assert!((c.x - 0.5).abs() < 1e-4 && (c.y - 0.75).abs() < 1e-4);
    }

    #[test]
    fn anchors_and_moves_are_pure() {
        let path = pen_sample();
        let list = anchors(&path);
        assert_eq!(list.len(), 4);
        assert_eq!(list[0], Vec2::new(0.0, 0.0));

        let moved = move_anchor(&path, 1, Vec2::new(20.0, 0.0));
        assert_eq!(moved.segments[0].endpoint(), Vec2::new(20.0, 0.0));
        // Original untouched (pure path → path).
        assert_eq!(path.segments[0].endpoint(), Vec2::new(10.0, 0.0));
        // Out of range: no-op.
        assert_eq!(move_anchor(&path, 99, Vec2::new(1.0, 1.0)), path);

        let handled = move_control(&path, 2, 1, Vec2::new(13.0, 11.0));
        assert!(matches!(
            handled.segments[2],
            PathSegment::Cubic {
                ctrl2: Vec2 { x: 13.0, y: 11.0 },
                ..
            }
        ));
        // Lines have no handles: no-op.
        assert_eq!(move_control(&path, 0, 0, Vec2::new(1.0, 1.0)), path);
    }

    #[test]
    fn flatten_counts_samples_per_segment() {
        let path = pen_sample();
        // 1 start + 1 line end + 4 quad + 4 cubic.
        assert_eq!(flatten(&path, 4).len(), 10);
        let flat = flatten(&path, 1);
        assert_eq!(flat.first(), Some(&Vec2::new(0.0, 0.0)));
        assert_eq!(flat.last(), Some(&Vec2::new(10.0, 10.0)));
    }

    #[test]
    fn translate_shifts_anchors_and_handles() {
        let path = pen_sample();
        let moved = translate(&path, 5.0, -3.0);
        assert_eq!(moved.start, Vec2::new(5.0, -3.0));
        assert_eq!(moved.segments[0].endpoint(), Vec2::new(15.0, -3.0));
        assert!(matches!(
            moved.segments[2],
            PathSegment::Cubic {
                ctrl1: Vec2 { x: 20.0, y: 5.0 },
                ..
            }
        ));
        assert_eq!(path.start, Vec2::new(0.0, 0.0));
    }

    #[test]
    fn bounds_cover_curve_extents() {
        let path = pen_sample();
        let b = path_bounds(&path).expect("non-empty path");
        assert!(b.x0 <= 0.0 && b.y0 <= 0.0);
        assert!(b.x1 >= 15.0 && b.y1 >= 10.0);
        // Handles included: cubic ctrl2 y=10 is the max.
        assert!((b.y1 - 10.0).abs() < 1e-4);
    }
}
