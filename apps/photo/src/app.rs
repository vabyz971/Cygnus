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

//! Photo app orchestrator: state, commands, engine, workspace.
//!
//! `PhotoApp` draws NOTHING itself: it owns the documents
//! (workers + states), drains the [`PhotoCommandQueue`] (routing to
//! local state or the worker) and calls [`PhotoWorkspace`] for one
//! frame. No panel, canvas, menu, or toolbar logic here.

use crate::commands::{PhotoAction, PhotoCommandQueue, PhotoUiContext};
use crate::layout::PhotoWorkspace;
use crate::layout::dock::{PhotoDockTab, default_tree, ensure_tab, push_canvas_tab};
use crate::persistence::{load_tree_or_default, load_workspace_or_default};
use crate::state::{
    OpenDocument, PhotoRuntimeState, PhotoShellState, PhotoUiState, apply_response, brush_luminance,
};
use crate::ui::features::layers::{
    find_attachment_in, find_layer_in, find_layer_in_mut, find_layer_of_attachment,
};
use crate::ui::{PhotoEngineCommand, spawn_photo_engine_worker};
use cygnus_ui::dialogs::{pick_image_to_open, pick_project_to_open, pick_project_to_save};
use cygnus_ui::layout::PanelId;
use photo_engine::Document;
use photo_engine::session::PaintTarget;
use std::path::Path;
use std::sync::mpsc::channel;

/// Photo app: tabbed documents + shell + runtime + action queue.
///
/// The app starts WITHOUT a document: the "Create document" window
/// opens for the first document (or an image). Zero open
/// documents is a normal state (welcome screen).
pub struct PhotoApp {
    /// Open documents (empty at startup).
    pub docs: Vec<OpenDocument>,
    /// Active document index (valid only when non-empty).
    pub active: usize,
    /// Global shell state (workspace, dialogs).
    pub shell: PhotoShellState,
    /// Ephemeral state (pickers, catalog, counters).
    pub runtime: PhotoRuntimeState,
    /// Pending UI actions (drained each frame).
    pub queue: PhotoCommandQueue,
    /// Interface language (French until the language preference is
    /// wired): UI texts and default names of created items.
    pub language: cygnus_i18n::Language,
    /// eframe's wgpu state: workers composite on this device and the
    /// canvas shows their frames without CPU readback (`None` in tests
    /// or with a non-wgpu backend: CPU previews only).
    render_state: Option<eframe::egui_wgpu::RenderState>,
    /// egui context the workers wake up when a response is ready
    /// (`None` in headless tests: polling on input still works).
    repaint: Option<egui::Context>,
}

impl PhotoApp {
    /// Creates the app without a document and opens the creation
    /// window (first document or image opening).
    pub fn new() -> Self {
        // Layout restore: no stored JSON yet
        // (disk wiring via `preferences`: next phase).
        let mut shell = PhotoShellState {
            workspace: load_workspace_or_default(None),
            ..Default::default()
        };
        // Photo has no timeline (yet): bottom region hidden.
        shell.workspace.set_visible(PanelId::Timeline, false);
        let mut app = Self {
            docs: Vec::new(),
            active: 0,
            shell,
            runtime: PhotoRuntimeState::new(),
            queue: PhotoCommandQueue::new(),
            language: cygnus_i18n::Language::Fr,
            render_state: None,
            repaint: None,
        };
        // No initial document: the user creates (or opens).
        app.shell.new_doc_dialog.open();
        // Dock arrangement around the canvases (disk wiring
        // via `preferences`: next phase).
        app.shell.tree = load_tree_or_default(None, &app.doc_ids());
        app
    }

    /// Lets document workers request a repaint when they answer:
    /// without it, a finished render only shows on the next input
    /// event (the idle egui loop never polls on its own).
    #[must_use]
    pub fn with_repaint_context(mut self, ctx: egui::Context) -> Self {
        self.repaint = Some(ctx);
        self
    }

    /// Lets workers render on eframe's GPU device (zero-copy canvas).
    #[must_use]
    pub fn with_render_state(
        mut self,
        render_state: Option<eframe::egui_wgpu::RenderState>,
    ) -> Self {
        self.render_state = render_state;
        self
    }

    /// Active document, if any.
    pub fn active_doc_opt(&self) -> Option<&OpenDocument> {
        self.docs
            .get(self.active.min(self.docs.len().saturating_sub(1)))
    }

    /// Mutable active document, if any.
    pub fn active_doc_mut_opt(&mut self) -> Option<&mut OpenDocument> {
        let index = self.active.min(self.docs.len().saturating_sub(1));
        self.docs.get_mut(index)
    }

    /// Opens a tab with the validated dimensions (New doc window),
    /// with a first transparent layer at those dimensions.
    pub fn open_sized_tab(&mut self, width: u32, height: u32) {
        self.runtime.untitled_counter += 1;
        let title = crate::ui::naming::untitled_title(self.language, self.runtime.untitled_counter);
        let first = crate::ui::naming::first_layer_name(self.language);
        self.spawn_document(title, Document::with_blank_layer(width, height, first));
    }

    /// Spawns a document's worker, adds its canvas tab
    /// and activates it.
    pub fn spawn_document(&mut self, title: String, document: Document) {
        let (tx, worker_rx) = channel();
        let (worker_tx, rx) = channel();
        // Handle deliberately detached: the worker lives while
        // the channel is open, never blocking the UI.
        let wake = self
            .repaint
            .clone()
            .map(|ctx| Box::new(move || ctx.request_repaint()) as crate::ui::engine_bridge::Waker);
        std::mem::forget(spawn_photo_engine_worker(
            document, worker_rx, worker_tx, wake,
        ));
        let ui = PhotoUiState {
            needs_repaint: true,
            status: crate::ui::status::PhotoStatus::Ready,
            ..Default::default()
        };
        let id = uuid::Uuid::new_v4();
        self.docs.push(OpenDocument {
            id,
            title,
            project_path: None,
            ui,
            tx,
            rx,
            names_language: None,
        });
        self.active = self.docs.len() - 1;
        push_canvas_tab(&mut self.shell.tree, PhotoDockTab::Canvas(id));
        if let Some(render_state) = &self.render_state {
            let _ = self.docs[self.active]
                .tx
                .send(PhotoEngineCommand::AttachGpu {
                    device: render_state.device.clone(),
                    queue: render_state.queue.clone(),
                });
        }
        let _ = self.docs[self.active].tx.send(PhotoEngineCommand::Refresh);
    }

    /// Open document ids (layout canvases).
    fn doc_ids(&self) -> Vec<uuid::Uuid> {
        self.docs.iter().map(|doc| doc.id).collect()
    }

    /// Closes the active tab (zero documents allowed: the
    /// welcome screen takes over). The worker stops when
    /// the channel drops.
    pub fn close_active_tab(&mut self) {
        if self.docs.is_empty() {
            return;
        }
        let index = self.active.min(self.docs.len() - 1);
        self.close_document(self.docs[index].id);
    }

    /// Closes document `id` and removes its canvas tab (no-op
    /// if unknown; zero documents allowed).
    pub fn close_document(&mut self, id: uuid::Uuid) {
        remove_document(&mut self.docs, &mut self.active, id);
        crate::layout::dock::remove_canvas_tab(&mut self.shell.tree, id);
    }

    /// Sends a command to the active document's worker (no-op
    /// without a document: the UI never panics).
    fn send_active(&self, command: PhotoEngineCommand) {
        if let Some(doc) = self.active_doc_opt() {
            let _ = doc.tx.send(command);
        }
    }

