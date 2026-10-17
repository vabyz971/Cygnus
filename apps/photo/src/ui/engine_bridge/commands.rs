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

//! Engine bridge commands: [`PhotoEngineCommand`], render routing
//! ([`render_routing`]) and batch coalescing ([`fold_batch`]).

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

/// Vector shape created from the Layers panel (bottom bar menu).
///
/// The worker sizes and centers the shape on the document — the UI
/// only chooses the kind (no geometry crosses the channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewVectorShape {
    /// Axis-aligned rectangle.
    Rect,
    /// Ellipse.
    Ellipse,
    /// Regular hexagon.
    Polygon,
    /// Five-branch star.
    Star,
}

/// UI → engine worker commands.
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoEngineCommand {
    /// Toggle a layer's visibility.
    ToggleLayerVisibility(Uuid),
    /// Solo a layer (Affinity-style isolate): only it and its
    /// ancestor chain stay visible. Single history entry.
    IsolateLayer(Uuid),
    /// Reorder by ids (hierarchical before/after drop: same
    /// parent or not — free nesting via the engine's `reorder_before`).
    ReorderNodes {
        /// Moved layer.
        dragged: Uuid,
        /// Target layer.
        target: Uuid,
        /// True = before the target, false = after.
        before: bool,
    },
    /// Nest a layer at a parent's head (engine `move_into_parent`:
    /// group as-is, pixel layer as a clipped child).
    MoveIntoParent {
        /// Moved layer.
        layer: Uuid,
        /// Host parent (group or pixel layer).
        parent: Uuid,
    },
    /// Collapse / expand a group (display only: snapshot without
    /// a new composite).
    ToggleGroupCollapsed(Uuid),
    /// Set a layer's opacity (engine units 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Set a layer's blend mode (discrete choice).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Undo the last mutation.
    Undo,
    /// Redo the last undo.
    Redo,
    /// Undo up to `steps` mutations (History panel
    /// state jump, O003): applies whatever exists (at most the stack),
    /// a single render.
    UndoSteps {
        /// Number of requested backward steps.
        steps: u32,
    },
    /// Redo up to `steps` mutations (O003, symmetric).
    RedoSteps {
        /// Number of requested forward steps.
        steps: u32,
    },
    /// Commit a brush/eraser stroke on a pixel layer.
    ///
    /// `points` in image pixels (viewport world space). The
    /// layer transform is assumed identity in Phase 4 (the fresh
    /// photo layer case); the full transform-aware commit
    /// (see `paint::commit_stroke`) will come in Phase 5.
    PaintStroke {
        /// Target pixel layer.
        layer: Uuid,
        /// Stroke polyline (image pixels).
        points: Vec<(f32, f32)>,
        /// True = eraser (lowers alpha), false = brush.
        eraser: bool,
        /// Radius in image pixels.
        radius: f32,
        /// RGB color (ignored in eraser mode).
        color: [u8; 3],
        /// Stroke opacity 0..=1.
        opacity: f32,
    },
    /// Move a pixel layer by `(dx, dy)` document pixels
    /// (select/move tool: drag committed on release).
    MoveLayer {
        /// Target pixel layer.
        layer: Uuid,
        /// Horizontal offset in document pixels.
        dx: f32,
        /// Vertical offset in document pixels.
        dy: f32,
    },
    /// Replace a pixel layer's transform wholesale (canvas transform
    /// overlay: move/scale/rotate handles). Absolute values, not
    /// deltas — consecutive drag frames keep only the latest
    /// (batch fold). `commit` ends the gesture: the coalescing slot
    /// is released so the next gesture opens a new history entry.
    SetTransform {
        /// Target pixel layer.
        layer: Uuid,
        /// New transform (document space).
        transform: photo_engine::Transform2D,
        /// True on release (gesture boundary).
        commit: bool,
    },
    /// Replace a vector layer's paths wholesale (node editor:
    /// absolute values, not deltas — consecutive drag frames keep
    /// only the latest). `commit` ends the gesture like
    /// `SetTransform`.
    SetVectorPaths {
        /// Target vector layer.
        layer: Uuid,
        /// New paths (layer space).
        paths: Vec<photo_engine::VectorPath>,
        /// True on release (gesture boundary).
        commit: bool,
    },
    /// Paint into a mask (brush = white/reveals, eraser =
    /// black/hides — Affinity convention, brush color
    /// ignored). Points in image pixels, like `PaintStroke`.
    PaintMask {
        /// Owner (layer or filter sub-layer).
        owner: Uuid,
        /// Targeted mask.
        mask: Uuid,
        /// Stroke polyline (image pixels).
        points: Vec<(f32, f32)>,
        /// True = eraser (black), false = brush (white).
        eraser: bool,
        /// Radius in image pixels.
        radius: f32,
        /// Stroke opacity 0..=1.
        opacity: f32,
    },
    /// Open an image as a new layer (heavy decode, already
    /// on the worker thread hence non-blocking for the UI).
    OpenImage {
        /// Image file path.
        path: PathBuf,
    },
    /// Add a transparent empty layer (document dimensions).
    AddEmptyLayer,
    /// Add a vector shape layer (worker-sized, document-centered).
    AddVectorLayer {
        /// Shape kind to generate.
        shape: NewVectorShape,
    },
    /// Duplicate a layer (new ids, inserted above).
    DuplicateLayer(Uuid),
    /// Duplicate a live filter (new id, inserted above in its chain).
    DuplicateFilter { layer: Uuid, filter: Uuid },
    /// Horizontal mirror of the pixel layer (O004, destructive + history).
    FlipHorizontal(Uuid),
    /// Vertical mirror of the pixel layer (O004).
    FlipVertical(Uuid),
    /// 90° clockwise rotation, center kept (O004).
    RotateClockwise(Uuid),
    /// 90° counterclockwise rotation (O004).
    RotateCounterclockwise(Uuid),
    /// Crop the pixel layer to its intersection with the document
    /// (O004): no overlap = clean error, already contained =
    /// no-op (no history entry).
    CropToDocument(Uuid),
    /// Delete a layer (refused when it is the last one).
    DeleteLayer(Uuid),
    /// Add a live filter to a pixel layer (registry `type_id`).
    AddFilter {
        /// Target pixel layer.
        layer: Uuid,
        /// Filter id (e.g. "brightness_contrast").
        filter_type: String,
    },
    /// Rename a layer, filter, or mask (non-empty name).
    RenameLayer { layer: Uuid, name: String },
    /// Add a white mask (all visible) to the owning layer.
    AddMask { layer: Uuid },
    /// Delete a mask from its owner.
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Enable/disable a live filter (toggle, keeping its settings).
    ToggleFilter {
        /// Owning pixel layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
    },
    /// Set a float parameter of a live filter (inspector
    /// sliders, continuous gesture coalesced like opacity).
    SetFilterParam {
        /// Owning pixel layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
        /// Parameter key (e.g. `"brightness"`).
        key: String,
        /// New value.
        value: f32,
    },
    /// Set a text parameter of a live filter (curve points, O005),
    /// coalesced like [`Self::SetFilterParam`].
    SetFilterText {
        /// Owning pixel layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
        /// Parameter key (e.g. `"points"`).
        key: String,
        /// New value.
        value: String,
    },
    /// Install a built-in preset on a live filter (O005): complete
    /// parameter set, one discrete history step.
    ApplyFilterPreset {
        /// Owning pixel layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
        /// Preset id (`photo_engine::presets_for`).
        preset: String,
    },
    /// Enable/disable a mask (toggle, keeping its pixels).
    ToggleMask {
        /// Owner (layer or filter sub-layer).
        owner: Uuid,
        /// Targeted mask.
        mask: Uuid,
    },
    /// Move a filter within its layer's stack (`up` = toward the displayed top).
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Move a live filter from one chain to another (pixel or group
    /// chains, same owner allowed for exact reordering): detached from
    /// `from`, inserted at clamped `index` in `to`. Identity, params
    /// and its own masks travel along. Single history entry.
    MoveFilterTo {
        /// Filter being moved.
        filter: Uuid,
        /// Source chain owner (pixel layer or group).
        from: Uuid,
        /// Destination chain owner (pixel layer or group).
        to: Uuid,
        /// Insertion index in the destination chain (clamped).
        index: usize,
    },
    /// Move a mask within its owner's stack.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Move a mask from one pile to another (layer, group, adjustment
    /// or live-filter piles): detached from `from`, inserted at clamped
    /// `index` in `to`. Pixels, enabled state and inversion travel
    /// along. Single history entry.
    MoveMaskTo {
        /// Mask being moved.
        mask: Uuid,
        /// Source pile owner (layer or filter id).
        from: Uuid,
        /// Destination pile owner (layer or filter id).
        to: Uuid,
        /// Insertion index in the destination pile (clamped).
        index: usize,
    },
    /// Delete a filter from a pixel layer.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Export the composite (format inferred from the extension: png, jpg,
    /// jpeg, webp, tif, tiff, gif — gif = first frame, no animated
    /// transparency).
    Export {
        /// Destination path.
        path: PathBuf,
        /// JPEG quality 1..=100 (ignored outside JPEG).
        quality: u8,
    },
    /// Split the stack around a layer about to be dragged (drag proxy):
    /// replies `DragProxyReady` (below / layer / above textures) or
    /// `DragProxyUnavailable` (stack not splittable). No history, no
    /// mutation: the canvas then moves the layer GPU-side until release.
    PrepareDragProxy {
        /// Layer about to be dragged.
        layer: Uuid,
    },
    /// Estimate the exported file size (O006, export dialog): replies
    /// `ExportSizeEstimated`, no history, no render.
    EstimateExportSize {
        /// Format extension (`"png"`, `"jpg"`, `"webp"`, `"tiff"`…).
        ext: String,
        /// JPEG quality 1..=100 (ignored outside JPEG).
        quality: u8,
    },
    /// Resync without mutation (initial snapshot at boot).
    Refresh,
    /// Save the document into a `.cygp` (O001: Save/Save As).
    /// The path is already resolved by the app (local state); the worker
    /// only serializes via `photo_engine::project::save`.
    SaveProject {
        /// Destination path (`.cygp` extension handled by the picker).
        path: PathBuf,
    },
    /// Load a `.cygp` replacing the live document (O001:
    /// Open). History and caches drained: new editing session.
    /// The UI title/path are set by the app at dispatch (local
    /// state); the worker only returns the composite (`LayersChanged`)
    /// or `EngineError` (foreign version, corrupt file).
    LoadProject {
        /// Project path to load.
        path: PathBuf,
    },
    /// Clip the preview to the document dimensions (View menu).
    /// `false` (default) = infinite plane: off-document content stays
    /// visible around the frame.
    SetPreviewClip {
        /// True = preview clipped to the document.
        clip: bool,
    },
}

impl PhotoEngineCommand {
    /// Stable command name (instrumentation only).
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::ToggleLayerVisibility(_) => "toggle_visibility",
            Self::IsolateLayer(_) => "isolate_layer",
            Self::ReorderNodes { .. } => "reorder_nodes",
            Self::MoveIntoParent { .. } => "move_into_parent",
            Self::ToggleGroupCollapsed(_) => "toggle_collapsed",
            Self::SetOpacity { .. } => "set_opacity",
            Self::SetBlendMode { .. } => "set_blend_mode",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::UndoSteps { .. } => "undo_steps",
            Self::RedoSteps { .. } => "redo_steps",
            Self::PaintStroke { .. } => "paint_stroke",
            Self::MoveLayer { .. } => "move_layer",
            Self::SetTransform { .. } => "set_transform",
            Self::SetVectorPaths { .. } => "set_vector_paths",
            Self::PaintMask { .. } => "paint_mask",
            Self::OpenImage { .. } => "open_image",
            Self::AddEmptyLayer => "add_empty_layer",
            Self::AddVectorLayer { .. } => "add_vector_layer",
            Self::DuplicateLayer(_) => "duplicate_layer",
            Self::DuplicateFilter { .. } => "duplicate_filter",
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
            Self::MoveFilterTo { .. } => "move_filter_to",
            Self::ToggleFilter { .. } => "toggle_filter",
            Self::SetFilterParam { .. } => "set_filter_param",
            Self::SetFilterText { .. } => "set_filter_text",
            Self::ApplyFilterPreset { .. } => "apply_filter_preset",
            Self::ToggleMask { .. } => "toggle_mask",
            Self::MoveMask { .. } => "move_mask",
            Self::MoveMaskTo { .. } => "move_mask_to",
            Self::RemoveFilter { .. } => "remove_filter",
            Self::Export { .. } => "export",
            Self::EstimateExportSize { .. } => "estimate_export_size",
            Self::PrepareDragProxy { .. } => "prepare_drag_proxy",
            Self::SaveProject { .. } => "save_project",
            Self::LoadProject { .. } => "load_project",
            Self::Refresh => "refresh",
            Self::SetPreviewClip { .. } => "set_preview_clip",
        }
    }
}

