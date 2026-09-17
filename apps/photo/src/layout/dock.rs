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

//! Docks ancrables Photo via `egui_dock`.
//!
//! ```text
//! PhotoWorkspace (top/bottom fixes)
//!   ↓ CentralPanel (DockArea)
//! PhotoDockTab (Tools / Canvas(id) / Inspector / Layers)
//!   ↓
//! Contenus métier (left_sidebar / central_view / right_sidebar)
//! ```
//!
//! Les barres haute (menus, modes) et basse (statut) restent des
//! `egui::Panel` fixes. Chaque document ouvert a son onglet
//! « Canevas », titré du nom du document et épinglé (non fermable) :
//! l'activer active le document, le fermer passe par le menu
//! Fichier. Le rail d'outils est compact (32 px) ; l'inspecteur est
//! au-dessus des calques. Outils, inspecteur et calques fermés se
//! rouvrent via le menu Fenêtre
//! ([`PhotoAction::ShowDockTab`](crate::commands::PhotoAction)).
//! Le style hérite d'egui (`Style::from_egui`, donc du thème
//! Cygnus) : aucune couleur en dur ici.

use super::{central_view, left_sidebar, right_sidebar};
use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::OpenDocument;
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use serde::{Deserialize, Serialize};
use ui_kit::i18n::TextKey;
use uuid::Uuid;

/// Onglet ancrable du workspace Photo (stable entre versions :
/// sérialisé dans les préférences, ne jamais renommer sans
/// migration — comme [`ui_kit::layout::PanelId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PhotoDockTab {
    /// Rail d'outils compact.
    Tools,
    /// Canevas du document `Uuid` (titre = nom du document).
    Canvas(Uuid),
    /// Inspecteur du calque sélectionné.
    Inspector,
    /// Liste des calques.
    Layers,
}

impl PhotoDockTab {
    /// Clé de traduction du titre (hors canevas : nom du document).
    pub fn title_key(self) -> TextKey {
        match self {
            Self::Tools => TextKey::Tools,
            Self::Canvas(_) => TextKey::Canvas,
            Self::Inspector => TextKey::Inspector,
            Self::Layers => TextKey::Layers,
        }
    }

    /// Identifiant egui stable (mémoire + drag & drop).
    pub fn dock_id(self) -> egui::Id {
        match self {
            Self::Tools => egui::Id::new("photo-dock-tools"),
            Self::Canvas(id) => egui::Id::new(id),
            Self::Inspector => egui::Id::new("photo-dock-inspector"),
            Self::Layers => egui::Id::new("photo-dock-layers"),
        }
    }
}

/// Layout par défaut : canevas au centre, outils à gauche (~4 %),
/// inspecteur au-dessus des calques à droite (40 / 60).
///
/// Attention : chez `egui_dock`, la fraction d'un `split_left` (ou
/// `split_above`) est la part du NOUVEAU nœud (outils), pas de
/// l'ancien — contrairement à `split_right` / `split_below`.
pub fn default_dock_state(canvases: Vec<PhotoDockTab>) -> DockState<PhotoDockTab> {
    debug_assert!(
        canvases
            .iter()
            .all(|tab| matches!(tab, PhotoDockTab::Canvas(_))),
        "la racine ne porte que des canevas"
    );
    let roots = if canvases.is_empty() {
        vec![PhotoDockTab::Tools]
    } else {
        canvases
    };
    let mut state = DockState::new(roots);
    {
        let surface = state.main_surface_mut();
        // Droite : 24 % pour inspecteur (haut) + calques (bas).
        let [canvas, right] =
            surface.split_right(NodeIndex::root(), 0.76, vec![PhotoDockTab::Inspector]);
        let [_inspector, _layers] = surface.split_below(right, 0.4, vec![PhotoDockTab::Layers]);
        // Gauche : rail d'outils compact (fraction = part du
        // nouveau nœud, ~4 % de la zone centrale soit ~32 px à
        // l'ouverture).
        surface.split_left(canvas, 0.04, vec![PhotoDockTab::Tools]);
    }
    apply_french_translations(&mut state);
    state
}