    /// Optimistic update of a layer in the local snapshot: the
    /// panel reflects the change from the current frame, the
    /// worker response (expensive recomposite) confirms next.
    /// Recursive: nested layers update too.
    fn update_layer_opt(
        &mut self,
        id: uuid::Uuid,
        update: impl Fn(&mut crate::ui::PhotoLayerInfo),
    ) {
        if let Some(doc) = self.active_doc_mut_opt()
            && let Some(layer) = find_layer_in_mut(&mut doc.ui.layers, id)
        {
            update(layer);
            doc.ui.needs_repaint = true;
        }
    }

    /// Routes a UI action: local state or worker command.
    ///
    /// Only place (with [`apply_response`](crate::state::apply_response))
    /// where the app touches the engine: panels never access it.
    pub fn handle_action(&mut self, ctx: &egui::Context, action: PhotoAction) {
        match action {
            PhotoAction::OpenNewDocumentDialog => self.shell.new_doc_dialog.open(),
            PhotoAction::CreateDocument { width, height } => self.open_sized_tab(width, height),
            PhotoAction::OpenImageDialog | PhotoAction::AddImageLayer => {
                self.runtime.open_picker = Some(pick_image_to_open());
            }
            PhotoAction::OpenProjectDialog => {
                self.runtime.project_open_picker = Some(pick_project_to_open());
            }
            PhotoAction::SaveProjectAsDialog => {
                self.runtime.project_save_picker = Some(pick_project_to_save());
            }
            PhotoAction::SaveProject => {
                if let Some(path) = self
                    .active_doc_opt()
                    .and_then(|doc| doc.project_path.clone())
                {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.ui.status = crate::ui::status::PhotoStatus::Saving(path.clone());
                    }
                    self.send_active(PhotoEngineCommand::SaveProject { path });
                } else {
                    // Never saved: fall back to "Save as".
                    self.runtime.project_save_picker = Some(pick_project_to_save());
                }
            }
            PhotoAction::OpenExportDialog => self.shell.export_dialog.open(),
            PhotoAction::PrepareDragProxy { layer } => {
                self.send_active(PhotoEngineCommand::PrepareDragProxy { layer });
            }
            PhotoAction::EstimateExportSize { ext, quality } => {
                self.send_active(PhotoEngineCommand::EstimateExportSize { ext, quality });
            }
            PhotoAction::ExportDocument { path, quality } => {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    let _ = std::fs::create_dir_all(parent);
                }
                self.send_active(PhotoEngineCommand::Export { path, quality });
            }
            PhotoAction::CloseTab => self.close_active_tab(),
            PhotoAction::Undo => self.send_active(PhotoEngineCommand::Undo),
            PhotoAction::Redo => self.send_active(PhotoEngineCommand::Redo),
            PhotoAction::HistoryBack(steps) => {
                self.send_active(PhotoEngineCommand::UndoSteps { steps });
            }
            PhotoAction::HistoryForward(steps) => {
                self.send_active(PhotoEngineCommand::RedoSteps { steps });
            }
            PhotoAction::AddEmptyLayer => self.send_active(PhotoEngineCommand::AddEmptyLayer),
            PhotoAction::AddVectorLayer { shape } => {
                self.send_active(PhotoEngineCommand::AddVectorLayer { shape });
            }
            PhotoAction::DuplicateSelectedLayer => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::DuplicateLayer(id));
                }
            }
            PhotoAction::FlipHorizontalSelected => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::FlipHorizontal(id));
                }
            }
            PhotoAction::FlipVerticalSelected => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::FlipVertical(id));
                }
            }
            PhotoAction::RotateClockwiseSelected => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::RotateClockwise(id));
                }
            }
            PhotoAction::RotateCounterclockwiseSelected => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::RotateCounterclockwise(id));
                }
            }
            PhotoAction::CropSelectedToDocument => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::CropToDocument(id));
                }
            }
            PhotoAction::DeleteSelectedLayer => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::DeleteLayer(id));
                }
            }
            PhotoAction::DeleteLayer(id) => {
                self.send_active(PhotoEngineCommand::DeleteLayer(id));
            }
            PhotoAction::DuplicateLayer(id) => {
                self.send_active(PhotoEngineCommand::DuplicateLayer(id));
            }
            PhotoAction::DuplicateFilter { layer, filter } => {
                self.send_active(PhotoEngineCommand::DuplicateFilter { layer, filter });
            }
            PhotoAction::AddMaskToSelected => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::AddMask { layer: id });
                }
            }
            PhotoAction::AddMaskToFilter { filter } => {
                self.send_active(PhotoEngineCommand::AddMask { layer: filter });
            }
            PhotoAction::AddFilter { layer, filter_type } => {
                self.send_active(PhotoEngineCommand::AddFilter { layer, filter_type });
            }
            PhotoAction::SetFilterParam {
                layer,
                filter,
                key,
                value,
            } => {
                // Optimistic: the slider follows the gesture without waiting for
                // the background thread (coalesced worker-side).
                if let Some(doc) = self.active_doc_mut_opt()
                    && let Some(info) =
                        crate::ui::features::layers::find_layer_in_mut(&mut doc.ui.layers, layer)
                    && let Some(sub) = info.filters.iter_mut().find(|sub| sub.id == filter)
                {
                    sub.params.insert(key.clone(), value);
                    doc.ui.needs_repaint = true;
                }
                self.send_active(PhotoEngineCommand::SetFilterParam {
                    layer,
                    filter,
                    key,
                    value,
                });
            }
            PhotoAction::SetMaskParams {
                owner,
                mask,
                density,
                feather,
                inverted,
            } => {
                // Optimistic like `SetFilterParam`: sliders follow the
                // gesture before the worker answers.
                if let Some(doc) = self.active_doc_mut_opt()
                    && let Some(sub) = mask_info_mut(&mut doc.ui.layers, owner, mask)
                {
                    use photo_engine::snapshot::{
                        MASK_DENSITY_KEY, MASK_FEATHER_KEY, MASK_INVERTED_KEY,
                    };
                    sub.params.insert(MASK_DENSITY_KEY.to_owned(), density);
                    sub.params.insert(MASK_FEATHER_KEY.to_owned(), feather);
                    sub.params
                        .insert(MASK_INVERTED_KEY.to_owned(), f32::from(u8::from(inverted)));
                    doc.ui.needs_repaint = true;
                }
                self.send_active(PhotoEngineCommand::SetMaskParams {
                    owner,
                    mask,
                    density,
                    feather,
                    inverted,
                });
            }
            PhotoAction::SetFilterText {
                layer,
                filter,
                key,
                value,
            } => {
                // Optimistic like `SetFilterParam`: the curve follows the
                // drag without waiting for the worker.
                if let Some(doc) = self.active_doc_mut_opt()
                    && let Some(info) =
                        crate::ui::features::layers::find_layer_in_mut(&mut doc.ui.layers, layer)
                    && let Some(sub) = info.filters.iter_mut().find(|sub| sub.id == filter)
                {
                    sub.text_params.insert(key.clone(), value.clone());
                    doc.ui.needs_repaint = true;
                }
                self.send_active(PhotoEngineCommand::SetFilterText {
                    layer,
                    filter,
                    key,
                    value,
                });
            }
            PhotoAction::ApplyFilterPreset {
                layer,
                filter,
                preset,
            } => {
                self.send_active(PhotoEngineCommand::ApplyFilterPreset {
                    layer,
                    filter,
                    preset,
                });
            }
            PhotoAction::SelectLayer(id) => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.selected = Some(id);
                    doc.ui.focused_attachment = None;
                }
            }
            PhotoAction::FocusAttachment { owner, id } => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    // The selection is always a LAYER: a filter owner
                    // resolves to its layer (filter-owned masks keep
                    // their filter owner in the focus).
                    let selected = find_layer_in(&doc.ui.layers, owner)
                        .map(|_| owner)
                        .or_else(|| find_layer_of_attachment(&doc.ui.layers, owner));
                    doc.ui.selected = selected;
                    doc.ui.focused_attachment =
                        selected.map(|_| crate::ui::AttachmentRef { owner, id });
                }
            }
            PhotoAction::ToggleFilter { layer, filter } => {
                self.send_active(PhotoEngineCommand::ToggleFilter { layer, filter });
            }
            PhotoAction::ToggleMask { owner, mask } => {
                self.send_active(PhotoEngineCommand::ToggleMask { owner, mask });
            }
            PhotoAction::RenameLayer { layer, name } => {
                self.send_active(PhotoEngineCommand::RenameLayer { layer, name });
            }
            PhotoAction::ReorderNodes {
                dragged,
                target,
                before,
            } => {
                self.send_active(PhotoEngineCommand::ReorderNodes {
                    dragged,
                    target,
                    before,
                });
            }
            PhotoAction::MoveIntoParent { layer, parent } => {
                self.send_active(PhotoEngineCommand::MoveIntoParent { layer, parent });
            }
            PhotoAction::ToggleGroupCollapsed(id) => {
                self.send_active(PhotoEngineCommand::ToggleGroupCollapsed(id));
            }
            PhotoAction::ToggleLayerVisibility(id) => {
                // Optimistic: the eye toggles from this frame, the worker
                // confirms (recomposites) right after.
                self.update_layer_opt(id, |layer| layer.visible = !layer.visible);
                self.send_active(PhotoEngineCommand::ToggleLayerVisibility(id));
            }
            PhotoAction::IsolateLayer(id) => {
                self.send_active(PhotoEngineCommand::IsolateLayer(id));
            }
            PhotoAction::SetOpacity { layer, opacity } => {
                // Optimistic: the slider follows the gesture without waiting for
                // the background thread (coalesced worker-side).
                self.update_layer_opt(layer, |info| info.opacity = opacity.clamp(0.0, 100.0));
                self.send_active(PhotoEngineCommand::SetOpacity { layer, opacity });
            }
            PhotoAction::SetBlendMode { layer, mode } => {
                self.update_layer_opt(layer, |info| info.blend_mode = mode);
                self.send_active(PhotoEngineCommand::SetBlendMode { layer, mode });
            }
            PhotoAction::MoveFilterTo {
                filter,
                from,
                to,
                index,
            } => {
                self.send_active(PhotoEngineCommand::MoveFilterTo {
                    filter,
                    from,
                    to,
                    index,
                });
            }
            PhotoAction::MoveMaskTo {
                mask,
                from,
                to,
                index,
            } => {
                self.send_active(PhotoEngineCommand::MoveMaskTo {
                    mask,
                    from,
                    to,
                    index,
                });
            }
            PhotoAction::RemoveFilter { layer, filter } => {
                self.send_active(PhotoEngineCommand::RemoveFilter { layer, filter });
            }
            PhotoAction::RemoveMask { owner, mask } => {
                self.send_active(PhotoEngineCommand::RemoveMask { owner, mask });
            }
            PhotoAction::SetTool(tool) => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.tool = tool;
                }
            }
            PhotoAction::SetSelection(selection) => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.set_selection(selection.clone());
                }
                self.send_active(PhotoEngineCommand::SetSelection(selection));
            }
            PhotoAction::FillAt { x, y } => {
                if let Some(doc) = self.active_doc_opt()
                    && let Some((target, mask)) = paint_target(doc)
                {
                    let brush = doc.ui.brush;
                    // Masks take the color's gray (reveal / hide).
                    let color = if mask { gray(brush.color) } else { brush.color };
                    self.send_active(PhotoEngineCommand::FillAt {
                        target,
                        x,
                        y,
                        color,
                        tolerance: doc.ui.bucket.tolerance.clamp(0.0, 255.0).round() as u8,
                        contiguous: doc.ui.bucket.contiguous,
                        opacity: brush.opacity,
                    });
                }
            }
            PhotoAction::FillGradient { start, end } => {
                if let Some(doc) = self.active_doc_opt()
                    && let Some((target, mask)) = paint_target(doc)
                {
                    let brush = doc.ui.brush;
                    let (from, to) = if mask {
                        (gray(brush.color), gray(brush.secondary))
                    } else {
                        (brush.color, brush.secondary)
                    };
                    self.send_active(PhotoEngineCommand::FillGradient {
                        target,
                        start,
                        end,
                        from,
                        to,
                        opacity: brush.opacity,
                    });
                }
            }
            PhotoAction::MagicWand { x, y, op } => {
                // Worker-side flood on the exact composite; the answer
                // (`SelectionChanged`) updates the document selection.
                if let Some(doc) = self.active_doc_opt() {
                    let wand = doc.ui.wand;
                    self.send_active(PhotoEngineCommand::MagicWand {
                        x,
                        y,
                        tolerance: wand.tolerance.clamp(0.0, 255.0).round() as u8,
                        contiguous: wand.contiguous,
                        op,
                    });
                }
            }
            PhotoAction::SelectAll => {
                let size = self
                    .active_doc_opt()
                    .and_then(|doc| doc.ui.preview_geom)
                    .map(|geom| {
                        (
                            geom.doc_size.x.max(1.0) as u32,
                            geom.doc_size.y.max(1.0) as u32,
                        )
                    });
                if let Some((width, height)) = size {
                    self.handle_action(
                        ctx,
                        PhotoAction::SetSelection(Some(photo_engine::Selection::all(
                            width, height,
                        ))),
                    );
                }
            }
            PhotoAction::Deselect => {
                self.handle_action(ctx, PhotoAction::SetSelection(None));
            }
            PhotoAction::InvertSelection => {
                let inverted = self
                    .active_doc_opt()
                    .and_then(|doc| doc.ui.selection.clone())
                    .map(photo_engine::Selection::inverse);
                if inverted.is_some() {
                    self.handle_action(ctx, PhotoAction::SetSelection(inverted));
                }
            }
            PhotoAction::MaskFromSelection => {
                let target = self
                    .active_doc_opt()
                    .filter(|doc| doc.ui.selection.is_some())
                    .and_then(|doc| doc.ui.selected);
                if let Some(layer) = target {
                    self.send_active(PhotoEngineCommand::AddMaskFromSelection { layer });
                }
            }
            PhotoAction::ToggleGrid => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.show_grid = !doc.ui.show_grid;
                }
            }
            PhotoAction::TogglePreviewClip => {
                let clip = !self.active_doc_opt().is_some_and(|doc| doc.ui.preview_clip);
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.preview_clip = clip;
                }
                self.send_active(PhotoEngineCommand::SetPreviewClip { clip });
            }
            PhotoAction::ZoomIn => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.viewport.zoom_by(1.25, None);
                }
            }
            PhotoAction::ZoomOut => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.viewport.zoom_by(0.8, None);
                }
            }
            PhotoAction::ZoomReset => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.viewport.reset();
                }
            }
            PhotoAction::CommitStroke(paint) => {
                // Tip dynamics first: the worker applies them to the
                // stroke that follows (session state, no history).
                if let Some(doc) = self.active_doc_opt() {
                    let dynamics = doc.ui.brush.dynamics();
                    self.send_active(PhotoEngineCommand::SetBrushDynamics(dynamics));
                }
                // Clone stamp: copy from the source ("aligned": the first
                // stroke after picking a source fixes the offset).
                if self
                    .active_doc_opt()
                    .is_some_and(|doc| doc.ui.tool == crate::ui::PhotoCanvasTool::Clone)
                {
                    let offset = self.active_doc_mut_opt().and_then(|doc| {
                        if doc.ui.clone_offset.is_none()
                            && let (Some(source), Some(first)) =
                                (doc.ui.clone_source, paint.points.first())
                        {
                            doc.ui.clone_offset = Some((source.0 - first.0, source.1 - first.1));
                        }
                        doc.ui.clone_offset
                    });
                    if let Some(offset) = offset {
                        self.send_active(PhotoEngineCommand::CloneStroke {
                            layer: paint.layer,
                            points: paint.points,
                            radius: paint.radius,
                            opacity: paint.opacity,
                            offset,
                        });
                    }
                    return;
                }
                // Focused mask of the selected layer: the stroke
                // goes into the mask (white/black), otherwise into
                // the layer source (historical behavior). The focused
                // mask may live in a filter-owned pile: its layer must
                // be the selection.
                let mask_target = self.active_doc_opt().and_then(focused_mask);
                if let Some((owner, mask)) = mask_target {
                    // Mask painting ignores hue: the eraser always hides
                    // (black) and a dark brush color hides like the
                    // eraser, while a bright one reveals.
                    let hide = paint.eraser || brush_luminance(paint.color) < 128.0;
                    self.send_active(PhotoEngineCommand::PaintMask {
                        owner,
                        mask,
                        points: paint.points,
                        eraser: hide,
                        radius: paint.radius,
                        opacity: paint.opacity,
                    });
                } else {
                    self.send_active(PhotoEngineCommand::PaintStroke {
                        layer: paint.layer,
                        points: paint.points,
                        eraser: paint.eraser,
                        radius: paint.radius,
                        color: paint.color,
                        opacity: paint.opacity,
                    });
                }
            }
            PhotoAction::MoveLayer { layer, dx, dy } => {
                self.send_active(PhotoEngineCommand::MoveLayer { layer, dx, dy });
            }
            PhotoAction::SetTransform {
                layer,
                transform,
                commit,
            } => {
                self.send_active(PhotoEngineCommand::SetTransform {
                    layer,
                    transform,
                    commit,
                });
            }
            PhotoAction::SetVectorPaths {
                layer,
                paths,
                commit,
            } => {
                self.send_active(PhotoEngineCommand::SetVectorPaths {
                    layer,
                    paths,
                    commit,
                });
            }
            PhotoAction::ShowHelp => {
                self.shell.help_open = true;
            }
            PhotoAction::ShowDockTab(tab) => {
                ensure_tab(&mut self.shell.tree, tab);
            }
            PhotoAction::HideDockTab(tab) => {
                if let Some(tile) = self.shell.tree.tiles.find_pane(&tab) {
                    self.shell.tree.tiles.set_visible(tile, false);
                }
            }
            PhotoAction::ResetDockLayout => {
                let canvases = self
                    .doc_ids()
                    .into_iter()
                    .map(PhotoDockTab::Canvas)
                    .collect();
                self.shell.tree = default_tree(canvases);
            }
            PhotoAction::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Non-blocking poll: worker responses + file pickers.
    pub fn poll(&mut self, ctx: &egui::Context) {
        // Default names follow the interface language: (re)sent to every
        // worker whose names are missing or stale (new document,
        // language switch). Applies to items created afterwards.
        for doc in &mut self.docs {
            if doc.names_language != Some(self.language) {
                let names = crate::ui::naming::default_names(self.language);
                if doc
                    .tx
                    .send(PhotoEngineCommand::SetDefaultNames(names))
                    .is_ok()
                {
                    doc.names_language = Some(self.language);
                }
            }
        }
        for doc in &mut self.docs {
            while let Ok(response) = doc.rx.try_recv() {
                apply_response(ctx, &mut doc.ui, response);
            }
        }
        // Image opening → active document (created if needed).
        if let Some(rx) = self.runtime.open_picker.take() {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    if self.docs.is_empty() {
                        self.runtime.untitled_counter += 1;
                        let title = crate::ui::naming::untitled_title(
                            self.language,
                            self.runtime.untitled_counter,
                        );
                        // Dimensions adjusted by the worker to the image.
                        self.spawn_document(title, Document::new(1, 1));
                    }
                    self.send_active(PhotoEngineCommand::OpenImage { path });
                }
                Ok(None) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.runtime.open_picker = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.ui.status = crate::ui::status::PhotoStatus::PickerUnavailable;
                    }
                }
            }
        }
        // `.cygp` project opening → ALWAYS a new tab (never
        // merged into the active document: a project replaces everything).
        if let Some(rx) = self.runtime.project_open_picker.take() {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    let title = project_title(&path, self.language);
                    self.spawn_document(title, Document::new(1, 1));
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.project_path = Some(path.clone());
                        doc.ui.status = crate::ui::status::PhotoStatus::Loading(path.clone());
                        let _ = doc.tx.send(PhotoEngineCommand::LoadProject { path });
                    }
                }
                Ok(None) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.runtime.project_open_picker = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.ui.status = crate::ui::status::PhotoStatus::PickerUnavailable;
                    }
                }
            }
        }
        // "Save as" → active document (path + title set at
        // dispatch, pure UI state; the worker only serializes).
        if let Some(rx) = self.runtime.project_save_picker.take() {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    let language = self.language;
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.title = project_title(&path, language);
                        doc.project_path = Some(path.clone());
                        doc.ui.status = crate::ui::status::PhotoStatus::Saving(path.clone());
                        let _ = doc.tx.send(PhotoEngineCommand::SaveProject { path });
                    }
                }
                Ok(None) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.runtime.project_save_picker = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.ui.status = crate::ui::status::PhotoStatus::PickerUnavailable;
                    }
                }
            }
        }
        // GPU frames: (re)register their texture with egui's renderer.
        if let Some(render_state) = &self.render_state {
            for doc in &mut self.docs {
                if let Some(frame) = &doc.ui.gpu_frame {
                    crate::ui::gpu_display::sync(render_state, &mut doc.ui.gpu_display, frame);
                }
            }
        }
    }

    /// One full frame: poll, workspace, action drain.
    pub fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.poll(ctx);
        let photo_ctx = PhotoUiContext::for_frame_in(ctx, self.language);
        PhotoWorkspace::show(ui, self, &photo_ctx);
        // Keyboard (O002): same flow as menus and panels — single queue.
        for action in crate::shortcuts::poll_shortcut_actions(ctx, &self.runtime.keys) {
            self.queue.push(action);
        }
        for action in self.queue.drain() {
            self.handle_action(ctx, action);
        }
        if self.docs.iter().any(|doc| doc.ui.needs_repaint) {
            for doc in &mut self.docs {
                doc.ui.needs_repaint = false;
            }
            ctx.request_repaint();
        }
    }
}