/// Render invalidation induced by a command: separates what only
/// touches document state (no new composite) from what
/// requires a re-render. Order: `Unchanged < StateOnly < Composite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderInvalidation {
    /// No state change (no-op): nothing to produce.
    Unchanged,
    /// State changed, identical pixels (e.g. rename): snapshot only.
    StateOnly,
    /// Potentially changed pixels: new composite required.
    Composite,
}

/// Routing of a command to its semantic render event
/// ([`RenderEvent`], `photo-engine::command` machinery) and its
/// concrete invalidation in the current CPU pipeline.
///
/// Note: `SetOpacity`/`MoveLayer` are `NodeInvalidated` in the
/// `Command::affects_composite` sense (meant for a future GPU draw), but
/// the CPU composite must still be re-blended → `Composite`.
/// Rename and group collapse are purely `StateOnly`.
pub fn render_routing(command: &PhotoEngineCommand) -> (RenderEvent, RenderInvalidation) {
    match command {
        PhotoEngineCommand::ToggleLayerVisibility(_) => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::IsolateLayer(_) => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Composite)
        }
        PhotoEngineCommand::ReorderNodes { .. } | PhotoEngineCommand::MoveIntoParent { .. } => {
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
        PhotoEngineCommand::SetTransform { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::Composite,
        ),
        PhotoEngineCommand::SetVectorPaths { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::Composite,
        ),
        PhotoEngineCommand::PaintStroke { layer, .. } => (
            RenderEvent::NodeInvalidated(*layer),
            RenderInvalidation::Composite,
        ),
        PhotoEngineCommand::PaintMask { owner, .. } => (
            RenderEvent::NodeInvalidated(*owner),
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
        | PhotoEngineCommand::AddVectorLayer { .. }
        | PhotoEngineCommand::DuplicateLayer(_)
        | PhotoEngineCommand::DuplicateFilter { .. }
        | PhotoEngineCommand::DeleteLayer(_)
        | PhotoEngineCommand::AddFilter { .. }
        | PhotoEngineCommand::SetFilterParam { .. }
        | PhotoEngineCommand::SetFilterText { .. }
        | PhotoEngineCommand::ApplyFilterPreset { .. }
        | PhotoEngineCommand::ToggleFilter { .. }
        | PhotoEngineCommand::ToggleMask { .. }
        | PhotoEngineCommand::MoveFilter { .. }
        | PhotoEngineCommand::MoveFilterTo { .. }
        | PhotoEngineCommand::RemoveFilter { .. }
        | PhotoEngineCommand::AddMask { .. }
        | PhotoEngineCommand::RemoveMask { .. }
        | PhotoEngineCommand::MoveMask { .. }
        | PhotoEngineCommand::MoveMaskTo { .. }
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
        PhotoEngineCommand::Export { .. }
        | PhotoEngineCommand::EstimateExportSize { .. }
        | PhotoEngineCommand::PrepareDragProxy { .. }
        | PhotoEngineCommand::SaveProject { .. } => {
            (RenderEvent::FullInvalidation, RenderInvalidation::Unchanged)
        }
    }
}