/// Libellés français des menus contextuels du dock (l'anglais est
/// le défaut de la crate ; `translations` n'est pas sérialisé, donc
/// à réappliquer après chaque chargement — voir
/// [`load_dock_or_default`](crate::persistence::load_dock_or_default)).
pub fn apply_french_translations(state: &mut DockState<PhotoDockTab>) {
    state.translations.tab_context_menu.close_button = String::from("Fermer l'onglet");
    state.translations.tab_context_menu.eject_button =
        String::from("Détacher dans une nouvelle fenêtre");
    state.translations.tab_context_menu.hide_tab_bar_button =
        String::from("Masquer la barre d'onglets");
    state.translations.tab_context_menu.show_tab_bar_button =
        String::from("Afficher la barre d'onglets");
}

/// Vrai si `tab` est ouvert quelque part (surface principale ou
/// fenêtre flottante).
pub fn has_tab(state: &DockState<PhotoDockTab>, tab: PhotoDockTab) -> bool {
    state.find_tab(&tab).is_some()
}

/// Rouvre `tab` s'il a été fermé (idempotent : sans effet s'il est
/// déjà ouvert). Le panneau rejoint la première feuille.
pub fn ensure_tab(state: &mut DockState<PhotoDockTab>, tab: PhotoDockTab) {
    if !has_tab(state, tab) {
        state.push_to_first_leaf(tab);
    }
}

/// Ajoute l'onglet canevas d'un document : dans la feuille des
/// autres canevas si elle existe (onglets côte à côte), sinon dans
/// la première feuille. Le nouvel onglet devient l'actif.
pub fn push_canvas_tab(state: &mut DockState<PhotoDockTab>, tab: PhotoDockTab) {
    let canvas_node = state
        .iter_all_tabs()
        .find(|(_, existing)| matches!(existing, PhotoDockTab::Canvas(_)))
        .map(|(path, _)| (path.surface, path.node));
    if let Some((surface, node)) = canvas_node
        && let Ok(leaf) = state[surface].leaf_mut(node)
    {
        leaf.append_tab(tab);
        return;
    }
    state.push_to_first_leaf(tab);
}

/// Réconcilie un layout restauré avec les documents réellement
/// ouverts : les ids changent à chaque lancement, donc les canevas
/// persistés sont orphelins — on retire ceux sans document et on
/// ajoute ceux manquants (outils, inspecteur, calques gardent leurs
/// positions persistées).
pub fn reconcile_canvases(state: &mut DockState<PhotoDockTab>, ids: &[Uuid]) {
    // Retire un par un : chaque `remove_tab` décale les index, les
    // chemins collectés d'avance seraient invalides après la
    // première suppression.
    loop {
        let stale = state
            .iter_all_tabs()
            .filter(|(_, tab)| matches!(tab, PhotoDockTab::Canvas(id) if !ids.contains(id)))
            .map(|(path, _)| path)
            .next();
        let Some(path) = stale else {
            break;
        };
        state.remove_tab(path);
    }
    for id in ids {
        let tab = PhotoDockTab::Canvas(*id);
        if !has_tab(state, tab) {
            push_canvas_tab(state, tab);
        }
    }
}

/// Dessine la zone de docks dans le panneau central et retourne les
/// actions métier des onglets.
///
/// Emprunts disjoints (`docs` / `active` vs `shell.dock_state`) :
/// le viewer ne prend jamais `PhotoApp` en entier, sinon le
/// `DockArea` (qui emprunte `dock_state`) refuserait de compiler.
pub fn show_dock_area(
    ui: &mut egui::Ui,
    app: &mut PhotoApp,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut viewer = PhotoTabViewer {
        docs: &mut app.docs,
        active: &mut app.active,
        ctx,
        actions: Vec::new(),
    };
    DockArea::new(&mut app.shell.dock_state)
        .style(Style::from_egui(ui.style().as_ref()))
        .show_close_buttons(true)
        .show_inside(ui, &mut viewer);
    viewer.actions
}

/// Viewer `egui_dock` : titres traduits + contenus métier.
///
/// Ne possède que les documents (jamais le `dock_state`) pour
/// respecter le découpage des emprunts de [`show_dock_area`].
struct PhotoTabViewer<'a> {
    /// Documents ouverts (toujours au moins un).
    docs: &'a mut Vec<OpenDocument>,
    /// Index du document actif.
    active: &'a mut usize,
    /// Dépendances d'affichage (thème, traduction).
    ctx: &'a PhotoUiContext,
    /// Actions métier collectées pendant le frame.
    actions: Vec<PhotoAction>,
}

