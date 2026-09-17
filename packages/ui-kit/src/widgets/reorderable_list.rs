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

//! Liste GÉNÉRIQUE réordonnable par drag & drop, avec virtualisation.
//!
//! ui-kit ne connaît PAS le type des items : l'app fournit le rendu
//! via un callback. Utilisée par photo pour les calques, réutilisable
//! par video (clips) et audio (pistes). Strictement domain-agnostic :
//! aucun type métier ici.

use super::super::utils::drag_state::ReorderDragState;
use crate::theme::CygnusTheme;

/// Calcule l'index cible d'insertion selon la position Y de la souris
/// relativement à la ligne survolee `current_index`.
///
/// Si la souris est dans la moitié basse de l'item, l'insertion se
/// fait APRÈS cet item (`current_index + 1`), sinon à sa place
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

/// Fond de ligne standard (sélection / survol), tokens du thème.
/// Helper pour les callbacks `draw_item` des apps (évite de dupliquer
/// la logique dans photo/video/audio).
pub fn item_background(ui: &mut egui::Ui, rect: egui::Rect, selected: bool, hovered: bool) {
    let theme = CygnusTheme::dark();
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

/// Dessine la ligne d'insertion à la cible de drop.
///
/// `rows` associe chaque ligne visible à son `y` écran ; la ligne est
/// tracée en haut de la ligne cible (ou sous la dernière ligne pour
/// une insertion en fin de liste). Rien hors drag ou sans cible.
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
    let theme = CygnusTheme::dark();
    ui.painter().hline(
        egui::Rangef::new(left, right),
        y,
        egui::Stroke::new(2.0, theme.colors.drop_indicator),
    );
}

/// Liste GÉNÉRIQUE réordonnable par drag & drop avec virtualisation.
///
/// Pour 100+ items, seules les lignes visibles sont dessinées
/// (`show_rows`). Retourne `Some((from, to))` quand un drop a eu lieu.
///
/// `draw_item(ui, item, index, is_dragging_this)` dessine le contenu
/// (l'item dragué doit typiquement s'afficher fantôme/transparent).
///
/// Le fantôme qui suit la souris est dessiné par l'app (elle seule
/// connaît le libellé) via `drag_state.dragging_index` et
/// `drag_state.current_mouse_pos`.
pub struct ReorderableList<'a, T> {
    /// Items à afficher (empruntés, jamais clonés par la liste).
    pub items: &'a [T],
    /// Hauteur fixe d'une ligne (requise par la virtualisation).
    pub item_height: f32,
    /// État de drag détenu par l'app.
    pub drag_state: &'a mut ReorderDragState,
}

impl<'a, T> ReorderableList<'a, T> {
    /// Crée une liste réordonnable sur `items`.
    pub fn new(items: &'a [T], item_height: f32, drag_state: &'a mut ReorderDragState) -> Self {
        Self {
            items,
            item_height,
            drag_state,
        }
    }

    /// Affiche la liste et retourne le réordonnancement éventuel.
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
                if response.dragged()
                    && let Some(pos) = ui.ctx().pointer_latest_pos()
                {
                    drag.current_mouse_pos = pos;
                    // Cible = ligne sous la souris (moitié haute/basse).
                    let mut target = drag.dragging_index.unwrap_or(index);
                    for (row_index, row_top) in &rows {
                        if pos.y >= *row_top && pos.y < *row_top + item_height {
                            let row_rect = egui::Rect::from_min_size(
                                egui::pos2(rect.min.x, *row_top),
                                egui::vec2(rect.width(), item_height),
                            );
                            target = compute_target_index(pos, row_rect, *row_index, item_height);
                            break;
                        }
                    }
                    drag.target_index = Some(target.min(total));
                }
                if response.drag_stopped() {
                    dropped = drag.end_drag();
                }

                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    draw_item(ui, item, index, is_dragging_this);
                });
            }
        });

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

    #[test]
    fn compute_target_index_upper_half() {
        // Souris dans la moitié haute -> insertion à la place de l'item.
        let rect = row_at(2, 48.0);
        assert_eq!(
            compute_target_index(egui::pos2(10.0, 100.0), rect, 2, 48.0),
            2
        );
    }

    #[test]
    fn compute_target_index_lower_half() {
        // Souris dans la moitié basse -> insertion après l'item.
        let rect = row_at(2, 48.0);
        assert_eq!(
            compute_target_index(egui::pos2(10.0, 130.0), rect, 2, 48.0),
            3
        );
    }

    #[test]
    fn drop_returns_from_to() {
        // Simulation d'un drag complet au niveau état : origine 1,
        // souris dans la moitié basse de la ligne 3 -> (1, 4).
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
        // Tout caractère non ASCII dessiné doit appartenir aux
        // codepoints de la police d'icônes : aucun emoji ni glyphe
        // unicode ne peut fuiter hors de CygnusIcon.
        use crate::widgets::icon::{ALL_ICONS, icon_button};
        let allowed: HashSet<char> = ALL_ICONS
            .iter()
            .flat_map(|icon| icon.codepoint().chars())
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
        // Viewport borné comme un vrai panneau (cf. Phase 3 v1 : en
        // headless non borné, show_rows dessine tout).
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

        // Budget 2ms en release (production) ; en debug le mode
        // immédiat coûte ~4x plus cher (mesuré Phase 3 v1).
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
}
