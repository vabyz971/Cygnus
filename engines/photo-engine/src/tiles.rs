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

//! Point d'intégration tuiles : région sale d'un geste, sans toucher au renderer.
//!
//! [`stroke_dirty_region`] convertit un trait (points + rayon, espace image)
//! en [`TileRegion`] prête pour `TileGrid::invalidate` : bbox du geste,
//! élargie du rayon de brosse PUIS du [`Padding`] d'effet (voisinage des
//! filtres actifs — flou…).
//!
//! Pipeline progressif visé (renderer actuel inchangé : il composite
//! toujours plein cadre ; les tuiles ordonnent et observent) :
//!
//! ```text
//! geste → stroke_dirty_region → grid.invalidate → DirtyTiles::mark_range
//!     → TileScheduler::plan_into → exécution progressive → renderer existant
//! ```
//!
//! Fonction PURE (aucun état moteur lu ou muté) : le remplacement du
//! renderer viendra plus tard, la géométrie sale est disponible dès
//! maintenant.
//!
//! RÈGLE DE COORDONNÉES (Phase 6B) : toute région envoyée à `TileGrid` /
//! `DirtyTiles` / `TileScheduler` / `tiles_for_rect` est en DOCUMENT SPACE.
//! Les points d'un trait de peinture restent en LAYER SPACE : la conversion
//! explicite passe par la [`Transform2D`] du calque
//! ([`stroke_footprint_in_document`], [`plan_stroke_tiles_layer_space`]).
//! La rastérisation (`paint_stroke_rgba`, espace calque) est inchangée.

use tiles::{DirtyTiles, TileGrid, TileRegion, TileScheduler, Viewport};

pub use tiles::Padding;

use crate::document::{FilterLayer, Transform2D};

/// Région sale d'un trait de pinceau/gomme (`None` si inexploitable :
/// aucun point, rayon non fini ou nul, coordonnées non finies).
///
/// `points` en pixels image, `radius` en pixels, `pad` = marge d'effet
/// (ex. [`Padding::for_blur_radius`] du filtre actif — 0 sinon).
/// La région peut dépasser la surface : le clipping appartient à la grille.
#[must_use]
pub fn stroke_dirty_region(points: &[(f32, f32)], radius: f32, pad: Padding) -> Option<TileRegion> {
    if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    // Couverture conservative : plancher en min, plafond en max (+ rayon).
    let region = TileRegion::new(
        (x0 - radius).floor() as i32,
        (y0 - radius).floor() as i32,
        ((x1 + radius).ceil() - (x0 - radius).floor()).max(1.0) as u32,
        ((y1 + radius).ceil() - (y0 - radius).floor()).max(1.0) as u32,
    );
    Some(region.pad(pad))
}

/// Taille de tuile d'observation (niveau 0 unique, 256 px) : même grain
/// que les tests du socle, assez fin pour un stroke, assez gros pour
/// rester lisible dans les métriques.
const OBSERVE_TILE_PX: u32 = 256;

/// Transform clampée comme le renderer (`prepare_top` / `extents_visit` :
/// échelles 0.05..=8.0) : l'empreinte d'invalidation couvre la même zone
/// que le rendu, jamais moins.
fn clamped_transform(t: &Transform2D) -> Transform2D {
    Transform2D {
        scale_x: t.scale_x.clamp(0.05, 8.0),
        scale_y: t.scale_y.clamp(0.05, 8.0),
        ..*t
    }
}

