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

//! GENERIC reorderable list via drag & drop, with virtualization.
//!
//! ui-kit does NOT know the items' type: the app provides rendering
//! via a callback. Used by photo for layers, reusable by video
//! (clips) and audio (tracks). Strictly domain-agnostic: no domain
//! type here.

use super::super::utils::drag_state::{DropPosition, ReorderDragState};
use crate::theme::UiThemeExt;

/// Computes the target insertion index from the mouse Y position
/// relative to the hovered `current_index` row.
///
/// When the mouse is in the item's bottom half, insertion happens
/// AFTER that item (`current_index + 1`), otherwise in its place
/// (`current_index`).
pub fn compute_target_index(
    mouse_pos: egui::Pos2,
    item_rect: egui::Rect,
    current_index: usize,
    _item_height: f32,
) -> usize {
    if mouse_pos.y > item_rect.center().y {
        current_index + 1
    } else {
        current_index
    }
}

/// Standard row background (selection / hover), theme tokens.
/// Helper for apps' `draw_item` callbacks (avoids duplicating
/// the logic across photo/video/audio).
pub fn item_background(ui: &mut egui::Ui, rect: egui::Rect, selected: bool, hovered: bool) {
    let theme = ui.cygnus_theme();
    let bg = if selected {
        theme.colors.item_selected
    } else if hovered {
        theme.colors.item_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    if bg != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, theme.radius.sm, bg);
    }
}

/// Draws the insertion line at the drop target.
///
/// `rows` maps each visible row to its screen `y`; the line is drawn
/// at the top of the target row (or below the last row for an
/// end-of-list insertion). Nothing outside a drag or without a target.
pub fn draw_drop_indicator(
    ui: &mut egui::Ui,
    drag_state: &ReorderDragState,
    rows: &[(usize, f32)],
    row_height: f32,
    left: f32,
    right: f32,
) {
    if !drag_state.is_dragging {
        return;
    }
    let Some(target) = drag_state.target_index else {
        return;
    };
    let mut y = None;
    for (index, top) in rows {
        if *index == target {
            y = Some(*top);
            break;
        }
    }
    let y = y.unwrap_or_else(|| {
        rows.last()
            .map_or(ui.min_rect().top(), |(_, top)| top + row_height)
    });
    let theme = ui.cygnus_theme();
    let stroke = egui::Stroke::new(theme.borders.selected, theme.colors.drop_indicator);
    let painter = ui.painter();
    painter.hline(egui::Rangef::new(left, right), y, stroke);
    for x in [left, right] {
        painter.circle_filled(
            egui::pos2(x, y),
            theme.borders.selected,
            theme.colors.drop_indicator,
        );
    }
}

/// Hierarchical drop: indexes into `items` + position relative
/// to the target (`Into` = nest inside).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HierarchicalDrop {
    /// Drag origin index.
    pub from: usize,
    /// Target row index.
    pub to: usize,
    /// Position relative to the target.
    pub position: DropPosition,
}

/// GENERIC reorderable list via drag & drop with virtualization.
///
/// For 100+ items, only visible rows are drawn (`show_rows`).
/// Returns `Some((from, to))` when a drop occurred.
///
/// `draw_item(ui, item, index, is_dragging_this)` draws the content
/// (the dragged item should typically render ghosted/transparent).
///
/// The ghost following the mouse is drawn by the app (only it knows
/// the label) via `drag_state.dragging_index` and
/// `drag_state.current_mouse_pos`.
pub struct ReorderableList<'a, T> {
    /// Items to display (borrowed, never cloned by the list).
    pub items: &'a [T],
    /// Fixed row height (required by virtualization).
    pub item_height: f32,
    /// Drag state owned by the app.
    pub drag_state: &'a mut ReorderDragState,
}

impl<'a, T> ReorderableList<'a, T> {
    /// Creates a reorderable list over `items`.
    pub fn new(items: &'a [T], item_height: f32, drag_state: &'a mut ReorderDragState) -> Self {
        Self {
            items,
            item_height,
            drag_state,
        }
    }

