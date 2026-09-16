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

//! Vue centrale : onglets de documents + canvas actif.
//!
//! Le canvas (`PhotoCanvas`, viewport générique + outils photo) ne
//! renvoie que des données : le commit d'un trait remonte en
//! [`PhotoAction::CommitStroke`](crate::commands::PhotoAction), la
//! pipette échantillonne en état UI local. Overlays photo
//! (sélection, origine) par-dessus.

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::sample_preview_color;
use crate::ui::{PhotoCanvas, PhotoCanvasTool, draw_origin_marker, draw_selection_chip};
use ui_kit::widgets::icon::{CygnusIcon, icon_button};

/// Onglets + canvas du document actif.
pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    egui::CentralPanel::default().show(ui, |ui| {
        actions.extend(draw_doc_tabs(ui, app));
        actions.extend(draw_active_canvas(ui, app, ctx));
    });
    actions
}

/// Onglets de documents + boutons nouveau/fermer.
fn draw_doc_tabs(ui: &mut egui::Ui, app: &PhotoApp) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        for index in 0..app.doc_count() {
            let title = app.docs[index].title.clone();
            let selected = index == app.active;
            if ui.selectable_label(selected, title).clicked() {
                actions.push(PhotoAction::SwitchTab(index));
            }
        }
        if icon_button(ui, CygnusIcon::Add, Some("Nouveau document")).clicked() {
            actions.push(PhotoAction::OpenNewDocumentDialog);
        }
        if icon_button(ui, CygnusIcon::Close, Some("Fermer le document")).clicked() {
            actions.push(PhotoAction::CloseTab);
        }
    });
    ui.separator();
    actions
}

/// Canvas du document actif + overlays + pipette.
fn draw_active_canvas(
    ui: &mut egui::Ui,
    app: &mut PhotoApp,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let doc = app.active_doc_mut();
    let texture = doc.ui.texture_cache.texture();
    let outcome = PhotoCanvas::new(&mut doc.ui.stroke)
        .texture(texture)
        .tool(doc.ui.tool)
        .show_grid(doc.ui.show_grid)
        .active_layer(doc.ui.selected)
        .brush(doc.ui.brush)
        .show(ui, &mut doc.ui.viewport);
    // Commit du trait : routé au worker par l'app.
    if let Some(paint) = outcome.paint {
        actions.push(PhotoAction::CommitStroke(paint));
    }
    // Pipette : échantillonne le composite sous le curseur (local).
    if doc.ui.tool == PhotoCanvasTool::Eyedropper
        && let (Some(world), Some(preview)) =
            (outcome.pointer_world.last(), doc.ui.last_preview.as_ref())
        && let Some(color) = sample_preview_color(preview, world.x, world.y)
    {
        doc.ui.brush.color = color;
    }
    // Overlays : étiquette de sélection + origine monde.
    let selected_name = doc
        .ui
        .selected
        .and_then(|id| doc.ui.layers.iter().find(|layer| layer.id == id))
        .map(|layer| layer.name.clone());
    draw_selection_chip(ui, ctx, selected_name.as_deref());
    draw_origin_marker(ui.painter(), ctx, &doc.ui.viewport);
    actions
}
