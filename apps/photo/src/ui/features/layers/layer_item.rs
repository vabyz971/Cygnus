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

//! Photo layer HUD (Layers panel row, domain widget).
//!
//! Single centered row (`Align::Center`): collapse/expand
//! chevron, visibility button, layer viewer, name
//! editable on double-click (Affinity-style: no destructive
//! action per row — delete via the context menu or the
//! bottom bar). Live
//! filters and masks are full child rows
//! ([`draw_attachment_row`]), indented under their owner.
//! NO engine send here: every interaction bubbles via
//! [`LayerItemAction`], converted to [`PhotoAction`](crate::commands::PhotoAction)
//! then routed by `PhotoApp` (local state or worker).

use super::types::{LayerTexts, LayerThumbView, PhotoLayerInfo, PhotoSubLayerInfo};
use crate::i18n::PhotoTextKey;
use ui_kit::components::{IconButton, menu_row, menu_style};
use ui_kit::i18n::TextKey;
use ui_kit::icons::{Icon, IconRegistry};
use ui_kit::theme::UiThemeExt;
use uuid::Uuid;

/// Inline rename state (double-click on the name, owned by the app).
#[derive(Debug, Clone, Default)]
pub struct LayerRenameState {
    /// Layer being edited (`None` = none).
    pub editing: Option<Uuid>,
    /// Input field buffer.
    pub buffer: String,
}

/// Action bubbled to the layout then converted to
/// [`PhotoAction`](crate::commands::PhotoAction).
///
/// Selection and rename touch the app-owned UI state; everything
/// else is routed to the worker by `PhotoApp::handle_action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerItemAction {
    /// Select this layer.
    Select(Uuid),
    /// Select an attachment (filter or mask): the owner
    /// becomes the primary selection (tree, O003+).
    SelectAttachment {
        /// Owning layer.
        owner: Uuid,
        /// Targeted filter or mask.
        id: Uuid,
    },
    /// Enable/disable a live filter (row eye).
    ToggleFilter {
        /// Owning layer.
        layer: Uuid,
        /// Targeted filter.
        filter: Uuid,
    },
    /// Enable/disable a mask (row eye).
    ToggleMask {
        /// Owner.
        owner: Uuid,
        /// Targeted mask.
        mask: Uuid,
    },
    /// Confirm the rename (name already trimmed, non-empty).
    RenameCommit { layer: Uuid, name: String },
    /// Toggle visibility.
    ToggleVisibility(Uuid),
    /// Delete this layer.
    DeleteLayer(Uuid),
    /// Duplicate this layer.
    DuplicateLayer(Uuid),
    /// Horizontal mirror (context menu: acts on the selection,
    /// set by the right click).
    FlipHorizontalSelected,
    /// Vertical mirror (same).
    FlipVerticalSelected,
    /// 90° clockwise rotation (same).
    RotateClockwiseSelected,
    /// 90° counterclockwise rotation (same).
    RotateCounterclockwiseSelected,
    /// Crop to document (same).
    CropSelectedToDocument,
    /// Collapse / expand a group (engine state, no re-render).
    ToggleCollapsed(Uuid),
    /// Move a filter within its stack.
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Move a mask within its stack.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Delete a filter.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Delete a mask.
    RemoveMask { owner: Uuid, mask: Uuid },
}

/// Collapsed/expanded state of a layer's attachments (tree):
/// egui memory, open by default. Read by the list (row filtering)
/// and toggled by the owner row's chevron.
pub fn attachments_open(ui: &egui::Ui, owner: Uuid) -> bool {
    let id = ui.make_persistent_id(("photo_attachments_open", owner));
    egui::collapsing_header::CollapsingState::load(ui.ctx(), id)
        .map(|state| state.is_open())
        .unwrap_or(true)
}

/// Toggles the state above (stored even on first click: `load`
/// alone yields `None` with no persisted state, which would freeze the chevron).
pub fn set_attachments_open(ui: &egui::Ui, owner: Uuid, open: bool) {
    let id = ui.make_persistent_id(("photo_attachments_open", owner));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    if state.is_open() != open {
        state.toggle(ui);
        state.store(ui.ctx());
    }
}

