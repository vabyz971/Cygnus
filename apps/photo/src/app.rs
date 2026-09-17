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

//! Orchestrateur de l'app Photo : état, commandes, moteur, workspace.
//!
//! `PhotoApp` ne dessine RIEN lui-même : il possède les documents
//! (workers + états), draine la [`PhotoCommandQueue`] (routage vers
//! l'état local ou le worker) et appelle [`PhotoWorkspace`] pour un
//! frame. Aucune logique de panel, canvas, menu ou toolbar ici.

use crate::commands::{PhotoAction, PhotoCommandQueue, PhotoUiContext};
use crate::layout::PhotoWorkspace;
use crate::layout::dock::{PhotoDockTab, default_dock_state, ensure_tab, push_canvas_tab};
use crate::persistence::{load_dock_or_default, load_workspace_or_default};
use crate::state::{
    OpenDocument, PhotoRuntimeState, PhotoShellState, PhotoUiState, apply_response,
};
use crate::ui::{PhotoEngineCommand, spawn_photo_engine_worker};
use photo_engine::Document;
use std::sync::mpsc::channel;
use ui_kit::dialogs::pick_image_to_open;
use ui_kit::layout::PanelId;

/// App Photo : documents à onglets + coquille + runtime + file d'actions.
pub struct PhotoApp {
    /// Documents ouverts (toujours au moins un).
    pub docs: Vec<OpenDocument>,
    /// Index du document actif.
    pub active: usize,
    /// État global de la coquille (workspace, dialogs).
    pub shell: PhotoShellState,
    /// État éphémère (pickers, catalogue, compteurs).
    pub runtime: PhotoRuntimeState,
    /// Actions UI en attente (drainées chaque frame).
    pub queue: PhotoCommandQueue,
}

impl PhotoApp {
    /// Crée l'app avec un document vide.
    pub fn new() -> Self {
        // Restauration du layout : aucun JSON stocké pour l'instant
        // (câblage disque via `preferences` : phase suivante).
        let mut shell = PhotoShellState {
            workspace: load_workspace_or_default(None),
            ..Default::default()
        };
        // Photo n'a pas (encore) de timeline : région basse masquée.
        shell.workspace.set_visible(PanelId::Timeline, false);
        let mut app = Self {
            docs: Vec::new(),
            active: 0,
            shell,
            runtime: PhotoRuntimeState::new(),
            queue: PhotoCommandQueue::new(),
        };
        app.open_blank_tab();
        // Disposition des docks autour des canevas (câblage disque
        // via `preferences` : phase suivante).
        app.shell.dock_state = load_dock_or_default(None, &app.doc_ids());
        app
    }

    /// Document actif (toujours présent : jamais zéro document).
    pub fn active_doc(&self) -> &OpenDocument {
        &self.docs[self.active.min(self.docs.len().saturating_sub(1))]
    }

    /// Document actif mutable.
    pub fn active_doc_mut(&mut self) -> &mut OpenDocument {
        let index = self.active.min(self.docs.len().saturating_sub(1));
        &mut self.docs[index]
    }

    /// Ouvre un onglet sur un document neuf et demande son snapshot.
    pub fn open_blank_tab(&mut self) {
        self.runtime.untitled_counter += 1;
        let title = format!("Sans titre {}", self.runtime.untitled_counter);
        self.spawn_document(title, Document::new(1920, 1080));
    }

    /// Ouvre un onglet aux dimensions validées (fenêtre Nouveau doc).
    pub fn open_sized_tab(&mut self, width: u32, height: u32) {
        self.runtime.untitled_counter += 1;
        let title = format!("Sans titre {}", self.runtime.untitled_counter);
        self.spawn_document(title, Document::new(width, height));
    }

