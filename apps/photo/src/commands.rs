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

//! Photo app UI commands: the only panels → app channel.
//!
//! Required flow:
//!
//! ```text
//! UI (panels, viewport, menus)
//!   ↓ PhotoAction
//! PhotoApp::handle_action
//!   ↓ PhotoEngineCommand
//! Engine (worker)
//! ```
//!
//! Panels NEVER touch the worker: they push
//! [`PhotoAction`] into the [`PhotoCommandQueue`] (or return them
//! as a `Vec`, drained by the workspace). [`PhotoUiContext`] carries the
//! shared display dependencies (theme, icons, translation) —
//! neither documents nor engines.

use crate::layout::dock::PhotoDockTab;
use crate::ui::{NewVectorShape, PaintRequest, PhotoCanvasTool};
use cygnus_i18n::{Catalog, Language};
use cygnus_ui::context::UiContext;
use cygnus_ui::icons::IconRegistry;
use cygnus_ui::theme::UiThemeExt;
use photo_engine::BlendMode;
use std::path::PathBuf;
use uuid::Uuid;

/// UI action emitted by a panel, the viewport, or a menu.
///
/// Layer ids are resolved at emission (the panel knows the
/// selection); `PhotoApp` only has to route to local state or
/// the worker.
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoAction {
    /// Open the "New document" window.
    OpenNewDocumentDialog,
    /// Create a document with the validated dimensions (new tab).
    CreateDocument { width: u32, height: u32 },
    /// Open the image file picker.
    OpenImageDialog,
    /// Open the `.cygp` project file picker (O001).
    OpenProjectDialog,
    /// Save the active project (known path) or fall back to
    /// "Save as" (O001).
    SaveProject,
    /// Open the `.cygp` "Save as" file picker (O001).
    SaveProjectAsDialog,
    /// Open the "Export" window.
    OpenExportDialog,
    /// Export the active document (folder created by the app if needed).
    ExportDocument { path: PathBuf, quality: u8 },
    /// Ask the worker to split the stack around a layer about to be
    /// dragged (GPU drag proxy, no history).
    PrepareDragProxy { layer: Uuid },
    /// Ask the worker for an export size estimate (O006, dialog).
    EstimateExportSize { ext: String, quality: u8 },
    /// Close the active tab.
    CloseTab,
    /// Undo / redo (worker).
    Undo,
    /// Redo (worker).
    Redo,
    /// Jump `steps` steps back (History panel, O003).
    HistoryBack(u32),
    /// Jump `steps` steps forward (O003).
    HistoryForward(u32),
    /// New empty layer (worker).
    AddEmptyLayer,
    /// Add a vector shape layer (worker-sized, document-centered).
    AddVectorLayer {
        /// Shape kind to generate.
        shape: NewVectorShape,
    },
    /// Layer from an image (file picker).
    AddImageLayer,
    /// Duplicate the selected layer (worker, no-op if none).
    DuplicateSelectedLayer,
    /// Horizontal mirror of the selected layer (O004, worker).
    FlipHorizontalSelected,
    /// Vertical mirror of the selected layer (O004, worker).
    FlipVerticalSelected,
    /// 90° clockwise rotation of the selected layer (O004, worker).
    RotateClockwiseSelected,
    /// 90° counterclockwise rotation of the selected layer (O004, worker).
    RotateCounterclockwiseSelected,
    /// Crop the selected layer to the document (O004, worker).
    CropSelectedToDocument,
    /// Delete the selected layer (worker, no-op if none).
    DeleteSelectedLayer,
    /// Delete layer `layer` (worker, from its row).
    DeleteLayer(Uuid),
    /// Duplicate layer `layer` (worker, context menu).
    DuplicateLayer(Uuid),
    /// Duplicate a live filter (worker, context menu).
    DuplicateFilter { layer: Uuid, filter: Uuid },
    /// Add a mask to the selected layer (worker, no-op if none).
    AddMaskToSelected,
    /// Add a mask to a live filter's own pile (worker).
    AddMaskToFilter {
        /// Owning filter.
        filter: Uuid,
    },
    /// Add a live filter (worker).
    AddFilter { layer: Uuid, filter_type: String },
    /// Tune a float parameter of a live filter (worker, coalesced
    /// continuous gesture).
    SetFilterParam {
        /// Owning layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
        /// Parameter key (e.g. `"brightness"`).
        key: String,
        /// New value.
        value: f32,
    },
    /// Set a mask's density, feather and inversion (worker, coalesced
    /// continuous gesture).
    SetMaskParams {
        /// Mask owner.
        owner: Uuid,
        /// Targeted mask.
        mask: Uuid,
        /// Density 0..=1.
        density: f32,
        /// Feather radius (mask pixels).
        feather: f32,
        /// Inverted coverage.
        inverted: bool,
    },
    /// Set a text parameter of a live filter (curve points, O005;
    /// worker, coalesced continuous gesture).
    SetFilterText {
        /// Owning layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
        /// Parameter key (e.g. `"points"`).
        key: String,
        /// New value.
        value: String,
    },
    /// Install a built-in adjustment preset (O005, one history step).
    ApplyFilterPreset {
        /// Owning layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
        /// Preset id (`photo_engine::presets_for`).
        preset: String,
    },
    /// Select a layer (local UI state).
    SelectLayer(Uuid),
    /// Focus an attachment (local UI state, owner selected too).
    FocusAttachment {
        /// Owner (layer id, or filter id for filter-owned masks).
        owner: Uuid,
        /// Targeted filter or mask.
        id: Uuid,
    },
    /// Enable/disable a filter (worker).
    ToggleFilter {
        /// Owning layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
    },
    /// Enable/disable a mask (worker).
    ToggleMask {
        /// Owner.
        owner: Uuid,
        /// Targeted mask.
        mask: Uuid,
    },
    /// Rename a layer (worker).
    RenameLayer { layer: Uuid, name: String },
    /// Reorder by ids (hierarchical drag & drop, worker):
    /// `dragged` before (`before`) or after (`!before`) `target`
    /// (same parent or not — free nesting).
    ReorderNodes {
        /// Moved layer.
        dragged: Uuid,
        /// Target layer.
        target: Uuid,
        /// True = before the target, false = after.
        before: bool,
    },
    /// Nest a layer at the head of a parent (worker): group as-is,
    /// pixel layer as a clipped child.
    MoveIntoParent {
        /// Moved layer.
        layer: Uuid,
        /// Host parent (group or pixel layer).
        parent: Uuid,
    },
    /// Collapse / expand a group (worker, no re-render).
    ToggleGroupCollapsed(Uuid),
    /// Toggle visibility (worker).
    ToggleLayerVisibility(Uuid),
    /// Solo a layer (worker): only it and its ancestors stay visible.
    IsolateLayer(Uuid),
    /// Set opacity (engine units 0..=100, worker).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Set the blend mode (worker).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Move a live filter from one chain to another (worker, single
    /// history entry): pixel or group chains, exact index.
    MoveFilterTo {
        /// Filter being moved.
        filter: Uuid,
        /// Source chain owner.
        from: Uuid,
        /// Destination chain owner.
        to: Uuid,
        /// Insertion index (clamped engine-side).
        index: usize,
    },
    /// Move a mask from one pile to another (worker, single history
    /// entry): layer, group, adjustment or filter piles, exact index.
    MoveMaskTo {
        /// Mask being moved.
        mask: Uuid,
        /// Source pile owner.
        from: Uuid,
        /// Destination pile owner.
        to: Uuid,
        /// Insertion index (clamped engine-side).
        index: usize,
    },
    /// Delete a filter (worker).
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Delete a mask (worker).
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Switch tool (local UI state).
    SetTool(PhotoCanvasTool),
    /// Toggle the grid (local UI state).
    ToggleGrid,
    /// Replace the active selection (`None` = deselect; mirrored to
    /// the worker, no history).
    SetSelection(Option<photo_engine::Selection>),
    /// Paint bucket click at document pixel `(x, y)`.
    FillAt {
        /// Document x.
        x: f32,
        /// Document y.
        y: f32,
    },
    /// Gradient from `start` to `end` (document pixels).
    FillGradient {
        /// Start point.
        start: (f32, f32),
        /// End point.
        end: (f32, f32),
    },
    /// Magic wand click at document pixel `(x, y)` (worker flood).
    MagicWand {
        /// Document x.
        x: f32,
        /// Document y.
        y: f32,
    },
    /// Select the whole document.
    SelectAll,
    /// Drop the selection.
    Deselect,
    /// Invert the selection.
    InvertSelection,
    /// Add a mask built from the selection to the selected layer.
    MaskFromSelection,
    /// Clip the preview to the document (worker, no history).
    TogglePreviewClip,
    /// Zoom in / out, anchored at center.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Reset zoom to 100%.
    ZoomReset,
    /// Commit a brush/eraser stroke (worker).
    CommitStroke(PaintRequest),
    /// Move a pixel layer (select tool, worker).
    MoveLayer { layer: Uuid, dx: f32, dy: f32 },
    /// Replace a pixel layer's transform (canvas transform overlay,
    /// worker). `commit` ends the drag gesture (history boundary).
    SetTransform {
        layer: Uuid,
        transform: photo_engine::Transform2D,
        commit: bool,
    },
    /// Replace a vector layer's paths (node editor, worker).
    /// `commit` ends the drag gesture (history boundary).
    SetVectorPaths {
        layer: Uuid,
        paths: Vec<photo_engine::VectorPath>,
        commit: bool,
    },
    /// Reopen a closed dock tab (tools, inspector, layers).
    ShowDockTab(PhotoDockTab),
    /// Hide a dock panel, keeping its slot (section menu).
    HideDockTab(PhotoDockTab),
    /// Restore the default dock layout.
    ResetDockLayout,
    /// Open the help window.
    ShowHelp,
    /// Quit the application.
    Quit,
}