/// Dirty marking carried by a mutation (Phase 6E): centralized in
/// `mutate()`, never asked of UI callers.
///
/// By default: `Composite` ⇒ `Global` (safe: any new command at worst
/// forgets precision, never correctness), else `None`. Arms
/// that know their zone (painting, moving…) refine to `Region`.
/// Applied to the worker cache right after the match, before `respond`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum DirtyMark {
    /// Derive from routing (`Composite` ⇒ `Global`, else `None`).
    #[default]
    Auto,
    /// No zone (no-op, state only).
    None,
    /// Precise DOCUMENT SPACE zone (footprint + halo already included).
    Region(TileRegion),
    /// The whole document (structural, global, safe fallback).
    Global,
}

/// Thumbnail send decision (Phase 6G.3 §17, pure and deterministic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmitDecision {
    /// Store already up to date: nothing to do.
    Fresh,
    /// Same version already pending: absorbed intent (coalescing).
    Coalesce,
    /// New intent to forward.
    Submit,
}

/// Result of a render-less mutation: immediate responses (export,
/// errors) + invalidation to produce next.
pub(crate) struct Mutation {
    /// Responses to send as-is (no associated render).
    pub(crate) immediates: Vec<PhotoEngineResponse>,
    /// Cumulative mutation invalidation.
    pub(crate) invalidation: RenderInvalidation,
    /// True when document state changed (snapshot to resend).
    pub(crate) changed: bool,
    /// Dirty marking (safe default from `invalidation`, refined per arm).
    pub(crate) dirty: DirtyMark,
}

