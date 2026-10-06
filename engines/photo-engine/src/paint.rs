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

//! Peinture destructive sur calque : rastérisation d'un trait de pinceau
//! ou de gomme.
//!
//! Le trait est tamponné dans un masque de couverture (évite le
//! assombrissement aux recouvrements), puis composé sur la base :
//! - [`StrokeMode::Paint`] : source-over avec opacité uniforme — comme un
//!   vrai coup de pinceau
//! - [`StrokeMode::Erase`] : destination-out — réduit l'alpha des pixels
//!   visés sans toucher à leur couleur

/// Ce que fait le trait sur les pixels du calque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeMode {
    /// Peint la couleur du pinceau (source-over)
    Paint,
    /// Efface : réduit l'alpha proportionnellement à l'opacité du trait
    Erase,
}

/// Réglages d'un outil à trait (pinceau ou gomme).
#[derive(Clone, Copy, Debug)]
pub struct BrushParams {
    /// Rayon en pixels DOCUMENT (l'espace visuel du canvas). `commit_stroke`
    /// le convertit en ellipse dans l'espace du calque via la transform.
    pub radius: f32,
    /// Couleur RGB (ignorée en mode Erase)
    pub color: [u8; 3],
    /// Opacité globale du trait [0..1] (les recouvrements internes ne
    /// s'accumulent PAS — un seul composite à la fin)
    pub opacity: f32,
    pub mode: StrokeMode,
}

/// Rastérise un trait circulaire dans un tampon RGBA8 (w×h, espace CALQUE).
///
/// * `points` : polyligne en coordonnées calque (centre du pixel (0,0) = 0.0)
pub fn paint_stroke_rgba(rgba: &mut [u8], w: u32, h: u32, points: &[(f32, f32)], b: &BrushParams) {
    paint_stroke_impl(rgba, w, h, points, b, b.radius, b.radius);
}

/// Variante elliptique (rayons X/Y indépendants) — permet au trait de suivre
/// une échelle de calque NON-unitaire : un cercle doc devient une ellipse
/// dans l'espace du calque. Les rayons sont en pixels CALQUE.
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

/// Boîte englobante d'un trait élargie du rayon (`pad`), en flottants
/// (l'appelant planche/clippe). `None` si aucun point. Partagée par le
/// chemin permanent et l'overlay interactif (même géométrie).
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

/// Tamponne un disque elliptique dans un masque de couverture (coordonnées
/// absolues, indexation relative à l'origine `(ox, oy)`, stride `stride`).
/// Le clip DOIT être contenu dans le masque (garanti par les deux appelants :
/// boîte locale ici, région overlay là-bas) — aucun pixel hors masque.
// Signature positionnelle volontaire : primitive chaude partagée par le
// chemin permanent et l'overlay (un struct grouperait sans gain).
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
            // Ellipse d'axes rx/ry (cercle quand rx == ry)
            if (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1.0 {
                let mi = ((py - oy) as usize) * stride + ((px - ox) as usize);
                mask[mi] = 255;
            }
        }
    }
}

/// Tamponne une polyligne (premier point + segments espacés d'un pas
/// ~ rayon/3) dans un masque. L'union est idempotente : tamponner deux fois
/// les mêmes segments donne le même masque (zéro accumulation) — propriété
/// qui autorise le tamponnage incrémental par morceaux de l'overlay.
// Signature positionnelle volontaire : cf. `stamp_disc`.
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
    // Tampons espacés le long des segments (pas ~ rayon max / 3 → trait continu)
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

/// Rééchantillonne une polyligne de geste (1 point/frame) en courbe
/// lisse : subdivision des segments au-delà de `max_gap`, puis une
/// passe de Chaikin (coupe des coins, extrémités préservées).
///
/// Sans ça, un trait rapide (points épars) se rend en segments
/// droits entre échantillons — d'autant plus visible que le trait
/// est long et le fps bas. Les gestes lents (points denses) sont
/// quasi inchangés (coins déjà fins). Le résultat reste dans
/// l'enveloppe convexe des entrées (bbox conservatrice valide).
#[must_use]
pub fn resample_stroke(points: &[(f32, f32)], max_gap: f32) -> Vec<(f32, f32)> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let max_gap = max_gap.max(1.0);
    // Subdivision : aucun segment ne dépasse `max_gap`.
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
    // Chaikin : Q = 3/4·P + 1/4·suivant, R = 1/4·P + 3/4·suivant.
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