/// Draws a layer row's HUD content.
///
/// Every icon goes through `Icon` (via [`IconButton`]). The row is
/// `Align::Center`: eye, thumbnail, and name are centered
/// vertically (never overlapping: height is allocated
/// by the list, see `layer_list`). Returns the actions, handled
/// by the app (never a direct worker send).
#[allow(clippy::too_many_lines)]
pub fn draw_photo_layer_item(
    ui: &mut egui::Ui,
    layer: &PhotoLayerInfo,
    selected: bool,
    rename: &mut LayerRenameState,
    thumb: Option<LayerThumbView>,
    depth: usize,
    t: LayerTexts,
) -> Vec<LayerItemAction> {
    let texts = t.texts;
    let catalog = t.catalog;
    let theme = ui.cygnus_theme();
    let mut actions = Vec::new();

    // Row interaction (click = select, double-click on the
    // name = rename) — registered BEFORE the content to paint the
    // selected/hovered background with no frame delay.
    let row_rect = ui.available_rect_before_wrap();
    let row_id = ui.make_persistent_id(("photo_layer_row", layer.id));
    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());
    if row_resp.clicked() {
        actions.push(LayerItemAction::Select(layer.id));
    }
    // Right click: selects the row and opens the
    // context menu (rerun-style sections: edit, visibility).
    if row_resp.secondary_clicked() {
        actions.push(LayerItemAction::Select(layer.id));
    }
    egui::Popup::context_menu(&row_resp)
        .style(menu_style(&theme))
        .show(|ui| {
            if menu_row(ui, &theme, catalog.get(TextKey::Rename)) {
                rename.editing = Some(layer.id);
                rename.buffer.clone_from(&layer.name);
                ui.close();
            }
            if menu_row(ui, &theme, catalog.get(TextKey::Duplicate)) {
                actions.push(LayerItemAction::DuplicateLayer(layer.id));
                ui.close();
            }
            // Layer transformations (selection already set by the
            // right click): menu-bar-style submenu.
            ui.menu_button(texts.get(PhotoTextKey::Transform), |ui| {
                if menu_row(ui, &theme, texts.get(PhotoTextKey::FlipHorizontal)) {
                    actions.push(LayerItemAction::FlipHorizontalSelected);
                    ui.close();
                }
                if menu_row(ui, &theme, texts.get(PhotoTextKey::FlipVertical)) {
                    actions.push(LayerItemAction::FlipVerticalSelected);
                    ui.close();
                }
                if menu_row(ui, &theme, texts.get(PhotoTextKey::RotateClockwise)) {
                    actions.push(LayerItemAction::RotateClockwiseSelected);
                    ui.close();
                }
                if menu_row(ui, &theme, texts.get(PhotoTextKey::RotateCounterclockwise)) {
                    actions.push(LayerItemAction::RotateCounterclockwiseSelected);
                    ui.close();
                }
                if menu_row(ui, &theme, texts.get(PhotoTextKey::CropToDocument)) {
                    actions.push(LayerItemAction::CropSelectedToDocument);
                    ui.close();
                }
            });
            ui.separator();
            let visibility_label = if layer.visible {
                catalog.get(TextKey::Hide)
            } else {
                catalog.get(TextKey::Show)
            };
            if menu_row(ui, &theme, visibility_label) {
                actions.push(LayerItemAction::ToggleVisibility(layer.id));
                ui.close();
            }
            if menu_row(ui, &theme, catalog.get(TextKey::Delete)) {
                actions.push(LayerItemAction::DeleteLayer(layer.id));
                ui.close();
            }
        });

    // Selection background across the full row width.
    if selected || row_resp.hovered() {
        let bg = if selected {
            theme.colors.item_selected
        } else {
            theme.colors.item_hover
        };
        ui.painter().rect_filled(row_rect, theme.radius.sm, bg);
    }

    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Hierarchical indent (one notch per level).
        ui.add_space(depth as f32 * theme.spacing.lg);
        // Collapse/expand chevron (groups with children) or
        // equivalent space to align columns.
        if layer.kind == super::types::PhotoLayerKind::Group && !layer.children.is_empty() {
            let chevron = if layer.collapsed {
                Icon::ExpandMore
            } else {
                Icon::ExpandLess
            };
            if IconButton::new(chevron)
                .tooltip(if layer.collapsed {
                    texts.get(PhotoTextKey::ExpandGroup)
                } else {
                    texts.get(PhotoTextKey::CollapseGroup)
                })
                .show(ui, &theme)
                .clicked()
            {
                actions.push(LayerItemAction::ToggleCollapsed(layer.id));
            }
        } else {
            ui.add_space(theme.sizes.icon_button);
        }
        // Attachment chevron (egui-demo-style tree): collapse /
        // expand of the filter + mask rows under their owner.
        if layer.has_filters || layer.has_masks {
            let open = attachments_open(ui, layer.id);
            let chevron = if open {
                Icon::ExpandLess
            } else {
                Icon::ExpandMore
            };
            if IconButton::new(chevron)
                .tooltip(if open {
                    texts.get(PhotoTextKey::CollapseAttachments)
                } else {
                    texts.get(PhotoTextKey::ExpandAttachments)
                })
                .show(ui, &theme)
                .clicked()
            {
                set_attachments_open(ui, layer.id, !open);
            }
        } else {
            ui.add_space(theme.sizes.icon_button);
        }
        let vis_icon = if layer.visible {
            Icon::Visibility
        } else {
            Icon::VisibilityOff
        };
        if IconButton::new(vis_icon)
            .tooltip(texts.get(PhotoTextKey::ShowHideLayer))
            .show(ui, &theme)
            .clicked()
        {
            actions.push(LayerItemAction::ToggleVisibility(layer.id));
        }
        // Layer viewer: pixel thumbnail when available,
        // else symbolic vignette (framed type icon).
        if let Some(view) = thumb {
            ui.add(
                egui::Image::new(egui::load::SizedTexture::new(view.texture_id, view.size))
                    .fit_to_exact_size(egui::vec2(
                        theme.sizes.layer_thumb,
                        theme.sizes.layer_thumb,
                    )),
            );
        } else {
            ui.add_sized(
                egui::vec2(theme.sizes.layer_thumb, theme.sizes.layer_thumb),
                egui::Button::new(
                    IconRegistry::new().sized(layer.kind.icon(), theme.typography.icon_size),
                )
                .frame(true),
            );
        }
        ui.add_space(theme.spacing.xs);
        // Name: double-click = inline edit, Enter = confirm,
        // Escape = cancel, focus loss = confirm when modified.
        if rename.editing == Some(layer.id) {
            let edit = egui::TextEdit::singleline(&mut rename.buffer)
                .desired_width(ui.available_width() - 40.0)
                .show(ui);
            if edit.response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape))
            {
                rename.editing = None;
            } else if edit.response.lost_focus()
                || ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                let name = rename.buffer.trim().to_owned();
                rename.editing = None;
                if !name.is_empty() && name != layer.name {
                    actions.push(LayerItemAction::RenameCommit {
                        layer: layer.id,
                        name,
                    });
                }
            }
            // Immediate focus when editing opens.
            edit.response.request_focus();
        } else {
            let name_resp = ui.add(
                egui::Label::new(egui::RichText::new(&layer.name).size(theme.typography.body_size))
                    .truncate(),
            );
            if name_resp.double_clicked() {
                rename.editing = Some(layer.id);
                rename.buffer = layer.name.clone();
            }
        }
        // No delete button per row (Affinity-style:
        // delete via the context menu or the bottom bar).
    });
    actions
}

