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

//! Tâche secondaire de miniatures (Phase 6G.3) : le canvas d'abord.
//!
//! Le chemin canvas-critical (`PreviewSurface` → `PreviewImage`) ne doit
//! jamais attendre la génération des miniatures de calques (`thumb_buf` ≈
//! 16 ms à 2048², mesuré 6G.3.1). Ce module isole ce travail secondaire :
//!
//! NOTE D'ÉTUDE §9-§10 : pas de miniature incrémentale. Un resample partiel
//! exigerait de rejouer la comptabilité halo/voisinage de la chaîne
//! d'apparence par tuile de miniature — risque correctness pour un gain
//! incertain, alors que le coût restant (resample complet) est DÉJÀ hors
//! chemin critique. Réévaluer uniquement sur mesures futures l'exigeant.
//!
//! ```text
//! EngineWorker (thread principal)
//!   │ submit ThumbJob (légers : Arc + params clonés, jamais de Document)
//!   ▼
//! ThumbWorker — inline (tests, déterministe) ou thread dédié (production)
//!   │ compute : Renderer frais + appearance_frame (MÊME fonction que le
//!   │   chemin cadre → octets bit-identiques par construction)
//!   ▼
//! ThumbDone ──► ingestion versionnée (stale rejeté, jamais affiché)
//! ```
//!
//! Règles (spec 6G.3 §5-§8) :
//!
//! - le thread principal reste SEUL propriétaire du `Document` (aucun
//!   partage mutable) : seuls transitent des `Arc` immuables et params ;
//! - coalescing structurel : file d'intentions `BTreeMap` par calque (la
//!   plus récente gagne) + drain-remplace côté thread (les états
//!   intermédiaires supplantés ne sont jamais calculés) ;
//! - AUCUN résultat obsolète affiché : garde `(version, source Arc)` ;
//! - AUCUN nouveau cache d'apparences : le store applicatif réutilise la
//!   validité existante (`appearance_version`), le calcul réutilise
//!   `Renderer::appearance_frame`.

use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use image::DynamicImage;
use uuid::Uuid;

use photo_engine::{FilterLayer, LayerMask, RgbaBuf};

/// Demande de miniature : tout le nécessaire pour dériver image + miniature,
/// cloné depuis le calque vivant (léger : `Arc` + params, jamais de pixels
/// copiés, jamais de `Document`).
#[derive(Debug, Clone)]
pub struct ThumbJob {
    /// Calque cible.
    pub layer: Uuid,
    /// `appearance_version` capturée (garde de fraîcheur).
    pub version: u64,
    /// Image source (partagée, immuable).
    pub source: Arc<DynamicImage>,
    /// Chaîne de filtres (params clonés).
    pub filters: Vec<FilterLayer>,
    /// Masques (couvertures partagées).
    pub masks: Vec<LayerMask>,
}

/// Miniature calculée + coûts mesurés côté calcul.
#[derive(Clone)]
pub struct ThumbDone {
    /// Calque cible.
    pub layer: Uuid,
    /// Version calculée (à comparer au vivant).
    pub version: u64,
    /// Miniature (48×32, mêmes octets que le chemin cadre).
    pub thumb: RgbaBuf,
    /// Image d'apparence : payload diagnostic vérifié par les tests
    /// (jamais stockée ni affichée — seule `thumb` part en réponse).
    #[allow(dead_code)]
    pub image: Arc<DynamicImage>,
    /// Temps chaîne + bake (µs).
    pub render_us: u128,
    /// Temps resample seul (µs).
    pub thumb_us: u128,
}

impl std::fmt::Debug for ThumbDone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbDone")
            .field("layer", &self.layer)
            .field("version", &self.version)
            .field("thumb_dims", &(self.thumb.width, self.thumb.height))
            .field("render_us", &self.render_us)
            .field("thumb_us", &self.thumb_us)
            .finish_non_exhaustive()
    }
}

/// Compteurs cumulés (§14 6G.3).
#[derive(Debug, Default, Clone, Copy)]
pub struct ThumbStats {
    /// Demandes transmises (file ou inline).
    pub submitted: u64,
    /// Demandes ignorées car version déjà en attente (coalescing émetteur).
    pub coalesced: u64,
    /// Résultats appliqués (frais).
    pub completed: u64,
    /// Résultats rejetés (périmés).
    pub discarded: u64,
    /// Temps de calcul cumulé des résultats appliqués (µs).
    pub thumb_us: u128,
}

