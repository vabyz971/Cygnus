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

//! Studio droit : empilement des features, sans contenu en dur.
//!
//! ```text
//! RightSidebar
//!   ├── InspectorPanel
//!   └── LayersPanel
//! ```
//!
//! La sidebar gère le placement (`egui::Panel` droit, espacement) ;
//! les panels gèrent leur contenu. L'ordre et la visibilité suivent
//! le [`WorkspaceState`](ui_kit::layout::WorkspaceState) (région
//! droite) : masquer un panneau dans le layout le retire d'ici.

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::ui::{
    InspectorPanel, LayersPanel, inspector_action_to_photo, layer_panel_action_to_photo,
};
use ui_kit::layout::PanelId;

/// Colonne droite (inspecteur + calques, selon le workspace).
pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    // Visibilités lues AVANT l'emprunt mutable du document.
    let show_inspector = is_panel_visible(app, PanelId::Inspector);
    let show_layers = is_panel_visible(app, PanelId::Layers);
    let mut actions = Vec::new();
    egui::Panel::right("photo_studio")
        .resizable(true)
        .default_size(300.0)
        .min_size(220.0)
        .show(ui, |ui| {
            let doc = app.active_doc_mut();
            let selected = doc
                .ui
                .selected
                .and_then(|id| doc.ui.layers.iter().find(|layer| layer.id == id));
            if show_inspector {
                actions.extend(
                    InspectorPanel::show(ui, ctx, selected)
                        .into_iter()
                        .map(inspector_action_to_photo),
                );
                ui.separator();
            }
            if show_layers {
                // Annule un renommage orphelin avant affichage.
                if doc
                    .ui
                    .rename
                    .editing
                    .is_some_and(|id| !doc.ui.layers.iter().any(|layer| layer.id == id))
                {
                    doc.ui.rename.editing = None;
                }
                actions.extend(
                    LayersPanel::show(
                        ui,
                        ctx,
                        &doc.ui.layers,
                        doc.ui.selected,
                        &mut doc.ui.rename,
                        &mut doc.ui.drag_state,
                    )
                    .into_iter()
                    .map(layer_panel_action_to_photo),
                );
            }
        });
    actions
}

/// Visibilité d'un panneau dans la région droite (défaut : visible).
fn is_panel_visible(app: &PhotoApp, id: PanelId) -> bool {
    app.shell
        .workspace
        .find(id)
        .is_none_or(|panel| panel.visible)
}
