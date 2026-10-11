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

//! Modals and dialogs over the workspace.
//!
//! Dialogs validate into pure data; validation bubbles up as
//! [`PhotoAction`](crate::commands::PhotoAction) (document
//! creation, export). Adding a filter goes through the Layers
//! panel context menu (never a modal). The app routes next.

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::ui::{
    NewDocumentChoice, draw_create_document_dialog, draw_export_dialog, draw_help_dialog,
};

/// All modals (new document, export, help).
pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    actions.extend(draw_new_document_overlay(ui, app, ctx));
    actions.extend(draw_export_overlay(ui, app, ctx));
    draw_help_dialog(ctx, &mut app.shell.help_open);
    actions
}

/// "Create document" window: confirm = new tab,
/// open image = file dialog.
fn draw_new_document_overlay(
    _ui: &mut egui::Ui,
    app: &mut PhotoApp,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    match draw_create_document_dialog(ctx, &mut app.shell.new_doc_dialog) {
        Some(NewDocumentChoice::Create(width, height)) => {
            actions.push(PhotoAction::CreateDocument { width, height });
        }
        Some(NewDocumentChoice::OpenImage) => {
            actions.push(PhotoAction::OpenImageDialog);
        }
        None => {}
    }
    actions
}

/// "Export" window: confirm = export the active document.
fn draw_export_overlay(
    _ui: &mut egui::Ui,
    app: &mut PhotoApp,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    if let Some(request) = draw_export_dialog(ctx, &mut app.shell.export_dialog) {
        actions.push(PhotoAction::ExportDocument {
            path: request.path,
            quality: request.quality,
        });
    }
    actions
}