impl TabViewer for PhotoTabViewer<'_> {
    type Tab = PhotoDockTab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        tab.dock_id()
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match *tab {
            // Titre document : nom + zoom. (Profil couleur et
            // profondeur 16/32 bits : le moteur est 100 % RGBA 8
            // bits sans profil — à ajouter ici quand le `Document`
            // exposera ces métadonnées.)
            PhotoDockTab::Canvas(id) => self
                .docs
                .iter()
                .find(|doc| doc.id == id)
                .map(|doc| {
                    format!(
                        "{} | {}%",
                        doc.title,
                        (doc.ui.viewport.zoom() * 100.0).round()
                    )
                })
                .unwrap_or_else(|| {
                    self.ctx
                        .shared
                        .translator()
                        .get(TextKey::Canvas)
                        .to_string()
                })
                .into(),
            _ => self.ctx.shared.translator().get(tab.title_key()).into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        // Emprunts disjoints explicites : `ctx` est copié (référence
        // partagée `Copy`) avant l'emprunt mutable des documents.
        let ctx: &PhotoUiContext = self.ctx;
        match *tab {
            PhotoDockTab::Tools => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let doc: &mut OpenDocument = &mut self.docs[index];
                self.actions.extend(left_sidebar::draw_tools_content(
                    ui,
                    doc,
                    ctx.shared.theme(),
                ));
            }
            PhotoDockTab::Canvas(id) => {
                // Activer l'onglet active le document.
                if let Some(index) = self.docs.iter().position(|doc| doc.id == id) {
                    *self.active = index;
                    let doc: &mut OpenDocument = &mut self.docs[index];
                    self.actions
                        .extend(central_view::draw_canvas_content(ui, doc, ctx));
                } else {
                    ui.label(ctx.shared.translator().get(TextKey::Close));
                }
            }
            PhotoDockTab::Inspector => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let doc: &OpenDocument = &self.docs[index];
                self.actions
                    .extend(right_sidebar::draw_inspector_content(ui, doc, ctx));
            }
            PhotoDockTab::Layers => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let doc: &mut OpenDocument = &mut self.docs[index];
                self.actions
                    .extend(right_sidebar::draw_layers_content(ui, doc, ctx));
            }
        }
    }

    /// Les canevas sont épinglés : pas de croix, sinon un
    /// document serait fermé sans passer par le menu Fichier
    /// (dernier document jamais à zéro). Les autres onglets se
    /// rouvrent via le menu Fenêtre.
    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        !matches!(tab, PhotoDockTab::Canvas(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quatre onglets de test (canevas sur ids stables).
    fn test_tabs() -> [PhotoDockTab; 4] {
        [
            PhotoDockTab::Tools,
            PhotoDockTab::Canvas(Uuid::from_u128(1)),
            PhotoDockTab::Inspector,
            PhotoDockTab::Layers,
        ]
    }

    #[test]
    fn default_layout_contains_tools_inspector_layers_and_canvases() {
        let canvases = vec![
            PhotoDockTab::Canvas(Uuid::from_u128(1)),
            PhotoDockTab::Canvas(Uuid::from_u128(2)),
        ];
        let state = default_dock_state(canvases.clone());
        for tab in test_tabs() {
            assert!(has_tab(&state, tab), "onglet manquant : {tab:?}");
        }
        assert!(has_tab(&state, canvases[1]));
    }

    #[test]
    fn inspector_and_layers_share_the_right_surface() {
        let state = default_dock_state(vec![PhotoDockTab::Canvas(Uuid::from_u128(1))]);
        let inspector = state
            .find_tab(&PhotoDockTab::Inspector)
            .expect("inspecteur présent");
        let layers = state
            .find_tab(&PhotoDockTab::Layers)
            .expect("calques présents");
        assert_eq!(inspector.surface, layers.surface);
        assert_ne!(inspector.node, layers.node, "calques sous l'inspecteur");
    }

    #[test]
    fn push_canvas_tab_groups_canvases_in_one_leaf() {
        let mut state = default_dock_state(vec![PhotoDockTab::Canvas(Uuid::from_u128(1))]);
        push_canvas_tab(&mut state, PhotoDockTab::Canvas(Uuid::from_u128(2)));
        let first = state
            .find_tab(&PhotoDockTab::Canvas(Uuid::from_u128(1)))
            .expect("canevas 1 présent");
        let second = state
            .find_tab(&PhotoDockTab::Canvas(Uuid::from_u128(2)))
            .expect("canevas 2 présent");
        assert_eq!(first.surface, second.surface);
        assert_eq!(first.node, second.node, "canevas en onglets côte à côte");
    }

    #[test]
    fn reconcile_drops_stale_canvases_and_adds_missing_ones() {
        let mut state = default_dock_state(vec![PhotoDockTab::Canvas(Uuid::from_u128(1))]);
        reconcile_canvases(&mut state, &[Uuid::from_u128(2)]);
        assert!(!has_tab(&state, PhotoDockTab::Canvas(Uuid::from_u128(1))));
        assert!(has_tab(&state, PhotoDockTab::Canvas(Uuid::from_u128(2))));
        // Panneaux conservés.
        for tab in [
            PhotoDockTab::Tools,
            PhotoDockTab::Inspector,
            PhotoDockTab::Layers,
        ] {
            assert!(has_tab(&state, tab), "panneau perdu : {tab:?}");
        }
    }

    #[test]
    fn canvas_is_pinned_other_tabs_closeable() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let viewer = PhotoTabViewer {
            docs: &mut app.docs,
            active: &mut app.active,
            ctx: &photo_ctx,
            actions: Vec::new(),
        };
        assert!(viewer.is_closeable(&PhotoDockTab::Tools));
        assert!(!viewer.is_closeable(&PhotoDockTab::Canvas(Uuid::from_u128(1))));
        assert!(viewer.is_closeable(&PhotoDockTab::Inspector));
        assert!(viewer.is_closeable(&PhotoDockTab::Layers));
    }

    #[test]
    fn canvas_title_shows_document_name() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let doc_id = app.docs[app.active.min(app.docs.len().saturating_sub(1))].id;
        let expected = app
            .docs
            .iter()
            .find(|doc| doc.id == doc_id)
            .expect("document présent")
            .title
            .clone();
        let mut viewer = PhotoTabViewer {
            docs: &mut app.docs,
            active: &mut app.active,
            ctx: &photo_ctx,
            actions: Vec::new(),
        };
        let title = viewer.title(&mut PhotoDockTab::Canvas(doc_id));
        assert!(
            title.text().starts_with(expected.as_str()),
            "titre inattendu : {}",
            title.text()
        );
        assert!(title.text().contains('|'), "zoom absent du titre");
    }

    #[test]
    fn ensure_tab_reopens_closed_panel_idempotently() {
        let mut state = default_dock_state(vec![PhotoDockTab::Canvas(Uuid::from_u128(1))]);
        // Ferme l'inspecteur où qu'il soit.
        if let Some(path) = state.find_tab(&PhotoDockTab::Inspector) {
            state.remove_tab(path);
        }
        assert!(!has_tab(&state, PhotoDockTab::Inspector));
        ensure_tab(&mut state, PhotoDockTab::Inspector);
        assert!(has_tab(&state, PhotoDockTab::Inspector));
        // Idempotent : pas de doublon.
        let before = state.iter_all_tabs().count();
        ensure_tab(&mut state, PhotoDockTab::Inspector);
        assert_eq!(state.iter_all_tabs().count(), before);
    }

    #[test]
    fn dock_state_survives_json_roundtrip() {
        use crate::persistence::ui_state::{load_dock_state, save_dock_state};

        // La sauvegarde a lieu après affichage : avant le premier
        // layout, les `Rect` internes valent `Rect::NOTHING`
        // (coordonnées infinies que `serde_json` écrit `null`).
        let mut app = PhotoApp::new();
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let ctx_clone = ctx.clone();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(&ctx_clone, ui);
        })
        .drop_without_applying_deltas();
        let json = save_dock_state(&app.shell.dock_state).expect("serialisation du dock");
        let restored = load_dock_state(&json).expect("restauration du dock");
        for tab in test_tabs() {
            if matches!(tab, PhotoDockTab::Canvas(_)) {
                continue;
            }
            assert!(has_tab(&restored, tab), "onglet perdu : {tab:?}");
        }
        // Les canevas portent les ids des documents ouverts.
        for doc in &app.docs {
            assert!(has_tab(&restored, PhotoDockTab::Canvas(doc.id)));
        }
    }

    #[test]
    fn corrupt_or_missing_dock_json_falls_back_to_defaults() {
        use crate::persistence::load_dock_or_default;

        let ids = [Uuid::from_u128(7)];
        for tab in test_tabs() {
            let tab = match tab {
                PhotoDockTab::Canvas(_) => PhotoDockTab::Canvas(ids[0]),
                other => other,
            };
            assert!(has_tab(
                &load_dock_or_default(Some("pas du json"), &ids),
                tab
            ));
            assert!(has_tab(&load_dock_or_default(None, &ids), tab));
        }
    }
}
