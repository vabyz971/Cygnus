// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Layers panel (CAL-4, CAL-6, CAL-7, docs/decisions/0025): the tree of the
//! document, top layer first; every change is a command of the bus.

use cygnus_photo_core::adjust::{
    Adjustment, BrightnessContrast, ColorBalance, Curves, HueSaturation, Levels,
};
use cygnus_photo_core::commands::{
    LayerCreate, LayerDelete, LayerDuplicate, LayerGroup, LayerKind, LayerMergeDown, LayerMove,
    LayerUngroup, LayerUpdate, MaskCreate, MaskDelete, MaskUpdate,
};
use cygnus_photo_core::resolve::Resolvers;
use cygnus_photo_core::{Document, Filter, Isolation, Node, NodeId, NodeKind, PhotoCommand};
use cygnus_ui_kit::i18n::{Key, tr, tr_format};
use cygnus_ui_kit::icons;
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::widgets;
use eframe::egui_wgpu::RenderState;
use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Ui};

use crate::canvas::{self, Session};
use crate::keys;
use crate::tools::{self, percent};
use crate::view::ViewState;

/// Translation key of an adjustment.
pub fn adjustment_key(adjustment: &Adjustment) -> Key {
    match adjustment {
        Adjustment::Levels(_) => keys::ADJUSTMENT_LEVELS,
        Adjustment::Curves(_) => keys::ADJUSTMENT_CURVES,
        Adjustment::HueSaturation(_) => keys::ADJUSTMENT_HUE_SATURATION,
        Adjustment::BrightnessContrast(_) => keys::ADJUSTMENT_BRIGHTNESS_CONTRAST,
        Adjustment::ColorBalance(_) => keys::ADJUSTMENT_COLOR_BALANCE,
    }
}

/// The five adjustments with their default parameters (identity).
pub fn new_adjustments() -> [Adjustment; 5] {
    [
        Adjustment::Levels(Levels::default()),
        Adjustment::Curves(Curves::default()),
        Adjustment::HueSaturation(HueSaturation::default()),
        Adjustment::BrightnessContrast(BrightnessContrast::default()),
        Adjustment::ColorBalance(ColorBalance::default()),
    ]
}

fn kind_key(node: &Node) -> Key {
    match &node.kind {
        NodeKind::Pixel { .. } => keys::LAYER_KIND_PIXEL,
        NodeKind::Group {
            isolation: Isolation::Isolated,
            ..
        } => keys::LAYER_KIND_GROUP,
        NodeKind::Group { .. } => keys::LAYER_KIND_PASS_THROUGH,
        NodeKind::Embedded { .. } => keys::LAYER_KIND_EMBEDDED,
        NodeKind::Adjustment { adjustment } => adjustment_key(adjustment),
        NodeKind::Filter {
            filter: Filter::GaussianBlur { .. },
        } => keys::FILTERS_GAUSSIAN_BLUR,
        NodeKind::Filter { .. } => keys::FILTERS_UNSHARP_MASK,
        NodeKind::Mask { .. } => keys::LAYERS_MASK,
    }
}

fn kind_icon(node: &Node) -> &'static str {
    match &node.kind {
        NodeKind::Pixel { .. } => icons::SQUARE,
        NodeKind::Group { .. } => icons::FOLDER_SIMPLE,
        NodeKind::Embedded { .. } => icons::IMAGE,
        NodeKind::Adjustment {
            adjustment: Adjustment::Curves(_),
        } => icons::BEZIER_CURVE,
        NodeKind::Adjustment { .. } => icons::SLIDERS,
        NodeKind::Filter { .. } => icons::SPARKLE,
        NodeKind::Mask { .. } => icons::SQUARE_HALF_BOTTOM,
    }
}

fn update(id: NodeId, change: impl FnOnce(&mut LayerUpdate)) -> PhotoCommand {
    let mut params = LayerUpdate::new(id);
    change(&mut params);
    PhotoCommand::LayerUpdate(params)
}

