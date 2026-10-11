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

//! History panel: undo/redo step list + current state.
//!
//! `HistoryPanel` does not know where it is displayed: the workspace
//! (`crate::layout`, "History" dock tab) decides placement.
//! No worker send: everything bubbles as
//! [`HistoryPanelAction`]. Labels come from the worker
//! (snapshot `undo_labels`/`redo_labels`); the current state and the
//! empty state use [`PhotoCatalog`](crate::i18n::PhotoCatalog).

use crate::commands::PhotoUiContext;
use crate::i18n::{PhotoCatalog, PhotoTextKey};

/// History panel action (state jump, routed by `PhotoApp`
/// to the worker after conversion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryPanelAction {
    /// Step back `steps` steps (jump to an undoable state).
    Back(u32),
    /// Step forward `steps` steps (jump to a redoable state).
    Forward(u32),
}

/// History panel (tab content, no chrome).
pub struct HistoryPanel;

impl HistoryPanel {
    /// Draws the list and returns the requested jumps (at most one
    /// per frame: a single click possible).
    ///
    /// Undo rows (oldest first, numbered) then redo
    /// rows (next one first). The head (last applied step = current
    /// state) carries the selection background, non-clickable: no
    /// separate "current state" row. Vertical scrolling.
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        undo: &[String],
        redo: &[String],
    ) -> Vec<HistoryPanelAction> {
        let texts = PhotoCatalog::new(ctx.shared.translator().language());
        if undo.is_empty() && redo.is_empty() {
            ui.label(texts.get(PhotoTextKey::HistoryEmpty));
            return Vec::new();
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut actions = Vec::new();
                // Opening state: undo everything (unreachable
                // otherwise: undo rows only go back to
                // their own head).
                if !undo.is_empty()
                    && ui
                        .selectable_label(false, texts.get(PhotoTextKey::HistoryInitial))
                        .clicked()
                {
                    actions.push(HistoryPanelAction::Back(undo.len() as u32));
                }
                for (index, label) in undo.iter().enumerate() {
                    let row = format!("{}. {label}", index + 1);
                    if index + 1 == undo.len() {
                        // Head = current state: selection background,
                        // deliberately non-clickable.
                        let _ = ui.selectable_label(true, row);
                    } else if ui.selectable_label(false, row).clicked() {
                        actions.push(HistoryPanelAction::Back(back_steps_for_undo_click(
                            undo.len(),
                            index,
                        )));
                    }
                }
                for (from_top, label) in redo.iter().rev().enumerate() {
                    if ui.selectable_label(false, label).clicked() {
                        actions.push(HistoryPanelAction::Forward(forward_steps_for_redo_click(
                            from_top,
                        )));
                    }
                }
                actions
            })
            .inner
    }
}

/// Steps back for a click on undo row `clicked`
/// (0 = oldest) among `undo_len`: the clicked row becomes the
/// head (everything after is undone), i.e. `undo_len - 1 - clicked.
/// Clicking the oldest = undo everything.
#[must_use]
pub fn back_steps_for_undo_click(undo_len: usize, clicked: usize) -> u32 {
    undo_len.saturating_sub(clicked.saturating_add(1)).max(1) as u32
}

/// Steps forward for a click on redo row `from_top` (0 =
/// head = next redo): `from_top + 1`.
#[must_use]
pub fn forward_steps_for_redo_click(from_top: usize) -> u32 {
    u32::try_from(from_top.saturating_add(1)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clic_undo_rend_la_rangee_cliquee_tete() {
        // 3 steps [A, B, C]: click B (index 1) = undo C alone, B at head.
        assert_eq!(back_steps_for_undo_click(3, 1), 1);
        // Click A (the oldest) = undo B + C, A at head.
        assert_eq!(back_steps_for_undo_click(3, 0), 2);
        // Head non-clickable in UI; guard: never zero.
        assert_eq!(back_steps_for_undo_click(3, 2), 1);
        assert_eq!(back_steps_for_undo_click(1, 5), 1);
    }

    #[test]
    fn clic_redo_avance_jusqu_a_l_etat_vise() {
        assert_eq!(forward_steps_for_redo_click(0), 1);
        assert_eq!(forward_steps_for_redo_click(2), 3);
    }

    #[test]
    fn panneau_rend_sans_panique_vide_et_rempli() {
        let ctx_egui = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx_egui);
        let ctx = PhotoUiContext::for_frame(&ctx_egui);
        ctx_egui
            .run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let vide = HistoryPanel::show(ui, &ctx, &[], &[]);
                    assert!(vide.is_empty(), "aucun clic sans interaction");
                    let actions = HistoryPanel::show(
                        ui,
                        &ctx,
                        &[String::from("Opacité"), String::from("Coup de pinceau")],
                        &[String::from("Renommer")],
                    );
                    assert!(actions.is_empty(), "aucun clic sans interaction");
                });
            })
            .drop_without_applying_deltas();
    }
}
