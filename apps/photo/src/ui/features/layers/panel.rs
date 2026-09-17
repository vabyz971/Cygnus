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
//! `LayersPanel` ne sait pas où il est affiché : le workspace
//! (`crate::layout`, onglet dock titré « Calques ») décide du
//! placement, sans chrome `Panel`. La liste vient de
//! [`super::layer_list`], la barre de boutons est en bas. Aucun
//! envoi worker : tout remonte en [`LayerPanelAction`], converti en
//! [`PhotoAction`](crate::commands::PhotoAction) (voir
//! [`super::actions`])).

use super::layer_item::{LayerItemAction, LayerRenameState};
use super::layer_list::draw_layer_list;
use super::types::PhotoLayerInfo;
use crate::commands::PhotoUiContext;
use photo_engine::BlendMode;
use ui_kit::components::{Select, Slider};
use ui_kit::primitives::Text;
use ui_kit::theme::CygnusTheme;
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
    /// Régler l'opacité du calque sélectionné (unités 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
    /// Régler le mode de fusion du calque sélectionné.
    SetBlendMode { layer: Uuid, mode: BlendMode },
}

/// Panneau Calques (contenu direct : liste + barre de boutons en
/// bas, sans chrome).
pub struct LayersPanel;

impl LayersPanel {
    /// Dessine le contenu (liste, barre de boutons bas) et retourne
    /// les actions.
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        layers: &[PhotoLayerInfo],
        selected: Option<Uuid>,
        rename: &mut LayerRenameState,
        drag_state: &mut ReorderDragState,
    ) -> Vec<LayerPanelAction> {
        draw_layers_panel(ui, ctx.shared.theme(), layers, selected, rename, drag_state)
    }
}

/// En-tête : opacité (slider coalescé côté worker) + mode de
/// fusion de la sélection. Sans sélection : hint explicite.
fn draw_selection_header(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    layers: &[PhotoLayerInfo],
    selected: Option<Uuid>,
) -> Vec<LayerPanelAction> {
    let mut actions = Vec::new();
    let Some(layer) = selected.and_then(|id| layers.iter().find(|item| item.id == id)) else {
        Text::caption(theme, "Sélectionnez un calque").show(ui);
        ui.separator();
        return actions;
    };
    // Unités moteur 0..=100 (comme l'inspecteur).
    let mut opacity = layer.opacity;
    Slider::new("Opacité", 0.0..=100.0).show(ui, theme, &mut opacity);
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
    Select::new("Fusion", &options).show(ui, theme, &mut choice);
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

/// En-tête + liste bornée (réserve la barre de boutons) + barre bas.
fn draw_layers_panel(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    layers: &[PhotoLayerInfo],
    selected: Option<Uuid>,
    rename: &mut LayerRenameState,
    drag_state: &mut ReorderDragState,
) -> Vec<LayerPanelAction> {
    let mut actions = Vec::new();
    actions.extend(draw_selection_header(ui, theme, layers, selected));
    // Liste bornée : réserve l'en-tête et la barre de boutons.
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
                    selected,
                    &mut rename,
                    &mut drag_state,
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
                LayersPanel::show(ui, &photo_ctx, &[], None, &mut rename, &mut drag_state);
            });
        })
        .drop_without_applying_deltas();
    }
}
