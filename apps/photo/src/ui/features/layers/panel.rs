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

//! Layers panel: domain content, position-independent.
//!
//! `LayersPanel` does not know where it is displayed: the workspace
//! (`crate::layout`, "Layers" dock tab) decides placement,
//! with no `Panel` chrome. The list comes from
//! [`super::layer_list`], the button bar sits at the bottom. No
//! worker send: everything bubbles as [`LayerPanelAction`], converted to
//! [`PhotoAction`](crate::commands::PhotoAction) (see
//! [`super::actions`])).

use super::layer_item::{LayerItemAction, LayerRenameState};
use super::layer_list::draw_layer_list;
use super::types::{
    FlatRow, FlatRowKind, LayerDrop, LayerTexts, LayerThumbView, LayerTreeSelection,
    PhotoLayerInfo, find_filter_in, find_filter_position, find_layer_in, find_mask_position,
};
use crate::commands::PhotoUiContext;
use cygnus_ui::components::{IconButton, Select, Slider, menu_row, menu_style};
use cygnus_ui::icons::Icon;
use cygnus_ui::primitives::Text;
use cygnus_ui::theme::CygnusTheme;
use cygnus_ui::utils::{DropPosition, ReorderDragState};
use photo_engine::BlendMode;
use std::collections::HashMap;
use uuid::Uuid;

/// Layers panel action: toolbar + HUD interactions.
///
/// Routed by `PhotoApp` (local selection, rename and mutations
/// via the worker) after conversion (see [`super::actions`]).
#[derive(Debug, Clone, PartialEq)]
pub enum LayerPanelAction {
    /// Add an image (file picker).
    AddImage,
    /// New empty layer.
    AddEmpty,
    /// Duplicate the selection.
    Duplicate,
    /// Add a live filter to the selected layer (button context
    /// menu, engine-registry catalog).
    AddFilter {
        /// Owning layer.
        layer: Uuid,
        /// Registry `type_id` (e.g. `"blur"`).
        filter_type: String,
    },
    /// Add a mask to the selected layer.
    AddMask,
    /// Delete the selection.
    Delete,
    /// Select a layer (click on its row).
    Select(Uuid),
    /// Focus an attachment (click on its tree row):
    /// the owner becomes the primary selection.
    SelectAttachment {
        /// Owning layer.
        owner: Uuid,
        /// Targeted filter or mask.
        id: Uuid,
    },
    /// Enable/disable a filter (its row eye).
    ToggleFilter {
        /// Owning layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
    },
    /// Enable/disable a mask (its row eye).
    ToggleMask {
        /// Owner.
        owner: Uuid,
        /// Targeted mask.
        mask: Uuid,
    },
    /// Confirm a layer rename.
    RenameCommit { layer: Uuid, name: String },
    /// Reorder via hierarchical drag & drop: `dragged` before or
    /// after `target` (same parent or not — free engine nesting).
    ReorderNodes {
        /// Moved layer.
        dragged: Uuid,
        /// Target layer.
        target: Uuid,
        /// True = before the target, false = after.
        before: bool,
    },
    /// Nest a layer at a parent's head (drop inside a group or a
    /// pixel layer).
    MoveIntoParent {
        /// Moved layer.
        layer: Uuid,
        /// Host parent (group or pixel layer).
        parent: Uuid,
    },
    /// Move a live filter to another chain at an exact index (DnD
    /// reparent, Affinity-style).
    MoveFilterTo {
        /// Filter being moved.
        filter: Uuid,
        /// Source chain owner.
        from: Uuid,
        /// Destination chain owner.
        to: Uuid,
        /// Insertion index (clamped engine-side).
        index: usize,
    },
    /// Move a mask to another pile at an exact index (DnD reparent).
    MoveMaskTo {
        /// Mask being moved.
        mask: Uuid,
        /// Source pile owner (layer or filter id).
        from: Uuid,
        /// Destination pile owner (layer or filter id).
        to: Uuid,
        /// Insertion index (clamped engine-side).
        index: usize,
    },
    /// Add a mask to the focused filter's own pile.
    AddMaskToFilter(Uuid),
    /// Collapse / expand a group (no re-render).
    ToggleCollapsed(Uuid),
    /// Toggle a layer's visibility (eye).
    ToggleVisibility(Uuid),
    /// Solo a layer (only it stays visible).
    IsolateLayer(Uuid),
    /// Delete a layer (row cross).
    DeleteLayer(Uuid),
    /// Duplicate a layer (row context menu).
    DuplicateLayer(Uuid),
    /// Horizontal mirror (row Transformations submenu).
    FlipHorizontalSelected,
    /// Vertical mirror (same).
    FlipVerticalSelected,
    /// 90° clockwise rotation (same).
    RotateClockwiseSelected,
    /// 90° counterclockwise rotation (same).
    RotateCounterclockwiseSelected,
    /// Crop to document (same).
    CropSelectedToDocument,
    /// Duplicate a live filter (context menu).
    DuplicateFilter { layer: Uuid, filter: Uuid },
    /// Delete a filter.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Delete a mask.
    RemoveMask { owner: Uuid, mask: Uuid },
    /// Set the selected layer's opacity (units 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Set the selected layer's blend mode.
    SetBlendMode { layer: Uuid, mode: BlendMode },
}

