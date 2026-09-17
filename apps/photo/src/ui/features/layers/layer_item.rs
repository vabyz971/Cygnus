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

//! HUD d'un calque photo (ligne du panneau Calques, widget métier).
//!
//! Ligne en deux temps : rangée principale (bouton de visibilité,
//! visualiseur du calque, nom éditable au double-clic, bouton de
//! suppression en bout) puis bandeau compact des sous-couches
//! (filtres live `FX` et masques, avec boutons monter/descendre/
//! supprimer). AUCUN envoi moteur ici : toute interaction remonte
//! via [`LayerItemAction`], convertie en [`PhotoAction`](crate::commands::PhotoAction)
//! puis routée par `PhotoApp` (état local ou worker).

use super::types::PhotoLayerInfo;
use ui_kit::components::{IconButton, menu_row, menu_style};
use ui_kit::icons::{Icon, IconRegistry};
use ui_kit::theme::CygnusTheme;
use uuid::Uuid;

/// État du renommage inline (double-clic sur le nom, détenu par l'app).
#[derive(Debug, Clone, Default)]
pub struct LayerRenameState {
    /// Calque en cours d'édition (`None` = aucun).
    pub editing: Option<Uuid>,
    /// Tampon du champ de saisie.
    pub buffer: String,
}

/// Action remontée au layout puis convertie en
/// [`PhotoAction`](crate::commands::PhotoAction).
///
/// Sélection et renommage touchent l'état UI détenu par l'app ; tout
/// le reste est routé au worker par `PhotoApp::handle_action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerItemAction {
    /// Sélectionner ce calque.
    Select(Uuid),
    /// Valider le renommage (nom déjà rogné, non vide).
    RenameCommit { layer: Uuid, name: String },
    /// Basculer la visibilité.
    ToggleVisibility(Uuid),
    /// Supprimer ce calque.
    DeleteLayer(Uuid),
    /// Dupliquer ce calque.
    DuplicateLayer(Uuid),
    /// Déplacer un filtre dans sa pile.
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Déplacer un masque dans sa pile.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Supprimer un filtre.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Supprimer un masque.
    RemoveMask { owner: Uuid, mask: Uuid },
}

