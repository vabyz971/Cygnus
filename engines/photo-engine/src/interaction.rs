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

//! Surface d'interaction transitoire (Phase 6G.3P) : feedback de peinture
//! pendant le drag, AVANT le commit définitif dans le document.
//!
//! ```text
//! MouseDown → begin_stroke() → MouseMove* → update (overlay)
//!     → MouseUp → commit (chemin permanent existant) → clear()
//!     → annulation → clear() (document intact)
//! ```
//!
//! Règles :
//!
//! - espace CALQUE, mêmes points et mêmes paramètres que le commit : le
//!   masque de couverture est bâti avec EXACTEMENT les primitives partagées
//!   de `paint` (`stroke_bbox`, `stamp_polyline`), et l'union des masques est
//!   idempotente — tamponner par morceaux donne le même masque qu'en une fois ;
//! - l'overlay ne touche JAMAIS au document (ni snapshots, ni dirty, ni
//!   worker) : le commit passe par le chemin `PaintStroke` existant, qui
//!   reste la seule écriture permanente (équivalence prouvée par tests) ;
//! - région bornée au trait (pas au calque) : mémoire et uploads en O(stroke),
//!   jamais en O(document) ;
//! - mode Paint : pixels exacts sur fond opaque (même formule source-over) ;
//!   mode Erase : voile blanc indicatif (la gomme réelle a besoin de la base ;
//!   le commit reste exact, seul le transitoire est indicatif) ;
//! - affichage réservé aux transforms rigides identité/translation/échelle
//!   ([`overlay_displayable`]) : rotation/skew désactivés côté présentation
//!   (moteur exact quand même, porte d'extension documentée).

use crate::document::Transform2D;
use crate::paint::{
    BrushParams, StrokeMode, composite_coverage, resample_stroke, stamp_polyline, stroke_bbox,
};

/// Métriques d'interaction (Phase 6G.3P §13) : événements et volumes de
/// travail, jamais de temps imposé (le matériel varie).
#[derive(Debug, Default, Clone, Copy)]
pub struct InteractionMetrics {
    /// Points reçus (bruts, avant filtrage).
    pub points_received: u64,
    /// Points coalescés (dégénérés `< EPSILON` ou non finis — sans effet).
    pub points_coalesced: u64,
    /// Soumissions de rendu (`render()` appelé).
    pub submissions: u64,
    /// Rendus effectués (== `submissions`, synchrone — documenté pour une
    /// future version asynchrone où ils divergeraient).
    pub completions: u64,
    /// Pixels (re)composés dans l'image overlay (cumul, bornes des segments).
    pub surface_pixels_written: u64,
    /// Mur du dernier `render()` (µs, indicatif, jamais asserté en test).
    pub last_render_us: u128,
}

/// Image transitoire pour présentation : région + pixels + génération.
#[derive(Debug, Clone)]
pub struct OverlayFrame {
    /// Génération de la session qui l'a produite (cf. `is_current`).
    pub generation: u64,
    /// Origine région en coords calque absolues.
    pub x: i32,
    /// Origine région en coords calque absolues.
    pub y: i32,
    /// Largeur région (px).
    pub width: u32,
    /// Hauteur région (px).
    pub height: u32,
    /// Pixels RGBA8 de la région (stroke-only, voir docs du module).
    pub rgba: Vec<u8>,
}

/// Session de trait transitoire (un geste = une instance logique réarmée
/// par `begin_stroke`).
#[derive(Debug, Clone)]
pub struct InteractionOverlay {
    brush: BrushParams,
    rx: f32,
    ry: f32,
    mask: Vec<u8>,
    image: Vec<u8>,
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
    /// Capacité allouée (`mask`/`image` font `cap_w × cap_h`, stride =
    /// `cap_w`) : la région logique (`w × h`) croît sans recopie tant
    /// qu'elle tient dans la capacité (croissance géométrique à la
    /// réallocation) — un balayage ne recopie plus toute la région à
    /// chaque frame (O(1) amorti au lieu de O(région)).
    cap_w: u32,
    cap_h: u32,
    last: Option<(f32, f32)>,
    generation: u64,
    dirty: bool,
    metrics: InteractionMetrics,
}