/// UI action queue drained once per frame by `PhotoApp`.
///
/// Alias of the generic [`ActionQueue`](app_shell::ActionQueue)
/// instantiated on [`PhotoAction`].
pub type PhotoCommandQueue = app_shell::ActionQueue<PhotoAction>;

/// Display dependencies shared by the features.
///
/// Built once per frame ([`PhotoUiContext::for_frame_in`]):
/// cloned egui context, theme, icon registry, translator. No
/// documents, no engines, no mutable state — no GodContext.
#[derive(Debug, Clone)]
pub struct PhotoUiContext {
    /// Shared cygnus-ui dependencies.
    pub shared: UiContext,
}

impl PhotoUiContext {
    /// Test context (French catalog); the app uses
    /// [`Self::for_frame_in`] with its interface language.
    #[cfg(test)]
    pub fn for_frame(ctx: &egui::Context) -> Self {
        Self::for_frame_in(ctx, Language::Fr)
    }

    /// Per-frame context in the app's interface `language`.
    pub fn for_frame_in(ctx: &egui::Context, language: Language) -> Self {
        Self {
            shared: UiContext::new(
                ctx.clone(),
                ctx.cygnus_theme(),
                IconRegistry::new(),
                Catalog::new(language),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_preserves_emission_order() {
        let mut queue = PhotoCommandQueue::new();
        assert!(queue.is_empty());
        queue.push(PhotoAction::Undo);
        queue.extend([PhotoAction::Redo, PhotoAction::ZoomIn]);
        assert_eq!(queue.len(), 3);
        assert_eq!(
            queue.drain(),
            vec![PhotoAction::Undo, PhotoAction::Redo, PhotoAction::ZoomIn]
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn context_builds_for_frame() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let _ = photo_ctx.shared.theme();
        let _ = photo_ctx.shared.icons();
    }
}
