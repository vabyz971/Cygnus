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

//! Layer list: flattened hierarchy + reordering.
//!
//! The hierarchy (`children`, filters, masks) is flattened into
//! known-height rows ([`super::types::flatten_layers`]) then displayed
//! via `ui_kit::components::ReorderableList::show_variable`: each
//! row allocates EXACTLY its height — no overlap possible.
//! The drop bubbles as [`LayerDrop`](super::types::LayerDrop) (before /
//! inside / after), converted to [`PhotoAction`](crate::commands::PhotoAction)
//! by [`super::panel`] (never a worker send here).

use super::layer_item::{
    LayerItemAction, LayerRenameState, attachments_open, draw_attachment_row, draw_photo_layer_item,
};
use super::types::{
    FlatRow, FlatRowKind, LayerDrop, LayerTexts, LayerThumbView, LayerTreeSelection,
    PhotoLayerInfo, find_attachment_in, find_layer_in, flat_row_name, flatten_layers,
};
use std::collections::HashMap;
use ui_kit::components::{HierarchicalDrop, ReorderableList};
use ui_kit::theme::CygnusTheme;
use ui_kit::theme::UiThemeExt;
use ui_kit::utils::ReorderDragState;
use uuid::Uuid;

/// Allocated height of a flattened row (never overlapping: the
/// list allocates exactly this height per row). All rows share the
/// layer height: nested layers, filters and masks look like their
/// parent rows (Affinity-style).
pub fn row_height_for(theme: &CygnusTheme, kind: FlatRowKind) -> f32 {
    let _ = kind;
    theme.sizes.layer_row
}

/// Draws the semi-transparent ghost following the mouse during the drag.
fn draw_drag_ghost(
    ui: &mut egui::Ui,
    drag_state: &ReorderDragState,
    rows: &[FlatRow],
    layers: &[PhotoLayerInfo],
) {
    if !drag_state.is_dragging {
        return;
    }
    let Some(dragging) = drag_state.dragging_index else {
        return;
    };
    let Some(row) = rows.get(dragging) else {
        return;
    };
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    // Full list-width ghost row at the cursor (Affinity-style: the
    // dragged row travels at full width, never a small box).
    let theme = ui.cygnus_theme();
    let list = ui.min_rect();
    let height = row_height_for(&theme, row.kind);
    let rect = egui::Rect::from_min_size(
        egui::pos2(list.left(), pointer.y - height / 2.0),
        egui::vec2(list.width(), height),
    );
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("photo_layer_drag_ghost"),
    ));
    painter.rect_filled(rect, 0.0, theme.colors.item_selected);
    painter.text(
        rect.left_center() + egui::vec2(theme.spacing.sm + row_depth_indent(row, &theme), 0.0),
        egui::Align2::LEFT_CENTER,
        flat_row_name(layers, row),
        egui::FontId::proportional(theme.typography.body_size),
        theme.colors.fg_primary,
    );
}

/// Indent of a row in pixels (mirrors the row content offset).
fn row_depth_indent(row: &FlatRow, theme: &CygnusTheme) -> f32 {
    row.depth as f32 * theme.spacing.lg
}

/// Converts a flattened-index drop into [`LayerDrop`] (checked
/// bounds: `None` when the list changed meanwhile).
fn resolve_drop(rows: &[FlatRow], drop: HierarchicalDrop) -> Option<LayerDrop> {
    let dragged = rows.get(drop.from).copied()?;
    let target = rows.get(drop.to).copied()?;
    Some(LayerDrop {
        dragged,
        target,
        position: drop.position,
    })
}

