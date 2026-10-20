use super::model::{
    BlendMode, FilterLayer, FilterNode, LayerMask, LayerNode, RgbaBuf, Transform2D,
};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use rayon::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

pub fn needs_fallback_in(nodes: &[LayerNode]) -> bool {
    for n in nodes {
        if !n.visible() || n.opacity() <= 0.01 {
            continue;
        }
        match n {
            LayerNode::Pixel(l) => {
                if l.blend_mode != BlendMode::Normal {
                    return true;
                }
                if l.transform.has_skew() {
                    // Skew: the fast path (1 texture per layer)
                    // cannot deform — CPU fallback required.
                    return true;
                }
            }
            LayerNode::Vector(v) => {
                if v.blend_mode != BlendMode::Normal {
                    return true;
                }
                if v.transform.has_skew() {
                    return true;
                }
            }
            LayerNode::Group(g) => {
                if g.opacity < 99.9
                    || g.blend_mode != BlendMode::Normal
                    || g.masks.iter().any(|m| m.enabled)
                    || needs_fallback_in(&g.children)
                {
                    return true;
                }
            }
            LayerNode::Adjustment(a) => {
                if a.filters.iter().any(|f| f.enabled) {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Display buffers
// ---------------------------------------------------------------------------

/// Interactive display buffer (downscaled beyond 2048 px, fast
/// box filter: indistinguishable on screen, ~10× faster than
/// Triangle on print documents).
pub fn preview_buf(image: &DynamicImage) -> RgbaBuf {
    const MAX_PREVIEW: u32 = 2048;
    let (w, h) = image.dimensions();
    if let (DynamicImage::ImageRgba8(buf), Some((nw, nh))) =
        (image, capped_thumbnail_dims(w, h, MAX_PREVIEW))
    {
        let out = fast_thumbnail_rgba8(buf, nw, nh);
        RgbaBuf::from_vec(out.width(), out.height(), out.into_raw())
    } else if w.max(h) <= MAX_PREVIEW {
        let rgba = image.to_rgba8();
        RgbaBuf::from_vec(rgba.width(), rgba.height(), rgba.into_raw())
    } else {
        let preview = image.resize(
            MAX_PREVIEW,
            MAX_PREVIEW,
            ::image::imageops::FilterType::Triangle,
        );
        let rgba = preview.to_rgba8();
        RgbaBuf::from_vec(rgba.width(), rgba.height(), rgba.into_raw())
    }
}

/// 48×32 thumbnail for the Layers panel (fast box filter:
/// indistinguishable at this size, ~20× faster than Triangle on
/// print documents).
pub fn thumb_buf(img: &DynamicImage) -> RgbaBuf {
    const NW: u32 = 48;
    const NH: u32 = 32;
    if let DynamicImage::ImageRgba8(buf) = img {
        let out = fast_thumbnail_rgba8(buf, NW, NH);
        RgbaBuf::from_vec(out.width(), out.height(), out.into_raw())
    } else {
        let t = img.resize(NW, NH, ::image::imageops::FilterType::Triangle);
        let rgba = t.to_rgba8();
        RgbaBuf::from_vec(rgba.width(), rgba.height(), rgba.into_raw())
    }
}

// ---------------------------------------------------------------------------
// Fast bit-exact downscale (Phase 6G.2 §3.5D)
// ---------------------------------------------------------------------------

/// Target dimensions of a bounded thumbnail (`max_dim`), exact replica
/// of `DynamicImage::thumbnail(max, max)` (aspect preserved, `fill=false`):
/// `None` if no shrinking (`max(w, h) <= max_dim`, legacy unchanged),
/// else `(nw, nh)` guaranteed non-zero. Same `f64` ops in the same
/// order as `resize_dimensions` (private, image 0.25) — fuzz-verified
/// against `thumbnail`'s output dimensions.
#[must_use]
pub fn capped_thumbnail_dims(w: u32, h: u32, max_dim: u32) -> Option<(u32, u32)> {
    if w.max(h) <= max_dim {
        return None;
    }
    // Exact mirror of resize_dimensions(w, h, max, max, fill=false).
    let wratio = f64::from(max_dim) / f64::from(w);
    let hratio = f64::from(max_dim) / f64::from(h);
    let ratio = wratio.min(hratio);
    let nw = ((f64::from(w) * ratio).round() as u64).max(1);
    let nh = ((f64::from(h) * ratio).round() as u64).max(1);
    Some(if nw > u64::from(u32::MAX) {
        let ratio = f64::from(u32::MAX) / f64::from(w);
        (u32::MAX, ((f64::from(h) * ratio).round() as u32).max(1))
    } else if nh > u64::from(u32::MAX) {
        let ratio = f64::from(u32::MAX) / f64::from(h);
        (((f64::from(w) * ratio).round() as u32).max(1), u32::MAX)
    } else {
        (nw as u32, nh as u32)
    })
}

/// Byte-identical RGBA8 box resampling matching
/// `image::imageops::thumbnail` (image 0.25), rows in parallel.
///
/// Replicates the reference algorithm EXACTLY: same bounds (`ceil` +
/// clamp in the same order), same `u32` sums, same integer averages
/// `(sum + n/2) / n`, same `f32` formulas in the same order and same
/// conversion (saturated truncation toward zero, like `NumCast`). Each
/// output pixel is independent (`y` then `x` accumulation like the reference):
/// so the result is identical sequential or parallel, whatever the
/// thread count — total determinism.
///
/// Runs on the DEDICATED render pool (`run_parallel`, never an extra
/// pool); null dimensions return an empty buffer like the reference.
/// Outside RGBA8 or degenerate case: the caller keeps the legacy
/// (`thumbnail`) path, never a silent divergence.
pub fn fast_thumbnail_rgba8(
    src: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    nw: u32,
    nh: u32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (w, h) = (src.width(), src.height());
    let mut out = ImageBuffer::from_pixel(nw, nh, Rgba([0, 0, 0, 0]));
    if w == 0 || h == 0 || nw == 0 || nh == 0 {
        return out;
    }
    let raw = src.as_raw();
    let x_ratio = w as f32 / nw as f32;
    let y_ratio = h as f32 / nh as f32;
    // Horizontal bounds precomputed ONCE (constant per column — same
    // values as the reference loop, without recomputing them per
    // row). Only vertical bounds stay per-row.
    let mut col_bounds: Vec<(u32, u32, f32, f32)> = Vec::with_capacity(nw as usize);
    for ox in 0..nw {
        let leftf = ox as f32 * x_ratio;
        let rightf = leftf + x_ratio;
        col_bounds.push((
            (leftf.ceil() as u32).clamp(0, w - 1),
            (rightf.ceil() as u32).clamp((leftf.ceil() as u32).clamp(0, w - 1), w),
            leftf,
            rightf,
        ));
    }
    let stride = nw as usize * 4;
    crate::render_pool::run_parallel(|| {
        out.as_flat_samples_mut()
            .samples
            .par_chunks_mut(stride)
            .enumerate()
            .for_each(|(oy, row)| {
                let oy = oy as u32;
                let bottomf = oy as f32 * y_ratio;
                let topf = bottomf + y_ratio;
                let bottom = (bottomf.ceil() as u32).clamp(0, h - 1);
                let top = (topf.ceil() as u32).clamp(bottom, h);
                for (ox, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let (left, right, leftf, rightf) = col_bounds[ox];
                    let (r, g, b, a) = if bottom != top && left != right {
                        sample_block(raw, w, left, right, bottom, top)
                    } else if bottom != top {
                        // left == right: horizontal fraction (same branching
                        // and same values as the reference).
                        let fraction = (leftf.fract() + rightf.fract()) / 2.0;
                        sample_fraction_horizontal(raw, w, right - 1, fraction, bottom, top)
                    } else if left != right {
                        // bottom == top: vertical fraction.
                        let fraction = (topf.fract() + bottomf.fract()) / 2.0;
                        sample_fraction_vertical(raw, w, left, right, top - 1, fraction)
                    } else {
                        // Crossed names kept from the reference (values matter,
                        // not names): horizontal ← topf/bottomf, vertical ←
                        // leftf/rightf.
                        let fraction_horizontal = (topf.fract() + bottomf.fract()) / 2.0;
                        let fraction_vertical = (leftf.fract() + rightf.fract()) / 2.0;
                        sample_fraction_both(
                            raw,
                            w,
                            right - 1,
                            fraction_horizontal,
                            top - 1,
                            fraction_vertical,
                        )
                    };
                    px[0] = r;
                    px[1] = g;
                    px[2] = b;
                    px[3] = a;
                }
            });
    });
    out
}

/// Integer average of a block (same `u32` sums and `(s + n/2) / n`).
fn sample_block(
    raw: &[u8],
    w: u32,
    left: u32,
    right: u32,
    bottom: u32,
    top: u32,
) -> (u8, u8, u8, u8) {
    let mut sum = [0u32; 4];
    for y in bottom..top {
        for x in left..right {
            let i = ((y * w + x) * 4) as usize;
            sum[0] += u32::from(raw[i]);
            sum[1] += u32::from(raw[i + 1]);
            sum[2] += u32::from(raw[i + 2]);
            sum[3] += u32::from(raw[i + 3]);
        }
    }
    let n = (right - left) * (top - bottom);
    let round = n / 2;
    (
        ((sum[0] + round) / n).min(255) as u8,
        ((sum[1] + round) / n).min(255) as u8,
        ((sum[2] + round) / n).min(255) as u8,
        ((sum[3] + round) / n).min(255) as u8,
    )
}

/// Saturated truncation toward zero: proven equivalent of `NumCast::from(f32)`
/// for `u8` over the whole reachable domain (averages in `[0, 255]`, the
/// reference would panic outside `(-1, 256)` — clamped here, never a crash).
#[inline]
fn cast_u8(v: f32) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

/// Horizontal blend of two columns (same `f32` formulas).
fn sample_fraction_horizontal(
    raw: &[u8],
    w: u32,
    left: u32,
    fract: f32,
    bottom: u32,
    top: u32,
) -> (u8, u8, u8, u8) {
    let mut sum_left = [0u32; 4];
    let mut sum_right = [0u32; 4];
    for y in bottom..top {
        let i = ((y * w + left) * 4) as usize;
        let j = ((y * w + left + 1) * 4) as usize;
        for c in 0..4 {
            sum_left[c] += u32::from(raw[i + c]);
            sum_right[c] += u32::from(raw[j + c]);
        }
    }
    let fact_right = fract / ((top - bottom) as f32);
    let fact_left = (1.0 - fract) / ((top - bottom) as f32);
    let mut out = [0u8; 4];
    for c in 0..4 {
        out[c] = cast_u8(fact_left * sum_left[c] as f32 + fact_right * sum_right[c] as f32);
    }
    (out[0], out[1], out[2], out[3])
}

/// Vertical blend of two rows (same `f32` formulas).
fn sample_fraction_vertical(
    raw: &[u8],
    w: u32,
    left: u32,
    right: u32,
    bottom: u32,
    fract: f32,
) -> (u8, u8, u8, u8) {
    let mut sum_bot = [0u32; 4];
    let mut sum_top = [0u32; 4];
    for x in left..right {
        let i = ((bottom * w + x) * 4) as usize;
        let j = (((bottom + 1) * w + x) * 4) as usize;
        for c in 0..4 {
            sum_bot[c] += u32::from(raw[i + c]);
            sum_top[c] += u32::from(raw[j + c]);
        }
    }
    let fact_top = fract / ((right - left) as f32);
    let fact_bot = (1.0 - fract) / ((right - left) as f32);
    let mut out = [0u8; 4];
    for c in 0..4 {
        out[c] = cast_u8(fact_bot * sum_bot[c] as f32 + fact_top * sum_top[c] as f32);
    }
    (out[0], out[1], out[2], out[3])
}

/// 2×2 bilinear blend (same `f32` formulas, same accesses).
fn sample_fraction_both(
    raw: &[u8],
    w: u32,
    left: u32,
    fract_h: f32,
    bottom: u32,
    fract_v: f32,
) -> (u8, u8, u8, u8) {
    let idx = |x: u32, y: u32| ((y * w + x) * 4) as usize;
    let bl = idx(left, bottom);
    let tl = idx(left, bottom + 1);
    let br = idx(left + 1, bottom);
    let tr = idx(left + 1, bottom + 1);
    let fact_tr = fract_v * fract_h;
    let fact_tl = fract_v * (1.0 - fract_h);
    let fact_br = (1.0 - fract_v) * fract_h;
    let fact_bl = (1.0 - fract_v) * (1.0 - fract_h);
    let mut out = [0u8; 4];
    for c in 0..4 {
        out[c] = cast_u8(
            fact_br * f32::from(raw[br + c])
                + fact_tr * f32::from(raw[tr + c])
                + fact_bl * f32::from(raw[bl + c])
                + fact_tl * f32::from(raw[tl + c]),
        );
    }
    (out[0], out[1], out[2], out[3])
}

// ---------------------------------------------------------------------------
// CPU blend primitives (unchanged, proven by the golden tests)
// ---------------------------------------------------------------------------

/// Per-channel math of the separable modes added after the first six
/// (W3C compositing formulas; `b` = base, `t` = top, both 0..=1). The
/// GPU shader (`gpu-compositor` `composite.wgsl`) mirrors it.
#[inline]
pub fn blend_separable(b: f32, t: f32, mode: u32) -> f32 {
    match mode {
        // Color Dodge
        6 => {
            if b <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                (b / (1.0 - t)).min(1.0)
            }
        }
        // Color Burn
        7 => {
            if b >= 1.0 {
                1.0
            } else if t <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / t).min(1.0)
            }
        }
        // Hard Light (overlay with the layers swapped)
        8 => {
            if t < 0.5 {
                2.0 * b * t
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - t)
            }
        }
        // Soft Light
        9 => {
            if t <= 0.5 {
                b - (1.0 - 2.0 * t) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 {
                    ((16.0 * b - 12.0) * b + 4.0) * b
                } else {
                    b.sqrt()
                };
                b + (2.0 * t - 1.0) * (d - b)
            }
        }
        // Difference
        10 => (b - t).abs(),
        // Exclusion
        11 => b + t - 2.0 * b * t,
        // Linear Dodge (Add)
        12 => (b + t).min(1.0),
        // Linear Burn
        13 => (b + t - 1.0).max(0.0),
        _ => t,
    }
}

/// Single-pixel blend: returns the composited color (top over base)
#[inline]
pub fn blend_pixel(b: [f32; 4], t: [f32; 4], mode: u32) -> [f32; 4] {
    let blended = match mode {
        1 => [b[0] * t[0], b[1] * t[1], b[2] * t[2]], // Multiply
        2 => [
            // Screen
            1.0 - (1.0 - b[0]) * (1.0 - t[0]),
            1.0 - (1.0 - b[1]) * (1.0 - t[1]),
            1.0 - (1.0 - b[2]) * (1.0 - t[2]),
        ],
        3 => [
            // Overlay
            if b[0] < 0.5 {
                2.0 * b[0] * t[0]
            } else {
                1.0 - 2.0 * (1.0 - b[0]) * (1.0 - t[0])
            },
            if b[1] < 0.5 {
                2.0 * b[1] * t[1]
            } else {
                1.0 - 2.0 * (1.0 - b[1]) * (1.0 - t[1])
            },
            if b[2] < 0.5 {
                2.0 * b[2] * t[2]
            } else {
                1.0 - 2.0 * (1.0 - b[2]) * (1.0 - t[2])
            },
        ],
        4 => [b[0].min(t[0]), b[1].min(t[1]), b[2].min(t[2])], // Darken
        5 => [b[0].max(t[0]), b[1].max(t[1]), b[2].max(t[2])], // Lighten
        6..=13 => std::array::from_fn(|c| blend_separable(b[c], t[c], mode)),
        _ => [t[0], t[1], t[2]], // Normal
    };
    // Alpha compositing: top over base, weighted by the top alpha
    let ta = t[3];
    let a = ta + b[3] * (1.0 - ta);
    let mut out = [b[0], b[1], b[2], a];
    if a > 0.0001 {
        for c in 0..3 {
            out[c] = blended[c] * ta + b[c] * b[3] * (1.0 - ta);
            out[c] /= a;
        }
    }
    out
}

/// Blends `top` INTO `base` in place — zero intermediate allocation.
/// Optional `mask` has the SAME dims as `top` (same transform); R channel
/// = 0..255 coverage, multiplied into the sampled alpha.
pub fn blend_into(
    base: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    top: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    mask: Option<&ImageBuffer<Rgba<u8>, Vec<u8>>>,
    opacity: f32,
    blend_mode: BlendMode,
    offset_x: f32,
    offset_y: f32,
) {
    let op = (opacity / 100.0).clamp(0.0, 1.0);
    if op <= 0.0 {
        return;
    }
    let mode = blend_mode.id();
    let w = base.width();
    let (tw, th) = (top.width() as i32, top.height() as i32);
    let ox = offset_x.round() as i32;
    let oy = offset_y.round() as i32;
    let t_raw = top.as_raw();
    let m_raw = mask.map(|m| m.as_raw());

    base.as_flat_samples_mut()
        .samples
        .par_chunks_exact_mut(4)
        .enumerate()
        .for_each(|(i, px)| {
            let x = (i as u32 % w) as i32;
            let y = (i as u32 / w) as i32;
            // Samples the top at (x - ox, y - oy) — transparent out of bounds
            let tx = x - ox;
            let ty = y - oy;
            let t: [f32; 4] = if tx >= 0 && tx < tw && ty >= 0 && ty < th {
                let o = (ty as u32 * tw as u32 + tx as u32) as usize * 4;
                let mut a = t_raw[o + 3] as f32 / 255.0 * op;
                if let Some(mr) = m_raw {
                    let cov = mr[o] as f32 / 255.0;
                    a *= cov;
                }
                [
                    t_raw[o] as f32 / 255.0,
                    t_raw[o + 1] as f32 / 255.0,
                    t_raw[o + 2] as f32 / 255.0,
                    a,
                ]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            // Top pixel absent/fully translucent: base unchanged (fast skip)
            if t[3] <= 0.001 {
                return;
            }
            let b: [f32; 4] = [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
                px[3] as f32 / 255.0,
            ];
            let o = blend_pixel(b, t, mode);
            px[0] = (o[0].clamp(0.0, 1.0) * 255.0) as u8;
            px[1] = (o[1].clamp(0.0, 1.0) * 255.0) as u8;
            px[2] = (o[2].clamp(0.0, 1.0) * 255.0) as u8;
            px[3] = (o[3].clamp(0.0, 1.0) * 255.0) as u8;
        });
}

/// Ready-to-draw item: appearance image + transform.
pub struct DrawItem<'a> {
    image: &'a DynamicImage,
    transform: Transform2D,
}

impl<'a> DrawItem<'a> {
    /// Builds a draw item (used by tests).
    #[cfg(test)]
    pub(crate) fn new(image: &'a DynamicImage, transform: Transform2D) -> Self {
        Self { image, transform }
    }
}

/// Applies scale + skew + rotation (around the center, like the canvas)
/// to the image. Returns (transformed buffer, adjusted offset_x, offset_y).
pub fn prepare_top(item: &DrawItem<'_>) -> (ImageBuffer<Rgba<u8>, Vec<u8>>, f32, f32) {
    let (w0, h0) = item.image.dimensions();
    let sx = item.transform.scale_x.clamp(0.05, 8.0);
    let sy = item.transform.scale_y.clamp(0.05, 8.0);
    if item.transform.has_skew() {
        // Skew: impossible in the fast path — single affine raytrace
        // (non-uniform scale + shear + rotation).
        return prepare_top_affine(item, sx, sy);
    }
    let mut buf: ImageBuffer<Rgba<u8>, Vec<u8>> = match item.image {
        DynamicImage::ImageRgba8(b) => {
            if (sx - 1.0).abs() > 0.001 || (sy - 1.0).abs() > 0.001 {
                let nw = ((w0 as f32 * sx).round() as u32).max(1);
                let nh = ((h0 as f32 * sy).round() as u32).max(1);
                image::imageops::resize(b, nw, nh, image::imageops::FilterType::Triangle)
            } else {
                b.clone()
            }
        }
        other => {
            let rgba = other.to_rgba8();
            if (sx - 1.0).abs() > 0.001 || (sy - 1.0).abs() > 0.001 {
                let nw = ((w0 as f32 * sx).round() as u32).max(1);
                let nh = ((h0 as f32 * sy).round() as u32).max(1);
                image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Triangle)
            } else {
                rgba
            }
        }
    };
    let (tw, th) = (buf.width() as f32, buf.height() as f32);
    let mut ox = item.transform.offset_x;
    let mut oy = item.transform.offset_y;
    let rot = item.transform.rotation_deg.rem_euclid(360.0); // ∈ [0, 360)
    // Multiples of 90° with epsilon (freely rounded rotations would be
    // a silent visual error)
    let is_0 = !(0.01..=359.99).contains(&rot);
    let is_90 = (rot - 90.0).abs() < 0.01;
    let is_180 = (rot - 180.0).abs() < 0.01;
    let is_270 = (rot - 270.0).abs() < 0.01;
    if is_90 || is_270 {
        // 90° / 270° — swap via imageops (fast, no interpolation)
        let rotated = if is_90 {
            image::imageops::rotate90(&buf)
        } else {
            image::imageops::rotate270(&buf)
        };
        let (nw, nh) = (rotated.width() as f32, rotated.height() as f32);
        ox += (tw - nw) / 2.0;
        oy += (th - nh) / 2.0;
        buf = rotated;
    } else if is_180 {
        buf = image::imageops::rotate180(&buf);
    } else if !is_0 {
        // Arbitrary rotation: bounding box + bilinear sampling
        let rad = rot.to_radians();
        let cos = rad.cos().abs();
        let sin = rad.sin().abs();
        let bbox_w = (tw * cos + th * sin).ceil().max(1.0) as u32;
        let bbox_h = (tw * sin + th * cos).ceil().max(1.0) as u32;
        let mut out = ImageBuffer::from_pixel(bbox_w, bbox_h, Rgba([0, 0, 0, 0]));
        let cx0 = tw / 2.0;
        let cy0 = th / 2.0;
        let cx1 = bbox_w as f32 / 2.0;
        let cy1 = bbox_h as f32 / 2.0;
        let cos_r = rad.cos();
        let sin_r = rad.sin();
        // Parallel fill (rayon)
        out.enumerate_pixels_mut()
            .par_bridge()
            .for_each(|(x, y, px)| {
                // Destination -> source (inverse rotation)
                let dx = x as f32 - cx1;
                let dy = y as f32 - cy1;
                let sx = dx * cos_r + dy * sin_r + cx0;
                let sy = -dx * sin_r + dy * cos_r + cy0;
                if sx >= 0.0 && sy >= 0.0 && sx < tw - 1.0 && sy < th - 1.0 {
                    // Bilinear
                    let x0 = sx.floor() as u32;
                    let y0 = sy.floor() as u32;
                    let fx = sx - x0 as f32;
                    let fy = sy - y0 as f32;
                    let p00 = buf.get_pixel(x0, y0);
                    let p10 = buf.get_pixel((x0 + 1).min(buf.width() - 1), y0);
                    let p01 = buf.get_pixel(x0, (y0 + 1).min(buf.height() - 1));
                    let p11 = buf.get_pixel(
                        (x0 + 1).min(buf.width() - 1),
                        (y0 + 1).min(buf.height() - 1),
                    );
                    for c in 0..4 {
                        let v = (p00[c] as f32 * (1.0 - fx) * (1.0 - fy)
                            + p10[c] as f32 * fx * (1.0 - fy)
                            + p01[c] as f32 * (1.0 - fx) * fy
                            + p11[c] as f32 * fx * fy)
                            .round() as u8;
                        px[c] = v;
                    }
                }
            });
        ox += (tw - bbox_w as f32) / 2.0;
        oy += (th - bbox_h as f32) / 2.0;
        buf = out;
    }
    (buf, ox, oy)
}

