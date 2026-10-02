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

//! Métriques et timings du worker : [`PreviewTimings`], [`OpMetrics`],
//! [`WorkerMetrics`].

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
    /// Phase 6G.4 : résultats miniature resservis par le cache sans
    /// calcul cette frame (HIT).
    pub thumb_cache_hits: u64,
    /// Phase 6G.4 : résultats miniature ayant exigé un calcul.
    pub thumb_cache_misses: u64,
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
    pub(crate) current_op: &'static str,
    /// Nombre de réponses produites.
    pub responses: u64,
    /// Nombre de réponses avec aperçu.
    pub previews: u64,
    /// Nombre de composites réellement produits (rendus non coalescés).
    pub renders: u64,
    /// Miniatures secondaires (Phase 6G.3 §14, cumulé).
    pub thumb: crate::ui::thumb_worker::ThumbStats,
}