/// Layers panel (direct content: list + bottom button bar,
/// no chrome).
pub struct LayersPanel;

impl LayersPanel {
    /// Draws the content (list, bottom button bar) and returns
    /// the actions.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        layers: &[PhotoLayerInfo],
        sel: LayerTreeSelection,
        rename: &mut LayerRenameState,
        drag_state: &mut ReorderDragState,
        thumbs: &HashMap<Uuid, LayerThumbView>,
        filter_types: &[(String, String)],
    ) -> Vec<LayerPanelAction> {
        draw_layers_panel(
            ui,
            ctx,
            layers,
            sel,
            rename,
            drag_state,
            thumbs,
            filter_types,
        )
    }
}

/// Affinity-style header: opacity (worker-coalesced slider +
/// compact value field) + the selection's blend mode (compact
/// combo). Two grid rows share one label column so controls align;
/// the combo and the value field stay small, the slider fills the
/// rest. Without a selection, controls are
/// disabled (never implicitly adopting the layer above); visibility
/// is set at each row's eye, never here.
fn draw_selection_header(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    t: LayerTexts,
    layers: &[PhotoLayerInfo],
    selected: Option<Uuid>,
) -> Vec<LayerPanelAction> {
    let texts = t.texts;
    let mut actions = Vec::new();
    // Recursive lookup: nested (child) layers drive the header too.
    let target = selected.and_then(|id| find_layer_in(layers, id));
    let (mut opacity, mut choice, enabled) = match target {
        Some(layer) => (
            layer.opacity,
            BlendMode::ALL
                .iter()
                .position(|mode| *mode == layer.blend_mode)
                .unwrap_or(0),
            true,
        ),
        None => (100.0, 0, false),
    };
    let before_opacity = opacity;
    let before_choice = choice;
    if layers.is_empty() {
        Text::caption(theme, texts.get(cygnus_i18n::PhotoTextKey::NoLayers)).show(ui);
        ui.separator();
        return actions;
    }
    ui.add_enabled_ui(enabled, |ui| {
        egui::Grid::new("layer_header_params")
            .num_columns(2)
            .spacing(egui::vec2(theme.spacing.sm, theme.spacing.xs))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(texts.get(cygnus_i18n::PhotoTextKey::OpacityLabel))
                        .size(theme.typography.body_size)
                        .color(theme.colors.fg_secondary),
                );
                ui.horizontal(|ui| {
                    Slider::new("", 0.0..=100.0).show(ui, theme, &mut opacity);
                    // Compact value field (derived token width).
                    ui.add_sized(
                        egui::vec2(theme.sizes.icon_button * 2.0, theme.sizes.button_sm),
                        egui::DragValue::new(&mut opacity)
                            .range(0.0..=100.0)
                            .speed(0.5),
                    );
                });
                ui.end_row();
                ui.label(
                    egui::RichText::new(texts.get(cygnus_i18n::PhotoTextKey::BlendLabel))
                        .size(theme.typography.body_size)
                        .color(theme.colors.fg_secondary),
                );
                let options: [&str; 6] = BlendMode::ALL.map(BlendMode::label);
                Select::new("", &options).show(ui, theme, &mut choice);
                ui.end_row();
            });
    });
    let Some(layer) = target else {
        ui.separator();
        return actions;
    };
    // Engine units 0..=100 (like the inspector).
    if opacity != before_opacity && opacity != layer.opacity {
        actions.push(LayerPanelAction::SetOpacity {
            layer: layer.id,
            opacity,
        });
    }
    if let Some(mode) = BlendMode::ALL.get(choice)
        && choice != before_choice
        && *mode != layer.blend_mode
    {
        actions.push(LayerPanelAction::SetBlendMode {
            layer: layer.id,
            mode: *mode,
        });
    }
    ui.separator();
    actions
}

