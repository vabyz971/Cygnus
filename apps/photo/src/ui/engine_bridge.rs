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
#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.

//! Pont non bloquant vers le worker `photo-engine` (pattern v2 §3.4).
//!
//! La boucle egui ne touche JAMAIS au `Document` : elle envoie des
//! [`PhotoEngineCommand`] via `Sender` et poll les
//! [`PhotoEngineResponse`] via `try_recv` à chaque frame. Le worker
//! (thread background) applique les commandes au `Document` pur,
//! maintient un historique undo/redo (snapshots partagés, quasi
//! gratuits) et répond avec un snapshot frais + un aperçu composite
//! pour le canvas.
//!
//! Convention d'indices : le panneau affiche le haut de pile en
//! premier ; `ReorderLayer { from, to }` utilise ces indices
//! d'affichage. Le worker les reconvertit (`doc = len - 1 - display`)
//! avant de manipuler `document.root` (index 0 = bas de pile).

use super::features::layers::{PhotoLayerInfo, snapshot_layers};
use photo_engine::{BlendMode, Document};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use uuid::Uuid;

fn assert_send<T: Send>() {}

/// Plus grande dimension de l'aperçu canvas (pixels).
pub const PREVIEW_MAX_DIMENSION: u32 = 1600;

/// Aperçu composite pour le canvas (RGBA8, pur, thread-safe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewImage {
    /// Largeur en pixels.
    pub width: u32,
    /// Hauteur en pixels.
    pub height: u32,
    /// Pixels RGBA8 ligne par ligne.
    pub rgba: Vec<u8>,
}

/// Commandes UI → worker moteur.
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoEngineCommand {
    /// Basculer la visibilité d'un calque.
    ToggleLayerVisibility(Uuid),
    /// Réordonner (indices d'affichage : 0 = haut de pile).
    ReorderLayer { from: usize, to: usize },
    /// Régler l'opacité d'un calque (unités moteur 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Régler le mode de fusion d'un calque (choix discret).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Annuler la dernière mutation.
    Undo,
    /// Rétablir la dernière annulation.
    Redo,
    /// Commiter un trait de pinceau/gomme sur un calque pixels.
    ///
    /// `points` en pixels image (espace monde du viewport). La
    /// transform du calque est supposée identité en Phase 4 (cas des
    /// calques photo frais) ; le commit transform-aware complet
    /// (cf. `paint::commit_stroke`) viendra en Phase 5.
    PaintStroke {
        /// Calque pixels cible.
        layer: Uuid,
        /// Polyligne du trait (pixels image).
        points: Vec<(f32, f32)>,
        /// Vrai = gomme (réduit l'alpha), faux = pinceau.
        eraser: bool,
        /// Rayon en pixels image.
        radius: f32,
        /// Couleur RGB (ignorée en mode gomme).
        color: [u8; 3],
        /// Opacité du trait 0..=1.
        opacity: f32,
    },
    /// Ouvrir une image comme nouveau calque (décodage lourd, déjà
    /// sur le thread worker donc non bloquant pour l'UI).
    OpenImage {
        /// Chemin du fichier image.
        path: PathBuf,
    },
    /// Ajouter un calque vide transparent (dimensions du document).
    AddEmptyLayer,
    /// Dupliquer un calque (nouveaux ids, inséré au-dessus).
    DuplicateLayer(Uuid),
    /// Supprimer un calque (refusé s'il est le dernier).
    DeleteLayer(Uuid),
    /// Ajouter un filtre live à un calque pixels (`type_id` du registre).
    AddFilter {
        /// Calque pixels cible.
        layer: Uuid,
        /// Identifiant du filtre (ex. « brightness_contrast »).
        filter_type: String,
    },
    /// Renommer un calque, un filtre ou un masque (nom non vide).
    RenameLayer { layer: Uuid, name: String },
    /// Ajouter un masque blanc (tout visible) au calque porteur.
    AddMask { layer: Uuid },
    /// Supprimer un masque de son porteur.
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Déplacer un filtre dans la pile de son calque (`up` = vers le haut affiché).
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Déplacer un masque dans la pile de son porteur.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Supprimer un filtre d'un calque pixels.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Exporter le composite (format déduit de l'extension : png, jpg,
    /// jpeg, gif — gif = première image, sans transparence animée).
    Export {
        /// Chemin de destination.
        path: PathBuf,
        /// Qualité JPEG 1..=100 (ignorée hors JPEG).
        quality: u8,
    },
    /// Resynchronisation sans mutation (snapshot initial au boot).
    Refresh,
}

