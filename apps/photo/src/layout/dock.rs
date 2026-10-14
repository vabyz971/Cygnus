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

//! Photo dockable tiles via `egui_tiles` (rerun engine).
//!
//! ```text
//! PhotoWorkspace (top/bottom fixed)
//!   ↓ CentralPanel (Tree)
//! PhotoDockTab (Tools / Canvas(id) / Inspector / Layers+History)
//!   ↓
//! Domain contents (left_sidebar / central_view / right_sidebar)
//! ```
//!
//! The top (menus, modes) and bottom (status) bars stay fixed
//! `egui::Panel`s. Each open document has its
//! "Canvas" panel, titled `name | zoom %` and CLOSABLE (tab cross):
//! activating it activates the document, closing it closes the document (back
//! to welcome if it was the last one). The tool rail is compact,
//! titled with a drag icon (the tab bar carries DnD);
//! the inspector sits above the layers, both with inner
//! `sm` padding. Hiding another panel (cross) makes it
//! invisible while keeping its slot; the Window menu shows it again
//! ([`PhotoAction::ShowDockTab`](crate::commands::PhotoAction)).
//! Styling inherits from egui (hence the Cygnus theme): no hardcoded
//! color here.

use super::{central_view, left_sidebar, right_sidebar};
use crate::app::{PhotoApp, remove_document};
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::OpenDocument;
use app_shell::dock::DockTab;
use cygnus_i18n::TextKey;
use cygnus_ui::icons::{Icon, IconRegistry};
use egui_tiles::{Behavior, TileId, Tiles, Tree, UiResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Generic dock mechanics (lookup, canvas tabs,
/// reconciliation): see `app-shell`.
pub use app_shell::dock::{ensure_tab, push_canvas_tab, reconcile_canvases, remove_canvas_tab};

/// Dockable Photo workspace panel (stable across versions:
/// serialized into preferences, never rename without
/// migration — like [`cygnus_ui::layout::PanelId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PhotoDockTab {
    /// Compact tool rail.
    Tools,
    /// Canvas of document `Uuid` (title = name + zoom).
    Canvas(Uuid),
    /// Inspector of the selected layer.
    Inspector,
    /// Layer list.
    Layers,
    /// Undo/redo step history (tabbed with Layers).
    History,
}

impl PhotoDockTab {
    /// Title translation key (except canvas: name + zoom).
    pub fn title_key(self) -> TextKey {
        match self {
            Self::Tools => TextKey::Tools,
            Self::Canvas(_) => TextKey::Canvas,
            Self::Inspector => TextKey::Inspector,
            Self::Layers => TextKey::Layers,
            Self::History => TextKey::History,
        }
    }
}

impl DockTab for PhotoDockTab {
    fn canvas_id(&self) -> Option<Uuid> {
        match self {
            Self::Canvas(id) => Some(*id),
            _ => None,
        }
    }

    fn canvas(id: Uuid) -> Self {
        Self::Canvas(id)
    }
}

/// Default photo layout: canvas at center, tools on the left,
/// inspector above layers on the right, History tabbed
/// with layers (see `app-shell::dock::default_tree`).
pub fn default_tree(canvases: Vec<PhotoDockTab>) -> Tree<PhotoDockTab> {
    let mut tree = app_shell::dock::default_tree(
        PhotoDockTab::Tools,
        canvases,
        PhotoDockTab::Inspector,
        PhotoDockTab::Layers,
    );
    ensure_history_tabbed(&mut tree);
    tree
}

/// History tabbed with Layers (bottom of the right column).
///
/// Idempotent: no effect if the tab already exists (trees restored
/// from an earlier JSON: see `load_tree_or_default`).
pub fn ensure_history_tabbed(tree: &mut Tree<PhotoDockTab>) {
    if app_shell::dock::has_tab(tree, PhotoDockTab::History) {
        return;
    }
    let Some(layers_tile) = tree.tiles.find_pane(&PhotoDockTab::Layers) else {
        return;
    };
    let Some(parent) = tree.tiles.parent_of(layers_tile) else {
        return;
    };
    // Position read BEFORE any mutation: on an unexpected container,
    // touch nothing (never an orphaned tile).
    let position = match tree.tiles.get(parent) {
        Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(linear))) => linear
            .children
            .iter()
            .position(|child| *child == layers_tile),
        _ => None,
    };
    let Some(position) = position else {
        return;
    };
    let history = tree.tiles.insert_pane(PhotoDockTab::History);
    let tabs = tree.tiles.insert_tab_tile(vec![layers_tile, history]);
    if let Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(linear))) =
        tree.tiles.get_mut(parent)
    {
        linear.children[position] = tabs;
    }
}

