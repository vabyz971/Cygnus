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
    OpenDocument, PhotoRuntimeState, PhotoShellState, PhotoUiState, apply_response,
};
use crate::ui::{PhotoEngineCommand, spawn_photo_engine_worker};
use photo_engine::Document;
use std::path::Path;
use std::sync::mpsc::channel;
use ui_kit::dialogs::{pick_image_to_open, pick_project_to_open, pick_project_to_save};
use ui_kit::layout::PanelId;

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
        };
        // No initial document: the user creates (or opens).
        app.shell.new_doc_dialog.open();
        // Dock arrangement around the canvases (disk wiring
        // via `preferences`: next phase).
        app.shell.tree = load_tree_or_default(None, &app.doc_ids());
        app
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
        let title = format!("Sans titre {}", self.runtime.untitled_counter);
        self.spawn_document(title, Document::with_blank_layer(width, height));
    }

    /// Spawns a document's worker, adds its canvas tab
    /// and activates it.
    pub fn spawn_document(&mut self, title: String, document: Document) {
        let (tx, worker_rx) = channel();
        let (worker_tx, rx) = channel();
        // Handle deliberately detached: the worker lives while
        // the channel is open, never blocking the UI.
        std::mem::forget(spawn_photo_engine_worker(document, worker_rx, worker_tx));
        let ui = PhotoUiState {
            needs_repaint: true,
            status: String::from("Pret"),
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
        });
        self.active = self.docs.len() - 1;
        push_canvas_tab(&mut self.shell.tree, PhotoDockTab::Canvas(id));
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
    fn update_layer_opt(
        &mut self,
        id: uuid::Uuid,
        update: impl Fn(&mut crate::ui::PhotoLayerInfo),
    ) {
        if let Some(doc) = self.active_doc_mut_opt()
            && let Some(layer) = doc.ui.layers.iter_mut().find(|layer| layer.id == id)
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
                        doc.ui.status = format!("Enregistrement : {}", path.display());
                    }
                    self.send_active(PhotoEngineCommand::SaveProject { path });
                } else {
                    // Never saved: fall back to "Save as".
                    self.runtime.project_save_picker = Some(pick_project_to_save());
                }
            }
            PhotoAction::OpenExportDialog => self.shell.export_dialog.open(),
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
            PhotoAction::AddMaskToSelected => {
                if let Some(id) = self.active_doc_opt().and_then(|doc| doc.ui.selected) {
                    self.send_active(PhotoEngineCommand::AddMask { layer: id });
                }
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
            PhotoAction::SelectLayer(id) => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.selected = Some(id);
                    doc.ui.focused_attachment = None;
                }
            }
            PhotoAction::FocusAttachment { owner, id } => {
                if let Some(doc) = self.active_doc_mut_opt() {
                    doc.ui.selected = Some(owner);
                    doc.ui.focused_attachment = Some(crate::ui::AttachmentRef { owner, id });
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
            PhotoAction::MoveIntoGroup { layer, group } => {
                self.send_active(PhotoEngineCommand::MoveIntoGroup { layer, group });
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
            PhotoAction::MoveFilter { layer, filter, up } => {
                self.send_active(PhotoEngineCommand::MoveFilter { layer, filter, up });
            }
            PhotoAction::MoveMask { owner, mask, up } => {
                self.send_active(PhotoEngineCommand::MoveMask { owner, mask, up });
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
                // Focused mask of the selected layer: the stroke
                // goes into the mask (white/black), otherwise into
                // the layer source (historical behavior).
                let mask_target = self.active_doc_opt().and_then(|doc| {
                    let focus = doc.ui.focused_attachment?;
                    if Some(focus.owner) != doc.ui.selected {
                        return None;
                    }
                    doc.ui
                        .layers
                        .iter()
                        .find(|layer| layer.id == focus.owner)
                        .filter(|layer| layer.masks.iter().any(|m| m.id == focus.id))
                        .map(|_| focus.id)
                });
                if let Some(mask) = mask_target {
                    self.send_active(PhotoEngineCommand::PaintMask {
                        owner: paint.layer,
                        mask,
                        points: paint.points,
                        eraser: paint.eraser,
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
            PhotoAction::ShowHelp => {
                self.shell.help_open = true;
            }
            PhotoAction::ShowDockTab(tab) => {
                ensure_tab(&mut self.shell.tree, tab);
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
                        let title = format!("Sans titre {}", self.runtime.untitled_counter);
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
                        doc.ui.status = String::from("Dialogue de fichier indisponible");
                    }
                }
            }
        }
        // `.cygp` project opening → ALWAYS a new tab (never
        // merged into the active document: a project replaces everything).
        if let Some(rx) = self.runtime.project_open_picker.take() {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    let title = project_title(&path);
                    self.spawn_document(title, Document::new(1, 1));
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.project_path = Some(path.clone());
                        doc.ui.status = format!("Chargement : {}", path.display());
                        let _ = doc.tx.send(PhotoEngineCommand::LoadProject { path });
                    }
                }
                Ok(None) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.runtime.project_open_picker = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.ui.status = String::from("Dialogue de fichier indisponible");
                    }
                }
            }
        }
        // "Save as" → active document (path + title set at
        // dispatch, pure UI state; the worker only serializes).
        if let Some(rx) = self.runtime.project_save_picker.take() {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.title = project_title(&path);
                        doc.project_path = Some(path.clone());
                        doc.ui.status = format!("Enregistrement : {}", path.display());
                        let _ = doc.tx.send(PhotoEngineCommand::SaveProject { path });
                    }
                }
                Ok(None) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.runtime.project_save_picker = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if let Some(doc) = self.active_doc_mut_opt() {
                        doc.ui.status = String::from("Dialogue de fichier indisponible");
                    }
                }
            }
        }
    }

    /// One full frame: poll, workspace, action drain.
    pub fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.poll(ctx);
        let photo_ctx = PhotoUiContext::for_frame(ctx);
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
/// ("rendu.cygp" → "rendu"), "Sans titre" fallback when unreadable.
fn project_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from("Sans titre"))
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
                .contains("Enregistre :")
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
            if history.iter().any(|label| label == "Miroir horizontal") {
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
                .any(|label| label == "Peindre le masque")
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
        ui_kit::theme::setup_fonts(&ctx);
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
                message: String::from("echec test"),
            },
        );
        assert_eq!(ui.status, "echec test");
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