/// Dessine le contenu HUD d'une ligne de calque.
///
/// Toute icône passe par `Icon` (via [`IconButton`]). Retourne les actions,
/// traitées par l'app (jamais d'envoi worker direct).
#[allow(clippy::too_many_lines)]
pub fn draw_photo_layer_item(
    ui: &mut egui::Ui,
    layer: &PhotoLayerInfo,
    selected: bool,
    rename: &mut LayerRenameState,
) -> Vec<LayerItemAction> {
    let theme = CygnusTheme::dark();
    let mut actions = Vec::new();

    // Interaction de la rangée (clic = sélection, double-clic sur le
    // nom = renommage) — enregistrée AVANT le contenu pour peindre le
    // fond sélectionné/survolé sans retard d'une frame.
    let row_rect = ui.available_rect_before_wrap();
    let row_id = ui.make_persistent_id(("photo_layer_row", layer.id));
    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());
    if row_resp.clicked() {
        actions.push(LayerItemAction::Select(layer.id));
    }
    // Clic droit : sélectionne la rangée et ouvre le menu
    // contextuel (sections façon rerun : édition, visibilité).
    if row_resp.secondary_clicked() {
        actions.push(LayerItemAction::Select(layer.id));
    }
    egui::Popup::context_menu(&row_resp)
        .style(menu_style(&theme))
        .show(|ui| {
            if menu_row(ui, &theme, "Renommer") {
                rename.editing = Some(layer.id);
                rename.buffer.clone_from(&layer.name);
                ui.close();
            }
            if menu_row(ui, &theme, "Dupliquer") {
                actions.push(LayerItemAction::DuplicateLayer(layer.id));
                ui.close();
            }
            ui.separator();
            let visibility_label = if layer.visible { "Masquer" } else { "Afficher" };
            if menu_row(ui, &theme, visibility_label) {
                actions.push(LayerItemAction::ToggleVisibility(layer.id));
                ui.close();
            }
            if menu_row(ui, &theme, "Supprimer") {
                actions.push(LayerItemAction::DeleteLayer(layer.id));
                ui.close();
            }
        });

    // Fond de sélection sur toute la largeur de la ligne.
    if selected || row_resp.hovered() {
        let bg = if selected {
            theme.colors.item_selected
        } else {
            theme.colors.item_hover
        };
        ui.painter().rect_filled(row_rect, theme.radius.sm, bg);
    }

    ui.vertical(|ui| {
        ui.add_space(theme.spacing.xs);
        // Rangée principale : oeil, visualiseur, nom, suppression.
        ui.horizontal(|ui| {
            ui.add_space(theme.spacing.xs);
            let vis_icon = if layer.visible {
                Icon::Visibility
            } else {
                Icon::VisibilityOff
            };
            if IconButton::new(vis_icon)
                .tooltip("Afficher / masquer")
                .show(ui, &theme)
                .clicked()
            {
                actions.push(LayerItemAction::ToggleVisibility(layer.id));
            }
            // Visualiseur du calque : vignette symbolique (icône de
            // type encadrée ; les miniatures pixels viendront du cache
            // d'apparence moteur).
            ui.add_sized(
                egui::vec2(28.0, 28.0),
                egui::Button::new(IconRegistry::new().sized(layer.kind.icon(), 18.0)).frame(true),
            );
            ui.add_space(theme.spacing.xs);
            // Nom : double-clic = édition inline, Entrée = valider,
            // Échap = annuler, perte de focus = valider si modifié.
            if rename.editing == Some(layer.id) {
                let edit = egui::TextEdit::singleline(&mut rename.buffer)
                    .desired_width(ui.available_width() - 40.0)
                    .show(ui);
                if edit.response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Escape))
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
                // Focus immédiat à l'ouverture de l'édition.
                edit.response.request_focus();
            } else {
                let name_resp = ui.add(
                    egui::Label::new(
                        egui::RichText::new(&layer.name).size(theme.typography.body_size),
                    )
                    .truncate(),
                );
                if name_resp.double_clicked() {
                    rename.editing = Some(layer.id);
                    rename.buffer = layer.name.clone();
                }
            }
            // Bouton de suppression en bout de rangée.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if IconButton::new(Icon::Delete)
                    .tooltip("Supprimer ce calque")
                    .show(ui, &theme)
                    .clicked()
                {
                    actions.push(LayerItemAction::DeleteLayer(layer.id));
                }
            });
        });
        // Bandeau des sous-couches : filtres live puis masques, avec
        // réordonnancement (haut/bas) et suppression. Le drag & drop
        // imbriqué viendra plus tard : les boutons couvrent groupes,
        // filtres et masques dès maintenant.
        if !layer.filters.is_empty() || !layer.masks.is_empty() {
            egui::ScrollArea::horizontal()
                .max_height(24.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for filter in &layer.filters {
                            actions.extend(draw_sub_layer(
                                ui,
                                &theme,
                                layer.id,
                                filter.id,
                                &filter.name,
                                true,
                            ));
                        }
                        for mask in &layer.masks {
                            actions.extend(draw_sub_layer(
                                ui, &theme, layer.id, mask.id, &mask.name, false,
                            ));
                        }
                    });
                });
        }
    });
    actions
}

/// Pastille d'une sous-couche (filtre ou masque) : nom + monter /
/// descendre / supprimer. Retourne les actions (routées au worker
/// par l'app).
#[allow(clippy::too_many_arguments)]
fn draw_sub_layer(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    owner: Uuid,
    sub_id: Uuid,
    name: &str,
    is_filter: bool,
) -> Vec<LayerItemAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
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
        ui.label(
            egui::RichText::new(name)
                .size(theme.typography.caption_size)
                .color(theme.colors.fg_secondary),
        );
        let (up_tip, down_tip, del_tip) = if is_filter {
            (
                "Monter le filtre",
                "Descendre le filtre",
                "Supprimer le filtre",
            )
        } else {
            (
                "Monter le masque",
                "Descendre le masque",
                "Supprimer le masque",
            )
        };
        // Monte/descend : pas de flèches au registre d'icônes —
        // petits boutons texte ASCII ("^"/"v"), jamais d'unicode.
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
        if IconButton::new(Icon::Close)
            .tooltip(del_tip)
            .show(ui, theme)
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
        ui.separator();
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
        let layer = fixture_layer();
        let mut rename = LayerRenameState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let actions = draw_photo_layer_item(ui, &layer, true, &mut rename);
                assert!(actions.is_empty(), "aucun clic sans interaction");
                // Tous les types, masqué / visible.
                for kind in [
                    PhotoLayerKind::Pixel,
                    PhotoLayerKind::Group,
                    PhotoLayerKind::Adjustment,
                ] {
                    let mut probing = layer.clone();
                    probing.kind = kind;
                    probing.visible = false;
                    let _ = draw_photo_layer_item(ui, &probing, false, &mut rename);
                }
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn item_without_interaction_reports_nothing() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let layer = fixture_layer();
        let mut rename = LayerRenameState::default();
        let mut reported = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                reported = draw_photo_layer_item(ui, &layer, false, &mut rename);
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
}