impl Default for PhotoApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Tab title from a project path: name without extension
/// ("rendu.cygp" → "rendu"), "Untitled" (translated) when unreadable.
fn project_title(path: &Path, language: cygnus_i18n::Language) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            cygnus_i18n::PhotoCatalog::new(language)
                .get(cygnus_i18n::PhotoTextKey::UntitledDocument)
                .to_owned()
        })
}

/// Focused mask of the selected layer (`(owner, mask)`), if any: paint
/// tools then write into the mask instead of the layer pixels. The
/// focused mask may live in a filter-owned pile: its layer must be the
/// selection.
fn focused_mask(doc: &OpenDocument) -> Option<(uuid::Uuid, uuid::Uuid)> {
    let focus = doc.ui.focused_attachment?;
    let focus_layer = find_layer_in(&doc.ui.layers, focus.owner)
        .map(|_| focus.owner)
        .or_else(|| find_layer_of_attachment(&doc.ui.layers, focus.owner))?;
    if Some(focus_layer) != doc.ui.selected {
        return None;
    }
    find_attachment_in(&doc.ui.layers, focus.owner, focus.id)
        .filter(|(_, is_filter)| !is_filter)
        .map(|_| (focus.owner, focus.id))
}

/// Buffer the fill tools write into: the focused mask, else the
/// selected layer. The flag is true for masks (gray colors).
fn paint_target(doc: &OpenDocument) -> Option<(PaintTarget, bool)> {
    if let Some((owner, mask)) = focused_mask(doc) {
        return Some((PaintTarget::Mask { owner, mask }, true));
    }
    doc.ui
        .selected
        .map(|layer| (PaintTarget::Layer(layer), false))
}

