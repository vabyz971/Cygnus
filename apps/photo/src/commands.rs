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
use crate::ui::{PaintRequest, PhotoCanvasTool};
use photo_engine::BlendMode;
use std::path::PathBuf;
use ui_kit::context::UiContext;
use ui_kit::i18n::{Catalog, Language};
use ui_kit::icons::IconRegistry;
use ui_kit::theme::UiThemeExt;
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
    /// Add a mask to the selected layer (worker, no-op if none).
    AddMaskToSelected,
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
    /// Select a layer (local UI state).
    SelectLayer(Uuid),
    /// Focus an attachment (local UI state, owner selected too).
    FocusAttachment {
        /// Owning layer.
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
    /// Nest a layer at the head of a group (worker).
    MoveIntoGroup {
        /// Moved layer.
        layer: Uuid,
        /// Host group.
        group: Uuid,
    },
    /// Collapse / expand a group (worker, no re-render).
    ToggleGroupCollapsed(Uuid),
    /// Toggle visibility (worker).
    ToggleLayerVisibility(Uuid),
    /// Set opacity (engine units 0..=100, worker).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Set the blend mode (worker).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Move a filter within its stack (worker).
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Move a mask within its stack (worker).
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Delete a filter (worker).
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Delete a mask (worker).
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Switch tool (local UI state).
    SetTool(PhotoCanvasTool),
    /// Toggle the grid (local UI state).
    ToggleGrid,
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
    /// Reopen a closed dock tab (tools, inspector, layers).
    ShowDockTab(PhotoDockTab),
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
/// Built once per frame ([`PhotoUiContext::for_frame`]):
/// cloned egui context, theme, icon registry, translator. No
/// documents, no engines, no mutable state — no GodContext.
#[derive(Debug, Clone)]
pub struct PhotoUiContext {
    /// Shared ui-kit dependencies.
    pub shared: UiContext,
}

impl PhotoUiContext {
    /// Standard per-frame context (dark theme, icons,
    /// French catalog: app language until the language
    /// preference is wired).
    pub fn for_frame(ctx: &egui::Context) -> Self {
        Self {
            shared: UiContext::new(
                ctx.clone(),
                ctx.cygnus_theme(),
                IconRegistry::new(),
                Catalog::new(Language::Fr),
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