/// Affine rasterization (non-uniform scale + skew + rotation) in a
/// single bilinear sampling via inverse mapping. Same convention as
/// [`Transform2D::local_to_doc`]: scale → skew → rotation around the
/// center, then offset. The returned offset is the enclosing bbox's
/// min corner (fractional, rounded later by `blend_into`).
fn prepare_top_affine(
    item: &DrawItem<'_>,
    sx: f32,
    sy: f32,
) -> (ImageBuffer<Rgba<u8>, Vec<u8>>, f32, f32) {
    let (w0, h0) = item.image.dimensions();
    let t = item.transform;
    let ox = t.offset_x;
    let oy = t.offset_y;
    // Scales clamped like `prepare_top`: the shared matrix is computed
    // on the clamped transform to stay identical.
    let tc = Transform2D {
        scale_x: sx,
        scale_y: sy,
        ..t
    };
    let rad = t.rotation_deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let cx = w0 as f32 / 2.0;
    let cy = h0 as f32 / 2.0;

    // A = R * K * S (scale, then shear, then rotation):
    // m00 m01 / m10 m11 = K*S = [[sx, kx*sy],[ky*sx, sy]]
    let (m00, m01, m10, m11) = tc.shear_scale_matrix();
    let det = m00 * m11 - m01 * m10;
    if det.abs() < 1e-4 {
        // Degenerate shear (tan → ∞): fall back to scale alone.
        let img = item.image.to_rgba8();
        let nw = ((w0 as f32 * sx).round() as u32).max(1);
        let nh = ((h0 as f32 * sy).round() as u32).max(1);
        let buf = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
        return (buf, ox, oy);
    }

    let fwd = |x: f32, y: f32| -> (f32, f32) { tc.local_to_doc(w0 as f32, h0 as f32, x, y) };
    let corners = [
        fwd(0.0, 0.0),
        fwd(w0 as f32, 0.0),
        fwd(w0 as f32, h0 as f32),
        fwd(0.0, h0 as f32),
    ];
    let min_x = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
    let min_y = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min);
    let max_x = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
    let max_y = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max);
    let bbox_w = (max_x - min_x).ceil().max(1.0) as u32;
    let bbox_h = (max_y - min_y).ceil().max(1.0) as u32;

    let mut out = ImageBuffer::from_pixel(bbox_w, bbox_h, Rgba([0, 0, 0, 0]));
    out.enumerate_pixels_mut()
        .par_bridge()
        .for_each(|(x, y, px)| {
            // Destination -> source (full inverse affine), bilinear.
            // The buffer covers [min_x, min_x+bbox_w) × [min_y, …].
            let dx = x as f32 + min_x - cx * sx - ox;
            let dy = y as f32 + min_y - cy * sy - oy;
            let rx = dx * cos + dy * sin;
            let ry = -dx * sin + dy * cos;
            let vx = (m11 * rx - m01 * ry) / det;
            let vy = (-m10 * rx + m00 * ry) / det;
            let sx_src = cx + vx;
            let sy_src = cy + vy;
            if sx_src >= 0.0 && sy_src >= 0.0 && sx_src < w0 as f32 && sy_src < h0 as f32 {
                let x0 = sx_src.floor() as u32;
                let y0 = sy_src.floor() as u32;
                let fx = sx_src - x0 as f32;
                let fy = sy_src - y0 as f32;
                let p00 = item.image.get_pixel(x0, y0);
                let p10 = item.image.get_pixel((x0 + 1).min(w0 - 1), y0);
                let p01 = item.image.get_pixel(x0, (y0 + 1).min(h0 - 1));
                let p11 = item
                    .image
                    .get_pixel((x0 + 1).min(w0 - 1), (y0 + 1).min(h0 - 1));
                for c in 0..4 {
                    let v = (p00[c] as f32 * (1.0 - fx) * (1.0 - fy)
                        + p10[c] as f32 * fx * (1.0 - fy)
                        + p01[c] as f32 * (1.0 - fx) * fy
                        + p11[c] as f32 * fx * fy)
                        .round() as u8;
                    px[c] = v;
                }
            }
        });
    (out, min_x, min_y)
}