/// Gray of `color` (mask fills: luminance decides reveal / hide).
fn gray(color: [u8; 3]) -> [u8; 3] {
    let v = brush_luminance(color).round().clamp(0.0, 255.0) as u8;
    [v, v, v]
}

/// Mask `mask` of `owner` in the layer snapshot (layer, filter or nested
/// child pile), for optimistic updates.
fn mask_info_mut(
    layers: &mut [crate::ui::PhotoLayerInfo],
    owner: uuid::Uuid,
    mask: uuid::Uuid,
) -> Option<&mut photo_engine::snapshot::PhotoSubLayerInfo> {
    for layer in layers {
        if layer.id == owner {
            return layer.masks.iter_mut().find(|m| m.id == mask);
        }
        if let Some(filter) = layer.filters.iter_mut().find(|f| f.id == owner) {
            return filter.masks.iter_mut().find(|m| m.id == mask);
        }
        if let Some(found) = mask_info_mut(&mut layer.children, owner, mask) {
            return Some(found);
        }
    }
    None
}

/// Removes document `id` from `docs` and re-seats `active` (zero
/// documents allowed: welcome takes over). The document's
/// worker stops when its channels drop. Shared by
/// `PhotoApp::close_document` (menu) and the canvas tab crosses
/// (`layout::dock`, which also removes the tile).
pub(crate) fn remove_document(docs: &mut Vec<OpenDocument>, active: &mut usize, id: uuid::Uuid) {
    if let Some(index) = docs.iter().position(|doc| doc.id == id) {
        docs.remove(index);
        // A document before the active one would shift the selection.
        if index < *active {
            *active -= 1;
        }
    }
    *active = (*active).min(docs.len().saturating_sub(1));
}

