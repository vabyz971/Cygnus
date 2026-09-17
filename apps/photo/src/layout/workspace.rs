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

//! Orchestrateur du workspace : barres fixes + docks + overlays.
//!
//! Ordre egui imposé : panneaux haut/bas d'abord, zone centrale
//! ([`DockArea`](egui_dock::DockArea) : un onglet canevas par
//! document + outils, inspecteur, calques) ensuite, modales
//! par-dessus. Les barres haute (menus, modes) et basse (statut)
//! restent fixes. Toute la logique vit dans les régions et les
//! features.
use super::{bottom_bar, dock, overlays, top_bar};
use crate::app::PhotoApp;
use crate::commands::PhotoUiContext;

/// Composition globale de l'application (orchestration pure).
pub struct PhotoWorkspace;

impl PhotoWorkspace {
    /// Dessine un frame complet et file les actions dans `app.queue`.
    pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) {
        let mut actions = Vec::new();
        // 1-2. Haut : menus puis modes/options (toute la largeur).
        actions.extend(top_bar::show_menu_bar(ui, app, ctx));
        actions.extend(top_bar::show_mode_bar(ui, app, ctx));
        // 3. Barre de statut basse (réservée avant la zone centrale).
        bottom_bar::show(ui, app, ctx);
        // 4. Zone centrale : docks ancrables (canevas par document).
        egui::CentralPanel::default().show(ui, |ui| {
            actions.extend(dock::show_dock_area(ui, app, ctx));
        });
        // 5. Modales et dialogs par-dessus.
        actions.extend(overlays::show(ui, app, ctx));
        app.queue.extend(actions);
    }
}
