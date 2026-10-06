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
//! Ligne en une rangée centrée (`Align::Center`) : chevron
//! repli/dépli, bouton de visibilité, visualiseur du calque, nom
//! éditable au double-clic, bouton de suppression en bout. Les
//! filtres live et les masques sont des lignes enfants à part
//! entière ([`draw_attachment_row`]), indentées sous leur porteur.
//! AUCUN envoi moteur ici : toute interaction remonte via
//! [`LayerItemAction`], convertie en [`PhotoAction`](crate::commands::PhotoAction)
//! puis routée par `PhotoApp` (état local ou worker).

use super::types::{LayerTexts, LayerThumbView, PhotoLayerInfo, PhotoSubLayerInfo};
use crate::i18n::PhotoTextKey;
use ui_kit::components::{IconButton, menu_row, menu_style};
use ui_kit::i18n::TextKey;
use ui_kit::icons::{Icon, IconRegistry};
use ui_kit::theme::UiThemeExt;
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
    /// Sélectionner une pièce jointe (filtre ou masque) : le porteur
    /// devient la sélection principale (tree, O003+).
    SelectAttachment {
        /// Calque porteur.
        owner: Uuid,
        /// Filtre ou masque visé.
        id: Uuid,
    },
    /// Activer/désactiver un filtre live (œil de la ligne).
    ToggleFilter {
        /// Calque porteur.
        layer: Uuid,
        /// Filtre visé.
        filter: Uuid,
    },
    /// Activer/désactiver un masque (œil de la ligne).
    ToggleMask {
        /// Porteur.
        owner: Uuid,
        /// Masque visé.
        mask: Uuid,
    },
    /// Valider le renommage (nom déjà rogné, non vide).
    RenameCommit { layer: Uuid, name: String },
    /// Basculer la visibilité.
    ToggleVisibility(Uuid),
    /// Supprimer ce calque.
    DeleteLayer(Uuid),
    /// Dupliquer ce calque.
    DuplicateLayer(Uuid),
    /// Miroir horizontal (menu contextuel : agit sur la sélection,
    /// posée par le clic droit).
    FlipHorizontalSelected,
    /// Miroir vertical (idem).
    FlipVerticalSelected,
    /// Rotation 90° horaire (idem).
    RotateClockwiseSelected,
    /// Rotation 90° antihoraire (idem).
    RotateCounterclockwiseSelected,
    /// Rogner au document (idem).
    CropSelectedToDocument,
    /// Replier / déplier un groupe (état moteur, sans re-rendu).
    ToggleCollapsed(Uuid),
    /// Déplacer un filtre dans sa pile.
    MoveFilter { layer: Uuid, filter: Uuid, up: bool },
    /// Déplacer un masque dans sa pile.
    MoveMask { owner: Uuid, mask: Uuid, up: bool },
    /// Supprimer un filtre.
    RemoveFilter { layer: Uuid, filter: Uuid },
    /// Supprimer un masque.
    RemoveMask { owner: Uuid, mask: Uuid },
}

/// État replié/déplié des pièces jointes d'un calque (tree) :
/// mémoire egui, ouvert par défaut. Lu par la liste (filtrage des
/// lignes) et basculé par le chevron de la ligne du porteur.
pub fn attachments_open(ui: &egui::Ui, owner: Uuid) -> bool {
    let id = ui.make_persistent_id(("photo_attachments_open", owner));
    egui::collapsing_header::CollapsingState::load(ui.ctx(), id)
        .map(|state| state.is_open())
        .unwrap_or(true)
}

/// Bascule l'état ci-dessus (stocké même au premier clic : `load`
/// seul rend `None` sans état persisté, ce qui figerait le chevron).
pub fn set_attachments_open(ui: &egui::Ui, owner: Uuid, open: bool) {
    let id = ui.make_persistent_id(("photo_attachments_open", owner));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    if state.is_open() != open {
        state.toggle(ui);
        state.store(ui.ctx());
    }
}

