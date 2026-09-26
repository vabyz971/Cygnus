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
#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.

//! Pont non bloquant vers le worker `photo-engine` (pattern v2 §3.4).
//!
//! La boucle egui ne touche JAMAIS au `Document` : elle envoie des
//! [`PhotoEngineCommand`] via `Sender` et poll les
//! [`PhotoEngineResponse`] via `try_recv` à chaque frame. Le worker
//! (thread background) applique les commandes au `Document` pur,
//! maintient un historique undo/redo (snapshots partagés, quasi
//! gratuits) et répond avec un snapshot frais + un aperçu composite
//! pour le canvas.
//!
//! Convention d'indices : le panneau affiche le haut de pile en
//! premier ; `ReorderLayer { from, to }` utilise ces indices
//! d'affichage. Le worker les reconvertit (`doc = len - 1 - display`)
//! avant de manipuler `document.root` (index 0 = bas de pile).

use super::features::layers::{
    PhotoLayerInfo, PhotoLayerThumb, snapshot_layers, snapshot_layers_with,
};
use photo_engine::tiles::TileRegion;
use photo_engine::{BlendMode, Document, RenderEvent, RenderRevision};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use uuid::Uuid;

fn assert_send<T: Send>() {}

/// Plus grande dimension de l'aperçu canvas (pixels).
pub const PREVIEW_MAX_DIMENSION: u32 = 1600;

/// Aperçu composite pour le canvas (RGBA8, pur, thread-safe).
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewImage {
    /// Largeur en pixels (miniature).
    pub width: u32,
    /// Hauteur en pixels (miniature).
    pub height: u32,
    /// Pixels RGBA8 ligne par ligne (miniature).
    pub rgba: Vec<u8>,
    /// Dimensions du composite pleine résolution avant miniature.
    pub full_width: u32,
    /// Hauteur pleine résolution.
    pub full_height: u32,
    /// Coin haut-gauche du DOCUMENT en pixels pleine résolution
    /// (le composite couvre le plan infini : origine > 0 dès qu'un
    /// calque dépasse).
    pub origin_x: f32,
    /// Origine verticale pleine résolution.
    pub origin_y: f32,
    /// Dimensions du document (le cadre à dessiner par-dessus).
    pub doc_width: u32,
    /// Hauteur du document.
    pub doc_height: u32,
}

/// Commandes UI → worker moteur.
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoEngineCommand {
    /// Basculer la visibilité d'un calque.
    ToggleLayerVisibility(Uuid),
    /// Réordonner (indices d'affichage : 0 = haut de pile).
    ReorderLayer { from: usize, to: usize },
    /// Régler l'opacité d'un calque (unités moteur 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Régler le mode de fusion d'un calque (choix discret).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Annuler la dernière mutation.
    Undo,
    /// Rétablir la dernière annulation.
    Redo,
    /// Commiter un trait de pinceau/gomme sur un calque pixels.
    ///
    /// `points` en pixels image (espace monde du viewport). La
    /// transform du calque est supposée identité en Phase 4 (cas des
    /// calques photo frais) ; le commit transform-aware complet
    /// (cf. `paint::commit_stroke`) viendra en Phase 5.
    PaintStroke {
        /// Calque pixels cible.
        layer: Uuid,
        /// Polyligne du trait (pixels image).
        points: Vec<(f32, f32)>,
        /// Vrai = gomme (réduit l'alpha), faux = pinceau.
        eraser: bool,
        /// Rayon en pixels image.
        radius: f32,
        /// Couleur RGB (ignorée en mode gomme).
        color: [u8; 3],
        /// Opacité du trait 0..=1.
        opacity: f32,
    },
    /// Déplacer un calque pixels de `(dx, dy)` pixels document
    /// (outil sélection/déplacement : drag commis au relâchement).
    MoveLayer {
        /// Calque pixels cible.
        layer: Uuid,
        /// Décalage horizontal en pixels document.
        dx: f32,
        /// Décalage vertical en pixels document.
        dy: f32,
    },
    /// Ouvrir une image comme nouveau calque (décodage lourd, déjà
    /// sur le thread worker donc non bloquant pour l'UI).
    OpenImage {
        /// Chemin du fichier image.
        path: PathBuf,
    },
    /// Ajouter un calque vide transparent (dimensions du document).
    AddEmptyLayer,
    /// Dupliquer un calque (nouveaux ids, inséré au-dessus).
    DuplicateLayer(Uuid),
    /// Supprimer un calque (refusé s'il est le dernier).
    DeleteLayer(Uuid),
    /// Ajouter un filtre live à un calque pixels (`type_id` du registre).
    AddFilter {
        /// Calque pixels cible.
        layer: Uuid,
        /// Identifiant du filtre (ex. « brightness_contrast »).
        filter_type: String,
    },
    /// Renommer un calque, un filtre ou un masque (nom non vide).
    RenameLayer { layer: Uuid, name: String },
    /// Ajouter un masque blanc (tout visible) au calque porteur.
    AddMask { layer: Uuid },
    /// Supprimer un masque de son porteur.
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Déplacer un filtre dans la pile de son calque (`up` = vers le haut affiché).
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Déplacer un masque dans la pile de son porteur.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Supprimer un filtre d'un calque pixels.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Exporter le composite (format déduit de l'extension : png, jpg,
    /// jpeg, gif — gif = première image, sans transparence animée).
    Export {
        /// Chemin de destination.
        path: PathBuf,
        /// Qualité JPEG 1..=100 (ignorée hors JPEG).
        quality: u8,
    },
    /// Resynchronisation sans mutation (snapshot initial au boot).
    Refresh,
    /// Rogner l'aperçu aux dimensions du document (menu Affichage).
    /// `false` (défaut) = plan infini : le contenu hors document reste
    /// visible autour du cadre.
    SetPreviewClip {
        /// Vrai = aperçu rogné au document.
        clip: bool,
    },
}

/// Réponses worker → UI.
#[derive(Debug, Clone)]
pub enum PhotoEngineResponse {
    /// État frais après application, avec nouveau composite.
    /// `revision` identifie le rendu : l'UI ignore tout aperçu dont
    /// la révision n'est pas plus récente que la texture affichée.
    LayersChanged {
        /// Snapshot pour l'UI.
        layers: Vec<PhotoLayerInfo>,
        /// Aperçu composite pour le canvas (`None` si vide).
        preview: Option<PreviewImage>,
        /// Révision du rendu produit.
        revision: RenderRevision,
        /// Profondeurs d'historique (pour griser undo/redo).
        can_undo: bool,
        /// Profondeur redo.
        can_redo: bool,
    },
    /// État frais SANS nouveau rendu (ex. renommage, no-op) : l'UI
    /// met à jour le panneau et conserve la texture affichée.
    StateChanged {
        /// Snapshot pour l'UI.
        layers: Vec<PhotoLayerInfo>,
        /// Révision du dernier rendu (inchangée, texture conservée).
        revision: RenderRevision,
        /// Profondeurs d'historique (pour griser undo/redo).
        can_undo: bool,
        /// Profondeur redo.
        can_redo: bool,
    },
    /// Miniature secondaire arrivée APRÈS le canvas (Phase 6G.3) : le
    /// `LayersChanged` précédent portait l'aperçu canvas + des miniatures
    /// potentiellement périmées d'une frame ; celle-ci synchronise le
    /// panneau sans jamais re-rendre le canvas. L'UI l'applique si et
    /// seulement si sa version est plus récente que le cache (garde
    /// anti-obsolescence côté UI, en plus de la garde worker).
    ThumbnailUpdated {
        /// Calque concerné.
        layer: Uuid,
        /// Miniature + version d'apparence calculée.
        thumb: PhotoLayerThumb,
    },
    /// Échec non bloquant (ex. décodage impossible).
    EngineError {
        /// Message affichable dans la barre de statut.
        message: String,
    },
    /// Export PNG terminé.
    ExportDone {
        /// Chemin du fichier écrit.
        path: PathBuf,
    },
}

/// Convertit un index d'affichage (0 = haut de pile) en index
/// document (`root[0]` = bas de pile). `None` si hors limites.
pub fn display_to_doc_index(display: usize, len: usize) -> Option<usize> {
    if display < len {
        Some(len - 1 - display)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// INSTRUMENTATION TEMPORAIRE (diagnostic perf, aucune incidence
// fonctionnelle) : timings worker + compteurs, à retirer une fois le
// rapport validé.
// ---------------------------------------------------------------------------

/// Durées d'une passe `render_preview` (microsecondes).
#[derive(Debug, Default, Clone, Copy)]
pub struct PreviewTimings {
    /// Pixels d'entrée du downscale preview §8 (0 si pas de rétrécissement).
    pub preview_resize_pixels: u64,
    /// Composite pleine résolution (`composite_preview` / `composite`).
    pub composite_us: u128,
    /// Miniature (`capped_preview` + `into_raw`).
    pub thumb_us: u128,
    /// Géométrie document (`preview_geometry`, 3e passe apparences).
    pub geometry_us: u128,
    /// Balayages prepare + blend + mix (hors extents, hors alloc).
    pub blend_us: u128,
    /// Calques pixels et groupes effectivement blendés.
    pub layers_blended: u64,
    /// Pixels parcourus (sommes des balayages d'accumulateur).
    pub pixels_processed: u64,
    /// Pixels de l'accumulateur (scope).
    pub scope_px: u64,
    /// Pixels du composite pleine résolution.
    pub full_px: u64,
    /// Pixels de la miniature envoyée.
    pub preview_px: u64,
}

/// Métriques d'une commande worker (remplies par `apply`).
#[derive(Debug, Default, Clone)]
pub struct OpMetrics {
    /// Nom de la commande (voir `PhotoEngineCommand::op_name`).
    pub op: &'static str,
    /// Durée totale de `apply` (mutation + snapshot + réponse).
    pub total_us: u128,
    /// Mutation pure (`mutate` : setters + historique + peinture).
    pub mutation_us: u128,
    /// Construction du snapshot couches (`snapshot_layers`).
    pub snapshot_us: u128,
    /// Composite pleine résolution.
    pub composite_us: u128,
    /// Miniature + mise en buffer.
    pub thumb_us: u128,
    /// Géométrie document (origine du plan infini).
    pub geometry_us: u128,
    /// Construction du partage inter-passes (résolutions renderer).
    pub frame_us: u128,
    /// Apparences résolues auprès du renderer pendant la réponse.
    pub appearance_resolves: u64,
    /// Réutilisations inter-passes (zéro nouvelle résolution).
    pub appearance_frame_hits: u64,
    /// Buffers `preview` régénérés (delta compteurs renderer).
    pub preview_rebuilds: u64,
    /// Miniatures `thumb` régénérées (delta compteurs renderer).
    pub thumb_rebuilds: u64,
    /// OBSERVATION tuiles (vertical slice, sans effet sur le rendu) :
    /// aire de la région sale du dernier stroke en pixels.
    pub dirty_region_px: u64,
    /// OBSERVATION tuiles : tuiles niveau 0 marquées sales.
    pub dirty_tiles: u64,
    /// OBSERVATION tuiles : tuiles retenues par le scheduler.
    pub scheduled_tiles: u64,
    /// Phase 6E : tuiles resservies depuis le cache incrémental (0 calcul).
    pub cache_hits: u64,
    /// Phase 6E : tuiles évaluées (`composite_region_with` fenêtré).
    pub cache_misses: u64,
    /// Phase 6E : rendus évités par les hits (== `cache_hits`).
    pub renders_avoided: u64,
    /// Phase 6F : tuiles visibles requises par le viewport rendu.
    pub visible_tiles: u64,
    /// Phase 6F : tuiles demandées au worker (== visibles sans prefetch).
    pub requested_tiles: u64,
    /// Phase 6G.2 §8 : zones dirty entièrement soldées par la frame
    /// (consommation post-rendu — le garde-fou anti-dérive F1).
    pub dirty_tiles_cleared: u64,
    /// Phase 6G : pixels écrits dans la surface persistante (0 si warm —
    /// hits n'écrivent rien ; partiel ≈ tuiles sales).
    pub surface_pixels_written: u64,
    /// Phase 6G.1 : temps d'ordonnancement du plan (scheduler pur).
    pub schedule_us: u128,
    /// Phase 6G.1 : temps total des évaluations de tuiles (lookup + rendus).
    pub tile_render_total_us: u128,
    /// Phase 6G.1 : temps de découpe d'assemblage (memcpy O(viewport)).
    pub assembly_us: u128,
    /// Phase 6G.1 : temps d'écriture surface de cette frame (delta).
    pub surface_update_us: u128,
    /// Phase 6G.2 §8 : pixels d'entrée du downscale preview (0 si pas de
    /// rétrécissement — même dimensions conservées, §3.7).
    pub preview_resize_pixels: u64,
    /// Phase 6G.3 §14 : miniatures secondaires terminées cette frame.
    pub thumbnail_jobs_completed: u64,
    /// Phase 6G.3 §14 : temps canvas-critical (frame images + composite +
    /// downscale + assemblage — tout ce qui produit les pixels du canvas,
    /// sans les miniatures).
    pub canvas_critical_us: u128,
    /// Phase 6G.3 §14 : travail secondaire absorbé cette frame (miniatures
    /// terminées, hors chemin canvas).
    pub secondary_work_us: u128,
    /// Balayages prepare + blend + mix (hors extents, hors alloc).
    pub blend_us: u128,
    /// Calques pixels et groupes effectivement blendés.
    pub layers_blended: u64,
    /// Pixels parcourus (sommes des balayages d'accumulateur).
    pub pixels_processed: u64,
    /// Pixels de l'accumulateur (scope).
    pub scope_px: u64,
    /// Pixels du composite pleine résolution.
    pub full_px: u64,
    /// Pixels de la miniature envoyée.
    pub preview_px: u64,
}

/// Compteurs cumulés du worker (remis à zéro à la création).
#[derive(Debug, Default)]
pub struct WorkerMetrics {
    /// Métriques de la dernière commande traitée.
    pub last: Option<OpMetrics>,
    /// Commande en cours (posée par `apply`/`apply_batch`).
    current_op: &'static str,
    /// Nombre de réponses produites.
    pub responses: u64,
    /// Nombre de réponses avec aperçu.
    pub previews: u64,
    /// Nombre de composites réellement produits (rendus non coalescés).
    pub renders: u64,
    /// Miniatures secondaires (Phase 6G.3 §14, cumulé).
    pub thumb: crate::ui::thumb_worker::ThumbStats,
}

impl PhotoEngineCommand {
    /// Nom stable de la commande (instrumentation uniquement).
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::ToggleLayerVisibility(_) => "toggle_visibility",
            Self::ReorderLayer { .. } => "reorder",
            Self::SetOpacity { .. } => "set_opacity",
            Self::SetBlendMode { .. } => "set_blend_mode",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::PaintStroke { .. } => "paint_stroke",
            Self::MoveLayer { .. } => "move_layer",
            Self::OpenImage { .. } => "open_image",
            Self::AddEmptyLayer => "add_empty_layer",
            Self::DuplicateLayer(_) => "duplicate_layer",
            Self::DeleteLayer(_) => "delete_layer",
            Self::AddFilter { .. } => "add_filter",
            Self::RenameLayer { .. } => "rename_layer",
            Self::AddMask { .. } => "add_mask",
            Self::RemoveMask { .. } => "remove_mask",
            Self::MoveFilter { .. } => "move_filter",
            Self::MoveMask { .. } => "move_mask",
            Self::RemoveFilter { .. } => "remove_filter",
            Self::Export { .. } => "export",
            Self::Refresh => "refresh",
            Self::SetPreviewClip { .. } => "set_preview_clip",
        }
    }
}

/// Invalidation de rendu induite par une commande : sépare ce qui ne
/// touche que l'état document (pas de nouveau composite) de ce qui
/// exige un re-rendu. Ordre : `Unchanged < StateOnly < Composite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderInvalidation {
    /// Aucun changement d'état (no-op) : rien à produire.
    Unchanged,
    /// État changé, pixels identiques (ex. renommage) : snapshot seul.
    StateOnly,
    /// Pixels potentiellement changés : nouveau composite requis.
    Composite,
}

/// Routage d'une commande vers son événement rendu sémantique
/// ([`RenderEvent`], mécanismes `photo-engine::command`) et son
/// invalidation concrète dans le pipeline CPU actuel.
///
/// Note : `SetOpacity`/`MoveLayer` sont `NodeInvalidated` au sens de
/// `Command::affects_composite` (pensé pour un futur draw GPU), mais
/// le composite CPU doit quand même être re-blendé → `Composite`.
/// Seul le renommage est purement `StateOnly` aujourd'hui.
pub fn render_routing(command: &PhotoEngineCommand) -> (RenderEvent, RenderInvalidation) {
    match command {
        PhotoEngineCommand::ToggleLayerVisibility(_) => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::ReorderLayer { .. } => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::SetOpacity { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::Composite,
        ),
        PhotoEngineCommand::SetBlendMode { .. } => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::MoveLayer { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::Composite,
        ),
        PhotoEngineCommand::PaintStroke { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::Composite,
        ),
        PhotoEngineCommand::OpenImage { .. }
        | PhotoEngineCommand::AddEmptyLayer
        | PhotoEngineCommand::DuplicateLayer(_)
        | PhotoEngineCommand::DeleteLayer(_)
        | PhotoEngineCommand::AddFilter { .. }
        | PhotoEngineCommand::MoveFilter { .. }
        | PhotoEngineCommand::RemoveFilter { .. }
        | PhotoEngineCommand::AddMask { .. }
        | PhotoEngineCommand::RemoveMask { .. }
        | PhotoEngineCommand::MoveMask { .. }
        | PhotoEngineCommand::Undo
        | PhotoEngineCommand::Redo
        | PhotoEngineCommand::Refresh
        | PhotoEngineCommand::SetPreviewClip { .. } => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::RenameLayer { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::StateOnly,
        ),
        PhotoEngineCommand::Export { .. } => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Unchanged)
        }
    }
}