/// Multiplicatively merges group masks (canvas space, `sub` dims,
/// inversion included) — same role as `combine_masks` for a group.
fn combine_group_masks(
    masks: &[LayerMask],
    sub: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let mut it = masks
        .iter()
        .filter(|m| m.enabled)
        .map(|m| prepare_group_mask(m, sub));
    let first = it.next()?;
    Some(it.fold(first, |acc, m| multiply_coverage(&acc, &m)))
}

/// A group mask's coverage: sample of the image centered on `sub`, no
/// transform (identity), honoring `inverted` — TODO real transform.
fn prepare_group_mask(
    m: &LayerMask,
    sub: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut buf = ImageBuffer::from_pixel(sub.width(), sub.height(), Rgba([255, 255, 255, 255]));
    let img = m.effective();
    let (mw, mh) = (img.width(), img.height());
    let copy_w = mw.min(sub.width());
    let copy_h = mh.min(sub.height());
    for y in 0..copy_h {
        for x in 0..copy_w {
            let p = img.get_pixel(x, y);
            let v = if m.inverted { 255 - p[0] } else { p[0] };
            buf.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    buf
}

/// Multiplies two coverage buffers (R channel) element-wise.
fn multiply_coverage(
    a: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    b: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let w = a.width().max(b.width());
    let h = a.height().max(b.height());
    let mut out = ImageBuffer::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    for y in 0..h {
        for x in 0..w {
            let c = |img: &ImageBuffer<Rgba<u8>, Vec<u8>>| -> u8 {
                if x < img.width() && y < img.height() {
                    img.get_pixel(x, y)[0]
                } else {
                    0
                }
            };
            let av = c(a) as u32;
            let bv = c(b) as u32;
            out.put_pixel(x, y, Rgba([(av * bv / 255) as u8, 0, 0, 255]));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Recursive compositing (groups + adjustments)
// ---------------------------------------------------------------------------

type Resolver<'a> = &'a dyn Fn(Uuid) -> Option<Arc<DynamicImage>>;

/// Half-extents needed to cover all visible items of a scope, around
/// the document center (infinite plane). Recursive: a group contributes
/// via its children (same canvas coordinates).
pub fn scope_half_extents(
    nodes: &[LayerNode],
    doc_w: u32,
    doc_h: u32,
    resolve: Resolver<'_>,
) -> (f32, f32) {
    let doc_cx = doc_w as f32 / 2.0;
    let doc_cy = doc_h as f32 / 2.0;
    let mut half = (doc_w as f32 / 2.0, doc_h as f32 / 2.0);
    extents_visit(nodes, doc_cx, doc_cy, resolve, &mut half);
    half
}

pub fn extents_visit(
    nodes: &[LayerNode],
    doc_cx: f32,
    doc_cy: f32,
    resolve: Resolver<'_>,
    half: &mut (f32, f32),
) {
    for node in nodes {
        match node {
            LayerNode::Group(g) => {
                if g.visible && g.opacity > 0.01 {
                    extents_visit(&g.children, doc_cx, doc_cy, resolve, half);
                }
            }
            LayerNode::Adjustment(_) => {}
            LayerNode::Vector(v) => {
                if !v.visible || v.opacity <= 0.01 {
                    continue;
                }
                let Some(img) = resolve(v.id) else {
                    continue;
                };
                let Some(placed) = v.placement() else {
                    continue;
                };
                let (w0, h0) = (img.width() as f32, img.height() as f32);
                let clamped = Transform2D {
                    scale_x: placed.scale_x.clamp(0.05, 8.0),
                    scale_y: placed.scale_y.clamp(0.05, 8.0),
                    ..placed
                };
                for (qx, qy) in clamped.doc_corners(w0, h0) {
                    half.0 = half.0.max((qx - doc_cx).abs());
                    half.1 = half.1.max((qy - doc_cy).abs());
                }
            }
            LayerNode::Pixel(l) => {
                if !l.visible || l.opacity <= 0.01 {
                    continue;
                }
                let Some(img) = resolve(l.id) else {
                    continue;
                };
                let (w0, h0) = (img.width() as f32, img.height() as f32);
                // Transformed layer corners (scale clamped like prepare_top)
                let clamped = Transform2D {
                    scale_x: l.transform.scale_x.clamp(0.05, 8.0),
                    scale_y: l.transform.scale_y.clamp(0.05, 8.0),
                    ..l.transform
                };
                for (qx, qy) in clamped.doc_corners(w0, h0) {
                    half.0 = half.0.max((qx - doc_cx).abs());
                    half.1 = half.1.max((qy - doc_cy).abs());
                }
                // Clipped children: same space, they extend the scope.
                extents_visit(&l.children, doc_cx, doc_cy, resolve, half);
            }
        }
    }
}

/// Clipping coverage of a children sub-render by the placed parent's
/// alpha (`top` at offset `off_x`/`off_y` in `sub` space): `Child_alpha`
/// × Parent_alpha`, R channel (like group masks), A = 255. Outside
/// the parent bounds → 0 (invisible). Parallel sweep like `blend_into`,
/// same integer offset rounding.
fn clip_cover_by_parent_alpha(
    top: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    w: u32,
    h: u32,
    off_x: f32,
    off_y: f32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (tw, th) = (top.width() as i32, top.height() as i32);
    let ox = off_x.round() as i32;
    let oy = off_y.round() as i32;
    let t_raw = top.as_raw();
    let mut out = ImageBuffer::from_pixel(w.max(1), h.max(1), Rgba([0, 0, 0, 255]));
    if w == 0 || h == 0 {
        return out;
    }
    out.as_flat_samples_mut()
        .samples
        .par_chunks_exact_mut(4)
        .enumerate()
        .for_each(|(i, px)| {
            let x = (i as u32 % w) as i32;
            let y = (i as u32 / w) as i32;
            let tx = x - ox;
            let ty = y - oy;
            px[0] = if tx >= 0 && tx < tw && ty >= 0 && ty < th {
                t_raw[(ty as u32 * tw as u32 + tx as u32) as usize * 4 + 3]
            } else {
                0
            };
        });
    out
}

/// Recursively blends a scope into the (already sized) accumulator.
/// Returns true if at least one element contributed.
///
/// Pixel: transform + direct blend (+ children clipped by its alpha).
/// Group: children composite first into a transparent buffer, nested
/// filters apply, then the result is blended with the group's opacity
/// and mode. Adjustment: the chain applies to the accumulator,
/// weighted by opacity (and restricted by its masks).
pub fn fold_scope(
    nodes: &[LayerNode],
    acc: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    origin_x: f32,
    origin_y: f32,
    resolve: Resolver<'_>,
) -> bool {
    let mut stats = CompositeStats::default();
    fold_scope_stats(nodes, acc, origin_x, origin_y, resolve, &mut stats)
}

/// Composite fallback counters (light, temporary instrumentation).
///
/// `pixels_processed` counts actually traversed pixels: each `blend_into`
/// / adjustment mix sweeps the WHOLE accumulator, so `pixels_processed ≈`
/// layers_blended × scope_px` (plus mixes). `blend_us` covers prepare_top
/// and blend per layer (allocs included), excluding extents and
/// accumulator allocation.
#[derive(Debug, Default, Clone, Copy)]
pub struct CompositeStats {
    /// Accumulator (scope) pixels.
    pub scope_px: u64,
    /// Accumulator allocation microseconds (zeroing).
    pub acc_alloc_us: u128,
    /// Prepare + blend + mix pass microseconds.
    pub blend_us: u128,
    /// Pixel layers and groups actually blended.
    pub layers_blended: u64,
    /// Traversed pixels (sum of accumulator sweeps).
    pub pixels_processed: u64,
}

/// Instrumented [`fold_scope`] variant: identical body, counting
/// blended layers, traversed pixels and prepare + blend time.
/// Unchanged render behavior.
pub fn fold_scope_stats(
    nodes: &[LayerNode],
    acc: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    origin_x: f32,
    origin_y: f32,
    resolve: Resolver<'_>,
    stats: &mut CompositeStats,
) -> bool {
    let mut contributed = false;
    let acc_px = u64::from(acc.width()) * u64::from(acc.height());
    for node in nodes {
        match node {
            LayerNode::Pixel(l) => {
                if !l.visible || l.opacity <= 0.01 {
                    continue;
                }
                let Some(img) = resolve(l.id) else {
                    continue;
                };
                let item = DrawItem {
                    image: &img,
                    transform: l.transform,
                };
                let t_blend = std::time::Instant::now();
                let (top, ox, oy) = prepare_top(&item);
                // Layer masks baked into appearance (source × filters × masks):
                // nothing left to attenuate here — direct blend.
                blend_into(
                    acc,
                    &top,
                    None,
                    l.opacity,
                    l.blend_mode,
                    ox + origin_x,
                    oy + origin_y,
                );
                stats.blend_us += t_blend.elapsed().as_micros();
                stats.layers_blended += 1;
                stats.pixels_processed += acc_px;
                contributed = true;
                // Affinity-style nested children: composited ABOVE the parent
                // then restricted to its alpha (clipping mask).
                if !l.children.is_empty() {
                    let mut sub = ImageBuffer::from_pixel(
                        acc.width().max(1),
                        acc.height().max(1),
                        Rgba([0, 0, 0, 0]),
                    );
                    if fold_scope_stats(&l.children, &mut sub, origin_x, origin_y, resolve, stats) {
                        let cover = clip_cover_by_parent_alpha(
                            &top,
                            sub.width(),
                            sub.height(),
                            ox + origin_x,
                            oy + origin_y,
                        );
                        let t_clip = std::time::Instant::now();
                        blend_into(acc, &sub, Some(&cover), 100.0, BlendMode::Normal, 0.0, 0.0);
                        stats.blend_us += t_clip.elapsed().as_micros();
                        stats.layers_blended += 1;
                        stats.pixels_processed += acc_px;
                    }
                }
            }
            LayerNode::Vector(v) => {
                if !v.visible || v.opacity <= 0.01 {
                    continue;
                }
                let Some(img) = resolve(v.id) else {
                    continue;
                };
                // Bounds origin folded into the placement (appearance
                // is bounds-shifted); user transform applies on top.
                let Some(placed) = v.placement() else {
                    continue;
                };
                let item = DrawItem {
                    image: &img,
                    transform: placed,
                };
                let t_blend = std::time::Instant::now();
                let (top, ox, oy) = prepare_top(&item);
                // Canvas-space masks (like groups: the raster has no
                // stable source space across appearances).
                let mask_buf = combine_group_masks(&v.masks, &top);
                blend_into(
                    acc,
                    &top,
                    mask_buf.as_ref(),
                    v.opacity,
                    v.blend_mode,
                    ox + origin_x,
                    oy + origin_y,
                );
                stats.blend_us += t_blend.elapsed().as_micros();
                stats.layers_blended += 1;
                stats.pixels_processed += acc_px;
                contributed = true;
            }
            LayerNode::Group(g) => {
                if !g.visible || g.opacity <= 0.01 {
                    continue;
                }
                let mut sub = ImageBuffer::from_pixel(
                    acc.width().max(1),
                    acc.height().max(1),
                    Rgba([0, 0, 0, 0]),
                );
                if fold_scope_stats(&g.children, &mut sub, origin_x, origin_y, resolve, stats) {
                    // Group nested filters: applied to the children composite
                    // AFTER composition, BEFORE the masks (same order as a
                    // pixel layer's sub-layers).
                    if g.filter_layers.iter().any(|f| f.enabled) {
                        let src = Arc::new(DynamicImage::ImageRgba8(sub));
                        let filtered = crate::filters::render_chain(&src, &g.filter_layers);
                        sub = filtered.to_rgba8();
                    }
                    let mask_buf = combine_group_masks(&g.masks, &sub);
                    let t_blend = std::time::Instant::now();
                    blend_into(
                        acc,
                        &sub,
                        mask_buf.as_ref(),
                        g.opacity,
                        g.blend_mode,
                        0.0,
                        0.0,
                    );
                    stats.blend_us += t_blend.elapsed().as_micros();
                    stats.layers_blended += 1;
                    stats.pixels_processed += acc_px;
                    contributed = true;
                }
            }
            LayerNode::Adjustment(a) => {
                if !a.visible || a.opacity <= 0.01 {
                    continue;
                }
                let t_blend = std::time::Instant::now();
                let applied =
                    apply_adjustment(acc, &a.filters, a.opacity, &a.masks, origin_x, origin_y);
                stats.blend_us += t_blend.elapsed().as_micros();
                if applied {
                    // Full-surface mix (original + adjusted + blend).
                    stats.pixels_processed += acc_px;
                    contributed = true;
                }
            }
        }
    }
    contributed
}

/// Composites a filter sub-layer's filtered result OVER the accumulated
/// image (Affinity-style: the filter applies to the content below, then
/// its result is blended with its own opacity/blend/transform/masks).
///
/// Spaces: `base` and `filtered` live in the PARENT space (same
/// dimensions, effect output). Masks are applied FIRST in that space
/// (same space as paint, which uses the carrier transform), THEN the
/// attenuated result is transformed — never the reverse. `filtered` is
/// taken by value: the neutral case copies nothing (move).
pub fn composite_filter_layer(
    base: &DynamicImage,
    filtered: DynamicImage,
    layer: &FilterLayer,
) -> DynamicImage {
    if layer.is_passthrough() {
        return filtered;
    }
    let attenuated = attenuate_by_masks(filtered, &layer.masks);
    let item = DrawItem {
        image: &attenuated,
        transform: layer.transform,
    };
    let (top, ox, oy) = prepare_top(&item);
    let mut acc = base.to_rgba8();
    blend_into(
        &mut acc,
        &top,
        None,
        layer.opacity,
        layer.blend_mode,
        ox,
        oy,
    );
    DynamicImage::ImageRgba8(acc)
}

/// Attenuates an image by the combined coverage of ACTIVE masks
/// (alpha channel × coverage — same semantics as `blend_into`). Masks
/// live in the carrier source space: identical dimensions required,
/// else guard rail (no attenuation rather than a wrong black). With
/// no active mask: returns the image as is.
fn attenuate_by_masks(img: DynamicImage, masks: &[LayerMask]) -> DynamicImage {
    let Some(cover) = combined_mask_coverage(masks) else {
        return img;
    };
    if cover.dimensions() != img.dimensions() {
        return img;
    }
    attenuate_by_coverage(img, &cover)
}

/// Combines ACTIVE masks into one coverage (coverage multiplication,
/// `inverted` applied). `None` = no active mask. Coverage is pure
///
/// data, cacheable independently of the image: it is what a future
/// shader path will sample at draw time.
pub fn combined_mask_coverage(masks: &[LayerMask]) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let mut it = masks.iter().filter(|m| m.enabled);
    let first = it.next()?;
    let mut cover = mask_coverage(first);
    for m in it {
        cover = multiply_coverage(&cover, &mask_coverage(m));
    }
    Some(cover)
}

/// Multiplies an image's alpha channel by a coverage (R channel).
/// Dimensions must match (caller-side guard rail).
pub fn attenuate_by_coverage(
    img: DynamicImage,
    cover: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> DynamicImage {
    let mut buf = img.to_rgba8();
    let raw = cover.as_raw();
    buf.as_flat_samples_mut()
        .samples
        .par_chunks_exact_mut(4)
        .enumerate()
        .for_each(|(i, px)| {
            let cov = raw[i * 4] as f32 / 255.0;
            px[3] = (px[3] as f32 * cov).round() as u8;
        });
    DynamicImage::ImageRgba8(buf)
}

/// A mask's coverage in its own buffer (`inverted` applied — same rule
/// as `prepare_mask`).
fn mask_coverage(mask: &LayerMask) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let effective = mask.effective();
    if !mask.inverted {
        return (*effective).clone();
    }
    let mut buf = (*effective).clone();
    for px in buf.pixels_mut() {
        let v = 255 - px[0];
        *px = Rgba([v, v, v, 255]);
    }
    buf
}

/// Bakes a pixel layer's appearance: attenuates `image` by the combined
/// coverage of its ACTIVE masks, in SOURCE SPACE (identity — the canvas
/// applies the layer transform underneath). No active mask → `image`
/// returned as is (same Arc, zero copy). Makes layer masks
///
/// homogeneous with filter sub-layer masks: they are part of the
/// cacheable appearance, no longer of the CPU fallback.
pub fn apply_layer_masks(image: Arc<DynamicImage>, masks: &[LayerMask]) -> Arc<DynamicImage> {
    let Some(cover) = combined_mask_coverage(masks) else {
        return image;
    };
    apply_coverage(image, &cover)
}

/// Applies an ALREADY-combined coverage to an image. Same guard
///
/// rail as [`apply_layer_masks`]: mismatched-dimensions coverage is
/// resampled to the image dimensions rather than wrongly attenuated.
/// Exposed so the cache can reuse a coverage without recombining it
/// (and, eventually, sample it at draw time in a shader instead of
/// baking it here).
pub fn apply_coverage(
    image: Arc<DynamicImage>,
    cover: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> Arc<DynamicImage> {
    let (iw, ih) = image.dimensions();
    let cover = if cover.dimensions() != (iw, ih) {
        image::imageops::resize(
            cover,
            iw.max(1),
            ih.max(1),
            ::image::imageops::FilterType::Triangle,
        )
    } else {
        cover.clone()
    };
    Arc::new(attenuate_by_coverage(image.as_ref().clone(), &cover))
}

/// Spatial dependency of an effect for windowed compositing (Phase 6C).
///
/// - [`SpatialScope::Local`]: a pixel's output only depends on the same
///   pixel (brightness/contrast, saturation…) — evaluable directly on
///   the requested window.
/// - [`SpatialScope::Neighborhood`]: the output depends on a
///   neighborhood (blur…) — the window must be grown by `halo_px`
///   before evaluation, then cropped.
/// - [`SpatialScope::Global`]: potentially total dependency (unknown op
///   from a future version, arbitrary offset…) — full-frame evaluation
///   then crop. Correctness beats performance.
///
/// Never claim a local adjustment just because it is fed a small
/// image: classification follows each effect's REAL definition
/// below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialScope {
    Local,
    Neighborhood { halo_px: u32 },
    Global,
}

/// Exact support (half-kernel, in px) of `blur(radius)`: `radius <=
/// 0.1` ⇒ passthrough (`nodes::blur::apply_effect`) ⇒ 0; else the
/// CPU gaussian kernel half-size (`image 0.25`, `kernel_size_from_sigma`),
/// which also covers the GPU box-blur (support = clamped `1..=50`
/// radius, always smaller or equal).
///
/// Distinct from `tiles::blur_halo_for_filters` (tile invalidation halo,
/// Phase 6B `ceil(r)` convention): here the render path requires the
/// kernel's exact support, else `regional != crop(full)` at window edges.
#[must_use]
pub fn blur_support_px(radius: f32) -> u32 {
    if !radius.is_finite() || radius <= 0.1 {
        return 0;
    }
    // Mirror of `GaussianBlurParameters::kernel_size_from_sigma`: size =
    // odd ≥ ((((σ − 0.8) / 0.3) + 1) × 2) + 1, support = size / 2.
    let possible = ((((radius - 0.8) / 0.3) + 1.0) * 2.0 + 1.0).max(3.0) as u32;
    let size = if possible.is_multiple_of(2) {
        possible + 1
    } else {
        possible
    };
    size / 2
}

/// Scope of an ACTIVE adjustment filter node (`FilterNode`) — the caller
/// skips disabled ones exactly like `render_nodes` (zero cost).
/// `brightness_contrast` and `color_correct` are point-wise (`par_chunks_mut(4)`
/// traversal with no neighborhood); `mix`/`input`/`output` forward as is
/// in a linear chain (point-wise); `layer` may shift arbitrarily
/// (`offset_x/y`) and any unknown `type_id` may do anything: conservative
/// GLOBAL (full-frame then crop — never wrong pixels).
#[must_use]
pub fn filter_spatial_scope(filter: &FilterNode) -> SpatialScope {
    spatial_scope_of(&filter.type_id, &filter.params)
}

/// Scope of an ACTIVE filter sub-layer (`FilterLayer`) — same
/// classification as [`filter_spatial_scope`] (same `type_id`/`params`).
/// Used for group nested filters (regional compositing dependency
/// window).
#[must_use]
pub fn filter_layer_spatial_scope(filter: &FilterLayer) -> SpatialScope {
    spatial_scope_of(&filter.type_id, &filter.params)
}

fn spatial_scope_of(
    type_id: &str,
    params: &std::collections::HashMap<String, datatypes::ParamValue>,
) -> SpatialScope {
    match type_id {
        "brightness_contrast" | "color_correct" | "mix" | "input_image" | "output" => {
            SpatialScope::Local
        }
        "blur" => {
            let r = params
                .get("radius")
                .and_then(datatypes::ParamValue::as_float)
                .unwrap_or(5.0);
            let halo = blur_support_px(r);
            if halo == 0 {
                SpatialScope::Local
            } else {
                SpatialScope::Neighborhood { halo_px: halo }
            }
        }
        _ => SpatialScope::Global,
    }
}

/// Dependency window of a scope (recursive, groups included):
/// `halo_px` = summed sequential halos (`r1 → r2` chain ⇒ `h1 + h2`);
/// `global` = required full-frame fallback (global op OR active group
/// mask — group masks are aligned to the accumulator origin, hence
/// not window-shift invariant, unlike layer masks baked into
/// full-size appearances).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScopeWindow {
    /// Summed neighborhood supports (0 = purely local).
    pub halo_px: u32,
    /// True ⇒ evaluate full-frame then crop (correctness first).
    pub global: bool,
}

impl ScopeWindow {
    /// Adds a scope (halo sums, OR of fallbacks).
    pub fn add(&mut self, scope: SpatialScope) {
        match scope {
            SpatialScope::Local => {}
            SpatialScope::Neighborhood { halo_px } => {
                self.halo_px = self.halo_px.saturating_add(halo_px);
            }
            SpatialScope::Global => {
                self.global = true;
            }
        }
    }
}

/// Dependency window of `nodes` adjustments (and their nested
/// groups). Disabled filters and invisible/zero-opacity layers are
/// skipped as in `fold_scope_stats` (no contribution).
#[must_use]
pub fn scope_adjustment_window(nodes: &[LayerNode]) -> ScopeWindow {
    let mut window = ScopeWindow::default();
    scope_adjustment_window_into(nodes, &mut window);
    window
}

fn scope_adjustment_window_into(nodes: &[LayerNode], window: &mut ScopeWindow) {
    for node in nodes {
        match node {
            LayerNode::Pixel(l) => {
                // Clipped children are only pixels (no global effect of their
                // own): recursion suffices.
                scope_adjustment_window_into(&l.children, window);
            }
            // Vectors carry a live-filter chain (spatial scope like
            // groups) but no children.
            LayerNode::Vector(v) => {
                for f in &v.filter_layers {
                    if !f.enabled {
                        continue;
                    }
                    window.add(filter_layer_spatial_scope(f));
                }
            }
            LayerNode::Group(g) => {
                if !g.visible || g.opacity <= 0.01 {
                    continue;
                }
                if g.masks.iter().any(|m| m.enabled) {
                    window.global = true;
                }
                // Group nested filters: same window as adjustments (a blur
                // widens the halo dependency).
                for f in &g.filter_layers {
                    if !f.enabled {
                        continue;
                    }
                    window.add(filter_layer_spatial_scope(f));
                }
                scope_adjustment_window_into(&g.children, window);
            }
            LayerNode::Adjustment(a) => {
                if !a.visible || a.opacity <= 0.01 {
                    continue;
                }
                // Adjustment masks: aligned to the document origin (like
                // group masks), hence not window-shift invariant → global
                // fallback.
                if a.masks.iter().any(|m| m.enabled) {
                    window.global = true;
                }
                for f in &a.filters {
                    if !f.enabled {
                        continue;
                    }
                    window.add(filter_spatial_scope(f));
                }
            }
        }
    }
}

/// Applies an adjustment chain to the accumulator, weighted by opacity
/// (linear original ↔ adjusted mix) and restricted by the combined
/// mask coverage (`masks`, document space): `dst = src·(1−w·c) + adj·(w·c)`;
/// with no active mask, `c = 1` everywhere (same ops as the legacy
/// mix, bit-identical).
/// Returns true if applied.
///
/// The accumulator may be a WINDOW (regional compositing): each effect
/// evaluates on the provided pixels — the caller (`Document::composite_region_with`)
/// guarantees the dependency window (`requested + halo`, or full-frame
/// on global fallback), so no useless full-frame processing happens
/// here for local/neighborhood ops. The final mix is point-wise
/// (window-safe).
pub fn apply_adjustment(
    acc: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    filters: &[FilterNode],
    opacity: f32,
    masks: &[LayerMask],
    origin_x: f32,
    origin_y: f32,
) -> bool {
    let weight = (opacity / 100.0).clamp(0.0, 1.0);
    if weight <= 0.0 || !filters.iter().any(|f| f.enabled) || acc.width() == 0 {
        return false;
    }
    // Combined coverage in accumulator space (`None` = no active mask:
    // 1.0 everywhere, zero allocation). Buffer → document mapping: `doc =`
    // buffer − origin` (same convention as `blend_into`, rounded offsets,
    // transparent outside the document).
    let cover = combined_adjustment_coverage(
        masks,
        acc.width(),
        acc.height(),
        origin_x.round() as i32,
        origin_y.round() as i32,
    );
    let original = acc.clone();
    let adjusted = crate::filters::render_nodes(
        &Arc::new(DynamicImage::ImageRgba8(original.clone())),
        filters,
    );
    let DynamicImage::ImageRgba8(adjusted_buf) = adjusted.as_ref() else {
        return false;
    };
    if adjusted_buf.dimensions() != acc.dimensions() {
        return false; // guard: exotic effect resized output — ignored
    }
    let raw_acc = acc.as_flat_samples_mut().samples;
    let raw_orig = original.as_raw();
    let raw_adj = adjusted_buf.as_raw();
    match cover {
        None => raw_acc
            .par_chunks_exact_mut(4)
            .zip(raw_orig.par_chunks_exact(4))
            .zip(raw_adj.par_chunks_exact(4))
            .for_each(|((dst, src), adj)| {
                for c in 0..4 {
                    dst[c] =
                        (src[c] as f32 * (1.0 - weight) + adj[c] as f32 * weight).round() as u8;
                }
            }),
        Some(cov) => raw_acc
            .par_chunks_exact_mut(4)
            .zip(raw_orig.par_chunks_exact(4))
            .zip(raw_adj.par_chunks_exact(4))
            .zip(cov.par_chunks_exact(1))
            .for_each(|(((dst, src), adj), cov)| {
                let c = cov[0] as f32 / 255.0;
                let w = weight * c;
                for ch in 0..4 {
                    dst[ch] = (src[ch] as f32 * (1.0 - w) + adj[ch] as f32 * w).round() as u8;
                }
            }),
    }
    true
}

/// Combined coverage of an adjustment's masks, in accumulator space
/// (`None` = no active mask). Multiplicative, `inverted` honored, 0
/// outside the document (the mask only covers the document, as its
/// `LayerMask::full` creation shows).
fn combined_adjustment_coverage(
    masks: &[LayerMask],
    w: u32,
    h: u32,
    origin_x: i32,
    origin_y: i32,
) -> Option<Vec<u8>> {
    let mut active = masks.iter().filter(|m| m.enabled);
    let first = active.next()?;
    // Zeros: `sample_mask_into` writes every pixel (no read before).
    let mut cov = vec![0u8; w as usize * h as usize];
    sample_mask_into(&mut cov, first, w, h, origin_x, origin_y);
    for m in active {
        sample_mask_multiply(&mut cov, m, w, h, origin_x, origin_y);
    }
    Some(cov)
}

/// Samples a mask (document space) into `cov` (accumulator space):
/// `cov = v` (`inverted` → `255 − v`), 0 outside the document. Parallel
/// sweep (one pixel = one entry, like `blend_into`).
fn sample_mask_into(cov: &mut [u8], m: &LayerMask, w: u32, h: u32, ox: i32, oy: i32) {
    debug_assert_eq!(cov.len(), w as usize * h as usize);
    let effective = m.effective();
    let (mw, mh) = (effective.width() as i32, effective.height() as i32);
    let inverted = m.inverted;
    let img = &effective;
    cov.par_chunks_exact_mut(1).enumerate().for_each(|(i, px)| {
        let x = (i as u32 % w) as i32;
        let y = (i as u32 / w) as i32;
        let dx = x - ox;
        let dy = y - oy;
        px[0] = if dx >= 0 && dx < mw && dy >= 0 && dy < mh {
            let p = img.get_pixel(dx as u32, dy as u32)[0];
            if inverted { 255 - p } else { p }
        } else {
            0
        };
    });
}

/// Multiplies `cov` by the mask sample (same mapping).
fn sample_mask_multiply(cov: &mut [u8], m: &LayerMask, w: u32, h: u32, ox: i32, oy: i32) {
    debug_assert_eq!(cov.len(), w as usize * h as usize);
    let effective = m.effective();
    let (mw, mh) = (effective.width() as i32, effective.height() as i32);
    let inverted = m.inverted;
    let img = &effective;
    cov.par_chunks_exact_mut(1).enumerate().for_each(|(i, px)| {
        let x = (i as u32 % w) as i32;
        let y = (i as u32 / w) as i32;
        let dx = x - ox;
        let dy = y - oy;
        let v = if dx >= 0 && dx < mw && dy >= 0 && dy < mh {
            let p = img.get_pixel(dx as u32, dy as u32)[0];
            if inverted { 255 - p } else { p }
        } else {
            0
        };
        px[0] = ((px[0] as u32 * v as u32) / 255) as u8;
    });
}

#[cfg(test)]
mod downsample_tests {
    use super::*;

    /// Deterministic LCG (no random dependency): varied content, extreme
    /// alphas included.
    fn pseudo_random_rgba(w: u32, h: u32, seed: u64) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        let mut state = seed.max(1);
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u8
        };
        ImageBuffer::from_fn(w.max(1), h.max(1), |_, _| {
            Rgba([next(), next(), next(), next()])
        })
    }

    fn assert_thumbnail_identical(w: u32, h: u32, nw: u32, nh: u32, seed: u64) {
        let src = pseudo_random_rgba(w, h, seed);
        let expected = image::imageops::thumbnail(&src, nw, nh);
        let got = fast_thumbnail_rgba8(&src, nw, nh);
        assert_eq!(
            got.dimensions(),
            expected.dimensions(),
            "dims {w}x{h} → {nw}x{nh}"
        );
        assert_eq!(
            got.as_raw(),
            expected.as_raw(),
            "octets {w}x{h} → {nw}x{nh} (seed {seed})"
        );
    }

    #[test]
    fn fast_thumbnail_egale_reference_petites_tailles() {
        // Exhaustive sweep 1..=32 (downscale, upscale, squares, rows).
        for h in 1..=32u32 {
            for w in 1..=32u32 {
                for (nw, nh) in [(1, 1), (2, 2), (w / 2 + 1, h / 2 + 1), (w * 2, h * 2)] {
                    assert_thumbnail_identical(
                        w,
                        h,
                        nw.max(1),
                        nh.max(1),
                        u64::from(w) * 1000 + u64::from(h),
                    );
                }
            }
        }
    }

    #[test]
    fn fast_thumbnail_egale_reference_tailles_reelles() {
        // Production sizes (preview 2048→1600) + odd ratios + primes.
        for (w, h, nw, nh) in [
            (2048, 2048, 1600, 1600),
            (512, 512, 200, 200),
            (1000, 777, 500, 389),
            (127, 129, 64, 65),
            (255, 256, 100, 100),
            (511, 511, 256, 256),
            (3840, 2160, 1600, 900),
            (7, 5, 3, 2),
            (5, 7, 2, 3),
        ] {
            assert_thumbnail_identical(w, h, nw, nh, 0xC10C);
        }
    }

    #[test]
    fn capped_dims_egales_thumbnail() {
        // `capped_thumbnail_dims` ≡ `thumbnail` output dimensions (or `None`
        // ⟺ unchanged), on real sizes and edge cases.
        for (w, h, max) in [
            (2048, 2048, 1600),
            (2048, 1024, 1600),
            (1024, 2048, 1600),
            (3000, 100, 1600),
            (100, 3000, 1600),
            (1600, 1600, 1600),
            (1599, 800, 1600),
            (800, 1599, 1600),
            (4096, 4096, 1600),
            (512, 512, 1600),
            (3, 2000, 100),
            (2000, 3, 100),
        ] {
            let src = pseudo_random_rgba(w, h, 7);
            let dynimg = DynamicImage::ImageRgba8(src);
            let shrunk = dynimg.thumbnail(max, max);
            match capped_thumbnail_dims(w, h, max) {
                None => assert!(
                    w.max(h) <= max,
                    "pas de shrink ⇒ déjà borné ({w}x{h}, max {max})"
                ),
                Some((nw, nh)) => {
                    assert!(w.max(h) > max);
                    assert_eq!((shrunk.width(), shrunk.height()), (nw, nh));
                    // And the pixels follow (full capped path).
                    let fast = fast_thumbnail_rgba8(dynimg.as_rgba8().expect("rgba8"), nw, nh);
                    assert_eq!(fast.as_raw(), shrunk.as_rgba8().expect("rgba8").as_raw());
                }
            }
        }
    }

    #[test]
    fn fast_thumbnail_solid_et_bords() {
        // Degenerate contents: solid, checkerboard, alpha 0/255.
        let uni = ImageBuffer::from_pixel(100, 100, Rgba([200, 40, 40, 255]));
        for nw in [1, 37, 100] {
            let expected = image::imageops::thumbnail(&uni, nw, nw);
            assert_eq!(
                fast_thumbnail_rgba8(&uni, nw, nw).as_raw(),
                expected.as_raw()
            );
        }
        let mut checker = ImageBuffer::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        for (x, y, px) in checker.enumerate_pixels_mut() {
            if (x + y) % 2 == 0 {
                *px = Rgba([255, 255, 255, 255]);
            }
        }
        for (nw, nh) in [(32, 32), (7, 7), (1, 1)] {
            let expected = image::imageops::thumbnail(&checker, nw, nh);
            assert_eq!(
                fast_thumbnail_rgba8(&checker, nw, nh).as_raw(),
                expected.as_raw(),
                "damier {nw}x{nh}"
            );
        }
    }

    #[test]
    fn thumb_et_preview_buf_dims_et_contenu() {
        // Thumbnail: exact 48×32 dimensions even from a portrait ratio;
        // solid content preserved within ±1 (box-filter rounding).
        let portrait =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(248, 350, Rgba([200, 40, 40, 255])));
        let thumb = thumb_buf(&portrait);
        assert_eq!((thumb.width, thumb.height), (48, 32));
        for px in thumb.data.as_chunks::<4>().0 {
            for (got, want) in px.iter().zip([200, 40, 40, 255]) {
                assert!(
                    (*got as i16 - want as i16).abs() <= 1,
                    "uni ⇒ miniature quasi unie : {px:?}"
                );
            }
        }
        // Preview: aspect preserved at the 2048 cap, bytes identical to the
        // reference (same box-filter as the capped path).
        let wide =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(3000, 1000, Rgba([10, 20, 30, 255])));
        let preview = preview_buf(&wide);
        assert_eq!((preview.width, preview.height), (2048, 683));
        let reference = wide.thumbnail(2048, 2048).to_rgba8();
        assert_eq!(preview.data.as_ref(), reference.as_raw().as_slice());
        // Under the cap: native image, zero resample.
        let small =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(100, 80, Rgba([1, 2, 3, 255])));
        let kept = preview_buf(&small);
        assert_eq!((kept.width, kept.height), (100, 80));
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::*;

    /// Non-square arbitrary rotation samples the right texels:
    /// regression test for the p11 corner clamped by height instead
    /// of width (invisible on square layers, streaks otherwise).
    /// Independent naive reference (plain inverse-map bilinear over
    /// the same bbox convention).
    #[test]
    fn rotation_arbitraire_non_carree_echantillonne_juste() {
        let (w0, h0) = (7u32, 4u32);
        let mut state = 11u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u8
        };
        let img = DynamicImage::ImageRgba8(ImageBuffer::from_fn(w0, h0, |_, _| {
            Rgba([next(), next(), next(), 255])
        }));
        let t = Transform2D {
            rotation_deg: 30.0,
            ..Transform2D::default()
        };
        let (buf, _, _) = prepare_top(&DrawItem::new(&img, t));
        // Naive reference: same bbox, same inverse map, written plainly.
        let corners = t.doc_corners(w0 as f32, h0 as f32);
        let min_x = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
        let min_y = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min);
        let max_x = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
        let max_y = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max);
        assert_eq!(buf.width(), (max_x - min_x).ceil().max(1.0) as u32);
        assert_eq!(buf.height(), (max_y - min_y).ceil().max(1.0) as u32);
        let rad = 30.0f32.to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        let (cx, cy) = (w0 as f32 / 2.0, h0 as f32 / 2.0);
        // Same convention as the implementation: destination relative
        // to the bbox center (ceil-snapped), not the frame center.
        let (bx, by) = (buf.width() as f32 / 2.0, buf.height() as f32 / 2.0);
        let rgba = img.to_rgba8();
        let sample = |x: u32, y: u32| rgba.get_pixel(x, y).0;
        let mut mismatches = 0;
        for (x, y, px) in buf.enumerate_pixels() {
            // Destination -> source (rotation about the center).
            let dx = x as f32 - bx;
            let dy = y as f32 - by;
            let sx = dx * cos + dy * sin + cx;
            let sy = -dx * sin + dy * cos + cy;
            // Validity band: within 1e-3 of the guard thresholds the
            // two implementations may disagree by 1 ulp on branch —
            // skip those pixels (the p11 regression lives strictly
            // inside, where x0 + 1 exceeds h - 1 with weight).
            let edge = sx < 1e-3
                || sy < 1e-3
                || sx > w0 as f32 - 1.0 - 1e-3
                || sy > h0 as f32 - 1.0 - 1e-3;
            if edge {
                continue;
            }
            if sx < 0.0 || sy < 0.0 || sx >= w0 as f32 - 1.0 || sy >= h0 as f32 - 1.0 {
                assert_eq!(px.0, [0, 0, 0, 0], "hors source = transparent");
                continue;
            }
            let (x0, y0) = (sx.floor() as u32, sy.floor() as u32);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let (w1, h1) = (w0 - 1, h0 - 1);
            let pa = sample(x0, y0);
            let pb = sample((x0 + 1).min(w1), y0);
            let pc = sample(x0, (y0 + 1).min(h1));
            let pd = sample((x0 + 1).min(w1), (y0 + 1).min(h1));
            let expected: [u8; 4] = std::array::from_fn(|c| {
                (pa[c] as f32 * (1.0 - fx) * (1.0 - fy)
                    + pb[c] as f32 * fx * (1.0 - fy)
                    + pc[c] as f32 * (1.0 - fx) * fy
                    + pd[c] as f32 * fx * fy)
                    .round() as u8
            });
            for (got, want) in px.0.iter().zip(expected.iter()) {
                if (*got as i16 - *want as i16).abs() > 2 {
                    mismatches += 1;
                }
            }
        }
        assert_eq!(mismatches, 0, "texels échantillonnés comme la référence");
    }
}
