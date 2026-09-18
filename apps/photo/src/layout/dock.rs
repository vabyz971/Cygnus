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

//! Tuiles ancrables Photo via `egui_tiles` (moteur de rerun).
//!
//! ```text
//! PhotoWorkspace (top/bottom fixes)
//!   ↓ CentralPanel (Tree)
//! PhotoDockTab (Tools / Canvas(id) / Inspector / Layers)
//!   ↓
//! Contenus métier (left_sidebar / central_view / right_sidebar)
//! ```
//!
//! Les barres haute (menus, modes) et basse (statut) restent des
//! `egui::Panel` fixes. Chaque document ouvert a son panneau
//! « Canevas », titré `nom | zoom %` et épinglé (non fermable) :
//! l'activer active le document, le fermer passe par le menu
//! Fichier. Le rail d'outils est compact, titré d'une icône de
//! drag (la barre d'onglet porte le DnD) ; l'inspecteur est
//! au-dessus des calques, tous deux avec padding interne `sm`.
//! Masquer un panneau (croix) le rend
//! invisible en gardant sa place ; le menu Fenêtre le réaffiche
//! ([`PhotoAction::ShowDockTab`](crate::commands::PhotoAction)).
//! Le style hérite d'egui (donc du thème Cygnus) : aucune couleur
//! en dur ici.

use super::{central_view, left_sidebar, right_sidebar};
use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::OpenDocument;
use egui_tiles::{Behavior, Container, TileId, Tiles, Tree, UiResponse};
use serde::{Deserialize, Serialize};
use ui_kit::i18n::TextKey;
use ui_kit::icons::{Icon, IconRegistry};
use uuid::Uuid;

/// Panneau ancrable du workspace Photo (stable entre versions :
/// sérialisé dans les préférences, ne jamais renommer sans
/// migration — comme [`ui_kit::layout::PanelId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PhotoDockTab {
    /// Rail d'outils compact.
    Tools,
    /// Canevas du document `Uuid` (titre = nom + zoom).
    Canvas(Uuid),
    /// Inspecteur du calque sélectionné.
    Inspector,
    /// Liste des calques.
    Layers,
}

impl PhotoDockTab {
    /// Clé de traduction du titre (hors canevas : nom + zoom).
    pub fn title_key(self) -> TextKey {
        match self {
            Self::Tools => TextKey::Tools,
            Self::Canvas(_) => TextKey::Canvas,
            Self::Inspector => TextKey::Inspector,
            Self::Layers => TextKey::Layers,
        }
    }
}

/// Parts de largeur : outils ~1/30 (≈32 px à l'ouverture).
const SHARE_TOOLS: f32 = 1.0;
/// Parts de largeur : canevas ~22/30.
const SHARE_CANVAS: f32 = 22.0;
/// Parts de largeur : colonne droite ~7/30.
const SHARE_SIDE: f32 = 7.0;
/// Parts de hauteur : inspecteur 40 % de la colonne droite.
const SHARE_INSPECTOR: f32 = 2.0;
/// Parts de hauteur : calques 60 % de la colonne droite.
const SHARE_LAYERS: f32 = 3.0;

/// Layout par défaut : canevas au centre, outils à gauche,
/// inspecteur au-dessus des calques à droite.
pub fn default_tree(canvases: Vec<PhotoDockTab>) -> Tree<PhotoDockTab> {
    debug_assert!(
        canvases
            .iter()
            .all(|tab| matches!(tab, PhotoDockTab::Canvas(_))),
        "la racine ne porte que des canevas"
    );
    let mut tiles = Tiles::default();
    let tools = tiles.insert_pane(PhotoDockTab::Tools);
    let canvas_ids: Vec<TileId> = canvases
        .into_iter()
        .map(|tab| tiles.insert_pane(tab))
        .collect();
    let canvas_tabs = tiles.insert_tab_tile(canvas_ids);
    let inspector = tiles.insert_pane(PhotoDockTab::Inspector);
    let layers = tiles.insert_pane(PhotoDockTab::Layers);
    let right = tiles.insert_vertical_tile(vec![inspector, layers]);
    let root = tiles.insert_horizontal_tile(vec![tools, canvas_tabs, right]);
    set_linear_shares(
        &mut tiles,
        root,
        &[
            (tools, SHARE_TOOLS),
            (canvas_tabs, SHARE_CANVAS),
            (right, SHARE_SIDE),
        ],
    );
    set_linear_shares(
        &mut tiles,
        right,
        &[(inspector, SHARE_INSPECTOR), (layers, SHARE_LAYERS)],
    );
    Tree::new("photo-tree", root, tiles)
}

