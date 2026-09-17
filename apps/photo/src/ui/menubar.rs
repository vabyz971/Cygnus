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

//! Menu contextuel PHOTO (barre haute, widget métier).
//!
//! Six menus : Fichier (nouveau document avec paramètres, ouvrir,
//! exporter avec paramètres, quitter), Édition (annuler, rétablir),
//! Calque (vide, image, dupliquer, masque, supprimer), Affichage
//! (grille, zoom), Fenêtre (rouvrir un panneau fermé, réinitialiser
//! la disposition des docks), Aide. Items via [`ui_kit::components::menu`].
//! Le style de la barre utilise des coins carrés via `menu_bar_style`,
//! les sous‑menus gardent le rayon arrondi du thème.
//! Les libellés génériques viennent du catalogue [`Catalog`](ui_kit::i18n::Catalog),
//! les chaînes spécifiques à Photo restent des `&str` locaux ;
//! les fenêtres de paramètres (nouveau document, export) sont des
//! modales détenues par l'app (voir `super::dialogs`).

use crate::layout::dock::PhotoDockTab;

use ui_kit::components::menu::{menu_bar_style, menu_item, menu_item_enabled, menu_style};
use ui_kit::i18n::{Catalog, TextKey};
use ui_kit::theme::CygnusTheme;

/// Action rapportée par [`draw_menu_bar`], traitée par l'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoMenuAction {
    /// Nouveau document (ouvre la fenêtre de paramètres).
    NewDocument,
    /// Ouvrir une image (file picker non bloquant).
    OpenImage,
    /// Exporter (ouvre la fenêtre de paramètres d'export).
    Export,
    /// Quitter l'application.
    Quit,
    /// Annuler / rétablir (worker).
    Undo,
    /// Rétablir.
    Redo,
    /// Nouveau calque vide.
    AddEmptyLayer,
    /// Dupliquer le calque sélectionné.
    DuplicateLayer,
    /// Ajouter un masque au calque sélectionné.
    AddMask,
    /// Supprimer le calque sélectionné.
    DeleteLayer,
    /// Basculer la grille du canvas.
    ToggleGrid,
    /// Zoom avant / arrière (ancré au centre).
    ZoomIn,
    /// Zoom arrière.
    ZoomOut,
    /// Réinitialiser le zoom à 100 %.
    ZoomReset,
    /// Fermer le document actif.
    CloseDocument,
    /// Rouvrir un onglet dock fermé.
    ShowDockTab(PhotoDockTab),
    /// Restaurer la disposition des docks par défaut.
    ResetDockLayout,
    /// Ouvrir la fenêtre d'aide.
    ShowHelp,
}

/// Disponibilités pour griser les entrées (historique, sélection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuAvailability {
    /// Undo / redo possibles.
    pub can_undo: bool,
    /// Redo possible.
    pub can_redo: bool,
    /// Un calque est sélectionné.
    pub has_selection: bool,
}