/// Calcule (image, miniature) depuis les pièces clonées — MÊME fonction que
/// le chemin cadre (`Renderer::appearance_frame` sur calque reconstruit) :
/// octets bit-identiques par construction (même code, mêmes entrées).
/// `None` si dimensions nulles (garde-fou, jamais en pratique).
pub fn compute_thumb(job: &ThumbJob) -> Option<(Arc<DynamicImage>, RgbaBuf, u128, u128)> {
    let mut layer = photo_engine::PixelLayer::new("thumb-job", Arc::clone(&job.source));
    layer.id = job.layer;
    layer.filter_layers = job.filters.clone();
    layer.masks = job.masks.clone();
    let mut renderer = photo_engine::renderer::Renderer::default();
    let t_render = std::time::Instant::now();
    let (image, _) = renderer.appearance_frame(&layer)?;
    let render_us = t_render.elapsed().as_micros();
    // Resample isolé mesuré séparément (§1 : il domine) — mêmes entrées,
    // même fonction que le chemin cadre.
    let t_thumb = std::time::Instant::now();
    let thumb = photo_engine::document::compositing::thumb_buf(&image);
    let thumb_us = t_thumb.elapsed().as_micros();
    Some((image, thumb, render_us, thumb_us))
}

/// File d'intentions : une entrée par calque, la plus récente gagne.
/// Pure et déterministe (testée) : 10 peintures rapides ⇒ 1 calcul.
pub fn select_latest(jobs: Vec<ThumbJob>) -> BTreeMap<Uuid, ThumbJob> {
    let mut pending = BTreeMap::new();
    for job in jobs {
        pending.insert(job.layer, job);
    }
    pending
}

/// Tâche secondaire : inline (tests, appels directs déterministes) ou thread
/// dédié (production, drain-remplace + calculs).
pub struct ThumbWorker {
    tx: Option<mpsc::Sender<ThumbJob>>,
    rx: Option<mpsc::Receiver<ThumbDone>>,
    // `None` en mode inline. Non joint à la destruction (même pattern que le
    // worker moteur : le thread sort seul quand l'émetteur est lâché).
    _handle: Option<JoinHandle<()>>,
}

impl ThumbWorker {
    /// Mode inline : `submit` calcule immédiatement (déterministe, sans
    /// thread — tests et `apply_command` éphémère).
    #[must_use]
    pub fn inline_() -> Self {
        Self {
            tx: None,
            rx: None,
            _handle: None,
        }
    }

    /// Mode threadé (production) : un thread dédié, file drain-remplace.
    #[must_use]
    pub fn spawned() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<ThumbJob>();
        let (done_tx, done_rx) = mpsc::channel::<ThumbDone>();
        let handle = std::thread::Builder::new()
            .name("cygnus-thumb".into())
            .spawn(move || Self::thread_loop(job_rx, done_tx))
            .ok();
        Self {
            tx: Some(job_tx),
            rx: Some(done_rx),
            _handle: handle,
        }
    }

    /// Vrai si un thread dédié tourne.
    #[must_use]
    pub fn is_threaded(&self) -> bool {
        self.tx.is_some()
    }

    /// Transmet une demande (sans bloquer). En mode threadé, le calcul peut
    /// être supplanté avant démarrage (coalescing). Retourne faux si le
    /// thread est tombé (l'appelant dégrade en inline sûr).
    pub fn submit(&self, job: ThumbJob) -> bool {
        match &self.tx {
            Some(tx) => tx.send(job).is_ok(),
            None => false,
        }
    }

    /// Calcule immédiatement (mode inline ET repli synchrone).
    /// Retourne le résultat complet (à ingérer via la garde de version).
    #[must_use]
    pub fn compute_now(job: &ThumbJob) -> Option<ThumbDone> {
        let (image, thumb, render_us, thumb_us) = compute_thumb(job)?;
        Some(ThumbDone {
            layer: job.layer,
            version: job.version,
            thumb,
            image,
            render_us,
            thumb_us,
        })
    }

    /// Draine les résultats disponibles (jamais bloquant).
    pub fn drain_results(&mut self) -> Vec<ThumbDone> {
        let mut out = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(done) = rx.try_recv() {
                out.push(done);
            }
        }
        out
    }

    /// Boucle du thread dédié : bloque sur la première demande, draine le
    /// reste via [`select_latest`] (les versions intermédiaires supplantées
    /// ne sont JAMAIS calculées), traite la plus récente par calque (ordre
    /// des ids, déterministe), publie. Sortie propre à la déconnexion.
    fn thread_loop(job_rx: mpsc::Receiver<ThumbJob>, done_tx: mpsc::Sender<ThumbDone>) {
        while let Ok(first) = job_rx.recv() {
            let mut batch = vec![first];
            while let Ok(job) = job_rx.try_recv() {
                batch.push(job);
            }
            let mut dead = false;
            for (_, job) in select_latest(batch) {
                let Some(done) = Self::compute_now(&job) else {
                    continue;
                };
                if done_tx.send(done).is_err() {
                    dead = true;
                    break;
                }
            }
            if dead {
                break;
            }
        }
    }
}