/// Converts a hierarchical drop into a panel action, following the
/// DnD mapping table (`egui-bounded-dnd` skill): layers reorder
/// (before/after) or nest (into a group or pixel layer); filters move
/// between chains at the exact indicated index; masks move between
/// piles at the exact indicated index. Drops onto the dragged row
/// itself are ignored. The engine re-validates everything (cycles,
/// unknown ids) — the UI never trusts itself alone.
fn convert_drop(layers: &[PhotoLayerInfo], drop: LayerDrop) -> Option<LayerPanelAction> {
    use FlatRowKind as Kind;
    if drop.dragged.id == drop.target.id {
        return None;
    }
    // Chain length of a layer's filter pile.
    let chain_len =
        |owner: Uuid| -> Option<usize> { find_layer_in(layers, owner).map(|l| l.filters.len()) };
    // Length of a mask pile owned by a layer or a filter.
    let pile_len = |owner: Uuid| -> Option<usize> {
        if let Some(porteur) = find_layer_in(layers, owner) {
            return Some(porteur.masks.len());
        }
        find_filter_in(layers, owner).map(|(_, f)| f.masks.len())
    };
    // Owning layer of any attachment row (filter rows carry it, mask
    // rows resolve through their owner chain).
    let row_layer = |row: FlatRow| -> Option<Uuid> {
        match row.kind {
            Kind::Layer => Some(row.id),
            Kind::Filter { owner } => Some(owner),
            Kind::Mask { layer, .. } => Some(layer),
        }
    };
    match (drop.dragged.kind, drop.target.kind, drop.position) {
        // --- Dragged layer: reorder, or nest into a group/pixel. ---
        (Kind::Layer, Kind::Layer, DropPosition::Before) => Some(LayerPanelAction::ReorderNodes {
            dragged: drop.dragged.id,
            target: drop.target.id,
            before: true,
        }),
        (Kind::Layer, Kind::Layer, DropPosition::After) => Some(LayerPanelAction::ReorderNodes {
            dragged: drop.dragged.id,
            target: drop.target.id,
            before: false,
        }),
        (Kind::Layer, Kind::Layer, DropPosition::Into) => {
            let target = find_layer_in(layers, drop.target.id)?;
            (target.kind == super::types::PhotoLayerKind::Group
                || target.kind == super::types::PhotoLayerKind::Pixel)
                .then_some(LayerPanelAction::MoveIntoParent {
                    layer: drop.dragged.id,
                    parent: drop.target.id,
                })
        }
        // Onto an attachment row: apply to its owner layer.
        (Kind::Layer, Kind::Filter { .. } | Kind::Mask { .. }, DropPosition::Before) => {
            Some(LayerPanelAction::ReorderNodes {
                dragged: drop.dragged.id,
                target: row_layer(drop.target)?,
                before: true,
            })
        }
        (Kind::Layer, Kind::Filter { .. } | Kind::Mask { .. }, _) => {
            Some(LayerPanelAction::ReorderNodes {
                dragged: drop.dragged.id,
                target: row_layer(drop.target)?,
                before: false,
            })
        }
        // --- Dragged filter: move between chains at the exact index. ---
        (Kind::Filter { owner: from }, Kind::Layer, DropPosition::Before) => {
            Some(LayerPanelAction::MoveFilterTo {
                filter: drop.dragged.id,
                from,
                to: drop.target.id,
                index: 0,
            })
        }
        (Kind::Filter { owner: from }, Kind::Layer, _) => Some(LayerPanelAction::MoveFilterTo {
            filter: drop.dragged.id,
            from,
            to: drop.target.id,
            index: chain_len(drop.target.id)?,
        }),
        (Kind::Filter { owner: from }, Kind::Filter { owner: to }, DropPosition::Before) => {
            Some(LayerPanelAction::MoveFilterTo {
                filter: drop.dragged.id,
                from,
                to,
                index: find_filter_position(layers, to, drop.target.id)?,
            })
        }
        (Kind::Filter { owner: from }, Kind::Filter { owner: to }, _) => {
            // After: target index + 1 in snapshot coordinates — the
            // engine normalizes the removal shift itself.
            let index = find_filter_position(layers, to, drop.target.id)? + 1;
            Some(LayerPanelAction::MoveFilterTo {
                filter: drop.dragged.id,
                from,
                to,
                index,
            })
        }
        // Onto a mask row: join its owner layer's chain (append).
        (Kind::Filter { owner: from }, Kind::Mask { layer, .. }, _) => {
            Some(LayerPanelAction::MoveFilterTo {
                filter: drop.dragged.id,
                from,
                to: layer,
                index: chain_len(layer)?,
            })
        }
        // --- Dragged mask: move between piles at the exact index. ---
        (Kind::Mask { owner: from, .. }, Kind::Layer, DropPosition::Before) => {
            Some(LayerPanelAction::MoveMaskTo {
                mask: drop.dragged.id,
                from,
                to: drop.target.id,
                index: 0,
            })
        }
        (Kind::Mask { owner: from, .. }, Kind::Layer, _) => Some(LayerPanelAction::MoveMaskTo {
            mask: drop.dragged.id,
            from,
            to: drop.target.id,
            index: pile_len(drop.target.id)?,
        }),
        (Kind::Mask { owner: from, .. }, Kind::Filter { owner: _ }, DropPosition::Before) => {
            Some(LayerPanelAction::MoveMaskTo {
                mask: drop.dragged.id,
                from,
                to: drop.target.id,
                index: 0,
            })
        }
        (Kind::Mask { owner: from, .. }, Kind::Filter { .. }, _) => {
            Some(LayerPanelAction::MoveMaskTo {
                mask: drop.dragged.id,
                from,
                to: drop.target.id,
                index: pile_len(drop.target.id)?,
            })
        }
        (Kind::Mask { owner: from, .. }, Kind::Mask { owner: to, .. }, DropPosition::Before) => {
            Some(LayerPanelAction::MoveMaskTo {
                mask: drop.dragged.id,
                from,
                to,
                index: find_mask_position(layers, to, drop.target.id)?,
            })
        }
        (Kind::Mask { owner: from, .. }, Kind::Mask { owner: to, .. }, _) => {
            // Same normalization rule as filter chains (engine-side).
            let index = find_mask_position(layers, to, drop.target.id)? + 1;
            Some(LayerPanelAction::MoveMaskTo {
                mask: drop.dragged.id,
                from,
                to,
                index,
            })
        }
    }
}

