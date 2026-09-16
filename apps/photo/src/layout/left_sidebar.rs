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

//! Rail d'outils gauche : icônes d'outils + couleur du pinceau.
//!
//! Le rail suit le mode d'édition ; le choix d'outil remonte en
//! [`PhotoAction::SetTool`](crate::commands::PhotoAction). La couleur
//! est un état UI local (mutation directe, aucun moteur).

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::ui::{draw_color_panel, draw_tool_rail};

/// Colonne gauche (rail 60px + nuancier).
pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, _ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    egui::Panel::left("photo_tools")
        .resizable(false)
        .exact_size(60.0)
        .show(ui, |ui| {
            let doc = app.active_doc_mut();
            if let Some(tool) = draw_tool_rail(
                ui,
                &mut doc.ui.tool,
                &mut doc.ui.brush.color,
                doc.ui.edit_mode,
            ) {
                actions.push(PhotoAction::SetTool(tool));
            }
            ui.separator();
            draw_color_panel(ui, &mut doc.ui.brush.color);
        });
    actions
}