/// Group and index where a new layer goes: just above the selection.
fn insertion(document: &Document, view: &ViewState) -> (Option<NodeId>, Option<usize>) {
    let Some(selected) = view.selected.and_then(|id| document.node(id)) else {
        return (None, None);
    };
    let Some(parent) = selected.parent else {
        return (None, None);
    };
    let index = document
        .children(parent)
        .iter()
        .position(|id| *id == selected.id)
        .map(|i| i + 1);
    (Some(parent), index)
}

fn create(document: &Document, view: &ViewState, name: String, kind: LayerKind) -> PhotoCommand {
    let (parent, index) = insertion(document, view);
    PhotoCommand::LayerCreate(LayerCreate {
        name,
        parent,
        index,
        kind,
    })
}

/// Runs a command of the user, giving the renderer of the canvas to
/// `layer.merge_down`.
pub fn run(session: &mut Session, render_state: Option<&RenderState>, command: PhotoCommand) {
    let mut pending = Some(command);
    if matches!(pending, Some(PhotoCommand::LayerMergeDown(_)))
        && let Some(render_state) = render_state
    {
        canvas::with_renderer(render_state, |renderer| {
            let resolvers = Resolvers {
                import: None,
                merge: Some(renderer),
            };
            if let Some(command) = pending.take() {
                session.user_with(command, &resolvers);
            }
        });
    }
    if let Some(command) = pending {
        session.user(command);
    }
}