    /// Displays the list and returns the resulting reorder, if any.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        mut draw_item: impl FnMut(&mut egui::Ui, &T, usize, bool),
    ) -> Option<(usize, usize)> {
        let mut dropped = None;
        let mut rows = Vec::new();
        let panel_rect = ui.min_rect();
        let item_height = self.item_height;
        let total = self.items.len();
        let drag = self.drag_state;
        let items = self.items;

        let mut stopped = false;
        egui::ScrollArea::vertical().show_rows(ui, item_height, total, |ui, range| {
            for index in range {
                let Some(item) = items.get(index) else {
                    continue;
                };
                let is_dragging_this = drag.dragging_index == Some(index);
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), item_height),
                    egui::Sense::click_and_drag(),
                );
                rows.push((index, rect.top()));

                if response.drag_started() {
                    drag.start(index);
                }
                if response.drag_stopped() {
                    stopped = true;
                }

                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    draw_item(ui, item, index, is_dragging_this);
                });
                // Re-pin the advance onto the allocated rect: the scope
                // advances the cursor per its CONTENT, overwriting the
                // `allocate` advance (a drift of a few px per row →
                // offset clicks and drops, see test).
                ui.advance_cursor_after_rect(rect);
            }
        });

        // Target computed AFTER the loop, over ALL visible rows
        // (same reason as `show_variable`: inside the loop, rows
        // below the origin are invisible).
        if drag.is_dragging {
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                drag.current_mouse_pos = pos;
                // Target = row under the mouse (top/bottom half).
                let mut target = drag.dragging_index.unwrap_or(0);
                for (row_index, row_top) in &rows {
                    if pos.y >= *row_top && pos.y < *row_top + item_height {
                        let row_rect = egui::Rect::from_min_size(
                            egui::pos2(panel_rect.min.x, *row_top),
                            egui::vec2(panel_rect.width(), item_height),
                        );
                        target = compute_target_index(pos, row_rect, *row_index, item_height);
                        break;
                    }
                }
                drag.target_index = Some(target.min(total));
            }
            if stopped {
                dropped = drag.end_drag();
            }
        }

        draw_drop_indicator(
            ui,
            drag,
            &rows,
            item_height,
            panel_rect.left(),
            panel_rect.right(),
        );

        dropped
    }

    /// Displays a NON-virtualized variable-height list with
    /// hierarchical drops (before / into / after).
    ///
    /// For heterogeneous lists of reasonable size (layers panel:
    /// parent rows + indented children) where each item has its own
    /// height (`heights`): no more overlaps from fixed-height
    /// virtualization. `can_nest` tells which rows accept a drop
    /// inside (middle third); `can_drag` tells which rows can be
    /// dragged; the others only get before/after (halves). Returns
    /// the hierarchical drop.
    ///
    /// Rows are separated by a theme `xxs` gap so the drop indicator
    /// line always sits on empty background, never straddling two
    /// row edges (see `paint_variable_indicator`).
    pub fn show_variable(
        self,
        ui: &mut egui::Ui,
        heights: impl Fn(&T) -> f32,
        can_nest: impl Fn(&T) -> bool,
        can_drag: impl Fn(&T) -> bool,
        mut draw_item: impl FnMut(&mut egui::Ui, &T, usize, bool),
    ) -> Option<HierarchicalDrop> {
        let mut dropped = None;
        // Visible rows: (index, real rect) — targets are computed
        // from the allocated geometry, never from an assumed height.
        let mut rows: Vec<(usize, egui::Rect)> = Vec::new();
        let panel_rect = ui.min_rect();
        let total = self.items.len();
        let drag = self.drag_state;
        let items = self.items;
        let row_gap = ui.cygnus_theme().spacing.xxs;

        let mut stopped = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (index, item) in items.iter().enumerate() {
                let height = heights(item).max(1.0);
                let is_dragging_this = drag.dragging_index == Some(index);
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height),
                    egui::Sense::click_and_drag(),
                );
                rows.push((index, rect));

                if response.drag_started() {
                    // Only draggable rows open a gesture
                    // (others stay normally clickable).
                    if can_drag(item) {
                        drag.start(index);
                        drag.drop_position = DropPosition::Before;
                    }
                }
                if response.drag_stopped() {
                    stopped = true;
                }

                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    draw_item(ui, item, index, is_dragging_this);
                });
                // Re-pin the advance onto the allocated rect: the scope
                // advances the cursor per its CONTENT, overwriting the
                // `allocate` advance (a drift of a few px per row →
                // offset clicks and drops, see test).
                ui.advance_cursor_after_rect(rect);
                // Breathing room between rows (not after the last one):
                // the drop indicator lives in this gap.
                if index + 1 < total {
                    ui.add_space(row_gap);
                }
            }
        });

        // Target computed AFTER the loop, over ALL rows: computing it
        // inside the loop only sees rows already pushed (above the
        // origin) — downward drops stayed invisible and the drop never
        // fired.
        if drag.is_dragging {
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                drag.current_mouse_pos = pos;
                let mut target = drag.dragging_index.unwrap_or(0);
                let mut position = DropPosition::After;
                for (row_index, row_rect) in &rows {
                    if row_rect.contains(pos) {
                        target = *row_index;
                        position = drop_position_in(pos, row_rect, can_nest(&items[*row_index]));
                        break;
                    }
                }
                drag.target_index = Some(target.min(total));
                drag.drop_position = position;
            }
            if stopped {
                if let Some((from, to, position)) = drag.end_drag_hierarchical() {
                    dropped = Some(HierarchicalDrop { from, to, position });
                }
            }
        }

        if drag.is_dragging {
            paint_variable_indicator(ui, drag, &rows, panel_rect);
        }

        dropped
    }
}

