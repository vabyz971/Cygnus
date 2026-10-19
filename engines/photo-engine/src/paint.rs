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

//! Destructive layer paint: brush or eraser stroke
//! rasterization.
//!
//! The stroke is stamped into a coverage mask (avoids darkening on
//! overlaps), then composited onto the base:
//! - [`StrokeMode::Paint`]: uniform-opacity source-over — like a real
//!   brush stroke
//! - [`StrokeMode::Erase`]: destination-out — reduces the alpha of
//!   targeted pixels without touching their color

/// What the stroke does to the layer's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeMode {
    /// Paints the brush color (source-over)
    Paint,
    /// Erases: reduces alpha proportionally to the stroke opacity
    Erase,
}

/// Settings of a stroke tool (brush or eraser).
#[derive(Clone, Copy, Debug)]
pub struct BrushParams {
    /// Radius in DOCUMENT pixels (the canvas visual space). `commit_stroke`
    /// converts it to an ellipse in layer space via the transform.
    pub radius: f32,
    /// RGB color (ignored in Erase mode)
    pub color: [u8; 3],
    /// Overall stroke opacity [0..1] (internal overlaps do NOT accumulate
    /// — a single composite at the end)
    pub opacity: f32,
    pub mode: StrokeMode,
}

/// Rasterizes a circular stroke into an RGBA8 buffer (w×h, LAYER space).
///
/// * `points`: polyline in layer coordinates (pixel (0,0) center = 0.0)
pub fn paint_stroke_rgba(rgba: &mut [u8], w: u32, h: u32, points: &[(f32, f32)], b: &BrushParams) {
    paint_stroke_impl(rgba, w, h, points, b, b.radius, b.radius);
}

/// Elliptical variant (independent X/Y radii) — lets the stroke follow a
/// NON-unit layer scale: a doc circle becomes an ellipse in layer space.
/// Radii are in LAYER pixels.
pub fn paint_stroke_ellipse(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    points: &[(f32, f32)],
    b: &BrushParams,
    rx: f32,
    ry: f32,
) {
    paint_stroke_impl(rgba, w, h, points, b, rx, ry);
}

/// A stroke's bounding box grown by the radius (`pad`), in floats (the
/// caller floors/clips). `None` if no points. Shared by the permanent
/// path and the interactive overlay (same geometry).
pub(crate) fn stroke_bbox(points: &[(f32, f32)], rx: f32, ry: f32) -> Option<(f32, f32, f32, f32)> {
    if points.is_empty() {
        return None;
    }
    let pad = rx.max(ry).ceil() + 1.0;
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for &(x, y) in points {
        min_x = min_x.min(x - pad);
        min_y = min_y.min(y - pad);
        max_x = max_x.max(x + pad);
        max_y = max_y.max(y + pad);
    }
    Some((min_x, min_y, max_x, max_y))
}

/// Stamps an elliptical disc into a coverage mask (absolute coordinates,
/// indexing relative to origin `(ox, oy)`, stride `stride`). The clip MUST
/// fit inside the mask (guaranteed by both callers: local box here,
/// overlay region there) — no pixel outside the mask.
// Deliberate positional signature: hot primitive shared by the permanent
// path and the overlay (a struct would group with no gain).
#[allow(clippy::too_many_arguments)]
pub(crate) fn stamp_disc(
    mask: &mut [u8],
    stride: usize,
    ox: i64,
    oy: i64,
    clip_x0: i64,
    clip_y0: i64,
    clip_x1: i64,
    clip_y1: i64,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
) {
    let x0 = (cx - rx).floor().max(clip_x0 as f32) as i64;
    let x1 = (cx + rx).ceil().min(clip_x1 as f32) as i64;
    let y0 = (cy - ry).floor().max(clip_y0 as f32) as i64;
    let y1 = (cy + ry).ceil().min(clip_y1 as f32) as i64;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            // rx/ry-axis ellipse (circle when rx == ry)
            if (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1.0 {
                let mi = ((py - oy) as usize) * stride + ((px - ox) as usize);
                mask[mi] = 255;
            }
        }
    }
}