/// Rows of the tree, top first: (node, depth).
fn rows(document: &Document, view: &ViewState) -> Vec<(NodeId, usize)> {
    fn walk(
        document: &Document,
        view: &ViewState,
        group: NodeId,
        depth: usize,
        out: &mut Vec<(NodeId, usize)>,
    ) {
        for id in document.children(group).iter().rev() {
            out.push((*id, depth));
            if !view.collapsed.contains(id) {
                walk(document, view, *id, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(document, view, document.root, 0, &mut out);
    out
}

/// Where a dropped layer goes when released on `target`: inside it at the
/// top for a group, just above it otherwise.
fn drop_place(document: &Document, dragged: NodeId, target: NodeId) -> Option<(NodeId, usize)> {
    let node = document.node(target)?;
    if matches!(node.kind, NodeKind::Group { .. }) {
        let len = document
            .children(target)
            .iter()
            .filter(|id| **id != dragged)
            .count();
        return Some((target, len));
    }
    let parent = node.parent?;
    let siblings: Vec<NodeId> = document
        .children(parent)
        .iter()
        .copied()
        .filter(|id| *id != dragged)
        .collect();
    let index = siblings.iter().position(|id| *id == target)?;
    Some((parent, index + 1))
}

/// The Layers panel.
pub fn panel(
    ui: &mut Ui,
    session: &mut Session,
    view: &mut ViewState,
    render_state: Option<&RenderState>,
) {
    view.follow(session.document());
    properties(ui, session, view);
    ui.add_space(4.0);

    let rows = rows(session.document(), view);
    if rows.is_empty() {
        ui.label(RichText::new(tr(keys::LAYERS_EMPTY)).color(palette::TEXT_MUTED));
    }
    let mut command: Option<PhotoCommand> = None;
    let mut menu_for: Option<NodeId> = None;
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - 36.0).max(60.0))
        .show(ui, |ui| {
            for (id, depth) in rows {
                if let Some(action) = row(ui, session, view, id, depth, &mut menu_for) {
                    command = Some(action);
                }
            }
        });
    if let Some(id) = menu_for {
        view.selected = Some(id);
    }
    if let Some(command) = command {
        run(session, render_state, command);
    }
    buttons(ui, session, view);
}

/// Blend mode and opacity of the selected layer; an opacity drag is one
/// gesture.
fn properties(ui: &mut Ui, session: &mut Session, view: &ViewState) {
    let Some(node) = view.selected.and_then(|id| session.document().node(id)) else {
        return;
    };
    let (id, mode, mut opacity) = (node.id, node.blend, node.opacity * 100.0);
    ui.horizontal(|ui| {
        if let Some(mode) = tools::blend_combo(ui, "layer-mode", None, mode, 110.0) {
            session.user(update(id, |p| p.blend = Some(mode)));
        }
        ui.label(RichText::new(tr(keys::LAYERS_OPACITY)).color(palette::TEXT_MUTED));
        ui.label(RichText::new(percent(opacity.round() as u32)).monospace());
    });
    let response = widgets::slider(ui, &mut opacity, 0.0..=100.0);
    if response.drag_started() {
        session.begin_gesture();
    }
    if response.changed() {
        session.user(update(id, |p| p.opacity = Some(opacity / 100.0)));
    }
    if response.drag_stopped() {
        session.end_gesture();
    }
}

/// One row of the tree; returns the command it asks for.
fn row(
    ui: &mut Ui,
    session: &mut Session,
    view: &mut ViewState,
    id: NodeId,
    depth: usize,
    menu_for: &mut Option<NodeId>,
) -> Option<PhotoCommand> {
    let document = session.document();
    let node = document.node(id)?.clone();
    let selected = view.selected == Some(id);
    let mut command = None;
    let frame = Frame::new()
        .fill(if selected {
            palette::SELECTED
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(CornerRadius::same(metrics::RADIUS))
        .inner_margin(Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0);
                if matches!(node.kind, NodeKind::Group { .. }) {
                    let folded = view.collapsed.contains(&id);
                    let caret = if folded {
                        icons::CARET_RIGHT
                    } else {
                        icons::CARET_DOWN
                    };
                    if widgets::icon_button(ui, caret, tr(keys::LAYERS_GROUP_TOGGLE)).clicked() {
                        if folded {
                            view.collapsed.remove(&id);
                        } else {
                            view.collapsed.insert(id);
                        }
                    }
                }
                let eye = if node.visible {
                    icons::EYE
                } else {
                    icons::EYE_SLASH
                };
                if widgets::icon_button(ui, eye, tr(keys::LAYERS_VISIBILITY)).clicked() {
                    command = Some(update(id, |p| p.visible = Some(!node.visible)));
                }
                if node.clip {
                    ui.label(
                        widgets::icon(icons::ARROW_ELBOW_DOWN_RIGHT, metrics::ICON - 4.0)
                            .color(palette::TEXT_MUTED),
                    );
                }
                Frame::new()
                    .stroke(Stroke::new(1.0, palette::BORDER))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        ui.label(
                            widgets::icon(kind_icon(&node), metrics::ICON - 2.0)
                                .color(palette::TEXT),
                        );
                    });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    match &mut view.renaming {
                        Some((renamed, text)) if *renamed == id => {
                            let edit = ui.text_edit_singleline(text);
                            edit.request_focus();
                            if edit.lost_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    && *text != node.name
                                {
                                    let name = text.clone();
                                    command = Some(update(id, |p| p.name = Some(name)));
                                }
                                view.renaming = None;
                            }
                        }
                        _ => {
                            let name = ui.add(
                                egui::Label::new(RichText::new(&node.name).color(palette::TEXT))
                                    .sense(Sense::click()),
                            );
                            if name.double_clicked() {
                                view.renaming = Some((id, node.name.clone()));
                            }
                            if name.clicked() {
                                view.selected = Some(id);
                                view.mask = false;
                            }
                        }
                    }
                    let kind = tr(kind_key(&node));
                    let kind = if node.clip {
                        tr_format(keys::LAYER_KIND_CLIPPED, &[("kind", kind)])
                    } else {
                        kind.to_owned()
                    };
                    ui.label(RichText::new(kind).small().color(palette::TEXT_MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let lock = if node.locked {
                        icons::LOCK_SIMPLE
                    } else {
                        icons::LOCK_SIMPLE_OPEN
                    };
                    if widgets::icon_button(ui, lock, tr(keys::LAYERS_LOCK)).clicked() {
                        command = Some(update(id, |p| p.locked = Some(!node.locked)));
                    }
                    if node.mask.is_some() {
                        let color = if selected && view.mask {
                            palette::ACCENT
                        } else {
                            palette::TEXT
                        };
                        let mask = ui
                            .add(
                                egui::Button::new(
                                    widgets::icon(icons::SQUARE_HALF_BOTTOM, metrics::ICON)
                                        .color(color),
                                )
                                .frame(false),
                            )
                            .on_hover_text(tr(keys::LAYERS_MASK));
                        if mask.clicked() {
                            view.selected = Some(id);
                            view.mask = true;
                        }
                    }
                });
            });
        });
    let response = frame.response.interact(Sense::click_and_drag());
    if response.clicked() {
        view.selected = Some(id);
        view.mask = false;
    }
    response.dnd_set_drag_payload(id);
    if response
        .dnd_hover_payload::<NodeId>()
        .is_some_and(|dragged| *dragged != id)
    {
        ui.painter().hline(
            response.rect.x_range(),
            response.rect.top(),
            Stroke::new(2.0, palette::ACCENT),
        );
    }
    if let Some(dragged) = response.dnd_release_payload::<NodeId>()
        && *dragged != id
        && let Some((parent, index)) = drop_place(session.document(), *dragged, id)
    {
        command = Some(PhotoCommand::LayerMove(LayerMove {
            id: *dragged,
            parent: Some(parent),
            index,
        }));
    }
    response.context_menu(|ui| {
        *menu_for = Some(id);
        // The menu picks its command; it runs after the tree is drawn.
        menu_entries(ui, session.document(), view, id, &mut |picked| {
            command = Some(picked)
        });
    });
    command
}

