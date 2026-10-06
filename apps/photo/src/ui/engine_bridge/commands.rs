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

//! Commandes du pont moteur : [`PhotoEngineCommand`], routage de rendu
//! ([`render_routing`]) et coalescence de lots ([`fold_batch`]).

use super::responses::PhotoEngineResponse;
use crate::ui::features::layers::{
    PhotoLayerInfo, PhotoLayerThumb, flattened_len, snapshot_layers, snapshot_layers_with,
};
use photo_engine::tiles::TileRegion;
use photo_engine::{BlendMode, Document, RenderEvent, RenderRevision};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use uuid::Uuid;

/// Commandes UI → worker moteur.
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoEngineCommand {
    /// Basculer la visibilité d'un calque.
    ToggleLayerVisibility(Uuid),
    /// Réordonner par ids (drop hiérarchique avant/après : même
    /// parent ou non — nesting libre via `reorder_before` moteur).
    ReorderNodes {
        /// Calque déplacé.
        dragged: Uuid,
        /// Calque cible.
        target: Uuid,
        /// Vrai = avant la cible, faux = après.
        before: bool,
    },
    /// Imbriquer un calque en tête d'un groupe (`move_into` moteur).
    MoveIntoGroup {
        /// Calque déplacé.
        layer: Uuid,
        /// Groupe d'accueil.
        group: Uuid,
    },
    /// Replier / déplier un groupe (affichage seul : snapshot sans
    /// nouveau composite).
    ToggleGroupCollapsed(Uuid),
    /// Régler l'opacité d'un calque (unités moteur 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Régler le mode de fusion d'un calque (choix discret).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Annuler la dernière mutation.
    Undo,
    /// Rétablir la dernière annulation.
    Redo,
    /// Annuler jusqu'à `steps` mutations (saut d'état du panneau
    /// Historique, O003) : applique ce qui existe (au plus la pile),
    /// un seul rendu.
    UndoSteps {
        /// Nombre de pas en arrière demandés.
        steps: u32,
    },
    /// Rétablir jusqu'à `steps` mutations (O003, symétrique).
    RedoSteps {
        /// Nombre de pas en avant demandés.
        steps: u32,
    },
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
    /// Miroir horizontal du calque pixels (O004, destructif + historique).
    FlipHorizontal(Uuid),
    /// Miroir vertical du calque pixels (O004).
    FlipVertical(Uuid),
    /// Rotation 90° horaire, centre conservé (O004).
    RotateClockwise(Uuid),
    /// Rotation 90° antihoraire (O004).
    RotateCounterclockwise(Uuid),
    /// Rogner le calque pixels à l'intersection avec le document
    /// (O004) : sans recouvrement = erreur propre, déjà contenu =
    /// no-op (sans entrée d'historique).
    CropToDocument(Uuid),
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
    /// Enregistrer le document dans un `.cygp` (O001 : Save/Save As).
    /// Le chemin est déjà résolu par l'app (état local) ; le worker
    /// ne fait que sérialiser via `photo_engine::project::save`.
    SaveProject {
        /// Chemin de destination (extension `.cygp` gérée par le picker).
        path: PathBuf,
    },
    /// Charger un `.cygp` en remplacement du document vivant (O001 :
    /// Open). Historique et caches vidés : nouvelle session d'édition.
    /// Le titre/chemin UI sont posés par l'app au dispatch (état
    /// local) ; le worker ne renvoie que le composite (`LayersChanged`)
    /// ou `EngineError` (version étrangère, fichier corrompu).
    LoadProject {
        /// Chemin du projet à charger.
        path: PathBuf,
    },
    /// Rogner l'aperçu aux dimensions du document (menu Affichage).
    /// `false` (défaut) = plan infini : le contenu hors document reste
    /// visible autour du cadre.
    SetPreviewClip {
        /// Vrai = aperçu rogné au document.
        clip: bool,
    },
}

