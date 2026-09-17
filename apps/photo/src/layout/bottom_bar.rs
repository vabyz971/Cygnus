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

//! Barre de statut basse : lecture seule (aucune action).
//!
//! Hint de l'outil à gauche, version de l'application à droite
//! (préfixe « Alpha »). Style exclusivement ui-kit (`typography`).

use crate::app::PhotoApp;
use crate::commands::PhotoUiContext;
use ui_kit::theme::typography::body_text;

/// Version de l'application (depuis `Cargo.toml`).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Barre basse (aucune action émise).
pub fn show(ui: &mut egui::Ui, app: &PhotoApp, ctx: &PhotoUiContext) {
    let theme = ctx.shared.theme();
    let doc = app.active_doc();
    egui::Panel::bottom("photo_status")
        .resizable(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(body_text(theme, doc.ui.tool.hint()));
                if !doc.ui.status.is_empty() {
                    ui.separator();
                    ui.label(body_text(theme, &doc.ui.status));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(body_text(theme, &format!("Alpha : {APP_VERSION}")));
                });
            });
        });
}
