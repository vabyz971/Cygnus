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
#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.

//! Worker-to-UI responses: [`PhotoEngineResponse`].

use super::preview::{PreviewImage, ProxyLayerImage};
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

/// Worker → UI responses.
#[derive(Debug, Clone)]
pub enum PhotoEngineResponse {
    /// Fresh state after application, with a new composite.
    /// `revision` identifies the render: the UI ignores any preview
    /// whose revision is not newer than the displayed texture.
    LayersChanged {
        /// Snapshot for the UI.
        layers: Vec<PhotoLayerInfo>,
        /// Composite preview for the canvas (`None` when empty).
        preview: Option<PreviewImage>,
        /// Produced render revision.
        revision: RenderRevision,
        /// History depths (to grey out undo/redo).
        can_undo: bool,
        /// Redo depth.
        can_redo: bool,
        /// Undoable step labels, oldest to newest
        /// (History panel, O003).
        undo_labels: Vec<String>,
        /// Redoable step labels, oldest to
        /// newest (the UI shows the last one first = next redo).
        redo_labels: Vec<String>,
    },
    /// Fresh state WITHOUT a new render (e.g. rename, no-op): the UI
    /// updates the panel and keeps the displayed texture.
    StateChanged {
        /// Snapshot for the UI.
        layers: Vec<PhotoLayerInfo>,
        /// Last render revision (unchanged, texture kept).
        revision: RenderRevision,
        /// History depths (to grey out undo/redo).
        can_undo: bool,
        /// Profondeur redo.
        can_redo: bool,
        /// Undoable step labels (O003, like `LayersChanged`).
        undo_labels: Vec<String>,
        /// Redoable step labels (O003).
        redo_labels: Vec<String>,
    },
    /// Secondary thumbnail arrived AFTER the canvas (Phase 6G.3): the
    /// previous `LayersChanged` carried the canvas preview + thumbnails
    /// potentially one frame stale; this one syncs the
    /// panel without ever re-rendering the canvas. The UI applies it if and
    /// only if its version is newer than the cache (UI-side
    /// anti-staleness guard, on top of the worker guard).
    ThumbnailUpdated {
        /// Concerned layer.
        layer: Uuid,
        /// Thumbnail + computed appearance version.
        thumb: PhotoLayerThumb,
    },
    /// Non-blocking failure (e.g. undecodable).
    EngineError {
        /// Message displayable in the status bar.
        message: String,
    },
    /// PNG export finished.
    ExportDone {
        /// Written file path.
        path: PathBuf,
    },
    /// Drag proxy ready (see `PrepareDragProxy`): flattened stack
    /// below and above the layer (`None` = nothing there) plus the layer
    /// appearance, drawn GPU-side through the live transform.
    DragProxyReady {
        /// Dragged layer.
        layer: Uuid,
        /// Composite of everything below.
        below: Option<PreviewImage>,
        /// The layer's own pixels (filters and masks baked).
        image: ProxyLayerImage,
        /// Composite of everything above, over transparency.
        above: Option<PreviewImage>,
    },
    /// The stack cannot be split around `layer` (non-Normal blend,
    /// adjustment above, nested layer…): the drag keeps live renders.
    DragProxyUnavailable {
        /// Requested layer.
        layer: Uuid,
    },
    /// Export size estimate (O006): `bytes` = `None` when the
    /// document is empty or the format cannot be estimated.
    ExportSizeEstimated {
        /// Format extension the estimate was computed for.
        ext: String,
        /// JPEG quality used (echoed so the UI can match its request).
        quality: u8,
        /// Estimated byte count.
        bytes: Option<u64>,
    },
    /// `.cygp` project saved (O001: Save/Save As).
    ProjectSaved {
        /// Written file path.
        path: PathBuf,
    },
}

// ---------------------------------------------------------------------------
// TEMPORARY INSTRUMENTATION (perf diagnostics, no functional
// impact): worker timings + counters, to remove once the
// report is validated.
// ---------------------------------------------------------------------------
