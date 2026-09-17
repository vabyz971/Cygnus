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

//! État de l'app Photo, séparé en trois couches.
//!
//! - [`OpenDocument`] : lien worker + état par document (titre, UI).
//! - [`PhotoUiState`] : état d'interface par document (sélection,
//!   outil, viewport, drag…), jamais de pixels.
//! - [`PhotoShellState`] : état global de la coquille (workspace,
//!   dialogs, modales).
//! - [`PhotoRuntimeState`] : éphémère non sérialisable (file
//!   pickers, catalogue de filtres, compteurs).
//!
//! La frontière moteur→UI ([`apply_response`]) vit ici : c'est le
//! seul point de conversion des réponses worker en état UI.

use crate::layout::dock::PhotoDockTab;
use crate::ui::{
    ExportDialogState, LayerRenameState, NewDocumentDialogState, PhotoBrushSettings,
    PhotoCanvasTool, PhotoEditMode, PhotoEngineResponse, PhotoLayerInfo, PreviewImage,
};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use ui_kit::layout::WorkspaceState;
use ui_kit::utils::ReorderDragState;
use ui_kit::viewport::{ViewportState, ViewportTextureCache};
use uuid::Uuid;

/// État UI d'un document ouvert.
#[derive(Default)]
pub struct PhotoUiState {
    /// Snapshot pour le panneau (haut de pile d'abord).
    pub layers: Vec<PhotoLayerInfo>,
    /// Calque sélectionné (état UI pur).
    pub selected: Option<Uuid>,
    /// Outil canvas actif.
    pub tool: PhotoCanvasTool,
    /// Mode d'édition (Vector / Pixel / Layout, 2e rangée haute).
    pub edit_mode: PhotoEditMode,
    /// Zoom/pan du canvas.
    pub viewport: ViewportState,
    /// Texture du composite moteur.
    pub texture_cache: ViewportTextureCache,
    /// Dernier aperçu reçu (échantillonnage pipette).
    pub last_preview: Option<PreviewImage>,
    /// Trait de pinceau en cours (pixels image).
    pub stroke: Vec<egui::Vec2>,
    /// Réglages pinceau/gomme.
    pub brush: PhotoBrushSettings,
    /// État de drag du panneau calques.
    pub drag_state: ReorderDragState,
    /// Renommage inline d'un calque (double-clic).
    pub rename: LayerRenameState,
    /// Profondeurs d'historique (boutons undo/redo).
    pub can_undo: bool,
    /// Redo disponible.
    pub can_redo: bool,
    /// Message de statut.
    pub status: String,
    /// Grille du canvas.
    pub show_grid: bool,
    /// Repaint explicite demandé.
    pub needs_repaint: bool,
}

/// Modale d'ajout de filtre (sélection dans le registre moteur).
#[derive(Default)]
pub struct FilterModalState {
    /// Modale visible.
    pub open: bool,
    /// Index dans la liste des filtres.
    pub choice: usize,
}

/// Un document ouvert : worker moteur + état UI.
pub struct OpenDocument {
    /// Identifiant stable (onglet dock, persistance du layout).
    pub id: Uuid,
    /// Titre de l'onglet dock.
    pub title: String,
    /// État UI du document.
    pub ui: PhotoUiState,
    /// Commandes vers le worker.
    pub tx: std::sync::mpsc::Sender<crate::ui::engine_bridge::PhotoEngineCommand>,
    /// Réponses du worker (poll non bloquant).
    pub rx: Receiver<PhotoEngineResponse>,
    // NOTE : le JoinHandle du worker n'est pas conservé : le thread
    // se termine seul quand le `Sender` est lâché (fin du `recv`).
}

/// État global de la coquille (fenêtre unique, tous documents).
pub struct PhotoShellState {
    /// Layout des régions et panneaux (persistable, voir
    /// `crate::persistence`).
    pub workspace: WorkspaceState,
    /// Tuiles ancrables (outils, canevas, inspecteur, calques).
    /// `Tree` n'implémente pas `Default` : voir le `impl`
    /// manuel ci-dessous (arbre vide, reconstruit par l'app autour
    /// du premier document).
    pub tree: egui_tiles::Tree<PhotoDockTab>,
    /// Modale d'ajout de filtre.
    pub filter_modal: FilterModalState,
    /// Fenêtre « Nouveau document » (format, dimensions, orientation).
    pub new_doc_dialog: NewDocumentDialogState,
    /// Fenêtre « Exportation » (dossier, format, validation).
    pub export_dialog: ExportDialogState,
    /// Fenêtre d'aide visible.
    pub help_open: bool,
}