/// Réponses worker → UI.
#[derive(Debug, Clone)]
pub enum PhotoEngineResponse {
    /// État frais après application (haut de pile d'abord).
    LayersChanged {
        /// Snapshot pour l'UI.
        layers: Vec<PhotoLayerInfo>,
        /// Aperçu composite pour le canvas (`None` si vide).
        preview: Option<PreviewImage>,
        /// Profondeurs d'historique (pour griser undo/redo).
        can_undo: bool,
        /// Profondeur redo.
        can_redo: bool,
    },
    /// Échec non bloquant (ex. décodage impossible).
    EngineError {
        /// Message affichable dans la barre de statut.
        message: String,
    },
    /// Export PNG terminé.
    ExportDone {
        /// Chemin du fichier écrit.
        path: PathBuf,
    },
}

/// Convertit un index d'affichage (0 = haut de pile) en index
/// document (`root[0]` = bas de pile). `None` si hors limites.
pub fn display_to_doc_index(display: usize, len: usize) -> Option<usize> {
    if display < len {
        Some(len - 1 - display)
    } else {
        None
    }
}

/// Invalide l'apparence d'un porteur de masque après mutation de ses
/// masques (les setters de masques du moteur ne bumpent pas la version
/// eux-mêmes). Groupes : composition à la volée, rien à invalider.
fn touch_mask_owner(document: &mut Document, owner: Uuid) {
    if let Some(photo_engine::LayerNode::Pixel(pixels)) = document.find_mut(owner) {
        pixels.touch();
    } else if let Some(parent) = document.find_filter_parent(owner)
        && let Some(photo_engine::LayerNode::Pixel(pixels)) = document.find_mut(parent)
    {
        pixels.touch();
    }
}

/// État du worker : document vivant + historique undo/redo.
pub struct EngineWorker {
    document: Document,
    undo: Vec<photo_engine::history::Snapshot>,
    redo: Vec<photo_engine::history::Snapshot>,
    /// Calque du dernier `SetOpacity` (coalescence des sliders).
    coalesced_opacity_layer: Option<Uuid>,
}

/// Bornes de l'historique (snapshots partagés, quasi gratuits).
pub const HISTORY_LIMIT: usize = 100;

