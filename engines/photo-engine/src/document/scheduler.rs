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

//! Ordonnancement viewport (Phase 6F) : QUOI rendre, DANS QUEL ORDRE.
//!
//! Le scheduler ne calcule AUCUN pixel et ne connaît ni blur ni blend :
//!
//! ```text
//! Camera (écran ⇄ document) → viewport document → TileCoord visibles →
//! analyse dirty → plan ordonné → RenderWorker → TileCache
//! ```
//!
//! Règles :
//!
//! - le compositing (`composite_region_with`) continue de travailler en
//!   document-space : jamais de coordonnée écran en aval ;
//! - les halos restent l'affaire exclusive de `SpatialScope` (le scheduler
//!   ne connaît que des rects sales déjà exprimés) ;
//! - pas de prefetch en 6F : seules les tuiles visibles sont planifiées ;
//! - ordre total déterministe : priorité, puis distance ENTIÈRE au centre,
//!   puis `y`, puis `x` (aucun flottant dans la clé d'ordre).

use tiles::TileRegion;

use super::dirty::DirtyRegion;
use crate::tiles::{TileCoord, tile_coords_for_rect, tile_rect};

/// Caméra minimale : zoom + pan (pas de rotation en 6F).
///
/// Convention : `screen = (doc − pan) × zoom + screen_center` où
/// `screen_center = (width/2, height/2)` ; l'inverse donne le viewport
/// document. Le zoom d'affichage reste state-only côté canvas : la caméra
/// sert ici à DÉRIVER le viewport document (quoi rendre), jamais à
/// re-rendre au zoom (l'échelle de rendu reste 1.0, cf. `RenderRequest`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Facteur de zoom (> 0 ; 1.0 = 100 %).
    pub zoom: f32,
    /// Décalage horizontal du centre document en px écran.
    pub pan_x: f32,
    /// Décalage vertical du centre document en px écran.
    pub pan_y: f32,
}

impl Camera {
    /// Caméra identité (zoom 100 %, centrée).
    #[must_use]
    pub fn identity() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }

    /// Nouvelle caméra (`zoom` non fini ou ≤ 0 ⇒ 1.0).
    #[must_use]
    pub fn new(zoom: f32, pan_x: f32, pan_y: f32) -> Self {
        Self {
            zoom: if zoom.is_finite() && zoom > 0.0 {
                zoom
            } else {
                1.0
            },
            pan_x: if pan_x.is_finite() { pan_x } else { 0.0 },
            pan_y: if pan_y.is_finite() { pan_y } else { 0.0 },
        }
    }

    /// Écran → document (`screen_w/h` = taille du canvas en px écran).
    #[must_use]
    pub fn screen_to_document(self, sx: f32, sy: f32, screen_w: f32, screen_h: f32) -> (f32, f32) {
        let z = self.zoom.max(f32::EPSILON);
        (
            (sx - screen_w / 2.0 - self.pan_x) / z,
            (sy - screen_h / 2.0 - self.pan_y) / z,
        )
    }

    /// Document → écran.
    #[must_use]
    pub fn document_to_screen(self, dx: f32, dy: f32, screen_w: f32, screen_h: f32) -> (f32, f32) {
        (
            (dx) * self.zoom + screen_w / 2.0 + self.pan_x,
            (dy) * self.zoom + screen_h / 2.0 + self.pan_y,
        )
    }

    /// Viewport document visible pour un canvas `screen_w × screen_h`
    /// (DOCUMENT SPACE, entièreté conservative : floor/ceil).
    #[must_use]
    pub fn document_viewport(self, screen_w: f32, screen_h: f32) -> TileRegion {
        let (x0, y0) = self.screen_to_document(0.0, 0.0, screen_w, screen_h);
        let (x1, y1) = self.screen_to_document(screen_w, screen_h, screen_w, screen_h);
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
            return TileRegion::EMPTY;
        }
        TileRegion::new(
            x0.floor() as i32,
            y0.floor() as i32,
            (x1.ceil() - x0.floor()).max(1.0) as u32,
            (y1.ceil() - y0.floor()).max(1.0) as u32,
        )
    }
}