/// Englobant entier conservateur d'un quad flottant (`floor` en min,
/// `ceil` en max — même convention que les tuiles). `EMPTY` si non fini.
fn quad_to_region(quad: &[(f32, f32); 4]) -> TileRegion {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &(x, y) in quad {
        if !x.is_finite() || !y.is_finite() {
            return TileRegion::EMPTY;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if max_x <= min_x || max_y <= min_y {
        return TileRegion::EMPTY;
    }
    TileRegion::new(
        min_x.floor() as i32,
        min_y.floor() as i32,
        (max_x.ceil() - min_x.floor()).max(1.0) as u32,
        (max_y.ceil() - min_y.floor()).max(1.0) as u32,
    )
}

/// Empreinte d'un calque entier en DOCUMENT SPACE : les 4 coins de l'image
/// (`w0 × h0`, espace calque) via [`Transform2D::doc_corners`] — exactement
/// le même calcul que `extents_visit` côté renderer — puis englobant.
///
/// `EMPTY` si dimensions nulles (aucun pixel à invalider).
#[must_use]
pub fn layer_footprint_in_document(transform: &Transform2D, w0: u32, h0: u32) -> TileRegion {
    if w0 == 0 || h0 == 0 {
        return TileRegion::EMPTY;
    }
    let clamped = clamped_transform(transform);
    quad_to_region(&clamped.doc_corners(w0 as f32, h0 as f32))
}

/// Empreinte d'un trait (LAYER SPACE) en DOCUMENT SPACE : bbox du trait
/// élargie du rayon **dans l'espace calque**, puis coins transformés par la
/// `Transform2D` (translation, rotation, scale, skew — pas de simple ajout
/// d'offset) et englobant conservateur.
///
/// `None` si inexploitable (aucun point, rayon non fini ou nul, coordonnée
/// non finie). `w0`/`h0` = dimensions de l'image du calque (centre de la
/// transform, comme `local_to_doc`). Sans halo : appliquer ensuite
/// [`expand_region_by_halo`].
#[must_use]
pub fn stroke_footprint_in_document(
    points: &[(f32, f32)],
    radius: f32,
    transform: &Transform2D,
    w0: u32,
    h0: u32,
) -> Option<TileRegion> {
    if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    // Rayon conservé AVANT conversion : la boîte élargie transformée couvre
    // le vrai trait dilaté (T(vrai) ⊆ T(boîte) ⊆ englobant).
    let clamped = clamped_transform(transform);
    let (w0f, h0f) = (w0 as f32, h0 as f32);
    let quad = [
        clamped.local_to_doc(w0f, h0f, x0 - radius, y0 - radius),
        clamped.local_to_doc(w0f, h0f, x1 + radius, y0 - radius),
        clamped.local_to_doc(w0f, h0f, x1 + radius, y1 + radius),
        clamped.local_to_doc(w0f, h0f, x0 - radius, y1 + radius),
    ];
    let region = quad_to_region(&quad);
    if region.is_empty() {
        return None;
    }
    Some(region)
}

/// Expansion d'une région DOCUMENT SPACE par un halo (voisinage d'effet).
/// Pur `pad` : le clipping au document appartient à la grille
/// (`TileGrid::invalidate`), pas ici.
#[must_use]
pub fn expand_region_by_halo(region: &TileRegion, halo: Padding) -> TileRegion {
    region.pad(halo)
}

/// Région sale d'un déplacement géométrique : `old ∪ new` (les deux
/// empreintes, puis halo). Toute mutation qui déplace la zone produite
/// (transform, resize, rotation) doit invalider l'union, jamais la seule
/// nouvelle position.
#[must_use]
pub fn layer_move_dirty_region(
    old_transform: &Transform2D,
    new_transform: &Transform2D,
    w0: u32,
    h0: u32,
    halo: Padding,
) -> TileRegion {
    expand_region_by_halo(
        &layer_footprint_in_document(old_transform, w0, h0).union(layer_footprint_in_document(
            new_transform,
            w0,
            h0,
        )),
        halo,
    )
}

/// Halo conservateur d'une chaîne de sous-calques de filtres : somme des
/// rayons des flous ACTIFS (`Blur(r1) → Blur(r2)` ⇒ `r1 + r2`, chaîne
/// séquentielle). Seul l'effet `blur` (param `radius` flottant) est voisiné ;
/// tout le reste (et toute chaîne vide) ⇒ halo nul.
///
/// Lecture locale des structures existantes : aucun refactor des effets,
/// aucun nouveau système — `Padding::for_blur_radius` par rayon.
#[must_use]
pub fn blur_halo_for_filters(filters: &[FilterLayer]) -> Padding {
    let mut total = 0.0f32;
    for f in filters {
        if !f.enabled || f.type_id != "blur" {
            continue;
        }
        if let Some(datatypes::ParamValue::Float(r)) = f.params.get("radius")
            && r.is_finite()
            && *r > 0.0
        {
            total += *r;
        }
    }
    Padding::for_blur_radius(total)
}

/// Plan de tuiles observable pour un stroke : purs compteurs + région
/// englobante, aucun pixel touché, aucun rendu piloté. Le worker
/// enregistre ces compteurs dans ses métriques ; le renderer actuel
/// tourne inchangé à côté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeTilePlan {
    /// Aire de la région sale en pixels (0 si geste inexploitable).
    pub dirty_region_px: u64,
    /// Tuiles marquées sales au niveau 0 (après clipping document).
    pub dirty_tiles: u64,
    /// Tuiles retenues par le scheduler (budget ouvert : == dirty_tiles).
    pub scheduled_tiles: u64,
    /// Région englobante des tuiles sales, en coordonnées DOCUMENT,
    /// clampée au document (`TileRegion::EMPTY` si aucune tuile).
    /// C'est elle — et rien d'autre — qu'un rendu régional consomme.
    pub bounds: TileRegion,
}

impl Default for StrokeTilePlan {
    fn default() -> Self {
        Self {
            dirty_region_px: 0,
            dirty_tiles: 0,
            scheduled_tiles: 0,
            bounds: TileRegion::EMPTY,
        }
    }
}

/// Calcule le plan observable d'un stroke : région → grille document →
/// dirty → scheduler. Pur et déterministe (mêmes entrées ⇒ même plan).
/// `doc_w`/`doc_h` = dimensions du document (espace du composite) ;
/// valeurs nulles ramenées à 1 px. Jamais de `None` : un geste
/// inexploitable donne un plan nul (zéros), pas d'erreur.
#[must_use]
pub fn plan_stroke_tiles(
    points: &[(f32, f32)],
    radius: f32,
    doc_w: u32,
    doc_h: u32,
) -> StrokeTilePlan {
    let mut plan = StrokeTilePlan::default();
    let Some(region) = stroke_dirty_region(points, radius, Padding::ZERO) else {
        return plan;
    };
    plan_region_tiles(&mut plan, &region, doc_w, doc_h);
    plan
}

/// Plan d'un stroke en LAYER SPACE avec transform de calque : empreinte
/// document ([`stroke_footprint_in_document`]) élargie du `halo` (espace
/// document, ex. [`blur_halo_for_filters`]), puis `tiles_for_rect` /
/// `DirtyTiles` via le même plan que [`plan_stroke_tiles`].
///
/// `layer_w`/`layer_h` = dimensions de l'image du calque. Pur et
/// déterministe. Transform identité ⇒ région équivalente à
/// [`plan_stroke_tiles`] (test `identity` ci-dessous).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn plan_stroke_tiles_layer_space(
    points: &[(f32, f32)],
    radius: f32,
    transform: &Transform2D,
    layer_w: u32,
    layer_h: u32,
    doc_w: u32,
    doc_h: u32,
    halo: Padding,
) -> StrokeTilePlan {
    let mut plan = StrokeTilePlan::default();
    let Some(footprint) = stroke_footprint_in_document(points, radius, transform, layer_w, layer_h)
    else {
        return plan;
    };
    let region = expand_region_by_halo(&footprint, halo);
    plan_region_tiles(&mut plan, &region, doc_w, doc_h);
    plan
}