impl EngineWorker {
    /// Crée un worker sur un document existant.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            coalesced_opacity_layer: None,
        }
    }

    /// Empile l'état pré-mutation (jamais après) et vide le redo.
    /// Les gestes continus (sliders) sont coalescés par calque.
    fn push_history(&mut self, coalesce_layer: Option<Uuid>) {
        if coalesce_layer.is_some() && coalesce_layer == self.coalesced_opacity_layer {
            return;
        }
        if self.undo.len() >= HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(self.document.snapshot());
        self.redo.clear();
        self.coalesced_opacity_layer = coalesce_layer;
    }

    /// Construit la réponse standard (snapshot + aperçu + historique).
    fn response(&self) -> PhotoEngineResponse {
        PhotoEngineResponse::LayersChanged {
            layers: snapshot_layers(&self.document),
            preview: render_preview(&self.document),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }

    /// Applique une commande. Ne panique jamais : commande invalide
    /// (id inconnu, index hors limites) = no-op (avec réponse fraîche
    /// tout de même, pour resynchroniser l'UI).
    pub fn apply(&mut self, command: PhotoEngineCommand) -> PhotoEngineResponse {
        match command {
            PhotoEngineCommand::ToggleLayerVisibility(id) => {
                if self.document.find_mut(id).is_none() {
                    return self.response();
                }
                self.push_history(None);
                if let Some(node) = self.document.find_mut(id) {
                    let visible = node.visible();
                    node.set_visible(!visible);
                }
                self.response()
            }
            PhotoEngineCommand::ReorderLayer { from, to } => {
                // Indices d'affichage (0 = haut de pile) vers `root`
                // (index 0 = bas de pile) : `to == len` = bas de pile.
                let len = self.document.root.len();
                if len == 0 || from >= len {
                    return self.response();
                }
                let to = to.min(len);
                if from == to {
                    return self.response();
                }
                self.push_history(None);
                let node = self.document.root.remove(len - 1 - from);
                let remaining = self.document.root.len();
                let insert_at = remaining.saturating_sub(to).min(remaining);
                self.document.root.insert(insert_at, node);
                self.response()
            }
            PhotoEngineCommand::SetOpacity { layer, opacity } => {
                if self.document.find(layer).is_none() {
                    return self.response();
                }
                // Coalescence : un seul snapshot par geste de slider.
                self.push_history(Some(layer));
                if let Some(node) = self.document.find_mut(layer) {
                    node.set_opacity(opacity);
                }
                self.response()
            }
            PhotoEngineCommand::SetBlendMode { layer, mode } => {
                if self.document.find(layer).is_none() {
                    return self.response();
                }
                // Choix discret : un snapshot par changement.
                self.push_history(None);
                if let Some(node) = self.document.find_mut(layer) {
                    node.set_blend_mode(mode);
                }
                self.response()
            }
            PhotoEngineCommand::Undo => {
                let Some(snapshot) = self.undo.pop() else {
                    return self.response();
                };
                self.redo.push(self.document.snapshot());
                self.document.restore_snapshot(snapshot);
                self.coalesced_opacity_layer = None;
                self.response()
            }
            PhotoEngineCommand::Redo => {
                let Some(snapshot) = self.redo.pop() else {
                    return self.response();
                };
                self.undo.push(self.document.snapshot());
                self.document.restore_snapshot(snapshot);
                self.coalesced_opacity_layer = None;
                self.response()
            }
            PhotoEngineCommand::PaintStroke {
                layer,
                points,
                eraser,
                radius,
                color,
                opacity,
            } => {
                // Garde-fous : rayon non fini (NaN/infini) ou nul → no-op.
                if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
                    return self.response();
                }
                let is_pixel = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                );
                if !is_pixel {
                    return self.response();
                }
                self.push_history(None);
                let Some(photo_engine::LayerNode::Pixel(pixels)) = self.document.find_mut(layer)
                else {
                    return self.response();
                };
                let source = pixels.source_image.clone();
                let raster = source.to_rgba8();
                let (width, height) = (raster.width(), raster.height());
                let mut buffer = raster.into_raw();
                let brush = photo_engine::paint::BrushParams {
                    radius,
                    color,
                    opacity: opacity.clamp(0.0, 1.0),
                    mode: if eraser {
                        photo_engine::paint::StrokeMode::Erase
                    } else {
                        photo_engine::paint::StrokeMode::Paint
                    },
                };
                photo_engine::paint::paint_stroke_rgba(&mut buffer, width, height, &points, &brush);
                if let Some(image) = image::RgbaImage::from_raw(width, height, buffer) {
                    // Re-lecture défensive après le push d'historique.
                    if let Some(photo_engine::LayerNode::Pixel(pixels)) =
                        self.document.find_mut(layer)
                    {
                        pixels.set_source_image(image::DynamicImage::ImageRgba8(image));
                        pixels.touch();
                    }
                }
                self.response()
            }
            PhotoEngineCommand::OpenImage { path } => match image::open(&path) {
                Ok(image) => {
                    self.push_history(None);
                    let name = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("image")
                        .to_owned();
                    self.document.width = self.document.width.max(image.width());
                    self.document.height = self.document.height.max(image.height());
                    self.document.push_layer(photo_engine::LayerNode::Pixel(
                        photo_engine::PixelLayer::new(name, std::sync::Arc::new(image)),
                    ));
                    self.response()
                }
                Err(error) => PhotoEngineResponse::EngineError {
                    message: format!("Ouverture impossible : {}", error),
                },
            },
            PhotoEngineCommand::AddEmptyLayer => {
                self.push_history(None);
                let (width, height) = (self.document.width.max(1), self.document.height.max(1));
                let blank = image::DynamicImage::new_rgba8(width, height);
                let name = format!("Calque {}", self.document.root.len() + 1);
                self.document.push_layer(photo_engine::LayerNode::Pixel(
                    photo_engine::PixelLayer::new(name, std::sync::Arc::new(blank)),
                ));
                self.response()
            }
            PhotoEngineCommand::DuplicateLayer(id) => {
                if self.document.find(id).is_none() {
                    return self.response();
                }
                self.push_history(None);
                self.document.duplicate(id);
                self.response()
            }
            PhotoEngineCommand::DeleteLayer(id) => {
                // Jamais de document vide : le dernier calque est conservé.
                if self.document.root.len() <= 1 || self.document.find(id).is_none() {
                    return self.response();
                }
                self.push_history(None);
                self.document.remove(id);
                self.response()
            }
            PhotoEngineCommand::AddFilter { layer, filter_type } => {
                let Some(filter) = photo_engine::new_filter_layer(&filter_type) else {
                    return PhotoEngineResponse::EngineError {
                        message: format!("Filtre inconnu : {filter_type}"),
                    };
                };
                let is_pixel = matches!(
                    self.document.find(layer),
                    Some(photo_engine::LayerNode::Pixel(_))
                );
                if !is_pixel {
                    return self.response();
                }
                self.push_history(None);
                // Re-lecture après le push d'historique (emprunt neuf).
                if let Some(photo_engine::LayerNode::Pixel(pixels)) = self.document.find_mut(layer)
                {
                    pixels.filter_layers.push(filter);
                    pixels.touch();
                }
                self.response()
            }
            PhotoEngineCommand::Export { path, quality } => match self.document.composite_preview()
            {
                Some(image) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase);
                    if ext.as_deref() == Some("gif") {
                        match image.save(&path) {
                            Ok(()) => PhotoEngineResponse::ExportDone { path },
                            Err(error) => PhotoEngineResponse::EngineError {
                                message: format!("Export impossible : {error}"),
                            },
                        }
                    } else {
                        let format = match ext.as_deref() {
                            Some("jpg") | Some("jpeg") => photo_engine::ExportFormat::Jpeg {
                                quality: quality.clamp(1, 100),
                            },
                            _ => photo_engine::ExportFormat::from_path(&path),
                        };
                        match photo_engine::export_image(&image, &path, format) {
                            Ok(()) => PhotoEngineResponse::ExportDone { path },
                            Err(error) => PhotoEngineResponse::EngineError {
                                message: format!("Export impossible : {error}"),
                            },
                        }
                    }
                }
                None => PhotoEngineResponse::EngineError {
                    message: String::from("Rien a exporter (document vide)"),
                },
            },
            PhotoEngineCommand::RenameLayer { layer, name } => {
                let name = name.trim().to_owned();
                if name.is_empty() || self.document.find(layer).is_none() {
                    return self.response();
                }
                self.push_history(None);
                self.document.set_name_any(layer, name);
                self.response()
            }
            PhotoEngineCommand::AddMask { layer } => {
                // Validation sans emprunt persistant (le push d'historique
                // emprunte `self` en mutable : pas de borrow maintenu).
                // `masks_of_mut` (et non `masks_of`) : les ajustements
                // exposent une tranche vide mais refusent l'insertion.
                if self.document.masks_of_mut(layer).is_none() {
                    return self.response();
                }
                self.push_history(None);
                let (width, height) = (self.document.width.max(1), self.document.height.max(1));
                if let Some(masks) = self.document.masks_of_mut(layer) {
                    let mut mask = photo_engine::LayerMask::full(width, height);
                    mask.name = format!("Masque {}", masks.len() + 1);
                    masks.push(mask);
                }
                touch_mask_owner(&mut self.document, layer);
                self.response()
            }
            PhotoEngineCommand::RemoveMask { owner, mask } => {
                let present = self
                    .document
                    .masks_of(owner)
                    .is_some_and(|masks| masks.iter().any(|m| m.id == mask));
                if !present {
                    return self.response();
                }
                self.push_history(None);
                if let Some(masks) = self.document.masks_of_mut(owner) {
                    masks.retain(|m| m.id != mask);
                }
                touch_mask_owner(&mut self.document, owner);
                self.response()
            }
            PhotoEngineCommand::MoveFilter { layer, filter, up } => {
                // `move_filter` mute : valider d'abord via une lecture
                // immuable (position + bornes), puis historiser, puis muter.
                let movable = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        match pixels.filter_layers.iter().position(|f| f.id == filter) {
                            Some(idx) => {
                                let other = if up {
                                    idx.checked_add(1)
                                } else {
                                    idx.checked_sub(1)
                                };
                                other.is_some_and(|o| o < pixels.filter_layers.len())
                            }
                            None => false,
                        }
                    }
                    _ => false,
                };
                if !movable {
                    return self.response();
                }
                self.push_history(None);
                self.document.move_filter(layer, filter, up);
                self.response()
            }
            PhotoEngineCommand::MoveMask { owner, mask, up } => {
                let movable = match self.document.masks_of(owner) {
                    Some(masks) => match masks.iter().position(|m| m.id == mask) {
                        Some(idx) => {
                            let other = if up {
                                idx.checked_sub(1)
                            } else {
                                idx.checked_add(1)
                            };
                            other.is_some_and(|o| o < masks.len())
                        }
                        None => false,
                    },
                    None => false,
                };
                if !movable {
                    return self.response();
                }
                self.push_history(None);
                self.document.move_mask(owner, mask, up);
                touch_mask_owner(&mut self.document, owner);
                self.response()
            }
            PhotoEngineCommand::RemoveFilter { layer, filter } => {
                let present = match self.document.find(layer) {
                    Some(photo_engine::LayerNode::Pixel(pixels)) => {
                        pixels.filter_layers.iter().any(|f| f.id == filter)
                    }
                    _ => false,
                };
                if !present {
                    return self.response();
                }
                self.push_history(None);
                self.document.remove_filter(layer, filter);
                self.response()
            }
            PhotoEngineCommand::Refresh => self.response(),
        }
    }
}

