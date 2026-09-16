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

//! Panneau Calques : contenu métier, indépendant de sa position.
//!
//! `LayersPanel` ne sait pas s'il est à gauche ou à droite : le
//! workspace (`crate::layout`) décide du placement. Le chrome vient
//! de ui-kit ([`containers::Panel`](ui_kit::containers::Panel)), la
//! liste de [`super::layer_list`], la barre d'outils d'ici. Aucun
//! envoi worker : tout remonte en [`LayerPanelAction`], converti en
//! [`PhotoAction`](crate::commands::PhotoAction) (voir
//! [`super::actions`)).

use super::layer_item::{LayerItemAction, LayerRenameState};
use super::layer_list::draw_layer_list;
use super::types::PhotoLayerInfo;
use crate::commands::PhotoUiContext;
use ui_kit::containers::Panel;
use ui_kit::layout::PanelId;
use ui_kit::utils::ReorderDragState;
use ui_kit::widgets::icon::{CygnusIcon, icon_button};
use uuid::Uuid;

/// Action du panneau calques : barre d'outils + interactions HUD.
///
/// Routée par `PhotoApp` (sélection locale, renommage et mutations
/// via le worker) après conversion (voir [`super::actions`]).
#[derive(Debug, Clone, PartialEq)]
pub enum LayerPanelAction {
    /// Ajouter une image (file picker).
    AddImage,
    /// Nouveau calque vide.
    AddEmpty,
    /// Dupliquer la sélection.
    Duplicate,
    /// Ouvrir le menu d'ajout de filtre.
    OpenFilterMenu,
    /// Ajouter un masque au calque sélectionné.
    AddMask,
    /// Supprimer la sélection.
    Delete,
    /// Sélectionner un calque (clic sur sa rangée).
    Select(Uuid),
    /// Valider le renommage d'un calque.
    RenameCommit { layer: Uuid, name: String },
    /// Réordonner (indices d'affichage, drop drag & drop).
    Reorder { from: usize, to: usize },
    /// Basculer la visibilité d'un calque (œil).
    ToggleVisibility(Uuid),
    /// Supprimer un calque (croix de la rangée).
    DeleteLayer(Uuid),
    /// Déplacer un filtre dans sa pile.
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Déplacer un masque dans sa pile.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Supprimer un filtre.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Supprimer un masque.
    RemoveMask { owner: Uuid, mask: Uuid },
}

/// Panneau Calques (contenu + chrome ui-kit, sans position).
pub struct LayersPanel;

impl LayersPanel {
    /// Dessine le panneau complet (titre traduit, liste, barre de
    /// boutons) et retourne les actions.
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        layers: &[PhotoLayerInfo],
        selected: Option<Uuid>,
        rename: &mut LayerRenameState,
        drag_state: &mut ReorderDragState,
    ) -> Vec<LayerPanelAction> {
        let theme = ctx.shared.theme();
        let title = ctx.shared.translator().get(PanelId::Layers.title_key());
        Panel::new(title)
            .show(ui, theme, |ui| {
                draw_layers_panel(ui, layers, selected, rename, drag_state)
            })
            .inner
    }
}

/// Liste bornée (réserve la barre de boutons) + barre bas.
fn draw_layers_panel(
    ui: &mut egui::Ui,
    layers: &[PhotoLayerInfo],
    selected: Option<Uuid>,
    rename: &mut LayerRenameState,
    drag_state: &mut ReorderDragState,
) -> Vec<LayerPanelAction> {
    let mut actions = Vec::new();
    // Liste bornée : réserve la barre de boutons en bas.
    let bar_height = 40.0;
    let list_height = (ui.available_height() - bar_height).max(80.0);
    let (item_actions, drop) = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), list_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                // Le renommage en cours sur un calque disparu : annuler.
                if rename
                    .editing
                    .is_some_and(|id| !layers.iter().any(|layer| layer.id == id))
                {
                    rename.editing = None;
                }
                draw_layer_list(ui, layers, selected, rename, drag_state)
            },
        )
        .inner;
    for item_action in item_actions {
        match item_action {
            LayerItemAction::Select(id) => actions.push(LayerPanelAction::Select(id)),
            LayerItemAction::RenameCommit { layer, name } => {
                actions.push(LayerPanelAction::RenameCommit { layer, name });
            }
            LayerItemAction::ToggleVisibility(id) => {
                actions.push(LayerPanelAction::ToggleVisibility(id));
            }
            LayerItemAction::DeleteLayer(id) => actions.push(LayerPanelAction::DeleteLayer(id)),
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
    if let Some((from, to)) = drop {
        actions.push(LayerPanelAction::Reorder { from, to });
    }
    // Barre de boutons bas : ajouter image, calque vide, dupliquer,
    // masque, filtre, supprimer.
    ui.horizontal(|ui| {
        if icon_button(ui, CygnusIcon::ImageIcon, Some("Ajouter une image")).clicked() {
            actions.push(LayerPanelAction::AddImage);
        }
        if icon_button(ui, CygnusIcon::Add, Some("Nouveau calque vide")).clicked() {
            actions.push(LayerPanelAction::AddEmpty);
        }
        ui.add_enabled_ui(selected.is_some(), |ui| {
            if icon_button(ui, CygnusIcon::Duplicate, Some("Dupliquer le calque")).clicked() {
                actions.push(LayerPanelAction::Duplicate);
            }
            if icon_button(ui, CygnusIcon::Mask, Some("Ajouter un masque")).clicked() {
                actions.push(LayerPanelAction::AddMask);
            }
        });
        if icon_button(ui, CygnusIcon::Filter, Some("Liste des filtres")).clicked() {
            actions.push(LayerPanelAction::OpenFilterMenu);
        }
        ui.add_enabled_ui(selected.is_some(), |ui| {
            if icon_button(ui, CygnusIcon::Delete, Some("Supprimer le calque")).clicked() {
                actions.push(LayerPanelAction::Delete);
            }
        });
    });
    actions
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
                reported =
                    LayersPanel::show(ui, &photo_ctx, &layers, None, &mut rename, &mut drag_state);
            });
        })
        .drop_without_applying_deltas();
        assert!(reported.is_empty(), "aucun clic sans interaction");
        assert!(!drag_state.is_dragging);
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
                LayersPanel::show(ui, &photo_ctx, &[], None, &mut rename, &mut drag_state);
            });
        })
        .drop_without_applying_deltas();
    }
}