impl InteractionOverlay {
    /// Overlay vide (génération 0, aucun pixel).
    #[must_use]
    pub fn new() -> Self {
        Self {
            brush: BrushParams {
                radius: 0.0,
                color: [0, 0, 0],
                opacity: 0.0,
                mode: StrokeMode::Paint,
            },
            rx: 0.0,
            ry: 0.0,
            mask: Vec::new(),
            image: Vec::new(),
            ox: 0,
            oy: 0,
            w: 0,
            h: 0,
            cap_w: 0,
            cap_h: 0,
            last: None,
            generation: 0,
            dirty: false,
            metrics: InteractionMetrics::default(),
        }
    }

    /// Démarre un geste : réarme l'état, incrémente la génération (les
    /// rendus taggés d'une génération antérieure deviennent périmés).
    /// Retourne la nouvelle génération.
    pub fn begin_stroke(&mut self, brush: BrushParams) -> u64 {
        self.brush = brush;
        self.rx = brush.radius;
        self.ry = brush.radius;
        self.mask.clear();
        self.image.clear();
        self.ox = 0;
        self.oy = 0;
        self.w = 0;
        self.h = 0;
        self.last = None;
        self.generation = self.generation.wrapping_add(1);
        self.dirty = false;
        self.generation
    }

    /// Génération courante.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Vrai si `generation` est la courante (résultat périmé ⇒ à jeter).
    #[must_use]
    pub fn is_current(&self, generation: u64) -> bool {
        generation == self.generation
    }

    /// Vrai si aucun dab (région vide).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Métriques cumulées de la session.
    #[must_use]
    pub fn metrics(&self) -> InteractionMetrics {
        self.metrics
    }

    /// Supprime l'état (annulation ou après commit) : document intact par
    /// construction (l'overlay n'y a jamais touché). La génération est
    /// conservée (seul `begin_stroke` l'incrémente).
    pub fn clear(&mut self) {
        self.mask.clear();
        self.image.clear();
        self.w = 0;
        self.h = 0;
        self.cap_w = 0;
        self.cap_h = 0;
        self.last = None;
        self.dirty = false;
    }

    /// Ajoute des points (coords calque absolues, MÊMES valeurs qu'au commit).
    /// Tamponne uniquement les nouveaux segments depuis le dernier point
    /// (union idempotente : chevauchements et doublons sans effet).
    /// Retourne vrai si le masque a changé (présentation à rafraîchir).
    /// Les points non finis sont ignorés un par un (le commit historique,
    /// lui, produit un résultat implémentation-défini sur entrée adverse —
    /// hors cas supportés, documenté) ; les segments dégénérés (`< EPSILON`,
    /// comme le chemin permanent) sont coalescés et comptés.
    ///
    /// Lissage : `fresh` ([dernier connu] + nouveaux) passe par le MÊME
    /// `resample_stroke` que le commit worker (subdivision + Chaikin) —
    /// le transitoire montre des courbes, pas des segments, y compris en
    /// balayage rapide (points épars). L'équivalence transitoire == commit
    /// est préservée : mêmes entrées, même fonction (micro-écarts aux
    /// jonctions de paquets, invisibles et remplacés au `MouseUp`).
    pub fn add_points(&mut self, points: &[(f32, f32)]) -> bool {
        if points.is_empty()
            || !self.brush.radius.is_finite()
            || self.brush.radius <= 0.0
            || self.brush.opacity <= 0.0
        {
            return false;
        }
        self.metrics.points_received += points.len() as u64;
        // Filtrage : finis et non dégénérés (même règle EPSILON que le
        // chemin permanent — un segment nul ne tamponne rien de toute façon).
        let mut fresh: Vec<(f32, f32)> = Vec::with_capacity(points.len() + 1);
        if let Some(last) = self.last {
            fresh.push(last);
        }
        for &(x, y) in points {
            if !x.is_finite() || !y.is_finite() {
                self.metrics.points_coalesced += 1;
                continue;
            }
            let skip = match fresh.last() {
                None => false,
                Some(&(px, py)) => {
                    let dx = x - px;
                    let dy = y - py;
                    (dx * dx + dy * dy).sqrt() < f32::EPSILON
                }
            };
            if skip {
                self.metrics.points_coalesced += 1;
                continue;
            }
            fresh.push((x, y));
        }
        // Premier point d'un geste vide : amorcé comme disque isolé (comme
        // le chemin permanent qui tamponne `points[0]`).
        if fresh.is_empty() {
            return false;
        }
        // Même lissage que le commit (subdivision + Chaikin sur [dernier
        // connu] + nouveaux) : les extrémités sont préservées, donc la
        // continuité inter-paquets (`self.last`) est exacte.
        let fresh = resample_stroke(&fresh, self.brush.radius.max(2.0));
        let changed = self.stamp_fresh(&fresh);
        if let Some(&p) = fresh.last() {
            self.last = Some(p);
        }
        changed
    }