/// Draws the tree in the central panel and returns the panels'
/// domain actions.
///
/// Disjoint borrows (`docs` / `active` vs `shell.tree`): the
/// behavior never takes the whole `PhotoApp`, otherwise `Tree::ui`
/// (which borrows the tree) would refuse to compile.
pub fn show_tree(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    let mut behavior = PhotoTreeBehavior {
        docs: &mut app.docs,
        active: &mut app.active,
        filter_types: &app.runtime.filter_types,
        ctx,
        actions: Vec::new(),
    };
    app.shell.tree.ui(&mut behavior, ui);
    behavior.actions
}

/// Tile content with inner `sm` padding (inspector,
/// layers): styling stays 100% theme tokens.
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

/// `egui_tiles` behavior: translated titles + domain contents.
///
/// Owns only the documents (never the tree) to honor the
/// borrow split of [`show_tree`]. Tiles are only
/// drawn with at least one document (the workspace shows
/// the welcome screen otherwise); defensive arms avoid any panic.
struct PhotoTreeBehavior<'a> {
    /// Open documents (non-empty when the tree is drawn).
    docs: &'a mut Vec<OpenDocument>,
    /// Active document index.
    active: &'a mut usize,
    /// Filter catalog (engine registry: name, `type_id`) for
    /// the Layers panel context menu.
    filter_types: &'a [(String, String)],
    /// Display dependencies (theme, translation).
    ctx: &'a PhotoUiContext,
    /// Domain actions collected during the frame.
    actions: Vec<PhotoAction>,
}