/// Stamps a polyline (first point + segments spaced ~radius/3 apart) into
/// a mask. The union is idempotent: stamping the same segments twice gives
/// the same mask (zero accumulation) — the property allowing incremental
/// piecewise stamping for the overlay.
// Deliberate positional signature: see `stamp_disc`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stamp_polyline(
    mask: &mut [u8],
    stride: usize,
    ox: i64,
    oy: i64,
    clip_x0: i64,
    clip_y0: i64,
    clip_x1: i64,
    clip_y1: i64,
    points: &[(f32, f32)],
    rx: f32,
    ry: f32,
) {
    if points.is_empty() {
        return;
    }
    // Dabs spaced along segments (~max-radius/3 step → continuous stroke)
    let step = (rx.max(ry) / 3.0).max(0.5);
    let mut prev = points[0];
    stamp_disc(
        mask, stride, ox, oy, clip_x0, clip_y0, clip_x1, clip_y1, prev.0, prev.1, rx, ry,
    );
    for &p in &points[1..] {
        let dx = p.0 - prev.0;
        let dy = p.1 - prev.1;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < f32::EPSILON {
            continue;
        }
        let n = (dist / step).ceil() as usize;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            stamp_disc(
                mask,
                stride,
                ox,
                oy,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
                prev.0 + dx * t,
                prev.1 + dy * t,
                rx,
                ry,
            );
        }
        prev = p;
    }
}

/// Resamples a gesture polyline (1 point/frame) into a smooth curve:
/// segment subdivision beyond `max_gap`, then one Chaikin pass (cuts
/// corners, preserves endpoints). Without it, a fast stroke (sparse
///
/// points) renders as straight segments between samples — all the more
/// visible the longer the stroke and the lower the fps. Slow gestures
/// (dense points) are nearly unchanged (corners already fine). The
/// result stays within the inputs' convex hull (valid conservative
/// bbox).
#[must_use]
pub fn resample_stroke(points: &[(f32, f32)], max_gap: f32) -> Vec<(f32, f32)> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let max_gap = max_gap.max(1.0);
    // Subdivision: no segment exceeds `max_gap`.
    let mut dense: Vec<(f32, f32)> = Vec::with_capacity(points.len() * 2);
    dense.push(points[0]);
    for window in points.windows(2) {
        let (ax, ay) = window[0];
        let (bx, by) = window[1];
        let dist = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        if dist.is_finite() && dist > max_gap {
            let n = (dist / max_gap).ceil() as usize;
            for i in 1..n {
                let t = i as f32 / n as f32;
                dense.push((ax + (bx - ax) * t, ay + (by - ay) * t));
            }
        }
        dense.push((bx, by));
    }
    if dense.len() < 3 {
        return dense;
    }
    // Chaikin: Q = 3/4·P + 1/4·next, R = 1/4·P + 3/4·next.
    let mut smooth: Vec<(f32, f32)> = Vec::with_capacity(dense.len() * 2);
    smooth.push(dense[0]);
    for window in dense.windows(2) {
        let (ax, ay) = window[0];
        let (bx, by) = window[1];
        smooth.push((ax * 0.75 + bx * 0.25, ay * 0.75 + by * 0.25));
        smooth.push((ax * 0.25 + bx * 0.75, ay * 0.25 + by * 0.75));
    }
    smooth.push(*dense.last().expect("non vide"));
    smooth
}