impl Default for PhotoShellState {
    /// Coquille par défaut : workspace standard + arbre vide
    /// (`Tree` n'a pas de `Default`). `PhotoApp::new`
    /// reconstruit aussitôt le layout autour du premier document.
    fn default() -> Self {
        Self {
            workspace: WorkspaceState::default(),
            tree: egui_tiles::Tree::empty("photo-tree"),
            filter_modal: FilterModalState::default(),
            new_doc_dialog: NewDocumentDialogState::default(),
            export_dialog: ExportDialogState::default(),
            help_open: false,
        }
    }
}

/// État éphémère d'exécution (non sérialisable).
pub struct PhotoRuntimeState {
    /// Catalogue des filtres (registre moteur, statique).
    pub filter_types: Vec<(String, String)>,
    /// File picker d'ouverture en cours (non bloquant).
    pub open_picker: Option<Receiver<Option<PathBuf>>>,
    /// Compteur « Sans titre ».
    pub untitled_counter: usize,
}

impl PhotoRuntimeState {
    /// Runtime initial (catalogue de filtres du registre moteur).
    pub fn new() -> Self {
        let filter_types = photo_engine::filterable_types()
            .iter()
            .map(|definition| (definition.name.clone(), definition.type_id.clone()))
            .collect();
        Self {
            filter_types,
            open_picker: None,
            untitled_counter: 0,
        }
    }
}

impl Default for PhotoRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Applique une réponse worker à l'état UI d'un document.
///
/// Seul point de conversion moteur→UI : snapshots + aperçu
/// composite (texture téléversée côté app, zéro régénération au
/// zoom/pan — state-only).
pub fn apply_response(ctx: &egui::Context, ui: &mut PhotoUiState, response: PhotoEngineResponse) {
    match response {
        PhotoEngineResponse::LayersChanged {
            layers,
            preview,
            can_undo,
            can_redo,
        } => {
            ui.layers = layers;
            ui.can_undo = can_undo;
            ui.can_redo = can_redo;
            if ui
                .selected
                .is_some_and(|id| !ui.layers.iter().any(|layer| layer.id == id))
            {
                ui.selected = None;
            }
            match preview {
                Some(image) => {
                    ui.texture_cache.update(
                        ctx,
                        "photo_preview",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width as usize, image.height as usize],
                            &image.rgba,
                        ),
                    );
                    ui.last_preview = Some(image);
                    ui.status.clear();
                }
                None => {
                    ui.texture_cache.clear();
                    ui.last_preview = None;
                }
            }
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::EngineError { message } => {
            ui.status = message;
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::ExportDone { path } => {
            ui.status = format!("Exporte : {}", path.display());
            ui.needs_repaint = true;
        }
    }
}

/// Échantillonne la couleur du composite sous `(x, y)` pixels image.
/// `None` hors bornes ou sans aperçu (la pipette ignore alors le clic).
pub fn sample_preview_color(preview: &PreviewImage, x: f32, y: f32) -> Option<[u8; 3]> {
    if preview.width == 0 || preview.height == 0 {
        return None;
    }
    let xi = x.round() as i64;
    let yi = y.round() as i64;
    if xi < 0 || yi < 0 || xi >= preview.width as i64 || yi >= preview.height as i64 {
        return None;
    }
    let offset = (yi as usize * preview.width as usize + xi as usize) * 4;
    preview
        .rgba
        .get(offset..offset + 3)
        .and_then(|pixel| <&[u8] as TryInto<&[u8; 3]>>::try_into(pixel).ok())
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_preview_color_clamps() {
        let preview = PreviewImage {
            width: 2,
            height: 2,
            rgba: vec![
                10, 20, 30, 255, 40, 50, 60, 255, //
                70, 80, 90, 255, 100, 110, 120, 255,
            ],
        };
        assert_eq!(sample_preview_color(&preview, 0.0, 0.0), Some([10, 20, 30]));
        assert_eq!(
            sample_preview_color(&preview, 1.0, 1.0),
            Some([100, 110, 120])
        );
        assert_eq!(sample_preview_color(&preview, 5.0, 0.0), None);
        assert_eq!(sample_preview_color(&preview, -1.0, 0.0), None);
        let empty = PreviewImage {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        };
        assert_eq!(sample_preview_color(&empty, 0.0, 0.0), None);
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
    fn export_done_sets_status() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ExportDone {
                path: PathBuf::from("/tmp/test.png"),
            },
        );
        assert!(ui.status.contains("test.png"));
    }

    #[test]
    fn shell_defaults_to_usable_workspace() {
        use ui_kit::layout::PanelId;
        let shell = PhotoShellState::default();
        assert!(!shell.help_open);
        assert!(!shell.filter_modal.open);
        assert!(shell.workspace.find(PanelId::Layers).is_some());
        assert!(shell.workspace.find(PanelId::Inspector).is_some());
    }
}