/// Entries of the layer menus; `emit` receives the command picked.
pub fn menu_entries(
    ui: &mut Ui,
    document: &Document,
    view: &mut ViewState,
    id: NodeId,
    emit: &mut dyn FnMut(PhotoCommand),
) {
    let Some(node) = document.node(id) else {
        return;
    };
    let is_group = matches!(node.kind, NodeKind::Group { .. });
    let (clip, mask, name) = (node.clip, node.mask, node.name.clone());
    let mask_inverted = mask
        .and_then(|m| document.node(m))
        .is_some_and(|m| matches!(m.kind, NodeKind::Mask { inverted: true, .. }));
    let mut pick = |ui: &mut Ui, enabled: bool, key: Key, command: Option<PhotoCommand>| {
        if ui
            .add_enabled(enabled, egui::Button::new(tr(key)))
            .clicked()
        {
            if let Some(command) = command {
                emit(command);
            }
            ui.close();
        }
    };
    pick(
        ui,
        true,
        keys::LAYER_DUPLICATE,
        Some(PhotoCommand::LayerDuplicate(LayerDuplicate { id })),
    );
    pick(
        ui,
        true,
        keys::LAYER_DELETE,
        Some(PhotoCommand::LayerDelete(LayerDelete { id })),
    );
    if ui.button(tr(keys::LAYER_RENAME)).clicked() {
        view.renaming = Some((id, name));
        ui.close();
    }
    ui.separator();
    let number = (document.nodes.len()).to_string();
    pick(
        ui,
        true,
        keys::LAYER_GROUP,
        Some(PhotoCommand::LayerGroup(LayerGroup {
            ids: vec![id],
            name: tr_format(keys::GROUP_DEFAULT_NAME, &[("number", &number)]),
            isolation: Isolation::default(),
        })),
    );
    pick(
        ui,
        is_group,
        keys::LAYER_UNGROUP,
        Some(PhotoCommand::LayerUngroup(LayerUngroup { id })),
    );
    pick(
        ui,
        true,
        keys::LAYER_MERGE_DOWN,
        Some(PhotoCommand::LayerMergeDown(LayerMergeDown {
            id,
            resolved: None,
        })),
    );
    let clip_key = if clip {
        keys::LAYER_UNCLIP
    } else {
        keys::LAYER_CLIP
    };
    pick(
        ui,
        true,
        clip_key,
        Some(update(id, |p| p.clip = Some(!clip))),
    );
    ui.separator();
    match mask {
        None => pick(
            ui,
            true,
            keys::MASK_ADD,
            Some(PhotoCommand::MaskCreate(MaskCreate {
                node: id,
                fill: Default::default(),
            })),
        ),
        Some(mask) => {
            pick(
                ui,
                true,
                keys::MASK_INVERT,
                Some(PhotoCommand::MaskUpdate(MaskUpdate {
                    id: mask,
                    density: None,
                    inverted: Some(!mask_inverted),
                    enabled: None,
                })),
            );
            pick(
                ui,
                true,
                keys::MASK_DELETE,
                Some(PhotoCommand::MaskDelete(MaskDelete { node: id })),
            );
        }
    }
    ui.separator();
    // The node graph is not part of the beta (CAL-7).
    pick(ui, false, keys::LAYER_SHOW_IN_GRAPH, None);
}