/// Marquage dirty porté par une mutation (Phase 6E) : centralisé dans
/// `mutate()`, jamais demandé aux appelants UI.
///
/// Par défaut : `Composite` ⇒ `Global` (sûr : toute nouvelle commande oublie
/// au pire de la précision, jamais de la correction), sinon `None`. Les bras
/// qui connaissent leur zone (peinture, déplacement…) affinent en `Region`.
/// Appliqué au cache du worker juste après le match, avant `respond`.
#[derive(Debug, Clone, Copy, Default)]
enum DirtyMark {
    /// Dériver du routage (`Composite` ⇒ `Global`, sinon `None`).
    #[default]
    Auto,
    /// Aucune zone (no-op, état seul).
    None,
    /// Zone DOCUMENT SPACE précise (footprint + halo déjà inclus).
    Region(TileRegion),
    /// Tout le document (structurel, global, repli sûr).
    Global,
}

/// Décision d'envoi miniature (Phase 6G.3 §17, pure et déterministe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubmitDecision {
    /// Store déjà à jour : rien à faire.
    Fresh,
    /// Même version déjà en attente : intention absorbée (coalescing).
    Coalesce,
    /// Nouvelle intention à transmettre.
    Submit,
}

/// Résultat d'une mutation sans rendu : réponses immédiates (export,
/// erreurs) + invalidation à produire ensuite.
struct Mutation {
    /// Réponses à envoyer telles quelles (pas de rendu associé).
    immediates: Vec<PhotoEngineResponse>,
    /// Invalidation cumulée de la mutation.
    invalidation: RenderInvalidation,
    /// Vrai si l'état document a changé (snapshot à renvoyer).
    changed: bool,
    /// Marquage dirty (défaut sûr selon `invalidation`, affiné par bras).
    dirty: DirtyMark,
}

impl Mutation {
    /// Mutation effective : un rendu/réponse suivra.
    fn changed(invalidation: RenderInvalidation) -> Self {
        Self {
            immediates: Vec::new(),
            invalidation,
            changed: true,
            dirty: DirtyMark::Auto,
        }
    }

    /// No-op (id inconnu, garde-fou) : état inchangé.
    fn unchanged() -> Self {
        Self {
            immediates: Vec::new(),
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
            dirty: DirtyMark::None,
        }
    }

    /// Réponse immédiate sans rendu (export, erreur non bloquante).
    fn immediate(response: PhotoEngineResponse) -> Self {
        Self {
            immediates: vec![response],
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
            dirty: DirtyMark::None,
        }
    }

    /// Affine le marquage avec une zone précise (consomme `self`).
    fn with_dirty(mut self, dirty: DirtyMark) -> Self {
        self.dirty = dirty;
        self
    }
}

/// Marquage précis pour un nœud porteur (empreinte + spread exact) :
/// `EMPTY` (ajustement, calque vide) ⇒ repli `Global` — jamais de région
/// vide silencieuse (un ajustement touche tout l'accumulateur).
fn mark_for_node(node: &photo_engine::LayerNode) -> DirtyMark {
    let region = photo_engine::tiles::node_mark_region(node);
    if region.is_empty() {
        DirtyMark::Global
    } else {
        DirtyMark::Region(region)
    }
}

/// Replie un lot de commandes en attente (latest-value-wins) AVANT
/// application : les `SetOpacity` consécutifs sur un même calque ne
/// gardent que la dernière valeur, les `MoveLayer` consécutifs sont
/// sommés (somme nulle = commande supprimée). Les valeurs
/// intermédiaires ne remplissent donc jamais la file de rendu, et
/// l'historique ne retient qu'une entrée par geste (coalescence
/// existante préservée : le repli ne franchit jamais `Undo`/`Redo`).
fn fold_batch(commands: Vec<PhotoEngineCommand>) -> Vec<PhotoEngineCommand> {
    let mut folded: Vec<PhotoEngineCommand> = Vec::with_capacity(commands.len());
    for command in commands {
        let merged = match (folded.last_mut(), &command) {
            (
                Some(PhotoEngineCommand::SetOpacity {
                    layer: prev,
                    opacity: prev_op,
                }),
                PhotoEngineCommand::SetOpacity { layer, opacity },
            ) if prev == layer => {
                *prev_op = *opacity;
                true
            }
            (
                Some(PhotoEngineCommand::MoveLayer {
                    layer: prev,
                    dx,
                    dy,
                }),
                PhotoEngineCommand::MoveLayer {
                    layer,
                    dx: ndx,
                    dy: ndy,
                },
            ) if prev == layer => {
                *dx += *ndx;
                *dy += *ndy;
                true
            }
            _ => false,
        };
        if !merged {
            folded.push(command);
        }
    }
    folded
        .into_iter()
        .filter(|command| {
            !matches!(
                command,
                PhotoEngineCommand::MoveLayer { dx, dy, .. } if *dx == 0.0 && *dy == 0.0
            )
        })
        .collect()
}

/// Invalide l'apparence d'un porteur de masque après mutation de ses
/// masques (les setters de masques du moteur ne bumpent pas la version
/// eux-mêmes). Groupes : composition à la volée, rien à invalider.
fn touch_mask_owner(document: &mut Document, owner: Uuid) {
    if let Some(photo_engine::LayerNode::Pixel(pixels)) = document.find_mut(owner) {
        pixels.touch();
    } else if let Some(parent) = document.find_filter_parent(owner)
        && let Some(photo_engine::LayerNode::Pixel(pixels)) = document.find_mut(parent)
    {
        pixels.touch();
    }
}

/// Partage des apparences pendant UNE réponse (`respond()`).
///
/// Le renderer persistant décide seul des recalculs pixels ; ce cache
/// local évite seulement de redemander le même résultat aux trois
/// consommateurs (snapshot, composite, géométrie) : chaque calque
/// pixels est résolu une fois, puis cloné en `Arc` partagés (aucune
/// copie de buffers, conformément à l'objectif 5).
struct AppearanceFrameCache {
    /// Images résolues, par calque (clones `Arc`, pas de pixels).
    /// Phase 6G.3 : UNIQUEMENT les images (composite + géométrie). Les
    /// miniatures partent en tâche secondaire (jamais sur ce chemin).
    map: HashMap<Uuid, Arc<image::DynamicImage>>,
    /// Appels au renderer (premières résolutions).
    resolves: u64,
    /// Réutilisations inter-passes (`Cell` : le résolveur est `Fn`).
    hits: std::cell::Cell<u64>,
}

impl AppearanceFrameCache {
    /// Résout chaque calque pixels UNE fois auprès du renderer
    /// (chemin image seule, sans miniature ni preview).
    fn build(document: &Document) -> Self {
        let ids: Vec<Uuid> = document.all_pixel_ids();
        let mut map = HashMap::with_capacity(ids.len());
        let mut resolves = 0;
        for id in ids {
            if let Some(image) = document.appearance_image_only(id) {
                map.insert(id, image);
                resolves += 1;
            }
        }
        Self {
            map,
            resolves,
            hits: std::cell::Cell::new(0),
        }
    }

    /// Image partagée pour le compositing (clone d'`Arc`).
    fn image(&self, id: Uuid) -> Option<Arc<image::DynamicImage>> {
        let image = self.map.get(&id)?;
        self.hits.set(self.hits.get() + 1);
        Some(Arc::clone(image))
    }

    /// Compteur de réutilisations (lecture après la réponse).
    fn hits(&self) -> u64 {
        self.hits.get()
    }
}

/// État du worker : document vivant + historique undo/redo.
pub struct EngineWorker {
    document: Document,
    undo: Vec<photo_engine::history::Snapshot>,
    redo: Vec<photo_engine::history::Snapshot>,
    /// Calque du dernier `SetOpacity` (coalescence des sliders).
    coalesced_opacity_layer: Option<Uuid>,
    /// Aperçu rogné au document (menu Affichage, défaut : plan infini).
    clip_to_doc: bool,
    /// Révision du dernier composite produit (rendus obsolètes
    /// ignorés côté UI).
    revision: RenderRevision,
    /// OBSERVATION tuiles (vertical slice) : plan calculé pendant la
    /// mutation d'un stroke, consommé par `respond` dans les métriques.
    /// Jamais lu par le renderer : pur constat, zéro effet de bord.
    /// En lot (`apply_batch`), seul le dernier stroke du lot est observé.
    pending_tile_plan: Option<photo_engine::tiles::StrokeTilePlan>,
    /// Rendu incrémental (Phase 6E) : cache de tuiles + périmètre suivi,
    /// alimenté par les marquages `Mutation::dirty`, consommé par le
    /// preview. Persiste entre les commandes : les tuiles propres sont
    /// réutilisées d'une frame à l'autre.
    render_worker: photo_engine::document::RenderWorker,
    /// Miniatures connues par calque (Phase 6G.3) : `(RgbaBuf, version)`
    /// des derniers résultats INGÉRÉS (garde de fraîcheur à l'ingestion).
    /// Le snapshot les sert (estampillées version live) ; les miniatures
    /// fraîches partent aussi en `ThumbnailUpdated`.
    thumb_store: HashMap<Uuid, (photo_engine::RgbaBuf, u64)>,
    /// Intentions soumises et non encore soldées : `(version, source)` par
    /// calque (déduplication d'envoi + second facteur de la garde de
    /// fraîcheur — voir `ingest_thumb_done`).
    thumb_pending: HashMap<Uuid, (u64, Arc<image::DynamicImage>)>,
    /// Calques aux miniatures fraîchement stockées depuis le dernier
    /// drainage (extras `ThumbnailUpdated` du mode lot).
    thumb_fresh: Vec<Uuid>,
    /// Tâche secondaire de miniatures (thread en production, inline en
    /// tests : même code, déterminisme préservé).
    thumb_worker: crate::ui::thumb_worker::ThumbWorker,
    /// Cumul des pixels écrits en surface à la dernière réponse (Phase 6G) :
    /// sert à rapporter le delta par frame dans `OpMetrics`.
    last_surface_pixels: u64,
    /// Cumul du temps surface à la dernière réponse (Phase 6G.1) : idem.
    last_surface_us: u128,
    /// INSTRUMENTATION TEMPORAIRE (diagnostic perf).
    pub metrics: WorkerMetrics,
}

/// Budget du cache de tuiles du worker (8M px ≈ 32 Mo RGBA8) : couvre
/// plusieurs documents 1080p en tuiles 256² avec marge pour les halos.
const TILE_CACHE_BUDGET_PX: usize = 8 << 20;

/// Bornes de l'historique (snapshots partagés, quasi gratuits).
pub const HISTORY_LIMIT: usize = 100;

