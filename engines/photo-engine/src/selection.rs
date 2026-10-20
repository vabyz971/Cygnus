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

//! Pixel selections: a rectangle or an ellipse (marquee) or a coverage
//! bitmap (lasso polygon, magic wand) in DOCUMENT pixels, optionally
//! inverted.
//!
//! [`Selection::coverage`] answers "how much
//! of document pixel (x, y) is selected" (0..=1, one-pixel anti-aliased
//! edge), which clips brush / eraser / mask strokes and fills masks
//! ([`Selection::clip_for_layer`] maps it into a layer's own space).
//! [`Selection::outline`] gives the polylines the UI strokes as marching
//! ants. Bitmaps are shared (`Arc`): cloning a selection is cheap. A selection is session state (the worker mirrors the UI's):
//! neither part of the undo history nor of the `.cygp` project.

use std::sync::Arc;

use crate::Transform2D;

/// Vertical sub-samples per pixel row when rasterizing a lasso polygon.
const POLYGON_SUBSAMPLES: usize = 4;
/// Longest side of the grid the bitmap outline is traced on (larger
/// bitmaps are sampled: the ants stay cheap whatever the document).
const OUTLINE_GRID: u32 = 1024;

/// Selected area, in document pixels (pixel `(x, y)` spans
/// `[x, x + 1) x [y, y + 1)`).
#[derive(Debug, Clone, PartialEq)]
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
    /// Per-pixel coverage over the whole document (lasso, wand).
    Bitmap {
        /// Document width (row stride of `data`).
        width: u32,
        /// Document height.
        height: u32,
        /// Coverage 0..=255, row-major.
        data: Arc<[u8]>,
        /// Covered area `(x0, y0, x1, y1)`, exclusive end.
        bounds: (u32, u32, u32, u32),
    },
}

/// How a new selection combines with the active one (Shift = add,
/// Alt = subtract, Shift+Alt = intersect, nothing = replace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionOp {
    /// The new selection replaces the active one.
    #[default]
    Replace,
    /// Union.
    Add,
    /// Active minus new.
    Subtract,
    /// Common part only.
    Intersect,
}

impl SelectionOp {
    /// Operation from the held modifiers.
    #[must_use]
    pub fn from_modifiers(shift: bool, alt: bool) -> Self {
        match (shift, alt) {
            (true, true) => Self::Intersect,
            (true, false) => Self::Add,
            (false, true) => Self::Subtract,
            (false, false) => Self::Replace,
        }
    }

    /// Combines `active` with `new` over a `width` x `height` document.
    /// `None` = nothing selected (subtracting everything, empty
    /// intersection, or no input at all).
    #[must_use]
    pub fn apply(
        self,
        active: Option<&Selection>,
        new: Option<Selection>,
        width: u32,
        height: u32,
    ) -> Option<Selection> {
        let combine = |f: &dyn Fn(f32, f32) -> f32, a: &Selection, b: &Selection| {
            let mut data = Vec::with_capacity(width as usize * height as usize);
            for y in 0..height {
                for x in 0..width {
                    let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let v = f(a.coverage(cx, cy), b.coverage(cx, cy));
                    data.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
            }
            Selection::from_bitmap(width, height, data)
        };
        match (self, active, new) {
            (Self::Replace, _, new) => new,
            (Self::Add, None, new) => new,
            (Self::Add, Some(a), None) => Some(a.clone()),
            (Self::Add, Some(a), Some(b)) => combine(&|x, y| x.max(y), a, &b),
            (Self::Subtract, None, _) => None,
            (Self::Subtract, Some(a), None) => Some(a.clone()),
            (Self::Subtract, Some(a), Some(b)) => combine(&|x, y| x * (1.0 - y), a, &b),
            (Self::Intersect, Some(a), Some(b)) => combine(&|x, y| x * y, a, &b),
            (Self::Intersect, _, _) => None,
        }
    }
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
#[derive(Debug, Clone, PartialEq)]
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
            SelectionShape::Bitmap {
                width,
                height,
                ref data,
                ..
            } => {
                if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
                    0.0
                } else {
                    let index = y as usize * width as usize + x as usize;
                    f32::from(data.get(index).copied().unwrap_or(0)) / 255.0
                }
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
        &self,
        transform: Transform2D,
        w: u32,
        h: u32,
    ) -> impl Fn(f32, f32) -> f32 + use<> {
        let identity = transform == Transform2D::default();
        let selection = self.clone();
        move |px: f32, py: f32| {
            let (x, y) = (px + 0.5, py + 0.5);
            let (dx, dy) = if identity {
                (x, y)
            } else {
                transform.local_to_doc(w as f32, h as f32, x, y)
            };
            selection.coverage(dx, dy)
        }
    }

