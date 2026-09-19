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
    CreateDocumentDialogState, ExportDialogState, LayerRenameState, PhotoBrushSettings,
    PhotoCanvasTool, PhotoEditMode, PhotoEngineResponse, PhotoLayerInfo, PreviewImage,
};
use photo_engine::RenderRevision;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use ui_kit::layout::WorkspaceState;
use ui_kit::utils::ReorderDragState;
use ui_kit::viewport::{ViewportState, ViewportTextureCache};
use uuid::Uuid;

/// Géométrie de l'aperçu : où se trouve le document dans la miniature
/// (plan infini : le composite dépasse dès qu'un calque sort du cadre).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PreviewGeom {
    /// Dimensions de la miniature en pixels.
    pub thumb_size: egui::Vec2,
    /// Dimensions du composite pleine résolution.
    pub full_size: egui::Vec2,
    /// Coin haut-gauche du document en pixels miniature.
    pub origin: egui::Vec2,
    /// Dimensions du document en pixels.
    pub doc_size: egui::Vec2,
}

impl PreviewGeom {
    /// Facteur miniature→pixels document (miniature uniforme).
    pub fn thumb_to_doc(self) -> f32 {
        if self.thumb_size.x > 0.0 && self.full_size.x > 0.0 {
            self.full_size.x / self.thumb_size.x
        } else {
            1.0
        }
    }

    /// Rectangle écran du document à partir du rectangle écran de la
    /// miniature (`None` sans géométrie valide).
    pub fn doc_rect(self, dest: egui::Rect) -> Option<egui::Rect> {
        if self.thumb_size.x <= 0.0 || self.doc_size.x <= 0.0 || self.doc_size.y <= 0.0 {
            return None;
        }
        let scale = dest.width() / self.thumb_size.x;
        if scale <= 0.0 {
            return None;
        }
        let min = egui::pos2(
            dest.min.x + self.origin.x * scale,
            dest.min.y + self.origin.y * scale,
        );
        let kx = if self.full_size.x > 0.0 {
            self.thumb_size.x / self.full_size.x
        } else {
            1.0
        };
        let size = egui::vec2(self.doc_size.x * kx * scale, self.doc_size.y * kx * scale);
        Some(egui::Rect::from_min_size(min, size))
    }
}

/// Miniature de calque téléversée (cache textures du panneau).
pub(crate) struct CachedLayerThumb {
    /// Texture GPU (vivante tant que le calque est affiché).
    handle: egui::TextureHandle,
    /// Version d'apparence servie (re-téléversement si changée).
    version: u64,
    /// Dimensions de la miniature en pixels.
    size: egui::Vec2,
}

impl PhotoUiState {
    /// Vue des miniatures pour le panneau Calques (id texture + taille).
    pub fn thumb_views(&self) -> HashMap<Uuid, crate::ui::LayerThumbView> {
        self.thumb_cache
            .iter()
            .map(|(id, cached)| {
                (
                    *id,
                    crate::ui::LayerThumbView {
                        texture_id: cached.handle.id(),
                        size: cached.size,
                    },
                )
            })
            .collect()
    }
}

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
    /// Géométrie du dernier aperçu (cadre document dans la miniature).
    pub preview_geom: Option<PreviewGeom>,
    /// Aperçu rogné au document (menu Affichage, défaut : plan infini).
    pub preview_clip: bool,
    /// Textures des miniatures du panneau Calques, par calque.
    pub thumb_cache: HashMap<Uuid, CachedLayerThumb>,
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
    /// INSTRUMENTATION TEMPORAIRE (diagnostic perf) : réponses worker
    /// appliquées (== previews reçus + erreurs/exports).
    pub responses_applied: u64,
    /// INSTRUMENTATION TEMPORAIRE : uploads de textures egui
    /// (aperçu + miniatures périmées).
    pub texture_uploads: u64,
    /// Révision du rendu affiché (`None` = aucune texture) : les
    /// réponses obsolètes ne re-téléversent pas.
    pub displayed_revision: Option<RenderRevision>,
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
    /// Fenêtre « Créer un document » (format, dimensions, orientation).
    pub new_doc_dialog: CreateDocumentDialogState,
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
            new_doc_dialog: CreateDocumentDialogState::default(),
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