impl Behavior<PhotoDockTab> for PhotoTreeBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &PhotoDockTab) -> egui::WidgetText {
        match *pane {
            // Narrow rail: drag icon instead of text (DnD
            // goes through the tab bar, which must be kept).
            PhotoDockTab::Tools => IconRegistry::new()
                .sized(
                    Icon::DragHandle,
                    self.ctx.shared.theme().typography.body_size,
                )
                .into(),
            // Document title: name + resolution + zoom (resolution
            // lives here, never as an overlay on the canvas). (Color profile
            // and 16/32-bit depth: the engine is 100% RGBA 8-bit
            // with no profile — add here when `Document`
            // exposes that metadata.)
            PhotoDockTab::Canvas(id) => self
                .docs
                .iter()
                .find(|doc| doc.id == id)
                .map(|doc| {
                    let zoom = (doc.ui.viewport.zoom() * 100.0).round();
                    match doc.ui.preview_geom {
                        Some(geom) => format!(
                            "{} | {}x{} | {}%",
                            doc.title, geom.doc_size.x as u32, geom.doc_size.y as u32, zoom
                        ),
                        None => format!("{} | {}%", doc.title, zoom),
                    }
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
        // Explicit disjoint borrows: `ctx` is copied (`Copy`
        // shared reference) before the mutable document borrow.
        let ctx: &PhotoUiContext = self.ctx;
        let translator = ctx.shared.translator();
        let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
        match *pane {
            PhotoDockTab::Tools => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get_mut(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let translator = ctx.shared.translator();
                let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
                self.actions.extend(left_sidebar::draw_tools_content(
                    ui,
                    doc,
                    ctx.shared.theme(),
                    texts,
                ));
            }
            PhotoDockTab::Canvas(id) => {
                // Activating the tab activates the document.
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
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
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
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_layers_content(ui, doc, ctx, self.filter_types)
                });
                self.actions.extend(inner);
            }
            PhotoDockTab::History => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_history_content(ui, doc, ctx)
                });
                self.actions.extend(inner);
            }
        }
        UiResponse::None
    }

    /// Every tab is closable: a canvas cross closes
    /// its document (see `on_tab_close`), any other panel's cross
    /// hides it while keeping its slot (Window menu to reopen).
    fn is_tab_closable(&self, _tiles: &Tiles<PhotoDockTab>, _tile_id: TileId) -> bool {
        true
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<PhotoDockTab>, tile_id: TileId) -> bool {
        // Canvas: close the DOCUMENT (removed from docs, worker stopped
        // when the channels drop) and destroy the tile (`true`).
        // Zero documents left = back to welcome (the workspace
        // only shows the tree while a document remains).
        if let Some(egui_tiles::Tile::Pane(PhotoDockTab::Canvas(id))) = tiles.get(tile_id) {
            let id = *id;
            remove_document(self.docs, self.active, id);
            return true;
        }
        // Other panel: hide instead of destroying to
        // keep its slot in the layout (`false` = the tile stays).
        tiles.set_visible(tile_id, false);
        false
    }

    /// Tab bar everywhere (even single-pane, dock-style).
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
    use app_shell::dock::has_tab;
    use egui_tiles::Container;

    /// Finds the `TileId` of panel `tab`, if any.
    fn find_tile(tree: &Tree<PhotoDockTab>, tab: PhotoDockTab) -> Option<TileId> {
        tree.tiles.find_pane(&tab)
    }

    /// Fixed test panels.
    const FIXED_TABS: [PhotoDockTab; 4] = [
        PhotoDockTab::Tools,
        PhotoDockTab::Inspector,
        PhotoDockTab::Layers,
        PhotoDockTab::History,
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
        let history = find_tile(&tree, PhotoDockTab::History).expect("historique présent");
        // Layers + History tabbed: same parent tile.
        let layers_parent = tree.tiles.parent_of(layers).expect("parent calques");
        let history_parent = tree.tiles.parent_of(history).expect("parent historique");
        assert_eq!(
            layers_parent, history_parent,
            "calques + historique en onglets"
        );
        assert!(
            matches!(
                tree.tiles.get(layers_parent),
                Some(egui_tiles::Tile::Container(Container::Tabs(_)))
            ),
            "conteneur d'onglets attendu"
        );
        // The tab shares the vertical right column with the inspector.
        let tabs_parent = tree.tiles.parent_of(layers_parent).expect("parent onglets");
        let inspector_parent = tree.tiles.parent_of(inspector).expect("parent inspecteur");
        assert_eq!(tabs_parent, inspector_parent, "même colonne droite");
        assert!(matches!(
            tree.tiles.get(tabs_parent),
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
        // The cross hides without destroying (slot kept).
        tree.set_visible(inspector, false);
        assert!(has_tab(&tree, PhotoDockTab::Inspector));
        assert!(!tree.is_visible(inspector));
        ensure_tab(&mut tree, PhotoDockTab::Inspector);
        assert!(tree.is_visible(inspector));
    }

    #[test]
    fn all_tabs_are_closable() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            filter_types: &[],
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
        assert!(
            behavior.is_tab_closable(&tree.tiles, canvas_tile),
            "canevas fermable (ferme son document)"
        );
    }

    #[test]
    fn closing_canvas_tab_closes_document_and_returns_home() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        app.open_sized_tab(64, 64);
        app.open_sized_tab(64, 64);
        let first = app.docs[0].id;
        let second = app.docs[1].id;
        let mut tree = default_tree(vec![
            PhotoDockTab::Canvas(first),
            PhotoDockTab::Canvas(second),
        ]);
        // Close the first canvas: document removed, tile destroyed
        // (`true` = egui_tiles destroys the tile — reproduced here with
        // `remove_recursively`), second document active.
        {
            let tile = find_tile(&tree, PhotoDockTab::Canvas(first)).expect("canevas 1");
            let mut behavior = PhotoTreeBehavior {
                docs: &mut app.docs,
                active: &mut app.active,
                filter_types: &[],
                ctx: &photo_ctx,
                actions: Vec::new(),
            };
            assert!(behavior.on_tab_close(&mut tree.tiles, tile));
            tree.remove_recursively(tile);
        }
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active_doc_opt().expect("doc restant").id, second);
        assert!(!has_tab(&tree, PhotoDockTab::Canvas(first)));
        assert!(has_tab(&tree, PhotoDockTab::Canvas(second)));
        // Close the last one: zero documents = back to welcome.
        {
            let tile = find_tile(&tree, PhotoDockTab::Canvas(second)).expect("canevas 2");
            let mut behavior = PhotoTreeBehavior {
                docs: &mut app.docs,
                active: &mut app.active,
                filter_types: &[],
                ctx: &photo_ctx,
                actions: Vec::new(),
            };
            assert!(behavior.on_tab_close(&mut tree.tiles, tile));
            tree.remove_recursively(tile);
        }
        assert!(app.docs.is_empty(), "accueil : aucun document");
        assert!(!has_tab(&tree, PhotoDockTab::Canvas(second)));
        // Panels kept for the next opening.
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau perdu : {tab:?}");
        }
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
        // Without preview: name + zoom (fallback before the first response).
        {
            let mut behavior = PhotoTreeBehavior {
                docs: &mut app.docs,
                active: &mut app.active,
                filter_types: &[],
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
        // With preview: resolution joins the title (never the overlay).
        app.active_doc_mut_opt().expect("doc actif").ui.preview_geom =
            Some(crate::state::PreviewGeom {
                thumb_size: egui::vec2(800.0, 600.0),
                full_size: egui::vec2(800.0, 600.0),
                origin: egui::Vec2::ZERO,
                doc_size: egui::vec2(800.0, 600.0),
            });
        {
            let mut behavior = PhotoTreeBehavior {
                docs: &mut app.docs,
                active: &mut app.active,
                filter_types: &[],
                ctx: &photo_ctx,
                actions: Vec::new(),
            };
            let title = behavior.tab_title_for_pane(&PhotoDockTab::Canvas(doc_id));
            assert!(
                title.text().contains("800x600"),
                "résolution absente du titre : {}",
                title.text()
            );
        }
    }

    #[test]
    fn tools_tab_shows_graphic_title_for_dnd() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let mut behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            filter_types: &[],
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
        // Panels kept.
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau perdu : {tab:?}");
        }
    }

    #[test]
    fn tree_survives_json_roundtrip() {
        use crate::persistence::ui_state::{load_tree_state, save_tree_state};

        let mut app = PhotoApp::new();
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
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
        // Canvases carry the open documents' ids.
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
