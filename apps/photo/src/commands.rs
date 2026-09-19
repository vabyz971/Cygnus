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

//! Commandes UI de l'app Photo : le seul canal panels → app.
//!
//! Flux imposé :
//!
//! ```text
//! UI (panels, viewport, menus)
//!   ↓ PhotoAction
//! PhotoApp::handle_action
//!   ↓ PhotoEngineCommand
//! Engine (worker)
//! ```
//!
//! Les panels ne touchent JAMAIS au worker : ils poussent des
//! [`PhotoAction`] dans la [`PhotoCommandQueue`] (ou les retournent
//! en `Vec`, drainés par le workspace). [`PhotoUiContext`] porte les
//! dépendances d'affichage partagées (thème, icônes, traduction) —
//! ni documents, ni moteurs.

use crate::layout::dock::PhotoDockTab;
use crate::ui::{PaintRequest, PhotoCanvasTool};
use photo_engine::BlendMode;
use std::path::PathBuf;
use ui_kit::context::UiContext;
use ui_kit::i18n::{Catalog, Language};
use ui_kit::icons::IconRegistry;
use ui_kit::theme::CygnusTheme;
use uuid::Uuid;

/// Action UI émise par un panel, le viewport ou un menu.
///
/// Les ids de calques sont résolus à l'émission (le panel connaît la
/// sélection) ; `PhotoApp` n'a plus qu'à router vers l'état local ou
/// le worker.
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoAction {
    /// Ouvrir la fenêtre « Nouveau document ».
    OpenNewDocumentDialog,
    /// Créer un document aux dimensions validées (nouvel onglet).
    CreateDocument { width: u32, height: u32 },
    /// Ouvrir le file picker d'image.
    OpenImageDialog,
    /// Ouvrir la fenêtre « Exportation ».
    OpenExportDialog,
    /// Exporter le document actif (dossier créé si besoin par l'app).
    ExportDocument { path: PathBuf, quality: u8 },
    /// Fermer l'onglet actif.
    CloseTab,
    /// Annuler / rétablir (worker).
    Undo,
    /// Rétablir (worker).
    Redo,
    /// Nouveau calque vide (worker).
    AddEmptyLayer,
    /// Calque depuis une image (file picker).
    AddImageLayer,
    /// Dupliquer le calque sélectionné (worker, sans effet si aucun).
    DuplicateSelectedLayer,
    /// Supprimer le calque sélectionné (worker, sans effet si aucun).
    DeleteSelectedLayer,
    /// Supprimer le calque `layer` (worker, depuis sa rangée).
    DeleteLayer(Uuid),
    /// Dupliquer le calque `layer` (worker, menu contextuel).
    DuplicateLayer(Uuid),
    /// Ajouter un masque au calque sélectionné (worker, sans effet si aucun).
    AddMaskToSelected,
    /// Ouvrir la modale d'ajout de filtre.
    OpenFilterMenu,
    /// Ajouter un filtre live (worker).
    AddFilter { layer: Uuid, filter_type: String },
    /// Sélectionner un calque (état UI local).
    SelectLayer(Uuid),
    /// Renommer un calque (worker).
    RenameLayer { layer: Uuid, name: String },
    /// Réordonner (indices d'affichage, worker).
    ReorderLayers { from: usize, to: usize },
    /// Basculer la visibilité (worker).
    ToggleLayerVisibility(Uuid),
    /// Régler l'opacité (unités moteur 0..=100, worker).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Régler le mode de fusion (worker).
    SetBlendMode { layer: Uuid, mode: BlendMode },
    /// Déplacer un filtre dans sa pile (worker).
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Déplacer un masque dans sa pile (worker).
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Supprimer un filtre (worker).
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Supprimer un masque (worker).
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Changer d'outil (état UI local).
    SetTool(PhotoCanvasTool),
    /// Basculer la grille (état UI local).
    ToggleGrid,
    /// Rogner l'aperçu au document (worker, sans historique).
    TogglePreviewClip,
    /// Zoom avant / arrière, ancré au centre.
    ZoomIn,
    /// Zoom arrière.
    ZoomOut,
    /// Réinitialiser le zoom à 100 %.
    ZoomReset,
    /// Commettre un trait pinceau/gomme (worker).
    CommitStroke(PaintRequest),
    /// Déplacer un calque pixels (outil sélection, worker).
    MoveLayer { layer: Uuid, dx: f32, dy: f32 },
    /// Rouvrir un onglet dock fermé (outils, inspecteur, calques).
    ShowDockTab(PhotoDockTab),
    /// Restaurer la disposition des docks par défaut.
    ResetDockLayout,
    /// Ouvrir la fenêtre d'aide.
    ShowHelp,
    /// Quitter l'application.
    Quit,
}

/// File d'actions UI drainée une fois par frame par `PhotoApp`.
#[derive(Debug, Default)]
pub struct PhotoCommandQueue {
    actions: Vec<PhotoAction>,
}

// `push` / `len` / `is_empty` : API du bus couverte par tests,
// câblage progressif par le workspace (qui utilise `extend`).
#[allow(dead_code)]
impl PhotoCommandQueue {
    /// File vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute une action en fin de file.
    pub fn push(&mut self, action: PhotoAction) {
        self.actions.push(action);
    }

    /// Ajoute plusieurs actions en fin de file.
    pub fn extend(&mut self, actions: impl IntoIterator<Item = PhotoAction>) {
        self.actions.extend(actions);
    }

    /// Nombre d'actions en attente.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Vrai si aucune action en attente.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Vide la file et retourne les actions (ordre d'émission).
    pub fn drain(&mut self) -> Vec<PhotoAction> {
        std::mem::take(&mut self.actions)
    }
}

/// Dépendances d'affichage partagées par les features.
///
/// Construit une fois par frame ([`PhotoUiContext::for_frame`]) :
/// contexte egui cloné, thème, registre d'icônes, traducteur. Ni
/// documents, ni moteurs, ni état mutable — pas de GodContext.
#[derive(Debug, Clone)]
pub struct PhotoUiContext {
    /// Dépendances communes ui-kit.
    pub shared: UiContext,
}

impl PhotoUiContext {
    /// Contexte standard pour la frame (thème sombre, icônes,
    /// catalogue français : langue de l'app jusqu'au câblage de la
    /// préférence de langue).
    pub fn for_frame(ctx: &egui::Context) -> Self {
        Self {
            shared: UiContext::new(
                ctx.clone(),
                CygnusTheme::dark(),
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
