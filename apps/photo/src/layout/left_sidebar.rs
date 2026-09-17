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

//! Contenu de l'onglet dock « Outils » : rail compact 32 px.
//!
//! Position-indépendant : le placement (taille, split, fenêtre
//! flottante) est géré par le [`DockArea`](egui_dock::DockArea)
//! (voir `super::dock`). Le choix d'outil remonte en
//! [`PhotoAction::SetTool`](crate::commands::PhotoAction) ; la
//! couleur est un état UI local (mutation directe, aucun moteur).

use crate::commands::PhotoAction;
use crate::state::OpenDocument;
use crate::ui::draw_tool_rail;
use ui_kit::theme::CygnusTheme;

/// Rail d'outils compact (contenu de l'onglet, sans `Panel`).
pub fn draw_tools_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    theme: &CygnusTheme,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    if let Some(tool) = draw_tool_rail(
        ui,
        &mut doc.ui.tool,
        &mut doc.ui.brush.color,
        doc.ui.edit_mode,
        theme,
    ) {
        actions.push(PhotoAction::SetTool(tool));
    }
    actions
}