/// Demande de rendu viewport (Phase 6F §7) : ce que le canvas regarde.
///
/// Le worker ne connaît jamais les widgets UI : il reçoit ce descripteur
/// pur (viewport document + échelle de rendu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
    /// Zone à produire (DOCUMENT SPACE, entière).
    pub viewport: TileRegion,
    /// Échelle de rendu (1.0 : le zoom d'affichage est state-only ; toute
    /// autre valeur partitionne les clés pour de futurs mipmaps, sans en
    /// implémenter — jamais de faux hit inter-échelles).
    pub scale: f32,
}

impl RenderRequest {
    /// Nouvelle demande (`scale` non fini ou ≤ 0 ⇒ 1.0).
    #[must_use]
    pub fn new(viewport: TileRegion, scale: f32) -> Self {
        Self {
            viewport,
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
        }
    }
}

/// Contexte de priorité (Phase 6F §10) : emplacement prévu pour distinguer
/// interaction active/inactive. En 6F, la politique d'ordre est identique
/// dans les deux cas (sales d'abord, puis distance) — le champ est accepté
/// et enregistré pour les phases suivantes (gating de prefetch…), sans
/// changer le plan d'aujourd'hui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderPriorityContext {
    /// Vrai pendant un geste (pan/scroll/paint en cours).
    pub interaction_active: bool,
}

impl RenderPriorityContext {
    /// Contexte inactif (complétion).
    #[must_use]
    pub fn idle() -> Self {
        Self {
            interaction_active: false,
        }
    }

    /// Contexte de geste actif.
    #[must_use]
    pub fn active() -> Self {
        Self {
            interaction_active: true,
        }
    }
}

/// Priorité d'une tuile (ordre croissant = rendu en premier).
///
/// 6F : `visible + dirty` puis `visible` (pas de prefetch — "visible first"
/// suffit). Le worker saute le calcul sur hit de toute façon ; l'ordre
/// compte pour la latence perçue et la pression LRU future.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TilePriority {
    /// Visible et sale : contenu périmé à re-rendre d'abord.
    VisibleDirty,
    /// Visible et propre (hit probable) : complétion.
    Visible,
}

/// Demande unitaire ordonnancée : coordonnée canonique + rect + priorité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRenderRequest {
    /// Identité canonique (grille `tile_px`).
    pub coord: TileCoord,
    /// Rectangle document clippé (ce que le worker demande au cache).
    pub rect: TileRegion,
    /// Priorité (ordre principal).
    pub priority: TilePriority,
}