    /// Tamponne `fresh` ([dernier connu] + nouveaux) en agrandissant la
    /// région si besoin. Retourne vrai si des pixels ont changé.
    fn stamp_fresh(&mut self, fresh: &[(f32, f32)]) -> bool {
        let rx = self.rx.max(0.5);
        let ry = self.ry.max(0.5);
        let Some((x0f, y0f, x1f, y1f)) = stroke_bbox(fresh, rx, ry) else {
            return false;
        };
        let (nx0, ny0) = (x0f.floor() as i32, y0f.floor() as i32);
        let (nx1, ny1) = (x1f.ceil() as i32, y1f.ceil() as i32);
        if nx1 <= nx0 || ny1 <= ny0 {
            return false;
        }
        self.grow_to(nx0, ny0, nx1, ny1);
        // Stride = capacité (la région logique `w × h` vit en haut à
        // gauche de l'origine) ; les clips restent la région logique —
        // le padding au-delà n'est jamais écrit ni lu.
        let stride = self.cap_w as usize;
        stamp_polyline(
            &mut self.mask,
            stride,
            self.ox as i64,
            self.oy as i64,
            self.ox as i64,
            self.oy as i64,
            (self.ox + self.w as i32) as i64,
            (self.oy + self.h as i32) as i64,
            fresh,
            rx,
            ry,
        );
        // Recomposite la zone étendue depuis le masque COMPLET (pas depuis
        // l'image : chaque pixel final ne dépend que du masque final —
        // exactitude indépendante de l'historique des morceaux).
        let (dx0, dy0) = ((nx0 - self.ox).max(0), (ny0 - self.oy).max(0));
        let dx1 = (nx1 - self.ox).min(self.w as i32);
        let dy1 = (ny1 - self.oy).min(self.h as i32);
        if dx1 > dx0 && dy1 > dy0 {
            let rw = self.cap_w as usize;
            let mask = &self.mask;
            if self.brush.mode == StrokeMode::Erase {
                // Pas de base ici : voile blanc indicatif sur la couverture
                // (le commit, lui, applique destination-out sur les vrais
                // pixels — exact, documenté au module).
                for y in dy0..dy1 {
                    for x in dx0..dx1 {
                        let mi = (y as usize) * stride + (x as usize);
                        if mask[mi] > 0 {
                            let di = ((y as usize) * rw + (x as usize)) * 4;
                            let a = mask[mi];
                            self.image[di] = 255;
                            self.image[di + 1] = 255;
                            self.image[di + 2] = 255;
                            self.image[di + 3] = a;
                        }
                    }
                }
            } else {
                let region_mask: Vec<u8> = (dy0..dy1)
                    .flat_map(|y| {
                        let base = (y as usize) * stride;
                        (dx0..dx1).map(move |x| mask[base + x as usize])
                    })
                    .collect();
                let mut region_img = vec![0u8; ((dx1 - dx0) as usize) * ((dy1 - dy0) as usize) * 4];
                composite_coverage(
                    &mut region_img,
                    (dx1 - dx0) as usize,
                    0,
                    0,
                    (dx1 - dx0) as u32,
                    (dy1 - dy0) as u32,
                    &region_mask,
                    (dx1 - dx0) as usize,
                    &self.brush,
                );
                for y in dy0..dy1 {
                    for x in dx0..dx1 {
                        let si = (((y - dy0) as usize) * ((dx1 - dx0) as usize)
                            + ((x - dx0) as usize))
                            * 4;
                        let di = ((y as usize) * rw + (x as usize)) * 4;
                        self.image[di..di + 4].copy_from_slice(&region_img[si..si + 4]);
                    }
                }
            }
            let area = ((dx1 - dx0) as u64) * ((dy1 - dy0) as u64);
            self.metrics.surface_pixels_written += area;
        }
        self.dirty = true;
        true
    }

