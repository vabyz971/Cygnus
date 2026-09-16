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

//! Orchestrateur du workspace : régions + overlays, rien d'autre.
//!
//! Ordre egui imposé : panneaux haut/bas/latéraux d'abord, vue
//! centrale en dernier, modales par-dessus. Toute la logique vit
//! dans les régions et les features.

use super::{bottom_bar, central_view, left_sidebar, overlays, right_sidebar, top_bar};
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
        // 3. Rail d'outils à gauche.
        actions.extend(left_sidebar::show(ui, app, ctx));
        // 4. Studio droit (inspecteur + calques).
        actions.extend(right_sidebar::show(ui, app, ctx));
        // 5. Barre de statut basse.
        bottom_bar::show(ui, app, ctx);
        // 6. Vue centrale (onglets + canvas) EN DERNIER.
        actions.extend(central_view::show(ui, app, ctx));
        // 7. Modales et dialogs par-dessus.
        actions.extend(overlays::show(ui, app, ctx));
        app.queue.extend(actions);
    }
}