/// Assigne les parts d'un conteneur linéaire (ignore silencieusement
/// un conteneur manquant : arbre restauré d'une ancienne version).
fn set_linear_shares(tiles: &mut Tiles<PhotoDockTab>, container: TileId, shares: &[(TileId, f32)]) {
    if let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tiles.get_mut(container) {
        for (child, share) in shares {
            linear.shares.set_share(*child, *share);
        }
    }
}

/// Vrai si `tab` existe quelque part dans l'arbre.
pub fn has_tab(tree: &Tree<PhotoDockTab>, tab: PhotoDockTab) -> bool {
    tree.tiles.find_pane(&tab).is_some()
}

/// Trouve le `TileId` du panneau `tab`, s'il existe.
fn find_tile(tree: &Tree<PhotoDockTab>, tab: PhotoDockTab) -> Option<TileId> {
    tree.tiles.find_pane(&tab)
}

/// Réaffiche `tab` s'il a été masqué (idempotent), et l'active dans
/// son onglet. Les panneaux masqués gardent leur place grâce aux
/// tuiles invisibles d'`egui_tiles`.
pub fn ensure_tab(tree: &mut Tree<PhotoDockTab>, tab: PhotoDockTab) {
    if let Some(tile) = find_tile(tree, tab) {
        tree.set_visible(tile, true);
        activate_tile(tree, tile);
    }
}

/// Active `tile` dans son conteneur d'onglets, s'il y en a un.
fn activate_tile(tree: &mut Tree<PhotoDockTab>, tile: TileId) {
    if let Some(parent) = tree.tiles.parent_of(tile)
        && let Some(egui_tiles::Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent)
    {
        tabs.set_active(tile);
    }
}

/// Ajoute l'onglet canevas d'un document : dans le conteneur des
/// autres canevas si possible (onglets côte à côte, nouveau actif),
/// sinon dans le premier conteneur d'onglets trouvé.
pub fn push_canvas_tab(tree: &mut Tree<PhotoDockTab>, tab: PhotoDockTab) {
    // Feuille des canevas existants d'abord…
    let mut target = tree
        .tiles
        .iter()
        .filter_map(|(id, tile)| match tile {
            egui_tiles::Tile::Pane(PhotoDockTab::Canvas(_)) => tree.tiles.parent_of(*id),
            _ => None,
        })
        .find(|parent| {
            matches!(
                tree.tiles.get(*parent),
                Some(egui_tiles::Tile::Container(Container::Tabs(_)))
            )
        });
    // …sinon le premier conteneur d'onglets (repli : jamais vide en
    // pratique, le layout garantit le conteneur des canevas).
    if target.is_none() {
        target = tree.tiles.iter().find_map(|(id, tile)| {
            matches!(tile, egui_tiles::Tile::Container(Container::Tabs(_))).then_some(*id)
        });
    }
    if let Some(parent) = target {
        let id = tree.tiles.insert_pane(tab);
        if let Some(egui_tiles::Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent)
        {
            tabs.add_child(id);
            tabs.set_active(id);
        }
    }
}