    /// Agrandit la région logique (jamais réduite pendant un geste).
    /// Zéro recopie tant que l'union tient dans la capacité allouée ;
    /// sinon réallocation géométrique (×1.5 arrondi à 64 px) avec recopie
    /// unique de l'ancien contenu. Le padding au-delà de la région
    /// logique reste à zéro (jamais écrit : les clips de tamponnage sont
    /// la région logique) et n'est jamais lu (`render` n'exporte que la
    /// région logique).
    fn grow_to(&mut self, nx0: i32, ny0: i32, nx1: i32, ny1: i32) {
        if self.w == 0 || self.h == 0 {
            self.ox = nx0;
            self.oy = ny0;
            self.w = (nx1 - nx0).max(1) as u32;
            self.h = (ny1 - ny0).max(1) as u32;
            self.cap_w = self.w;
            self.cap_h = self.h;
            self.mask = vec![0u8; self.w as usize * self.h as usize];
            self.image = vec![0u8; self.w as usize * self.h as usize * 4];
            return;
        }
        let gx0 = self.ox.min(nx0);
        let gy0 = self.oy.min(ny0);
        let gx1 = (self.ox + self.w as i32).max(nx1);
        let gy1 = (self.oy + self.h as i32).max(ny1);
        if gx0 == self.ox
            && gy0 == self.oy
            && gx1 == self.ox + self.w as i32
            && gy1 == self.oy + self.h as i32
        {
            return;
        }
        // Tient dans la capacité ? Région logique étendue, zéro recopie
        // (l'indexation utilise déjà `ox`/`oy` + stride de capacité).
        if gx0 >= self.ox
            && gy0 >= self.oy
            && gx1 <= self.ox + self.cap_w as i32
            && gy1 <= self.oy + self.cap_h as i32
        {
            self.w = (gx1 - gx0) as u32;
            self.h = (gy1 - gy0) as u32;
            return;
        }
        // Réallocation géométrique : l'amorti rend la recopie rare (un
        // balayage monotone ne réalloue qu'en O(log)).
        let grow = |need: i32| ((need as f32 * 1.5).ceil() as i32).max(need).max(64);
        let gw = grow(gx1 - gx0) as u32;
        let gh = grow(gy1 - gy0) as u32;
        let mut mask = vec![0u8; gw as usize * gh as usize];
        let mut image = vec![0u8; gw as usize * gh as usize * 4];
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                let si = (y as usize) * self.cap_w as usize + x as usize;
                let dx = self.ox + x - gx0;
                let dy = self.oy + y - gy0;
                let di = (dy as usize) * gw as usize + dx as usize;
                mask[di] = self.mask[si];
                image[di * 4..di * 4 + 4].copy_from_slice(&self.image[si * 4..si * 4 + 4]);
            }
        }
        self.ox = gx0;
        self.oy = gy0;
        self.w = (gx1 - gx0) as u32;
        self.h = (gy1 - gy0) as u32;
        self.cap_w = gw;
        self.cap_h = gh;
        self.mask = mask;
        self.image = image;
    }

    /// Rend la frame transitoire (région logique entière, clone borné au
    /// trait). `None` si vide. Taguée de la génération courante (cf.
    /// `is_current`). Ne touche ni au document ni au worker.
    pub fn render(&mut self) -> Option<OverlayFrame> {
        self.metrics.submissions += 1;
        if self.is_empty() {
            return None;
        }
        let t = std::time::Instant::now();
        // Export contigu de la région logique (le stockage suit le
        // stride de capacité) : le padding alloué ne voyage jamais.
        let (w, h) = (self.w as usize, self.h as usize);
        let stride = self.cap_w as usize;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = (y * stride) * 4;
            let dst = (y * w) * 4;
            rgba[dst..dst + w * 4].copy_from_slice(&self.image[src..src + w * 4]);
        }
        let frame = OverlayFrame {
            generation: self.generation,
            x: self.ox,
            y: self.oy,
            width: self.w,
            height: self.h,
            rgba,
        };
        self.metrics.last_render_us = t.elapsed().as_micros();
        self.metrics.completions += 1;
        self.dirty = false;
        Some(frame)
    }

    /// Vrai si du nouveau contenu attend une présentation.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty && !self.is_empty()
    }

    /// Géométrie de la frame courante `(x, y, w, h, génération)` SANS
    /// cloner les pixels : permet à l'app de replacer sa texture en
    /// cache à chaque frame (zoom/pan state-only) sans re-rendre.
    /// `None` si vide (rien à afficher).
    #[must_use]
    pub fn frame_geom(&self) -> Option<(i32, i32, u32, u32, u64)> {
        if self.is_empty() {
            None
        } else {
            Some((self.ox, self.oy, self.w, self.h, self.generation))
        }
    }
}