impl eframe::App for PhotoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.draw(&ctx, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PhotoUiState;
    use crate::ui::PhotoEngineResponse;

    #[test]
    fn app_boots_without_doc_and_open_dialog() {
        let app = PhotoApp::new();
        assert!(app.docs.is_empty(), "aucun document au démarrage");
        assert!(app.shell.new_doc_dialog.open);
    }

    #[test]
    fn tabs_open_close() {
        use uuid::Uuid;

        let mut app = PhotoApp::new();
        app.open_sized_tab(800, 600);
        app.open_sized_tab(800, 600);
        assert_eq!(app.docs.len(), 2);
        // One canvas tab per document.
        for doc in &app.docs {
            assert!(app_shell::dock::has_tab(
                &app.shell.tree,
                PhotoDockTab::Canvas(doc.id)
            ));
        }
        // Closing the active one also removes its canvas.
        let removed = app.active_doc_opt().expect("doc actif").id;
        app.close_active_tab();
        assert_eq!(app.docs.len(), 1);
        assert!(!app_shell::dock::has_tab(
            &app.shell.tree,
            PhotoDockTab::Canvas(removed)
        ));
        // Closing the last one: zero documents allowed (welcome).
        app.close_active_tab();
        assert!(app.docs.is_empty());
        // Unknown id: no-op.
        app.close_document(Uuid::from_u128(999));
        assert!(app.docs.is_empty());
    }

    #[test]
    fn create_document_action_opens_sized_tab_with_layer() {
        let mut app = PhotoApp::new();
        assert!(app.docs.is_empty());
        let ctx = egui::Context::default();
        app.handle_action(
            &ctx,
            PhotoAction::CreateDocument {
                width: 800,
                height: 600,
            },
        );
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active, 0);
        assert!(app_shell::dock::has_tab(
            &app.shell.tree,
            PhotoDockTab::Canvas(app.active_doc_opt().expect("doc actif").id)
        ));
        // Initial worker snapshot: the blank layer at the right dimensions.
        let response = app
            .active_doc_opt()
            .expect("doc actif")
            .rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("snapshot initial");
        apply_response(
            &ctx,
            &mut app.active_doc_mut_opt().expect("doc actif").ui,
            response,
        );
        let layers = &app.active_doc_opt().expect("doc actif").ui.layers;
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].name, "Calque 1");
    }

    #[test]
    fn save_project_writes_cygp_and_confirms_status() {
        use std::time::{Duration, Instant};

        let dir = std::env::temp_dir().join(format!("cyg-app-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let path = dir.join("travail.cygp");
        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(16, 16);
        app.active_doc_mut_opt().expect("doc").project_path = Some(path.clone());
        app.handle_action(&ctx, PhotoAction::SaveProject);
        // Real egui loop: non-blocking poll until the worker ack
        // ("Enregistre :", not the optimistic "Enregistrement" status).
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if app
                .active_doc_opt()
                .expect("doc")
                .ui
                .status
                .eq(&crate::ui::status::PhotoStatus::Saved(path.clone()))
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "accusé de sauvegarde attendu"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(path.is_file(), "fichier projet écrit");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_project_picker_spawns_tab_and_loads_layers() {
        use std::sync::mpsc::channel;
        use std::time::{Duration, Instant};

        let dir = std::env::temp_dir().join(format!("cyg-app-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let path = dir.join("maquette.cygp");
        // Witness project: a 16×16 document saved via its worker.
        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(16, 16);
        app.active_doc_mut_opt().expect("doc").project_path = Some(path.clone());
        app.handle_action(&ctx, PhotoAction::SaveProject);
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if path.is_file() {
                // Drain the ack before opening (deterministic order).
                std::thread::sleep(Duration::from_millis(50));
                app.poll(&ctx);
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(10));
        }
        // Simulate the file picker (ready channel, no rfd): new tab.
        let (tx, rx) = channel();
        tx.send(Some(path.clone())).expect("envoi picker");
        app.runtime.project_open_picker = Some(rx);
        app.poll(&ctx);
        assert_eq!(app.docs.len(), 2, "projet ouvert dans un nouvel onglet");
        assert_eq!(
            app.active_doc_opt().expect("doc").title,
            "maquette",
            "titre = nom du fichier"
        );
        assert_eq!(
            app.active_doc_opt().expect("doc").project_path.as_deref(),
            Some(path.as_path())
        );
        // Worker loading replaces the 1×1 document with the saved tree.
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            let layers = &app.active_doc_opt().expect("doc").ui.layers;
            if layers.iter().any(|layer| layer.name == "Calque 1") {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "calques du projet attendus"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn history_back_jumps_to_initial_state() {
        use std::time::{Duration, Instant};

        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(16, 16);
        app.handle_action(&ctx, PhotoAction::AddEmptyLayer);
        app.handle_action(&ctx, PhotoAction::AddEmptyLayer);
        // Wait for the 2 steps (real egui loop, non-blocking poll).
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if app.active_doc_opt().expect("doc").ui.history_undo.len() == 2 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "2 pas d'historique attendus"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // Backward jump past the stack: back to the initial layer.
        app.handle_action(&ctx, PhotoAction::HistoryBack(5));
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            let ui = &app.active_doc_opt().expect("doc").ui;
            if ui.history_undo.is_empty() && ui.history_redo.len() == 2 {
                assert_eq!(ui.layers.len(), 1, "calque initial restauré");
                assert_eq!(ui.layers[0].name, "Calque 1");
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "saut attendu");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Then one step forward: the added layer comes back.
        app.handle_action(&ctx, PhotoAction::HistoryForward(1));
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            let ui = &app.active_doc_opt().expect("doc").ui;
            if ui.history_undo.len() == 1 {
                assert_eq!(ui.layers.len(), 2);
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "redo attendu");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn flip_selected_routes_to_worker_with_label() {
        use std::time::{Duration, Instant};

        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(16, 16);
        // Wait for the snapshot to select the initial layer.
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if !app.active_doc_opt().expect("doc").ui.layers.is_empty() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(10));
        }
        let id = app.active_doc_opt().expect("doc").ui.layers[0].id;
        app.handle_action(&ctx, PhotoAction::SelectLayer(id));
        app.handle_action(&ctx, PhotoAction::FlipHorizontalSelected);
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            let history = &app.active_doc_opt().expect("doc").ui.history_undo;
            if history.iter().any(|op| op == "flip_horizontal") {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "flip attendu");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn commit_stroke_on_focused_mask_paints_mask() {
        use std::time::{Duration, Instant};

        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(16, 16);
        // Wait for the layer, select it, add a white mask.
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if !app.active_doc_opt().expect("doc").ui.layers.is_empty() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(10));
        }
        let id = app.active_doc_opt().expect("doc").ui.layers[0].id;
        app.handle_action(&ctx, PhotoAction::SelectLayer(id));
        app.handle_action(&ctx, PhotoAction::AddMaskToSelected);
        let start = Instant::now();
        let mid = loop {
            app.poll(&ctx);
            if let Some(mask) = app
                .active_doc_opt()
                .expect("doc")
                .ui
                .layers
                .iter()
                .find(|layer| layer.id == id)
                .and_then(|layer| layer.masks.first())
            {
                break mask.id;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "masque attendu");
            std::thread::sleep(Duration::from_millis(10));
        };
        // Mask focus + eraser stroke: must go as PaintMask.
        app.handle_action(&ctx, PhotoAction::FocusAttachment { owner: id, id: mid });
        app.handle_action(
            &ctx,
            PhotoAction::CommitStroke(crate::ui::PaintRequest {
                layer: id,
                points: vec![(4.0, 4.0), (8.0, 8.0)],
                eraser: true,
                radius: 3.0,
                color: [255, 0, 0],
                opacity: 1.0,
            }),
        );
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if app
                .active_doc_opt()
                .expect("doc")
                .ui
                .history_undo
                .iter()
                .any(|op| op == "paint_mask")
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "peinture masque attendue"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn commit_stroke_on_filter_mask_paints_filter_mask() {
        // Full filter-mask flow: filter → mask on the filter → focus
        // the mask → brush stroke lands in the FILTER's pile (white
        // reveals), selection stays on the layer.
        use std::time::{Duration, Instant};

        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(16, 16);
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if !app.active_doc_opt().expect("doc").ui.layers.is_empty() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(10));
        }
        let id = app.active_doc_opt().expect("doc").ui.layers[0].id;
        app.handle_action(&ctx, PhotoAction::SelectLayer(id));
        app.handle_action(
            &ctx,
            PhotoAction::AddFilter {
                layer: id,
                filter_type: String::from("brightness_contrast"),
            },
        );
        let start = Instant::now();
        let fid = loop {
            app.poll(&ctx);
            if let Some(filtre) = app
                .active_doc_opt()
                .expect("doc")
                .ui
                .layers
                .iter()
                .find(|layer| layer.id == id)
                .and_then(|layer| layer.filters.first())
            {
                break filtre.id;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "filtre attendu");
            std::thread::sleep(Duration::from_millis(10));
        };
        // Focus the filter, add a mask to ITS pile.
        app.handle_action(&ctx, PhotoAction::FocusAttachment { owner: id, id: fid });
        app.handle_action(&ctx, PhotoAction::AddMaskToFilter { filter: fid });
        let start = Instant::now();
        let mid = loop {
            app.poll(&ctx);
            let doc = app.active_doc_opt().expect("doc");
            // Selection stayed on the layer (never on the filter).
            assert_eq!(doc.ui.selected, Some(id));
            if let Some(mask) = doc
                .ui
                .layers
                .iter()
                .find(|layer| layer.id == id)
                .and_then(|layer| layer.filters.iter().find(|f| f.id == fid))
                .and_then(|filtre| filtre.masks.first())
            {
                break mask.id;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "masque attendu");
            std::thread::sleep(Duration::from_millis(10));
        };
        // Focus the filter-owned mask, paint: must go as PaintMask
        // with the FILTER as owner.
        app.handle_action(
            &ctx,
            PhotoAction::FocusAttachment {
                owner: fid,
                id: mid,
            },
        );
        // Selection resolved back to the layer.
        assert_eq!(app.active_doc_opt().expect("doc").ui.selected, Some(id));
        app.handle_action(
            &ctx,
            PhotoAction::CommitStroke(crate::ui::PaintRequest {
                layer: id,
                points: vec![(4.0, 4.0), (8.0, 8.0)],
                eraser: false,
                radius: 3.0,
                color: [255, 0, 0],
                opacity: 1.0,
            }),
        );
        let start = Instant::now();
        loop {
            app.poll(&ctx);
            if app
                .active_doc_opt()
                .expect("doc")
                .ui
                .history_undo
                .iter()
                .any(|op| op == "paint_mask")
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "peinture masque de filtre attendue"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn closing_before_active_keeps_selection() {
        let mut app = PhotoApp::new();
        app.open_sized_tab(800, 600);
        app.open_sized_tab(800, 600);
        let middle = app.docs[1].id;
        app.active = 1;
        app.close_document(app.docs[0].id);
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active, 0);
        assert_eq!(app.active_doc_opt().expect("doc actif").id, middle);
    }

    #[test]
    fn select_tool_drag_through_the_workspace_creates_a_selection() {
        // End to end: real frames of `PhotoApp::draw` (dock, central
        // view, canvas) with a mouse drag — the marquee must survive
        // across frames and become the document selection on release.
        let mut app = PhotoApp::new();
        app.shell.new_doc_dialog.open = false;
        app.open_sized_tab(400, 300);
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let frame = |app: &mut PhotoApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let ctx_clone = ctx.clone();
            ctx.run_ui(input, |ui| app.draw(&ctx_clone, ui))
                .drop_without_applying_deltas();
        };
        // Wait for the first composite (worker thread) to place the canvas.
        for _ in 0..500 {
            frame(&mut app, Vec::new());
            let doc = app.active_doc_opt().expect("doc actif");
            if doc.ui.preview_geom.is_some() && doc.ui.last_dest.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let dest = app
            .active_doc_opt()
            .and_then(|doc| doc.ui.last_dest)
            .expect("canvas placé");
        app.handle_action(
            &ctx,
            PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Select),
        );
        let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        let a = dest.min + dest.size() * 0.25;
        let b = dest.min + dest.size() * 0.75;
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(a), button(a, true)],
        );
        for step in 1..=6 {
            let p = a + (b - a) * (step as f32 / 6.0);
            frame(&mut app, vec![egui::Event::PointerMoved(p)]);
        }
        assert!(
            app.active_doc_opt()
                .expect("doc")
                .ui
                .selection_preview
                .is_some(),
            "marquee visible while dragging"
        );
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(b), button(b, false)],
        );
        let selection = app
            .active_doc_opt()
            .expect("doc")
            .ui
            .selection
            .clone()
            .expect("selection created on release");
        let (x0, y0, x1, y1) = selection.bounds();
        assert!(
            x1 - x0 > 100.0 && y1 - y0 > 75.0,
            "half the document selected: {:?}",
            selection.bounds()
        );
    }

    /// App with one 400x300 document drawn until its canvas is placed;
    /// returns the app, the context and the document screen rect.
    fn app_with_placed_canvas() -> (PhotoApp, egui::Context, egui::Rect) {
        let mut app = PhotoApp::new();
        app.shell.new_doc_dialog.open = false;
        app.open_sized_tab(400, 300);
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        for _ in 0..500 {
            draw_frame(&mut app, &ctx, Vec::new());
            let doc = app.active_doc_opt().expect("doc actif");
            if doc.ui.preview_geom.is_some() && doc.ui.last_dest.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let dest = app
            .active_doc_opt()
            .and_then(|doc| doc.ui.last_dest)
            .expect("canvas placé");
        (app, ctx, dest)
    }

    /// One frame of the full workspace with `events`.
    fn draw_frame(app: &mut PhotoApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        draw_frame_with(app, ctx, events, egui::Modifiers::default());
    }

    /// [`draw_frame`] with held `modifiers`.
    fn draw_frame_with(
        app: &mut PhotoApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        // egui 0.36 tracks held modifiers through this event.
        let mut events = events;
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let ctx_clone = ctx.clone();
        ctx.run_ui(input, |ui| app.draw(&ctx_clone, ui))
            .drop_without_applying_deltas();
    }

    fn primary(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    #[test]
    fn lasso_drag_through_the_workspace_selects_the_polygon() {
        let (mut app, ctx, dest) = app_with_placed_canvas();
        app.handle_action(
            &ctx,
            PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Lasso),
        );
        let at = |fx: f32, fy: f32| dest.min + egui::vec2(dest.width() * fx, dest.height() * fy);
        let path = [at(0.2, 0.2), at(0.8, 0.2), at(0.8, 0.8), at(0.2, 0.8)];
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(path[0]), primary(path[0], true)],
        );
        for p in &path[1..] {
            for step in 1..=3 {
                let from = draw_target(&path, *p);
                let q = from + (*p - from) * (step as f32 / 3.0);
                draw_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
            }
        }
        assert!(
            app.active_doc_opt().expect("doc").ui.lasso_preview.len() >= 3,
            "lasso path visible while dragging"
        );
        let last = path[3];
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(last), primary(last, false)],
        );
        let doc = app.active_doc_opt().expect("doc");
        let selection = doc.ui.selection.clone().expect("lasso selection");
        assert_eq!(selection.coverage(200.5, 150.5), 1.0, "center selected");
        assert_eq!(selection.coverage(10.5, 10.5), 0.0, "corner left out");
        assert!(!doc.ui.selection_outline.is_empty(), "outline cached");
    }

    /// Previous point of `path` before `p` (lasso test helper).
    fn draw_target(path: &[egui::Pos2], p: egui::Pos2) -> egui::Pos2 {
        let index = path.iter().position(|q| *q == p).unwrap_or(1);
        path[index.saturating_sub(1)]
    }

    #[test]
    fn wand_click_routes_to_the_worker_and_adopts_its_selection() {
        let (mut app, ctx, dest) = app_with_placed_canvas();
        app.handle_action(&ctx, PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Wand));
        let p = dest.center();
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), primary(p, true)],
        );
        draw_frame(&mut app, &ctx, vec![primary(p, false)]);
        // The flood runs on the worker thread: poll until it answers.
        for _ in 0..500 {
            draw_frame(&mut app, &ctx, Vec::new());
            if app.active_doc_opt().expect("doc").ui.selection.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let selection = app
            .active_doc_opt()
            .expect("doc")
            .ui
            .selection
            .clone()
            .expect("wand selection");
        // Blank document: one uniform color, everything selected.
        assert_eq!(selection.bounds(), (0.0, 0.0, 400.0, 300.0));
    }

    /// Waits (worker thread) until the active document can undo.
    fn wait_for_undo(app: &mut PhotoApp, ctx: &egui::Context) -> bool {
        for _ in 0..500 {
            draw_frame(app, ctx, Vec::new());
            if app.active_doc_opt().is_some_and(|doc| doc.ui.can_undo) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// Selects the first layer of the active document.
    fn select_first_layer(app: &mut PhotoApp, ctx: &egui::Context) {
        let layer = app
            .active_doc_opt()
            .and_then(|doc| doc.ui.layers.first().map(|layer| layer.id))
            .expect("un calque");
        app.handle_action(ctx, PhotoAction::SelectLayer(layer));
    }

    #[test]
    fn bucket_click_through_the_workspace_fills_the_layer() {
        let (mut app, ctx, dest) = app_with_placed_canvas();
        select_first_layer(&mut app, &ctx);
        app.handle_action(
            &ctx,
            PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Bucket),
        );
        let p = dest.center();
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), primary(p, true)],
        );
        draw_frame(&mut app, &ctx, vec![primary(p, false)]);
        assert!(wait_for_undo(&mut app, &ctx), "fill recorded as an edit");
    }

    #[test]
    fn gradient_drag_through_the_workspace_fills_the_layer() {
        let (mut app, ctx, dest) = app_with_placed_canvas();
        select_first_layer(&mut app, &ctx);
        app.handle_action(
            &ctx,
            PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Gradient),
        );
        let a = dest.min + dest.size() * 0.2;
        let b = dest.min + dest.size() * 0.8;
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(a), primary(a, true)],
        );
        for step in 1..=4 {
            let q = a + (b - a) * (step as f32 / 4.0);
            draw_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
        }
        assert!(
            app.active_doc_opt()
                .expect("doc")
                .ui
                .gradient_preview
                .is_some(),
            "line shown while dragging"
        );
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(b), primary(b, false)],
        );
        assert!(
            app.active_doc_opt()
                .expect("doc")
                .ui
                .gradient_preview
                .is_none()
        );
        assert!(
            wait_for_undo(&mut app, &ctx),
            "gradient recorded as an edit"
        );
    }

    #[test]
    fn shift_drag_adds_a_second_marquee_to_the_selection() {
        let (mut app, ctx, dest) = app_with_placed_canvas();
        app.handle_action(
            &ctx,
            PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Select),
        );
        let at = |fx: f32, fy: f32| dest.min + egui::vec2(dest.width() * fx, dest.height() * fy);
        let marquee = |app: &mut PhotoApp, a: egui::Pos2, b: egui::Pos2, shift: bool| {
            let modifiers = egui::Modifiers {
                shift,
                ..Default::default()
            };
            draw_frame_with(
                app,
                &ctx,
                vec![egui::Event::PointerMoved(a), primary(a, true)],
                modifiers,
            );
            for step in 1..=4 {
                let q = a + (b - a) * (step as f32 / 4.0);
                draw_frame_with(app, &ctx, vec![egui::Event::PointerMoved(q)], modifiers);
            }
            draw_frame_with(
                app,
                &ctx,
                vec![egui::Event::PointerMoved(b), primary(b, false)],
                modifiers,
            );
        };
        marquee(&mut app, at(0.05, 0.1), at(0.3, 0.9), false);
        marquee(&mut app, at(0.7, 0.1), at(0.95, 0.9), true);
        let selection = app
            .active_doc_opt()
            .and_then(|doc| doc.ui.selection.clone())
            .expect("selection");
        // Document 400 x 300: both strips selected, the middle not.
        assert_eq!(selection.coverage(60.5, 150.5), 1.0, "first marquee kept");
        assert_eq!(
            selection.coverage(330.5, 150.5),
            1.0,
            "second marquee added"
        );
        assert_eq!(selection.coverage(200.5, 150.5), 0.0, "gap between them");
    }

    #[test]
    fn clone_stamp_alt_click_then_drag_clones_in_the_worker() {
        let (mut app, ctx, dest) = app_with_placed_canvas();
        select_first_layer(&mut app, &ctx);
        app.handle_action(
            &ctx,
            PhotoAction::SetTool(crate::ui::PhotoCanvasTool::Clone),
        );
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        let source = dest.min + dest.size() * 0.25;
        draw_frame_with(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(source), primary(source, true)],
            alt,
        );
        draw_frame_with(&mut app, &ctx, vec![primary(source, false)], alt);
        assert!(
            app.active_doc_opt().expect("doc").ui.clone_source.is_some(),
            "Alt+click picks the source"
        );
        assert!(
            !app.active_doc_opt().expect("doc").ui.can_undo,
            "no paint yet"
        );
        let a = dest.min + dest.size() * 0.6;
        let b = dest.min + dest.size() * 0.8;
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(a), primary(a, true)],
        );
        for step in 1..=4 {
            let q = a + (b - a) * (step as f32 / 4.0);
            draw_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
        }
        draw_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(b), primary(b, false)],
        );
        assert!(
            app.active_doc_opt().expect("doc").ui.clone_offset.is_some(),
            "first stroke fixes the aligned offset"
        );
        assert!(wait_for_undo(&mut app, &ctx), "clone stroke recorded");
    }

    #[test]
    fn selection_actions_update_the_document_selection() {
        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        // Without a document: no-op, no panic.
        app.handle_action(&ctx, PhotoAction::SelectAll);
        app.open_sized_tab(800, 600);
        app.active_doc_mut_opt().expect("doc actif").ui.preview_geom =
            Some(crate::state::PreviewGeom {
                thumb_size: egui::vec2(800.0, 600.0),
                full_size: egui::vec2(800.0, 600.0),
                origin: egui::Vec2::ZERO,
                doc_size: egui::vec2(800.0, 600.0),
            });
        let selection = |app: &PhotoApp| {
            app.active_doc_opt()
                .expect("doc actif")
                .ui
                .selection
                .clone()
        };
        app.handle_action(&ctx, PhotoAction::SelectAll);
        assert_eq!(
            selection(&app),
            Some(photo_engine::Selection::all(800, 600))
        );
        app.handle_action(&ctx, PhotoAction::InvertSelection);
        assert_eq!(
            selection(&app),
            Some(photo_engine::Selection::all(800, 600).inverse())
        );
        app.handle_action(&ctx, PhotoAction::Deselect);
        assert_eq!(selection(&app), None);
        // Nothing to invert: stays deselected.
        app.handle_action(&ctx, PhotoAction::InvertSelection);
        assert_eq!(selection(&app), None);
    }

    #[test]
    fn actions_route_to_local_state() {
        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        // Without a document: no-op, no panic.
        app.handle_action(&ctx, PhotoAction::ToggleGrid);
        app.open_sized_tab(800, 600);
        let before = app.active_doc_opt().expect("doc actif").ui.show_grid;
        app.handle_action(&ctx, PhotoAction::ToggleGrid);
        assert_eq!(
            app.active_doc_opt().expect("doc actif").ui.show_grid,
            !before
        );
        app.handle_action(&ctx, PhotoAction::ZoomIn);
        app.handle_action(&ctx, PhotoAction::ZoomReset);
        app.handle_action(&ctx, PhotoAction::ShowHelp);
        assert!(app.shell.help_open);
    }

    #[test]
    fn select_action_updates_selection_without_worker() {
        use uuid::Uuid;
        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        // Without a document: no-op, no panic.
        let id = Uuid::new_v4();
        app.handle_action(&ctx, PhotoAction::SelectLayer(id));
        assert!(app.active_doc_opt().is_none());
        app.open_sized_tab(800, 600);
        app.handle_action(&ctx, PhotoAction::SelectLayer(id));
        assert_eq!(
            app.active_doc_opt().expect("doc actif").ui.selected,
            Some(id)
        );
    }

    #[test]
    fn full_layout_draws_without_panic() {
        let mut app = PhotoApp::new();
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let ctx_clone = ctx.clone();
        // Welcome + creation dialog (zero documents).
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(&ctx_clone, ui);
        })
        .drop_without_applying_deltas();
        assert!(app.queue.is_empty());
        // Then the full workspace with one document.
        app.open_sized_tab(64, 64);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(&ctx_clone, ui);
        })
        .drop_without_applying_deltas();
        // Draw leaves no pending action.
        assert!(app.queue.is_empty());
    }

    #[test]
    fn engine_error_sets_status() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::EngineError {
                error: photo_engine::session::EngineError::NothingToExport,
            },
        );
        assert_eq!(
            ui.status,
            crate::ui::status::PhotoStatus::Error(
                photo_engine::session::EngineError::NothingToExport
            )
        );
    }

    #[test]
    fn zoom_actions_send_no_worker_command() {
        // §28 6G.1: display zoom = state-only. No worker command,
        // hence no response, no render, no upload, no regeneration.
        use std::time::{Duration, Instant};

        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        app.open_sized_tab(64, 64);
        let zoom_before = app.active_doc_opt().expect("doc").ui.viewport.zoom();
        // Deterministic barrier: drain until SILENCE (boot emits
        // `LayersChanged` then, since 6G.3, background `ThumbnailUpdated`
        // — counting ONE response breaks as soon as the thumbnail thread
        // beats the drain). After 300 ms without a response, any later
        // traffic can only come from the zoom tested below.
        let start = Instant::now();
        let mut last_traffic = Instant::now();
        let mut saw_response = false;
        loop {
            match app.active_doc_opt().expect("doc").rx.try_recv() {
                Ok(_) => {
                    saw_response = true;
                    last_traffic = Instant::now();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    assert!(
                        start.elapsed() < Duration::from_secs(60),
                        "réponse de boot attendue"
                    );
                    if saw_response && last_traffic.elapsed() > Duration::from_millis(300) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("worker arrêté prématurément")
                }
            }
        }
        // Zoom gestures: display-only state, zero worker traffic.
        fn zoom_of(app: &PhotoApp) -> f32 {
            app.active_doc_opt().expect("doc").ui.viewport.zoom()
        }
        app.handle_action(&ctx, crate::commands::PhotoAction::ZoomIn);
        assert_ne!(zoom_before, zoom_of(&app), "le zoom display a bougé");
        app.handle_action(&ctx, crate::commands::PhotoAction::ZoomOut);
        app.handle_action(&ctx, crate::commands::PhotoAction::ZoomReset);
        std::thread::sleep(Duration::from_millis(300));
        match app.active_doc_opt().expect("doc").rx.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Ok(_) => panic!("le zoom ne doit provoquer aucune réponse worker"),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("worker arrêté prématurément")
            }
        }
    }

    #[test]
    fn heavy_image_open_never_blocks_ui_thread() {
        use crate::ui::PhotoEngineCommand;
        use std::time::{Duration, Instant};

        // Large image (3000x3000): expensive decode + composite,
        // entirely on the worker.
        let path =
            std::env::temp_dir().join(format!("cygnus_phase5_big_{}.png", std::process::id()));
        image::save_buffer(
            &path,
            &vec![128u8; 3000 * 3000 * 3],
            3000,
            3000,
            image::ColorType::Rgb8,
        )
        .expect("image de test");
        let mut app = PhotoApp::new();
        app.open_sized_tab(64, 64);
        app.active_doc_opt()
            .expect("doc actif")
            .tx
            .send(PhotoEngineCommand::OpenImage { path: path.clone() })
            .expect("envoi commande");

        // Egui loop pattern: non-blocking poll, the UI thread
        // stays free (each try_recv returns immediately). We
        // ignore earlier responses (boot initial snapshot).
        let start = Instant::now();
        let mut polls = 0;
        let layers = loop {
            polls += 1;
            match app.active_doc_opt().expect("doc actif").rx.try_recv() {
                Ok(PhotoEngineResponse::LayersChanged { layers, .. })
                    if layers
                        .iter()
                        .any(|layer| layer.name.starts_with("cygnus_phase5_big_")) =>
                {
                    break layers;
                }
                Ok(_) => {
                    // Initial snapshot or intermediate preview: keep going.
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    assert!(
                        start.elapsed() < Duration::from_secs(60),
                        "worker bloqué ou perdu"
                    );
                    std::thread::yield_now();
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("worker arrêté prématurément")
                }
            }
        };
        assert!(polls >= 1, "au moins un poll non bloquant");
        assert!(
            layers
                .iter()
                .any(|layer| layer.name.starts_with("cygnus_phase5_big_")),
            "calque de l'image lourde présent"
        );
        let _ = std::fs::remove_file(&path);
    }
}
