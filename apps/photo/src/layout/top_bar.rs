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

//! Barre haute : menus + modes/outils (toute la largeur).
//!
//! Contenus délégués aux widgets métier (`menubar`, `modebar`) ; ici
//! on ne fait que placer les `egui::Panel` et convertir les actions
//! widget en [`PhotoAction`](crate::commands::PhotoAction).

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::ui::{
    MenuAvailability, PhotoCanvasTool, PhotoMenuAction, draw_menu_bar, draw_photo_modebar,
};

/// Convertit une action de menu en action app.
fn menu_action_to_photo(action: PhotoMenuAction) -> PhotoAction {
    match action {
        PhotoMenuAction::NewDocument => PhotoAction::OpenNewDocumentDialog,
        PhotoMenuAction::OpenImage => PhotoAction::OpenImageDialog,
        PhotoMenuAction::Export => PhotoAction::OpenExportDialog,
        PhotoMenuAction::Quit => PhotoAction::Quit,
        PhotoMenuAction::Undo => PhotoAction::Undo,
        PhotoMenuAction::Redo => PhotoAction::Redo,
        PhotoMenuAction::AddEmptyLayer => PhotoAction::AddEmptyLayer,
        PhotoMenuAction::DuplicateLayer => PhotoAction::DuplicateSelectedLayer,
        PhotoMenuAction::AddMask => PhotoAction::AddMaskToSelected,
        PhotoMenuAction::DeleteLayer => PhotoAction::DeleteSelectedLayer,
        PhotoMenuAction::ToggleGrid => PhotoAction::ToggleGrid,
        PhotoMenuAction::ZoomIn => PhotoAction::ZoomIn,
        PhotoMenuAction::ZoomOut => PhotoAction::ZoomOut,
        PhotoMenuAction::ZoomReset => PhotoAction::ZoomReset,
        PhotoMenuAction::ShowHelp => PhotoAction::ShowHelp,
    }
}

/// Rangée 1 : barre de menus (Fichier, Édition, Calque…).
pub fn show_menu_bar(
    ui: &mut egui::Ui,
    app: &mut PhotoApp,
    _ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    egui::Panel::top("photo_menubar")
        .resizable(false)
        .show(ui, |ui| {
            let availability = MenuAvailability {
                can_undo: app.active_doc().ui.can_undo,
                can_redo: app.active_doc().ui.can_redo,
                has_selection: app.active_doc().ui.selected.is_some(),
            };
            actions.extend(
                draw_menu_bar(ui, availability)
                    .into_iter()
                    .map(menu_action_to_photo),
            );
        });
    actions
}

/// Rangée 2 : modes d'édition + paramètres de l'outil.
pub fn show_mode_bar(
    ui: &mut egui::Ui,
    app: &mut PhotoApp,
    _ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    egui::Panel::top("photo_modebar")
        .resizable(false)
        .show(ui, |ui| {
            let doc = app.active_doc_mut();
            let chosen = draw_photo_modebar(
                ui,
                &mut doc.ui.edit_mode,
                doc.ui.tool,
                &mut doc.ui.brush,
                &mut doc.ui.show_grid,
            );
            // Outil incompatible avec le nouveau mode : rebascule.
            if let Some(mode) = chosen
                && !mode.supports(doc.ui.tool)
            {
                actions.push(PhotoAction::SetTool(PhotoCanvasTool::Move));
            }
        });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_action_converts() {
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::Undo),
            PhotoAction::Undo
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::DeleteLayer),
            PhotoAction::DeleteSelectedLayer
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::NewDocument),
            PhotoAction::OpenNewDocumentDialog
        );
    }
}