impl Default for InteractionOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// L'overlay est-il présentable pour cette transform ? Placements
/// axe-alignés uniquement (identité, translation, échelle) : rotation et
/// cisaillement exigeraient une rasterisation orientée côté présentation
/// (porte d'extension — le moteur reste exact dans tous les cas, et le
/// commit ne dépend jamais de ce gate).
#[must_use]
pub fn overlay_displayable(t: &Transform2D) -> bool {
    if t.has_skew() {
        return false;
    }
    // Même epsilon que le chemin de rendu (rotations libres arrondies
    // silencieusement = erreur visuelle).
    let rot = t.rotation_deg.rem_euclid(360.0);
    !(0.01..=359.99).contains(&rot)
}

/// Placement écran d'une frame overlay (maths pures, testées) : l'overlay
/// (doc px, origine incluse) suit exactement la même mise à l'échelle que
/// le preview, ancré au coin du dest preview.
///
/// * `dest_min` : coin écran du rect preview dessiné ;
/// * `screen_per_doc` : px écran par px document (même facteur que le preview) ;
/// * `ox/oy/w/h` : région overlay en px document.
///
/// Retourne `(x, y, w, h)` écran.
#[must_use]
pub fn overlay_screen_rect(
    dest_min_x: f32,
    dest_min_y: f32,
    screen_per_doc: f32,
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
) -> (f32, f32, f32, f32) {
    (
        dest_min_x + ox as f32 * screen_per_doc,
        dest_min_y + oy as f32 * screen_per_doc,
        w as f32 * screen_per_doc,
        h as f32 * screen_per_doc,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::paint_stroke_rgba;

    fn brush_paint() -> BrushParams {
        BrushParams {
            radius: 4.0,
            color: [255, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        }
    }

    /// Blit source-over d'une frame overlay sur un tampon doc (miroir de la
    /// composition d'affichage : stroke-only par-dessus le composite).
    fn blit_over(dst: &mut [u8], dst_w: u32, frame: &OverlayFrame) {
        for y in 0..frame.height {
            for x in 0..frame.width {
                let si = ((y as usize) * frame.width as usize + x as usize) * 4;
                let dx = frame.x + x as i32;
                let dy = frame.y + y as i32;
                if dx < 0 || dy < 0 {
                    continue;
                }
                let di = ((dy as usize) * dst_w as usize + dx as usize) * 4;
                if di + 3 >= dst.len() || si + 3 >= frame.rgba.len() {
                    continue;
                }
                let sa = frame.rgba[si + 3] as f32 / 255.0;
                if sa <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    let v = frame.rgba[si + c] as f32 * sa + dst[di + c] as f32 * (1.0 - sa);
                    dst[di + c] = v.round().clamp(0.0, 255.0) as u8;
                }
                let da = dst[di + 3] as f32 / 255.0;
                dst[di + 3] = ((sa + da * (1.0 - sa)) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    #[test]
    fn feedback_non_vide_avant_commit() {
        // §15 : MouseDown + MouseMove ⇒ overlay non vide AVANT tout MouseUp
        // (aucun worker, aucun document touché ici — pur moteur).
        let mut overlay = InteractionOverlay::new();
        assert!(overlay.is_empty());
        overlay.begin_stroke(brush_paint());
        assert!(overlay.is_empty(), "rien tamponné tant qu'aucun point");
        assert!(overlay.add_points(&[(10.0, 10.0), (20.0, 20.0)]));
        let frame = overlay.render().expect("overlay non vide avant commit");
        assert!(!frame.rgba.iter().all(|&v| v == 0), "pixels visibles");
        assert!(!overlay.is_dirty(), "render consomme le dirty");
        assert_eq!(overlay.metrics().points_received, 2);
    }

    #[test]
    fn morceaux_equivalent_appel_unique() {
        // §5/§15 : 10 points en 1 fois == en 3 morceaux (continuité par
        // dernier point, union idempotente) — y compris avec chevauchement.
        let points: Vec<(f32, f32)> = (0..10).map(|i| (8.0 + i as f32 * 3.0, 30.0)).collect();
        let brush = brush_paint();
        let mut whole = InteractionOverlay::new();
        whole.begin_stroke(brush);
        whole.add_points(&points);
        let whole = whole.render().expect("frame");
        let mut chunked = InteractionOverlay::new();
        chunked.begin_stroke(brush);
        chunked.add_points(&points[0..4]);
        chunked.add_points(&points[3..7]);
        chunked.add_points(&points[5..10]);
        // Chevauchement volontaire ([3..7] puis [5..10]) : idempotent.
        chunked.add_points(&points[4..6]);
        let chunked = chunked.render().expect("frame");
        assert_eq!(
            (whole.x, whole.y, whole.width, whole.height),
            (chunked.x, chunked.y, chunked.width, chunked.height)
        );
        assert_eq!(whole.rgba, chunked.rgba, "morceaux == unique");
    }

    #[test]
    fn transient_proche_du_commit_avec_lissage() {
        // §5 : overlay (transitoire) + commit permanent == référence, à la
        // jonction de paquets près. Les deux passent par le MÊME
        // `resample_stroke` que le worker (`PaintStroke`) : le transitoire
        // montre les mêmes courbes que le commit. Seule différence
        // structurelle : le Chaikin global élimine les points intérieurs
        // (coins coupés) tandis que le découpage par paquets ancre chaque
        // jonction (disque tamponné au point brut). L'écart est donc borné
        // aux disques (rayon du pinceau) autour des points intérieurs —
        // invisible, et remplacé par le commit exact au `MouseUp`.
        use crate::paint::resample_stroke;
        for (brush, w, h) in [
            (brush_paint(), 64u32, 64u32),
            (
                BrushParams {
                    radius: 6.0,
                    color: [0, 255, 0],
                    opacity: 0.5,
                    mode: StrokeMode::Paint,
                },
                96u32,
                64u32,
            ),
        ] {
            let points: Vec<(f32, f32)> =
                vec![(10.0, 10.0), (30.0, 25.0), (50.0, 10.0), (55.0, 40.0)];
            // Référence : commit direct sur fond opaque (resample inclus,
            // comme le bras `PaintStroke` du worker).
            let mut base = vec![0u8; w as usize * h as usize * 4];
            for px in base.chunks_exact_mut(4) {
                px[0] = 200;
                px[1] = 40;
                px[2] = 40;
                px[3] = 255;
            }
            let mut reference = base.clone();
            let smooth = resample_stroke(&points, brush.radius.max(2.0));
            paint_stroke_rgba(&mut reference, w, h, &smooth, &brush);
            // Overlay morcelé (2 appels, jonction en `points[1]`).
            let mut overlay = InteractionOverlay::new();
            overlay.begin_stroke(brush);
            overlay.add_points(&points[0..2]);
            overlay.add_points(&points[2..4]);
            let frame = overlay.render().expect("frame");
            // Recomposite overlay sur la base (même opération que l'affichage
            // transitoire) et compare à la référence.
            let mut shown = base.clone();
            blit_over(&mut shown, w, &frame);
            let mut ndiff = 0usize;
            for (i, (a, b)) in shown.iter().zip(reference.iter()).enumerate() {
                if a == b {
                    continue;
                }
                let px = i / 4;
                let (x, y) = (px % w as usize, px / w as usize);
                let near_junction = points[1..points.len() - 1].iter().any(|&(jx, jy)| {
                    let dx = x as f32 - jx;
                    let dy = y as f32 - jy;
                    (dx * dx + dy * dy).sqrt() <= brush.radius + 1.0
                });
                assert!(
                    near_junction,
                    "pixel ({x},{y}) loin de toute jonction : divergence non bornée"
                );
                ndiff += 1;
            }
            let bound =
                (std::f32::consts::PI * (brush.radius + 2.0).powi(2) * (points.len() - 2) as f32)
                    .ceil() as usize;
            assert!(
                ndiff <= bound,
                "écart {ndiff} > aire des disques de jonction ({bound})"
            );
        }
    }

    #[test]
    fn cancel_ne_touche_rien() {
        // §15 : cancel ⇒ overlay vide, aucune autre trace (le document n'est
        // jamais touché par construction — ici : état interne réarmé).
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(10.0, 10.0), (40.0, 40.0)]);
        assert!(!overlay.is_empty());
        let generation = overlay.generation();
        overlay.clear();
        assert!(overlay.is_empty());
        assert!(overlay.render().is_none());
        assert_eq!(
            overlay.generation(),
            generation,
            "clear ne touche pas la génération"
        );
        assert!(!overlay.is_dirty());
    }

    #[test]
    fn generation_rejette_perime() {
        // §14 : complétion A arrivée après B ⇒ A jetée.
        let mut overlay = InteractionOverlay::new();
        let generation_a = overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(5.0, 5.0)]);
        let frame_a = overlay.render().expect("frame A");
        assert!(overlay.is_current(frame_a.generation));
        assert_eq!(frame_a.generation, generation_a);
        let generation_b = overlay.begin_stroke(brush_paint());
        assert_ne!(generation_a, generation_b);
        assert!(!overlay.is_current(frame_a.generation), "A périmée après B");
        // Nouvelle session repart vide (pas de résidu).
        assert!(overlay.is_empty());
    }

    #[test]
    fn gate_display_transforms() {
        // §11 : identité/translation/échelle OK ; rotation/skew refusés côté
        // présentation (moteur exact quand même).
        use crate::document::Transform2D;
        assert!(overlay_displayable(&Transform2D::default()));
        assert!(overlay_displayable(&Transform2D {
            offset_x: 30.0,
            offset_y: -12.0,
            ..Transform2D::default()
        }));
        assert!(overlay_displayable(&Transform2D {
            scale_x: 2.0,
            scale_y: 0.5,
            ..Transform2D::default()
        }));
        assert!(!overlay_displayable(&Transform2D {
            rotation_deg: 90.0,
            ..Transform2D::default()
        }));
        assert!(!overlay_displayable(&Transform2D {
            rotation_deg: 0.02,
            ..Transform2D::default()
        }));
        assert!(!overlay_displayable(&Transform2D {
            skew_x: 5.0,
            ..Transform2D::default()
        }));
    }

    #[test]
    fn placement_ecran_suit_preview() {
        // L'overlay suit la même échelle que le preview, ancré au coin.
        assert_eq!(
            overlay_screen_rect(100.0, 50.0, 2.0, 10, 20, 30, 40),
            (120.0, 90.0, 60.0, 80.0)
        );
        assert_eq!(
            overlay_screen_rect(0.0, 0.0, 0.5, 0, 0, 64, 64),
            (0.0, 0.0, 32.0, 32.0)
        );
    }

    #[test]
    fn points_degeneres_coalesces() {
        // §8/§13 : points dupliqués et non finis comptés, sans effet.
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(10.0, 10.0), (10.0, 10.0), (f32::NAN, 0.0), (20.0, 20.0)]);
        let m = overlay.metrics();
        assert_eq!(m.points_received, 4);
        assert_eq!(m.points_coalesced, 2, "doublon + NaN");
        assert!(!overlay.is_empty());
        // Pinceau invalide : no-op total.
        let mut bad = InteractionOverlay::new();
        bad.begin_stroke(BrushParams {
            radius: 0.0,
            color: [0, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        assert!(!bad.add_points(&[(5.0, 5.0)]));
        assert!(bad.is_empty());
    }

    #[test]
    fn frame_geom_reexpose_sans_recloner() {
        // Support du redraw persistant côté app : la géométrie reste
        // disponible après `render()` (qui a consommé le dirty) sans
        // toucher aux pixels, et disparaît au `clear()`.
        let mut overlay = InteractionOverlay::new();
        assert!(overlay.frame_geom().is_none());
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&[(10.0, 10.0), (20.0, 20.0)]);
        let frame = overlay.render().expect("frame");
        let geom = overlay.frame_geom().expect("géométrie conservée");
        assert_eq!(
            (geom.0, geom.1, geom.2, geom.3),
            (frame.x, frame.y, frame.width, frame.height)
        );
        assert_eq!(geom.4, frame.generation);
        assert!(overlay.is_current(geom.4));
        overlay.clear();
        assert!(overlay.frame_geom().is_none());
    }

    #[test]
    fn region_bornee_au_trait_grands_documents() {
        // 1920x1080, 2048², 4096² : un PETIT trait reste petit — la
        // région overlay est en O(stroke), jamais en O(document).
        for (doc_w, doc_h) in [(1920.0, 1080.0), (2048.0, 2048.0), (4096.0, 4096.0)] {
            let points = ligne(100.0, 100.0, 300.0, 250.0, 16);
            let mut overlay = InteractionOverlay::new();
            overlay.begin_stroke(BrushParams {
                radius: 8.0,
                color: [200, 30, 30],
                opacity: 1.0,
                mode: StrokeMode::Paint,
            });
            for chunk in points.chunks(8) {
                overlay.add_points(chunk);
            }
            let frame = overlay.render().expect("frame non vide");
            let area = u64::from(frame.width) * u64::from(frame.height);
            let doc_area = (doc_w as u64) * (doc_h as u64);
            assert!(
                area * 20 < doc_area,
                "région {w}x{h} bornée au trait (< 5 % du document {doc_w}x{doc_h})",
                w = frame.width,
                h = frame.height,
            );
            assert!(
                frame.x >= 0 && frame.y >= 0,
                "région dans l'espace document {doc_w}x{doc_h}"
            );
            // Mémoire bornée : masque + image de la région seule.
            assert_eq!(
                frame.rgba.len(),
                frame.width as usize * frame.height as usize * 4
            );
        }
    }
    /// Mesure un geste : `add_points` incrémental (par paquets de 8, comme
    /// un drag) + `render()` final. Retourne le mur total (µs) et la taille
    /// de la région overlay. JAMAIS d'assert sur le mur (matériel variable).
    fn mesure_geste(points: &[(f32, f32)], radius: f32) -> (u128, u32, u32) {
        use std::time::Instant;
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(BrushParams {
            radius,
            color: [200, 30, 30],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        let start = Instant::now();
        for chunk in points.chunks(8) {
            overlay.add_points(chunk);
        }
        let frame = overlay.render().expect("frame non vide");
        (start.elapsed().as_micros(), frame.width, frame.height)
    }

    fn ligne(x0: f32, y0: f32, x1: f32, y1: f32, n: usize) -> Vec<(f32, f32)> {
        (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1).max(1) as f32;
                (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)
            })
            .collect()
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_geste_court_1920() {
        // Petit pinceau, diagonale 1920x1080 : le cas courant.
        let points = ligne(100.0, 100.0, 1800.0, 950.0, 64);
        let (us, w, h) = mesure_geste(&points, 8.0);
        eprintln!("geste-court-1920 : {us} µs pour 64 pts r=8 (région {w}x{h})");
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_geste_long_gros_pinceau() {
        // Gros pinceau, traversée 2048² : borne haute de la région.
        let points = ligne(0.0, 0.0, 2048.0, 2048.0, 512);
        let (us, w, h) = mesure_geste(&points, 64.0);
        eprintln!("geste-long-2048 : {us} µs pour 512 pts r=64 (région {w}x{h})");
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_frame_drag_courant() {
        // Vrai régime interactif : 8 pts + render PAR FRAME (10 frames),
        // mur moyen comparé au budget 16 ms. L'overlay est préchauffé
        // (région déjà allouée — comme en milieu de geste réel).
        use std::time::Instant;
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(BrushParams {
            radius: 8.0,
            color: [200, 30, 30],
            opacity: 1.0,
            mode: StrokeMode::Paint,
        });
        overlay.add_points(&ligne(100.0, 100.0, 300.0, 250.0, 16));
        let _ = overlay.render();
        let start = Instant::now();
        for frame in 0..10 {
            let t0 = 320.0 + frame as f32 * 24.0;
            overlay.add_points(&ligne(t0, 260.0, t0 + 24.0, 280.0, 8));
            let rendered = overlay.render().expect("frame");
            assert!(overlay.is_current(rendered.generation));
        }
        let mean_us = start.elapsed().as_micros() / 10;
        eprintln!("frame-drag : {mean_us} µs/frame (8 pts + render, r=8)");
    }

    #[test]
    #[ignore = "mesure interactive (mur indicatif, rapport 6G.3P §13)"]
    fn perf_geste_coords_4096() {
        // Coordonnées espace 4096² (placement exact à l'échelle, §10).
        let points = ligne(10.0, 10.0, 4086.0, 4086.0, 256);
        let (us, w, h) = mesure_geste(&points, 12.0);
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(brush_paint());
        overlay.add_points(&points);
        let frame = overlay.render().expect("frame");
        assert!(
            frame.x >= 0 && frame.y >= 0,
            "région dans l'espace 4096², pas de débordement"
        );
        eprintln!("geste-coords-4096 : {us} µs pour 256 pts r=12 (région {w}x{h})");
    }

    #[test]
    fn gomme_voile_indicatif() {
        // Erase : voile blanc sur la couverture (indicateur, commit exact par
        // ailleurs) — jamais de pixel noir inventé.
        let mut overlay = InteractionOverlay::new();
        overlay.begin_stroke(BrushParams {
            radius: 4.0,
            color: [0, 0, 0],
            opacity: 1.0,
            mode: StrokeMode::Erase,
        });
        overlay.add_points(&[(10.0, 10.0), (20.0, 20.0)]);
        let frame = overlay.render().expect("frame");
        assert!(!frame.rgba.iter().all(|&v| v == 0), "voile visible");
        for px in frame.rgba.chunks_exact(4) {
            if px[3] > 0 {
                assert_eq!(&px[0..3], &[255, 255, 255], "que du blanc + alpha");
            }
        }
    }
}