impl Mutation {
    /// Effective mutation: a render/response will follow.
    pub(crate) fn changed(invalidation: RenderInvalidation) -> Self {
        Self {
            immediates: Vec::new(),
            invalidation,
            changed: true,
            dirty: DirtyMark::Auto,
        }
    }

    /// No-op (unknown id, guard): state unchanged.
    pub(crate) fn unchanged() -> Self {
        Self {
            immediates: Vec::new(),
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
            dirty: DirtyMark::None,
        }
    }

    /// Immediate response without rendering (export, non-blocking error).
    pub(crate) fn immediate(response: PhotoEngineResponse) -> Self {
        Self {
            immediates: vec![response],
            invalidation: RenderInvalidation::Unchanged,
            changed: false,
            dirty: DirtyMark::None,
        }
    }

    /// Refines the marking with a precise zone (consumes `self`).
    pub(crate) fn with_dirty(mut self, dirty: DirtyMark) -> Self {
        self.dirty = dirty;
        self
    }
}

/// Precise marking for a bearer node (footprint + exact spread):
/// `EMPTY` (adjustment, empty layer) ⇒ `Global` fallback — never a silent
/// empty region (an adjustment touches the whole accumulator).
pub(crate) fn mark_for_node(node: &photo_engine::LayerNode) -> DirtyMark {
    let region = photo_engine::tiles::node_mark_region(node);
    if region.is_empty() {
        DirtyMark::Global
    } else {
        DirtyMark::Region(region)
    }
}

