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

//! Réponses du worker vers l'UI : [`PhotoEngineResponse`].

use super::preview::PreviewImage;
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
        /// Libellés des pas annulables, du plus ancien au plus récent
        /// (panneau Historique, O003).
        undo_labels: Vec<String>,
        /// Libellés des pas rétablissables, du plus ancien au plus
        /// récent (l'UI affiche en tête le dernier = prochain redo).
        redo_labels: Vec<String>,
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
        /// Libellés des pas annulables (O003, comme `LayersChanged`).
        undo_labels: Vec<String>,
        /// Libellés des pas rétablissables (O003).
        redo_labels: Vec<String>,
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
    /// Projet `.cygp` enregistré (O001 : Save/Save As).
    ProjectSaved {
        /// Chemin du fichier écrit.
        path: PathBuf,
    },
}

// ---------------------------------------------------------------------------
// INSTRUMENTATION TEMPORAIRE (diagnostic perf, aucune incidence
// fonctionnelle) : timings worker + compteurs, à retirer une fois le
// rapport validé.
// ---------------------------------------------------------------------------