/// Garde de fraîcheur (§6-§7) : le résultat n'est applicable que si le
/// calque vivant porte encore EXACTEMENT cette version ET cette source
/// (`Arc::ptr_eq` : même allocation = mêmes pixels, les snapshots
/// d'historique préservant les `Arc`). Sinon : rejet (jamais affiché).
#[must_use]
pub fn is_fresh(
    done: &ThumbDone,
    live_version: u64,
    live_source: &Arc<DynamicImage>,
    job_source: &Arc<DynamicImage>,
) -> bool {
    done.version == live_version && Arc::ptr_eq(live_source, job_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_job(value: u8, version: u64) -> ThumbJob {
        use image::{ImageBuffer, Rgba};
        ThumbJob {
            layer: Uuid::new_v4(),
            version,
            source: Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
                64,
                64,
                Rgba([value, value, value, 255]),
            ))),
            filters: Vec::new(),
            masks: Vec::new(),
        }
    }

    #[test]
    fn calcul_egale_chemin_cadre() {
        // Même fonction, mêmes entrées : le job produit EXACTEMENT ce que le
        // chemin cadre produirait (pixels + miniature).
        let job = solid_job(77, 3);
        let done = ThumbWorker::compute_now(&job).expect("calcul");
        assert_eq!(done.version, 3);
        let mut renderer = photo_engine::renderer::Renderer::default();
        let mut layer = photo_engine::PixelLayer::new("x", Arc::clone(&job.source));
        layer.id = job.layer;
        let (image, thumb) = renderer.appearance_frame(&layer).expect("cadre");
        assert_eq!(
            done.image.to_rgba8().into_raw(),
            image.to_rgba8().into_raw()
        );
        assert_eq!(
            (
                done.thumb.width,
                done.thumb.height,
                done.thumb.data.as_ref()
            ),
            (thumb.width, thumb.height, thumb.data.as_ref())
        );
    }

    #[test]
    fn coalescing_ne_garde_que_le_dernier() {
        // 10 versions rapides du même calque ⇒ 1 seule intention.
        let id = Uuid::new_v4();
        let jobs: Vec<ThumbJob> = (0..10)
            .map(|v| ThumbJob {
                layer: id,
                version: v,
                ..solid_job(1, v)
            })
            .collect();
        let pending = select_latest(jobs);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[&id].version, 9, "la plus récente gagne");
    }

    #[test]
    fn garde_rejette_perime() {
        let job = solid_job(5, 10);
        let done = ThumbWorker::compute_now(&job).expect("calcul");
        // Vivant identique : frais.
        assert!(is_fresh(&done, 10, &job.source, &job.source));
        // Version avancée : périmé.
        assert!(!is_fresh(&done, 11, &job.source, &job.source));
        // Source remplacée (même version théorique) : périmé.
        let other = solid_job(5, 10);
        assert!(!is_fresh(&done, 10, &other.source, &job.source));
    }

    #[test]
    fn inline_sans_thread() {
        let worker = ThumbWorker::inline_();
        assert!(!worker.is_threaded());
        let job = solid_job(9, 1);
        // Sans thread : pas d'envoi possible → repli inline par l'appelant.
        assert!(!worker.submit(job));
    }
}