/// Latest-value-wins folding (see `engine_runtime::batch`):
/// consecutive `SetOpacity` on the same layer keep only the last
/// value (same for `SetFilterParam` per layer/filter/key,
/// `SetTransform` / `SetVectorPaths` per layer), consecutive
/// `MoveLayer` are summed (zero sum = dropped command). Intermediate
/// values therefore never fill the render queue, and history keeps one
/// entry per gesture (folding never crosses `Undo`/`Redo`).
impl engine_runtime::Coalesce for PhotoEngineCommand {
    fn absorb(&mut self, command: &Self) -> bool {
        match (self, command) {
            (
                PhotoEngineCommand::SetOpacity {
                    layer: prev,
                    opacity: prev_op,
                },
                PhotoEngineCommand::SetOpacity { layer, opacity },
            ) if prev == layer => {
                *prev_op = *opacity;
                true
            }
            (
                PhotoEngineCommand::SetFilterParam {
                    layer: prev_layer,
                    filter: prev_filter,
                    key: prev_key,
                    value: prev_value,
                },
                PhotoEngineCommand::SetFilterParam {
                    layer,
                    filter,
                    key,
                    value,
                },
            ) if prev_layer == layer && prev_filter == filter && prev_key == key => {
                *prev_value = *value;
                true
            }
            (
                PhotoEngineCommand::MoveLayer {
                    layer: prev,
                    dx,
                    dy,
                },
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
            (
                PhotoEngineCommand::SetTransform {
                    layer: prev,
                    transform: prev_t,
                    commit: prev_c,
                },
                PhotoEngineCommand::SetTransform {
                    layer,
                    transform,
                    commit,
                },
            ) if prev == layer => {
                *prev_t = *transform;
                *prev_c = *commit;
                true
            }
            (
                PhotoEngineCommand::SetVectorPaths {
                    layer: prev,
                    paths: prev_paths,
                    commit: prev_c,
                },
                PhotoEngineCommand::SetVectorPaths {
                    layer,
                    paths,
                    commit,
                },
            ) if prev == layer => {
                *prev_paths = paths.clone();
                *prev_c = *commit;
                true
            }
            _ => false,
        }
    }

    fn is_noop(&self) -> bool {
        matches!(self, PhotoEngineCommand::MoveLayer { dx, dy, .. } if *dx == 0.0 && *dy == 0.0)
    }
}

/// Folds a batch of pending commands BEFORE application
/// (`engine_runtime::fold_batch` over [`engine_runtime::Coalesce`]).
pub(crate) fn fold_batch(commands: Vec<PhotoEngineCommand>) -> Vec<PhotoEngineCommand> {
    engine_runtime::fold_batch(commands)
}