/// Composites a coverage mask onto an RGBA buffer (same formulas as the
/// legacy path: uniform-opacity source-over, eraser destination-out).
/// `rgba` is indexed absolutely (`stride` = buffer width); `mask` covers
/// exactly rect `(x0, y0, w, h)` (implicit origin).
// Deliberate positional signature: see `stamp_disc`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_coverage(
    rgba: &mut [u8],
    stride: usize,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    mask: &[u8],
    mask_stride: usize,
    b: &BrushParams,
) {
    let opacity = b.opacity.clamp(0.0, 1.0);
    // --- Composite by mode ---
    let a_paint = opacity;
    let (cr, cg, cb) = (b.color[0] as f32, b.color[1] as f32, b.color[2] as f32);
    for my in 0..h {
        for mx in 0..w {
            let cov = mask[my as usize * mask_stride + mx as usize] as f32 / 255.0;
            if cov <= 0.0 {
                continue;
            }
            let a = a_paint * cov;
            let x = x0 + mx;
            let y = y0 + my;
            let idx = ((y as usize * stride) + x as usize) * 4;
            let sa = rgba[idx + 3] as f32 / 255.0;
            match b.mode {
                StrokeMode::Paint => {
                    let sr = rgba[idx] as f32;
                    let sg = rgba[idx + 1] as f32;
                    let sb = rgba[idx + 2] as f32;
                    // source-over : out = src*a + dst*(1-a)
                    let out_a = a + sa * (1.0 - a);
                    if out_a <= 0.0 {
                        continue;
                    }
                    rgba[idx] = ((cr * a + sr * sa * (1.0 - a)) / out_a)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    rgba[idx + 1] = ((cg * a + sg * sa * (1.0 - a)) / out_a)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    rgba[idx + 2] = ((cb * a + sb * sa * (1.0 - a)) / out_a)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    rgba[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                StrokeMode::Erase => {
                    // destination-out: alpha is reduced, color stays valid
                    // (straight pixels — RGB unchanged as alpha drops).
                    let out_a = sa * (1.0 - a);
                    rgba[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

fn paint_stroke_impl(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    points: &[(f32, f32)],
    b: &BrushParams,
    rx: f32,
    ry: f32,
) {
    if points.is_empty() || w == 0 || h == 0 || rx <= 0.0 || ry <= 0.0 || b.opacity <= 0.0 {
        return;
    }
    let rx = rx.max(0.5);
    let ry = ry.max(0.5);

    // --- Stroke bounding box (limited to the layer) ---
    let Some((min_x, min_y, max_x, max_y)) = stroke_bbox(points, rx, ry) else {
        return;
    };
    let bx0 = min_x.floor().max(0.0) as u32;
    let by0 = min_y.floor().max(0.0) as u32;
    let bx1 = (max_x.ceil() as u32).min(w);
    let by1 = (max_y.ceil() as u32).min(h);
    if bx0 >= bx1 || by0 >= by1 {
        return;
    }
    let bw = (bx1 - bx0) as usize;
    let bh = (by1 - by0) as usize;

    // --- 0/255 coverage mask ---
    let mut mask = vec![0u8; bw * bh];
    stamp_polyline(
        &mut mask, bw, bx0 as i64, by0 as i64, bx0 as i64, by0 as i64, bx1 as i64, by1 as i64,
        points, rx, ry,
    );

    composite_coverage(
        rgba, w as usize, bx0, by0, bw as u32, bh as u32, &mask, bw, b,
    );
}

/// A stroke commit's result: buffers ready for the UI layer (no heavy
/// computation left interface-side).
#[derive(Clone)]
pub struct StrokeCommit {
    /// Full layer pixels (w×h RGBA8)
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Interactive preview (≤2048 px) — pure buffer, UI conversion app-side
    pub preview: crate::document::RgbaBuf,
    /// 48×32 thumbnail — pure buffer
    pub thumb: crate::document::RgbaBuf,
}

impl std::fmt::Debug for StrokeCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Pixels deliberately omitted (bulky buffers)
        f.debug_struct("StrokeCommit")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// HEAVY work of a brush/eraser stroke — run OFF the UI thread
/// (`Task::perform`): buffer copy, rasterization, preview, thumbnail.
///
/// * `base`: layer source image (shared Arc, unmodified)
/// * `pts_doc`: polyline in DOCUMENT coordinates
/// * `transform`: layer's current transform (doc → layer)
pub fn commit_stroke(
    base: &image::DynamicImage,
    pts_doc: &[(f32, f32)],
    transform: &crate::document::Transform2D,
    brush: &BrushParams,
) -> StrokeCommit {
    // Stroke pipeline confined to the dedicated render pool (invariant #4).
    crate::render_pool::run_parallel(|| commit_stroke_locked(base, pts_doc, transform, brush))
}

/// [`commit_stroke`] body — never called directly.
fn commit_stroke_locked(
    base: &image::DynamicImage,
    pts_doc: &[(f32, f32)],
    transform: &crate::document::Transform2D,
    brush: &BrushParams,
) -> StrokeCommit {
    use ::image::GenericImageView;
    let (lw, lh) = base.dimensions();

    // DOCUMENT space → LAYER space: full inverse affine (non-uniform
    // scale + skew + rotation inverted around the layer center) — same as
    // the draw and prepare_top transform.
    let ox = transform.offset_x;
    let oy = transform.offset_y;
    let sx = transform.scale_x.clamp(0.05, 8.0);
    let sy = transform.scale_y.clamp(0.05, 8.0);
    // Canonical shear×scale matrix (same source as compositing).
    let (m00, m01, m10, m11) = crate::document::Transform2D {
        scale_x: sx,
        scale_y: sy,
        ..*transform
    }
    .shear_scale_matrix();
    let rad = transform.rotation_deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let (lw2, lh2) = (lw as f32 / 2.0, lh as f32 / 2.0);
    // M = K*S = [[sx, kx*sy],[ky*sx, sy]]
    let det = m00 * m11 - m01 * m10;
    let pts: Vec<(f32, f32)> = if det.abs() < 1e-4 {
        // Degenerate shear: fall back to the old uniform path.
        let cx = ox + lw2 * sx;
        let cy = oy + lh2 * sy;
        pts_doc
            .iter()
            .map(|&(dx, dy)| {
                let (rx, ry) = (
                    (dx - cx) * cos - (dy - cy) * sin,
                    (dx - cx) * sin + (dy - cy) * cos,
                );
                (rx / sx + lw2, ry / sy + lh2)
            })
            .collect()
    } else {
        pts_doc
            .iter()
            .map(|&(dx, dy)| {
                // q - T where T = scaled rectangle center (offset + half-extents)
                let ux = dx - ox - lw2 * sx;
                let uy = dy - oy - lh2 * sy;
                let rx = ux * cos + uy * sin;
                let ry = -ux * sin + uy * cos;
                let plx = (m11 * rx - m01 * ry) / det;
                let ply = (-m10 * rx + m00 * ry) / det;
                (plx + lw2, ply + lh2)
            })
            .collect()
    };

    let mut rgba = base.to_rgba8().into_raw();
    // The RADIUS lives in doc space (the cursor's): it is brought back
    // into layer space via scales (min 0.5 px). Doc circle → layer ellipse
    // as soon as scale_x ≠ scale_y — on screen, the painted zone exactly
    // hugs the cursor whatever the layer size. (The layer's rotation/shear
    // does not change the shape seen from doc when sx == sy; a straight
    // ellipse is a good approximation
    // otherwise.)
    let r_x = (brush.radius / sx).max(0.5);
    let r_y = (brush.radius / sy).max(0.5);
    paint_stroke_ellipse(&mut rgba, lw, lh, &pts, brush, r_x, r_y);
    // Length guaranteed by construction: to_rgba8().into_raw() returns w*h*4
    let painted = ::image::DynamicImage::ImageRgba8(
        ::image::RgbaImage::from_raw(lw, lh, rgba.clone())
            .unwrap_or_else(|| ::image::RgbaImage::new(lw, lh)),
    );

    StrokeCommit {
        width: lw,
        height: lh,
        rgba,
        preview: crate::document::preview_buf(&painted),
        thumb: crate::document::thumb_buf(&painted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_conserve_points_denses_et_adoucit_les_angles() {
        // Slow gesture: nearly unchanged (same endpoints, no blow-up).
        let dense = vec![(0.0, 0.0), (1.0, 0.5), (2.0, 1.0), (3.0, 1.5)];
        let out = resample_stroke(&dense, 4.0);
        assert_eq!(out.first(), Some(&(0.0, 0.0)));
        assert_eq!(out.last(), Some(&(3.0, 1.5)));
        assert!(out.len() < dense.len() * 4);
        // Sparse right angle: the corner is cut (no point left exactly on
        // the apex, replaced by Chaikin Q/R).
        let angle = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let out = resample_stroke(&angle, 4.0);
        assert!(out.len() > angle.len(), "subdivision + Chaikin");
        assert_eq!(out.first(), Some(&(0.0, 0.0)));
        assert_eq!(out.last(), Some(&(10.0, 10.0)));
        assert!(
            !out.iter().any(|&(x, y)| x == 10.0 && y == 0.0),
            "sommet (10,0) adouci : {out:?}"
        );
        // Edge cases: empty, single point, degenerate segment.
        assert!(resample_stroke(&[], 4.0).is_empty());
        assert_eq!(resample_stroke(&[(1.0, 2.0)], 4.0), vec![(1.0, 2.0)]);
    }

    #[test]
    fn trait_opaque_sur_fond_transparent() {
        let w = 16u32;
        let h = 16u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        paint_stroke_rgba(
            &mut rgba,
            w,
            h,
            &[(4.0, 8.0), (12.0, 8.0)],
            &BrushParams {
                radius: 2.0,
                color: [255, 0, 0],
                opacity: 1.0,
                mode: StrokeMode::Paint,
            },
        );
        // Stroke center: opaque red
        let idx = ((8 * w + 8) * 4) as usize;
        assert_eq!(rgba[idx], 255);
        assert_eq!(rgba[idx + 3], 255);
        // Corner outside the stroke: transparent
        assert_eq!(rgba[3], 0);
    }

    #[test]
    fn opacite_50_sur_fond_blanc() {
        let w = 8u32;
        let h = 8u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        paint_stroke_rgba(
            &mut rgba,
            w,
            h,
            &[(4.0, 4.0)],
            &BrushParams {
                radius: 2.0,
                color: [0, 0, 0],
                opacity: 0.5,
                mode: StrokeMode::Paint,
            },
        );
        let idx = ((4 * w + 4) * 4) as usize;
        assert_eq!(rgba[idx], 128); // 50/50 mix
        assert_eq!(rgba[idx + 3], 255); // opaque background preserved
    }

    #[test]
    fn gomme_opaque_efface_le_centre_preserve_les_bords() {
        let w = 16u32;
        let h = 16u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize]; // opaque white
        paint_stroke_rgba(
            &mut rgba,
            w,
            h,
            &[(8.0, 8.0)],
            &BrushParams {
                radius: 3.0,
                color: [0, 0, 0],
                opacity: 1.0,
                mode: StrokeMode::Erase,
            },
        );
        let centre = ((8 * w + 8) * 4) as usize;
        assert_eq!(rgba[centre + 3], 0); // alpha erased
        assert_eq!(rgba[centre], 255); // RGB unchanged (straight)
        let coin = 0usize; // document pixel (0, 0), outside the stroke
        assert_eq!(rgba[coin + 3], 255); // outside the stroke: intact
    }

    #[test]
    fn gomme_50_reduit_alpha_de_moitie() {
        let w = 8u32;
        let h = 8u32;
        let mut rgba = vec![200u8; (w * h * 4) as usize];
        paint_stroke_rgba(
            &mut rgba,
            w,
            h,
            &[(4.0, 4.0)],
            &BrushParams {
                radius: 2.0,
                color: [0, 0, 0],
                opacity: 0.5,
                mode: StrokeMode::Erase,
            },
        );
        let idx = ((4 * w + 4) * 4) as usize;
        assert!((rgba[idx + 3] as i16 - 100).abs() <= 1); // 200 → ≈100
        assert_eq!(rgba[idx], 200); // color preserved
    }

    #[test]
    fn gomme_sur_pixel_deja_transparent_sans_effet_bord() {
        let w = 8u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize]; // all transparent
        paint_stroke_rgba(
            &mut rgba,
            w,
            h,
            &[(4.0, 4.0)],
            &BrushParams {
                radius: 2.0,
                color: [9, 9, 9],
                opacity: 1.0,
                mode: StrokeMode::Erase,
            },
        );
        // Nothing can become opaque or colored via an eraser
        assert!(rgba.iter().all(|&v| v == 0));
    }

    #[test]
    fn ellipse_respecte_les_rayons_d_axes() {
        let w = 40u32;
        let h = 40u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        paint_stroke_ellipse(
            &mut rgba,
            w,
            h,
            &[(20.0, 20.0)],
            &BrushParams {
                radius: 8.0,
                color: [255, 0, 0],
                opacity: 1.0,
                mode: StrokeMode::Paint,
            },
            8.0, // rx
            3.0, // ry
        );
        let px = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize] > 0;
        assert!(px(20, 20), "centre");
        assert!(px(20 - 7, 20), "bord gauche (rx)");
        assert!(px(20 + 7, 20), "bord droit (rx)");
        assert!(px(20, 20 - 2), "bord haut (ry)");
        assert!(px(20, 20 + 2), "bord bas (ry)");
        // Outside the ellipse (pixel center ≳ edge): transparent
        assert!(!px(20 + 8, 20), "au-delà de rx");
        assert!(!px(20, 20 + 3), "au-delà de ry");
        assert!(!px(4, 4), "coin lointain");
    }

    #[test]
    fn commit_stroke_sur_calque_redimensionne_adapte_le_rayon() {
        use crate::document::Transform2D;
        // 64×64 layer displayed at 50%: the DOC radius (10 px) must produce a
        // ~20 px LAYER disc (10 / 0.5) — both match on screen.
        let base = image::RgbaImage::new(64, 64); // transparent: only the painted zone counts
        let transform = Transform2D {
            offset_x: 100.0,
            offset_y: 100.0,
            scale_x: 0.5,
            scale_y: 0.5,
            ..Transform2D::default()
        };
        // Doc point at the layer center (offset + scaled half-extent).
        let centre_doc = (100.0 + 32.0 * 0.5, 100.0 + 32.0 * 0.5);
        let commit = commit_stroke(
            &image::DynamicImage::ImageRgba8(base.clone()),
            &[centre_doc],
            &transform,
            &BrushParams {
                radius: 10.0, // DOC PIXELS
                color: [255, 0, 0],
                opacity: 1.0,
                mode: StrokeMode::Paint,
            },
        );
        let rgba = commit.rgba;
        // Painted horizontal width measured in layer px (~ 2 × 20).
        let mut min_x = 64i32;
        let mut max_x = -1i32;
        for y in 0..64u32 {
            for x in 0..64u32 {
                if rgba[((y * 64 + x) * 4 + 3) as usize] > 0 {
                    min_x = min_x.min(x as i32);
                    max_x = max_x.max(x as i32);
                }
            }
        }
        let painted = (max_x - min_x + 1).max(0);
        assert!(painted >= 36, "disque ~40 px calque, mesuré {painted}");
        assert!(painted <= 44, "disque ~40 px calque, mesuré {painted}");
    }
}