impl EngineWorker {
    /// Crée un worker sur un document existant.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            coalesced_opacity_layer: None,
            clip_to_doc: false,
            revision: RenderRevision::default(),
            pending_tile_plan: None,
            render_worker: photo_engine::document::RenderWorker::new(
                TILE_CACHE_BUDGET_PX,
                photo_engine::tiles::VIEWPORT_TILE_PX,
            ),
            thumb_store: HashMap::new(),
            thumb_pending: HashMap::new(),
            thumb_fresh: Vec::new(),
            thumb_worker: crate::ui::thumb_worker::ThumbWorker::inline_(),
            last_surface_pixels: 0,
            last_surface_us: 0,
            metrics: WorkerMetrics::default(),
        }
    }

    /// INSTRUMENTATION TEMPORAIRE : accès en lecture au document
    /// vivant (sondes de mesure uniquement).
    pub fn test_document(&self) -> &Document {
        &self.document
    }

    /// Active la tâche secondaire threadée (production ; le worker vit sur
    /// son propre thread de toute façon). Sans appel : mode inline
    /// déterministe (tests, `apply_command` éphémère — aucun thread fui).
    pub fn enable_thumb_thread(&mut self) {
        if !self.thumb_worker.is_threaded() {
            self.thumb_worker = crate::ui::thumb_worker::ThumbWorker::spawned();
        }
    }

    /// Miniature connue d'un calque SI elle correspond à la version live
    /// (sinon `None` : icône en attendant la tâche secondaire). Source
    /// unique des miniatures du snapshot — jamais le renderer direct.
    fn stored_thumb(&self, id: Uuid) -> Option<photo_engine::RgbaBuf> {
        let live = match self.document.find(id) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => return None,
        };
        let (thumb, version) = self.thumb_store.get(&id)?;
        (*version == live).then(|| thumb.clone())
    }

    /// Retire du store les calques disparus (structurel, undo/redo…).
    fn prune_thumb_store(&mut self) {
        let live: std::collections::HashSet<Uuid> =
            self.document.all_pixel_ids().into_iter().collect();
        self.thumb_store.retain(|id, _| live.contains(id));
        self.thumb_pending.retain(|id, _| live.contains(id));
    }

    /// Décision d'envoi pure (testable) : `Fresh` (rien à faire),
    /// `Coalesce` (intention identique déjà en attente — absorbée),
    /// `Submit` (nouvelle intention à transmettre).
    fn submit_decision(known: bool, pending: Option<u64>, live: u64) -> SubmitDecision {
        if pending == Some(live) {
            SubmitDecision::Coalesce
        } else if known {
            SubmitDecision::Fresh
        } else {
            SubmitDecision::Submit
        }
    }

    /// Soumet les miniatures périmées (version live inconnue du store ET non
    /// déjà en attente à cette version). En mode inline : calcul + ingestion
    /// immédiats (déterministe). En mode threadé : envoi non bloquant
    /// (repli inline si le thread est tombé).
    /// Retourne les versions soumises (diagnostic).
    fn submit_stale_thumbs(&mut self) -> u64 {
        use crate::ui::thumb_worker::{ThumbJob, ThumbWorker};
        self.prune_thumb_store();
        let mut submitted = 0u64;
        let mut inline_done = Vec::new();
        for id in self.document.all_pixel_ids() {
            let Some(photo_engine::LayerNode::Pixel(layer)) = self.document.find(id) else {
                continue;
            };
            let live_version = layer.appearance_version;
            let known = self
                .thumb_store
                .get(&id)
                .is_some_and(|(_, v)| *v == live_version);
            let pending = self.thumb_pending.get(&id).map(|(v, _)| *v);
            match Self::submit_decision(known, pending, live_version) {
                SubmitDecision::Fresh => continue,
                // Intention redondante absorbée (coalescing émetteur).
                SubmitDecision::Coalesce => {
                    self.metrics.thumb.coalesced += 1;
                    continue;
                }
                SubmitDecision::Submit => {}
            }
            let job = ThumbJob {
                layer: id,
                version: live_version,
                source: Arc::clone(&layer.source_image),
                filters: layer.filter_layers.clone(),
                masks: layer.masks.clone(),
            };
            self.metrics.thumb.submitted += 1;
            submitted += 1;
            self.thumb_pending
                .insert(id, (live_version, Arc::clone(&layer.source_image)));
            if self.thumb_worker.is_threaded() {
                if !self.thumb_worker.submit(job.clone()) {
                    // Thread tombé : repli inline sûr (pixels identiques).
                    if let Some(done) = ThumbWorker::compute_now(&job) {
                        inline_done.push(done);
                    }
                }
            } else if let Some(done) = ThumbWorker::compute_now(&job) {
                inline_done.push(done);
            }
        }
        for done in inline_done {
            self.ingest_thumb_done(done);
        }
        submitted
    }

    /// Ingère un résultat (garde §6-§7) : applicable ssi version ET source
    /// correspondent encore au vivant — le compteur global monotone ne
    /// réutilise jamais un numéro pour un contenu différent (les snapshots
    /// préservant les `Arc`), et `ptr_eq` verrouille l'allocation.
    /// Retourne vrai si stocké (frais). Utilisé par le poll threadé ET le
    /// mode inline (même code).
    fn ingest_thumb_done(&mut self, done: crate::ui::thumb_worker::ThumbDone) -> bool {
        let pending_source = self
            .thumb_pending
            .get(&done.layer)
            .map(|(_, src)| Arc::clone(src));
        if let Some((v, _)) = self.thumb_pending.get(&done.layer)
            && *v == done.version
        {
            self.thumb_pending.remove(&done.layer);
        }
        // Double facteur : version (compteur global monotone, jamais réutilisé
        // pour un contenu différent — les snapshots préservant les `Arc`) ET
        // identité d'allocation source (conservée à l'envoi ; repli version
        // seule pour un résultat externe sans trace, p. ex. tests).
        let fresh = match self.document.find(done.layer) {
            Some(photo_engine::LayerNode::Pixel(layer)) => {
                let job_source = pending_source.as_ref().unwrap_or(&layer.source_image);
                crate::ui::thumb_worker::is_fresh(
                    &done,
                    layer.appearance_version,
                    &layer.source_image,
                    job_source,
                )
            }
            _ => false,
        };
        if fresh {
            self.metrics.thumb.completed += 1;
            self.metrics.thumb.thumb_us += done.render_us + done.thumb_us;
            self.thumb_store
                .insert(done.layer, (done.thumb, done.version));
            if !self.thumb_fresh.contains(&done.layer) {
                self.thumb_fresh.push(done.layer);
            }
            true
        } else {
            self.metrics.thumb.discarded += 1;
            false
        }
    }

    /// Draine les résultats threadés disponibles (jamais bloquant).
    /// Retourne le nombre de frais ingérés.
    fn poll_thumb_results(&mut self) -> u64 {
        let mut fresh = 0u64;
        // Emprunt disjoint : drain d'abord (fin d'emprunt), ingestion ensuite.
        let done: Vec<_> = self.thumb_worker.drain_results();
        for d in done {
            if self.ingest_thumb_done(d) {
                fresh += 1;
            }
        }
        fresh
    }

    /// Extras `ThumbnailUpdated` pour les miniatures fraîchement stockées
    /// depuis le dernier drainage (mode lot uniquement — `apply` solo garde
    /// le contrat 1:1). Saute les couches dont la réponse principale vient
    /// DÉJÀ d'embarquer la version (cas inline : snapshot synchrone — aucun
    /// doublon). Chaque extra porte la version calculée ; l'UI ignore tout
    /// ce qui n'est pas plus récent que son cache (idempotent).
    fn drain_thumb_extras(&mut self, layers: &[PhotoLayerInfo]) -> Vec<PhotoEngineResponse> {
        let mut out = Vec::new();
        for layer in std::mem::take(&mut self.thumb_fresh) {
            let embedded = layers
                .iter()
                .find(|l| l.id == layer)
                .and_then(|l| l.thumb.as_ref().map(|t| t.version));
            if let Some((thumb, version)) = self.thumb_store.get(&layer) {
                if embedded == Some(*version) {
                    continue;
                }
                // Vérifie que le vivant correspond encore (sinon : périmé
                // entre-temps — le prochain job le recalculera).
                let current = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(l)) => Some(l.appearance_version),
                    _ => None,
                };
                if current == Some(*version) {
                    out.push(PhotoEngineResponse::ThumbnailUpdated {
                        layer,
                        thumb: PhotoLayerThumb {
                            width: thumb.width,
                            height: thumb.height,
                            rgba: thumb.data.to_vec(),
                            version: *version,
                        },
                    });
                }
            }
        }
        out
    }

    /// Empile l'état pré-mutation (jamais après) et vide le redo.
    /// Les gestes continus (sliders) sont coalescés par calque.
    fn push_history(&mut self, coalesce_layer: Option<Uuid>) {
        if coalesce_layer.is_some() && coalesce_layer == self.coalesced_opacity_layer {
            return;
        }
        if self.undo.len() >= HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(self.document.snapshot());
        self.redo.clear();
        self.coalesced_opacity_layer = coalesce_layer;
    }

    /// Construit la réponse d'une mutation : nouveau composite
    /// (`Composite`, révision incrémentée) ou snapshot seul
    /// (`StateOnly`/`Unchanged`, texture UI conservée).
    ///
    /// En mode `Composite`, les images sont résolues UNE fois dans un
    /// [`AppearanceFrameCache`] local (images seules, sans miniatures —
    /// Phase 6G.3) puis partagées entre composite et géométrie
    /// (single-pass, sans toucher au cache persistant du renderer). Les
    /// miniatures viennent du store secondaire (`stored_thumb`, potentiellement
    /// en retard d'une frame — §18) ; le snapshot n'attend jamais leur calcul.
    /// INSTRUMENTATION TEMPORAIRE : remplit `metrics.last` (hors
    /// `op`, `total_us` et `mutation_us`, posés par `apply`/`apply_batch`).
    fn respond(&mut self, invalidation: RenderInvalidation) -> PhotoEngineResponse {
        let stats_before = self.document.appearance_stats();
        // Miniatures secondaires terminées précédemment : ingérées AVANT le
        // snapshot (le panneau profite du travail déjà fini, sans attendre).
        let thumb_completed_before = self.metrics.thumb.completed;
        let thumb_us_before = self.metrics.thumb.thumb_us;
        self.poll_thumb_results();
        let t_frame = std::time::Instant::now();
        let frame = (invalidation == RenderInvalidation::Composite)
            .then(|| AppearanceFrameCache::build(&self.document));
        let frame_us = t_frame.elapsed().as_micros();
        // Tâche secondaire : soumise APRES le cadre (les entrées sont
        // clonées, l'état est figé par la mutation terminée) et AVANT le
        // composite — en mode threadé, le calcul recouvre le rendu canvas.
        if frame.is_some() {
            self.submit_stale_thumbs();
        }
        let t_snapshot = std::time::Instant::now();
        // Miniatures TOUJOURS depuis le store secondaire (même en StateChanged :
        // jamais de rebuild synchrone pour le panneau — `None` = icône en
        // attendant la tâche, §18).
        let layers = snapshot_layers_with(&self.document, &|id| self.stored_thumb(id));
        let snapshot_us = t_snapshot.elapsed().as_micros();
        let can_undo = !self.undo.is_empty();
        let can_redo = !self.redo.is_empty();
        if invalidation == RenderInvalidation::Composite {
            let frame = frame.as_ref().expect("cadre construit en mode Composite");
            // Pipeline incrémental 6E : tuiles sales réévaluées, propres
            // réutilisées ; mêmes octets que le legacy à contenu égal.
            let (preview, timings, worker_stats) = self.render_preview_incremental(frame);
            // Phase 6G.2 §1 : le dirty n'est soldé qu'APRÈS production du
            // résultat (jamais avant, jamais sans rendu). `None` ⇒ rien à
            // solder (re-tentative conservatrice à la prochaine frame).
            // Retourne le nombre de zones entièrement soldées (§8).
            let dirty_cleared = if preview.is_some() {
                let rendered = self.render_worker.take_rendered_tiles();
                self.render_worker.cache_mut().consume_rendered(&rendered)
            } else {
                0
            };
            let revision = self.revision.bump();
            self.metrics.renders += 1;
            let stats_after = self.document.appearance_stats();
            // Observation tuiles consommée ici (zéro si la commande
            // n'était pas un stroke) : le renderer a déjà tourné au-dessus.
            let tiles = self.pending_tile_plan.take().unwrap_or_default();
            self.metrics.last = Some(OpMetrics {
                op: self.metrics.current_op,
                total_us: 0,
                mutation_us: 0,
                frame_us,
                snapshot_us,
                composite_us: timings.composite_us,
                thumb_us: timings.thumb_us,
                geometry_us: timings.geometry_us,
                appearance_resolves: frame.resolves,
                appearance_frame_hits: frame.hits(),
                preview_rebuilds: stats_after.preview_rebuilds - stats_before.preview_rebuilds,
                thumb_rebuilds: stats_after.thumb_rebuilds - stats_before.thumb_rebuilds,
                blend_us: timings.blend_us,
                layers_blended: timings.layers_blended,
                pixels_processed: timings.pixels_processed,
                scope_px: timings.scope_px,
                full_px: timings.full_px,
                preview_px: timings.preview_px,
                preview_resize_pixels: timings.preview_resize_pixels,
                dirty_region_px: tiles.dirty_region_px,
                dirty_tiles: tiles.dirty_tiles,
                scheduled_tiles: tiles.scheduled_tiles,
                cache_hits: worker_stats.cache_hits,
                cache_misses: worker_stats.cache_misses,
                renders_avoided: worker_stats.cache_hits,
                visible_tiles: worker_stats.visible_tiles,
                requested_tiles: worker_stats.requested_tiles,
                dirty_tiles_cleared: dirty_cleared,
                surface_pixels_written: self
                    .render_worker
                    .surface_stats()
                    .pixels_written
                    .saturating_sub(self.last_surface_pixels),
                schedule_us: worker_stats.schedule_us,
                tile_render_total_us: worker_stats.tile_render_total_us,
                assembly_us: worker_stats.assembly_us,
                surface_update_us: self
                    .render_worker
                    .surface_stats()
                    .update_us
                    .saturating_sub(self.last_surface_us),
                thumbnail_jobs_completed: self
                    .metrics
                    .thumb
                    .completed
                    .saturating_sub(thumb_completed_before),
                canvas_critical_us: frame_us
                    + timings.composite_us
                    + timings.thumb_us
                    + worker_stats.assembly_us,
                secondary_work_us: self.metrics.thumb.thumb_us.saturating_sub(thumb_us_before),
            });
            // Fenêtre glissante : la prochaine réponse rapportera son delta.
            self.last_surface_pixels = self.render_worker.surface_stats().pixels_written;
            self.last_surface_us = self.render_worker.surface_stats().update_us;
            PhotoEngineResponse::LayersChanged {
                layers,
                preview,
                revision,
                can_undo,
                can_redo,
            }
        } else {
            self.metrics.last = Some(OpMetrics {
                op: self.metrics.current_op,
                total_us: 0,
                mutation_us: 0,
                frame_us,
                snapshot_us,
                composite_us: 0,
                thumb_us: 0,
                geometry_us: 0,
                appearance_resolves: 0,
                appearance_frame_hits: 0,
                preview_rebuilds: 0,
                thumb_rebuilds: 0,
                blend_us: 0,
                layers_blended: 0,
                pixels_processed: 0,
                scope_px: 0,
                full_px: 0,
                preview_px: 0,
                dirty_region_px: 0,
                dirty_tiles: 0,
                scheduled_tiles: 0,
                cache_hits: 0,
                cache_misses: 0,
                renders_avoided: 0,
                visible_tiles: 0,
                requested_tiles: 0,
                dirty_tiles_cleared: 0,
                surface_pixels_written: 0,
                schedule_us: 0,
                tile_render_total_us: 0,
                assembly_us: 0,
                surface_update_us: 0,
                preview_resize_pixels: 0,
                thumbnail_jobs_completed: 0,
                canvas_critical_us: 0,
                secondary_work_us: 0,
            });
            PhotoEngineResponse::StateChanged {
                layers,
                revision: self.revision,
                can_undo,
                can_redo,
            }
        }
    }

    /// Applique une commande (contrat 1:1 : toujours une réponse).
    /// Ne panique jamais : commande invalide (id inconnu, index hors
    /// limites) = no-op (avec réponse d'état pour resynchroniser
    /// l'UI, sans re-rendu).
    /// INSTRUMENTATION TEMPORAIRE : `metrics` est mis à jour ici.
    pub fn apply(&mut self, command: PhotoEngineCommand) -> PhotoEngineResponse {
        let t_total = std::time::Instant::now();
        self.metrics.current_op = command.op_name();
        let t_mutation = std::time::Instant::now();
        let mutation = self.mutate(command);
        let mutation_us = t_mutation.elapsed().as_micros();
        let response = match mutation.immediates.into_iter().next() {
            Some(immediate) => immediate,
            None => self.respond(if mutation.changed {
                mutation.invalidation
            } else {
                RenderInvalidation::Unchanged
            }),
        };
        let total_us = t_total.elapsed().as_micros();
        if let Some(last) = self.metrics.last.as_mut() {
            last.op = self.metrics.current_op;
            last.total_us = total_us;
            last.mutation_us = mutation_us;
        }
        self.metrics.responses += 1;
        if matches!(
            response,
            PhotoEngineResponse::LayersChanged {
                preview: Some(_),
                ..
            }
        ) {
            self.metrics.previews += 1;
        }
        response
    }

    /// Applique un lot de commandes (boucle worker) avec coalescence
    /// rendu : repli latest-value-wins ([`fold_batch`]), UN seul rendu
    /// pour tout le lot. L'historique reste transactionnel par
    /// commande (coalescence slider préservée) : undo/redo inchangés.
    /// Retourne 0..=N réponses (aucune si le lot se replie à vide).
    pub fn apply_batch(&mut self, commands: Vec<PhotoEngineCommand>) -> Vec<PhotoEngineResponse> {
        let t_total = std::time::Instant::now();
        self.metrics.current_op = "batch";
        let folded = fold_batch(commands);
        let had_commands = !folded.is_empty();
        let mut out = Vec::new();
        let mut max_invalidation = RenderInvalidation::Unchanged;
        let mut changed = false;
        let t_mutation = std::time::Instant::now();
        for command in folded {
            let mutation = self.mutate(command);
            out.extend(mutation.immediates);
            max_invalidation = max_invalidation.max(mutation.invalidation);
            changed |= mutation.changed;
        }
        let mutation_us = t_mutation.elapsed().as_micros();
        if changed {
            out.push(self.respond(max_invalidation));
        } else if had_commands {
            // Resynchronisation sans rendu (que des no-ops).
            out.push(self.respond(RenderInvalidation::Unchanged));
        }
        // Miniatures secondaires terminées pendant le lot (Phase 6G.3 §4A) :
        // re-poll (résultats arrivés PENDANT le composite) puis poussées
        // APRES la réponse principale pour ne jamais retarder le canvas
        // (`drain_thumb_extras` saute celles déjà embarquées).
        // Emprunt disjoint : `out` est local, `self` est emprunté après.
        self.poll_thumb_results();
        let embedded: &[PhotoLayerInfo] = match out.last() {
            Some(PhotoEngineResponse::LayersChanged { layers, .. })
            | Some(PhotoEngineResponse::StateChanged { layers, .. }) => layers,
            _ => &[],
        };
        let extras = self.drain_thumb_extras(embedded);
        out.extend(extras);
        let total_us = t_total.elapsed().as_micros();
        if let Some(last) = self.metrics.last.as_mut() {
            last.op = self.metrics.current_op;
            last.total_us = total_us;
            last.mutation_us = mutation_us;
        }
        self.metrics.responses += out.len() as u64;
        self.metrics.previews += out
            .iter()
            .filter(|response| {
                matches!(
                    response,
                    PhotoEngineResponse::LayersChanged {
                        preview: Some(_),
                        ..
                    }
                )
            })
            .count() as u64;
        out
    }

    /// Corps de [`Self::apply`] (instrumentation dans l'enveloppe).
    /// Ne rend jamais : applique la mutation, marque le dirty incrémental
    /// (Phase 6E §1 : centralisé ici, jamais chez les appelants UI) et
    /// retourne son invalidation (+ réponses immédiates type export/erreur).
    /// Le rendu éventuel est produit par [`Self::respond`].
    fn mutate(&mut self, command: PhotoEngineCommand) -> Mutation {
        let mutation = self.mutate_inner(command);
        // Marquage centralisé : précis si le bras sait (`Region`), repli
        // global sûr si `Composite` sans précision, rien sinon. Une commande
        // future oublie au pire de la précision (re-rendu large), jamais de
        // la correction (pas de tuile périmée).
        let mark = match mutation.dirty {
            DirtyMark::Auto
                if mutation.changed && mutation.invalidation == RenderInvalidation::Composite =>
            {
                DirtyMark::Global
            }
            DirtyMark::Auto | DirtyMark::None => DirtyMark::None,
            precise => precise,
        };
        match mark {
            DirtyMark::None | DirtyMark::Auto => {}
            DirtyMark::Region(region) => self.render_worker.cache_mut().mark_dirty(region),
            DirtyMark::Global => self
                .render_worker
                .cache_mut()
                .mark_global(self.document.width.max(1), self.document.height.max(1)),
        }
        mutation
    }

    /// Application brute d'une commande (sans marquage) — voir [`Self::mutate`].
    fn mutate_inner(&mut self, command: PhotoEngineCommand) -> Mutation {
        let (_, invalidation) = render_routing(&command);
        match command {
            PhotoEngineCommand::ToggleLayerVisibility(id) => {
                if self.document.find_mut(id).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                if let Some(node) = self.document.find_mut(id) {
                    let visible = node.visible();
                    node.set_visible(!visible);
                }
                let dirty = self
                    .document
                    .find(id)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::ReorderLayer { from, to } => {
                // Indices d'affichage (0 = haut de pile) vers `root`
                // (index 0 = bas de pile) : `to == len` = bas de pile.
                let len = self.document.root.len();
                if len == 0 || from >= len {
                    return Mutation::unchanged();
                }
                let to = to.min(len);
                if from == to {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                let node = self.document.root.remove(len - 1 - from);
                let remaining = self.document.root.len();
                let insert_at = remaining.saturating_sub(to).min(remaining);
                self.document.root.insert(insert_at, node);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::SetOpacity { layer, opacity } => {
                if self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                // Coalescence : un seul snapshot par geste de slider.
                self.push_history(Some(layer));
                if let Some(node) = self.document.find_mut(layer) {
                    node.set_opacity(opacity);
                }
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::SetBlendMode { layer, mode } => {
                if self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                // Choix discret : un snapshot par changement.
                self.push_history(None);
                if let Some(node) = self.document.find_mut(layer) {
                    node.set_blend_mode(mode);
                }
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::MoveLayer { layer, dx, dy } => {
                // Garde-fous : deltas finis et non nuls, calque pixels.
                if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
                    return Mutation::unchanged();
                }
                if self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                // Ancienne transform AVANT déplacement (old ∪ new ensuite).
                let before: Option<(photo_engine::Transform2D, u32, u32)> =
                    match self.document.find(layer) {
                        Some(photo_engine::LayerNode::Pixel(pixels)) => Some((
                            pixels.transform,
                            pixels.dimensions().0,
                            pixels.dimensions().1,
                        )),
                        _ => None,
                    };
                if let Some(node) = self.document.find_mut(layer) {
                    // Groupes/ajustements : no-op (snapshot déjà poussé
                    // par prudence, état inchangé).
                    node.translate_by(dx, dy);
                }
                // Marquage précis : union des empreintes avant/après (+spread
                // exact). Sans calque pixels : repli global sûr.
                let dirty = match (before, self.document.find(layer)) {
                    (Some((old_t, w, h)), Some(photo_engine::LayerNode::Pixel(pixels))) => {
                        let spread = photo_engine::tiles::appearance_spread(&pixels.filter_layers);
                        DirtyMark::Region(photo_engine::tiles::layer_move_dirty_region(
                            &old_t,
                            &pixels.transform,
                            w,
                            h,
                            spread,
                        ))
                    }
                    _ => DirtyMark::Global,
                };
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::Undo => {
                let Some(snapshot) = self.undo.pop() else {
                    return Mutation::unchanged();
                };
                self.redo.push(self.document.snapshot());
                self.document.restore_snapshot(snapshot);
                self.coalesced_opacity_layer = None;
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::Redo => {
                let Some(snapshot) = self.redo.pop() else {
                    return Mutation::unchanged();
                };
                self.undo.push(self.document.snapshot());
                self.document.restore_snapshot(snapshot);
                self.coalesced_opacity_layer = None;
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::PaintStroke {
                layer,
                points,
                eraser,
                radius,
                color,
                opacity,
            } => {
                // Garde-fous : rayon non fini (NaN/infini) ou nul → no-op.
                if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
                    return Mutation::unchanged();
                }
                let is_pixel = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                );
                if !is_pixel {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                let Some(photo_engine::LayerNode::Pixel(pixels)) = self.document.find_mut(layer)
                else {
                    return Mutation::unchanged();
                };
                let source = pixels.source_image.clone();
                let raster = source.to_rgba8();
                let (width, height) = (raster.width(), raster.height());
                let mut buffer = raster.into_raw();
                let brush = photo_engine::paint::BrushParams {
                    radius,
                    color,
                    opacity: opacity.clamp(0.0, 1.0),
                    mode: if eraser {
                        photo_engine::paint::StrokeMode::Erase
                    } else {
                        photo_engine::paint::StrokeMode::Paint
                    },
                };
                photo_engine::paint::paint_stroke_rgba(&mut buffer, width, height, &points, &brush);
                if let Some(image) = image::RgbaImage::from_raw(width, height, buffer) {
                    // Re-lecture défensive après le push d'historique.
                    if let Some(photo_engine::LayerNode::Pixel(pixels)) =
                        self.document.find_mut(layer)
                    {
                        pixels.set_source_image(image::DynamicImage::ImageRgba8(image));
                        pixels.touch();
                    }
                }
                // OBSERVATION tuiles (vertical slice) : plan calculé sur les
                // entrées validées de la commande, après peinture réussie.
                // Points en LAYER SPACE → région DOCUMENT SPACE via la
                // transform du calque + halo des flous actifs (Phase 6B).
                // Ne touche ni au document ni au renderer : le composite
                // pleine cadre reste produit comme avant par `respond`.
                // La rastérisation ci-dessus est inchangée (espace calque).
                let (layer_transform, layer_w, layer_h, halo) = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => (
                        pixels.transform,
                        pixels.dimensions().0,
                        pixels.dimensions().1,
                        photo_engine::tiles::blur_halo_for_filters(&pixels.filter_layers),
                    ),
                    _ => (
                        photo_engine::Transform2D::default(),
                        self.document.width,
                        self.document.height,
                        photo_engine::tiles::Padding::ZERO,
                    ),
                };
                self.pending_tile_plan = Some(photo_engine::tiles::plan_stroke_tiles_layer_space(
                    &points,
                    radius,
                    &layer_transform,
                    layer_w,
                    layer_h,
                    self.document.width,
                    self.document.height,
                    halo,
                ));
                // Marquage précis (validité cache) : empreinte document +
                // diffusion EXACTE de la chaîne d'apparence (pas le halo
                // d'observation 6B) — la peinture ne déplace pas le calque.
                let spread = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        photo_engine::tiles::appearance_spread(&pixels.filter_layers)
                    }
                    _ => photo_engine::tiles::Padding::ZERO,
                };
                let dirty = photo_engine::tiles::stroke_footprint_in_document(
                    &points,
                    radius,
                    &layer_transform,
                    layer_w,
                    layer_h,
                )
                .map(|footprint| DirtyMark::Region(footprint.pad(spread)))
                .unwrap_or(DirtyMark::Global);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::OpenImage { path } => match image::open(&path) {
                Ok(image) => {
                    self.push_history(None);
                    let name = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("image")
                        .to_owned();
                    self.document.width = self.document.width.max(image.width());
                    self.document.height = self.document.height.max(image.height());
                    self.document.push_layer(photo_engine::LayerNode::Pixel(
                        photo_engine::PixelLayer::new(name, std::sync::Arc::new(image)),
                    ));
                    Mutation::changed(invalidation)
                }
                Err(error) => Mutation::immediate(PhotoEngineResponse::EngineError {
                    message: format!("Ouverture impossible : {}", error),
                }),
            },
            PhotoEngineCommand::AddEmptyLayer => {
                self.push_history(None);
                let (width, height) = (self.document.width.max(1), self.document.height.max(1));
                let blank = image::DynamicImage::new_rgba8(width, height);
                let name = format!("Calque {}", self.document.root.len() + 1);
                self.document.push_layer(photo_engine::LayerNode::Pixel(
                    photo_engine::PixelLayer::new(name, std::sync::Arc::new(blank)),
                ));
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::DuplicateLayer(id) => {
                if self.document.find(id).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.duplicate(id);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::DeleteLayer(id) => {
                // Jamais de document vide : le dernier calque est conservé.
                if self.document.root.len() <= 1 || self.document.find(id).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.remove(id);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::AddFilter { layer, filter_type } => {
                let Some(filter) = photo_engine::new_filter_layer(&filter_type) else {
                    return Mutation::immediate(PhotoEngineResponse::EngineError {
                        message: format!("Filtre inconnu : {filter_type}"),
                    });
                };
                let is_pixel = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                );
                if !is_pixel {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                // Re-lecture après le push d'historique (emprunt neuf).
                if let Some(photo_engine::LayerNode::Pixel(pixels)) = self.document.find_mut(layer)
                {
                    pixels.filter_layers.push(filter);
                    pixels.touch();
                }
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::Export { path, quality } => {
                Mutation::immediate(match self.document.composite_preview() {
                    Some(image) => {
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_ascii_lowercase);
                        if ext.as_deref() == Some("gif") {
                            match image.save(&path) {
                                Ok(()) => PhotoEngineResponse::ExportDone { path },
                                Err(error) => PhotoEngineResponse::EngineError {
                                    message: format!("Export impossible : {error}"),
                                },
                            }
                        } else {
                            let format = match ext.as_deref() {
                                Some("jpg") | Some("jpeg") => photo_engine::ExportFormat::Jpeg {
                                    quality: quality.clamp(1, 100),
                                },
                                _ => photo_engine::ExportFormat::from_path(&path),
                            };
                            match photo_engine::export_image(&image, &path, format) {
                                Ok(()) => PhotoEngineResponse::ExportDone { path },
                                Err(error) => PhotoEngineResponse::EngineError {
                                    message: format!("Export impossible : {error}"),
                                },
                            }
                        }
                    }
                    None => PhotoEngineResponse::EngineError {
                        message: String::from("Rien a exporter (document vide)"),
                    },
                })
            }
            PhotoEngineCommand::RenameLayer { layer, name } => {
                let name = name.trim().to_owned();
                if name.is_empty() || self.document.find(layer).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.set_name_any(layer, name);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::AddMask { layer } => {
                // Validation sans emprunt persistant (le push d'historique
                // emprunte `self` en mutable : pas de borrow maintenu).
                // `masks_of_mut` (et non `masks_of`) : les ajustements
                // exposent une tranche vide mais refusent l'insertion.
                if self.document.masks_of_mut(layer).is_none() {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                let (width, height) = (self.document.width.max(1), self.document.height.max(1));
                if let Some(masks) = self.document.masks_of_mut(layer) {
                    let mut mask = photo_engine::LayerMask::full(width, height);
                    mask.name = format!("Masque {}", masks.len() + 1);
                    masks.push(mask);
                }
                touch_mask_owner(&mut self.document, layer);
                let dirty = self
                    .document
                    .find(layer)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::RemoveMask { owner, mask } => {
                let present = self
                    .document
                    .masks_of(owner)
                    .is_some_and(|masks| masks.iter().any(|m| m.id == mask));
                if !present {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                if let Some(masks) = self.document.masks_of_mut(owner) {
                    masks.retain(|m| m.id != mask);
                }
                touch_mask_owner(&mut self.document, owner);
                let dirty = self
                    .document
                    .find(owner)
                    .map_or(DirtyMark::Global, mark_for_node);
                Mutation::changed(invalidation).with_dirty(dirty)
            }
            PhotoEngineCommand::MoveFilter { layer, filter, up } => {
                // `move_filter` mute : valider d'abord via une lecture
                // immuable (position + bornes), puis historiser, puis muter.
                let movable = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        match pixels.filter_layers.iter().position(|f| f.id == filter) {
                            Some(idx) => {
                                let other = if up {
                                    idx.checked_add(1)
                                } else {
                                    idx.checked_sub(1)
                                };
                                other.is_some_and(|o| o < pixels.filter_layers.len())
                            }
                            None => false,
                        }
                    }
                    _ => false,
                };
                if !movable {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.move_filter(layer, filter, up);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::MoveMask { owner, mask, up } => {
                let movable = match self.document.masks_of(owner) {
                    Some(masks) => match masks.iter().position(|m| m.id == mask) {
                        Some(idx) => {
                            let other = if up {
                                idx.checked_sub(1)
                            } else {
                                idx.checked_add(1)
                            };
                            other.is_some_and(|o| o < masks.len())
                        }
                        None => false,
                    },
                    None => false,
                };
                if !movable {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.move_mask(owner, mask, up);
                touch_mask_owner(&mut self.document, owner);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::RemoveFilter { layer, filter } => {
                let present = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        pixels.filter_layers.iter().any(|f| f.id == filter)
                    }
                    _ => false,
                };
                if !present {
                    return Mutation::unchanged();
                }
                self.push_history(None);
                self.document.remove_filter(layer, filter);
                Mutation::changed(invalidation)
            }
            PhotoEngineCommand::Refresh => Mutation::changed(invalidation),
            PhotoEngineCommand::SetPreviewClip { clip } => {
                // Préférence d'affichage : aucun snapshot d'historique.
                // Pixels inchangés (même R ⇒ même rendu dans les deux modes) :
                // aucun marquage, la détection de scope couvre le reste.
                self.clip_to_doc = clip;
                Mutation::changed(invalidation).with_dirty(DirtyMark::None)
            }
        }
    }
}

/// Applique une commande à un document éphémère (tests, sans
/// worker persistant ni historique).
pub fn apply_command(document: &mut Document, command: PhotoEngineCommand) {
    let mut worker = EngineWorker::new(Document::new(0, 0));
    std::mem::swap(&mut worker.document, document);
    let _ = worker.apply(command);
    std::mem::swap(&mut worker.document, document);
}

/// Réduit un composite au plafond `PREVIEW_MAX_DIMENSION` SANS
/// jamais agrandir (`thumbnail` agrandit les petits composites, ce
/// qui gonfle la texture et fausse l'échelle miniature→document).
/// Miniature plafonnée + pixels d'entrée du rétrécissement (0 si pas de
/// shrink — compteur §8 6G.2, aucun effet sur les octets).
fn capped_preview(image: image::DynamicImage) -> (image::DynamicImage, u64) {
    let (w, h) = (image.width(), image.height());
    // Chemin rapide bit-exact (Phase 6G.2 §3.5D) : mêmes octets que
    // `thumbnail` (fuzz prouvé : dims + pixels), ~4× plus rapide, pool
    // dédié existant. Tout autre cas : legacy inchangé.
    if let image::DynamicImage::ImageRgba8(buf) = &image {
        if let Some((nw, nh)) =
            photo_engine::document::compositing::capped_thumbnail_dims(w, h, PREVIEW_MAX_DIMENSION)
        {
            let fast = photo_engine::document::compositing::fast_thumbnail_rgba8(buf, nw, nh);
            debug_assert_eq!((fast.width(), fast.height()), (nw, nh));
            return (
                image::DynamicImage::ImageRgba8(fast),
                u64::from(w) * u64::from(h),
            );
        }
        return (image, 0);
    }
    if w.max(h) <= PREVIEW_MAX_DIMENSION {
        (image, 0)
    } else {
        (
            image.thumbnail(PREVIEW_MAX_DIMENSION, PREVIEW_MAX_DIMENSION),
            u64::from(w) * u64::from(h),
        )
    }
}

/// Calcule l'aperçu composite (plafonné à `PREVIEW_MAX_DIMENSION`).
/// Coûteux : à appeler uniquement sur thread background.
/// `clip_to_doc` = aperçu rogné au document (sinon plan infini avec
/// l'origine du document dans `origin_*`).
pub fn render_preview(document: &Document, clip_to_doc: bool) -> Option<PreviewImage> {
    render_preview_timed(document, clip_to_doc).0
}

/// Variante instrumentée de [`render_preview`] (diagnostic perf) :
/// sépare le temps du composite pleine résolution et celui de la
/// miniature. Comportement fonctionnel identique.
pub fn render_preview_timed(
    document: &Document,
    clip_to_doc: bool,
) -> (Option<PreviewImage>, PreviewTimings) {
    let frame = AppearanceFrameCache::build(document);
    render_preview_timed_with(document, clip_to_doc, &frame)
}

/// Variante de [`render_preview_timed`] avec cadre d'apparences déjà
/// résolu : composite et géométrie partagent les mêmes `Arc` (zéro
/// nouvelle résolution). Comportement identique à parité de contenu.
fn render_preview_timed_with(
    document: &Document,
    clip_to_doc: bool,
    frame: &AppearanceFrameCache,
) -> (Option<PreviewImage>, PreviewTimings) {
    let doc_width = document.width.max(1);
    let doc_height = document.height.max(1);
    if clip_to_doc {
        let t_composite = std::time::Instant::now();
        let cropped = document.composite();
        let composite_us = t_composite.elapsed().as_micros();
        let Some(cropped) = cropped else {
            return (None, PreviewTimings::default());
        };
        let t_thumb = std::time::Instant::now();
        let (capped, _) = capped_preview(cropped);
        let raster = capped.to_rgba8();
        let (w, h) = (raster.width(), raster.height());
        let thumb_us = t_thumb.elapsed().as_micros();
        // `composite()` centre le document (extents symétriques) : le
        // cadre vaut le buffer sauf contenu plus petit que le document.
        return (
            Some(PreviewImage {
                width: w,
                height: h,
                rgba: raster.into_raw(),
                full_width: w,
                full_height: h,
                origin_x: (w as f32 - doc_width as f32) / 2.0,
                origin_y: (h as f32 - doc_height as f32) / 2.0,
                doc_width,
                doc_height,
            }),
            PreviewTimings {
                composite_us,
                thumb_us,
                geometry_us: 0,
                blend_us: 0,
                layers_blended: 0,
                pixels_processed: 0,
                scope_px: u64::from(w) * u64::from(h),
                full_px: u64::from(w) * u64::from(h),
                preview_px: u64::from(w) * u64::from(h),
                preview_resize_pixels: 0,
            },
        );
    }
    let t_composite = std::time::Instant::now();
    let mut composite_stats = photo_engine::document::compositing::CompositeStats::default();
    let composite =
        document.composite_preview_with_stats(&|id| frame.image(id), &mut composite_stats);
    let composite_us = t_composite.elapsed().as_micros();
    let Some(composite) = composite else {
        return (None, PreviewTimings::default());
    };
    let (full_w, full_h) = (composite.width(), composite.height());
    let t_thumb = std::time::Instant::now();
    let (preview, _) = capped_preview(composite);
    let raster = preview.to_rgba8();
    let (w, h) = (raster.width(), raster.height());
    let thumb_us = t_thumb.elapsed().as_micros();
    // Même miniature uniforme : la géométrie pleine résolution est
    // ramenée à l'échelle (demi-extents symétriques → origine exacte,
    // pas un recentrage entier approximatif).
    let t_geometry = std::time::Instant::now();
    let (origin_x, origin_y) = document
        .preview_geometry_with(&|id| frame.image(id))
        .map(|(_, _, ox, oy)| (ox, oy))
        .unwrap_or((
            (full_w.saturating_sub(doc_width)) as f32 / 2.0,
            (full_h.saturating_sub(doc_height)) as f32 / 2.0,
        ));
    let geometry_us = t_geometry.elapsed().as_micros();
    let kx = if full_w > 0 {
        w as f32 / full_w as f32
    } else {
        1.0
    };
    let ky = if full_h > 0 {
        h as f32 / full_h as f32
    } else {
        1.0
    };
    (
        Some(PreviewImage {
            width: w,
            height: h,
            rgba: raster.into_raw(),
            full_width: full_w,
            full_height: full_h,
            origin_x: origin_x * kx,
            origin_y: origin_y * ky,
            doc_width,
            doc_height,
        }),
        PreviewTimings {
            composite_us,
            thumb_us,
            geometry_us,
            blend_us: composite_stats.blend_us,
            layers_blended: composite_stats.layers_blended,
            pixels_processed: composite_stats.pixels_processed,
            scope_px: composite_stats.scope_px,
            full_px: u64::from(full_w) * u64::from(full_h),
            preview_px: u64::from(w) * u64::from(h),
            preview_resize_pixels: 0,
        },
    )
}

impl EngineWorker {
    /// Aperçu composite via le pipeline incrémental (Phase 6E) : le périmètre
    /// (document rogné ou plan infini) est découpé en tuiles rendues par
    /// `RenderWorker` (hits réutilisés, miss ⇒ `composite_region_with`
    /// fenêtré 6C), puis assemblé. Post-traitement (plafond, géométrie)
    /// IDENTIQUE au legacy : à contenu égal, mêmes octets.
    ///
    /// Retourne aussi les stats d'orchestration (tuiles demandées/hits/miss)
    /// pour `OpMetrics`. Le cache persiste entre les appels : une petite
    /// mutation ne re-rend que ses tuiles sales.
    fn render_preview_incremental(
        &mut self,
        frame: &AppearanceFrameCache,
    ) -> (
        Option<PreviewImage>,
        PreviewTimings,
        photo_engine::document::RenderWorkerStats,
    ) {
        let doc_width = self.document.width.max(1);
        let doc_height = self.document.height.max(1);
        let resolve = |id: Uuid| frame.image(id);
        // Périmètre + requête selon le mode (mêmes espaces que le legacy) :
        // rogné ⇒ viewport document ; plan infini ⇒ requête couvrant le
        // scope (DOCUMENT SPACE via `scope_request`). Échelle 1.0 : le zoom
        // d'affichage reste state-only côté canvas (AGENTS.md).
        let (scope, request) = if self.clip_to_doc {
            let scope = photo_engine::document::ScopeGeom::document(doc_width, doc_height);
            let request = photo_engine::document::RenderRequest::new(
                TileRegion::new(0, 0, doc_width, doc_height),
                1.0,
            );
            (scope, request)
        } else {
            let scope = self.render_worker.scope_for(&self.document, &resolve);
            (
                scope,
                photo_engine::document::RenderWorker::scope_request(scope, 1.0),
            )
        };
        let t_composite = std::time::Instant::now();
        let mut composite_stats = photo_engine::document::compositing::CompositeStats::default();
        let mut worker_stats = photo_engine::document::RenderWorkerStats::default();
        let assembled = self.render_worker.render(
            &self.document,
            &resolve,
            &mut composite_stats,
            scope,
            &request,
            &photo_engine::document::RenderPriorityContext::idle(),
            &mut worker_stats,
        );
        let composite_us = t_composite.elapsed().as_micros();
        let Some(assembled) = assembled else {
            return (None, PreviewTimings::default(), worker_stats);
        };
        // L'assemblage démarre à l'origine du viewport (0,0 en pratique :
        // viewports pleins) ; le legacy raisonne de même sur son buffer.
        let composite = assembled.image;
        if self.clip_to_doc {
            let t_thumb = std::time::Instant::now();
            let (preview, resize_px) = capped_preview(composite);
            let raster = preview.to_rgba8();
            let (w, h) = (raster.width(), raster.height());
            let thumb_us = t_thumb.elapsed().as_micros();
            return (
                Some(PreviewImage {
                    width: w,
                    height: h,
                    rgba: raster.into_raw(),
                    full_width: w,
                    full_height: h,
                    origin_x: (w as f32 - doc_width as f32) / 2.0,
                    origin_y: (h as f32 - doc_height as f32) / 2.0,
                    doc_width,
                    doc_height,
                }),
                PreviewTimings {
                    composite_us,
                    thumb_us,
                    geometry_us: 0,
                    blend_us: composite_stats.blend_us,
                    layers_blended: composite_stats.layers_blended,
                    pixels_processed: composite_stats.pixels_processed,
                    scope_px: worker_stats.dependency_px,
                    full_px: u64::from(w) * u64::from(h),
                    preview_px: u64::from(w) * u64::from(h),
                    preview_resize_pixels: resize_px,
                },
                worker_stats,
            );
        }
        let (full_w, full_h) = (composite.width(), composite.height());
        let t_thumb = std::time::Instant::now();
        let (preview, resize_px) = capped_preview(composite);
        let raster = preview.to_rgba8();
        let (w, h) = (raster.width(), raster.height());
        let thumb_us = t_thumb.elapsed().as_micros();
        let t_geometry = std::time::Instant::now();
        let (origin_x, origin_y) = self
            .document
            .preview_geometry_with(&|id| frame.image(id))
            .map(|(_, _, ox, oy)| (ox, oy))
            .unwrap_or((
                (full_w.saturating_sub(doc_width)) as f32 / 2.0,
                (full_h.saturating_sub(doc_height)) as f32 / 2.0,
            ));
        let geometry_us = t_geometry.elapsed().as_micros();
        let kx = if full_w > 0 {
            w as f32 / full_w as f32
        } else {
            1.0
        };
        let ky = if full_h > 0 {
            h as f32 / full_h as f32
        } else {
            1.0
        };
        (
            Some(PreviewImage {
                width: w,
                height: h,
                rgba: raster.into_raw(),
                full_width: full_w,
                full_height: full_h,
                origin_x: origin_x * kx,
                origin_y: origin_y * ky,
                doc_width,
                doc_height,
            }),
            PreviewTimings {
                composite_us,
                thumb_us,
                geometry_us,
                blend_us: composite_stats.blend_us,
                layers_blended: composite_stats.layers_blended,
                pixels_processed: composite_stats.pixels_processed,
                scope_px: worker_stats.dependency_px,
                full_px: u64::from(full_w) * u64::from(full_h),
                preview_px: u64::from(w) * u64::from(h),
                preview_resize_pixels: resize_px,
            },
            worker_stats,
        )
    }
}

/// Démarre le worker moteur sur un thread background.
///
/// Boucle bloquante `recv` (autorisée hors thread UI) : applique
/// chaque commande puis répond. La fin du `Sender` côté UI arrête
/// proprement le thread.
pub fn spawn_photo_engine_worker(
    document: Document,
    commands: Receiver<PhotoEngineCommand>,
    responses: Sender<PhotoEngineResponse>,
) -> JoinHandle<()> {
    assert_send::<Document>();
    std::thread::spawn(move || {
        let mut worker = EngineWorker::new(document);
        // Tâche secondaire threadée (Phase 6G.3) : sans cet appel, les
        // miniatures seraient calculées en inline sur le thread worker,
        // donc sur le chemin canvas-critical — exactement ce que le
        // ThumbWorker existe pour éviter.
        worker.enable_thumb_thread();
        // Boucle par lots : la première commande bloque (`recv`), puis
        // tout ce qui est déjà en attente est coalescé (`try_recv`) et
        // ne produit qu'UN rendu (latest-value-wins). Les commandes
        // structurelles restent appliquées une par une, dans l'ordre.
        'worker: while let Ok(first) = commands.recv() {
            let mut batch = vec![first];
            while let Ok(next) = commands.try_recv() {
                batch.push(next);
            }
            for response in worker.apply_batch(batch) {
                if responses.send(response).is_err() {
                    break 'worker;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::{LayerNode, PixelLayer};
    use std::sync::Arc;
    use std::sync::mpsc::channel;

    fn test_image() -> Arc<image::DynamicImage> {
        Arc::new(image::DynamicImage::new_rgba8(4, 4))
    }

    fn three_layer_doc() -> Document {
        let mut doc = Document::new(8, 8);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("milieu", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("dessus", test_image())));
        doc
    }

    #[test]
    fn display_index_conversion() {
        assert_eq!(display_to_doc_index(0, 3), Some(2));
        assert_eq!(display_to_doc_index(2, 3), Some(0));
        assert_eq!(display_to_doc_index(3, 3), None);
        assert_eq!(display_to_doc_index(0, 0), None);
    }

    #[test]
    fn toggle_visibility_command() {
        let mut doc = three_layer_doc();
        let id = doc.root[0].id();
        assert!(doc.find(id).expect("present").visible());
        apply_command(&mut doc, PhotoEngineCommand::ToggleLayerVisibility(id));
        assert!(!doc.find(id).expect("present").visible());
        apply_command(&mut doc, PhotoEngineCommand::ToggleLayerVisibility(id));
        assert!(doc.find(id).expect("present").visible());
    }

    #[test]
    fn reorder_command_moves_display_top_to_bottom() {
        // Affichage [dessus, milieu, fond] ; (0 -> 3) = "dessus" en bas.
        let mut doc = three_layer_doc();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 0, to: 3 },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);
    }

    #[test]
    fn reorder_command_bottom_to_top() {
        // (2 -> 0) = "fond" tout en haut.
        let mut doc = three_layer_doc();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 2, to: 0 },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["fond", "dessus", "milieu"]);
    }

    #[test]
    fn invalid_commands_are_noops() {
        let mut doc = three_layer_doc();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ToggleLayerVisibility(Uuid::new_v4()),
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 9, to: 0 },
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 1, to: 1 },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["dessus", "milieu", "fond"]);
        assert!(snapshot_layers(&doc).iter().all(|l| l.visible));
    }

    #[test]
    fn set_blend_mode_snapshots_and_undoes() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::SetBlendMode {
            layer: id,
            mode: BlendMode::Multiply,
        });
        assert_eq!(
            worker.document.find(id).expect("present").blend_mode(),
            Some(BlendMode::Multiply)
        );
        assert_eq!(worker.undo.len(), 1);
        // Id inconnu : sans effet, sans snapshot.
        worker.apply(PhotoEngineCommand::SetBlendMode {
            layer: Uuid::new_v4(),
            mode: BlendMode::Screen,
        });
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(
            worker.document.find(id).expect("present").blend_mode(),
            Some(BlendMode::Normal)
        );
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        assert_eq!(
            layers
                .iter()
                .find(|l| l.id == id)
                .expect("present")
                .blend_mode,
            BlendMode::Multiply
        );
    }

    #[test]
    fn set_opacity_clamps_and_snapshots() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 250.0,
        });
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        // Un seul snapshot malgré 3 appels (coalescence du slider).
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 10.0,
        });
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 20.0,
        });
        assert_eq!(worker.undo.len(), 1);
        // Undo restaure l'opacité d'origine (100 par défaut moteur).
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        assert_eq!(
            layers.iter().find(|l| l.id == id).expect("present").opacity,
            100.0
        );
    }

    #[test]
    fn undo_redo_cycle() {
        let mut worker = EngineWorker::new(three_layer_doc());
        // Undo sans historique : état sans rendu (pas de composite).
        let response = worker.apply(PhotoEngineCommand::Undo);
        assert!(matches!(
            response,
            PhotoEngineResponse::StateChanged {
                can_undo: false,
                can_redo: false,
                ..
            }
        ));
        assert_eq!(worker.metrics.renders, 0);
        worker.apply(PhotoEngineCommand::ReorderLayer { from: 0, to: 3 });
        worker.apply(PhotoEngineCommand::Undo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["dessus", "milieu", "fond"]);
        worker.apply(PhotoEngineCommand::Redo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);
    }

    #[test]
    fn open_missing_image_is_engine_error() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::OpenImage {
            path: PathBuf::from("/chemin/inexistant/photo.png"),
        });
        assert!(matches!(response, PhotoEngineResponse::EngineError { .. }));
        assert_eq!(worker.document.root.len(), 3);
    }

    #[test]
    fn preview_present_after_command() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::ToggleLayerVisibility(
            worker.document.root[0].id(),
        ));
        let PhotoEngineResponse::LayersChanged { preview, .. } = response else {
            panic!("réponse attendue");
        };
        let preview = preview.expect("apercu genere");
        assert_eq!(
            preview.rgba.len(),
            preview.width as usize * preview.height as usize * 4
        );
    }

    #[test]
    fn worker_roundtrip_over_channels() {
        let doc = three_layer_doc();
        let (cmd_tx, cmd_rx) = channel();
        let (resp_tx, resp_rx) = channel();
        let handle = spawn_photo_engine_worker(doc, cmd_rx, resp_tx);

        // Le thread UI ne bloque jamais : poll immédiat vide…
        assert!(resp_rx.try_recv().is_err());
        cmd_tx
            .send(PhotoEngineCommand::ReorderLayer { from: 0, to: 3 })
            .expect("envoi commande");
        // …puis réponse du worker (opération lourde hors UI).
        let response = resp_rx.recv().expect("reponse worker");
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        let names: Vec<String> = layers.iter().map(|l| l.name.clone()).collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);

        drop(cmd_tx);
        handle.join().expect("arret propre du worker");
    }

    #[test]
    fn rename_trims_and_rejects_empty_or_unknown() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("  fond clair  "),
        });
        assert_eq!(
            worker.document.find(id).expect("present").name(),
            "fond clair"
        );
        assert_eq!(worker.undo.len(), 1);
        // Vide ou inconnu : no-op sans snapshot.
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("   "),
        });
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: Uuid::new_v4(),
            name: String::from("fantome"),
        });
        assert_eq!(
            worker.document.find(id).expect("present").name(),
            "fond clair"
        );
        assert_eq!(worker.undo.len(), 1);
        // Undo restaure le nom d'origine.
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.find(id).expect("present").name(), "fond");
    }

    #[test]
    fn mask_add_remove_roundtrip_with_undo() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::AddMask { layer: id });
        let masks = worker.document.masks_of(id).expect("masques");
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0].name, "Masque 1");
        let mask_id = masks[0].id;
        // Snapshot UI : le masque apparaît dans la HUD.
        let layers = snapshot_layers(&worker.document);
        let info = layers.iter().find(|l| l.id == id).expect("present");
        assert!(info.has_masks);
        assert_eq!(info.masks.len(), 1);
        // Bornes : déplacer un masque seul = no-op.
        worker.apply(PhotoEngineCommand::MoveMask {
            owner: id,
            mask: mask_id,
            up: true,
        });
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
        // Suppression puis undo.
        worker.apply(PhotoEngineCommand::RemoveMask {
            owner: id,
            mask: mask_id,
        });
        assert!(worker.document.masks_of(id).expect("masques").is_empty());
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
        // Porteur inconnu : no-op.
        worker.apply(PhotoEngineCommand::AddMask {
            layer: Uuid::new_v4(),
        });
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
    }

    #[test]
    fn filter_move_and_remove() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        for type_id in ["brightness_contrast", "blur"] {
            let filter = photo_engine::new_filter_layer(type_id).expect("filtre connu");
            worker.document.add_filter(id, filter);
        }
        // Ordre d'application : [brightness, blur] ; monter le premier.
        let first = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels.filter_layers[0].id,
            _ => panic!("pixels attendus"),
        };
        worker.apply(PhotoEngineCommand::MoveFilter {
            layer: id,
            filter: first,
            up: true,
        });
        let order: Vec<String> = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels
                .filter_layers
                .iter()
                .map(|f| f.type_id.clone())
                .collect(),
            _ => panic!("pixels attendus"),
        };
        assert_eq!(order, ["blur", "brightness_contrast"]);
        // Borne haute : no-op.
        worker.apply(PhotoEngineCommand::MoveFilter {
            layer: id,
            filter: first,
            up: true,
        });
        // Suppression.
        worker.apply(PhotoEngineCommand::RemoveFilter {
            layer: id,
            filter: first,
        });
        let remaining = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels.filter_layers.len(),
            _ => panic!("pixels attendus"),
        };
        assert_eq!(remaining, 1);
    }

    #[test]
    fn move_layer_translates_pixel_transform_with_undo() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 12.0,
            dy: -5.0,
        });
        let moved = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => pixels.transform,
            _ => panic!("pixels attendus"),
        };
        assert_eq!((moved.offset_x, moved.offset_y), (12.0, -5.0));
        assert_eq!(worker.undo.len(), 1);
        // Delta nul ou id inconnu : sans effet, sans snapshot.
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 0.0,
            dy: 0.0,
        });
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: Uuid::new_v4(),
            dx: 3.0,
            dy: 3.0,
        });
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        let restored = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => pixels.transform,
            _ => panic!("pixels attendus"),
        };
        assert_eq!((restored.offset_x, restored.offset_y), (0.0, 0.0));
    }

    #[test]
    fn preview_clip_crops_to_document_without_history() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        // Calque hors cadre : l'aperçu plan infini dépasse le document.
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 6.0,
            dy: 0.0,
        });
        let wide = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("apercu"),
            PhotoEngineResponse::EngineError { message } => panic!("{message}"),
            PhotoEngineResponse::ExportDone { .. } => panic!("export inattendu"),
            PhotoEngineResponse::StateChanged { .. } => panic!("rendu attendu"),
            PhotoEngineResponse::ThumbnailUpdated { .. } => {
                panic!("pas de miniature seule sur apply solo")
            }
        };
        assert!(wide.full_width > wide.doc_width, "plan infini plus large");
        assert!((wide.origin_x - 2.0).abs() < 1.0, "origine exacte");
        let undo_len = worker.undo.len();
        // Rognage : aperçu aux dimensions du document, sans historique.
        worker.apply(PhotoEngineCommand::SetPreviewClip { clip: true });
        assert_eq!(worker.undo.len(), undo_len, "pas de snapshot");
        let clipped = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("apercu rogne"),
            other => panic!("reponse inattendue : {other:?}"),
        };
        assert_eq!(clipped.full_width, clipped.doc_width);
        assert_eq!(clipped.full_height, clipped.doc_height);
    }

    #[test]
    fn export_png_and_gif_write_files() {
        let dir = std::env::temp_dir().join(format!("cygnus-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let mut worker = EngineWorker::new(three_layer_doc());
        for ext in ["png", "jpg", "gif"] {
            let path = dir.join(format!("rendu.{ext}"));
            let response = worker.apply(PhotoEngineCommand::Export {
                path: path.clone(),
                quality: 80,
            });
            assert!(
                matches!(response, PhotoEngineResponse::ExportDone { .. }),
                "export {ext} attendu"
            );
            assert!(path.is_file(), "fichier {ext} écrit");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_routing_matches_semantics() {
        use RenderEvent::{FullInvalidation, NodeInvalidated};
        use RenderInvalidation::{Composite, StateOnly};
        let id = Uuid::new_v4();
        // Structurel : invalidation composite totale.
        assert_eq!(
            render_routing(&PhotoEngineCommand::ReorderLayer { from: 0, to: 1 }),
            (FullInvalidation, Composite)
        );
        // Visibilité : affecte le blending global (affects_composite).
        assert_eq!(
            render_routing(&PhotoEngineCommand::ToggleLayerVisibility(id)),
            (FullInvalidation, Composite)
        );
        // Opacité : nœud isolé au sens RenderEvent, mais le composite
        // CPU doit être re-blendé → Composite.
        assert_eq!(
            render_routing(&PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: 50.0
            }),
            (NodeInvalidated(id), Composite)
        );
        // Renommage : seul l'état change, aucun pixel.
        assert_eq!(
            render_routing(&PhotoEngineCommand::RenameLayer {
                layer: id,
                name: String::from("x")
            }),
            (NodeInvalidated(id), StateOnly)
        );
    }

    #[test]
    fn batch_opacity_coalesces_to_single_render() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let gestures: Vec<PhotoEngineCommand> = (1..=20)
            .map(|i| PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: i as f32 * 4.0,
            })
            .collect();
        let responses = worker.apply_batch(gestures);
        // 20 événements intermédiaires → 1 seule réponse, 1 seul rendu.
        assert_eq!(responses.len(), 1, "un seul rendu par geste");
        assert_eq!(worker.metrics.renders, 1);
        let revision = match &responses[0] {
            PhotoEngineResponse::LayersChanged {
                revision,
                preview: Some(_),
                ..
            } => *revision,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(revision, RenderRevision(1));
        // Dernière valeur gagnante, une seule entrée d'historique.
        assert_eq!(worker.document.find(id).expect("present").opacity(), 80.0);
        assert_eq!(worker.undo.len(), 1);
    }

    #[test]
    fn batch_opacity_undo_restores_initial_value() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let gestures: Vec<PhotoEngineCommand> = (1..=20)
            .map(|i| PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: i as f32 * 4.0,
            })
            .collect();
        worker.apply_batch(gestures);
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(
            worker.document.find(id).expect("present").opacity(),
            100.0,
            "undo restaure la valeur initiale du geste"
        );
        worker.apply(PhotoEngineCommand::Redo);
        assert_eq!(
            worker.document.find(id).expect("present").opacity(),
            80.0,
            "redo réapplique la valeur finale"
        );
    }

    #[test]
    fn batch_move_sums_deltas_with_single_history_entry() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let responses = worker.apply_batch(vec![
            PhotoEngineCommand::MoveLayer {
                layer: id,
                dx: 5.0,
                dy: 0.0,
            },
            PhotoEngineCommand::MoveLayer {
                layer: id,
                dx: -2.0,
                dy: 1.0,
            },
        ]);
        assert_eq!(responses.len(), 1);
        assert_eq!(worker.metrics.renders, 1);
        let offset = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => {
                (pixels.transform.offset_x, pixels.transform.offset_y)
            }
            _ => panic!("pixels attendus"),
        };
        assert_eq!(offset, (3.0, 1.0));
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        let restored = match worker.document.find(id).expect("present") {
            photo_engine::LayerNode::Pixel(pixels) => {
                (pixels.transform.offset_x, pixels.transform.offset_y)
            }
            _ => panic!("pixels attendus"),
        };
        assert_eq!(restored, (0.0, 0.0));
    }

    #[test]
    fn rename_produces_state_without_render() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        // D'abord un rendu pour fixer la révision à 1.
        worker.apply(PhotoEngineCommand::Refresh);
        assert_eq!(worker.metrics.renders, 1);
        let response = worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("renomme"),
        });
        match response {
            PhotoEngineResponse::StateChanged {
                layers, revision, ..
            } => {
                assert_eq!(revision, RenderRevision(1), "pas de nouveau rendu");
                assert!(
                    layers.iter().any(|l| l.id == id && l.name == "renomme"),
                    "snapshot à jour"
                );
            }
            other => panic!("StateChanged attendu, obtenu : {other:?}"),
        }
        assert_eq!(worker.metrics.renders, 1, "aucun rendu pour un renommage");
    }

    #[test]
    fn batch_keeps_export_and_error_responses() {
        let dir = std::env::temp_dir().join(format!("cygnus-batch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let path = dir.join("rendu.png");
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let responses = worker.apply_batch(vec![
            PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: 50.0,
            },
            PhotoEngineCommand::Export {
                path: path.clone(),
                quality: 80,
            },
            PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: 60.0,
            },
        ]);
        // ExportDone immédiat + un seul état final (opacités repliées).
        assert_eq!(responses.len(), 2);
        assert!(matches!(
            responses[0],
            PhotoEngineResponse::ExportDone { .. }
        ));
        assert!(matches!(
            responses[1],
            PhotoEngineResponse::LayersChanged { .. }
        ));
        assert_eq!(worker.metrics.renders, 1);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 60.0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mutation_does_not_invalidate_other_layers_caches() {
        let mut worker = EngineWorker::new(three_layer_doc());
        worker.apply(PhotoEngineCommand::Refresh);
        let before: Vec<(uuid::Uuid, u64)> = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers
                .iter()
                .map(|l| (l.id, l.thumb.as_ref().map(|t| t.version).unwrap_or(0)))
                .collect(),
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let after = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        for (id, version) in before {
            if id == target {
                continue;
            }
            let current = after
                .iter()
                .find(|l| l.id == id)
                .and_then(|l| l.thumb.as_ref().map(|t| t.version));
            assert_eq!(
                current,
                Some(version),
                "le cache des autres calques est intact"
            );
        }
    }
}

