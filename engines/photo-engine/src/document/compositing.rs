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
                    // Inclinaison : le chemin rapide (1 texture par calque)
                    // ne sait pas déformer — repli CPU requis.
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
// Buffers d'affichage
// ---------------------------------------------------------------------------

/// Buffer d'affichage interactif (downscale au-delà de 2048 px).
pub fn preview_buf(image: &DynamicImage) -> RgbaBuf {
    const MAX_PREVIEW: u32 = 2048;
    let (w, h) = image.dimensions();
    if w.max(h) <= MAX_PREVIEW {
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

/// Miniature 48×32 pour le panneau Calques.
pub fn thumb_buf(img: &DynamicImage) -> RgbaBuf {
    let t = img.resize(48, 32, ::image::imageops::FilterType::Triangle);
    let rgba = t.to_rgba8();
    RgbaBuf::from_vec(rgba.width(), rgba.height(), rgba.into_raw())
}

// ---------------------------------------------------------------------------
// Primitives de fusion CPU (inchangées, éprouvées par les tests golden)
// ---------------------------------------------------------------------------

/// Fusion d'un pixel : renvoie la couleur composée (top sur base)
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
        _ => [t[0], t[1], t[2]],                               // Normal
    };
    // Alpha compositing : top au-dessus de base, pondéré par l'alpha du top
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

/// Fusionne `top` DANS `base` en place — zéro allocation intermédiaire.
/// `mask` optionnel a les MÊMES dims que `top` (même transform) ; canal R =
/// couverture 0..255, multiplié à l'alpha échantillonné.
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
            // Échantillonne le dessus à (x - ox, y - oy) — transparent hors bornes
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
            // Pixel du dessus absent/translucide nul : base inchangée (skip rapide)
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

/// Item prêt à dessiner : image d'apparence + transformation.
pub struct DrawItem<'a> {
    image: &'a DynamicImage,
    transform: Transform2D,
}

impl<'a> DrawItem<'a> {
    /// Construit un item de dessin (utilisé par les tests).
    #[cfg(test)]
    pub(crate) fn new(image: &'a DynamicImage, transform: Transform2D) -> Self {
        Self { image, transform }
    }
}