/// Retire l'onglet canevas du document `id` (sans effet s'il est
/// absent). Utilisé à la fermeture d'un document.
pub fn remove_canvas_tab(tree: &mut Tree<PhotoDockTab>, id: Uuid) {
    if let Some(tile) = find_tile(tree, PhotoDockTab::Canvas(id)) {
        tree.remove_recursively(tile);
    }
}

/// Réconcilie un layout restauré avec les documents réellement
/// ouverts : les ids changent à chaque lancement, donc les canevas
/// persistés sont orphelins — on retire ceux sans document et on
/// ajoute ceux manquants (outils, inspecteur, calques gardent leurs
/// places et visibilités persistées).
pub fn reconcile_canvases(tree: &mut Tree<PhotoDockTab>, ids: &[Uuid]) {
    loop {
        let stale = tree
            .tiles
            .iter()
            .filter_map(|(id, tile)| match tile {
                egui_tiles::Tile::Pane(PhotoDockTab::Canvas(doc)) if !ids.contains(doc) => {
                    Some(*id)
                }
                _ => None,
            })
            .next();
        let Some(tile) = stale else {
            break;
        };
        tree.remove_recursively(tile);
    }
    for id in ids {
        let tab = PhotoDockTab::Canvas(*id);
        if !has_tab(tree, tab) {
            push_canvas_tab(tree, tab);
        }
    }
}

/// Dessine l'arbre dans le panneau central et retourne les actions
/// métier des panneaux.
///
/// Emprunts disjoints (`docs` / `active` vs `shell.tree`) : le
/// behavior ne prend jamais `PhotoApp` en entier, sinon `Tree::ui`
/// (qui emprunte l'arbre) refuserait de compiler.
pub fn show_tree(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    let mut behavior = PhotoTreeBehavior {
        docs: &mut app.docs,
        active: &mut app.active,
        ctx,
        actions: Vec::new(),
    };
    app.shell.tree.ui(&mut behavior, ui);
    behavior.actions
}

/// Contenu d'une tuile avec padding interne `sm` (inspecteur,
/// calques) : le style reste 100 % tokens du thème.
fn padded_tile<R>(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::NONE
        .inner_margin(egui::Margin::same(ctx.shared.theme().spacing.sm as i8))
        .show(ui, add_contents)
        .inner
}

/// Behavior `egui_tiles` : titres traduits + contenus métier.
///
/// Ne possède que les documents (jamais l'arbre) pour respecter le
/// découpage des emprunts de [`show_tree`]. Les tuiles ne sont
/// dessinées qu'avec au moins un document (le workspace affiche
/// l'accueil sinon) ; les bras défensifs évitent tout panic.
struct PhotoTreeBehavior<'a> {
    /// Documents ouverts (non vide quand l'arbre est dessiné).
    docs: &'a mut Vec<OpenDocument>,
    /// Index du document actif.
    active: &'a mut usize,
    /// Dépendances d'affichage (thème, traduction).
    ctx: &'a PhotoUiContext,
    /// Actions métier collectées pendant le frame.
    actions: Vec<PhotoAction>,
}