/// Buttons at the bottom of the panel.
fn buttons(ui: &mut Ui, session: &mut Session, view: &mut ViewState) {
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = metrics::SPACING * 1.5;
            let number = (session.document().nodes.len()).to_string();
            if widgets::icon_button(ui, icons::PLUS, tr(keys::LAYERS_ADD)).clicked() {
                let name = tr_format(keys::LAYER_DEFAULT_NAME, &[("number", &number)]);
                let command = create(session.document(), view, name, LayerKind::Pixel);
                session.user(command);
            }
            if widgets::icon_button(ui, icons::FOLDER_SIMPLE_PLUS, tr(keys::LAYERS_ADD_GROUP))
                .clicked()
            {
                let name = tr_format(keys::GROUP_DEFAULT_NAME, &[("number", &number)]);
                let kind = LayerKind::Group {
                    isolation: Isolation::default(),
                };
                let command = create(session.document(), view, name, kind);
                session.user(command);
            }
            let adjustment =
                widgets::icon_button(ui, icons::CIRCLE_HALF, tr(keys::LAYERS_ADD_ADJUSTMENT));
            egui::Popup::menu(&adjustment).show(|ui| {
                for adjustment in new_adjustments() {
                    if ui.button(tr(adjustment_key(&adjustment))).clicked() {
                        let name = tr(adjustment_key(&adjustment)).to_owned();
                        let command = create(
                            session.document(),
                            view,
                            name,
                            LayerKind::Adjustment { adjustment },
                        );
                        session.user(command);
                        ui.close();
                    }
                }
            });
            let target = view.selected;
            let can_mask = target
                .and_then(|id| session.document().node(id))
                .is_some_and(|node| node.mask.is_none());
            let add_mask = ui.add_enabled(
                can_mask,
                egui::Button::new(widgets::icon(icons::SELECTION, metrics::ICON)).frame(false),
            );
            if add_mask.on_hover_text(tr(keys::LAYERS_ADD_MASK)).clicked()
                && let Some(node) = target
            {
                session.user(PhotoCommand::MaskCreate(MaskCreate {
                    node,
                    fill: Default::default(),
                }));
            }
            // Filters render from B10, after J4.
            ui.add_enabled(
                false,
                egui::Button::new(widgets::icon(icons::SPARKLE, metrics::ICON)).frame(false),
            )
            .on_disabled_hover_text(tr(keys::LAYERS_FILTERS_LATER));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, icons::TRASH, tr(keys::LAYERS_DELETE)).clicked()
                    && let Some(id) = view.selected
                {
                    session.user(PhotoCommand::LayerDelete(LayerDelete { id }));
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_commands::Source;
    use cygnus_photo_core::Depth;
    use cygnus_photo_core::commands::DocumentCreate;

    /// 1 bottom, 2 group (3 inside it), 4 top.
    fn session() -> Session {
        let create = |name: &str, parent: Option<u64>, kind: LayerKind| {
            PhotoCommand::LayerCreate(LayerCreate {
                name: name.to_owned(),
                parent: parent.map(NodeId),
                index: None,
                kind,
            })
        };
        Session::from_commands(
            vec![
                PhotoCommand::DocumentCreate(DocumentCreate {
                    width: 8,
                    height: 8,
                    resolution: 72.0,
                    depth: Depth::U8,
                    profile: Default::default(),
                }),
                create("bas", None, LayerKind::Pixel),
                create(
                    "groupe",
                    None,
                    LayerKind::Group {
                        isolation: Isolation::Isolated,
                    },
                ),
                create("dedans", Some(2), LayerKind::Pixel),
                create("haut", None, LayerKind::Pixel),
            ],
            Source::Test,
        )
    }

    #[test]
    fn rows_list_the_top_layer_first_and_hide_folded_groups() {
        let session = session();
        let mut view = ViewState::default();
        let ids = |rows: Vec<(NodeId, usize)>| {
            rows.into_iter()
                .map(|(id, d)| (id.0, d))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(rows(session.document(), &view)),
            [(4, 0), (2, 0), (3, 1), (1, 0)]
        );
        view.collapsed.insert(NodeId(2));
        assert_eq!(
            ids(rows(session.document(), &view)),
            [(4, 0), (2, 0), (1, 0)]
        );
    }

    #[test]
    fn a_layer_dropped_on_a_row_goes_above_it_or_into_a_group() {
        let session = session();
        let document = session.document();
        // On layer 1 (bottom): just above it, in the root.
        assert_eq!(
            drop_place(document, NodeId(4), NodeId(1)),
            Some((document.root, 1))
        );
        // On the group: inside it, at the top.
        assert_eq!(
            drop_place(document, NodeId(4), NodeId(2)),
            Some((NodeId(2), 1))
        );
    }

    #[test]
    fn new_layers_go_just_above_the_selection() {
        let session = session();
        let mut view = ViewState::default();
        assert_eq!(insertion(session.document(), &view), (None, None));
        view.selected = Some(NodeId(3));
        assert_eq!(
            insertion(session.document(), &view),
            (Some(NodeId(2)), Some(1))
        );
    }

    #[test]
    fn user_actions_are_recorded_as_the_user_and_refusals_are_reported() {
        let mut session = session();
        session.user(update(NodeId(1), |p| p.opacity = Some(0.5)));
        let last = session.bus().entries().last().map(|entry| entry.source);
        assert_eq!(last, Some(Source::User));
        assert!(session.message().is_none());
        // Merging without a renderer is refused with a message.
        run(
            &mut session,
            None,
            PhotoCommand::LayerMergeDown(LayerMergeDown {
                id: NodeId(4),
                resolved: None,
            }),
        );
        assert!(session.message().is_some());
    }

    #[test]
    fn a_slider_drag_is_one_history_entry() {
        let mut session = session();
        let before = session.bus().entries().count();
        session.begin_gesture();
        for opacity in [0.9, 0.7, 0.5, 0.3] {
            session.user(update(NodeId(1), |p| p.opacity = Some(opacity)));
        }
        session.end_gesture();
        assert_eq!(session.bus().entries().count(), before + 1);
        session.undo();
        assert_eq!(
            session.document().node(NodeId(1)).map(|n| n.opacity),
            Some(1.0)
        );
        session.redo();
        assert_eq!(
            session.document().node(NodeId(1)).map(|n| n.opacity),
            Some(0.3)
        );
    }

    #[test]
    fn the_selection_forgets_deleted_layers() {
        let mut session = session();
        let mut view = ViewState {
            selected: Some(NodeId(4)),
            ..ViewState::default()
        };
        session.user(PhotoCommand::LayerDelete(LayerDelete { id: NodeId(4) }));
        view.follow(session.document());
        assert_eq!(view.selected, None);
    }
}