/// Cœur partagé des plans : région DOCUMENT SPACE → grille → dirty →
/// scheduler. `region` peut dépasser la surface : le clipping appartient à
/// la grille (`invalidate` + `to_region` déjà clampés au document).
fn plan_region_tiles(plan: &mut StrokeTilePlan, region: &TileRegion, doc_w: u32, doc_h: u32) {
    plan.dirty_region_px = region.area();
    let (doc_w, doc_h) = (doc_w.max(1), doc_h.max(1));
    let grid = TileGrid::new(OBSERVE_TILE_PX, doc_w, doc_h, 1);
    let range = grid.invalidate(region, 0, Padding::ZERO);
    let mut dirty = DirtyTiles::new(&grid);
    plan.dirty_tiles = dirty.mark_range(range);
    // Région englobante des tuiles sales (union déjà clampée au document
    // par `to_region`) : entrée du rendu régional, sans liste de tuiles.
    plan.bounds = range.to_region(&grid);
    // Viewport d'observation : document entier visible, focus au centre
    // du geste — déterministe, sans état UI à threader jusqu'ici.
    let visible = TileRegion::new(0, 0, doc_w, doc_h);
    let focus = (
        region
            .x
            .saturating_add((region.width / 2).min(i32::MAX as u32) as i32),
        region
            .y
            .saturating_add((region.height / 2).min(i32::MAX as u32) as i32),
    );
    let view = Viewport::new(visible, focus);
    plan.scheduled_tiles = TileScheduler.plan(&dirty, &grid, &view, None).len() as u64;
}