#[cfg(test)]
mod fold_batch_tests {
    use super::*;

    fn opacity(id: Uuid, value: f32) -> PhotoEngineCommand {
        PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: value,
        }
    }

    fn shifted(id: Uuid, dx: f32, dy: f32) -> PhotoEngineCommand {
        PhotoEngineCommand::MoveLayer { layer: id, dx, dy }
    }

    #[test]
    fn consecutive_same_layer_opacity_keeps_last() {
        let id = Uuid::new_v4();
        let folded = fold_batch(vec![
            opacity(id, 10.0),
            opacity(id, 20.0),
            opacity(id, 30.0),
        ]);
        assert_eq!(folded, vec![opacity(id, 30.0)]);
    }

    #[test]
    fn opacity_does_not_fold_across_other_commands() {
        let id = Uuid::new_v4();
        let undo = PhotoEngineCommand::Undo;
        // Undo interrompt le repli : les trois commandes survivent.
        let folded = fold_batch(vec![opacity(id, 10.0), undo, opacity(id, 30.0)]);
        assert_eq!(folded.len(), 3);
        // Deux calques différents ne se replient pas non plus.
        let other = Uuid::new_v4();
        let folded = fold_batch(vec![opacity(id, 10.0), opacity(other, 20.0)]);
        assert_eq!(folded.len(), 2);
    }

    #[test]
    fn consecutive_moves_sum_and_zero_sum_is_dropped() {
        let id = Uuid::new_v4();
        let folded = fold_batch(vec![shifted(id, 5.0, 0.0), shifted(id, -2.0, 1.0)]);
        assert_eq!(folded, vec![shifted(id, 3.0, 1.0)]);
        // Somme nulle : commande supprimée (aucun rendu à produire).
        let folded = fold_batch(vec![shifted(id, 5.0, 0.0), shifted(id, -5.0, 0.0)]);
        assert!(folded.is_empty());
    }

    #[test]
    fn noop_batch_returns_single_state_response() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        // Que des no-ops : une seule resynchronisation, aucun rendu.
        let responses = worker.apply_batch(vec![
            PhotoEngineCommand::ToggleLayerVisibility(Uuid::new_v4()),
            PhotoEngineCommand::DeleteLayer(Uuid::new_v4()),
        ]);
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            PhotoEngineResponse::StateChanged { .. }
        ));
        assert_eq!(worker.metrics.renders, 0);
    }

    #[test]
    fn batch_with_undo_inside_preserves_order() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let id = worker.document.root[0].id();
        // Opacité appliquée PUIS annulée dans le même lot : retour à 100.
        let responses = worker.apply_batch(vec![
            opacity(id, 10.0),
            opacity(id, 20.0),
            PhotoEngineCommand::Undo,
        ]);
        assert_eq!(responses.len(), 1);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        // L'historique garde la trace : redo rejoue la valeur repliée.
        worker.apply(PhotoEngineCommand::Redo);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 20.0);
    }

    fn three_layer_doc_for_fold() -> Document {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        let mut doc = Document::new(8, 8);
        for name in ["fond", "milieu", "dessus"] {
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(name, image.clone())));
        }
        doc
    }
}