/// Composite un masque de couverture sur un tampon RGBA (mêmes formules que
/// le chemin historique : source-over à opacité uniforme, destination-out
/// en gomme). `rgba` est indexé en absolu (`stride` = largeur du tampon) ;
/// `mask` couvre exactement le rect `(x0, y0, w, h)` (origine implicite).
// Signature positionnelle volontaire : cf. `stamp_disc`.
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
    // --- Composite selon le mode ---
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
                    // destination-out : l'alpha est réduit, la couleur reste
                    // valide (pixels droits — RGB inchangé quand alpha baisse).
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

    // --- Bounding box du trait (limité au calque) ---
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

    // --- Masque de couverture 0/255 ---
    let mut mask = vec![0u8; bw * bh];
    stamp_polyline(
        &mut mask, bw, bx0 as i64, by0 as i64, bx0 as i64, by0 as i64, bx1 as i64, by1 as i64,
        points, rx, ry,
    );

    composite_coverage(
        rgba, w as usize, bx0, by0, bw as u32, bh as u32, &mask, bw, b,
    );
}

/// Résultat d'un commit de trait : buffers prêts pour la couche UI
/// (aucun calcul lourd restant côté interface).
#[derive(Clone)]
pub struct StrokeCommit {
    /// Pixels calque complets (w×h RGBA8)
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Aperçu interactif (≤2048 px) — buffer pur, conversion UI côté app
    pub preview: crate::document::RgbaBuf,
    /// Miniature 48×32 — buffer pur
    pub thumb: crate::document::RgbaBuf,
}

impl std::fmt::Debug for StrokeCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Pixels omis volontairement (buffers volumineux)
        f.debug_struct("StrokeCommit")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// Travail LOURD d'un coup de pinceau/gomme — à exécuter HORS thread UI
/// (`Task::perform`) : copie du buffer, rastérisation, aperçu, miniature.
///
/// * `base`      : image source du calque (Arc partagé, non modifiée)
/// * `pts_doc`   : polyligne en coordonnées DOCUMENT
/// * `transform` : transform courant du calque (doc → calque)
pub fn commit_stroke(
    base: &image::DynamicImage,
    pts_doc: &[(f32, f32)],
    transform: &crate::document::Transform2D,
    brush: &BrushParams,
) -> StrokeCommit {
    // Pipeline du trait confiné au pool de rendu dédié (invariant #4).
    crate::render_pool::run_parallel(|| commit_stroke_locked(base, pts_doc, transform, brush))
}