/// Header + bounded list (reserves the button bar) + bottom bar.
#[allow(clippy::too_many_arguments)]
fn draw_layers_panel(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    layers: &[PhotoLayerInfo],
    sel: LayerTreeSelection,
    rename: &mut LayerRenameState,
    drag_state: &mut ReorderDragState,
    thumbs: &HashMap<Uuid, LayerThumbView>,
    filter_types: &[(String, String)],
) -> Vec<LayerPanelAction> {
    let theme = ctx.shared.theme();
    let translator = ctx.shared.translator();
    let t = LayerTexts {
        catalog: translator,
        texts: cygnus_i18n::PhotoCatalog::new(translator.language()),
    };
    let texts = t.texts;
    let mut actions = Vec::new();
    actions.extend(draw_selection_header(ui, theme, t, layers, sel.selected));
    // Bounded list: reserves the header and button bar.
    let bar_height = theme.sizes.bar_height;
    let list_height = (ui.available_height() - bar_height).max(80.0);
    let (item_actions, drop) = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), list_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                // In-progress rename on a vanished layer: cancel.
                if rename
                    .editing
                    .is_some_and(|id| !layers.iter().any(|layer| layer.id == id))
                {
                    rename.editing = None;
                }
                draw_layer_list(ui, layers, sel, t, rename, drag_state, thumbs)
            },
        )
        .inner;
    for item_action in item_actions {
        match item_action {
            LayerItemAction::Select(id) => actions.push(LayerPanelAction::Select(id)),
            LayerItemAction::SelectAttachment { owner, id } => {
                actions.push(LayerPanelAction::SelectAttachment { owner, id });
            }
            LayerItemAction::ToggleFilter { layer, filter } => {
                actions.push(LayerPanelAction::ToggleFilter { layer, filter });
            }
            LayerItemAction::ToggleMask { owner, mask } => {
                actions.push(LayerPanelAction::ToggleMask { owner, mask });
            }
            LayerItemAction::RenameCommit { layer, name } => {
                actions.push(LayerPanelAction::RenameCommit { layer, name });
            }
            LayerItemAction::ToggleVisibility(id) => {
                actions.push(LayerPanelAction::ToggleVisibility(id));
            }
            LayerItemAction::IsolateLayer(id) => {
                actions.push(LayerPanelAction::IsolateLayer(id));
            }
            LayerItemAction::DeleteLayer(id) => actions.push(LayerPanelAction::DeleteLayer(id)),
            LayerItemAction::DuplicateLayer(id) => {
                actions.push(LayerPanelAction::DuplicateLayer(id));
            }
            LayerItemAction::FlipHorizontalSelected => {
                actions.push(LayerPanelAction::FlipHorizontalSelected);
            }
            LayerItemAction::FlipVerticalSelected => {
                actions.push(LayerPanelAction::FlipVerticalSelected);
            }
            LayerItemAction::RotateClockwiseSelected => {
                actions.push(LayerPanelAction::RotateClockwiseSelected);
            }
            LayerItemAction::RotateCounterclockwiseSelected => {
                actions.push(LayerPanelAction::RotateCounterclockwiseSelected);
            }
            LayerItemAction::CropSelectedToDocument => {
                actions.push(LayerPanelAction::CropSelectedToDocument);
            }
            LayerItemAction::ToggleCollapsed(id) => {
                actions.push(LayerPanelAction::ToggleCollapsed(id));
            }
            LayerItemAction::RemoveFilter { layer, filter } => {
                actions.push(LayerPanelAction::RemoveFilter { layer, filter });
            }
            LayerItemAction::DuplicateFilter { layer, filter } => {
                actions.push(LayerPanelAction::DuplicateFilter { layer, filter });
            }
            LayerItemAction::RemoveMask { owner, mask } => {
                actions.push(LayerPanelAction::RemoveMask { owner, mask });
            }
        }
    }
    if let Some(drop) = drop
        && let Some(action) = convert_drop(layers, drop)
    {
        actions.push(action);
    }
    // Bottom button bar: add image, empty layer, duplicate,
    // mask, filter (catalog context menu), delete.
    ui.horizontal(|ui| {
        if IconButton::new(Icon::ImageIcon)
            .tooltip(texts.get(cygnus_i18n::PhotoTextKey::LayerFromImage))
            .show(ui, theme)
            .clicked()
        {
            actions.push(LayerPanelAction::AddImage);
        }
        if IconButton::new(Icon::Add)
            .tooltip(texts.get(cygnus_i18n::PhotoTextKey::NewEmptyLayer))
            .show(ui, theme)
            .clicked()
        {
            actions.push(LayerPanelAction::AddEmpty);
        }
        ui.add_enabled_ui(sel.selected.is_some(), |ui| {
            if IconButton::new(Icon::Duplicate)
                .tooltip(texts.get(cygnus_i18n::PhotoTextKey::DuplicateLayer))
                .show(ui, theme)
                .clicked()
            {
                actions.push(LayerPanelAction::Duplicate);
            }
            if IconButton::new(Icon::Mask)
                .tooltip(texts.get(cygnus_i18n::PhotoTextKey::AddMask))
                .show(ui, theme)
                .clicked()
            {
                // A focused filter owns the new mask (its own pile);
                // otherwise the selected layer does.
                let filter_focus = sel.focused.and_then(|focus| {
                    sel.selected
                        .and_then(|owner| find_layer_in(layers, owner))
                        .and_then(|porteur| {
                            porteur
                                .filters
                                .iter()
                                .find(|f| f.id == focus.id)
                                .map(|f| f.id)
                        })
                });
                match filter_focus {
                    Some(filter) => actions.push(LayerPanelAction::AddMaskToFilter(filter)),
                    None => actions.push(LayerPanelAction::AddMask),
                }
            }
            // Filter-add context menu (Affinity-style: no
            // modal, the catalog opens at the button).
            let filter_resp = IconButton::new(Icon::Settings)
                .tooltip(texts.get(cygnus_i18n::PhotoTextKey::FilterList))
                .show(ui, theme);
            egui::Popup::menu(&filter_resp)
                .style(menu_style(theme))
                .show(|ui| {
                    for (name, type_id) in filter_types {
                        let label =
                            crate::ui::naming::filter_display_name(type_id, texts).unwrap_or(name);
                        if menu_row(ui, theme, label) {
                            if let Some(layer) = sel.selected {
                                actions.push(LayerPanelAction::AddFilter {
                                    layer,
                                    filter_type: type_id.clone(),
                                });
                            }
                            ui.close();
                        }
                    }
                });
            if IconButton::new(Icon::Delete)
                .tooltip(texts.get(cygnus_i18n::PhotoTextKey::DeleteLayer))
                .show(ui, theme)
                .clicked()
            {
                actions.push(LayerPanelAction::Delete);
            }
        });
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::super::types::{FlatRow, FlatRowKind, LayerDrop};
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
    fn panel_renders_without_panic_and_reports_nothing() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let layers = fixture_layers();
        let mut drag_state = ReorderDragState::default();
        let mut rename = LayerRenameState::default();
        let mut reported = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = LayersPanel::show(
                    ui,
                    &photo_ctx,
                    &layers,
                    LayerTreeSelection::default(),
                    &mut rename,
                    &mut drag_state,
                    &std::collections::HashMap::new(),
                    &[],
                );
            });
        })
        .drop_without_applying_deltas();
        assert!(reported.is_empty(), "aucun clic sans interaction");
        assert!(!drag_state.is_dragging);
    }

    #[test]
    fn panel_with_selection_renders_header_without_actions() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let layers = fixture_layers();
        let selected = layers.first().map(|layer| layer.id);
        let mut drag_state = ReorderDragState::default();
        let mut rename = LayerRenameState::default();
        let mut reported = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = LayersPanel::show(
                    ui,
                    &photo_ctx,
                    &layers,
                    LayerTreeSelection {
                        selected,
                        focused: None,
                    },
                    &mut rename,
                    &mut drag_state,
                    &std::collections::HashMap::new(),
                    &[],
                );
            });
        })
        .drop_without_applying_deltas();
        assert!(
            reported.is_empty(),
            "en-tête inerte sans interaction : {reported:?}"
        );
    }

    #[test]
    fn panel_empty_renders_without_panic() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut drag_state = ReorderDragState::default();
        let mut rename = LayerRenameState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                LayersPanel::show(
                    ui,
                    &photo_ctx,
                    &[],
                    LayerTreeSelection::default(),
                    &mut rename,
                    &mut drag_state,
                    &std::collections::HashMap::new(),
                    &[],
                );
            });
        })
        .drop_without_applying_deltas();
    }

    fn flat_layer(id: Uuid, depth: usize, nestable: bool) -> FlatRow {
        FlatRow {
            id,
            kind: FlatRowKind::Layer,
            depth,
            parent: None,
            nestable,
        }
    }

    #[test]
    fn convert_drop_maps_before_after_into() {
        use super::super::types::snapshot_layers;
        use cygnus_ui::utils::DropPosition;
        let dragged = Uuid::new_v4();
        let target = Uuid::new_v4();
        // Before / after between layers (same parent or not).
        for (position, before) in [(DropPosition::Before, true), (DropPosition::After, false)] {
            let action = convert_drop(
                &[],
                LayerDrop {
                    dragged: flat_layer(dragged, 0, false),
                    target: flat_layer(target, 0, false),
                    position,
                },
            );
            assert_eq!(
                action,
                Some(LayerPanelAction::ReorderNodes {
                    dragged,
                    target,
                    before,
                })
            );
        }
        // Inside an expanded snapshot group: nesting.
        let mut doc = photo_engine::Document::new(8, 8);
        let inner = photo_engine::LayerNode::Pixel(photo_engine::PixelLayer::new(
            "d",
            std::sync::Arc::new(image::DynamicImage::new_rgba8(4, 4)),
        ));
        doc.push_layer(photo_engine::LayerNode::Group(
            photo_engine::GroupLayer::new("g", vec![inner]),
        ));
        let layers = snapshot_layers(&doc);
        let group_id = layers[0].id;
        let action = convert_drop(
            &layers,
            LayerDrop {
                dragged: flat_layer(dragged, 0, false),
                target: flat_layer(group_id, 0, true),
                position: DropPosition::Into,
            },
        );
        assert_eq!(
            action,
            Some(LayerPanelAction::MoveIntoParent {
                layer: dragged,
                parent: group_id,
            })
        );
        // Inside a pixel layer: clipped child (Affinity-style nesting).
        let pixel_id = layers[0].children[0].id;
        assert_eq!(
            convert_drop(
                &layers,
                LayerDrop {
                    dragged: flat_layer(dragged, 0, false),
                    target: flat_layer(pixel_id, 1, false),
                    position: DropPosition::Into,
                },
            ),
            Some(LayerPanelAction::MoveIntoParent {
                layer: dragged,
                parent: pixel_id,
            })
        );
        // Dragged filter onto a layer row: exact chain placement.
        let mut doc2 = photo_engine::Document::new(8, 8);
        doc2.push_layer(photo_engine::LayerNode::Pixel(
            photo_engine::PixelLayer::new(
                "a",
                std::sync::Arc::new(image::DynamicImage::new_rgba8(4, 4)),
            ),
        ));
        doc2.push_layer(photo_engine::LayerNode::Pixel(
            photo_engine::PixelLayer::new(
                "b",
                std::sync::Arc::new(image::DynamicImage::new_rgba8(4, 4)),
            ),
        ));
        let layers2 = snapshot_layers(&doc2);
        // Top of stack first: layers2[0] = "b", layers2[1] = "a".
        let (bid, aid) = (layers2[0].id, layers2[1].id);
        let fid = uuid::Uuid::new_v4();
        // Before a layer row: index 0 of its chain.
        assert_eq!(
            convert_drop(
                &layers2,
                LayerDrop {
                    dragged: FlatRow {
                        id: fid,
                        kind: FlatRowKind::Filter { owner: aid },
                        depth: 1,
                        parent: Some(aid),
                        nestable: false,
                    },
                    target: flat_layer(bid, 0, false),
                    position: DropPosition::Before,
                },
            ),
            Some(LayerPanelAction::MoveFilterTo {
                filter: fid,
                from: aid,
                to: bid,
                index: 0,
            })
        );
        // Drop onto itself: ignored.
        assert_eq!(
            convert_drop(
                &layers2,
                LayerDrop {
                    dragged: flat_layer(bid, 0, false),
                    target: flat_layer(bid, 0, false),
                    position: DropPosition::After,
                },
            ),
            None
        );
    }
}