    /// Bounds of the shape (before inversion): `(x0, y0, x1, y1)`.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        match self.shape {
            SelectionShape::Rect { x0, y0, x1, y1 } => (x0, y0, x1, y1),
            SelectionShape::Ellipse { cx, cy, rx, ry } => (cx - rx, cy - ry, cx + rx, cy + ry),
            SelectionShape::Bitmap {
                bounds: (x0, y0, x1, y1),
                ..
            } => (x0 as f32, y0 as f32, x1 as f32, y1 as f32),
        }
    }

    /// Outline of the shape in document pixels (marching ants), as
    /// polylines: one closed loop (first point repeated) for marquees,
    /// boundary runs between selected and unselected cells for bitmaps.
    /// Ellipses are sampled with `segments` points (min 8).
    #[must_use]
    pub fn outline(&self, segments: usize) -> Vec<Vec<(f32, f32)>> {
        let closed = |mut points: Vec<(f32, f32)>| {
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            vec![points]
        };
        match self.shape {
            SelectionShape::Rect { x0, y0, x1, y1 } => {
                closed(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
            }
            SelectionShape::Ellipse { cx, cy, rx, ry } => {
                let n = segments.max(8);
                closed(
                    (0..n)
                        .map(|i| {
                            let t = i as f32 / n as f32 * std::f32::consts::TAU;
                            (cx + rx * t.cos(), cy + ry * t.sin())
                        })
                        .collect(),
                )
            }
            SelectionShape::Bitmap {
                width,
                height,
                ref data,
                ..
            } => bitmap_outline(width, height, data),
        }
    }

    /// Lasso: closed polygon `points` (document pixels) rasterized with
    /// anti-aliased edges over a `width` x `height` document. `None` for
    /// fewer than 3 points or an empty area.
    #[must_use]
    pub fn from_polygon(points: &[(f32, f32)], width: u32, height: u32) -> Option<Self> {
        if points.len() < 3 || width == 0 || height == 0 {
            return None;
        }
        if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return None;
        }
        let (w, h) = (width as usize, height as usize);
        let min_y = points.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        let row0 = min_y.floor().max(0.0) as usize;
        let row1 = (max_y.ceil().max(0.0) as usize).min(h);
        let mut coverage = vec![0f32; w * h];
        let mut crossings: Vec<f32> = Vec::new();
        for row in row0..row1 {
            for k in 0..POLYGON_SUBSAMPLES {
                let ys = row as f32 + (k as f32 + 0.5) / POLYGON_SUBSAMPLES as f32;
                crossings.clear();
                for (i, a) in points.iter().enumerate() {
                    let b = points[(i + 1) % points.len()];
                    if (a.1 <= ys && ys < b.1) || (b.1 <= ys && ys < a.1) {
                        crossings.push(a.0 + (ys - a.1) * (b.0 - a.0) / (b.1 - a.1));
                    }
                }
                crossings.sort_by(f32::total_cmp);
                for [start, end] in crossings.as_chunks::<2>().0 {
                    let (xa, xb) = (start.max(0.0), end.min(width as f32));
                    if xb <= xa {
                        continue;
                    }
                    let line = &mut coverage[row * w..(row + 1) * w];
                    for (px, cell) in line
                        .iter_mut()
                        .enumerate()
                        .take(xb.ceil() as usize)
                        .skip(xa.floor() as usize)
                    {
                        let overlap = (xb.min(px as f32 + 1.0) - xa.max(px as f32)).max(0.0);
                        *cell += overlap / POLYGON_SUBSAMPLES as f32;
                    }
                }
            }
        }
        let data: Vec<u8> = coverage
            .iter()
            .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
        Self::from_bitmap(width, height, data)
    }

    /// Magic wand: pixels of `rgba` (`width` x `height`, straight RGBA8)
    /// within `tolerance` (largest channel difference, 0..=255) of the
    /// seed pixel — only those connected to it when `contiguous`.
    /// `None` when the seed is outside the image.
    #[must_use]
    pub fn from_color(
        rgba: &[u8],
        width: u32,
        height: u32,
        seed: (u32, u32),
        tolerance: u8,
        contiguous: bool,
    ) -> Option<Self> {
        let (w, h) = (width as usize, height as usize);
        if seed.0 >= width || seed.1 >= height || rgba.len() < w * h * 4 {
            return None;
        }
        let pixel = |i: usize| &rgba[i * 4..i * 4 + 4];
        let target: [u8; 4] = pixel(seed.1 as usize * w + seed.0 as usize)
            .try_into()
            .ok()?;
        let matches = |i: usize| {
            pixel(i)
                .iter()
                .zip(target)
                .all(|(c, t)| c.abs_diff(t) <= tolerance)
        };
        let mut data = vec![0u8; w * h];
        if contiguous {
            let mut stack = vec![seed.1 as usize * w + seed.0 as usize];
            while let Some(i) = stack.pop() {
                if data[i] != 0 || !matches(i) {
                    continue;
                }
                data[i] = 255;
                let (x, y) = (i % w, i / w);
                if x > 0 {
                    stack.push(i - 1);
                }
                if x + 1 < w {
                    stack.push(i + 1);
                }
                if y > 0 {
                    stack.push(i - w);
                }
                if y + 1 < h {
                    stack.push(i + w);
                }
            }
        } else {
            for (i, cell) in data.iter_mut().enumerate() {
                if matches(i) {
                    *cell = 255;
                }
            }
        }
        Self::from_bitmap(width, height, data)
    }

    /// Bitmap selection from coverage `data` (`None` when all zero).
    fn from_bitmap(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        let w = width as usize;
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for (i, value) in data.iter().enumerate() {
            if *value == 0 {
                continue;
            }
            let (x, y) = ((i % w) as u32, (i / w) as u32);
            bounds = Some(match bounds {
                None => (x, y, x + 1, y + 1),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
            });
        }
        Some(Self {
            shape: SelectionShape::Bitmap {
                width,
                height,
                data: data.into(),
                bounds: bounds?,
            },
            inverted: false,
        })
    }
}