/// Cache des miniatures : re-téléverse uniquement les calques dont
/// l'apparence a changé, purge les disparus. Partagé par les deux
/// bras d'état (`LayersChanged` et `StateChanged`).
fn sync_thumbs(ctx: &egui::Context, ui: &mut PhotoUiState) {
    for layer in &ui.layers {
        if let Some(thumb) = layer.thumb.as_ref() {
            let stale = ui
                .thumb_cache
                .get(&layer.id)
                .is_none_or(|cached| cached.version != thumb.version);
            if stale {
                let handle = ctx.load_texture(
                    format!("photo_layer_thumb_{}", layer.id),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [thumb.width as usize, thumb.height as usize],
                        &thumb.rgba,
                    ),
                    egui::TextureOptions::LINEAR,
                );
                // INSTRUMENTATION TEMPORAIRE.
                ui.texture_uploads += 1;
                ui.thumb_cache.insert(
                    layer.id,
                    CachedLayerThumb {
                        handle,
                        version: thumb.version,
                        size: egui::vec2(thumb.width as f32, thumb.height as f32),
                    },
                );
            }
        }
    }
    ui.thumb_cache
        .retain(|id, _| ui.layers.iter().any(|layer| layer.id == *id));
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
            revision,
            can_undo,
            can_redo,
        } => {
            ui.layers = layers;
            ui.can_undo = can_undo;
            ui.can_redo = can_redo;
            // INSTRUMENTATION TEMPORAIRE.
            ui.responses_applied += 1;
            if ui
                .selected
                .is_some_and(|id| !ui.layers.iter().any(|layer| layer.id == id))
            {
                ui.selected = None;
            }
            sync_thumbs(ctx, ui);
            match preview {
                Some(image) => {
                    // Garde anti-obsolescence : une réponse en retard
                    // (révision <= texture affichée) ne re-téléverse pas.
                    let fresh = ui
                        .displayed_revision
                        .is_none_or(|shown| revision.is_newer_than(shown));
                    if fresh {
                        ui.preview_geom = Some(PreviewGeom {
                            thumb_size: egui::vec2(image.width as f32, image.height as f32),
                            full_size: egui::vec2(
                                image.full_width as f32,
                                image.full_height as f32,
                            ),
                            origin: egui::vec2(image.origin_x, image.origin_y),
                            doc_size: egui::vec2(image.doc_width as f32, image.doc_height as f32),
                        });
                        ui.texture_cache.update(
                            ctx,
                            "photo_preview",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width as usize, image.height as usize],
                                &image.rgba,
                            ),
                        );
                        // INSTRUMENTATION TEMPORAIRE.
                        ui.texture_uploads += 1;
                        ui.last_preview = Some(image);
                        ui.displayed_revision = Some(revision);
                        ui.status.clear();
                    }
                }
                None => {
                    ui.texture_cache.clear();
                    ui.last_preview = None;
                    ui.preview_geom = None;
                    ui.displayed_revision = None;
                }
            }
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::StateChanged {
            layers,
            revision: _,
            can_undo,
            can_redo,
        } => {
            // Snapshot seul : la texture affichée est conservée telle
            // quelle (aucun nouveau rendu produit côté worker).
            ui.layers = layers;
            ui.can_undo = can_undo;
            ui.can_redo = can_redo;
            // INSTRUMENTATION TEMPORAIRE.
            ui.responses_applied += 1;
            if ui
                .selected
                .is_some_and(|id| !ui.layers.iter().any(|layer| layer.id == id))
            {
                ui.selected = None;
            }
            sync_thumbs(ctx, ui);
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
            full_width: 2,
            full_height: 2,
            origin_x: 0.0,
            origin_y: 0.0,
            doc_width: 2,
            doc_height: 2,
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
            full_width: 0,
            full_height: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            doc_width: 0,
            doc_height: 0,
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

    fn preview_at_revision(revision: RenderRevision) -> PhotoEngineResponse {
        PhotoEngineResponse::LayersChanged {
            layers: Vec::new(),
            preview: Some(PreviewImage {
                width: 2,
                height: 2,
                rgba: vec![
                    10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
                ],
                full_width: 2,
                full_height: 2,
                origin_x: 0.0,
                origin_y: 0.0,
                doc_width: 2,
                doc_height: 2,
            }),
            revision,
            can_undo: false,
            can_redo: false,
        }
    }

    #[test]
    fn stale_preview_revision_does_not_reupload() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(2)));
        assert_eq!(ui.displayed_revision, Some(RenderRevision(2)));
        let uploads = ui.texture_uploads;
        let texture = ui.texture_cache.texture().expect("texture");
        // Réponse en retard : texture et aperçu conservés.
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(1)));
        assert_eq!(ui.texture_uploads, uploads, "aucun re-téléversement");
        assert_eq!(ui.texture_cache.texture().expect("texture"), texture);
        assert_eq!(ui.displayed_revision, Some(RenderRevision(2)));
        // Révision plus récente : téléversement normal.
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(3)));
        assert_eq!(ui.texture_uploads, uploads + 1);
        assert_eq!(ui.displayed_revision, Some(RenderRevision(3)));
    }

    #[test]
    fn state_changed_keeps_displayed_texture() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(4)));
        let uploads = ui.texture_uploads;
        let texture = ui.texture_cache.texture().expect("texture");
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::StateChanged {
                layers: Vec::new(),
                revision: RenderRevision(4),
                can_undo: true,
                can_redo: false,
            },
        );
        // Panneau à jour (undo), texture intacte.
        assert!(ui.can_undo);
        assert_eq!(ui.texture_uploads, uploads);
        assert_eq!(ui.texture_cache.texture().expect("texture"), texture);
        assert_eq!(ui.displayed_revision, Some(RenderRevision(4)));
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

#[cfg(test)]
mod geom_tests {
    use super::PreviewGeom;

    #[test]
    fn doc_rect_stable_quand_la_miniature_deborde() {
        // Miniature 28x8 (plan infini), document 8x8 à l'origine (10, 0).
        let geom = PreviewGeom {
            thumb_size: egui::vec2(28.0, 8.0),
            full_size: egui::vec2(28.0, 8.0),
            origin: egui::vec2(10.0, 0.0),
            doc_size: egui::vec2(8.0, 8.0),
        };
        assert!((geom.thumb_to_doc() - 1.0).abs() < 1e-5);
        // Dest écran 280x80 à (0, 0) : échelle 10.
        let dest = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(280.0, 80.0));
        let rect = geom.doc_rect(dest).expect("cadre document");
        assert!((rect.min.x - 100.0).abs() < 1e-3);
        assert!((rect.min.y - 0.0).abs() < 1e-3);
        assert!((rect.width() - 80.0).abs() < 1e-3);
        assert!((rect.height() - 80.0).abs() < 1e-3);
    }

    #[test]
    fn doc_rect_none_sans_geometrie_valide() {
        let dest = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let empty = PreviewGeom::default();
        assert!(empty.doc_rect(dest).is_none());
    }
}