/// Draws the reorderable hierarchical list.
///
/// Attachments of a collapsed owner (its row chevron)
/// are hidden; drop indices resolve on those same
/// visible rows (never shifted). Returns the rows' actions
/// and, when present, the hierarchical drop resolved to rows
/// (never a worker send here).
pub fn draw_layer_list(
    ui: &mut egui::Ui,
    layers: &[PhotoLayerInfo],
    sel: LayerTreeSelection,
    t: LayerTexts,
    rename: &mut LayerRenameState,
    drag_state: &mut ReorderDragState,
    thumbs: &HashMap<Uuid, LayerThumbView>,
) -> (Vec<LayerItemAction>, Option<LayerDrop>) {
    let mut actions = Vec::new();
    let all_rows = flatten_layers(layers);
    // Owners folded shut (attachments chevron): rows under them hide
    // via their parent chain (see `visible_rows`). Engine-collapsed
    // groups already prune their subtree in `flatten_layers`.
    let mut closed: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for row in &all_rows {
        if row.kind == FlatRowKind::Layer && !attachments_open(ui, row.id) {
            closed.insert(row.id);
        }
    }
    let rows: Vec<FlatRow> = super::types::visible_rows(all_rows, &closed);
    let theme = ui.cygnus_theme();
    let drop = ReorderableList::new(&rows, theme.sizes.layer_row, drag_state).show_variable(
        ui,
        |row| row_height_for(&theme, row.kind),
        |row| row.nestable,
        // Every row is draggable (layers, filters, masks alike):
        // drops resolve to exact reparent operations.
        |_| true,
        |ui, row, _index, is_dragging_this| {
            // Origin row of the drag: keep the allocated space but
            // draw nothing (the full-width ghost travels instead) —
            // otherwise the row appears duplicated during the drag.
            if is_dragging_this {
                return;
            }
            match row.kind {
                FlatRowKind::Layer => {
                    if let Some(layer) = find_layer_in(layers, row.id) {
                        actions.extend(draw_photo_layer_item(
                            ui,
                            layer,
                            Some(layer.id) == sel.selected,
                            rename,
                            thumbs.get(&layer.id).copied(),
                            row.depth,
                            t,
                        ));
                    }
                }
                FlatRowKind::Filter { owner } | FlatRowKind::Mask { owner, .. } => {
                    if let Some((sub, _)) = find_attachment_in(layers, owner, row.id) {
                        let focused_here = sel
                            .focused
                            .is_some_and(|focus| focus.owner == owner && focus.id == row.id);
                        actions.extend(draw_attachment_row(
                            ui,
                            owner,
                            sub,
                            focused_here,
                            row.depth,
                            t,
                            thumbs,
                        ));
                    }
                }
            }
        },
    );
    draw_drag_ghost(ui, drag_state, &rows, layers);
    let drop = drop.and_then(|d| resolve_drop(&rows, d));
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
        let texts = super::super::types::LayerTexts {
            catalog: ui_kit::i18n::Catalog::new(ui_kit::i18n::Language::Fr),
            texts: crate::i18n::PhotoCatalog::new(ui_kit::i18n::Language::Fr),
        };
        let layers = fixture_layers();
        let mut drag_state = ReorderDragState::default();
        let mut rename = LayerRenameState::default();
        let mut reported = (Vec::new(), None);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = draw_layer_list(
                    ui,
                    &layers,
                    LayerTreeSelection::default(),
                    texts,
                    &mut rename,
                    &mut drag_state,
                    &std::collections::HashMap::new(),
                );
            });
        })
        .drop_without_applying_deltas();
        assert!(reported.0.is_empty(), "aucun clic sans interaction");
        assert_eq!(reported.1, None);
        assert!(!drag_state.is_dragging);
    }

    #[test]
    fn rows_allocate_exact_heights_by_kind() {
        use super::super::types::{FlatRowKind, flatten_layers};
        let theme = CygnusTheme::dark();
        // 3 flat layers: 3 layer-height rows, exact sum.
        let layers = fixture_layers();
        let rows = flatten_layers(&layers);
        assert_eq!(rows.len(), 3);
        let total: f32 = rows
            .iter()
            .map(|row| row_height_for(&theme, row.kind))
            .sum();
        assert_eq!(total, 3.0 * theme.sizes.layer_row);
        assert!(rows.iter().all(|row| row.kind == FlatRowKind::Layer));
        // One layer with a filter: 2 uniform rows (layer + child).
        let mut doc = Document::new(8, 8);
        let mut pixels = PixelLayer::new("fond", Arc::new(image::DynamicImage::new_rgba8(4, 4)));
        pixels.filter_layers.push(photo_engine::FilterLayer::new(
            "brightness_contrast",
            "luminosite",
            std::collections::HashMap::new(),
        ));
        doc.push_layer(LayerNode::Pixel(pixels));
        let layers = super::super::types::snapshot_layers(&doc);
        let rows = flatten_layers(&layers);
        assert_eq!(rows.len(), 2, "filtre = ligne à part entière");
        let total: f32 = rows
            .iter()
            .map(|row| row_height_for(&theme, row.kind))
            .sum();
        assert_eq!(total, 2.0 * theme.sizes.layer_row);
    }

    #[test]
    fn hierarchical_list_renders_group_and_attachment_without_panic() {
        use photo_engine::GroupLayer;
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let texts = super::super::types::LayerTexts {
            catalog: ui_kit::i18n::Catalog::new(ui_kit::i18n::Language::Fr),
            texts: crate::i18n::PhotoCatalog::new(ui_kit::i18n::Language::Fr),
        };
        let mut doc = Document::new(8, 8);
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", image.clone())));
        doc.push_layer(LayerNode::Group(GroupLayer::new(
            "groupe",
            vec![LayerNode::Pixel(PixelLayer::new("dedans", image))],
        )));
        let layers = super::super::types::snapshot_layers(&doc);
        let mut drag_state = ReorderDragState::default();
        let mut rename = LayerRenameState::default();
        let mut reported = (Vec::new(), None);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = draw_layer_list(
                    ui,
                    &layers,
                    LayerTreeSelection::default(),
                    texts,
                    &mut rename,
                    &mut drag_state,
                    &std::collections::HashMap::new(),
                );
            });
        })
        .drop_without_applying_deltas();
        assert!(reported.0.is_empty(), "aucun clic sans interaction");
        assert_eq!(reported.1, None);
    }
}