/// Dessine la barre de menus et retourne les actions.
///
/// Les items sont des boutons fantômes ui-kit (fond transparent,
/// surlignage egui natif au survol) ; le clic ferme le menu ouvert.
/// Les libellés génériques viennent du catalogue, les chaînes
/// spécifiques à Photo restent des `&str` locaux.
pub fn draw_menu_bar(
    ui: &mut egui::Ui,
    availability: MenuAvailability,
    theme: &CygnusTheme,
    catalog: Catalog,
) -> Vec<PhotoMenuAction> {
    let mut actions = Vec::new();
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(
            theme.spacing.sm as i8,
            theme.spacing.sm as i8,
        ))
        .show(ui, |ui| {
            // Appliquer le style de barre de menus (coins carrés)
            // via un Scope pour ne pas polluer le style global.
            let bar_style = {
                let s = ui.style_mut();
                menu_bar_style(theme).apply(s);
                s.clone()
            };
            ui.scope_builder(egui::UiBuilder::new().style(bar_style), |ui| {
                // Padding interne des titres de menus (plus grosses zones
                // cliquables, barre aérée), tout du thème. Les items
                // déroulants gardent leur hauteur fixe (min_size ui-kit).
                ui.spacing_mut().button_padding = egui::vec2(theme.spacing.sm, theme.spacing.sm);
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button(catalog.get(TextKey::File), |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        if menu_item(ui, theme, catalog.get(TextKey::NewDocument)) {
                            actions.push(PhotoMenuAction::NewDocument);
                        }
                        if menu_item(ui, theme, catalog.get(TextKey::Open)) {
                            actions.push(PhotoMenuAction::OpenImage);
                        }
                        if menu_item(ui, theme, catalog.get(TextKey::Export)) {
                            actions.push(PhotoMenuAction::Export);
                        }
                        ui.separator();
                        if menu_item(ui, theme, "Fermer le document") {
                            actions.push(PhotoMenuAction::CloseDocument);
                        }
                        if menu_item(ui, theme, catalog.get(TextKey::Quit)) {
                            actions.push(PhotoMenuAction::Quit);
                        }
                    });
                    ui.menu_button(catalog.get(TextKey::Edit), |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        if menu_item_enabled(
                            ui,
                            theme,
                            catalog.get(TextKey::Undo),
                            availability.can_undo,
                        ) {
                            actions.push(PhotoMenuAction::Undo);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            catalog.get(TextKey::Redo),
                            availability.can_redo,
                        ) {
                            actions.push(PhotoMenuAction::Redo);
                        }
                    });
                    ui.menu_button("Calque", |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        if menu_item(ui, theme, "Nouveau calque vide") {
                            actions.push(PhotoMenuAction::AddEmptyLayer);
                        }
                        if menu_item(ui, theme, "Calque depuis une image") {
                            actions.push(PhotoMenuAction::OpenImage);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            "Dupliquer le calque",
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::DuplicateLayer);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            "Ajouter un masque",
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::AddMask);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            "Supprimer le calque",
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::DeleteLayer);
                        }
                    });
                    ui.menu_button(catalog.get(TextKey::View), |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        if menu_item(ui, theme, catalog.get(TextKey::Grid)) {
                            actions.push(PhotoMenuAction::ToggleGrid);
                        }
                        if menu_item(ui, theme, catalog.get(TextKey::ZoomIn)) {
                            actions.push(PhotoMenuAction::ZoomIn);
                        }
                        if menu_item(ui, theme, catalog.get(TextKey::ZoomOut)) {
                            actions.push(PhotoMenuAction::ZoomOut);
                        }
                        if menu_item(ui, theme, "Zoom 100 %") {
                            actions.push(PhotoMenuAction::ZoomReset);
                        }
                    });
                    ui.menu_button(catalog.get(TextKey::Window), |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        // Canevas épinglés (non fermables) : seuls
                        // outils, inspecteur et calques sont à rouvrir.
                        for tab in [
                            PhotoDockTab::Tools,
                            PhotoDockTab::Inspector,
                            PhotoDockTab::Layers,
                        ] {
                            if menu_item(ui, theme, catalog.get(tab.title_key())) {
                                actions.push(PhotoMenuAction::ShowDockTab(tab));
                            }
                        }
                        ui.separator();
                        if menu_item(ui, theme, "Réinitialiser la disposition") {
                            actions.push(PhotoMenuAction::ResetDockLayout);
                        }
                    });
                    ui.menu_button(catalog.get(TextKey::Help), |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        if menu_item(ui, theme, "À propos") {
                            actions.push(PhotoMenuAction::ShowHelp);
                        }
                    });
                });
            });
        });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_kit::i18n::Language;

    #[test]
    fn menu_bar_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let theme = CygnusTheme::dark();
        for language in [Language::En, Language::Fr] {
            let catalog = Catalog::new(language);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let actions = draw_menu_bar(ui, MenuAvailability::default(), &theme, catalog);
                    assert!(actions.is_empty(), "aucun clic sans interaction");
                });
            })
            .drop_without_applying_deltas();
        }
    }
}
