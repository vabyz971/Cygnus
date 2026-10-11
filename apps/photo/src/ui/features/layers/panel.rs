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
    FlatRowKind, LayerDrop, LayerTexts, LayerThumbView, LayerTreeSelection, PhotoLayerInfo,
    find_layer_in,
};
use crate::commands::PhotoUiContext;
use photo_engine::BlendMode;
use std::collections::HashMap;
use ui_kit::components::{IconButton, Select, Slider, menu_row, menu_style};
use ui_kit::icons::Icon;
use ui_kit::primitives::Text;
use ui_kit::theme::CygnusTheme;
use ui_kit::utils::{DropPosition, ReorderDragState};
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
    /// Nest a layer at a group's head (drop inside).
    MoveIntoGroup {
        /// Moved layer.
        layer: Uuid,
        /// Host group.
        group: Uuid,
    },
    /// Collapse / expand a group (no re-render).
    ToggleCollapsed(Uuid),
    /// Toggle a layer's visibility (eye).
    ToggleVisibility(Uuid),
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
    /// Move a filter within its stack.
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Move a mask within its stack.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
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

/// Affinity-style header: opacity (worker-coalesced slider) +
/// the selection's blend mode. Without a selection, controls are
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
    let Some(layer) = selected.and_then(|id| layers.iter().find(|item| item.id == id)) else {
        if layers.is_empty() {
            Text::caption(theme, texts.get(crate::i18n::PhotoTextKey::NoLayers)).show(ui);
        } else {
            // Inert but visible controls (no targeted layer).
            ui.add_enabled_ui(false, |ui| {
                let mut opacity = 100.0;
                Slider::new(
                    texts.get(crate::i18n::PhotoTextKey::OpacityLabel),
                    0.0..=100.0,
                )
                .show(ui, theme, &mut opacity);
                let options: [&str; 6] = BlendMode::ALL.map(BlendMode::label);
                let mut choice = 0;
                Select::new(texts.get(crate::i18n::PhotoTextKey::BlendLabel), &options).show(
                    ui,
                    theme,
                    &mut choice,
                );
            });
        }
        ui.separator();
        return actions;
    };
    // Engine units 0..=100 (like the inspector).
    let mut opacity = layer.opacity;
    Slider::new(
        texts.get(crate::i18n::PhotoTextKey::OpacityLabel),
        0.0..=100.0,
    )
    .show(ui, theme, &mut opacity);
    if opacity != layer.opacity {
        actions.push(LayerPanelAction::SetOpacity {
            layer: layer.id,
            opacity,
        });
    }
    let options: [&str; 6] = BlendMode::ALL.map(BlendMode::label);
    let mut choice = BlendMode::ALL
        .iter()
        .position(|mode| *mode == layer.blend_mode)
        .unwrap_or(0);
    Select::new(texts.get(crate::i18n::PhotoTextKey::BlendLabel), &options).show(
        ui,
        theme,
        &mut choice,
    );
    if let Some(mode) = BlendMode::ALL.get(choice)
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

/// Converts a hierarchical drop into a panel action: before/after
/// between layers ⇒ `ReorderNodes` (same parent or not), inside an
/// expanded group ⇒ `MoveIntoGroup`. Attachments do not
/// move via DnD (`None`: their order is set with their row
/// buttons), nor does a drop inside a non-group
/// (guard: `can_nest` already forbids it list-side).
fn convert_drop(layers: &[PhotoLayerInfo], drop: LayerDrop) -> Option<LayerPanelAction> {
    if drop.dragged.kind != FlatRowKind::Layer {
        return None;
    }
    match (drop.target.kind, drop.position) {
        (FlatRowKind::Layer, DropPosition::Before) => Some(LayerPanelAction::ReorderNodes {
            dragged: drop.dragged.id,
            target: drop.target.id,
            before: true,
        }),
        (FlatRowKind::Layer, DropPosition::After) => Some(LayerPanelAction::ReorderNodes {
            dragged: drop.dragged.id,
            target: drop.target.id,
            before: false,
        }),
        (FlatRowKind::Layer, DropPosition::Into) => {
            let groupe = find_layer_in(layers, drop.target.id)?;
            (groupe.kind == super::types::PhotoLayerKind::Group).then_some(
                LayerPanelAction::MoveIntoGroup {
                    layer: drop.dragged.id,
                    group: drop.target.id,
                },
            )
        }
        _ => None,
    }
}

/// Header + bounded list (reserves the button bar) + bottom bar.
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
        texts: crate::i18n::PhotoCatalog::new(translator.language()),
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
            LayerItemAction::MoveFilter { layer, filter, up } => {
                actions.push(LayerPanelAction::MoveFilter { layer, filter, up });
            }
            LayerItemAction::MoveMask { owner, mask, up } => {
                actions.push(LayerPanelAction::MoveMask { owner, mask, up });
            }
            LayerItemAction::RemoveFilter { layer, filter } => {
                actions.push(LayerPanelAction::RemoveFilter { layer, filter });
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
            .tooltip(texts.get(crate::i18n::PhotoTextKey::LayerFromImage))
            .show(ui, theme)
            .clicked()
        {
            actions.push(LayerPanelAction::AddImage);
        }
        if IconButton::new(Icon::Add)
            .tooltip(texts.get(crate::i18n::PhotoTextKey::NewEmptyLayer))
            .show(ui, theme)
            .clicked()
        {
            actions.push(LayerPanelAction::AddEmpty);
        }
        ui.add_enabled_ui(sel.selected.is_some(), |ui| {
            if IconButton::new(Icon::Duplicate)
                .tooltip(texts.get(crate::i18n::PhotoTextKey::DuplicateLayer))
                .show(ui, theme)
                .clicked()
            {
                actions.push(LayerPanelAction::Duplicate);
            }
            if IconButton::new(Icon::Mask)
                .tooltip(texts.get(crate::i18n::PhotoTextKey::AddMask))
                .show(ui, theme)
                .clicked()
            {
                actions.push(LayerPanelAction::AddMask);
            }
            // Filter-add context menu (Affinity-style: no
            // modal, the catalog opens at the button).
            let filter_resp = IconButton::new(Icon::Settings)
                .tooltip(texts.get(crate::i18n::PhotoTextKey::FilterList))
                .show(ui, theme);
            egui::Popup::menu(&filter_resp)
                .style(menu_style(theme))
                .show(|ui| {
                    for (name, type_id) in filter_types {
                        if menu_row(ui, theme, name) {
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
                .tooltip(texts.get(crate::i18n::PhotoTextKey::DeleteLayer))
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
        ui_kit::theme::setup_fonts(&ctx);
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
        ui_kit::theme::setup_fonts(&ctx);
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
        ui_kit::theme::setup_fonts(&ctx);
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
            nestable,
        }
    }

    #[test]
    fn convert_drop_maps_before_after_into() {
        use super::super::types::snapshot_layers;
        use ui_kit::utils::DropPosition;
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
            Some(LayerPanelAction::MoveIntoGroup {
                layer: dragged,
                group: group_id,
            })
        );
        // Inside a pixel layer: refused (guard).
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
            None
        );
        // Moved attachment: refused (order via buttons).
        assert_eq!(
            convert_drop(
                &layers,
                LayerDrop {
                    dragged: FlatRow {
                        id: Uuid::new_v4(),
                        kind: FlatRowKind::Filter { owner: group_id },
                        depth: 1,
                        nestable: false,
                    },
                    target: flat_layer(group_id, 0, true),
                    position: DropPosition::Before,
                },
            ),
            None
        );
    }
}
