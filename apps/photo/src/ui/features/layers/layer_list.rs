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

//! Liste des calques : virtualisation + réordonnancement.
//!
//! Toute la mécanique de drag & drop est déléguée à
//! `ui_kit::ReorderableList` (générique) ; ce module ne contient que
//! le rendu photo (voir [`super::layer_item`]) et le fantôme de drag.
//! Le drop remonte en `(from, to)` (indices d'affichage), converti en
//! [`PhotoAction`](crate::commands::PhotoAction) par [`super::panel`].

use super::layer_item::{LayerItemAction, LayerRenameState, draw_photo_layer_item};
use super::types::PhotoLayerInfo;
use ui_kit::theme::tokens::CygnusTheme;
use ui_kit::utils::ReorderDragState;
use ui_kit::widgets::ReorderableList;
use uuid::Uuid;

/// Hauteur d'une ligne de calque HUD (rangée principale + bandeau
/// sous-couches, virtualisation à hauteur fixe).
pub const PHOTO_LAYER_ITEM_HEIGHT: f32 = 76.0;

/// Dessine le fantôme semi-transparent qui suit la souris pendant le drag.
fn draw_drag_ghost(ui: &mut egui::Ui, drag_state: &ReorderDragState, layers: &[PhotoLayerInfo]) {
    if !drag_state.is_dragging {
        return;
    }
    let Some(dragging) = drag_state.dragging_index else {
        return;
    };
    let Some(layer) = layers.get(dragging) else {
        return;
    };
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    let theme = CygnusTheme::dark();
    let rect = egui::Rect::from_min_size(
        egui::pos2(pointer.x + 12.0, pointer.y - 16.0),
        egui::vec2(200.0, 32.0),
    );
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("photo_layer_drag_ghost"),
    ));
    painter.rect_filled(rect, theme.radius.sm, theme.colors.item_selected);
    painter.text(
        rect.left_center() + egui::vec2(theme.spacing.sm, 0.0),
        egui::Align2::LEFT_CENTER,
        layer.name.clone(),
        egui::FontId::proportional(theme.typography.body_size),
        theme.colors.fg_primary,
    );
}

/// Dessine la liste virtualisée et réordonnable.
///
/// Retourne les actions des lignes et, le cas échéant, le drop
/// `(from, to)` en indices d'affichage (jamais d'envoi worker ici).
pub fn draw_layer_list(
    ui: &mut egui::Ui,
    layers: &[PhotoLayerInfo],
    selected: Option<Uuid>,
    rename: &mut LayerRenameState,
    drag_state: &mut ReorderDragState,
) -> (Vec<LayerItemAction>, Option<(usize, usize)>) {
    let mut actions = Vec::new();
    let reorder = ReorderableList::new(layers, PHOTO_LAYER_ITEM_HEIGHT, drag_state).show(
        ui,
        |ui, layer, _index, _is_dragging| {
            actions.extend(draw_photo_layer_item(
                ui,
                layer,
                Some(layer.id) == selected,
                rename,
            ));
        },
    );
    draw_drag_ghost(ui, drag_state, layers);
    let drop = reorder.filter(|(from, to)| from != to);
    (actions, drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::{Document, LayerNode, PixelLayer};
    use std::sync::Arc;

    fn fixture_layers() -> Vec<PhotoLayerInfo> {
        let mut doc = Document::new(8, 8);
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        for name in ["fond", "milieu", "dessus"] {
            doc.push_layer(LayerNode::Pixel(PixelLayer::new(name, image.clone())));
        }
        super::super::types::snapshot_layers(&doc)
    }

    #[test]
    fn list_renders_without_panic_and_reports_nothing() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let layers = fixture_layers();
        let mut drag_state = ReorderDragState::default();
        let mut rename = LayerRenameState::default();
        let mut reported = (Vec::new(), None);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = draw_layer_list(ui, &layers, None, &mut rename, &mut drag_state);
            });
        })
        .drop_without_applying_deltas();
        assert!(reported.0.is_empty(), "aucun clic sans interaction");
        assert_eq!(reported.1, None);
        assert!(!drag_state.is_dragging);
    }
}
