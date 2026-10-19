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

//! CPU rasterization of vector layers (spec §2: SDF at draw, CPU
//! preview worker-side).
//!
//! Closed paths are flattened ([`crate::scene::flatten`]) then filled
//! with the layer color (even-odd rule). Open paths are skipped
//! (strokes arrive with the style slice); layer transform and blend
//! apply later at composite time (state-only rendering).

use image::{ImageBuffer, Rgba};

use super::VectorLayer;
use super::pen::{flatten, path_bounds, translate};

/// Fill subdivisions per curve (matches preview quality; curves
/// stay analytic in the layer — this only affects pixels).
const CURVE_SUBDIV: u32 = 16;

/// Union bounds of closed paths (`None` when empty).
///
/// Open strokes are excluded (no stroke model yet — bounds would
/// lie about the painted area).
pub fn paths_bounds(paths: &[super::VectorPath]) -> Option<datatypes::Rect> {
    let mut acc: Option<datatypes::Rect> = None;
    for path in paths.iter().filter(|p| p.closed) {
        let Some(b) = path_bounds(path) else {
            continue;
        };
        acc = Some(match acc {
            None => b,
            Some(a) => datatypes::Rect::new(
                a.x0.min(b.x0),
                a.y0.min(b.y0),
                a.x1.max(b.x1),
                a.y1.max(b.y1),
            ),
        });
    }
    acc
}

/// Union bounds of a layer's closed paths (`None` when empty).
///
/// Open strokes are excluded (no stroke model yet — bounds would
/// lie about the painted area).
pub fn vector_bounds(layer: &VectorLayer) -> Option<datatypes::Rect> {
    paths_bounds(&layer.paths)
}

/// Rasterize a layer shifted so its bounds origin lands on (0, 0).
///
/// Returns the image (bounds size, ≥1×1) and the origin that must be
/// folded into the placement transform. `None` for empty layers.
pub fn rasterize_bounds(layer: &VectorLayer) -> Option<(image::DynamicImage, datatypes::Vec2)> {
    let bounds = vector_bounds(layer)?;
    let origin = datatypes::Vec2::new(bounds.x0.floor(), bounds.y0.floor());
    let w = (bounds.x1.ceil() - origin.x).max(1.0) as u32;
    let h = (bounds.y1.ceil() - origin.y).max(1.0) as u32;
    let shifted = VectorLayer {
        paths: layer
            .paths
            .iter()
            .map(|p| translate(p, -origin.x, -origin.y))
            .collect(),
        fill: layer.fill,
        filter_layers: Vec::new(),
        masks: Vec::new(),
        appearance_version: 0,
    };
    let img = rasterize(&shifted, w, h);
    Some((image::DynamicImage::ImageRgba8(img), origin))
}

/// Placement transform for paths: bounds origin folded into the
/// offset (the appearance raster is bounds-shifted — same helper
/// for compositing, overlay and hit-testing, single source of
/// truth for "where path point `p` renders").
pub fn vector_placement(
    transform: &math_utils::Transform2D,
    paths: &[super::VectorPath],
) -> Option<math_utils::Transform2D> {
    let bounds = paths_bounds(paths)?;
    let mut placed = *transform;
    placed.offset_x += bounds.x0.floor();
    placed.offset_y += bounds.y0.floor();
    Some(placed)
}

/// Document position of a layer-space path point (overlay nodes,
/// hit-testing). `None` for empty layers.
pub fn place_vector_point(
    transform: &math_utils::Transform2D,
    paths: &[super::VectorPath],
    point: datatypes::Vec2,
) -> Option<(f32, f32)> {
    let bounds = paths_bounds(paths)?;
    let origin = (bounds.x0.floor(), bounds.y0.floor());
    let bw = (bounds.x1.ceil() - origin.0).max(1.0);
    let bh = (bounds.y1.ceil() - origin.1).max(1.0);
    let mut placed = *transform;
    placed.offset_x += origin.0;
    placed.offset_y += origin.1;
    Some(placed.local_to_doc(bw, bh, point.x - origin.0, point.y - origin.1))
}

/// Rasterize a vector layer onto a transparent canvas.
///
/// Paths are interpreted in output-pixel space (layer space =
/// pixel space; the layer transform applies at composite time).
/// Later paths paint over earlier ones; the fill color is written
/// as-is (straight alpha — blending happens at composite time).
pub fn rasterize(layer: &VectorLayer, width: u32, height: u32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut out = ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    if width == 0 || height == 0 {
        return out;
    }
    let color = Rgba(layer.fill);
    for path in &layer.paths {
        if !path.closed {
            continue;
        }
        let poly = flatten(path, CURVE_SUBDIV);
        if poly.len() < 3 {
            continue;
        }
        fill_even_odd(&mut out, &poly, color);
    }
    out
}

