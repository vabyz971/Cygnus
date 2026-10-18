// Cygnus — Professional open-source creative suite
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

// Re-exports keep the old single module's public paths
// (including test-used ones): some only serve
// under `#[cfg(test)]`, hence the targeted tolerance.
#![allow(unused_imports)]

//! Editing session: the engine worker the UI talks to (pattern v2 §3.4).
//!
//! The egui loop NEVER touches the `Document`: it sends
//! [`PhotoEngineCommand`](commands::PhotoEngineCommand) via `Sender` and polls
//! [`PhotoEngineResponse`](responses::PhotoEngineResponse) via `try_recv` each frame.
//!
//! Breakdown (pure move, zero logic change):
//! - [`commands`]: commands, render routing, batch coalescing;
//! - [`responses`]: worker-to-UI responses;
//! - [`metrics`]: metrics and timings;
//! - [`preview`]: composite previews (legacy + incremental);
//! - [`thumbs`]: async thumbnails of the `EngineWorker`;
//! - [`worker`]: worker (live document, history, application).

pub(crate) fn assert_send<T: Send>() {}

pub mod commands;
pub mod errors;
pub mod metrics;
pub mod naming;
pub mod preview;
pub mod responses;
pub mod thumb_worker;
pub mod thumbs;
pub mod worker;

pub use commands::{NewVectorShape, PhotoEngineCommand, RenderInvalidation, render_routing};
pub use errors::EngineError;
pub use metrics::{OpMetrics, PreviewTimings, WorkerMetrics};
pub use naming::DefaultNames;
pub use preview::{
    PREVIEW_MAX_DIMENSION, PROXY_LAYER_MAX_DIMENSION, PreviewImage, ProxyLayerImage,
    render_preview, render_preview_timed,
};
pub use responses::PhotoEngineResponse;
pub use worker::{EngineWorker, HISTORY_LIMIT, Waker, apply_command, spawn_photo_engine_worker};