/// Corps de [`commit_stroke`] — jamais appelé directement.
fn commit_stroke_locked(
    base: &image::DynamicImage,
    pts_doc: &[(f32, f32)],
    transform: &crate::document::Transform2D,
    brush: &BrushParams,
) -> StrokeCommit {
    use ::image::GenericImageView;
    let (lw, lh) = base.dimensions();

    // Espace DOCUMENT → espace CALQUE : inverse affine complet
    // (échelle non uniforme + skew + rotation inversées autour du centre du
    // calque) — identique au transform de draw et de prepare_top.
    let ox = transform.offset_x;
    let oy = transform.offset_y;
    let sx = transform.scale_x.clamp(0.05, 8.0);
    let sy = transform.scale_y.clamp(0.05, 8.0);
    // Matrice cisaillement×échelle canonique (même source que le compositing).
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
        // Cisaillement dégénéré : repli sur l'ancien chemin uniforme.
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
                // q - T où T = centre du rectangle scalé (offset + demi-extents)
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
    // LE RAYON vit en espace doc (celui du curseur) : on le ramène dans
    // l'espace du calque par les échelles (min. 0.5 px). Cercle doc →
    // ellipse calque dès que scale_x ≠ scale_y — à l'écran, la zone peinte
    // épouse exactement le curseur, quelle que soit la taille du calque.
    // (La rotation/cisaillement du calque ne change pas la forme vue depuis
    // le doc quand sx == sy ; une ellipse droite est une bonne approximation
    // sinon.)
    let r_x = (brush.radius / sx).max(0.5);
    let r_y = (brush.radius / sy).max(0.5);
    paint_stroke_ellipse(&mut rgba, lw, lh, &pts, brush, r_x, r_y);
    // La longueur est garantie par construction : to_rgba8().into_raw() retourne w*h*4
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
        // Geste lent : quasi inchangé (mêmes extrémités, pas d'explosion).
        let dense = vec![(0.0, 0.0), (1.0, 0.5), (2.0, 1.0), (3.0, 1.5)];
        let out = resample_stroke(&dense, 4.0);
        assert_eq!(out.first(), Some(&(0.0, 0.0)));
        assert_eq!(out.last(), Some(&(3.0, 1.5)));
        assert!(out.len() < dense.len() * 4);
        // Angle droit épars : le coin est coupé (plus aucun point
        // exactement sur le sommet, remplacé par Q/R de Chaikin).
        let angle = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let out = resample_stroke(&angle, 4.0);
        assert!(out.len() > angle.len(), "subdivision + Chaikin");
        assert_eq!(out.first(), Some(&(0.0, 0.0)));
        assert_eq!(out.last(), Some(&(10.0, 10.0)));
        assert!(
            !out.iter().any(|&(x, y)| x == 10.0 && y == 0.0),
            "sommet (10,0) adouci : {out:?}"
        );
        // Cas limites : vide, point unique, segment dégénéré.
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
        // Centre du trait : rouge opaque
        let idx = ((8 * w + 8) * 4) as usize;
        assert_eq!(rgba[idx], 255);
        assert_eq!(rgba[idx + 3], 255);
        // Coin hors trait : transparent
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
        assert_eq!(rgba[idx], 128); // mélange 50/50
        assert_eq!(rgba[idx + 3], 255); // fond opaque préservé
    }

    #[test]
    fn gomme_opaque_efface_le_centre_preserve_les_bords() {
        let w = 16u32;
        let h = 16u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize]; // blanc opaque
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
        assert_eq!(rgba[centre + 3], 0); // alpha effacé
        assert_eq!(rgba[centre], 255); // RGB inchangé (droits)
        let coin = 0usize; // pixel document (0, 0), hors trait
        assert_eq!(rgba[coin + 3], 255); // hors trait : intact
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
        assert_eq!(rgba[idx], 200); // couleur préservée
    }

    #[test]
    fn gomme_sur_pixel_deja_transparent_sans_effet_bord() {
        let w = 8u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize]; // tout transparent
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
        // Rien ne peut devenir opaque ni coloré par une gomme
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
        // Hors ellipse (centre du pixel ≳ frontière) : transparent
        assert!(!px(20 + 8, 20), "au-delà de rx");
        assert!(!px(20, 20 + 3), "au-delà de ry");
        assert!(!px(4, 4), "coin lointain");
    }

    #[test]
    fn commit_stroke_sur_calque_redimensionne_adapte_le_rayon() {
        use crate::document::Transform2D;
        // Calque 64×64 affiché à 50 % : le rayon DOC (10 px) doit produire un
        // disque de ~20 px LAYER (10 / 0.5) — à l'écran les deux coïncident.
        let base = image::RgbaImage::new(64, 64); // transparent : seule la zone peinte compte
        let transform = Transform2D {
            offset_x: 100.0,
            offset_y: 100.0,
            scale_x: 0.5,
            scale_y: 0.5,
            ..Transform2D::default()
        };
        // Point doc au centre du calque (offset + demi-étendue scalée).
        let centre_doc = (100.0 + 32.0 * 0.5, 100.0 + 32.0 * 0.5);
        let commit = commit_stroke(
            &image::DynamicImage::ImageRgba8(base.clone()),
            &[centre_doc],
            &transform,
            &BrushParams {
                radius: 10.0, // PIXELS DOC
                color: [255, 0, 0],
                opacity: 1.0,
                mode: StrokeMode::Paint,
            },
        );
        let rgba = commit.rgba;
        // Largueur horizontale peinte mesurée en px calque (~ 2 × 20).
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