/// Applique une commande à un document éphémère (tests, sans
/// worker persistant ni historique).
pub fn apply_command(document: &mut Document, command: PhotoEngineCommand) {
    let mut worker = EngineWorker::new(Document::new(0, 0));
    std::mem::swap(&mut worker.document, document);
    let _ = worker.apply(command);
    std::mem::swap(&mut worker.document, document);
}

/// Calcule l'aperçu composite (plafonné à `PREVIEW_MAX_DIMENSION`).
/// Coûteux : à appeler uniquement sur thread background.
pub fn render_preview(document: &Document) -> Option<PreviewImage> {
    let composite = document.composite_preview()?;
    let preview = composite.thumbnail(PREVIEW_MAX_DIMENSION, PREVIEW_MAX_DIMENSION);
    let raster = preview.to_rgba8();
    Some(PreviewImage {
        width: raster.width(),
        height: raster.height(),
        rgba: raster.into_raw(),
    })
}

/// Démarre le worker moteur sur un thread background.
///
/// Boucle bloquante `recv` (autorisée hors thread UI) : applique
/// chaque commande puis répond. La fin du `Sender` côté UI arrête
/// proprement le thread.
pub fn spawn_photo_engine_worker(
    document: Document,
    commands: Receiver<PhotoEngineCommand>,
    responses: Sender<PhotoEngineResponse>,
) -> JoinHandle<()> {
    assert_send::<Document>();
    std::thread::spawn(move || {
        let mut worker = EngineWorker::new(document);
        while let Ok(command) = commands.recv() {
            let response = worker.apply(command);
            if responses.send(response).is_err() {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::{LayerNode, PixelLayer};
    use std::sync::Arc;
    use std::sync::mpsc::channel;

    fn test_image() -> Arc<image::DynamicImage> {
        Arc::new(image::DynamicImage::new_rgba8(4, 4))
    }

    fn three_layer_doc() -> Document {
        let mut doc = Document::new(8, 8);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("milieu", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("dessus", test_image())));
        doc
    }

    #[test]
    fn display_index_conversion() {
        assert_eq!(display_to_doc_index(0, 3), Some(2));
        assert_eq!(display_to_doc_index(2, 3), Some(0));
        assert_eq!(display_to_doc_index(3, 3), None);
        assert_eq!(display_to_doc_index(0, 0), None);
    }

    #[test]
    fn toggle_visibility_command() {
        let mut doc = three_layer_doc();
        let id = doc.root[0].id();
        assert!(doc.find(id).expect("present").visible());
        apply_command(&mut doc, PhotoEngineCommand::ToggleLayerVisibility(id));
        assert!(!doc.find(id).expect("present").visible());
        apply_command(&mut doc, PhotoEngineCommand::ToggleLayerVisibility(id));
        assert!(doc.find(id).expect("present").visible());
    }

    #[test]
    fn reorder_command_moves_display_top_to_bottom() {
        // Affichage [dessus, milieu, fond] ; (0 -> 3) = "dessus" en bas.
        let mut doc = three_layer_doc();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 0, to: 3 },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);
    }

    #[test]
    fn reorder_command_bottom_to_top() {
        // (2 -> 0) = "fond" tout en haut.
        let mut doc = three_layer_doc();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 2, to: 0 },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["fond", "dessus", "milieu"]);
    }

    #[test]
    fn invalid_commands_are_noops() {
        let mut doc = three_layer_doc();
        apply_command(
            &mut doc,
            PhotoEngineCommand::ToggleLayerVisibility(Uuid::new_v4()),
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 9, to: 0 },
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::ReorderLayer { from: 1, to: 1 },
        );
        let names: Vec<String> = snapshot_layers(&doc)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["dessus", "milieu", "fond"]);
        assert!(snapshot_layers(&doc).iter().all(|l| l.visible));
    }

    #[test]
    fn set_blend_mode_snapshots_and_undoes() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::SetBlendMode {
            layer: id,
            mode: BlendMode::Multiply,
        });
        assert_eq!(
            worker.document.find(id).expect("present").blend_mode(),
            Some(BlendMode::Multiply)
        );
        assert_eq!(worker.undo.len(), 1);
        // Id inconnu : sans effet, sans snapshot.
        worker.apply(PhotoEngineCommand::SetBlendMode {
            layer: Uuid::new_v4(),
            mode: BlendMode::Screen,
        });
        assert_eq!(worker.undo.len(), 1);
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(
            worker.document.find(id).expect("present").blend_mode(),
            Some(BlendMode::Normal)
        );
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        assert_eq!(
            layers
                .iter()
                .find(|l| l.id == id)
                .expect("present")
                .blend_mode,
            BlendMode::Multiply
        );
    }

    #[test]
    fn set_opacity_clamps_and_snapshots() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        let response = worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 250.0,
        });
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        // Un seul snapshot malgré 3 appels (coalescence du slider).
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 10.0,
        });
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: 20.0,
        });
        assert_eq!(worker.undo.len(), 1);
        // Undo restaure l'opacité d'origine (100 par défaut moteur).
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.find(id).expect("present").opacity(), 100.0);
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        assert_eq!(
            layers.iter().find(|l| l.id == id).expect("present").opacity,
            100.0
        );
    }

    #[test]
    fn undo_redo_cycle() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::Undo);
        assert!(matches!(
            response,
            PhotoEngineResponse::LayersChanged {
                can_undo: false,
                can_redo: false,
                ..
            }
        ));
        worker.apply(PhotoEngineCommand::ReorderLayer { from: 0, to: 3 });
        worker.apply(PhotoEngineCommand::Undo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["dessus", "milieu", "fond"]);
        worker.apply(PhotoEngineCommand::Redo);
        let names: Vec<String> = snapshot_layers(&worker.document)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);
    }

    #[test]
    fn open_missing_image_is_engine_error() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::OpenImage {
            path: PathBuf::from("/chemin/inexistant/photo.png"),
        });
        assert!(matches!(response, PhotoEngineResponse::EngineError { .. }));
        assert_eq!(worker.document.root.len(), 3);
    }

    #[test]
    fn preview_present_after_command() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let response = worker.apply(PhotoEngineCommand::ToggleLayerVisibility(
            worker.document.root[0].id(),
        ));
        let PhotoEngineResponse::LayersChanged { preview, .. } = response else {
            panic!("réponse attendue");
        };
        let preview = preview.expect("apercu genere");
        assert_eq!(
            preview.rgba.len(),
            preview.width as usize * preview.height as usize * 4
        );
    }

    #[test]
    fn worker_roundtrip_over_channels() {
        let doc = three_layer_doc();
        let (cmd_tx, cmd_rx) = channel();
        let (resp_tx, resp_rx) = channel();
        let handle = spawn_photo_engine_worker(doc, cmd_rx, resp_tx);

        // Le thread UI ne bloque jamais : poll immédiat vide…
        assert!(resp_rx.try_recv().is_err());
        cmd_tx
            .send(PhotoEngineCommand::ReorderLayer { from: 0, to: 3 })
            .expect("envoi commande");
        // …puis réponse du worker (opération lourde hors UI).
        let response = resp_rx.recv().expect("reponse worker");
        let PhotoEngineResponse::LayersChanged { layers, .. } = response else {
            panic!("réponse attendue");
        };
        let names: Vec<String> = layers.iter().map(|l| l.name.clone()).collect();
        assert_eq!(names, ["milieu", "fond", "dessus"]);

        drop(cmd_tx);
        handle.join().expect("arret propre du worker");
    }

    #[test]
    fn rename_trims_and_rejects_empty_or_unknown() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("  fond clair  "),
        });
        assert_eq!(
            worker.document.find(id).expect("present").name(),
            "fond clair"
        );
        assert_eq!(worker.undo.len(), 1);
        // Vide ou inconnu : no-op sans snapshot.
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: id,
            name: String::from("   "),
        });
        worker.apply(PhotoEngineCommand::RenameLayer {
            layer: Uuid::new_v4(),
            name: String::from("fantome"),
        });
        assert_eq!(
            worker.document.find(id).expect("present").name(),
            "fond clair"
        );
        assert_eq!(worker.undo.len(), 1);
        // Undo restaure le nom d'origine.
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.find(id).expect("present").name(), "fond");
    }

    #[test]
    fn mask_add_remove_roundtrip_with_undo() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        worker.apply(PhotoEngineCommand::AddMask { layer: id });
        let masks = worker.document.masks_of(id).expect("masques");
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0].name, "Masque 1");
        let mask_id = masks[0].id;
        // Snapshot UI : le masque apparaît dans la HUD.
        let layers = snapshot_layers(&worker.document);
        let info = layers.iter().find(|l| l.id == id).expect("present");
        assert!(info.has_masks);
        assert_eq!(info.masks.len(), 1);
        // Bornes : déplacer un masque seul = no-op.
        worker.apply(PhotoEngineCommand::MoveMask {
            owner: id,
            mask: mask_id,
            up: true,
        });
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
        // Suppression puis undo.
        worker.apply(PhotoEngineCommand::RemoveMask {
            owner: id,
            mask: mask_id,
        });
        assert!(worker.document.masks_of(id).expect("masques").is_empty());
        worker.apply(PhotoEngineCommand::Undo);
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
        // Porteur inconnu : no-op.
        worker.apply(PhotoEngineCommand::AddMask {
            layer: Uuid::new_v4(),
        });
        assert_eq!(worker.document.masks_of(id).expect("masques").len(), 1);
    }

    #[test]
    fn filter_move_and_remove() {
        let mut worker = EngineWorker::new(three_layer_doc());
        let id = worker.document.root[0].id();
        for type_id in ["brightness_contrast", "blur"] {
            let filter = photo_engine::new_filter_layer(type_id).expect("filtre connu");
            worker.document.add_filter(id, filter);
        }
        // Ordre d'application : [brightness, blur] ; monter le premier.
        let first = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels.filter_layers[0].id,
            _ => panic!("pixels attendus"),
        };
        worker.apply(PhotoEngineCommand::MoveFilter {
            layer: id,
            filter: first,
            up: true,
        });
        let order: Vec<String> = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels
                .filter_layers
                .iter()
                .map(|f| f.type_id.clone())
                .collect(),
            _ => panic!("pixels attendus"),
        };
        assert_eq!(order, ["blur", "brightness_contrast"]);
        // Borne haute : no-op.
        worker.apply(PhotoEngineCommand::MoveFilter {
            layer: id,
            filter: first,
            up: true,
        });
        // Suppression.
        worker.apply(PhotoEngineCommand::RemoveFilter {
            layer: id,
            filter: first,
        });
        let remaining = match worker.document.find(id).expect("present") {
            LayerNode::Pixel(pixels) => pixels.filter_layers.len(),
            _ => panic!("pixels attendus"),
        };
        assert_eq!(remaining, 1);
    }

    #[test]
    fn export_png_and_gif_write_files() {
        let dir = std::env::temp_dir().join(format!("cygnus-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let mut worker = EngineWorker::new(three_layer_doc());
        for ext in ["png", "jpg", "gif"] {
            let path = dir.join(format!("rendu.{ext}"));
            let response = worker.apply(PhotoEngineCommand::Export {
                path: path.clone(),
                quality: 80,
            });
            assert!(
                matches!(response, PhotoEngineResponse::ExportDone { .. }),
                "export {ext} attendu"
            );
            assert!(path.is_file(), "fichier {ext} écrit");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