/// Even-odd scanline fill of a closed polyline (pixel centers tested).
fn fill_even_odd(
    out: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    poly: &[datatypes::Vec2],
    color: Rgba<u8>,
) {
    let (w, h) = (out.width() as i32, out.height() as i32);
    // Vertical bounds of the polygon (clamped row range).
    let mut ymin = h;
    let mut ymax = -1;
    for p in poly {
        ymin = ymin.min(p.y.floor() as i32);
        ymax = ymax.max(p.y.ceil() as i32);
    }
    ymin = ymin.max(0);
    ymax = ymax.min(h - 1);
    if ymin > ymax {
        return;
    }
    let mut xs = Vec::new();
    for y in ymin..=ymax {
        let scan = y as f32 + 0.5;
        xs.clear();
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            // Half-open crossing test: each vertex counted once.
            if (a.y <= scan && b.y > scan) || (b.y <= scan && a.y > scan) {
                let t = (scan - a.y) / (b.y - a.y);
                xs.push(a.x + t * (b.x - a.x));
            }
        }
        xs.sort_by(|x, y| x.total_cmp(y));
        let (pairs, _) = xs.as_chunks::<2>();
        for pair in pairs {
            let (mut x0, mut x1) = (pair[0], pair[1]);
            if x1 < x0 {
                std::mem::swap(&mut x0, &mut x1);
            }
            // Pixel px covered iff center px+0.5 in [x0, x1).
            let mut px = (x0 - 0.5).ceil() as i32;
            let end = (x1 - 0.5).ceil() as i32;
            px = px.max(0);
            let end = end.min(w);
            let mut x = px;
            while x < end {
                out.put_pixel(x as u32, y as u32, color);
                x += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PenPath, rect, star};

    fn alpha_count(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> u32 {
        img.pixels().filter(|p| p[3] > 0).count() as u32
    }

    #[test]
    fn rect_fills_exact_integer_area() {
        let layer = rect(2.0, 2.0, 6.0, 6.0);
        let img = rasterize(&layer, 10, 10);
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert_eq!(img.get_pixel(9, 9)[3], 0);
        assert_eq!(img.get_pixel(2, 2)[3], 255);
        assert_eq!(img.get_pixel(7, 7)[3], 255);
        assert_eq!(img.get_pixel(8, 8)[3], 0);
        assert_eq!(alpha_count(&img), 36);
    }

    #[test]
    fn empty_and_open_paths_paint_nothing() {
        let empty = VectorLayer {
            paths: vec![],
            fill: [255, 0, 0, 255],
            filter_layers: Vec::new(),
            masks: Vec::new(),
            appearance_version: 0,
        };
        assert_eq!(alpha_count(&rasterize(&empty, 10, 10)), 0);
        assert_eq!(rasterize(&empty, 0, 10).dimensions(), (0, 10));
        // Open stroke: skipped until the style slice.
        let mut pen = PenPath::new();
        pen.move_to(datatypes::Vec2::new(1.0, 1.0))
            .line_to(datatypes::Vec2::new(8.0, 8.0));
        let open = VectorLayer {
            paths: vec![pen.build().expect("open path")],
            fill: [255, 0, 0, 255],
            filter_layers: Vec::new(),
            masks: Vec::new(),
            appearance_version: 0,
        };
        assert_eq!(alpha_count(&rasterize(&open, 10, 10)), 0);
    }

    #[test]
    fn star_area_stays_within_bounds() {
        let layer = star(10.0, 10.0, 9.0, 4.0, 5);
        let img = rasterize(&layer, 20, 20);
        let n = alpha_count(&img);
        assert!(n > 40 && n < 200, "star pixels: {n}");
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert_eq!(img.get_pixel(10, 10)[3], 255);
    }

    #[test]
    fn bounds_shift_rasterizes_off_origin_shapes() {
        let layer = rect(100.0, 50.0, 6.0, 4.0);
        let (img, origin) = rasterize_bounds(&layer).expect("bounded layer");
        assert_eq!(origin, datatypes::Vec2::new(100.0, 50.0));
        assert_eq!((img.width(), img.height()), (6, 4));
        assert_eq!(alpha_count(&img.to_rgba8()), 24);
        let empty = VectorLayer {
            paths: vec![],
            fill: [0, 0, 0, 255],
            filter_layers: Vec::new(),
            masks: Vec::new(),
            appearance_version: 0,
        };
        assert!(vector_bounds(&empty).is_none());
        assert!(rasterize_bounds(&empty).is_none());
    }

    #[test]
    fn placement_maps_layer_points_to_document() {
        use math_utils::Transform2D;
        // Off-origin shape, translated layer: node positions must land
        // where the raster does (overlay/hit-test contract).
        let layer = rect(100.0, 50.0, 6.0, 4.0);
        let t = Transform2D {
            offset_x: 10.0,
            offset_y: 20.0,
            ..Transform2D::default()
        };
        let placed = vector_placement(&t, &layer.paths).expect("placed");
        assert_eq!((placed.offset_x, placed.offset_y), (110.0, 70.0));
        let doc = place_vector_point(&t, &layer.paths, datatypes::Vec2::new(100.0, 50.0))
            .expect("placed point");
        assert_eq!(doc, (110.0, 70.0));
        assert!(place_vector_point(&t, &[], datatypes::Vec2::new(0.0, 0.0)).is_none());
    }
}
