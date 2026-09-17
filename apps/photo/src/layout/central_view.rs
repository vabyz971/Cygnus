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

//! Contenu des onglets dock « Canevas » (un par document ouvert).
//!
//! Chaque document ouvert a son onglet dock, titré du nom du
//! document (voir `super::dock`) : activer l'onglet active le
//! document, fermer l'onglet ferme le document. Le canvas
//! (`PhotoCanvas`, viewport générique + outils photo) ne renvoie que
//! des données : le commit d'un trait remonte en
//! [`PhotoAction::CommitStroke`](crate::commands::PhotoAction), la
//! pipette échantillonne en état UI local. Overlay photo (origine)
//! par-dessus.

use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::{OpenDocument, sample_preview_color};
use crate::ui::{PhotoCanvas, PhotoCanvasTool, draw_origin_marker};
use ui_kit::components::{menu_row, menu_style};

/// Canvas du document + overlays + pipette (contenu de l'onglet
/// dock, sans `CentralPanel`).
pub fn draw_canvas_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
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
    // Clic droit : menu contextuel vue (zoom, grille), sections
    // façon rerun.
    if let Some(response) = &outcome.response {
        let theme = ctx.shared.theme();
        let catalog = ctx.shared.translator();
        egui::Popup::context_menu(response)
            .style(menu_style(theme))
            .show(|ui| {
                if menu_row(ui, theme, catalog.get(ui_kit::i18n::TextKey::ZoomIn)) {
                    actions.push(PhotoAction::ZoomIn);
                    ui.close();
                }
                if menu_row(ui, theme, catalog.get(ui_kit::i18n::TextKey::ZoomOut)) {
                    actions.push(PhotoAction::ZoomOut);
                    ui.close();
                }
                if menu_row(ui, theme, "Zoom 100 %") {
                    actions.push(PhotoAction::ZoomReset);
                    ui.close();
                }
                ui.separator();
                if menu_row(ui, theme, catalog.get(ui_kit::i18n::TextKey::Grid)) {
                    actions.push(PhotoAction::ToggleGrid);
                    ui.close();
                }
            });
    }
    // Pipette : échantillonne le composite sous le curseur (local).
    if doc.ui.tool == PhotoCanvasTool::Eyedropper
        && let (Some(world), Some(preview)) =
            (outcome.pointer_world.last(), doc.ui.last_preview.as_ref())
        && let Some(color) = sample_preview_color(preview, world.x, world.y)
    {
        doc.ui.brush.color = color;
    }
    // Overlays : origine monde.
    draw_origin_marker(ui.painter(), ctx, &doc.ui.viewport);
    actions
}
