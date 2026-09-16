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

//! Modales et dialogs par-dessus le workspace.
//!
//! Les dialogs valident en données pures ; la validation remonte en
//! [`PhotoAction`](crate::commands::PhotoAction) (création de
//! document, export, ajout de filtre). L'app route ensuite.

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::ui::{draw_export_dialog, draw_help_dialog, draw_new_document_dialog};
use ui_kit::dialogs::{CygnusModal, ModalAction};
use ui_kit::widgets::dropdown::CygnusDropdown;

/// Toutes les modales (filtre, nouveau document, export, aide).
pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, _ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    actions.extend(draw_filter_modal(ui, app));
    actions.extend(draw_new_document_overlay(ui, app));
    actions.extend(draw_export_overlay(ui, app));
    draw_help_dialog(ui.ctx(), &mut app.shell.help_open);
    actions
}

/// Modale d'ajout de filtre (registre moteur statique).
fn draw_filter_modal(ui: &mut egui::Ui, app: &mut PhotoApp) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    if !app.shell.filter_modal.open {
        return actions;
    }
    let names: Vec<&str> = app
        .runtime
        .filter_types
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let mut open = true;
    let mut choice = app.shell.filter_modal.choice;
    let ctx = ui.ctx().clone();
    let action =
        CygnusModal::new("Ajouter un filtre", "Ajouter", "Annuler").show(&ctx, &mut open, |ui| {
            CygnusDropdown::new("Filtre", &names).show(ui, &mut choice);
        });
    app.shell.filter_modal.open = open;
    app.shell.filter_modal.choice = choice;
    if action == Some(ModalAction::Confirm)
        && let (Some(id), Some((_, type_id))) = (
            app.active_doc().ui.selected,
            app.runtime.filter_types.get(app.shell.filter_modal.choice),
        )
    {
        actions.push(PhotoAction::AddFilter {
            layer: id,
            filter_type: type_id.clone(),
        });
    }
    actions
}

/// Fenêtre « Nouveau document » : validation = nouvel onglet.
fn draw_new_document_overlay(ui: &mut egui::Ui, app: &mut PhotoApp) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let ctx = ui.ctx().clone();
    if let Some((width, height)) = draw_new_document_dialog(&ctx, &mut app.shell.new_doc_dialog) {
        actions.push(PhotoAction::CreateDocument { width, height });
    }
    actions
}

/// Fenêtre « Exportation » : validation = export du document actif.
fn draw_export_overlay(ui: &mut egui::Ui, app: &mut PhotoApp) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let ctx = ui.ctx().clone();
    if let Some(request) = draw_export_dialog(&ctx, &mut app.shell.export_dialog) {
        actions.push(PhotoAction::ExportDocument {
            path: request.path,
            quality: request.quality,
        });
    }
    actions
}
