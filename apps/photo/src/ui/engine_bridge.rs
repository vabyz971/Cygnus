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

use super::features::layers::{PhotoLayerInfo, snapshot_layers, snapshot_layers_with};
use photo_engine::{Appearance, BlendMode, Document, RenderEvent, RenderRevision};
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

/// Résultat d'une mutation sans rendu : réponses immédiates (export,
/// erreurs) + invalidation à produire ensuite.
struct Mutation {
    /// Réponses à envoyer telles quelles (pas de rendu associé).
    immediates: Vec<PhotoEngineResponse>,
    /// Invalidation cumulée de la mutation.
    invalidation: RenderInvalidation,
    /// Vrai si l'état document a changé (snapshot à renvoyer).
    changed: bool,
}

impl Mutation {
    /// Mutation effective : un rendu/réponse suivra.
    fn changed(invalidation: RenderInvalidation) -> Self {
        Self {
            immediates: Vec::new(),
            invalidation,
            changed: true,
        }
    }

    /// No-op (id inconnu, garde-fou) : état inchangé.
    fn unchanged() -> Self {
        Self {
            immediates: Vec::new(),
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
        }
    }

    /// Réponse immédiate sans rendu (export, erreur non bloquante).
    fn immediate(response: PhotoEngineResponse) -> Self {
        Self {
            immediates: vec![response],
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
        }
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
    /// Apparences résolues, par calque (clones `Arc`, pas de pixels).
    map: HashMap<Uuid, Appearance>,
    /// Appels au renderer (premières résolutions).
    resolves: u64,
    /// Réutilisations inter-passes (`Cell` : le résolveur est `Fn`).
    hits: std::cell::Cell<u64>,
}

impl AppearanceFrameCache {
    /// Résout chaque calque pixels UNE fois auprès du renderer
    /// (visibles ou non : le snapshot a besoin des miniatures même
    /// des calques masqués).
    fn build(document: &Document) -> Self {
        let ids: Vec<Uuid> = document.all_pixel_ids();
        let mut map = HashMap::with_capacity(ids.len());
        let mut resolves = 0;
        for id in ids {
            if let Some(appearance) = document.appearance(id) {
                map.insert(id, appearance);
                resolves += 1;
            }
        }
        Self {
            map,
            resolves,
            hits: std::cell::Cell::new(0),
        }
    }

    /// Apparence partagée (clone bon marché), comptée en réutilisation.
    fn appearance(&self, id: Uuid) -> Option<Appearance> {
        let appearance = self.map.get(&id)?.clone();
        self.hits.set(self.hits.get() + 1);
        Some(appearance)
    }

    /// Image partagée pour le compositing (clone d'`Arc`).
    fn image(&self, id: Uuid) -> Option<Arc<image::DynamicImage>> {
        self.appearance(id).map(|appearance| appearance.image)
    }

    /// Miniature partagée pour le snapshot (clone de `RgbaBuf`).
    fn thumb(&self, id: Uuid) -> Option<photo_engine::RgbaBuf> {
        self.appearance(id).map(|appearance| appearance.thumb)
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
    /// INSTRUMENTATION TEMPORAIRE (diagnostic perf).
    pub metrics: WorkerMetrics,
}

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
            metrics: WorkerMetrics::default(),
        }
    }

    /// INSTRUMENTATION TEMPORAIRE : accès en lecture au document
    /// vivant (sondes de mesure uniquement).
    pub fn test_document(&self) -> &Document {
        &self.document
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
    /// En mode `Composite`, les apparences sont résolues UNE fois dans
    /// un [`AppearanceFrameCache`] local puis partagées entre snapshot,
    /// composite et géométrie (single-pass, sans toucher au cache
    /// persistant du renderer).
    /// INSTRUMENTATION TEMPORAIRE : remplit `metrics.last` (hors
    /// `op`, `total_us` et `mutation_us`, posés par `apply`/`apply_batch`).
    fn respond(&mut self, invalidation: RenderInvalidation) -> PhotoEngineResponse {
        let stats_before = self.document.appearance_stats();
        let t_frame = std::time::Instant::now();
        let frame = (invalidation == RenderInvalidation::Composite)
            .then(|| AppearanceFrameCache::build(&self.document));
        let frame_us = t_frame.elapsed().as_micros();
        let t_snapshot = std::time::Instant::now();
        let layers = match frame.as_ref() {
            Some(frame) => snapshot_layers_with(&self.document, &|id| frame.thumb(id)),
            None => snapshot_layers(&self.document),
        };
        let snapshot_us = t_snapshot.elapsed().as_micros();
        let can_undo = !self.undo.is_empty();
        let can_redo = !self.redo.is_empty();
        if invalidation == RenderInvalidation::Composite {
            let frame = frame.as_ref().expect("cadre construit en mode Composite");
            let (preview, timings) =
                render_preview_timed_with(&self.document, self.clip_to_doc, frame);
            let revision = self.revision.bump();
            self.metrics.renders += 1;
            let stats_after = self.document.appearance_stats();
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
            });
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
    /// Ne rend jamais : applique la mutation et retourne son
    /// invalidation (+ réponses immédiates type export/erreur).
    /// Le rendu éventuel est produit par [`Self::respond`].
    fn mutate(&mut self, command: PhotoEngineCommand) -> Mutation {
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
                Mutation::changed(invalidation)
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
                Mutation::changed(invalidation)
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
                Mutation::changed(invalidation)
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
                if let Some(node) = self.document.find_mut(layer) {
                    // Groupes/ajustements : no-op (snapshot déjà poussé
                    // par prudence, état inchangé).
                    node.translate_by(dx, dy);
                }
                Mutation::changed(invalidation)
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
                Mutation::changed(invalidation)
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
                Mutation::changed(invalidation)
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
                Mutation::changed(invalidation)
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
                self.clip_to_doc = clip;
                Mutation::changed(invalidation)
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
fn capped_preview(image: image::DynamicImage) -> image::DynamicImage {
    let (w, h) = (image.width(), image.height());
    if w.max(h) <= PREVIEW_MAX_DIMENSION {
        image
    } else {
        image.thumbnail(PREVIEW_MAX_DIMENSION, PREVIEW_MAX_DIMENSION)
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
        let raster = capped_preview(cropped).to_rgba8();
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
    let preview = capped_preview(composite);
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
        },
    )
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
        // 3 calques → 3 résolutions, partagées entre les consommateurs :
        // snapshot (3) + extents composite (3) + fold (3) + extents
        // géométrie (3) + contributes (1, sortie précoce au 1er calque).
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
        // Exactement un calque reconstruit (preview + thumb).
        let m = worker.metrics.last.as_ref().expect("metriques");
        assert_eq!(m.preview_rebuilds, 1);
        assert_eq!(m.thumb_rebuilds, 1);
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
        assert_eq!(m.preview_rebuilds, 1, "seule la chaîne du calque rejoue");
        assert_eq!(m.thumb_rebuilds, 1);
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
}
