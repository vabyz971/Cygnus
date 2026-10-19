// Cygnus — Professional open-source creative suite
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

//! Pixel selections (marquee): a rectangle or an ellipse in DOCUMENT
//! pixels, optionally inverted.
//!
//! Pure geometry, no buffer: [`Selection::coverage`] answers "how much
//! of document pixel (x, y) is selected" (0..=1, one-pixel anti-aliased
//! edge), which clips brush / eraser / mask strokes and fills masks
//! ([`Selection::clip_for_layer`] maps it into a layer's own space).
//! [`Selection::outline`] gives the polygon the UI strokes as marching
//! ants. A selection is session state (the worker mirrors the UI's):
//! neither part of the undo history nor of the `.cygp` project.

use crate::Transform2D;

/// Selected area, in document pixels (pixel `(x, y)` spans
/// `[x, x + 1) x [y, y + 1)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectionShape {
    /// Axis-aligned rectangle (`x0 < x1`, `y0 < y1`).
    Rect {
        /// Left edge.
        x0: f32,
        /// Top edge.
        y0: f32,
        /// Right edge.
        x1: f32,
        /// Bottom edge.
        y1: f32,
    },
    /// Axis-aligned ellipse inscribed in its bounding box.
    Ellipse {
        /// Center x.
        cx: f32,
        /// Center y.
        cy: f32,
        /// Horizontal radius.
        rx: f32,
        /// Vertical radius.
        ry: f32,
    },
}

/// Marquee kind picked by the selection tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionKind {
    /// Rectangle marquee.
    #[default]
    Rect,
    /// Ellipse marquee.
    Ellipse,
}

/// A pixel selection: shape + inversion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    /// Selected shape (before inversion).
    pub shape: SelectionShape,
    /// True = everything OUTSIDE the shape is selected.
    pub inverted: bool,
}

/// Smallest marquee side kept (a click without drag selects nothing).
pub const MIN_SELECTION_SIZE: f32 = 1.0;