/// Boundary runs of a coverage bitmap (cells = half covered or more),
/// traced on a grid of at most [`OUTLINE_GRID`] cells per side.
fn bitmap_outline(width: u32, height: u32, data: &[u8]) -> Vec<Vec<(f32, f32)>> {
    let cell = width.max(height).div_ceil(OUTLINE_GRID).max(1);
    let (gw, gh) = (width.div_ceil(cell), height.div_ceil(cell));
    let inside = |gx: i64, gy: i64| -> bool {
        if gx < 0 || gy < 0 || gx >= i64::from(gw) || gy >= i64::from(gh) {
            return false;
        }
        let x = ((gx as u32) * cell + cell / 2).min(width - 1);
        let y = ((gy as u32) * cell + cell / 2).min(height - 1);
        data.get(y as usize * width as usize + x as usize)
            .is_some_and(|v| *v >= 128)
    };
    let size = cell as f32;
    let mut runs = Vec::new();
    // Horizontal edges (between rows gy-1 and gy), merged into runs.
    for gy in 0..=i64::from(gh) {
        let mut start: Option<i64> = None;
        for gx in 0..=i64::from(gw) {
            let edge = gx < i64::from(gw) && inside(gx, gy - 1) != inside(gx, gy);
            match (edge, start) {
                (true, None) => start = Some(gx),
                (false, Some(x0)) => {
                    let y = (gy as f32 * size).min(height as f32);
                    runs.push(vec![
                        (x0 as f32 * size, y),
                        ((gx as f32 * size).min(width as f32), y),
                    ]);
                    start = None;
                }
                _ => {}
            }
        }
    }
    // Vertical edges (between columns gx-1 and gx).
    for gx in 0..=i64::from(gw) {
        let mut start: Option<i64> = None;
        for gy in 0..=i64::from(gh) {
            let edge = gy < i64::from(gh) && inside(gx - 1, gy) != inside(gx, gy);
            match (edge, start) {
                (true, None) => start = Some(gy),
                (false, Some(y0)) => {
                    let x = (gx as f32 * size).min(width as f32);
                    runs.push(vec![
                        (x, y0 as f32 * size),
                        (x, (gy as f32 * size).min(height as f32)),
                    ]);
                    start = None;
                }
                _ => {}
            }
        }
    }
    runs
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
        let inverse = sel.clone().inverse();
        assert_eq!(inverse.coverage(4.5, 4.5), 0.0);
        assert_eq!(inverse.coverage(0.5, 0.5), 1.0);
        assert_eq!(inverse.clone().inverse(), sel);
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
        let loops = rect.outline(0);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 5, "closed loop");
        let ellipse = Selection::from_drag(SelectionKind::Ellipse, (0.0, 0.0), (10.0, 4.0))
            .expect("selection");
        let points = &ellipse.outline(4)[0];
        assert_eq!(points.len(), 9, "minimum sampling, closed");
        assert!((points[0].0 - 10.0).abs() < 1e-4);
    }

    #[test]
    fn lasso_polygon_covers_its_inside_with_soft_edges() {
        let square = [(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)];
        let sel = Selection::from_polygon(&square, 10, 10).expect("lasso");
        assert_eq!(sel.coverage(5.5, 5.5), 1.0);
        assert_eq!(sel.coverage(0.5, 0.5), 0.0);
        assert_eq!(sel.bounds(), (2.0, 2.0, 8.0, 8.0));
        let half = [(2.5, 0.0), (6.0, 0.0), (6.0, 4.0), (2.5, 4.0)];
        let edge = Selection::from_polygon(&half, 10, 10).expect("lasso");
        assert!((edge.coverage(2.5, 1.5) - 0.5).abs() < 0.01, "half pixel");
        assert!(Selection::from_polygon(&square[..2], 10, 10).is_none());
        let outside = [(20.0, 20.0), (30.0, 20.0), (30.0, 30.0)];
        assert!(Selection::from_polygon(&outside, 10, 10).is_none());
    }

    #[test]
    fn wand_floods_similar_connected_pixels() {
        // 4x1: red, red, blue, red.
        let red = [255u8, 0, 0, 255];
        let blue = [0u8, 0, 255, 255];
        let rgba: Vec<u8> = [red, red, blue, red].concat();
        let contiguous = Selection::from_color(&rgba, 4, 1, (0, 0), 10, true).expect("wand");
        assert_eq!(contiguous.coverage(1.5, 0.5), 1.0);
        assert_eq!(contiguous.coverage(2.5, 0.5), 0.0);
        assert_eq!(contiguous.coverage(3.5, 0.5), 0.0, "cut off by blue");
        let global = Selection::from_color(&rgba, 4, 1, (0, 0), 10, false).expect("wand");
        assert_eq!(global.coverage(3.5, 0.5), 1.0, "non-contiguous");
        assert!(Selection::from_color(&rgba, 4, 1, (9, 0), 10, true).is_none());
    }

    #[test]
    fn bitmap_outline_traces_the_boundary() {
        let square = [(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)];
        let sel = Selection::from_polygon(&square, 10, 10).expect("lasso");
        let runs = sel.outline(0);
        assert_eq!(runs.len(), 4, "one run per side");
        let total: f32 = runs
            .iter()
            .map(|run| (run[1].0 - run[0].0).abs() + (run[1].1 - run[0].1).abs())
            .sum();
        assert!((total - 16.0).abs() < 1e-4, "perimeter {total}");
    }

    #[test]
    fn selection_ops_combine_coverage() {
        let left = Selection::from_drag(SelectionKind::Rect, (0.0, 0.0), (6.0, 4.0));
        let right = Selection::from_drag(SelectionKind::Rect, (4.0, 0.0), (10.0, 4.0));
        let at = |sel: &Option<Selection>, x: f32| sel.as_ref().map_or(0.0, |s| s.coverage(x, 1.5));
        let add = SelectionOp::Add.apply(left.as_ref(), right.clone(), 10, 4);
        assert_eq!((at(&add, 1.5), at(&add, 8.5)), (1.0, 1.0));
        let sub = SelectionOp::Subtract.apply(left.as_ref(), right.clone(), 10, 4);
        assert_eq!((at(&sub, 1.5), at(&sub, 5.5)), (1.0, 0.0));
        let inter = SelectionOp::Intersect.apply(left.as_ref(), right.clone(), 10, 4);
        assert_eq!((at(&inter, 1.5), at(&inter, 5.5)), (0.0, 1.0));
        assert_eq!(
            SelectionOp::Replace.apply(left.as_ref(), right.clone(), 10, 4),
            right
        );
        // Nothing active: add = new, subtract / intersect = nothing.
        assert_eq!(SelectionOp::Add.apply(None, right.clone(), 10, 4), right);
        assert_eq!(
            SelectionOp::Subtract.apply(None, right.clone(), 10, 4),
            None
        );
        // Subtracting everything leaves no selection.
        assert_eq!(
            SelectionOp::Subtract.apply(left.as_ref(), Some(Selection::all(10, 4)), 10, 4),
            None
        );
        assert_eq!(
            SelectionOp::from_modifiers(true, true),
            SelectionOp::Intersect
        );
        assert_eq!(
            SelectionOp::from_modifiers(false, true),
            SelectionOp::Subtract
        );
    }
}
