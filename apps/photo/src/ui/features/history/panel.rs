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

//! Panneau Historique : liste des pas undo/redo + état actuel.
//!
//! `HistoryPanel` ne sait pas où il est affiché : le workspace
//! (`crate::layout`, onglet dock titré « Historique ») décide du
//! placement. Aucun envoi worker : tout remonte en
//! [`HistoryPanelAction`]. Les libellés viennent du worker
//! (`undo_labels`/`redo_labels` du snapshot) ; l'état actuel et le
//! vide utilisent [`PhotoCatalog`](crate::i18n::PhotoCatalog).

use crate::commands::PhotoUiContext;
use crate::i18n::{PhotoCatalog, PhotoTextKey};

/// Action du panneau Historique (saut d'état, routée par `PhotoApp`
/// vers le worker après conversion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryPanelAction {
    /// Reculer de `steps` pas (saut vers un état annulable).
    Back(u32),
    /// Avancer de `steps` pas (saut vers un état rétablissable).
    Forward(u32),
}

/// Panneau Historique (contenu de l'onglet, sans chrome).
pub struct HistoryPanel;

impl HistoryPanel {
    /// Dessine la liste et retourne les sauts demandés (au plus un
    /// par frame : un seul clic possible).
    ///
    /// Rangées undo (plus ancien d'abord, numérotées) puis rangées
    /// redo (prochain en tête). La tête (dernier pas appliqué = état
    /// actuel) porte le fond de sélection, non cliquable : pas de
    /// rangée « état actuel » séparée. Défilement vertical.
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
                // État d'ouverture : tout annuler (inaccessible
                // autrement : les rangées undo ne remontent qu'à
                // leur propre tête).
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
                        // Tête = état actuel : fond de sélection,
                        // volontairement non cliquable.
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

/// Pas en arrière pour un clic sur la rangée undo `clicked`
/// (0 = plus ancien) parmi `undo_len` : la rangée cliquée devient la
/// tête (tout ce qui suit est annulé), soit `undo_len - 1 - clicked.
/// Cliquer le plus ancien = tout annuler.
#[must_use]
pub fn back_steps_for_undo_click(undo_len: usize, clicked: usize) -> u32 {
    undo_len.saturating_sub(clicked.saturating_add(1)).max(1) as u32
}

/// Pas en avant pour un clic sur la rangée redo `from_top` (0 =
/// tête = prochain redo) : `from_top + 1`.
#[must_use]
pub fn forward_steps_for_redo_click(from_top: usize) -> u32 {
    u32::try_from(from_top.saturating_add(1)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clic_undo_rend_la_rangee_cliquee_tete() {
        // 3 pas [A, B, C] : clic B (index 1) = annuler C seul, B en tête.
        assert_eq!(back_steps_for_undo_click(3, 1), 1);
        // Clic A (le plus ancien) = annuler B + C, A en tête.
        assert_eq!(back_steps_for_undo_click(3, 0), 2);
        // Tête non cliquable en UI ; garde-fou : jamais zéro.
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
