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

//! Écran d'accueil : aucun document ouvert.
//!
//! Propose de créer un document (fenêtre de création) ou d'ouvrir
//! une image. Affiché à la place de l'arbre tant que `docs` est vide.

use crate::commands::PhotoAction;
use ui_kit::components::{Button, ButtonVariant};
use ui_kit::theme::typography::{body_text, heading_text};

/// Dessine l'accueil et retourne les actions (création / ouverture).
pub fn show(ui: &mut egui::Ui, ctx: &crate::commands::PhotoUiContext) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let theme = ctx.shared.theme();
    ui.vertical_centered(|ui| {
        ui.add_space(theme.spacing.xl * 3.0);
        ui.label(heading_text(theme, "Aucun document ouvert"));
        ui.add_space(theme.spacing.sm);
        ui.label(body_text(
            theme,
            "Créez un document ou ouvrez une image pour commencer.",
        ));
        ui.add_space(theme.spacing.lg);
        ui.horizontal(|ui| {
            if Button::new("Nouveau document")
                .variant(ButtonVariant::Primary)
                .show(ui, theme)
                .clicked()
            {
                actions.push(PhotoAction::OpenNewDocumentDialog);
            }
            if Button::new("Ouvrir une image…")
                .variant(ButtonVariant::Secondary)
                .show(ui, theme)
                .clicked()
            {
                actions.push(PhotoAction::OpenImageDialog);
            }
        });
    });
    actions
}