/// Dessine le contenu HUD d'une ligne de calque.
///
/// Toute icône passe par `Icon` (via [`IconButton`]). La rangée est
/// en `Align::Center` : œil, vignette et nom sont centrés
/// verticalement (jamais de chevauchement : la hauteur est allouée
/// par la liste, voir `layer_list`). Retourne les actions, traitées
/// par l'app (jamais d'envoi worker direct).
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
            if menu_row(ui, &theme, catalog.get(TextKey::Rename)) {
                rename.editing = Some(layer.id);
                rename.buffer.clone_from(&layer.name);
                ui.close();
            }
            if menu_row(ui, &theme, catalog.get(TextKey::Duplicate)) {
                actions.push(LayerItemAction::DuplicateLayer(layer.id));
                ui.close();
            }
            // Transformations du calque (sélection déjà posée par le
            // clic droit) : sous-menu façon barre de menus.
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

    // Fond de sélection sur toute la largeur de la ligne.
    if selected || row_resp.hovered() {
        let bg = if selected {
            theme.colors.item_selected
        } else {
            theme.colors.item_hover
        };
        ui.painter().rect_filled(row_rect, theme.radius.sm, bg);
    }

    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Indentation hiérarchique (un cran par niveau).
        ui.add_space(depth as f32 * theme.spacing.lg);
        // Chevron repli/dépli (groupes avec enfants) ou espace
        // équivalent pour aligner les colonnes.
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
        // Chevron des pièces jointes (tree façon egui demo) : repli /
        // dépli des lignes filtres + masques sous leur porteur.
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
        // Visualiseur du calque : miniature pixels si disponible,
        // sinon vignette symbolique (icône de type encadrée).
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
        // Nom : double-clic = édition inline, Entrée = valider,
        // Échap = annuler, perte de focus = valider si modifié.
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
            // Focus immédiat à l'ouverture de l'édition.
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
        // Bouton de suppression en bout de rangée.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if IconButton::new(Icon::Delete)
                .tooltip(texts.get(PhotoTextKey::DeleteLayer))
                .show(ui, &theme)
                .clicked()
            {
                actions.push(LayerItemAction::DeleteLayer(layer.id));
            }
        });
    });
    actions
}

/// Ligne enfant d'une pièce jointe (filtre live ou masque) : nœud
/// d'arbre sélectionnable (clic = focus pour modification, voir
/// [`LayerItemAction::SelectAttachment`]), œil on/off, indentée sous
/// son porteur. Retourne les actions (routées au worker par l'app).
/// Ni renommable ni déplaçable par DnD (ordre aux boutons
/// monter/descendre, comme avant).
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
    // Clic = focus (sélection secondaire, porteur sélectionné aussi).
    let row_rect = ui.available_rect_before_wrap();
    let row_id = ui.make_persistent_id(("photo_attachment_row", owner, sub_id));
    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());
    if row_resp.clicked() {
        actions.push(LayerItemAction::SelectAttachment { owner, id: sub_id });
    }
    // Fond de sélection sur toute la largeur de la ligne.
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
        // Colonne du chevron (vide ici : alignée sur les calques).
        ui.add_space(theme.sizes.icon_button);
        // Colonne du chevron pièces jointes (vide : alignée).
        ui.add_space(theme.sizes.icon_button);
        // Œil on/off (bascule sans perdre réglages ni pixels).
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
        // Monte/descend : pas de flèches au registre d'icônes —
        // petits boutons texte ASCII ("^"/"v"), jamais d'unicode.
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
                // Tous les types, masqué / visible.
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
        // Ouvert par défaut, même sans état stocké.
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert!(attachments_open(ui, owner));
            });
        })
        .drop_without_applying_deltas();
        // Premier clic : bascule vraiment (régression : `load` seul
        // rendait `None` et figeait le chevron).
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