/// Drop position within a row: middle third + `nestable` ⇒
/// `Into`, else top/bottom half ⇒ `Before`/`After`.
fn drop_position_in(pos: egui::Pos2, rect: &egui::Rect, nestable: bool) -> DropPosition {
    let third = rect.height() / 3.0;
    if nestable && pos.y >= rect.top() + third && pos.y < rect.top() + 2.0 * third {
        DropPosition::Into
    } else if pos.y < rect.center().y {
        DropPosition::Before
    } else {
        DropPosition::After
    }
}

/// Variable drop indicator: background on the target (`Into`),
/// insertion line otherwise (target top, or list bottom at the end).
/// The line is drawn thicker with round end dots so it stays visible
/// in the inter-row gap (it must never blend into a row edge).
fn paint_variable_indicator(
    ui: &mut egui::Ui,
    drag: &ReorderDragState,
    rows: &[(usize, egui::Rect)],
    panel_rect: egui::Rect,
) {
    let theme = ui.cygnus_theme();
    let Some(target) = drag.target_index else {
        return;
    };
    if drag.drop_position == DropPosition::Into {
        if let Some((_, rect)) = rows.iter().find(|(index, _)| *index == target) {
            ui.painter()
                .rect_filled(*rect, theme.radius.sm, theme.colors.item_selected);
        }
        return;
    }
    let y = rows
        .iter()
        .find(|(index, _)| *index == target)
        .map(|(_, rect)| rect.top())
        .unwrap_or_else(|| {
            rows.last()
                .map_or(panel_rect.top(), |(_, rect)| rect.bottom())
        });
    let stroke = egui::Stroke::new(theme.borders.selected, theme.colors.drop_indicator);
    let painter = ui.painter();
    painter.hline(
        egui::Rangef::new(panel_rect.left(), panel_rect.right()),
        y,
        stroke,
    );
    for x in [panel_rect.left(), panel_rect.right()] {
        painter.circle_filled(
            egui::pos2(x, y),
            theme.borders.selected,
            theme.colors.drop_indicator,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn row_at(index: usize, height: f32) -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(0.0, index as f32 * height),
            egui::vec2(300.0, height),
        )
    }

    /// Simulates a full drag (press → move → release) over
    /// `show_variable` and returns the drop emitted on the last frame.
    /// Target positions are read from the real layout (event-free
    /// frame), never guessed.
    fn simulate_drag(
        ctx: &egui::Context,
        items: &[String],
        drag: &mut ReorderDragState,
        from_row: usize,
        to_point: egui::Pos2,
    ) -> Option<HierarchicalDrop> {
        use std::cell::RefCell;
        let rects = RefCell::new(Vec::new());
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                rects.borrow_mut().clear();
                let _ = ReorderableList::new(items, 40.0, drag).show_variable(
                    ui,
                    |_| 40.0,
                    |_| false,
                    |_| true,
                    |ui, _item, _index, _dragging| {
                        rects.borrow_mut().push(ui.available_rect_before_wrap());
                        ui.label("ligne");
                    },
                );
            });
        };
        // Frame 1: layout, read the rects.
        ctx.run_ui(egui::RawInput::default(), &mut draw)
            .drop_without_applying_deltas();
        let from = rects.borrow()[from_row].center();
        // Frame 2: press on the origin row.
        let mut dropped = None;
        ctx.run_ui(
            pointer_input(vec![egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }]),
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    dropped = ReorderableList::new(items, 40.0, drag).show_variable(
                        ui,
                        |_| 40.0,
                        |_| false,
                        |_| true,
                        |ui, _item, _index, _dragging| {
                            ui.label("ligne");
                        },
                    );
                });
            },
        )
        .drop_without_applying_deltas();
        assert!(dropped.is_none(), "pas de drop à l'appui");
        // Frame 3: move (button still held).
        ctx.run_ui(
            pointer_input(vec![egui::Event::PointerMoved(to_point)]),
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    dropped = ReorderableList::new(items, 40.0, drag).show_variable(
                        ui,
                        |_| 40.0,
                        |_| false,
                        |_| true,
                        |ui, _item, _index, _dragging| {
                            ui.label("ligne");
                        },
                    );
                });
            },
        )
        .drop_without_applying_deltas();
        assert!(dropped.is_none(), "pas de drop avant relâchement");
        // Frame 4: release.
        ctx.run_ui(
            pointer_input(vec![egui::Event::PointerButton {
                pos: to_point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }]),
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    dropped = ReorderableList::new(items, 40.0, drag).show_variable(
                        ui,
                        |_| 40.0,
                        |_| false,
                        |_| true,
                        |ui, _item, _index, _dragging| {
                            ui.label("ligne");
                        },
                    );
                });
            },
        )
        .drop_without_applying_deltas();
        dropped
    }

    /// `RawInput` carrying only pointer events.
    fn pointer_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            events,
            ..Default::default()
        }
    }

    #[test]
    fn drag_complet_emet_drop_avant_ou_apres() {
        let ctx = egui::Context::default();
        crate::theme::setup_fonts(&ctx);
        let items: Vec<String> = (0..3).map(|i| format!("item {i}")).collect();
        let mut drag = ReorderDragState::default();
        // Target: bottom half of row 2 (drop after).
        let target = {
            use std::cell::RefCell;
            let rects = RefCell::new(Vec::new());
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = ReorderableList::new(&items, 40.0, &mut drag).show_variable(
                        ui,
                        |_| 40.0,
                        |_| false,
                        |_| true,
                        |ui, _item, _index, _dragging| {
                            rects.borrow_mut().push(ui.available_rect_before_wrap());
                            ui.label("ligne");
                        },
                    );
                });
            })
            .drop_without_applying_deltas();
            let rects = rects.borrow();
            egui::pos2(
                rects[2].center().x,
                rects[2].top() + rects[2].height() * 0.75,
            )
        };
        let drop =
            simulate_drag(&ctx, &items, &mut drag, 0, target).expect("drop attendu au relâchement");
        assert_eq!(drop.from, 0);
        assert_eq!(drop.to, 2);
        assert_eq!(drop.position, DropPosition::After);
        assert!(!drag.is_dragging, "état réinitialisé");
    }

    #[test]
    fn compute_target_index_upper_half() {
        // Mouse in the top half -> insert in the item's place.
        let rect = row_at(2, 48.0);
        assert_eq!(
            compute_target_index(egui::pos2(10.0, 100.0), rect, 2, 48.0),
            2
        );
    }

    #[test]
    fn compute_target_index_lower_half() {
        // Mouse in the bottom half -> insert after the item.
        let rect = row_at(2, 48.0);
        assert_eq!(
            compute_target_index(egui::pos2(10.0, 130.0), rect, 2, 48.0),
            3
        );
    }

    #[test]
    fn drop_returns_from_to() {
        // Full drag simulation at state level: origin 1, mouse in the
        // bottom half of row 3 -> (1, 4).
        let mut drag = ReorderDragState::default();
        drag.start(1);
        let rect = row_at(3, 48.0);
        drag.target_index = Some(compute_target_index(
            egui::pos2(10.0, 3.0 * 48.0 + 30.0),
            rect,
            3,
            48.0,
        ));
        assert_eq!(drag.end_drag(), Some((1, 4)));
    }

    #[test]
    fn show_without_interaction_returns_none() {
        let ctx = egui::Context::default();
        let items: Vec<String> = (0..10).map(|i| format!("item {i}")).collect();
        let mut drag = ReorderDragState::default();
        let mut drawn = 0;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let result = ReorderableList::new(&items, 48.0, &mut drag).show(
                    ui,
                    |ui, item, _index, _dragging| {
                        drawn += 1;
                        ui.label(item);
                    },
                );
                assert_eq!(result, None);
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(drawn, 10);
    }

    #[test]
    fn no_emoji_in_output() {
        // Every non-ASCII drawn character must belong to the icon
        // font's codepoints: no emoji or unicode glyph may leak outside
        // the internal `icons::glyph` module.
        use crate::icons::{ALL_ICONS, IconRegistry, icon_button};
        let allowed: HashSet<char> = ALL_ICONS
            .iter()
            .flat_map(|icon| IconRegistry::resolve(*icon).codepoint().chars())
            .collect();
        assert!(!allowed.is_empty());

        let ctx = egui::Context::default();
        crate::theme::setup_fonts(&ctx);
        let items: Vec<String> = (0..5).map(|i| format!("item {i}")).collect();
        let mut drag = ReorderDragState::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = ReorderableList::new(&items, 48.0, &mut drag).show(
                    ui,
                    |ui, item, index, _dragging| {
                        ui.horizontal(|ui| {
                            let _ = icon_button(ui, ALL_ICONS[index % ALL_ICONS.len()], None);
                            ui.label(item);
                        });
                    },
                );
            });
        });
        let mut text = String::new();
        for clipped in &output.shapes {
            if let egui::Shape::Text(shape) = &clipped.shape {
                text.push_str(&shape.galley.job.text);
                text.push('\n');
            }
        }
        output.drop_without_applying_deltas();
        assert!(!text.is_empty(), "aucun texte dessine");
        for ch in text.chars() {
            if ch.is_ascii() || ch.is_whitespace() {
                continue;
            }
            assert!(
                allowed.contains(&ch),
                "caractere non ASCII hors police d'icones : {ch:?} (U+{:04X})",
                ch as u32
            );
        }
    }

    #[test]
    fn performance_200_items_under_2ms() {
        use std::time::Instant;
        let ctx = egui::Context::default();
        crate::theme::setup_fonts(&ctx);
        let items: Vec<String> = (0..200).map(|i| format!("item {i}")).collect();
        // Viewport bounded like a real panel (see Phase 3 v1: unbounded
        // headless draws everything in show_rows).
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 600.0));
        let show_bounded = |ui: &mut egui::Ui, drag: &mut ReorderDragState| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(viewport), |ui| {
                let _ = ReorderableList::new(&items, 48.0, drag).show(ui, |ui, item, _, _| {
                    ui.label(item);
                });
            });
        };
        let mut drag = ReorderDragState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            show_bounded(ui, &mut drag);
        })
        .drop_without_applying_deltas();

        // 2ms budget in release (production); in debug, immediate mode
        // costs ~4x more (measured Phase 3 v1).
        let budget_ms = if cfg!(debug_assertions) { 6.0 } else { 2.0 };
        let frames = 10;
        let start = Instant::now();
        for _ in 0..frames {
            let mut drag = ReorderDragState::default();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                show_bounded(ui, &mut drag);
            })
            .drop_without_applying_deltas();
        }
        let mean_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
        assert!(
            mean_ms < budget_ms,
            "rendu moyen trop lent avec 200 items : {mean_ms:.2}ms/frame (budget {budget_ms}ms)"
        );
    }

    #[test]
    fn drop_position_in_tiers_central_si_nestable() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 90.0), egui::vec2(300.0, 90.0));
        // Middle third + nestable => Into.
        assert_eq!(
            drop_position_in(egui::pos2(10.0, 135.0), &rect, true),
            DropPosition::Into
        );
        // Middle third + non nestable => half (top one here).
        assert_eq!(
            drop_position_in(egui::pos2(10.0, 130.0), &rect, false),
            DropPosition::Before
        );
        // Top => Before, bottom => After (nestable or not).
        assert_eq!(
            drop_position_in(egui::pos2(10.0, 95.0), &rect, true),
            DropPosition::Before
        );
        assert_eq!(
            drop_position_in(egui::pos2(10.0, 170.0), &rect, true),
            DropPosition::After
        );
        assert_eq!(
            drop_position_in(egui::pos2(10.0, 170.0), &rect, false),
            DropPosition::After
        );
    }

    #[test]
    fn show_variable_without_interaction_returns_none() {
        let ctx = egui::Context::default();
        let items: Vec<(String, f32)> = (0..5)
            .map(|i| (format!("item {i}"), 40.0 + i as f32 * 8.0))
            .collect();
        let mut drag = ReorderDragState::default();
        let mut drawn = 0;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let result = ReorderableList::new(&items, 0.0, &mut drag).show_variable(
                    ui,
                    |(_, height)| *height,
                    |_| false,
                    |_| true,
                    |ui, (label, _), _index, _dragging| {
                        drawn += 1;
                        ui.label(label);
                    },
                );
                assert_eq!(result, None);
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(drawn, 5, "toutes les lignes dessinées, hauteurs variables");
    }
}
