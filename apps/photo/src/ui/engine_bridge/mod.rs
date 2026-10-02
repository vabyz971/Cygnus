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

// Les réexports conservent les chemins publics de l'ancien module
// unique (dont ceux utilisés par les tests) : certains ne servent
// qu'en `#[cfg(test)]`, d'où la tolérance ciblée.
#![allow(unused_imports)]

//! Pont non bloquant vers le worker `photo-engine` (pattern v2 §3.4).
//!
//! La boucle egui ne touche JAMAIS au `Document` : elle envoie des
//! [`PhotoEngineCommand`](commands::PhotoEngineCommand) via `Sender` et poll les
//! [`PhotoEngineResponse`](responses::PhotoEngineResponse) via `try_recv` à chaque frame.
//!
//! Découpage (déplacement pur, zéro changement de logique) :
//! - [`commands`] : commandes, routage de rendu, coalescence de lots ;
//! - [`responses`] : réponses du worker vers l'UI ;
//! - [`metrics`] : métriques et timings ;
//! - [`preview`] : aperçus composites (legacy + incrémental) ;
//! - [`thumbs`] : miniatures asynchrones de l'`EngineWorker` ;
//! - [`worker`] : worker (document vivant, historique, application).

pub(crate) fn assert_send<T: Send>() {}

pub mod commands;
pub mod metrics;
pub mod preview;
pub mod responses;
pub mod thumbs;
pub mod worker;

pub use commands::{PhotoEngineCommand, RenderInvalidation, render_routing};
pub use metrics::{OpMetrics, PreviewTimings, WorkerMetrics};
pub use preview::{PREVIEW_MAX_DIMENSION, PreviewImage, render_preview, render_preview_timed};
pub use responses::PhotoEngineResponse;
pub use worker::{EngineWorker, HISTORY_LIMIT, apply_command, spawn_photo_engine_worker};