impl Selection {
    /// Marquee spanned by two drag corners (any order), `None` when
    /// thinner than [`MIN_SELECTION_SIZE`] or not finite.
    #[must_use]
    pub fn from_drag(kind: SelectionKind, a: (f32, f32), b: (f32, f32)) -> Option<Self> {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite())
            || x1 - x0 < MIN_SELECTION_SIZE
            || y1 - y0 < MIN_SELECTION_SIZE
        {
            return None;
        }
        let shape = match kind {
            SelectionKind::Rect => SelectionShape::Rect { x0, y0, x1, y1 },
            SelectionKind::Ellipse => SelectionShape::Ellipse {
                cx: (x0 + x1) / 2.0,
                cy: (y0 + y1) / 2.0,
                rx: (x1 - x0) / 2.0,
                ry: (y1 - y0) / 2.0,
            },
        };
        Some(Self {
            shape,
            inverted: false,
        })
    }

    /// "Select all": the whole `width` x `height` document.
    #[must_use]
    pub fn all(width: u32, height: u32) -> Self {
        Self {
            shape: SelectionShape::Rect {
                x0: 0.0,
                y0: 0.0,
                x1: width as f32,
                y1: height as f32,
            },
            inverted: false,
        }
    }

    /// Same shape, inside and outside swapped.
    #[must_use]
    pub fn inverse(self) -> Self {
        Self {
            inverted: !self.inverted,
            ..self
        }
    }

    /// Selected fraction (0..=1) of the document pixel whose CENTER is
    /// `(x, y)`, with a one-pixel anti-aliased edge.
    #[must_use]
    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        let inside = match self.shape {
            SelectionShape::Rect { x0, y0, x1, y1 } => {
                let distance = (x - x0).min(x1 - x).min(y - y0).min(y1 - y);
                (distance + 0.5).clamp(0.0, 1.0)
            }
            SelectionShape::Ellipse { cx, cy, rx, ry } => {
                if rx <= 0.0 || ry <= 0.0 {
                    0.0
                } else {
                    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
                    let norm = (dx * dx + dy * dy).sqrt();
                    // Signed distance to the edge, approximated along the
                    // radius (exact on circles, close on ellipses).
                    let radius = if norm > 0.0 {
                        let (ux, uy) = (dx / norm * rx, dy / norm * ry);
                        (ux * ux + uy * uy).sqrt()
                    } else {
                        rx.min(ry)
                    };
                    ((1.0 - norm) * radius + 0.5).clamp(0.0, 1.0)
                }
            }
        };
        if self.inverted { 1.0 - inside } else { inside }
    }

    /// Coverage lookup in a layer's OWN pixel space: layer pixel
    /// `(px, py)` (top-left origin) goes through `transform` to the
    /// document. `w` x `h` = layer source size.
    pub fn clip_for_layer(
        self,
        transform: Transform2D,
        w: u32,
        h: u32,
    ) -> impl Fn(f32, f32) -> f32 {
        let identity = transform == Transform2D::default();
        move |px: f32, py: f32| {
            let (x, y) = (px + 0.5, py + 0.5);
            let (dx, dy) = if identity {
                (x, y)
            } else {
                transform.local_to_doc(w as f32, h as f32, x, y)
            };
            self.coverage(dx, dy)
        }
    }

    /// Bounds of the shape (before inversion): `(x0, y0, x1, y1)`.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        match self.shape {
            SelectionShape::Rect { x0, y0, x1, y1 } => (x0, y0, x1, y1),
            SelectionShape::Ellipse { cx, cy, rx, ry } => (cx - rx, cy - ry, cx + rx, cy + ry),
        }
    }

    /// Closed outline of the shape in document pixels (marching ants);
    /// ellipses are sampled with `segments` points (min 8).
    #[must_use]
    pub fn outline(&self, segments: usize) -> Vec<(f32, f32)> {
        match self.shape {
            SelectionShape::Rect { x0, y0, x1, y1 } => {
                vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            SelectionShape::Ellipse { cx, cy, rx, ry } => {
                let n = segments.max(8);
                (0..n)
                    .map(|i| {
                        let t = i as f32 / n as f32 * std::f32::consts::TAU;
                        (cx + rx * t.cos(), cy + ry * t.sin())
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_normalizes_corners_and_rejects_clicks() {
        let sel =
            Selection::from_drag(SelectionKind::Rect, (10.0, 8.0), (2.0, 4.0)).expect("selection");
        assert_eq!(
            sel.shape,
            SelectionShape::Rect {
                x0: 2.0,
                y0: 4.0,
                x1: 10.0,
                y1: 8.0
            }
        );
        assert!(Selection::from_drag(SelectionKind::Rect, (3.0, 3.0), (3.2, 9.0)).is_none());
        assert!(Selection::from_drag(SelectionKind::Rect, (f32::NAN, 0.0), (4.0, 4.0)).is_none());
    }

    #[test]
    fn rect_coverage_inside_edge_outside() {
        let sel =
            Selection::from_drag(SelectionKind::Rect, (2.0, 2.0), (6.0, 6.0)).expect("selection");
        assert_eq!(sel.coverage(4.5, 4.5), 1.0);
        assert_eq!(sel.coverage(2.5, 4.5), 1.0, "edge pixel fully inside");
        assert_eq!(sel.coverage(1.5, 4.5), 0.0, "pixel left of the edge");
        assert_eq!(sel.coverage(2.0, 4.5), 0.5, "center on the edge: half");
        let inverse = sel.inverse();
        assert_eq!(inverse.coverage(4.5, 4.5), 0.0);
        assert_eq!(inverse.coverage(0.5, 0.5), 1.0);
        assert_eq!(inverse.inverse(), sel);
    }

    #[test]
    fn ellipse_coverage_follows_the_radius() {
        let sel = Selection::from_drag(SelectionKind::Ellipse, (0.0, 0.0), (20.0, 20.0))
            .expect("selection");
        assert_eq!(sel.coverage(10.0, 10.0), 1.0);
        assert_eq!(sel.coverage(1.0, 1.0), 0.0, "corner outside the circle");
        assert!((sel.coverage(20.0, 10.0) - 0.5).abs() < 1e-4, "on the rim");
    }

    #[test]
    fn select_all_covers_the_document() {
        let sel = Selection::all(4, 3);
        assert_eq!(sel.coverage(0.5, 0.5), 1.0);
        assert_eq!(sel.coverage(3.5, 2.5), 1.0);
        assert_eq!(sel.coverage(4.5, 2.5), 0.0);
        assert_eq!(sel.bounds(), (0.0, 0.0, 4.0, 3.0));
    }

    #[test]
    fn layer_clip_follows_the_transform() {
        let sel = Selection::from_drag(SelectionKind::Rect, (10.0, 0.0), (20.0, 10.0))
            .expect("selection");
        let identity = sel.clip_for_layer(Transform2D::default(), 32, 32);
        assert_eq!(identity(12.0, 2.0), 1.0);
        assert_eq!(identity(2.0, 2.0), 0.0);
        // Layer moved 10 px right: its pixel 2 sits at document 12.
        let moved = Transform2D {
            offset_x: 10.0,
            ..Transform2D::default()
        };
        let clip = sel.clip_for_layer(moved, 32, 32);
        assert_eq!(clip(2.0, 2.0), 1.0);
        assert_eq!(clip(12.0, 2.0), 0.0);
    }

    #[test]
    fn outlines_close_the_shape() {
        let rect = Selection::all(4, 3);
        assert_eq!(rect.outline(0).len(), 4);
        let ellipse = Selection::from_drag(SelectionKind::Ellipse, (0.0, 0.0), (10.0, 4.0))
            .expect("selection");
        let points = ellipse.outline(4);
        assert_eq!(points.len(), 8, "minimum sampling");
        assert!((points[0].0 - 10.0).abs() < 1e-4);
    }
}
