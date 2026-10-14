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

//! Bottom status bar: read-only (no actions).
//!
//! Active tool hint on the left, application version on the right
//! ("Alpha" prefix). cygnus-ui-only styling (`typography`).

use crate::app::PhotoApp;
use crate::commands::PhotoUiContext;
use cygnus_ui::theme::typography::body_text;

/// Application version (from `Cargo.toml`).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bottom bar (emits no action).
pub fn show(ui: &mut egui::Ui, app: &PhotoApp, ctx: &PhotoUiContext) {
    let theme = ctx.shared.theme();
    let translator = ctx.shared.translator();
    let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
    egui::Panel::bottom("photo_status")
        .resizable(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                match app.active_doc_opt() {
                    Some(doc) => {
                        ui.label(body_text(theme, doc.ui.tool.hint(texts)));
                        if !doc.ui.status.is_empty() {
                            ui.separator();
                            ui.label(body_text(theme, &doc.ui.status));
                        }
                    }
                    None => {
                        ui.label(body_text(
                            theme,
                            texts.get(cygnus_i18n::PhotoTextKey::NoDocumentOpen),
                        ));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(body_text(theme, &format!("Alpha : {APP_VERSION}")));
                });
            });
        });
}