/// Construit le plan ordonné pour `request.viewport` au pas `tile_px` :
/// tuiles visibles sales marquées `VisibleDirty`, ordre total déterministe
/// `(priorité, distance² entière au centre, y, x)`.
///
/// Pas de clipping au document : les tuiles en débordement (plan infini)
/// sont légitimes — le rendu les clampe et rend `None` si rien ne contribue.
/// Le scheduler ne recalcule aucun halo : `dirty` est déjà exprimé en zones
/// d'invalidation (appelant : `mark_dirty_scope` 6D). Pas de tuile hors
/// viewport (pas de prefetch en 6F).
#[must_use]
pub fn schedule_viewport(
    request: &RenderRequest,
    dirty: &DirtyRegion,
    tile_px: u32,
    _ctx: &RenderPriorityContext,
) -> Vec<TileRenderRequest> {
    let visible = request.viewport;
    if visible.is_empty() {
        return Vec::new();
    }
    // Centre du viewport (entiers : pas de flottant dans l'ordre).
    let cx = i64::from(visible.x) + i64::from(visible.width) / 2;
    let cy = i64::from(visible.y) + i64::from(visible.height) / 2;
    let mut plan: Vec<(TileRenderRequest, u64)> = Vec::new();
    for coord in tile_coords_for_rect(&visible, tile_px) {
        // Rect canonique PLEIN (non clippé) : même tuile ⇒ même clé quelle
        // que soit la demande (§4) ; le rendu clampe et rend None si vide.
        let rect = tile_rect(coord, tile_px);
        let priority = if dirty.intersects(rect) {
            TilePriority::VisibleDirty
        } else {
            TilePriority::Visible
        };
        let (ccx, ccy) = coord.center(tile_px.max(1));
        let dx = ccx - cx;
        let dy = ccy - cy;
        let dist2 = (dx * dx + dy * dy) as u64;
        plan.push((
            TileRenderRequest {
                coord,
                rect,
                priority,
            },
            dist2,
        ));
    }
    // Ordre total déterministe : priorité, distance², y, x.
    plan.sort_by(|a, b| {
        (a.0.priority, a.1, a.0.coord.y, a.0.coord.x).cmp(&(
            b.0.priority,
            b.1,
            b.0.coord.y,
            b.0.coord.x,
        ))
    });
    plan.into_iter().map(|(r, _)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_round_trip() {
        let cam = Camera::new(2.0, 30.0, -15.0);
        let (dx, dy) = cam.screen_to_document(400.0, 300.0, 800.0, 600.0);
        let (sx, sy) = cam.document_to_screen(dx, dy, 800.0, 600.0);
        assert!((sx - 400.0).abs() < 0.01, "round-trip x");
        assert!((sy - 300.0).abs() < 0.01, "round-trip y");
        // Identité : centre écran == centre document.
        let (dx, dy) = Camera::identity().screen_to_document(400.0, 300.0, 800.0, 600.0);
        assert!((dx - 0.0).abs() < 0.01 && (dy - 0.0).abs() < 0.01);
        // Zoom 2 : le viewport document couvre la moitié de l'écran
        // (hauteur 301 : floor(-142.5) = -143, conservateur).
        let view = cam.document_viewport(800.0, 600.0);
        assert_eq!(view.width, 400);
        assert_eq!(view.height, 301);
        // Garde-fous.
        assert_eq!(Camera::new(0.0, 0.0, 0.0).zoom, 1.0);
        assert_eq!(Camera::new(f32::NAN, 0.0, 0.0).zoom, 1.0);
    }

    #[test]
    fn plan_visible_dirty_d_abord_deterministe() {
        let req = RenderRequest::new(TileRegion::new(0, 0, 512, 512), 1.0);
        let mut dirty = DirtyRegion::new();
        dirty.add(TileRegion::new(300, 300, 10, 10));
        let idle = RenderPriorityContext::idle();
        let plan = schedule_viewport(&req, &dirty, 256, &idle);
        assert_eq!(plan.len(), 4);
        // Sale d'abord.
        assert_eq!(plan[0].coord, TileCoord::new(1, 1));
        assert_eq!(plan[0].priority, TilePriority::VisibleDirty);
        assert!(
            plan[1..]
                .iter()
                .all(|r| r.priority == TilePriority::Visible)
        );
        // Déterminisme : deux demandes identiques, même plan.
        let plan2 = schedule_viewport(&req, &dirty, 256, &idle);
        assert_eq!(plan, plan2);
        // Contexte actif : même ensemble (politique 6F), ordre identique.
        let active = RenderPriorityContext::active();
        let plan3 = schedule_viewport(&req, &dirty, 256, &active);
        assert_eq!(plan, plan3);
    }

    #[test]
    fn plan_vide_et_hors_document() {
        let dirty = DirtyRegion::new();
        let idle = RenderPriorityContext::idle();
        let req = RenderRequest::new(TileRegion::EMPTY, 1.0);
        assert!(schedule_viewport(&req, &dirty, 256, &idle).is_empty());
        // Hors document : la tuile couvrante est planifiée (visibilité),
        // le rendu la clampe et rend None (pas de pixels hors document —
        // voir les tests worker) — jamais de trou dans la couverture.
        let req = RenderRequest::new(TileRegion::new(900, 900, 10, 10), 1.0);
        let plan = schedule_viewport(&req, &dirty, 256, &idle);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].coord, TileCoord::new(3, 3));
    }

    #[test]
    fn plan_echelles_distinguees() {
        // Même viewport, échelles différentes : requêtes distinctes
        // (partitionnement anti-faux-hit, rendu 1.0 inchangé).
        let a = RenderRequest::new(TileRegion::new(0, 0, 256, 256), 1.0);
        let b = RenderRequest::new(TileRegion::new(0, 0, 256, 256), 2.0);
        assert_ne!(a, b);
        assert_eq!(RenderRequest::new(a.viewport, f32::NAN).scale, 1.0);
    }
}
