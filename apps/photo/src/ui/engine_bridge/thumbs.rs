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

//! Miniatures asynchrones de l'[`EngineWorker`](super::worker::EngineWorker) :
//! méthodes `impl EngineWorker` (store, soumission, ingestion, drainage).

use super::commands::{
    DirtyMark, Mutation, PhotoEngineCommand, RenderInvalidation, SubmitDecision,
};
use super::metrics::{OpMetrics, PreviewTimings, WorkerMetrics};
use super::preview::AppearanceFrameCache;
use super::responses::PhotoEngineResponse;
use super::worker::EngineWorker;
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

impl EngineWorker {
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
    pub(crate) fn stored_thumb(&self, id: Uuid) -> Option<photo_engine::RgbaBuf> {
        let live = match self.document.find(id) {
            Some(photo_engine::LayerNode::Pixel(l)) => l.appearance_version,
            _ => return None,
        };
        let (thumb, version) = self.thumb_store.get(&id)?;
        (*version == live).then(|| thumb.clone())
    }

    /// Retire du store les calques disparus (structurel, undo/redo…).
    pub(crate) fn prune_thumb_store(&mut self) {
        let live: std::collections::HashSet<Uuid> =
            self.document.all_pixel_ids().into_iter().collect();
        self.thumb_store.retain(|id, _| live.contains(id));
        self.thumb_pending.retain(|id, _| live.contains(id));
    }

    /// Décision d'envoi pure (testable) : `Fresh` (rien à faire),
    /// `Coalesce` (intention identique déjà en attente — absorbée),
    /// `Submit` (nouvelle intention à transmettre).
    pub(crate) fn submit_decision(known: bool, pending: Option<u64>, live: u64) -> SubmitDecision {
        if pending == Some(live) {
            SubmitDecision::Coalesce
        } else if known {
            SubmitDecision::Fresh
        } else {
            SubmitDecision::Submit
        }
    }

    /// État replié courant d'un groupe (faux si `id` n'est pas un
    /// groupe) : le toggle worker inverse cette valeur.
    pub(crate) fn is_collapsed(&self, id: Uuid) -> bool {
        matches!(
            self.document.find(id),
            Some(photo_engine::LayerNode::Group(groupe)) if groupe.collapsed
        )
    }

    /// Soumet les miniatures périmées (version live inconnue du store ET non
    /// déjà en attente à cette version). En mode inline : calcul + ingestion
    /// immédiats (déterministe). En mode threadé : envoi non bloquant
    /// (repli inline si le thread est tombé).
    /// Retourne les versions soumises (diagnostic).
    pub(crate) fn submit_stale_thumbs(&mut self) -> u64 {
        use crate::ui::thumb_worker::ThumbJob;
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
                key: photo_engine::ThumbnailKey::for_layer(layer),
            };
            self.metrics.thumb.submitted += 1;
            submitted += 1;
            self.thumb_pending
                .insert(id, (live_version, Arc::clone(&layer.source_image)));
            if self.thumb_worker.is_threaded() {
                if !self.thumb_worker.submit(job.clone()) {
                    // Thread tombé : repli inline sûr (pixels identiques,
                    // cache 6G.4 partagé de l'instance inline).
                    if let Some(done) = self.thumb_worker.compute_cached(&job) {
                        inline_done.push(done);
                    }
                }
            } else if let Some(done) = self.thumb_worker.compute_cached(&job) {
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
    pub(crate) fn ingest_thumb_done(&mut self, done: crate::ui::thumb_worker::ThumbDone) -> bool {
        // Comptage cache 6G.4 (HIT/MISS mesurés ici : couvre inline ET
        // threadé, frais comme périmés — un HIT périmé reste un calcul
        // évité, rejeté juste après par la garde).
        if done.from_cache {
            self.metrics.thumb.cache_hits += 1;
        } else {
            self.metrics.thumb.cache_misses += 1;
        }
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
    pub(crate) fn poll_thumb_results(&mut self) -> u64 {
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
    pub(crate) fn drain_thumb_extras(
        &mut self,
        layers: &[PhotoLayerInfo],
    ) -> Vec<PhotoEngineResponse> {
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
}
