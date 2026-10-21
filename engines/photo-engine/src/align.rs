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

//! Align / distribute layers (pure geometry on document-space boxes).
//!
//! [`plan`] turns the layers' bounding boxes into per-layer moves: with
//! several layers they align on their combined bounds, a single layer
//! aligns on the document; distributing spaces the centers evenly
//! between the outermost two (three layers or more).

/// Alignment or distribution of the selected layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerAlign {
    /// Left edges.
    Left,
    /// Horizontal centers.
    CenterH,
    /// Right edges.
    Right,
    /// Top edges.
    Top,
    /// Vertical centers.
    Middle,
    /// Bottom edges.
    Bottom,
    /// Even horizontal spacing of the centers.
    DistributeH,
    /// Even vertical spacing of the centers.
    DistributeV,
}

impl LayerAlign {
    /// Every operation, in toolbar order.
    pub const ALL: [LayerAlign; 8] = [
        LayerAlign::Left,
        LayerAlign::CenterH,
        LayerAlign::Right,
        LayerAlign::Top,
        LayerAlign::Middle,
        LayerAlign::Bottom,
        LayerAlign::DistributeH,
        LayerAlign::DistributeV,
    ];
}

/// Axis-aligned box `(x0, y0, x1, y1)` in document pixels.
pub type Bounds = (f32, f32, f32, f32);

/// Box around `points`.
#[must_use]
pub fn bounds_of(points: &[(f32, f32)]) -> Option<Bounds> {
    let first = points.first()?;
    Some(points.iter().fold(
        (first.0, first.1, first.0, first.1),
        |(x0, y0, x1, y1), (x, y)| (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y)),
    ))
}

/// Moves `(dx, dy)` per box (same order) for `align`; `document` is the
/// reference of a single box. Distributing fewer than three boxes
/// moves nothing.
#[must_use]
pub fn plan(boxes: &[Bounds], align: LayerAlign, document: Bounds) -> Vec<(f32, f32)> {
    let reference = match boxes {
        [] => return Vec::new(),
        [_] => document,
        _ => boxes.iter().skip(1).fold(boxes[0], |(x0, y0, x1, y1), b| {
            (x0.min(b.0), y0.min(b.1), x1.max(b.2), y1.max(b.3))
        }),
    };
    let (rx0, ry0, rx1, ry1) = reference;
    let center = |b: &Bounds| ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    match align {
        LayerAlign::Left => boxes.iter().map(|b| (rx0 - b.0, 0.0)).collect(),
        LayerAlign::Right => boxes.iter().map(|b| (rx1 - b.2, 0.0)).collect(),
        LayerAlign::CenterH => boxes
            .iter()
            .map(|b| ((rx0 + rx1) / 2.0 - center(b).0, 0.0))
            .collect(),
        LayerAlign::Top => boxes.iter().map(|b| (0.0, ry0 - b.1)).collect(),
        LayerAlign::Bottom => boxes.iter().map(|b| (0.0, ry1 - b.3)).collect(),
        LayerAlign::Middle => boxes
            .iter()
            .map(|b| (0.0, (ry0 + ry1) / 2.0 - center(b).1))
            .collect(),
        LayerAlign::DistributeH | LayerAlign::DistributeV => {
            let horizontal = align == LayerAlign::DistributeH;
            let axis = |b: &Bounds| {
                let c = center(b);
                if horizontal { c.0 } else { c.1 }
            };
            if boxes.len() < 3 {
                return vec![(0.0, 0.0); boxes.len()];
            }
            let mut order: Vec<usize> = (0..boxes.len()).collect();
            order.sort_by(|a, b| axis(&boxes[*a]).total_cmp(&axis(&boxes[*b])));
            let first = axis(&boxes[order[0]]);
            let last = axis(&boxes[order[order.len() - 1]]);
            let step = (last - first) / (order.len() - 1) as f32;
            let mut moves = vec![(0.0, 0.0); boxes.len()];
            for (rank, index) in order.iter().enumerate() {
                let delta = first + step * rank as f32 - axis(&boxes[*index]);
                moves[*index] = if horizontal {
                    (delta, 0.0)
                } else {
                    (0.0, delta)
                };
            }
            moves
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: Bounds = (0.0, 0.0, 100.0, 50.0);

    #[test]
    fn single_layer_aligns_on_the_document() {
        let boxes = [(10.0, 10.0, 30.0, 20.0)];
        assert_eq!(plan(&boxes, LayerAlign::Left, DOC), vec![(-10.0, 0.0)]);
        assert_eq!(plan(&boxes, LayerAlign::Right, DOC), vec![(70.0, 0.0)]);
        assert_eq!(plan(&boxes, LayerAlign::CenterH, DOC), vec![(30.0, 0.0)]);
        assert_eq!(plan(&boxes, LayerAlign::Bottom, DOC), vec![(0.0, 30.0)]);
    }

    #[test]
    fn several_layers_align_on_their_combined_bounds() {
        let boxes = [(10.0, 0.0, 20.0, 10.0), (40.0, 20.0, 60.0, 30.0)];
        assert_eq!(
            plan(&boxes, LayerAlign::Left, DOC),
            vec![(0.0, 0.0), (-30.0, 0.0)]
        );
        assert_eq!(
            plan(&boxes, LayerAlign::Top, DOC),
            vec![(0.0, 0.0), (0.0, -20.0)]
        );
        assert_eq!(
            plan(&boxes, LayerAlign::Middle, DOC),
            vec![(0.0, 10.0), (0.0, -10.0)]
        );
    }

    #[test]
    fn distribute_spaces_centers_evenly() {
        // Centers at x = 5, 15, 95 → 5, 50, 95.
        let boxes = [
            (0.0, 0.0, 10.0, 10.0),
            (90.0, 0.0, 100.0, 10.0),
            (10.0, 0.0, 20.0, 10.0),
        ];
        assert_eq!(
            plan(&boxes, LayerAlign::DistributeH, DOC),
            vec![(0.0, 0.0), (0.0, 0.0), (35.0, 0.0)]
        );
        assert_eq!(
            plan(&boxes[..2], LayerAlign::DistributeH, DOC),
            vec![(0.0, 0.0), (0.0, 0.0)],
            "two layers: nothing to distribute"
        );
    }

    #[test]
    fn bounds_of_points() {
        assert_eq!(bounds_of(&[]), None);
        assert_eq!(
            bounds_of(&[(3.0, 4.0), (-1.0, 9.0), (2.0, 0.0)]),
            Some((-1.0, 0.0, 3.0, 9.0))
        );
    }
}