/// Applique scale + skew + rotation (autour du centre, comme le canvas) à
/// l'image. Retourne (buffer transformé, offset_x ajusté, offset_y ajusté).
pub fn prepare_top(item: &DrawItem<'_>) -> (ImageBuffer<Rgba<u8>, Vec<u8>>, f32, f32) {
    let (w0, h0) = item.image.dimensions();
    let sx = item.transform.scale_x.clamp(0.05, 8.0);
    let sy = item.transform.scale_y.clamp(0.05, 8.0);
    if item.transform.has_skew() {
        // Inclinaison : impossible dans le chemin rapide — raytrace affine
        // unique (scale non uniforme + cisaillement + rotation).
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
    // Multiples de 90° avec epsilon (les rotations libres arrondies
    // silencieusement seraient une erreur visuelle)
    let is_0 = !(0.01..=359.99).contains(&rot);
    let is_90 = (rot - 90.0).abs() < 0.01;
    let is_180 = (rot - 180.0).abs() < 0.01;
    let is_270 = (rot - 270.0).abs() < 0.01;
    if is_90 || is_270 {
        // 90° / 270° — swap via imageops (rapide, sans interpolation)
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
        // Rotation arbitraire : bounding box + échantillonnage bilinéaire
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
        // Remplissage en parallèle (rayon)
        out.enumerate_pixels_mut()
            .par_bridge()
            .for_each(|(x, y, px)| {
                // Destination -> source (rotation inverse)
                let dx = x as f32 - cx1;
                let dy = y as f32 - cy1;
                let sx = dx * cos_r + dy * sin_r + cx0;
                let sy = -dx * sin_r + dy * cos_r + cy0;
                if sx >= 0.0 && sy >= 0.0 && sx < tw - 1.0 && sy < th - 1.0 {
                    // Bilinéaire
                    let x0 = sx.floor() as u32;
                    let y0 = sy.floor() as u32;
                    let fx = sx - x0 as f32;
                    let fy = sy - y0 as f32;
                    let p00 = buf.get_pixel(x0, y0);
                    let p10 = buf.get_pixel((x0 + 1).min(buf.width() - 1), y0);
                    let p01 = buf.get_pixel(x0, (y0 + 1).min(buf.height() - 1));
                    let p11 = buf.get_pixel(
                        (x0 + 1).min(buf.height() - 1),
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

/// Rasterisation affine (scale non uniforme + skew + rotation) en un seul
/// échantillonnage bilinéaire via inverse mapping. Convention identique à
/// [`Transform2D::local_to_doc`] : scale → skew → rotation autour du centre,
/// puis décalage. L'offset renvoyé est le coin min de la bbox englobante
/// (fractionnaire, arrondi ensuite par `blend_into`).
fn prepare_top_affine(
    item: &DrawItem<'_>,
    sx: f32,
    sy: f32,
) -> (ImageBuffer<Rgba<u8>, Vec<u8>>, f32, f32) {
    let (w0, h0) = item.image.dimensions();
    let t = item.transform;
    let ox = t.offset_x;
    let oy = t.offset_y;
    // Échelles clampées comme `prepare_top` : la matrice partagée est
    // calculée sur la transform clampée pour rester à l'identique.
    let tc = Transform2D {
        scale_x: sx,
        scale_y: sy,
        ..t
    };
    let rad = t.rotation_deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let cx = w0 as f32 / 2.0;
    let cy = h0 as f32 / 2.0;

    // A = R * K * S (scale, puis cisaillement, puis rotation) :
    // m00 m01 / m10 m11 = K*S = [[sx, kx*sy],[ky*sx, sy]]
    let (m00, m01, m10, m11) = tc.shear_scale_matrix();
    let det = m00 * m11 - m01 * m10;
    if det.abs() < 1e-4 {
        // Cisaillement dégénéré (tan → ∞) : repli sur scale seul.
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
            // Destination -> source (inverse affine complet), bilinéaire.
            // Le buffer couvre [min_x, min_x+bbox_w) × [min_y, …].
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

/// Fusionne multiplicativement les masques de groupe (espace canvas, dims de
/// `sub`, inversion incluse) — même rôle que `combine_masks` pour un groupe.
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

/// Couverture d'un masque de groupe : sample de l'image centrée sur `sub`,
/// sans transform (identité), en respectant `inverted` — TODO vraie transform.
fn prepare_group_mask(
    m: &LayerMask,
    sub: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut buf = ImageBuffer::from_pixel(sub.width(), sub.height(), Rgba([255, 255, 255, 255]));
    let (mw, mh) = (m.image.width(), m.image.height());
    let copy_w = mw.min(sub.width());
    let copy_h = mh.min(sub.height());
    for y in 0..copy_h {
        for x in 0..copy_w {
            let p = m.image.get_pixel(x, y);
            let v = if m.inverted { 255 - p[0] } else { p[0] };
            buf.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    buf
}

/// Multiplie deux buffers de couverture (canal R) élément par élément.
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
// Compositing récursif (groupes + ajustements)
// ---------------------------------------------------------------------------

type Resolver<'a> = &'a dyn Fn(Uuid) -> Option<Arc<DynamicImage>>;

/// Demi-extents nécessaires pour couvrir tous les items visibles d'une
/// portée, autour du centre document (plan infini). Récursif : un groupe
/// contribue via ses enfants (mêmes coordonnées canvas).
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
            LayerNode::Pixel(l) => {
                if !l.visible || l.opacity <= 0.01 {
                    continue;
                }
                let Some(img) = resolve(l.id) else {
                    continue;
                };
                let (w0, h0) = (img.width() as f32, img.height() as f32);
                // Corners du calque transformé (scale clampé comme prepare_top)
                let clamped = Transform2D {
                    scale_x: l.transform.scale_x.clamp(0.05, 8.0),
                    scale_y: l.transform.scale_y.clamp(0.05, 8.0),
                    ..l.transform
                };
                for (qx, qy) in clamped.doc_corners(w0, h0) {
                    half.0 = half.0.max((qx - doc_cx).abs());
                    half.1 = half.1.max((qy - doc_cy).abs());
                }
            }
        }
    }
}

/// Fond récursivement une portée dans l'accumulateur (déjà dimensionné).
/// Retourne true si au moins un élément a contribué.
///
/// Pixel : transform + blend direct. Groupe : les enfants composent d'abord
/// dans un buffer transparent, puis le résultat est fondu avec l'opacité et
/// le mode du groupe. Ajustement : la chaîne s'applique à l'accumulateur,
/// pondérée par opacité.
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

/// Compteurs d'un repli composite (instrumentation légère, temporaire).
///
/// `pixels_processed` compte les pixels réellement parcourus : chaque
/// `blend_into` / mix d'ajustement balaie TOUT l'accumulateur, donc
/// `pixels_processed ≈ layers_blended × scope_px` (plus les mixes).
/// `blend_us` couvre prepare_top + blend par calque (allocs incluses),
/// hors extents et hors allocation de l'accumulateur.
#[derive(Debug, Default, Clone, Copy)]
pub struct CompositeStats {
    /// Pixels de l'accumulateur (scope).
    pub scope_px: u64,
    /// Microsecondes d'allocation de l'accumulateur (zéroïsation).
    pub acc_alloc_us: u128,
    /// Microsecondes des passes prepare + blend + mix.
    pub blend_us: u128,
    /// Calques pixels et groupes effectivement blendés.
    pub layers_blended: u64,
    /// Pixels parcourus (somme des balayages d'accumulateur).
    pub pixels_processed: u64,
}

/// Variante instrumentée de [`fold_scope`] : corps identique, avec
/// comptage des calques blendés, des pixels parcourus et du temps
/// prepare + blend. Comportement de rendu inchangé.
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
                // Masques de calque bakés dans l'apparence (source × filtres ×
                // masques) : plus rien à atténuer ici — blend direct.
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
                let applied = apply_adjustment(acc, &a.filters, a.opacity);
                stats.blend_us += t_blend.elapsed().as_micros();
                if applied {
                    // Mix pleine surface (original + ajusté + fusion).
                    stats.pixels_processed += acc_px;
                    contributed = true;
                }
            }
        }
    }
    contributed
}

/// Composite le résultat filtré d'un sous-calque de filtre SUR l'accumulé
/// (façon Affinity : le filtre s'applique au contenu en dessous, puis son
/// résultat est fondu avec opacité/fusion/transform/masques propres).
///
/// Espaces : `base` et `filtered` vivent dans l'espace du PARENT (mêmes
/// dimensions, sortie d'effet). Les masques sont appliqués EN PREMIER dans
/// cet espace (même espace que la peinture, qui utilise la transform du
/// porteur), PUIS le résultat atténué est transformé — jamais l'inverse.
/// `filtered` est pris par valeur : le cas neutre ne copie rien (move).
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

/// Atténue une image par la couverture combinée des masques ACTIFS
/// (canal alpha × couverture — même sémantique que `blend_into`).
/// Les masques vivent dans l'espace source du porteur : dimensions
/// identiques requises, sinon garde-fou (pas d'atténuation plutôt qu'un
/// noir erroné). Sans masque actif : retourne l'image telle quelle.
fn attenuate_by_masks(img: DynamicImage, masks: &[LayerMask]) -> DynamicImage {
    let Some(cover) = combined_mask_coverage(masks) else {
        return img;
    };
    if cover.dimensions() != img.dimensions() {
        return img;
    }
    attenuate_by_coverage(img, &cover)
}

/// Combine les masques ACTIFS en une couverture unique (multiplication des
/// couvertures, `inverted` appliqué). `None` = aucun masque actif.
///
/// La couverture est une donnée pure, cacheable indépendamment de l'image :
/// c'est elle qu'un futur chemin shader échantillonnera au draw.
pub fn combined_mask_coverage(masks: &[LayerMask]) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let mut it = masks.iter().filter(|m| m.enabled);
    let first = it.next()?;
    let mut cover = mask_coverage(first);
    for m in it {
        cover = multiply_coverage(&cover, &mask_coverage(m));
    }
    Some(cover)
}

/// Multiplie le canal alpha d'une image par une couverture (canal R).
/// Les dimensions doivent correspondre (garde-fou à l'appelant).
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

/// Couverture d'un masque en buffer propre (`inverted` appliqué — même
/// règle que `prepare_mask`).
fn mask_coverage(mask: &LayerMask) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    if !mask.inverted {
        return (*mask.image).clone();
    }
    let mut buf = (*mask.image).clone();
    for px in buf.pixels_mut() {
        let v = 255 - px[0];
        *px = Rgba([v, v, v, 255]);
    }
    buf
}

/// Bake l'apparence d'un calque pixels : atténue `image` par la couverture
/// combinée de ses masques ACTIFS, dans l'ESPACE SOURCE (identité — le
/// canvas applique la transform du calque derrière). Aucun masque actif →
/// `image` est retourné tel quel (même Arc, zéro copie).
///
/// Rend les masques de calque homogènes aux masques de sous-calques de
/// filtre : ils font partie de l'apparence cacheable, plus du repli CPU.
pub fn apply_layer_masks(image: Arc<DynamicImage>, masks: &[LayerMask]) -> Arc<DynamicImage> {
    let Some(cover) = combined_mask_coverage(masks) else {
        return image;
    };
    apply_coverage(image, &cover)
}

/// Applique une couverture DÉJÀ combinée à une image.
///
/// Garde-fou identique à [`apply_layer_masks`] : couverture de dimensions
/// différentes → rééchantillonnée aux dimensions de l'image plutôt qu'une
/// atténuation erronée. Exposée pour que le cache puisse réutiliser une
/// couverture sans la recombiner (et, à terme, pour l'échantillonner au
/// draw dans un shader au lieu de la baker ici).
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

/// Applique une chaîne d'ajustements à l'accumulateur, pondérée par
/// l'opacité (mix linéaire original ↔ ajusté). Retourne true si appliqué.
pub fn apply_adjustment(
    acc: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    filters: &[FilterNode],
    opacity: f32,
) -> bool {
    let weight = (opacity / 100.0).clamp(0.0, 1.0);
    if weight <= 0.0 || !filters.iter().any(|f| f.enabled) || acc.width() == 0 {
        return false;
    }
    let original = acc.clone();
    let adjusted = crate::filters::render_nodes(
        &Arc::new(DynamicImage::ImageRgba8(original.clone())),
        filters,
    );
    let DynamicImage::ImageRgba8(adjusted_buf) = adjusted.as_ref() else {
        return false;
    };
    if adjusted_buf.dimensions() != acc.dimensions() {
        return false; // défense : effet exotique ayant redimensionné — ignoré
    }
    let raw_acc = acc.as_flat_samples_mut().samples;
    let raw_orig = original.as_raw();
    let raw_adj = adjusted_buf.as_raw();
    raw_acc
        .par_chunks_exact_mut(4)
        .zip(raw_orig.par_chunks_exact(4))
        .zip(raw_adj.par_chunks_exact(4))
        .for_each(|((dst, src), adj)| {
            for c in 0..4 {
                dst[c] = (src[c] as f32 * (1.0 - weight) + adj[c] as f32 * weight).round() as u8;
            }
        });
    true
}