#[cfg(test)]
mod single_pass_tests {
    use super::*;

    fn layers_of(response: &PhotoEngineResponse) -> Vec<PhotoLayerInfo> {
        match response {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers.clone(),
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
    }

    fn thumb_version(layer: &PhotoLayerInfo) -> u64 {
        layer.thumb.as_ref().map(|t| t.version).unwrap_or(0)
    }

    #[test]
    fn opacity_shares_one_resolution_per_layer_without_rebuilds() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 50.0,
        });
        let m = worker.metrics.last.as_ref().expect("metriques");
        // 3 calques → 3 résolutions image, partagées entre les consommateurs :
        // extents scope (3, Phase 6E) + extents composite (3) + fold (3) +
        // extents géométrie (3) + contributes (1, sortie précoce au 1er
        // calque). Le snapshot ne résout PLUS les miniatures ( Phase 6G.3 :
        // store secondaire) — d'où 13 et non 16.
        assert_eq!(m.appearance_resolves, 3);
        assert_eq!(m.appearance_frame_hits, 13);
        // Aucun pixel recalculé : ni preview ni thumb reconstruits.
        assert_eq!(m.preview_rebuilds, 0);
        assert_eq!(m.thumb_rebuilds, 0);
        // Le composite lui-même reste produit (re-blend, révision +1).
        assert_eq!(worker.metrics.renders, 2);
    }

    #[test]
    fn paint_rebuilds_only_touched_layer() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let before = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        let after = layers_of(&worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        }));
        // Calque peint : nouvelle version ; les autres : intactes.
        for layer in &after {
            let old = before
                .iter()
                .find(|l| l.id == layer.id)
                .map(thumb_version)
                .unwrap_or(0);
            if layer.id == target {
                assert_ne!(thumb_version(layer), old, "peinture invalide le calque");
            } else {
                assert_eq!(thumb_version(layer), old, "autres calques intacts");
            }
        }
        // Tâche secondaire (inline ici) : seule la miniature du calque peint
        // est recalculée — versions live intactes par ailleurs, cadre 6G.3
        // sans `preview` (6G.2) ni `thumb` synchrone.
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.preview_rebuilds, 0);
        assert_eq!(m.thumb_rebuilds, 0);
        assert_eq!(m.thumbnail_jobs_completed, 1, "seul le calque peint");
    }

    /// Document large (640x480) pour les tests tuiles multi-cases
    /// (tuiles d'observation : 256 px, niveau 0 unique).
    fn wide_doc() -> Document {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let mut doc = Document::new(640, 480);
        let blank = Arc::new(image::DynamicImage::new_rgba8(640, 480));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", blank)));
        doc
    }

    #[test]
    fn paint_stroke_observes_tile_plan() {
        // Cas 1+5 : petit stroke → région exacte + 1 tuile + plan à 1.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        // Rendu inchangé : composite produit, révision bumpée comme avant.
        match response {
            PhotoEngineResponse::LayersChanged { revision, .. } => {
                assert_eq!(revision, RenderRevision(1));
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        // Observation : bbox [−2,5)² → 49 px², doc 8x8 → tuile (0,0) unique.
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.dirty_region_px, 49);
        assert_eq!(m.dirty_tiles, 1);
        assert_eq!(m.scheduled_tiles, 1);
    }

    #[test]
    fn paint_stroke_multi_tile_plan() {
        // Cas 2 : stroke à cheval sur 2x2 tuiles de 256 px.
        let mut worker = EngineWorker::new(wide_doc());
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(10.0, 10.0), (500.0, 400.0)],
            eraser: false,
            radius: 8.0,
            color: [255, 0, 0],
            opacity: 1.0,
        });
        assert!(
            matches!(response, PhotoEngineResponse::LayersChanged { .. }),
            "rendu produit comme avant"
        );
        // x ∈ [2,508) → tuiles 0,1 ; y ∈ [2,408) → tuiles 0,1.
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.dirty_region_px, 506 * 406);
        assert_eq!(m.dirty_tiles, 4);
        assert_eq!(m.scheduled_tiles, 4, "budget ouvert : tout planifié");
    }

    #[test]
    fn stroke_outside_document_marks_no_tiles() {
        // Cas 3 : région réelle mais hors surface → clipping grille,
        // aucune tuile invalide (et surtout aucune tuile fantôme).
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![(5000.0, 5000.0)],
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        assert!(
            matches!(response, PhotoEngineResponse::LayersChanged { .. }),
            "peinture commise (pixels inchangés hors calque)"
        );
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert!(m.dirty_region_px > 0, "région calculée");
        assert_eq!(m.dirty_tiles, 0);
        assert_eq!(m.scheduled_tiles, 0);
    }

    #[test]
    fn invalid_stroke_observes_nothing() {
        // Cas 4 : geste inexploitable → no-op, aucun travail tuile.
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let target = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points: vec![],
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        assert!(
            matches!(response, PhotoEngineResponse::StateChanged { .. }),
            "no-op sans rendu"
        );
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.dirty_region_px, 0);
        assert_eq!(m.dirty_tiles, 0);
        assert_eq!(m.scheduled_tiles, 0);
    }

    #[test]
    fn filter_change_rebuilds_only_concerned_layer() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        let target = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::AddFilter {
            layer: target,
            filter_type: String::from("brightness_contrast"),
        });
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.preview_rebuilds, 0, "cadre 6G.2 : pas de preview dérivé");
        assert_eq!(m.thumb_rebuilds, 0, "cadre 6G.3 : miniature en tâche");
        assert_eq!(
            m.thumbnail_jobs_completed, 1,
            "seule la chaîne du calque rejoue (secondaire)"
        );
    }

    #[test]
    fn undo_redo_revalidates_versions() {
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let pristine = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        let target = worker.document.root[0].id();
        let pristine_thumb = pristine
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        let points: Vec<(f32, f32)> = (0..4).map(|i| (i as f32, i as f32)).collect();
        let painted = layers_of(&worker.apply(PhotoEngineCommand::PaintStroke {
            layer: target,
            points,
            eraser: false,
            radius: 2.0,
            color: [0, 0, 255],
            opacity: 1.0,
        }));
        let painted_thumb = painted
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        assert_ne!(painted_thumb.version, pristine_thumb.version);
        // Undo : pixels et version d'origine restaurés (rebuild ciblé).
        let undone = layers_of(&worker.apply(PhotoEngineCommand::Undo));
        let undone_thumb = undone
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        assert_eq!(undone_thumb.version, pristine_thumb.version);
        assert_eq!(undone_thumb.rgba, pristine_thumb.rgba);
        // Redo : retour exact à l'état peint.
        let redone = layers_of(&worker.apply(PhotoEngineCommand::Redo));
        let redone_thumb = redone
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.thumb.clone())
            .expect("miniature");
        assert_eq!(redone_thumb.version, painted_thumb.version);
        assert_eq!(redone_thumb.rgba, painted_thumb.rgba);
    }

    #[test]
    fn export_neither_renders_nor_bumps_revision() {
        let dir = std::env::temp_dir().join(format!("cygnus-sp-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        worker.apply(PhotoEngineCommand::Refresh);
        assert_eq!(worker.metrics.renders, 1);
        let path = dir.join("rendu.png");
        let response = worker.apply(PhotoEngineCommand::Export {
            path: path.clone(),
            quality: 80,
        });
        assert!(matches!(response, PhotoEngineResponse::ExportDone { .. }));
        assert!(path.is_file(), "fichier écrit");
        assert_eq!(worker.metrics.renders, 1, "export sans rendu");
        // Le refresh suivant bump d'exactement une révision.
        match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { revision, .. } => {
                assert_eq!(revision, RenderRevision(2));
            }
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hidden_layer_keeps_thumb_and_resolves_once() {
        // Les calques masqués ne contribuent pas au composite mais
        // gardent leur miniature (parité avec l'ancien chemin thumb()).
        let mut worker = EngineWorker::new(three_layer_doc_for_fold());
        let hidden = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(hidden));
        let layers = match worker.apply(PhotoEngineCommand::Refresh) {
            PhotoEngineResponse::LayersChanged { layers, .. } => layers,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        let info = layers.iter().find(|l| l.id == hidden).expect("calque");
        assert!(!info.visible);
        assert!(info.thumb.is_some(), "miniature conservée même masqué");
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(
            m.appearance_resolves, 3,
            "tous les calques résolus une fois"
        );
        assert_eq!(m.preview_rebuilds, 0);
        assert_eq!(m.thumb_rebuilds, 0);
    }

    fn three_layer_doc_for_fold() -> Document {
        use photo_engine::{LayerNode, PixelLayer};
        use std::sync::Arc;
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        let mut doc = Document::new(8, 8);
        for name in ["fond", "milieu", "dessus"] {
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(name, image.clone())));
        }
        doc
    }

    // ------------------------------------------------------------------
    // Phase 6E — pipeline incrémental réellement utilisé par le worker.
    // ------------------------------------------------------------------

    fn painted_doc() -> (Document, Uuid) {
        use photo_engine::{LayerNode, PixelLayer};
        // Document 64² avec contenu contrasté (bords francs) : fond opaque +
        // carré décalé.
        let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            64,
            64,
            image::Rgba([200, 40, 40, 255]),
        ));
        let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            24,
            24,
            image::Rgba([40, 200, 40, 255]),
        ));
        let mut doc = Document::new(64, 64);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
        let mut vert = PixelLayer::new("vert", Arc::new(carre));
        vert.transform.offset_x = 20.0;
        vert.transform.offset_y = 12.0;
        let id = vert.id;
        doc.push_layer(LayerNode::Pixel(vert));
        (doc, id)
    }

    #[test]
    fn incremental_preview_matches_legacy_clip_et_infini() {
        // Les deux modes : incrémental (worker) == legacy (pleine cadre).
        for clip in [true, false] {
            let (doc, _) = painted_doc();
            let mut worker = EngineWorker::new(doc);
            worker.apply(PhotoEngineCommand::SetPreviewClip { clip });
            // Référence legacy sur le même état document.
            let legacy = render_preview(worker.test_document(), clip)
                .expect("legacy")
                .rgba;
            // Forcer le re-rendu incrémental (le clip a déjà rendu une fois).
            worker.render_worker.cache_mut().invalidate_all();
            let frame = AppearanceFrameCache::build(worker.test_document());
            let (preview, _, _) = worker.render_preview_incremental(&frame);
            assert_eq!(
                preview.expect("incrémental").rgba,
                legacy,
                "clip={clip} : mêmes octets"
            );
        }
    }

    fn painted_big_doc() -> (Document, Uuid) {
        use photo_engine::{LayerNode, PixelLayer};
        // Document 300² (4 tuiles à 256 px) : fond identité + carré.
        let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            300,
            300,
            image::Rgba([200, 40, 40, 255]),
        ));
        let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            60,
            60,
            image::Rgba([40, 200, 40, 255]),
        ));
        let mut doc = Document::new(300, 300);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
        let mut vert = PixelLayer::new("vert", Arc::new(carre));
        vert.transform.offset_x = 200.0;
        vert.transform.offset_y = 200.0;
        doc.push_layer(LayerNode::Pixel(vert));
        let fond_id = doc.root[0].id();
        (doc, fond_id)
    }

    #[test]
    fn incremental_warm_reuse_apres_second_stroke() {
        // Deuxième peinture loin : tuiles propres réutilisées (hits > 0),
        // aperçu identique au legacy pleine cadre.
        let (doc, fond) = painted_big_doc();
        let mut worker = EngineWorker::new(doc);
        let stroke = |worker: &mut EngineWorker, points: Vec<(f32, f32)>| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points,
                eraser: false,
                radius: 3.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        stroke(&mut worker, vec![(10.0, 10.0), (20.0, 20.0)]);
        let first = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(first.cache_misses, 4, "premier rendu froid : 4 tuiles");
        stroke(&mut worker, vec![(270.0, 270.0), (280.0, 280.0)]);
        let second = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(second.cache_misses, 1, "seule la tuile sale est réévaluée");
        assert_eq!(second.cache_hits, 3, "tuiles propres réutilisées");
        let legacy = render_preview(worker.test_document(), false)
            .expect("legacy")
            .rgba;
        // Dernier aperçu incrémental == legacy (via une relecture forcée).
        worker.render_worker.cache_mut().invalidate_all();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        assert_eq!(preview.expect("incrémental").rgba, legacy);
    }

    #[test]
    fn incremental_scope_change_reste_correct() {
        // Calque déplacé hors document : scope changé ⇒ invalidation totale,
        // aperçu toujours égal au legacy.
        let (doc, id) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 500.0,
            dy: 0.0,
        });
        let legacy = render_preview(worker.test_document(), false)
            .expect("legacy")
            .rgba;
        worker.render_worker.cache_mut().invalidate_all();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        assert_eq!(preview.expect("incrémental").rgba, legacy);
    }

    #[test]
    fn surface_pixels_written_partiel_de_bout_en_bout() {
        // Phase 6G : premier stroke ⇒ surface pleine (300²) ; second loin ⇒
        // seule la tuile (256,256,44,44) est réécrite.
        let (doc, fond) = painted_big_doc();
        let mut worker = EngineWorker::new(doc);
        let stroke = |worker: &mut EngineWorker, points: Vec<(f32, f32)>| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points,
                eraser: false,
                radius: 3.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        stroke(&mut worker, vec![(10.0, 10.0), (20.0, 20.0)]);
        let first = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(first.surface_pixels_written, 300 * 300);
        stroke(&mut worker, vec![(270.0, 270.0), (280.0, 280.0)]);
        let second = worker.metrics.last.as_ref().expect("metriques").clone();
        assert_eq!(second.surface_pixels_written, 44 * 44);
        assert_eq!(second.cache_hits, 3);
    }

    #[test]
    fn appearance_frame_rebuild_seulement_touche() {
        // §2.7 6G.2/6G.3 : paint A → miniature secondaire de A seule (B hit),
        // cadre sans preview ni thumb synchrone ; SetOpacity A (source
        // inchangée) → 0 rebuild. Pixels == legacy dans tous les cas.
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(20.0, 20.0), (36.0, 20.0)],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let m = last_metrics(&worker);
        assert_eq!(m.thumb_rebuilds, 0, "cadre 6G.3 : miniature en tâche");
        assert_eq!(m.preview_rebuilds, 0, "cadre sans preview");
        assert_eq!(m.thumbnail_jobs_completed, 1, "seul A en tâche");
        assert_eq!(m.appearance_resolves, 2, "A + B images résolues une fois");
        let legacy = render_preview(worker.test_document(), false)
            .expect("legacy")
            .rgba;
        worker.render_worker.cache_mut().invalidate_all();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        assert_eq!(preview.expect("incrémental").rgba, legacy);
        // Opacité : la source ne change pas ⇒ hit, aucun rebuild.
        let carre = worker.test_document().root[1].id();
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: carre,
            opacity: 50.0,
        });
        let m = last_metrics(&worker);
        assert_eq!(m.thumb_rebuilds, 0, "aucune chaîne rejouée");
        assert_eq!(m.preview_rebuilds, 0);
    }

    #[test]
    fn marquage_paint_precis_pas_global() {
        // La peinture marque une région bornée, pas tout le document.
        let (doc, id) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        // `mutate` seul (sans `respond`) : le marquage est observable avant
        // que la détection de scope du premier rendu ne l'absorbe.
        let _ = worker.mutate(PhotoEngineCommand::PaintStroke {
            layer: id,
            points: vec![(8.0, 8.0)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let dirty = worker.render_worker.cache().dirty();
        assert!(!dirty.is_empty());
        let bounds = dirty.bounds();
        assert!(
            (bounds.area()) < (64 * 64),
            "marquage précis, pas global : {bounds:?}"
        );
        // Empreinte : boîte [5,11]² + offset (20,12) = [25,31]×[17,23].
        assert!(dirty.intersects(TileRegion::new(20, 12, 16, 16)));
        assert!(!dirty.intersects(TileRegion::new(48, 48, 16, 16)));
    }

    #[test]
    fn marquage_structurel_global() {
        // Opération structurelle ⇒ repli global sûr.
        let (doc, _) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        let _ = worker.mutate(PhotoEngineCommand::AddEmptyLayer);
        let dirty = worker.render_worker.cache().dirty();
        assert!(!dirty.is_empty());
        assert!(dirty.intersects(TileRegion::new(0, 0, 64, 64)));
    }

    fn live_version(worker: &EngineWorker, id: Uuid) -> u64 {
        match worker.test_document().find(id) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => panic!("calque pixels attendu"),
        }
    }

    fn paint_fond(worker: &mut EngineWorker, id: Uuid, x: f32, y: f32) {
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: id,
            points: vec![(x, y)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
    }

    #[test]
    fn undo_rewind_ne_sert_jamais_un_thumb_perime() {
        // Revue §6 : undo restaure (version, Arc) anciens. Le store versionné
        // ne doit jamais servir la miniature d'un autre contenu, et le
        // pending ne doit pas rester coincé (pas d'intention obsolète qui
        // survivrait et polluerait les soumissions suivantes).
        let (doc, _) = painted_doc();
        let fond = doc.root[0].id();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        paint_fond(&mut worker, fond, 8.0, 8.0);
        let version1 = live_version(&worker, fond);
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == version1),
            "store alimenté en v1 (inline)"
        );
        // Undo : le vivant redevient l'état d'avant (même Arc).
        let layers = layers_of(&worker.apply(PhotoEngineCommand::Undo));
        let version0 = live_version(&worker, fond);
        assert_ne!(version0, version1, "version restaurée en arrière");
        // Après undo, la miniature servie (recalculée inline pour v0, ou
        // absente) porte TOUJOURS la version live — jamais du contenu v1
        // estampillé v0.
        let thumb_apres_undo = layers
            .iter()
            .find(|l| l.id == fond)
            .and_then(|l| l.thumb.clone());
        if let Some(thumb) = thumb_apres_undo {
            assert_eq!(
                thumb.version, version0,
                "miniature servie == version live (pas de v1 périmée)"
            );
        }
        // Nouvelle peinture : nouveau numéro global (jamais de réutilisation),
        // soumission normale, store à jour, pending vide.
        paint_fond(&mut worker, fond, 40.0, 40.0);
        let version2 = live_version(&worker, fond);
        assert_ne!(version2, version1);
        assert_ne!(version2, version0);
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == version2),
            "store à jour en v2"
        );
        assert!(
            worker.thumb_pending.is_empty(),
            "aucune intention coincée (inline)"
        );
        let layers = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        assert!(
            layers
                .iter()
                .find(|l| l.id == fond)
                .and_then(|l| l.thumb.clone())
                .is_some_and(|t| t.version == version2),
            "miniature v2 servie"
        );
    }

    // ------------------------------------------------------------------
    // Phase 6G.3 — canvas d'abord, miniatures ensuite (déterministe).
    // ------------------------------------------------------------------

    #[test]
    fn submit_decision_branches() {
        // Pure : store à jour ⇒ rien ; intention identique ⇒ coalescé ;
        // sinon ⇒ soumettre.
        assert_eq!(
            EngineWorker::submit_decision(true, None, 7),
            SubmitDecision::Fresh
        );
        assert_eq!(
            EngineWorker::submit_decision(false, None, 7),
            SubmitDecision::Submit
        );
        assert_eq!(
            EngineWorker::submit_decision(false, Some(7), 7),
            SubmitDecision::Coalesce
        );
        assert_eq!(
            EngineWorker::submit_decision(false, Some(6), 7),
            SubmitDecision::Submit
        );
        // Store à jour mais intention résiduelle : la fraîcheur prime.
        assert_eq!(
            EngineWorker::submit_decision(true, Some(7), 7),
            SubmitDecision::Coalesce
        );
    }

    #[test]
    fn ingest_rejette_perime_ordre_inverse() {
        // §16 : B calculé puis A (plus ancien) → A rejeté, B conservé.
        // Aucun thread : résultats synthétisés, garde seule.
        use crate::ui::thumb_worker::ThumbDone;
        let (doc, fond) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        // Version live après une peinture.
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(8.0, 8.0)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let live_version = match worker.test_document().find(fond) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => panic!("calque"),
        };
        // (Le mode inline a déjà stocké la miniature fraîche.)
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == live_version)
        );
        // Faux "ancien" résultat (version - 1, contenu arbitraire) : rejeté.
        let stale = ThumbDone {
            layer: fond,
            version: live_version.wrapping_sub(1),
            thumb: photo_engine::RgbaBuf::from_vec(48, 32, vec![0u8; 48 * 32 * 4]),
            image: std::sync::Arc::new(image::DynamicImage::new_rgba8(1, 1)),
            render_us: 0,
            thumb_us: 0,
        };
        assert!(!worker.ingest_thumb_done(stale), "périmé rejeté");
        // Le frais est intact.
        assert!(
            worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == live_version)
        );
    }

    #[test]
    fn canvas_first_puis_thumbnail_async() {
        // §18 : en mode threadé, la réponse canvas part SANS attendre la
        // miniature (pending observable) ; le résultat finit par arriver et
        // vaut l'octet du calcul direct.
        let (doc, fond) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        worker.enable_thumb_thread();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(8.0, 8.0)],
            eraser: false,
            radius: 3.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let live_version = match worker.test_document().find(fond) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => panic!("calque"),
        };
        // La réponse est déjà revenue (canvas) avec l'intention en attente —
        // déterministe : aucune attente du thread.
        assert!(
            worker
                .thumb_pending
                .get(&fond)
                .is_some_and(|(v, _)| *v == live_version),
            "intention en attente pendant que le canvas est servi"
        );
        // Convergence : le résultat finit par arriver (timeout large =
        // robustesse, pas de race : l'échec signifierait un thread mort).
        let start = std::time::Instant::now();
        loop {
            worker.poll_thumb_results();
            if worker
                .thumb_store
                .get(&fond)
                .is_some_and(|(_, v)| *v == live_version)
            {
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(10),
                "la tâche secondaire doit aboutir"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Octets == calcul direct (même fonction, mêmes entrées).
        let stored = worker.thumb_store.get(&fond).expect("stockée").0.clone();
        let layer = match worker.test_document().find(fond) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.clone(),
            _ => panic!("calque"),
        };
        let mut renderer = photo_engine::renderer::Renderer::default();
        let (_, expected) = renderer.appearance_frame(&layer).expect("cadre");
        assert_eq!(
            (stored.width, stored.height, stored.data.as_ref()),
            (expected.width, expected.height, expected.data.as_ref()),
            "miniature async == cadre direct"
        );
    }

    #[test]
    fn drain_extras_sauf_si_deja_embarquee() {
        // §4A : une fraîcheur NON embarquée part en extra ; déjà embarquée
        // ⇒ rien (pas de doublon). Déterministe, sans thread (vraies réponses).
        let (doc, fond) = painted_doc();
        let mut worker = EngineWorker::new(doc);
        let layers = layers_of(&worker.apply(PhotoEngineCommand::Refresh));
        assert!(
            layers.iter().any(|l| l.id == fond && l.thumb.is_some()),
            "inline : snapshot embarque les miniatures"
        );
        // L'inline a déjà marqué les deux calques comme frais : le drain
        // sans réponse principale les émet tous les deux (cas async).
        let extras = worker.drain_thumb_extras(&[]);
        assert_eq!(extras.len(), 2, "miniatures secondaires après canvas");
        assert!(
            extras
                .iter()
                .all(|r| matches!(r, PhotoEngineResponse::ThumbnailUpdated { .. })),
            "que des extras"
        );
        // Mêmes fraîcheurs, réponse principale les embarquant : rien.
        worker.thumb_fresh.push(fond);
        assert!(
            worker.drain_thumb_extras(&layers).is_empty(),
            "pas de doublon avec le snapshot"
        );
    }

    // ------------------------------------------------------------------
    // Phase 6G.1 — profilage end-to-end (diagnostic, aucune optimisation).
    //
    // Les tests rapides (512²) verrouillent les VOLUMES (déterministes) ;
    // les chronos sont rapportés par les benches `#[ignore]` en release.
    // Règle : jamais d'assert sur un temps (matériel variable), seulement
    // sur des événements et des volumes de travail.
    // ------------------------------------------------------------------

    fn bench_doc(size: u32) -> (Document, Uuid) {
        use photo_engine::{LayerNode, PixelLayer};
        let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            size,
            size,
            image::Rgba([200, 40, 40, 255]),
        ));
        let klein = size / 4;
        let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            klein,
            klein,
            image::Rgba([40, 200, 40, 255]),
        ));
        let mut doc = Document::new(size, size);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
        let mut vert = PixelLayer::new("carre", Arc::new(carre));
        vert.transform.offset_x = (size / 3) as f32;
        vert.transform.offset_y = (size / 4) as f32;
        let id = vert.id;
        doc.push_layer(LayerNode::Pixel(vert));
        let fond_id = doc.root[0].id();
        let _ = id;
        (doc, fond_id)
    }

    fn last_metrics(worker: &EngineWorker) -> OpMetrics {
        worker.metrics.last.clone().expect("metriques")
    }

    #[test]
    fn profile_volumes_paint_512() {
        // Volumes déterministes d'une petite peinture sur 512².
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let paint = |worker: &mut EngineWorker, x: f32, y: f32| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points: vec![(x, y), (x + 16.0, y + 16.0)],
                eraser: false,
                radius: 5.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        paint(&mut worker, 20.0, 20.0);
        let cold = last_metrics(&worker);
        assert_eq!(cold.cache_misses, 4, "froid : 4 tuiles 256²");
        assert_eq!(cold.cache_hits, 0);
        assert_eq!(cold.surface_pixels_written, 512 * 512);
        paint(&mut worker, 400.0, 400.0);
        let warm = last_metrics(&worker);
        assert_eq!(warm.cache_misses, 1, "seule la tuile sale");
        assert_eq!(warm.cache_hits, 3, "le reste réutilisé");
        assert_eq!(warm.surface_pixels_written, 256 * 256);
    }

    #[test]
    fn profile_volumes_toggle_512() {
        // Toggle ON/OFF : volumes symétriques, pas de surprise.
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
        let off = last_metrics(&worker);
        assert_eq!((off.cache_misses, off.cache_hits), (4, 0), "froid complet");
        worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
        let on = last_metrics(&worker);
        // Empreinte 128² en (170,128) : à cheval sur x=256 ⇒ 2 tuiles.
        assert_eq!((on.cache_misses, on.cache_hits), (2, 2));
        assert_eq!(on.surface_pixels_written, 2 * 256 * 256);
        // §2.5/§13 : ni source ni filtres touchés ⇒ aucun rebuild synchrone.
        assert_eq!(on.thumb_rebuilds, 0);
        assert_eq!(on.preview_rebuilds, 0);
        // §2.5 : la visibilité ne touche ni source ni filtres ⇒ les chaînes
        // ne rejouent pas (compositing d'état seulement).
        assert_eq!(on.thumb_rebuilds, 0, "aucune apparence recalculée");
        assert_eq!(on.preview_rebuilds, 0);
    }

    #[test]
    fn profile_volumes_opacity_512() {
        // Deux réglages : chaque passe recalcule l'empreinte (coalescé historique).
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        for opacity in [30.0, 70.0] {
            worker.apply(PhotoEngineCommand::SetOpacity { layer: id, opacity });
        }
        // Second réglage (coalescé) : seule l'empreinte est réévaluée
        // (128² à cheval sur x=256 ⇒ 2 tuiles).
        let m = last_metrics(&worker);
        assert_eq!((m.cache_misses, m.cache_hits), (2, 2));
        assert_eq!(m.surface_pixels_written, 2 * 256 * 256);
    }

    #[test]
    fn profile_legacy_vs_incremental_512() {
        // Même document : legacy (pleine cadre) == incrémental (worker), octets
        // ET dimensions ; chronos muraux relevés (rapport uniquement).
        let (doc, _) = bench_doc(512);
        let t_legacy = std::time::Instant::now();
        let legacy = render_preview(&doc, false).expect("legacy");
        let legacy_us = t_legacy.elapsed().as_micros();
        let mut worker = EngineWorker::new(doc);
        let t_incr = std::time::Instant::now();
        let response = worker.apply(PhotoEngineCommand::Refresh);
        let incr_us = t_incr.elapsed().as_micros();
        let m = last_metrics(&worker);
        let bytes = match response {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("aperçu").rgba,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(bytes, legacy.rgba, "mêmes octets");
        assert_eq!(m.cache_misses, 4, "froid complet");
        eprintln!("BENCH legacy_vs_incremental_512 legacy_us={legacy_us} incr_us={incr_us}");
    }

    #[test]
    fn profile_warm_ecrit_rien() {
        // §26 : frame chaude (même requête, dirty présenté donc clear) ⇒
        // 0 rendu + 0 écriture surface. Le `clear_dirty` simule l'étape de
        // présentation du contrat 6D (aucun appel production ne le fait
        // aujourd'hui — voir diagnostic_dirty_accumulates).
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        worker.render_worker.cache_mut().clear_dirty();
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, _) = worker.render_preview_incremental(&frame);
        preview.expect("froid");
        let written = worker.render_worker.surface_stats().pixels_written;
        let frame = AppearanceFrameCache::build(worker.test_document());
        let (preview, _, worker_stats) = worker.render_preview_incremental(&frame);
        preview.expect("chaud");
        assert_eq!(
            (worker_stats.rendered_tiles, worker_stats.cache_hits),
            (0, 4),
            "chaud : aucun rendu"
        );
        assert_eq!(
            worker.render_worker.surface_stats().pixels_written,
            written,
            "chaud : aucune écriture"
        );
    }

    #[test]
    fn dirty_consumed_after_render_stable() {
        // §1.3 6G.2 (ex-constat 6G.1 corrigé) : paint A → dirty=A → render A
        // → dirty soldé ; paint B (même tuile) → dirty=A → render A seule.
        // Plus de dérive monotone : le dirty est consommé après production.
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let stroke = |worker: &mut EngineWorker, x: f32, y: f32| {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points: vec![(x, y), (x + 16.0, y + 16.0)],
                eraser: false,
                radius: 5.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
        };
        stroke(&mut worker, 20.0, 20.0);
        let first = last_metrics(&worker);
        assert_eq!((first.cache_misses, first.cache_hits), (4, 0), "froid");
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "dirty soldé après rendu"
        );
        // Premier rendu : l'init de scope vide le cache ET le dirty
        // (invalidate_all) — rien à solder par tuile.
        assert_eq!(first.dirty_tiles_cleared, 0);
        stroke(&mut worker, 24.0, 24.0);
        let second = last_metrics(&worker);
        assert_eq!(
            (second.cache_misses, second.cache_hits),
            (1, 3),
            "même tuile : 1 seul rendu"
        );
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "toujours soldé"
        );
        assert_eq!(second.dirty_tiles_cleared, 1, "la zone marquée est soldée");
        stroke(&mut worker, 400.0, 400.0);
        let third = last_metrics(&worker);
        assert_eq!((third.cache_misses, third.cache_hits), (1, 3));
        assert_eq!(third.dirty_tiles_cleared, 1);
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "jamais d'accumulation"
        );
    }

    #[test]
    fn long_session_no_drift_512() {
        // §1.4 + §10 (version rapide, 50 mutations) : même zone, 1 rendu par
        // mutation, dirty vide à la fin. La version 2048²×100 est en bench
        // release ignoré (bench_long_session_2048).
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        // Amorçage froid (scope init : tout est rendu une fois).
        worker.apply(PhotoEngineCommand::Refresh);
        let mut total_renders = 0u64;
        for i in 0..50 {
            if i % 10 == 9 {
                // Opérations variées mais locales : toggle + re-toggle
                // (empreinte 2 tuiles à cheval sur x=256, dans les deux sens).
                worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
                assert_eq!(last_metrics(&worker).cache_misses, 2);
                worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
                let m = last_metrics(&worker);
                assert_eq!(m.cache_misses, 2);
                total_renders += 4;
            } else {
                worker.apply(PhotoEngineCommand::PaintStroke {
                    layer: fond,
                    points: vec![(20.0, 20.0), (36.0, 20.0)],
                    eraser: false,
                    radius: 5.0,
                    color: [(i % 256) as u8, 0, 255],
                    opacity: 1.0,
                });
                let m = last_metrics(&worker);
                assert_eq!(m.cache_misses, 1, "itération {i} : 1 seul rendu");
                total_renders += m.cache_misses;
            }
        }
        assert_eq!(total_renders, 45 + 5 * 4, "stable : 45×1 + 5×(2+2)");
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "aucune dérive après 50+ mutations"
        );
    }

    #[test]
    #[ignore]
    fn bench_long_session_2048() {
        // §10 6G.2 : 100 mutations mixtes sur 2048² — dirty soldé à chaque
        // frame (pas de dérive), apparences ciblées, previews ciblés.
        // Recommandé release : ~10-20 s.
        let (doc, fond) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::Refresh);
        let id = worker.test_document().root[1].id();
        let mut samples = Vec::with_capacity(100);
        let mut total_renders = 0u64;
        let mut total_cleared = 0u64;
        for i in 0..100 {
            let t = std::time::Instant::now();
            match i % 10 {
                9 => {
                    worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
                }
                8 => {
                    worker.apply(PhotoEngineCommand::SetOpacity {
                        layer: id,
                        opacity: 20.0 + (i % 60) as f32,
                    });
                }
                _ => {
                    bench_paint_apply(&mut worker, fond, 40.0, 40.0);
                }
            }
            samples.push(t.elapsed().as_micros());
            let m = last_metrics(&worker);
            total_renders += m.cache_misses;
            total_cleared += m.dirty_tiles_cleared;
            assert!(
                m.cache_misses <= 8,
                "itération {i} : jamais de dérive vers le full (64)"
            );
        }
        assert!(
            worker.render_worker.cache().dirty().is_empty(),
            "dirty vide après 100 mutations"
        );
        let m = last_metrics(&worker);
        let mut sorted = samples.clone();
        let (min, median, p95) = stats_us(&mut sorted);
        eprintln!(
            "BENCH op=long_session size=2048 n=100 min={min} median={median} p95={p95} \
             total_renders={total_renders} total_cleared={total_cleared}"
        );
        bench_row(
            "long_session",
            2048,
            "mixed",
            &samples,
            &m,
            &worker.metrics.thumb,
        );
    }

    /// Min/médiane/p95 sur des microsecondes (benchmarks release).
    fn stats_us(samples: &mut [u128]) -> (u128, u128, u128) {
        samples.sort_unstable();
        let min = samples[0];
        let median = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        (min, median, p95)
    }

    /// Ligne de résultat benchmark (lue avec `--nocapture` en release).
    fn bench_row(
        op: &str,
        size: u32,
        label: &str,
        samples: &[u128],
        m: &OpMetrics,
        thumb: &crate::ui::thumb_worker::ThumbStats,
    ) {
        let mut sorted = samples.to_vec();
        let (min, median, p95) = stats_us(&mut sorted);
        eprintln!(
            "BENCH op={op} size={size} {label} n={} min_us={min} median_us={median} p95_us={p95} \
             mutation_us={} snapshot_us={} frame_us={} thumb_us={} geometry_us={} composite_us={} \
             tile_render_total_us={} surface_us={} assembly_us={} \
             blend_us={} layers_blended={} pixels_processed={} \
             appearance_resolves={} appearance_frame_hits={} preview_rebuilds={} thumb_rebuilds={} \
             hits={} misses={} avoided={} surface_px={} scope_px={} full_px={} preview_px={} \
             resize_px={} cleared={} thumb_done={} canvas_us={} secondary_us={} \
             t_submitted={} t_completed={} t_discarded={} t_coalesced={} t_us={}",
            samples.len(),
            m.mutation_us,
            m.snapshot_us,
            m.frame_us,
            m.thumb_us,
            m.geometry_us,
            m.composite_us,
            m.tile_render_total_us,
            m.surface_update_us,
            m.assembly_us,
            m.blend_us,
            m.layers_blended,
            m.pixels_processed,
            m.appearance_resolves,
            m.appearance_frame_hits,
            m.preview_rebuilds,
            m.thumb_rebuilds,
            m.cache_hits,
            m.cache_misses,
            m.renders_avoided,
            m.surface_pixels_written,
            m.scope_px,
            m.full_px,
            m.preview_px,
            m.preview_resize_pixels,
            m.dirty_tiles_cleared,
            m.thumbnail_jobs_completed,
            m.canvas_critical_us,
            m.secondary_work_us,
            thumb.submitted,
            thumb.completed,
            thumb.discarded,
            thumb.coalesced,
            thumb.thumb_us,
        );
    }

    fn bench_paint_apply(worker: &mut EngineWorker, fond: Uuid, x: f32, y: f32) -> u128 {
        let t = std::time::Instant::now();
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(x, y), (x + 16.0, y + 16.0)],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        t.elapsed().as_micros()
    }

    #[test]
    #[ignore]
    fn bench_512_all() {
        // Lignes BENCH 512² froid+chaud : paint, toggle, opacity (5 reps).
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        // Paint froid puis chaud.
        let cold = vec![bench_paint_apply(&mut worker, fond, 20.0, 20.0)];
        let m_cold = last_metrics(&worker);
        bench_row("paint", 512, "cold", &cold, &m_cold, &worker.metrics.thumb);
        let mut warm = Vec::new();
        for _ in 0..5 {
            warm.push(bench_paint_apply(&mut worker, fond, 400.0, 400.0));
        }
        let m_warm = last_metrics(&worker);
        bench_row("paint", 512, "warm", &warm, &m_warm, &worker.metrics.thumb);
        // Toggle froid (carreau neuf) : worker frais pour un vrai froid.
        let (doc, _) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let t = std::time::Instant::now();
            worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
            samples.push(t.elapsed().as_micros());
        }
        bench_row(
            "toggle",
            512,
            "on_off",
            &samples,
            &last_metrics(&worker),
            &worker.metrics.thumb,
        );
        // Opacity ×2.
        let mut samples = Vec::new();
        for opacity in [30.0, 70.0, 30.0, 70.0, 30.0] {
            let t = std::time::Instant::now();
            worker.apply(PhotoEngineCommand::SetOpacity { layer: id, opacity });
            samples.push(t.elapsed().as_micros());
        }
        bench_row(
            "opacity",
            512,
            "slider",
            &samples,
            &last_metrics(&worker),
            &worker.metrics.thumb,
        );
    }

    #[test]
    #[ignore]
    fn bench_paint_2048() {
        // Recommandé : `cargo test --release -p photo bench_paint_2048 -- --ignored --nocapture`.
        // Stroke < 64×64 effectifs sur 2048², froid puis chaud (7 reps).
        let (doc, fond) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        let mut cold = Vec::new();
        for i in 0..3 {
            cold.push(bench_paint_apply(
                &mut worker,
                fond,
                20.0 + i as f32 * 300.0,
                20.0,
            ));
        }
        let m_cold = last_metrics(&worker);
        let mut warm = Vec::new();
        for _ in 0..7 {
            // Même zone repeinte : tuile déjà chaude après la 1re.
            warm.push(bench_paint_apply(&mut worker, fond, 20.0, 20.0));
        }
        let m_warm = last_metrics(&worker);
        bench_row("paint", 2048, "cold", &cold, &m_cold, &worker.metrics.thumb);
        bench_row("paint", 2048, "warm", &warm, &m_warm, &worker.metrics.thumb);
    }

    #[test]
    #[ignore]
    fn bench_paint_4096() {
        // Stroke < 64×64 sur 4096², froid puis chaud (5 reps).
        let (doc, fond) = bench_doc(4096);
        let mut worker = EngineWorker::new(doc);
        let mut cold = Vec::new();
        for i in 0..2 {
            cold.push(bench_paint_apply(
                &mut worker,
                fond,
                20.0 + i as f32 * 900.0,
                20.0,
            ));
        }
        let m_cold = last_metrics(&worker);
        let mut warm = Vec::new();
        for _ in 0..5 {
            warm.push(bench_paint_apply(&mut worker, fond, 20.0, 20.0));
        }
        let m_warm = last_metrics(&worker);
        bench_row("paint", 4096, "cold", &cold, &m_cold, &worker.metrics.thumb);
        bench_row("paint", 4096, "warm", &warm, &m_warm, &worker.metrics.thumb);
    }

    #[test]
    #[ignore]
    fn bench_toggle_2048() {
        let (doc, _) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        let id = worker.test_document().root[1].id();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let t = std::time::Instant::now();
            worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
            samples.push(t.elapsed().as_micros());
        }
        let m = last_metrics(&worker);
        bench_row(
            "toggle",
            2048,
            "on_off",
            &samples,
            &m,
            &worker.metrics.thumb,
        );
    }

    #[test]
    #[ignore]
    fn bench_open_2048() {
        // Fichier PNG réel (généré) : décodage inclus, puis composite.
        let dir = std::env::temp_dir().join("cygnus-bench-open");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("bench-2048.png");
        if !path.exists() {
            let img = image::RgbaImage::from_pixel(2048, 2048, image::Rgba([120, 130, 140, 255]));
            img.save(&path).expect("png bench");
        }
        let t_decode = std::time::Instant::now();
        let decoded = image::open(&path).expect("décodage bench");
        let decode_us = t_decode.elapsed().as_micros();
        let (w, h) = (decoded.width(), decoded.height());
        let mut worker = EngineWorker::new(Document::new(8, 8));
        let t = std::time::Instant::now();
        worker.apply(PhotoEngineCommand::OpenImage { path });
        let total_us = t.elapsed().as_micros();
        let m = last_metrics(&worker);
        eprintln!("BENCH op=open size=2048 decode_us={decode_us} dims={w}x{h} total_us={total_us}");
        bench_row(
            "open",
            2048,
            "apply",
            &[total_us],
            &m,
            &worker.metrics.thumb,
        );
    }

    #[test]
    #[ignore]
    fn bench_presentation_ui_512() {
        // Recommandé release : worker paint → réponse → apply_response avec
        // Context headless. Mesure ColorImage + texture.set + thumbs + total.
        use crate::state::{PhotoUiState, apply_response};
        let ctx = egui::Context::default();
        let (doc, fond) = bench_doc(512);
        let mut worker = EngineWorker::new(doc);
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![(20.0, 20.0), (36.0, 20.0)],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        // Rejoue la même réponse N fois sur des états frais (uploads + bytes).
        let mut samples = Vec::new();
        for _ in 0..7 {
            let response = worker.apply(PhotoEngineCommand::Refresh);
            let preview_bytes = match &response {
                PhotoEngineResponse::LayersChanged { preview, .. } => preview
                    .as_ref()
                    .map(|p| p.width as u64 * p.height as u64 * 4),
                _ => None,
            };
            let mut ui = PhotoUiState::default();
            let t = std::time::Instant::now();
            apply_response(&ctx, &mut ui, response);
            let total_us = t.elapsed().as_micros();
            let p = ui.last_presentation;
            samples.push(total_us);
            eprintln!(
                "BENCHUI apply_us={total_us} thumb_us={} colorimage_us={} texture_us={} \
                 preview_us={} uploads={} upload_bytes={} preview_bytes={:?}",
                p.thumb_sync_us,
                p.colorimage_us,
                p.texture_set_us,
                p.preview_apply_us,
                ui.texture_uploads,
                ui.texture_upload_bytes,
                preview_bytes,
            );
        }
        let mut sorted = samples.clone();
        let (min, median, p95) = stats_us(&mut sorted);
        eprintln!(
            "BENCHUI op=apply_response size=512 n={} min={min} median={median} p95={p95}",
            samples.len()
        );
    }

    #[test]
    #[ignore]
    fn bench_presentation_ui_2048() {
        // UI 2048² : 3 applies (froid partiel), millis par étape.
        use crate::state::{PhotoUiState, apply_response};
        let ctx = egui::Context::default();
        let (doc, fond) = bench_doc(2048);
        let mut worker = EngineWorker::new(doc);
        for i in 0..3 {
            worker.apply(PhotoEngineCommand::PaintStroke {
                layer: fond,
                points: vec![
                    (20.0 + i as f32 * 300.0, 20.0),
                    (36.0 + i as f32 * 300.0, 20.0),
                ],
                eraser: false,
                radius: 5.0,
                color: [0, 0, 255],
                opacity: 1.0,
            });
            let response = worker.apply(PhotoEngineCommand::Refresh);
            let mut ui = PhotoUiState::default();
            let t = std::time::Instant::now();
            apply_response(&ctx, &mut ui, response);
            let total_us = t.elapsed().as_micros();
            let p = ui.last_presentation;
            eprintln!(
                "BENCHUI op=apply size=2048 apply_us={total_us} thumb_us={} \
                 colorimage_us={} texture_us={} preview_us={} uploads={} upload_bytes={}",
                p.thumb_sync_us,
                p.colorimage_us,
                p.texture_set_us,
                p.preview_apply_us,
                ui.texture_uploads,
                ui.texture_upload_bytes,
            );
        }
    }

    #[test]
    #[ignore]
    fn bench_legacy_vs_incremental_2048() {
        // Legacy pleine cadre vs worker incrémental, même document 2048².
        let (doc, _) = bench_doc(2048);
        let t_legacy = std::time::Instant::now();
        let legacy = render_preview(&doc, false).expect("legacy");
        let legacy_us = t_legacy.elapsed().as_micros();
        let mut worker = EngineWorker::new(doc);
        let t_incr = std::time::Instant::now();
        let response = worker.apply(PhotoEngineCommand::Refresh);
        let incr_us = t_incr.elapsed().as_micros();
        let bytes = match response {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview.expect("aperçu").rgba,
            other => panic!("LayersChanged attendu, obtenu : {other:?}"),
        };
        assert_eq!(bytes, legacy.rgba, "mêmes octets");
        let m = last_metrics(&worker);
        eprintln!(
            "BENCH op=legacy_vs_incremental size=2048 legacy_us={legacy_us} incr_us={incr_us}"
        );
        bench_row(
            "incremental",
            2048,
            "cold",
            &[incr_us],
            &m,
            &worker.metrics.thumb,
        );
    }
}
