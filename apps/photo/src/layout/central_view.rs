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
//! [`PhotoAction::CommitStroke`](crate::commands::PhotoAction), le
//! déplacement en [`PhotoAction::MoveLayer`](crate::commands::PhotoAction),
//! la pipette échantillonne en état UI local. Cadre du document et
//! curseur du pinceau par-dessus.

use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::{OpenDocument, sample_preview_color};
use crate::ui::{
    CanvasMapping, PhotoCanvas, PhotoCanvasTool, draw_brush_cursor, draw_document_bounds,
};
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
    // Plan infini : la miniature peut dépasser le document (calque
    // déplacé hors cadre) et être réduite (plafond 1600 px) — le
    // mapping ramène chaque geste en pixels document exacts.
    let geom = doc.ui.preview_geom;
    let mapping = geom
        .map(|g| CanvasMapping {
            origin: g.origin,
            thumb_to_doc: g.thumb_to_doc(),
        })
        .unwrap_or_else(CanvasMapping::identity);
    let outcome = PhotoCanvas::new(&mut doc.ui.stroke)
        .texture(texture)
        .tool(doc.ui.tool)
        .show_grid(doc.ui.show_grid)
        .active_layer(doc.ui.selected)
        .brush(doc.ui.brush)
        .mapping(mapping)
        .show(ui, &mut doc.ui.viewport);
    // Commit du trait : routé au worker par l'app.
    if let Some(paint) = outcome.paint {
        actions.push(PhotoAction::CommitStroke(paint));
    }
    // Commit du déplacement (outil sélection) : routé au worker.
    if let Some(moving) = outcome.move_layer {
        actions.push(PhotoAction::MoveLayer {
            layer: moving.layer,
            dx: moving.dx,
            dy: moving.dy,
        });
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
    // Pipette : le geste est en pixels document, l'échantillon en
    // pixels miniature — reconverti via l'origine et l'échelle.
    if doc.ui.tool == PhotoCanvasTool::Eyedropper
        && let (Some(world), Some(preview)) =
            (outcome.pointer_world.last(), doc.ui.last_preview.as_ref())
    {
        let origin = geom.map(|g| g.origin).unwrap_or(egui::Vec2::ZERO);
        let to_thumb = 1.0 / mapping.thumb_to_doc.max(f32::EPSILON);
        let sampled = sample_preview_color(
            preview,
            origin.x + world.x * to_thumb,
            origin.y + world.y * to_thumb,
        );
        if let Some(color) = sampled {
            doc.ui.brush.color = color;
        }
    }
    // Overlays : cadre EXACT du document dans la miniature (stable
    // quand un calque sort du cadre ou que la miniature réduit),
    // dimensions en haut à gauche, puis cercle du pinceau/gomme.
    let (doc_rect, label) = match (geom, outcome.dest_rect) {
        (Some(g), Some(dest)) => {
            let rect = g.doc_rect(dest);
            let dims = format!("{} x {}", g.doc_size.x as u32, g.doc_size.y as u32);
            (rect, Some(dims))
        }
        _ => (outcome.dest_rect, None),
    };
    draw_document_bounds(ui.painter(), ctx, doc_rect, label.as_deref());
    if matches!(
        doc.ui.tool,
        PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser
    ) {
        // Curseur OS conservé (croix du viewport) + cercle au diamètre
        // du pinceau/gomme par-dessus.
        if let (Some(dest), Some(geom)) = (outcome.dest_rect, geom) {
            let to_screen = if geom.thumb_size.x > 0.0 {
                dest.width() / geom.thumb_size.x / mapping.thumb_to_doc.max(f32::EPSILON)
            } else {
                0.0
            };
            draw_brush_cursor(
                ui.painter(),
                ctx,
                Some(dest),
                outcome.hover_pos,
                doc.ui.brush.radius,
                to_screen,
            );
        }
    }
    actions
}
