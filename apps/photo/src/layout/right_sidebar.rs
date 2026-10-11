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

//! Dock tab contents: "Inspector", "Layers" and
//! "History".
//!
//! ```text
//! PhotoDockTab::Inspector → InspectorPanel
//! PhotoDockTab::Layers    → LayersPanel
//! PhotoDockTab::History   → HistoryPanel
//! ```
//!
//! Position-independent: visibility and placement via the dock
//! (close + Window menu), plus via the `WorkspaceState`.

use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::OpenDocument;
use crate::ui::{
    HistoryPanel, InspectorPanel, LayersPanel, history_panel_action_to_photo,
    inspector_action_to_photo, layer_panel_action_to_photo,
};

/// Inspector of the selected layer (tab content).
pub fn draw_inspector_content(
    ui: &mut egui::Ui,
    doc: &OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let selected = doc
        .ui
        .selected
        .and_then(|id| doc.ui.layers.iter().find(|layer| layer.id == id));
    InspectorPanel::show(ui, ctx, selected, doc.ui.focused_attachment)
        .into_iter()
        .map(inspector_action_to_photo)
        .collect()
}

/// Layer list (tab content).
pub fn draw_layers_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    ctx: &PhotoUiContext,
    filter_types: &[(String, String)],
) -> Vec<PhotoAction> {
    // Cancel an orphaned rename before display.
    if doc
        .ui
        .rename
        .editing
        .is_some_and(|id| !doc.ui.layers.iter().any(|layer| layer.id == id))
    {
        doc.ui.rename.editing = None;
    }
    let thumbs = doc.ui.thumb_views();
    let sel = crate::ui::features::layers::LayerTreeSelection {
        selected: doc.ui.selected,
        focused: doc.ui.focused_attachment,
    };
    LayersPanel::show(
        ui,
        ctx,
        &doc.ui.layers,
        sel,
        &mut doc.ui.rename,
        &mut doc.ui.drag_state,
        &thumbs,
        filter_types,
    )
    .into_iter()
    .map(layer_panel_action_to_photo)
    .collect()
}

/// Undo/redo step history (tab content).
pub fn draw_history_content(
    ui: &mut egui::Ui,
    doc: &OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    HistoryPanel::show(ui, ctx, &doc.ui.history_undo, &doc.ui.history_redo)
        .into_iter()
        .map(history_panel_action_to_photo)
        .collect()
}