/// Child row of an attachment (live filter or mask): selectable
/// tree node (click = focus for editing, see
/// [`LayerItemAction::SelectAttachment`]), on/off eye, indented under
/// its owner. Returns the actions (routed to the worker by the app).
/// Neither renamable nor DnD-movable (order via the
/// up/down buttons, as before).
#[allow(clippy::too_many_arguments)]
pub fn draw_attachment_row(
    ui: &mut egui::Ui,
    owner: Uuid,
    sub: &PhotoSubLayerInfo,
    selected: bool,
    depth: usize,
    t: LayerTexts,
) -> Vec<LayerItemAction> {
    let theme = ui.cygnus_theme();
    let texts = t.texts;
    let mut actions = Vec::new();
    let sub_id = sub.id;
    let is_filter = sub.is_filter;
    let enabled = sub.enabled;
    let name = sub.name.as_str();
    // Click = focus (secondary selection, owner selected too).
    let row_rect = ui.available_rect_before_wrap();
    let row_id = ui.make_persistent_id(("photo_attachment_row", owner, sub_id));
    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());
    if row_resp.clicked() {
        actions.push(LayerItemAction::SelectAttachment { owner, id: sub_id });
    }
    // Selection background across the full row width.
    if selected || row_resp.hovered() {
        let bg = if selected {
            theme.colors.item_selected
        } else {
            theme.colors.item_hover
        };
        ui.painter().rect_filled(row_rect, theme.radius.sm, bg);
    }
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(depth as f32 * theme.spacing.lg);
        // Chevron column (empty here: aligned with layers).
        ui.add_space(theme.sizes.icon_button);
        // Attachment chevron column (empty: aligned).
        ui.add_space(theme.sizes.icon_button);
        // On/off eye (toggles without losing settings or pixels).
        let vis_icon = if enabled {
            Icon::Visibility
        } else {
            Icon::VisibilityOff
        };
        if IconButton::new(vis_icon)
            .tooltip(if is_filter {
                texts.get(PhotoTextKey::ToggleFilterEnabled)
            } else {
                texts.get(PhotoTextKey::ToggleMaskEnabled)
            })
            .show(ui, &theme)
            .clicked()
        {
            if is_filter {
                actions.push(LayerItemAction::ToggleFilter {
                    layer: owner,
                    filter: sub_id,
                });
            } else {
                actions.push(LayerItemAction::ToggleMask {
                    owner,
                    mask: sub_id,
                });
            }
        }
        if is_filter {
            ui.label(
                egui::RichText::new("FX")
                    .size(theme.typography.caption_size)
                    .color(theme.colors.accent),
            );
        } else {
            ui.label(IconRegistry::new().colored(
                Icon::Mask,
                theme.typography.caption_size,
                theme.colors.fg_secondary,
            ));
        }
        ui.add_space(theme.spacing.xs);
        ui.add(
            egui::Label::new(
                egui::RichText::new(name)
                    .size(theme.typography.caption_size)
                    .color(theme.colors.fg_secondary),
            )
            .truncate(),
        );
        let (up_tip, down_tip, del_tip) = if is_filter {
            (
                texts.get(PhotoTextKey::MoveFilterUp),
                texts.get(PhotoTextKey::MoveFilterDown),
                texts.get(PhotoTextKey::DeleteFilter),
            )
        } else {
            (
                texts.get(PhotoTextKey::MoveMaskUp),
                texts.get(PhotoTextKey::MoveMaskDown),
                texts.get(PhotoTextKey::DeleteMask),
            )
        };
        // Up/down: no arrows in the icon registry —
        // small ASCII text buttons ("^"/"v"), never unicode.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if IconButton::new(Icon::Close)
                .tooltip(del_tip)
                .show(ui, &theme)
                .clicked()
            {
                if is_filter {
                    actions.push(LayerItemAction::RemoveFilter {
                        layer: owner,
                        filter: sub_id,
                    });
                } else {
                    actions.push(LayerItemAction::RemoveMask {
                        owner,
                        mask: sub_id,
                    });
                }
            }
            if ui.small_button("v").on_hover_text(down_tip).clicked() {
                if is_filter {
                    actions.push(LayerItemAction::MoveFilter {
                        layer: owner,
                        filter: sub_id,
                        up: false,
                    });
                } else {
                    actions.push(LayerItemAction::MoveMask {
                        owner,
                        mask: sub_id,
                        up: false,
                    });
                }
            }
            if ui.small_button("^").on_hover_text(up_tip).clicked() {
                if is_filter {
                    actions.push(LayerItemAction::MoveFilter {
                        layer: owner,
                        filter: sub_id,
                        up: true,
                    });
                } else {
                    actions.push(LayerItemAction::MoveMask {
                        owner,
                        mask: sub_id,
                        up: true,
                    });
                }
            }
        });
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::features::layers::types::PhotoLayerKind;
    use photo_engine::{Document, LayerNode, PixelLayer};
    use std::sync::Arc;

    fn fixture_layer() -> PhotoLayerInfo {
        let mut doc = Document::new(8, 8);
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", image)));
        super::super::types::snapshot_layers(&doc)
            .pop()
            .expect("un calque")
    }

    #[test]
    fn item_renders_without_panic() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let texts = super::super::types::LayerTexts {
            catalog: ui_kit::i18n::Catalog::new(ui_kit::i18n::Language::Fr),
            texts: crate::i18n::PhotoCatalog::new(ui_kit::i18n::Language::Fr),
        };
        let layer = fixture_layer();
        let mut rename = LayerRenameState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let actions = draw_photo_layer_item(ui, &layer, true, &mut rename, None, 0, texts);
                assert!(actions.is_empty(), "aucun clic sans interaction");
                // All types, hidden / visible.
                for kind in [
                    PhotoLayerKind::Pixel,
                    PhotoLayerKind::Group,
                    PhotoLayerKind::Adjustment,
                ] {
                    let mut probing = layer.clone();
                    probing.kind = kind;
                    probing.visible = false;
                    let _ = draw_photo_layer_item(ui, &probing, false, &mut rename, None, 1, texts);
                }
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn item_without_interaction_reports_nothing() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let texts = super::super::types::LayerTexts {
            catalog: ui_kit::i18n::Catalog::new(ui_kit::i18n::Language::Fr),
            texts: crate::i18n::PhotoCatalog::new(ui_kit::i18n::Language::Fr),
        };
        let layer = fixture_layer();
        let mut rename = LayerRenameState::default();
        let mut reported = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = draw_photo_layer_item(ui, &layer, false, &mut rename, None, 0, texts);
            });
        })
        .drop_without_applying_deltas();
        assert!(reported.is_empty(), "aucune action attendue");
    }

    #[test]
    fn rename_state_defaults_to_idle() {
        let state = LayerRenameState::default();
        assert_eq!(state.editing, None);
        assert!(state.buffer.is_empty());
    }

    #[test]
    fn attachments_collapse_roundtrip() {
        use uuid::Uuid;
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let owner = Uuid::new_v4();
        // Open by default, even with no stored state.
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert!(attachments_open(ui, owner));
            });
        })
        .drop_without_applying_deltas();
        // First click: really toggles (regression: `load` alone
        // yielded `None` and froze the chevron).
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                set_attachments_open(ui, owner, false);
            });
        })
        .drop_without_applying_deltas();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert!(!attachments_open(ui, owner));
                set_attachments_open(ui, owner, true);
            });
        })
        .drop_without_applying_deltas();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert!(attachments_open(ui, owner));
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn attachment_rows_render_without_panic_and_report_nothing() {
        use uuid::Uuid;
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let texts = super::super::types::LayerTexts {
            catalog: ui_kit::i18n::Catalog::new(ui_kit::i18n::Language::Fr),
            texts: crate::i18n::PhotoCatalog::new(ui_kit::i18n::Language::Fr),
        };
        let owner = Uuid::new_v4();
        let filter = Uuid::new_v4();
        let mask = Uuid::new_v4();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let fx = draw_attachment_row(
                    ui,
                    owner,
                    &PhotoSubLayerInfo {
                        id: filter,
                        name: String::from("luminosite"),
                        is_filter: true,
                        enabled: true,
                        filter_type: String::from("brightness_contrast"),
                        params: std::collections::HashMap::from([
                            (String::from("brightness"), 0.0),
                            (String::from("contrast"), 0.0),
                        ]),
                    },
                    false,
                    1,
                    texts,
                );
                assert!(fx.is_empty(), "aucun clic sans interaction");
                let mk = draw_attachment_row(
                    ui,
                    owner,
                    &PhotoSubLayerInfo {
                        id: mask,
                        name: String::from("masque 1"),
                        is_filter: false,
                        enabled: false,
                        filter_type: String::new(),
                        params: std::collections::HashMap::new(),
                    },
                    false,
                    1,
                    texts,
                );
                assert!(mk.is_empty(), "aucun clic sans interaction");
            });
        })
        .drop_without_applying_deltas();
    }
}