    /// Crée le worker d'un document, lui ajoute son onglet canevas
    /// et l'active.
    pub fn spawn_document(&mut self, title: String, document: Document) {
        let (tx, worker_rx) = channel();
        let (worker_tx, rx) = channel();
        // Handle détaché volontairement : le worker vit tant que
        // le channel est ouvert, sans jamais bloquer l'UI.
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
            ui,
            tx,
            rx,
        });
        self.active = self.docs.len() - 1;
        push_canvas_tab(&mut self.shell.dock_state, PhotoDockTab::Canvas(id));
        let _ = self.active_doc().tx.send(PhotoEngineCommand::Refresh);
    }

    /// Ids des documents ouverts (canevas du layout).
    fn doc_ids(&self) -> Vec<uuid::Uuid> {
        self.docs.iter().map(|doc| doc.id).collect()
    }

    /// Ferme l'onglet actif (jamais zéro document : le dernier est
    /// remplacé par un neuf). Le worker s'arrête à la chute du channel.
    pub fn close_active_tab(&mut self) {
        if self.docs.is_empty() {
            self.open_blank_tab();
            return;
        }
        let index = self.active.min(self.docs.len() - 1);
        self.close_document(self.docs[index].id);
    }

    /// Ferme le document `id` et retire son onglet canevas (sans
    /// effet si inconnu ; jamais zéro document).
    pub fn close_document(&mut self, id: uuid::Uuid) {
        if let Some(index) = self.docs.iter().position(|doc| doc.id == id) {
            self.docs.remove(index);
            if let Some(path) = self.shell.dock_state.find_tab(&PhotoDockTab::Canvas(id)) {
                self.shell.dock_state.remove_tab(path);
            }
            // Un document avant l'actif décalerait la sélection.
            if index < self.active {
                self.active -= 1;
            }
        }
        if self.docs.is_empty() {
            self.open_blank_tab();
        } else {
            self.active = self.active.min(self.docs.len() - 1);
        }
    }

    /// Envoie une commande au worker du document actif (échec
    /// silencieux si le worker est arrêté : l'UI ne panique jamais).
    fn send_active(&self, command: PhotoEngineCommand) {
        let _ = self.active_doc().tx.send(command);
    }

    /// Route une action UI : état local ou commande worker.
    ///
    /// Seul endroit (avec [`apply_response`](crate::state::apply_response))
    /// où l'app touche au moteur : les panels n'y accèdent jamais.
    pub fn handle_action(&mut self, ctx: &egui::Context, action: PhotoAction) {
        match action {
            PhotoAction::OpenNewDocumentDialog => self.shell.new_doc_dialog.open(),
            PhotoAction::CreateDocument { width, height } => self.open_sized_tab(width, height),
            PhotoAction::OpenImageDialog | PhotoAction::AddImageLayer => {
                self.runtime.open_picker = Some(pick_image_to_open());
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
            PhotoAction::AddEmptyLayer => self.send_active(PhotoEngineCommand::AddEmptyLayer),
            PhotoAction::DuplicateSelectedLayer => {
                if let Some(id) = self.active_doc().ui.selected {
                    self.send_active(PhotoEngineCommand::DuplicateLayer(id));
                }
            }
            PhotoAction::DeleteSelectedLayer => {
                if let Some(id) = self.active_doc().ui.selected {
                    self.send_active(PhotoEngineCommand::DeleteLayer(id));
                }
            }
            PhotoAction::DeleteLayer(id) => {
                self.send_active(PhotoEngineCommand::DeleteLayer(id));
            }
            PhotoAction::AddMaskToSelected => {
                if let Some(id) = self.active_doc().ui.selected {
                    self.send_active(PhotoEngineCommand::AddMask { layer: id });
                }
            }
            PhotoAction::OpenFilterMenu => {
                self.shell.filter_modal.open = self.active_doc().ui.selected.is_some();
                self.shell.filter_modal.choice = 0;
            }
            PhotoAction::AddFilter { layer, filter_type } => {
                self.send_active(PhotoEngineCommand::AddFilter { layer, filter_type });
            }
            PhotoAction::SelectLayer(id) => {
                self.active_doc_mut().ui.selected = Some(id);
            }
            PhotoAction::RenameLayer { layer, name } => {
                self.send_active(PhotoEngineCommand::RenameLayer { layer, name });
            }
            PhotoAction::ReorderLayers { from, to } => {
                self.send_active(PhotoEngineCommand::ReorderLayer { from, to });
            }
            PhotoAction::ToggleLayerVisibility(id) => {
                self.send_active(PhotoEngineCommand::ToggleLayerVisibility(id));
            }
            PhotoAction::SetOpacity { layer, opacity } => {
                self.send_active(PhotoEngineCommand::SetOpacity { layer, opacity });
            }
            PhotoAction::SetBlendMode { layer, mode } => {
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
                self.active_doc_mut().ui.tool = tool;
            }
            PhotoAction::ToggleGrid => {
                let ui = &mut self.active_doc_mut().ui;
                ui.show_grid = !ui.show_grid;
            }
            PhotoAction::ZoomIn => {
                self.active_doc_mut().ui.viewport.zoom_by(1.25, None);
            }
            PhotoAction::ZoomOut => {
                self.active_doc_mut().ui.viewport.zoom_by(0.8, None);
            }
            PhotoAction::ZoomReset => {
                self.active_doc_mut().ui.viewport.reset();
            }
            PhotoAction::CommitStroke(paint) => {
                self.send_active(PhotoEngineCommand::PaintStroke {
                    layer: paint.layer,
                    points: paint.points,
                    eraser: paint.eraser,
                    radius: paint.radius,
                    color: paint.color,
                    opacity: paint.opacity,
                });
            }
            PhotoAction::ShowHelp => {
                self.shell.help_open = true;
            }
            PhotoAction::ShowDockTab(tab) => {
                ensure_tab(&mut self.shell.dock_state, tab);
            }
            PhotoAction::ResetDockLayout => {
                let canvases = self
                    .doc_ids()
                    .into_iter()
                    .map(PhotoDockTab::Canvas)
                    .collect();
                self.shell.dock_state = default_dock_state(canvases);
            }
            PhotoAction::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Poll non bloquant : réponses workers + file pickers.
    pub fn poll(&mut self, ctx: &egui::Context) {
        for doc in &mut self.docs {
            while let Ok(response) = doc.rx.try_recv() {
                apply_response(ctx, &mut doc.ui, response);
            }
        }
        // Ouverture d'image → document actif.
        if let Some(rx) = self.runtime.open_picker.take() {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    self.send_active(PhotoEngineCommand::OpenImage { path });
                }
                Ok(None) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.runtime.open_picker = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.active_doc_mut().ui.status =
                        String::from("Dialogue de fichier indisponible");
                }
            }
        }
    }

    /// Un frame complet : poll, workspace, drainage des actions.
    pub fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.poll(ctx);
        let photo_ctx = PhotoUiContext::for_frame(ctx);
        PhotoWorkspace::show(ui, self, &photo_ctx);
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
    fn app_boots_with_one_blank_doc() {
        let mut app = PhotoApp::new();
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active, 0);
        // Snapshot initial du worker (document vide).
        let response = app
            .active_doc()
            .rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("snapshot initial");
        let ctx = egui::Context::default();
        apply_response(&ctx, &mut app.active_doc_mut().ui, response);
        assert!(app.active_doc().ui.layers.is_empty());
    }

    #[test]
    fn tabs_open_close() {
        use uuid::Uuid;

        let mut app = PhotoApp::new();
        app.open_blank_tab();
        app.open_blank_tab();
        assert_eq!(app.docs.len(), 3);
        // Un onglet canevas par document.
        for doc in &app.docs {
            assert!(
                app.shell
                    .dock_state
                    .find_tab(&PhotoDockTab::Canvas(doc.id))
                    .is_some()
            );
        }
        // Fermer l'actif retire aussi son canevas.
        let removed = app.active_doc().id;
        app.close_active_tab();
        assert_eq!(app.docs.len(), 2);
        assert!(
            app.shell
                .dock_state
                .find_tab(&PhotoDockTab::Canvas(removed))
                .is_none()
        );
        // Fermer jusqu'au dernier : jamais zéro document.
        app.close_active_tab();
        app.close_active_tab();
        assert_eq!(app.docs.len(), 1);
        // Id inconnu : sans effet.
        app.close_document(Uuid::from_u128(999));
        assert_eq!(app.docs.len(), 1);
    }

    #[test]
    fn closing_before_active_keeps_selection() {
        let mut app = PhotoApp::new();
        app.open_blank_tab();
        app.open_blank_tab();
        let middle = app.docs[1].id;
        app.active = 1;
        app.close_document(app.docs[0].id);
        assert_eq!(app.docs.len(), 2);
        assert_eq!(app.active, 0);
        assert_eq!(app.active_doc().id, middle);
    }

    #[test]
    fn actions_route_to_local_state() {
        let ctx = egui::Context::default();
        let mut app = PhotoApp::new();
        let before = app.active_doc().ui.show_grid;
        app.handle_action(&ctx, PhotoAction::ToggleGrid);
        assert_eq!(app.active_doc().ui.show_grid, !before);
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
        let id = Uuid::new_v4();
        app.handle_action(&ctx, PhotoAction::SelectLayer(id));
        assert_eq!(app.active_doc().ui.selected, Some(id));
    }

    #[test]
    fn full_layout_draws_without_panic() {
        let mut app = PhotoApp::new();
        app.active_doc_mut().ui.status = String::from("test");
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let ctx_clone = ctx.clone();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(&ctx_clone, ui);
        })
        .drop_without_applying_deltas();
        // Le draw ne laisse aucune action en attente.
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
    fn heavy_image_open_never_blocks_ui_thread() {
        use crate::ui::PhotoEngineCommand;
        use std::time::{Duration, Instant};

        // Grosse image (3000x3000) : décodage + composite coûteux,
        // intégralement sur le worker.
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
        let app = PhotoApp::new();
        app.active_doc()
            .tx
            .send(PhotoEngineCommand::OpenImage { path: path.clone() })
            .expect("envoi commande");

        // Pattern de la boucle egui : poll non bloquant, le thread UI
        // reste libre (chaque try_recv retourne immédiatement). On
        // ignore les réponses antérieures (snapshot initial du boot).
        let start = Instant::now();
        let mut polls = 0;
        let layers = loop {
            polls += 1;
            match app.active_doc().rx.try_recv() {
                Ok(PhotoEngineResponse::LayersChanged { layers, .. })
                    if layers
                        .iter()
                        .any(|layer| layer.name.starts_with("cygnus_phase5_big_")) =>
                {
                    break layers;
                }
                Ok(_) => {
                    // Snapshot initial ou aperçu intermédiaire : on continue.
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