impl PhotoEngineCommand {
    /// Nom stable de la commande (instrumentation uniquement).
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::ToggleLayerVisibility(_) => "toggle_visibility",
            Self::ReorderNodes { .. } => "reorder_nodes",
            Self::MoveIntoGroup { .. } => "move_into_group",
            Self::ToggleGroupCollapsed(_) => "toggle_collapsed",
            Self::SetOpacity { .. } => "set_opacity",
            Self::SetBlendMode { .. } => "set_blend_mode",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::UndoSteps { .. } => "undo_steps",
            Self::RedoSteps { .. } => "redo_steps",
            Self::PaintStroke { .. } => "paint_stroke",
            Self::MoveLayer { .. } => "move_layer",
            Self::OpenImage { .. } => "open_image",
            Self::AddEmptyLayer => "add_empty_layer",
            Self::DuplicateLayer(_) => "duplicate_layer",
            Self::FlipHorizontal(_) => "flip_horizontal",
            Self::FlipVertical(_) => "flip_vertical",
            Self::RotateClockwise(_) => "rotate_clockwise",
            Self::RotateCounterclockwise(_) => "rotate_counterclockwise",
            Self::CropToDocument(_) => "crop_to_document",
            Self::DeleteLayer(_) => "delete_layer",
            Self::AddFilter { .. } => "add_filter",
            Self::RenameLayer { .. } => "rename_layer",
            Self::AddMask { .. } => "add_mask",
            Self::RemoveMask { .. } => "remove_mask",
            Self::MoveFilter { .. } => "move_filter",
            Self::MoveMask { .. } => "move_mask",
            Self::RemoveFilter { .. } => "remove_filter",
            Self::Export { .. } => "export",
            Self::SaveProject { .. } => "save_project",
            Self::LoadProject { .. } => "load_project",
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
/// Renommage et repli de groupe sont purement `StateOnly`.
pub fn render_routing(command: &PhotoEngineCommand) -> (RenderEvent, RenderInvalidation) {
    match command {
        PhotoEngineCommand::ToggleLayerVisibility(_) => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::ReorderNodes { .. } | PhotoEngineCommand::MoveIntoGroup { .. } => {
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
        | PhotoEngineCommand::LoadProject { .. }
        | PhotoEngineCommand::FlipHorizontal(_)
        | PhotoEngineCommand::FlipVertical(_)
        | PhotoEngineCommand::RotateClockwise(_)
        | PhotoEngineCommand::RotateCounterclockwise(_)
        | PhotoEngineCommand::CropToDocument(_)
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
        | PhotoEngineCommand::UndoSteps { .. }
        | PhotoEngineCommand::RedoSteps { .. }
        | PhotoEngineCommand::Refresh
        | PhotoEngineCommand::SetPreviewClip { .. } => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::RenameLayer { layer, .. }
        | PhotoEngineCommand::ToggleGroupCollapsed(layer) => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::StateOnly,
        ),
        PhotoEngineCommand::Export { .. } | PhotoEngineCommand::SaveProject { .. } => {
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
pub(crate) enum DirtyMark {
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
pub(crate) enum SubmitDecision {
    /// Store déjà à jour : rien à faire.
    Fresh,
    /// Même version déjà en attente : intention absorbée (coalescing).
    Coalesce,
    /// Nouvelle intention à transmettre.
    Submit,
}

/// Résultat d'une mutation sans rendu : réponses immédiates (export,
/// erreurs) + invalidation à produire ensuite.
pub(crate) struct Mutation {
    /// Réponses à envoyer telles quelles (pas de rendu associé).
    pub(crate) immediates: Vec<PhotoEngineResponse>,
    /// Invalidation cumulée de la mutation.
    pub(crate) invalidation: RenderInvalidation,
    /// Vrai si l'état document a changé (snapshot à renvoyer).
    pub(crate) changed: bool,
    /// Marquage dirty (défaut sûr selon `invalidation`, affiné par bras).
    pub(crate) dirty: DirtyMark,
}

impl Mutation {
    /// Mutation effective : un rendu/réponse suivra.
    pub(crate) fn changed(invalidation: RenderInvalidation) -> Self {
        Self {
            immediates: Vec::new(),
            invalidation,
            changed: true,
            dirty: DirtyMark::Auto,
        }
    }

    /// No-op (id inconnu, garde-fou) : état inchangé.
    pub(crate) fn unchanged() -> Self {
        Self {
            immediates: Vec::new(),
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
            dirty: DirtyMark::None,
        }
    }

    /// Réponse immédiate sans rendu (export, erreur non bloquante).
    pub(crate) fn immediate(response: PhotoEngineResponse) -> Self {
        Self {
            immediates: vec![response],
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
            dirty: DirtyMark::None,
        }
    }

    /// Affine le marquage avec une zone précise (consomme `self`).
    pub(crate) fn with_dirty(mut self, dirty: DirtyMark) -> Self {
        self.dirty = dirty;
        self
    }
}

/// Marquage précis pour un nœud porteur (empreinte + spread exact) :
/// `EMPTY` (ajustement, calque vide) ⇒ repli `Global` — jamais de région
/// vide silencieuse (un ajustement touche tout l'accumulateur).
pub(crate) fn mark_for_node(node: &photo_engine::LayerNode) -> DirtyMark {
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
pub(crate) fn fold_batch(commands: Vec<PhotoEngineCommand>) -> Vec<PhotoEngineCommand> {
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