impl Behavior<PhotoDockTab> for PhotoTreeBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &PhotoDockTab) -> egui::WidgetText {
        match *pane {
            // Rail étroit : icône de drag au lieu du texte (le DnD
            // passe par la barre d'onglet, qu'il faut conserver).
            PhotoDockTab::Tools => IconRegistry::new().sized(Icon::DragHandle, 13.0).into(),
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
            _ => self.ctx.shared.translator().get(pane.title_key()).into(),
        }
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        pane: &mut PhotoDockTab,
    ) -> UiResponse {
        // Emprunts disjoints explicites : `ctx` est copié (référence
        // partagée `Copy`) avant l'emprunt mutable des documents.
        let ctx: &PhotoUiContext = self.ctx;
        match *pane {
            PhotoDockTab::Tools => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get_mut(index) else {
                    ui.label("Aucun document");
                    return UiResponse::None;
                };
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
                let Some(doc) = self.docs.get(index) else {
                    ui.label("Aucun document");
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_inspector_content(ui, doc, ctx)
                });
                self.actions.extend(inner);
            }
            PhotoDockTab::Layers => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get_mut(index) else {
                    ui.label("Aucun document");
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_layers_content(ui, doc, ctx)
                });
                self.actions.extend(inner);
            }
        }
        UiResponse::None
    }

    /// Les canevas sont épinglés : pas de croix, sinon un
    /// document serait fermé sans passer par le menu Fichier
    /// (dernier document jamais à zéro). Masquer un autre panneau
    /// le rend invisible en gardant sa place (menu Fenêtre).
    fn is_tab_closable(&self, _tiles: &Tiles<PhotoDockTab>, tile_id: TileId) -> bool {
        !matches!(
            _tiles.get(tile_id),
            Some(egui_tiles::Tile::Pane(PhotoDockTab::Canvas(_)))
        )
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<PhotoDockTab>, tile_id: TileId) -> bool {
        // `false` = la tuile reste : on la masque au lieu de la
        // détruire pour garder sa place dans le layout.
        tiles.set_visible(tile_id, false);
        false
    }

    /// Barre d'onglets partout (même à panneau unique, façon dock).
    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Panneaux fixes de test.
    const FIXED_TABS: [PhotoDockTab; 3] = [
        PhotoDockTab::Tools,
        PhotoDockTab::Inspector,
        PhotoDockTab::Layers,
    ];

    fn canvas(id: u128) -> PhotoDockTab {
        PhotoDockTab::Canvas(Uuid::from_u128(id))
    }

    #[test]
    fn default_layout_contains_tools_inspector_layers_and_canvases() {
        let tree = default_tree(vec![canvas(1), canvas(2)]);
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau manquant : {tab:?}");
        }
        assert!(has_tab(&tree, canvas(1)));
        assert!(has_tab(&tree, canvas(2)));
    }

    #[test]
    fn inspector_and_layers_share_the_right_column() {
        let tree = default_tree(vec![canvas(1)]);
        let inspector = find_tile(&tree, PhotoDockTab::Inspector).expect("inspecteur présent");
        let layers = find_tile(&tree, PhotoDockTab::Layers).expect("calques présents");
        let inspector_parent = tree.tiles.parent_of(inspector).expect("parent inspecteur");
        let layers_parent = tree.tiles.parent_of(layers).expect("parent calques");
        assert_eq!(inspector_parent, layers_parent, "même colonne droite");
        assert!(matches!(
            tree.tiles.get(inspector_parent),
            Some(egui_tiles::Tile::Container(Container::Linear(linear)))
                if linear.dir == egui_tiles::LinearDir::Vertical
        ));
    }

    #[test]
    fn tools_column_is_narrow() {
        let tree = default_tree(vec![canvas(1)]);
        let tools = find_tile(&tree, PhotoDockTab::Tools).expect("outils présents");
        let root = tree.root().expect("racine présente");
        let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tree.tiles.get(root)
        else {
            panic!("racine horizontale attendue");
        };
        let total: f32 = linear.shares.iter().map(|(_, share)| *share).sum();
        let tools_share = linear.shares[tools];
        assert!(
            tools_share / total < 0.1,
            "outils compacts : part {tools_share}/{total}"
        );
    }

    #[test]
    fn push_canvas_tab_groups_canvases_in_one_tabs_container() {
        let mut tree = default_tree(vec![canvas(1)]);
        push_canvas_tab(&mut tree, canvas(2));
        let first = find_tile(&tree, canvas(1)).expect("canevas 1 présent");
        let second = find_tile(&tree, canvas(2)).expect("canevas 2 présent");
        assert_eq!(
            tree.tiles.parent_of(first),
            tree.tiles.parent_of(second),
            "canevas en onglets côte à côte"
        );
    }

    #[test]
    fn closing_panel_hides_it_and_menu_reopens_it() {
        let mut tree = default_tree(vec![canvas(1)]);
        let inspector = find_tile(&tree, PhotoDockTab::Inspector).expect("inspecteur présent");
        // La croix masque sans détruire (place conservée).
        tree.set_visible(inspector, false);
        assert!(has_tab(&tree, PhotoDockTab::Inspector));
        assert!(!tree.is_visible(inspector));
        ensure_tab(&mut tree, PhotoDockTab::Inspector);
        assert!(tree.is_visible(inspector));
    }

    #[test]
    fn canvas_is_pinned_other_tabs_closeable() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            ctx: &photo_ctx,
            actions: Vec::new(),
        };
        let tree = default_tree(vec![canvas(1)]);
        for tab in FIXED_TABS {
            let tile = find_tile(&tree, tab).expect("panneau présent");
            assert!(
                behavior.is_tab_closable(&tree.tiles, tile),
                "{tab:?} fermable"
            );
        }
        let canvas_tile = find_tile(&tree, canvas(1)).expect("canevas présent");
        assert!(!behavior.is_tab_closable(&tree.tiles, canvas_tile));
    }

    #[test]
    fn canvas_title_shows_document_name_and_zoom() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        app.open_sized_tab(800, 600);
        let doc_id = app.active_doc_opt().expect("doc actif").id;
        let expected = app
            .docs
            .iter()
            .find(|doc| doc.id == doc_id)
            .expect("document présent")
            .title
            .clone();
        let mut behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            ctx: &photo_ctx,
            actions: Vec::new(),
        };
        let title = behavior.tab_title_for_pane(&PhotoDockTab::Canvas(doc_id));
        assert!(
            title.text().starts_with(expected.as_str()),
            "titre inattendu : {}",
            title.text()
        );
        assert!(title.text().contains('|'), "zoom absent du titre");
    }

    #[test]
    fn tools_tab_shows_graphic_title_for_dnd() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let mut behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            ctx: &photo_ctx,
            actions: Vec::new(),
        };
        let title = behavior.tab_title_for_pane(&PhotoDockTab::Tools);
        assert!(!title.text().is_empty(), "titre graphique attendu");
        assert_ne!(title.text(), "Outils", "pas de texte, juste l'icône");
    }

    #[test]
    fn reconcile_drops_stale_canvases_and_adds_missing_ones() {
        let mut tree = default_tree(vec![canvas(1)]);
        reconcile_canvases(&mut tree, &[Uuid::from_u128(2)]);
        assert!(!has_tab(&tree, canvas(1)));
        assert!(has_tab(&tree, canvas(2)));
        // Panneaux conservés.
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau perdu : {tab:?}");
        }
    }

    #[test]
    fn tree_survives_json_roundtrip() {
        use crate::persistence::ui_state::{load_tree_state, save_tree_state};

        let mut app = PhotoApp::new();
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let ctx_clone = ctx.clone();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(&ctx_clone, ui);
        })
        .drop_without_applying_deltas();
        let json = save_tree_state(&app.shell.tree).expect("serialisation des tuiles");
        let restored = load_tree_state(&json).expect("restauration des tuiles");
        for tab in FIXED_TABS {
            assert!(has_tab(&restored, tab), "panneau perdu : {tab:?}");
        }
        // Les canevas portent les ids des documents ouverts.
        for doc in &app.docs {
            assert!(has_tab(&restored, PhotoDockTab::Canvas(doc.id)));
        }
    }

    #[test]
    fn corrupt_or_missing_tree_json_falls_back_to_defaults() {
        use crate::persistence::load_tree_or_default;

        let ids = [Uuid::from_u128(7)];
        for tab in [PhotoDockTab::Tools, canvas(7), PhotoDockTab::Inspector] {
            assert!(has_tab(
                &load_tree_or_default(Some("pas du json"), &ids),
                tab
            ));
            assert!(has_tab(&load_tree_or_default(None, &ids), tab));
        }
    }
}