/// Politique de fallback FULL_FRAME ↔ REGIONAL (vertical slice) :
/// volontairement triviale — aucun seuil heuristique complexe.
///
/// * plan vide / région invalide (`dirty_px == 0`) → pleine image ;
/// * région couvrant tout ou plus que le cadre (`dirty_px >= scope_px`)
///   → pleine image (aucun gain à découper) ;
/// * sinon → le renderer régional est supporté, l'utiliser.
///
/// `scope_px` = pixels de l'accumulateur pleine cadre (cf.
/// `CompositeStats::scope_px`). Les cas GPU incompatible / renderer non
/// supporté n'existent pas ici : le régional réutilise exactement la même
/// passe CPU (mêmes apparences pleine taille, aucun readback ajouté) —
/// documenté dans `Document::composite_region_with`.
#[must_use]
pub fn use_regional_render(dirty_px: u64, scope_px: u64) -> bool {
    dirty_px > 0 && dirty_px < scope_px
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiles::{TileGrid, TileRange};

    #[test]
    fn trait_couvre_points_plus_rayon() {
        let region = stroke_dirty_region(&[(10.0, 10.0), (20.0, 30.0)], 5.0, Padding::ZERO)
            .expect("geste valide");
        assert_eq!(region, TileRegion::new(5, 5, 20, 30));
    }

    #[test]
    fn padding_elargit_pour_le_voisinage() {
        let base = stroke_dirty_region(&[(100.0, 100.0)], 4.0, Padding::ZERO).expect("ok");
        let padded = stroke_dirty_region(&[(100.0, 100.0)], 4.0, Padding::for_blur_radius(20.0))
            .expect("ok");
        assert_eq!(base, TileRegion::new(96, 96, 8, 8));
        assert_eq!(padded, TileRegion::new(76, 76, 48, 48));
    }

    #[test]
    fn pipeline_geste_vers_tuiles() {
        // Chaîne complète : geste → région → grille → tuiles → dirty set.
        let grid = TileGrid::new(256, 1920, 1080, 1);
        let region =
            stroke_dirty_region(&[(10.0, 10.0), (300.0, 10.0)], 8.0, Padding::ZERO).expect("ok");
        let range: TileRange = grid.invalidate(&region, 0, Padding::ZERO);
        assert_eq!(range.count(), 2, "le trait chevauche 2 tuiles");
        let mut dirty = tiles::DirtyTiles::new(&grid);
        assert_eq!(dirty.mark_range(range), 2);
    }

    #[test]
    fn gestes_invalides_refuses() {
        assert!(stroke_dirty_region(&[], 5.0, Padding::ZERO).is_none());
        assert!(stroke_dirty_region(&[(0.0, 0.0)], 0.0, Padding::ZERO).is_none());
        assert!(stroke_dirty_region(&[(0.0, 0.0)], f32::NAN, Padding::ZERO).is_none());
        assert!(stroke_dirty_region(&[(f32::INFINITY, 0.0)], 5.0, Padding::ZERO).is_none());
    }

    #[test]
    fn plan_petit_stroke_une_tuile() {
        // Région (5,5,20,30) sur doc 1920x1080 : entièrement dans la tuile (0,0).
        let plan = plan_stroke_tiles(&[(10.0, 10.0), (20.0, 30.0)], 5.0, 1920, 1080);
        assert_eq!(plan.dirty_region_px, 20 * 30);
        assert_eq!(plan.dirty_tiles, 1);
        assert_eq!(plan.scheduled_tiles, 1);
    }

    #[test]
    fn plan_stroke_large_plusieurs_tuiles() {
        // x ∈ [2,308] → tuiles 0 et 1 ; y ∈ [2,18] → tuile 0 : 2 tuiles.
        let plan = plan_stroke_tiles(&[(10.0, 10.0), (300.0, 10.0)], 8.0, 1920, 1080);
        assert_eq!(plan.dirty_tiles, 2);
        assert_eq!(plan.scheduled_tiles, plan.dirty_tiles);
    }

    #[test]
    fn plan_hors_document_aucune_tuile() {
        // Région réelle mais entièrement hors surface 8x8 : clipping grille.
        let plan = plan_stroke_tiles(&[(5000.0, 5000.0)], 2.0, 8, 8);
        assert!(plan.dirty_region_px > 0, "région calculée");
        assert_eq!(plan.dirty_tiles, 0, "clippé, pas de tuile invalide");
        assert_eq!(plan.scheduled_tiles, 0);
    }

    #[test]
    fn plan_geste_invalide_nul() {
        assert_eq!(
            plan_stroke_tiles(&[], 5.0, 1920, 1080),
            StrokeTilePlan::default()
        );
        assert_eq!(
            plan_stroke_tiles(&[(0.0, 0.0)], 0.0, 1920, 1080),
            StrokeTilePlan::default()
        );
    }

    #[test]
    fn plan_deterministe() {
        let points = [(10.0, 10.0), (300.0, 10.0)];
        assert_eq!(
            plan_stroke_tiles(&points, 8.0, 1920, 1080),
            plan_stroke_tiles(&points, 8.0, 1920, 1080)
        );
    }

    #[test]
    fn plan_bounds_englobe_exactement_les_tuiles() {
        // Tuiles (4,5),(5,5),(4,6),(5,6) à 256 px sur doc 2048² :
        // x ∈ [1090,1460) → tuiles 4,5 ; y ∈ [1340,1710) → tuiles 5,6.
        let plan = plan_stroke_tiles(&[(1100.0, 1350.0), (1450.0, 1700.0)], 10.0, 2048, 2048);
        assert_eq!(plan.dirty_tiles, 4);
        assert_eq!(plan.bounds, TileRegion::new(1024, 1280, 512, 512));
    }

    #[test]
    fn plan_bounds_vide_sans_tuile() {
        let plan = plan_stroke_tiles(&[(5000.0, 5000.0)], 2.0, 8, 8);
        assert_eq!(plan.bounds, TileRegion::EMPTY);
        let nul = plan_stroke_tiles(&[], 5.0, 1920, 1080);
        assert_eq!(nul.bounds, TileRegion::EMPTY);
    }

    #[test]
    fn fallback_regional_trivial() {
        assert!(!use_regional_render(0, 1000), "plan vide → pleine image");
        assert!(
            !use_regional_render(1000, 1000),
            "couverture totale → pleine image"
        );
        assert!(
            !use_regional_render(2000, 1000),
            "débordement → pleine image"
        );
        assert!(use_regional_render(1, 1000), "région partielle → régional");
        assert!(use_regional_render(999, 1000));
    }

    use crate::document::Transform2D;

    fn ident() -> Transform2D {
        Transform2D::default()
    }

    #[test]
    fn identity_equivalent_a_l_ancien_chemin() {
        // Transform identité : même région qu'avant la phase.
        let points = [(100.0f32, 100.0f32)];
        let fp =
            stroke_footprint_in_document(&points, 10.0, &ident(), 512, 512).expect("geste valide");
        assert_eq!(fp, TileRegion::new(90, 90, 20, 20));
        assert_eq!(
            fp,
            stroke_dirty_region(&points, 10.0, Padding::ZERO).expect("ancien chemin")
        );
        let old = plan_stroke_tiles(&points, 10.0, 1920, 1080);
        let new = plan_stroke_tiles_layer_space(
            &points,
            10.0,
            &ident(),
            512,
            512,
            1920,
            1080,
            Padding::ZERO,
        );
        assert_eq!(old, new);
    }

    #[test]
    fn translation_deplace_la_region_document() {
        let t = Transform2D {
            offset_x: 100.0,
            offset_y: 50.0,
            ..ident()
        };
        let fp =
            stroke_footprint_in_document(&[(10.0, 20.0)], 5.0, &t, 512, 512).expect("geste valide");
        // Boîte calque [5,15]×[15,25] décalée de (+100,+50), via les vraies APIs.
        assert_eq!(fp, TileRegion::new(105, 65, 10, 10));
        let expect = TileRegion::new(5, 15, 10, 10);
        let corner = t.local_to_doc(512.0, 512.0, expect.x as f32, expect.y as f32);
        assert_eq!((corner.0, corner.1), (105.0, 65.0));
    }

    #[test]
    fn rotation_90_bbox_reelle() {
        // Calque 100×50, rotation 90° : math exactes x[25,75]×y[-25,75]
        // (coins (0,0)→(75,-25), (100,0)→(75,75), (100,50)→(25,75),
        // (0,50)→(25,-25)). Le cos(90°) flottant (~-4e-8) élargit d'au plus
        // 1 px : on vérifie le contenu exact + une borne de surestimation.
        let t = Transform2D {
            rotation_deg: 90.0,
            ..ident()
        };
        let full = layer_footprint_in_document(&t, 100, 50);
        assert_ne!(full, TileRegion::new(0, 0, 100, 50), "transformée");
        assert!(full.x <= 25 && full.y <= -25, "couvre le min exact");
        assert!(
            full.right() >= 75 && full.bottom() >= 75,
            "couvre le max exact"
        );
        assert!(full.area() <= 52 * 102, "surestimation ≤ 1 px par bord");
        // Stroke asymétrique : l'empreinte n'est PAS la bbox non transformée
        // et couvre la bbox exacte x[65,75]×y[-25,-5].
        let fp = stroke_footprint_in_document(&[(0.0, 0.0), (20.0, 10.0)], 1.0, &t, 100, 50)
            .expect("geste valide");
        assert_ne!(fp, TileRegion::new(-1, -1, 22, 12));
        assert!(fp.x <= 65 && fp.y <= -25);
        assert!(fp.right() >= 75 && fp.bottom() >= -5);
        assert!(fp.area() <= 14 * 24, "surestimation ≤ 1 px par bord");
    }

    #[test]
    fn scale_uniforme_et_non_uniforme_conservateurs() {
        let t2 = Transform2D {
            scale_x: 2.0,
            scale_y: 2.0,
            ..ident()
        };
        // Calque 64×64, point (10,10) r=2 : boîte [8,12]² → doc [16,24]².
        let fp =
            stroke_footprint_in_document(&[(10.0, 10.0)], 2.0, &t2, 64, 64).expect("geste valide");
        assert_eq!(fp, TileRegion::new(16, 16, 8, 8));

        let tnu = Transform2D {
            scale_x: 2.0,
            scale_y: 0.5,
            ..ident()
        };
        let fp_nu =
            stroke_footprint_in_document(&[(10.0, 10.0)], 2.0, &tnu, 64, 64).expect("geste valide");
        // ux∈[-48,-40]→x[16,24] ; uy∈[-12,-10]→y[4,6].
        assert_eq!(fp_nu, TileRegion::new(16, 4, 8, 2));
    }

    #[test]
    fn skew_non_nul_modifie_l_empreinte() {
        let t = Transform2D {
            skew_x: 45.0,
            ..ident()
        };
        let fp =
            stroke_footprint_in_document(&[(10.0, 20.0)], 2.0, &t, 100, 100).expect("geste valide");
        // Boîte non transformée [8,12]×[18,22] ; skew X (tan=1) décale vers la gauche.
        assert_ne!(fp, TileRegion::new(8, 18, 4, 4));
        assert_eq!(fp, TileRegion::new(-24, 18, 8, 4));
    }

    #[test]
    fn old_union_new_couvre_les_deux_positions() {
        let a = ident();
        let b = Transform2D {
            offset_x: 200.0,
            ..ident()
        };
        // Calque 64×64 : [0,64]² ∪ [200,264]×[0,64] = [0,264]×[0,64].
        assert_eq!(
            layer_move_dirty_region(&a, &b, 64, 64, Padding::ZERO),
            TileRegion::new(0, 0, 264, 64)
        );
        // Avec halo 10 : élargi de 10 px partout.
        assert_eq!(
            layer_move_dirty_region(&a, &b, 64, 64, Padding::new(10)),
            TileRegion::new(-10, -10, 284, 84)
        );
    }

    #[test]
    fn halo_elargit_et_clipping_grille() {
        assert_eq!(
            expand_region_by_halo(&TileRegion::new(100, 100, 100, 100), Padding::new(10)),
            TileRegion::new(90, 90, 120, 120)
        );
        assert_eq!(
            expand_region_by_halo(&TileRegion::EMPTY, Padding::new(10)),
            TileRegion::EMPTY
        );
        // Clipping : doc 64×64, tuile 256 ⇒ une seule tuile, bornes clampées.
        let grid = TileGrid::new(256, 64, 64, 1);
        let grown = expand_region_by_halo(&TileRegion::new(0, 0, 64, 64), Padding::new(10));
        let range = grid.invalidate(&grown, 0, Padding::ZERO);
        assert_eq!(range.count(), 1);
        assert_eq!(range.to_region(&grid), TileRegion::new(0, 0, 64, 64));
    }

    fn blur_filter(radius: f32) -> crate::document::FilterLayer {
        let mut f = crate::document::FilterLayer::neutral("blur", Default::default());
        f.params
            .insert("radius".to_string(), datatypes::ParamValue::Float(radius));
        f
    }

    #[test]
    fn halo_blur_somme_sequentielle() {
        assert_eq!(blur_halo_for_filters(&[]), Padding::ZERO);
        assert_eq!(
            blur_halo_for_filters(&[blur_filter(10.0)]),
            Padding::new(10)
        );
        assert_eq!(
            blur_halo_for_filters(&[blur_filter(10.0), blur_filter(20.0)]),
            Padding::new(30)
        );
        // Désactivé ou non-flou : ignoré.
        let mut off = blur_filter(50.0);
        off.enabled = false;
        let mut bc =
            crate::document::FilterLayer::neutral("brightness_contrast", Default::default());
        bc.params
            .insert("brightness".to_string(), datatypes::ParamValue::Float(5.0));
        assert_eq!(blur_halo_for_filters(&[off, bc]), Padding::ZERO);
    }

    #[test]
    fn plan_blur_etend_vers_la_tuile_voisine() {
        // Point (255,100) r=1, doc 1024 : boîte [254,256] ⇒ seule tuile x=0.
        let plain = plan_stroke_tiles_layer_space(
            &[(255.0, 100.0)],
            1.0,
            &ident(),
            1024,
            1024,
            1024,
            1024,
            Padding::ZERO,
        );
        assert_eq!(plain.dirty_tiles, 1);
        // Halo 10 (Blur 10) : [244,266] ⇒ tuiles x=0 et x=1.
        let halo = blur_halo_for_filters(&[blur_filter(10.0)]);
        let grown = plan_stroke_tiles_layer_space(
            &[(255.0, 100.0)],
            1.0,
            &ident(),
            1024,
            1024,
            1024,
            1024,
            halo,
        );
        assert_eq!(grown.dirty_tiles, 2);
        assert!(grown.dirty_region_px > plain.dirty_region_px);
    }

    #[test]
    fn frontiere_tuile_avec_et_sans_halo() {
        let grid = TileGrid::new(256, 1920, 1080, 1);
        // x = 250 → 270 : chevauche les tuiles 0 et 1.
        let range = grid.invalidate(&TileRegion::new(250, 10, 20, 10), 0, Padding::ZERO);
        assert_eq!(range.count(), 2);
        // Région contenue + halo ⇒ voisine sale.
        let tight = grid.invalidate(&TileRegion::new(250, 10, 5, 10), 0, Padding::ZERO);
        assert_eq!(tight.count(), 1);
        let grown = grid.invalidate(&TileRegion::new(250, 10, 5, 10), 0, Padding::new(10));
        assert_eq!(grown.count(), 2);
    }

    #[test]
    fn bornes_document_clampees_aucune_tuile_hors_doc() {
        // Entièrement hors document : aucune tuile, bornes vides.
        let plan = plan_stroke_tiles_layer_space(
            &[(-500.0, -500.0)],
            2.0,
            &ident(),
            64,
            64,
            8,
            8,
            Padding::ZERO,
        );
        assert_eq!(plan.dirty_tiles, 0);
        assert_eq!(plan.bounds, TileRegion::EMPTY);
        // Partiellement hors cadre : clippé à la surface, sans tuile négative.
        let plan = plan_stroke_tiles_layer_space(
            &[(2.0, 2.0)],
            10.0,
            &ident(),
            64,
            64,
            8,
            8,
            Padding::ZERO,
        );
        assert_eq!(plan.dirty_tiles, 1);
        assert_eq!(plan.bounds, TileRegion::new(0, 0, 8, 8));
        // y < 0 et x > largeur : idem via la grille nue.
        let grid = TileGrid::new(256, 100, 100, 1);
        let range = grid.invalidate(&TileRegion::new(-50, -50, 300, 300), 0, Padding::ZERO);
        assert_eq!(range.to_region(&grid), TileRegion::new(0, 0, 100, 100));
    }

    #[test]
    fn integration_transform_paint_tiles() {
        // Document 1024, tuiles 256 ; calque 200×200, translation + rotation 90°.
        let t = Transform2D {
            offset_x: 100.0,
            offset_y: 50.0,
            rotation_deg: 90.0,
            ..ident()
        };
        let plan = plan_stroke_tiles_layer_space(
            &[(10.0, 20.0)],
            2.0,
            &t,
            200,
            200,
            1024,
            1024,
            Padding::ZERO,
        );
        // Boîte calque [8,12]×[18,22] → doc x[278,282]×y[58,62] (rotation
        // 90° autour du centre (100,100), puis + centre scalé + offset
        // (100,50), cf. `local_to_doc`) : tuile (1,0) unique.
        let fp =
            stroke_footprint_in_document(&[(10.0, 20.0)], 2.0, &t, 200, 200).expect("geste valide");
        assert_eq!(fp, TileRegion::new(278, 58, 4, 4));
        assert_eq!(plan.dirty_tiles, 1);
        assert_eq!(plan.scheduled_tiles, 1);
        assert_eq!(plan.bounds, TileRegion::new(256, 0, 256, 256));
        // Équivalence : le même geste exprimé directement en document-space
        // donne les mêmes tuiles.
        let direct = plan_stroke_tiles(&[(280.0, 60.0)], 2.0, 1024, 1024);
        assert_eq!(direct.dirty_tiles, plan.dirty_tiles);
        assert_eq!(direct.bounds, plan.bounds);
    }

    #[test]
    fn geste_invalide_plan_nul_transforme() {
        assert_eq!(
            plan_stroke_tiles_layer_space(&[], 5.0, &ident(), 64, 64, 64, 64, Padding::ZERO),
            StrokeTilePlan::default()
        );
        assert_eq!(
            plan_stroke_tiles_layer_space(
                &[(0.0, 0.0)],
                0.0,
                &ident(),
                64,
                64,
                64,
                64,
                Padding::ZERO
            ),
            StrokeTilePlan::default()
        );
        assert!(stroke_footprint_in_document(&[], 5.0, &ident(), 64, 64).is_none());
        assert_eq!(
            layer_footprint_in_document(&ident(), 0, 64),
            TileRegion::EMPTY
        );
    }
}
