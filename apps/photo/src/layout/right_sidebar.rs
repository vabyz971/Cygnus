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

//! Contenus des onglets dock « Inspecteur » et « Calques ».
//!
//! ```text
//! PhotoDockTab::Inspector → InspectorPanel
//! PhotoDockTab::Layers    → LayersPanel
//! ```
//!
//! Position-indépendants : visibilité et placement via le dock
//! (fermeture + menu Fenêtre), plus via le `WorkspaceState`.

use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::OpenDocument;
use crate::ui::{
    InspectorPanel, LayersPanel, inspector_action_to_photo, layer_panel_action_to_photo,
};

/// Inspecteur du calque sélectionné (contenu de l'onglet).
pub fn draw_inspector_content(
    ui: &mut egui::Ui,
    doc: &OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let selected = doc
        .ui
        .selected
        .and_then(|id| doc.ui.layers.iter().find(|layer| layer.id == id));
    InspectorPanel::show(ui, ctx, selected)
        .into_iter()
        .map(inspector_action_to_photo)
        .collect()
}

/// Liste des calques (contenu de l'onglet).
pub fn draw_layers_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    // Annule un renommage orphelin avant affichage.
    if doc
        .ui
        .rename
        .editing
        .is_some_and(|id| !doc.ui.layers.iter().any(|layer| layer.id == id))
    {
        doc.ui.rename.editing = None;
    }
    LayersPanel::show(
        ui,
        ctx,
        &doc.ui.layers,
        doc.ui.selected,
        &mut doc.ui.rename,
        &mut doc.ui.drag_state,
    )
    .into_iter()
    .map(layer_panel_action_to_photo)
    .collect()
}
