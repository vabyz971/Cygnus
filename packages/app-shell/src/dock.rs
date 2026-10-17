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

//! Generic dock logic over `egui_tiles`.
//!
//! The functions below only know the [`DockTab`] trait:
//! finding and re-showing panels, grouped canvas tabs,
//! reconciliation with open documents, shares of linear
//! containers and the four-pane photo layout ([`default_tree`], a mere
//! convenience: apps are free to compose differently with the same
//! primitives). The `Behavior` (panel titles and contents) stays
//! in each app because it is domain-specific.

use egui_tiles::{Container, TileId, Tiles, Tree};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

/// Dockable panel of an app (implemented e.g. by
/// `PhotoDockTab`). Requires serialization: layouts persist
/// as JSON via preferences.
pub trait DockTab: Clone + PartialEq + Serialize + DeserializeOwned {
    /// Document carried by this panel, if it is a canvas.
    fn canvas_id(&self) -> Option<Uuid>;
    /// Builds the canvas for document `id`.
    fn canvas(id: Uuid) -> Self;
}

/// Width shares: tools ~1/30 (original photo values, a mere
/// reusable default).
pub const SHARE_TOOLS: f32 = 1.0;
/// Width shares: canvas ~22/30.
pub const SHARE_CANVAS: f32 = 22.0;
/// Width shares: side column ~7/30.
pub const SHARE_SIDE: f32 = 7.0;
/// Height shares: top of the side column (40 %).
pub const SHARE_INSPECTOR: f32 = 2.0;
/// Height shares: bottom of the side column (60 %).
pub const SHARE_LAYERS: f32 = 3.0;

/// Default layout: tools on the left, tabbed canvases in the
/// center, two stacked panels on the right. `canvases` carries only
/// canvases (checked in debug).
pub fn default_tree<T: DockTab>(
    tools: T,
    canvases: Vec<T>,
    side_top: T,
    side_bottom: T,
) -> Tree<T> {
    debug_assert!(
        canvases.iter().all(|tab| tab.canvas_id().is_some()),
        "la racine ne porte que des canevas"
    );
    let mut tiles = Tiles::default();
    let tools = tiles.insert_pane(tools);
    let canvas_ids: Vec<TileId> = canvases
        .into_iter()
        .map(|tab| tiles.insert_pane(tab))
        .collect();
    let canvas_tabs = tiles.insert_tab_tile(canvas_ids);
    let top = tiles.insert_pane(side_top);
    let bottom = tiles.insert_pane(side_bottom);
    let right = tiles.insert_vertical_tile(vec![top, bottom]);
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
        &[(top, SHARE_INSPECTOR), (bottom, SHARE_LAYERS)],
    );
    Tree::new("cygnus-tree", root, tiles)
}

/// Assigns the shares of a linear container (silently ignores
/// a missing container: tree restored from an older version).
pub fn set_linear_shares<T>(tiles: &mut Tiles<T>, container: TileId, shares: &[(TileId, f32)]) {
    if let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tiles.get_mut(container) {
        for (child, share) in shares {
            linear.shares.set_share(*child, *share);
        }
    }
}

/// True if `tab` exists somewhere in the tree.
pub fn has_tab<T: DockTab>(tree: &Tree<T>, tab: T) -> bool {
    tree.tiles.find_pane(&tab).is_some()
}

/// Finds the `TileId` of panel `tab`, if it exists.
fn find_tile<T: DockTab>(tree: &Tree<T>, tab: T) -> Option<TileId> {
    tree.tiles.find_pane(&tab)
}

/// Re-shows `tab` if it was hidden (idempotent), and activates it in
/// its tab strip. Hidden panels keep their place thanks to
/// `egui_tiles`' invisible tiles.
pub fn ensure_tab<T: DockTab>(tree: &mut Tree<T>, tab: T) {
    if let Some(tile) = find_tile(tree, tab) {
        tree.set_visible(tile, true);
        activate_tile(tree, tile);
    }
}

/// Activates `tile` in its tab container, if it has one.
fn activate_tile<T: DockTab>(tree: &mut Tree<T>, tile: TileId) {
    if let Some(parent) = tree.tiles.parent_of(tile)
        && let Some(egui_tiles::Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent)
    {
        tabs.set_active(tile);
    }
}

/// Adds a document's canvas tab: into the container of the
/// other canvases when possible (side-by-side tabs, new one active),
/// otherwise into the first tab container found.
pub fn push_canvas_tab<T: DockTab>(tree: &mut Tree<T>, tab: T) {
    let is_tabs = |tree: &Tree<T>, id: TileId| {
        matches!(
            tree.tiles.get(id),
            Some(egui_tiles::Tile::Container(Container::Tabs(_)))
        )
    };
    // Existing canvases' leaf first…
    let mut target = tree
        .tiles
        .iter()
        .filter_map(|(id, tile)| match tile {
            egui_tiles::Tile::Pane(pane) if pane.canvas_id().is_some() => tree.tiles.parent_of(*id),
            _ => None,
        })
        .find(|parent| is_tabs(tree, *parent));
    // …then an EMPTY tab container (the canvas slot left by the last
    // closed document). Never "the first tab container": side panels
    // are tab containers too, and iteration order is not stable.
    if target.is_none() {
        target = tree.tiles.iter().find_map(|(id, tile)| match tile {
            egui_tiles::Tile::Container(Container::Tabs(tabs)) if tabs.children.is_empty() => {
                Some(*id)
            }
            _ => None,
        });
    }
    let id = tree.tiles.insert_pane(tab);
    if let Some(parent) = target {
        if let Some(egui_tiles::Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent)
        {
            tabs.add_child(id);
            tabs.set_active(id);
        }
        return;
    }
    // The empty slot was simplified away: rebuild the center column
    // right after the tools in a horizontal root, or wrap the root.
    let slot = tree.tiles.insert_tab_tile(vec![id]);
    let root = tree.root();
    if let Some(root) = root
        && let Some(egui_tiles::Tile::Container(Container::Linear(linear))) =
            tree.tiles.get_mut(root)
        && linear.dir == egui_tiles::LinearDir::Horizontal
    {
        let at = linear.children.len().min(1);
        linear.children.insert(at, slot);
        linear.shares.set_share(slot, SHARE_CANVAS);
        return;
    }
    let children: Vec<TileId> = root.into_iter().chain(std::iter::once(slot)).collect();
    let wrapped = tree.tiles.insert_horizontal_tile(children);
    tree.root = Some(wrapped);
}

/// Removes the canvas tab of document `id` (no effect if
/// absent). Used when closing a document.
pub fn remove_canvas_tab<T: DockTab>(tree: &mut Tree<T>, id: Uuid) {
    if let Some(tile) = find_tile(tree, T::canvas(id)) {
        tree.remove_recursively(tile);
    }
}

/// Reconciles a restored layout with the actually
/// open documents: ids change on every launch, so persisted
/// canvases are orphaned — remove those without a document and add
/// the missing ones (tools and side panels keep their
/// persisted places and visibilities).
pub fn reconcile_canvases<T: DockTab>(tree: &mut Tree<T>, ids: &[Uuid]) {
    loop {
        let stale = tree
            .tiles
            .iter()
            .filter_map(|(id, tile)| match tile {
                egui_tiles::Tile::Pane(pane) => {
                    if let Some(doc) = pane.canvas_id()
                        && !ids.contains(&doc)
                    {
                        Some(*id)
                    } else {
                        None
                    }
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
        let tab = T::canvas(*id);
        if !has_tab(tree, tab.clone()) {
            push_canvas_tab(tree, tab);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// Test panel: rail, canvas, two side panels.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    enum TestTab {
        Tools,
        Canvas(Uuid),
        SideTop,
        SideBottom,
    }

    impl DockTab for TestTab {
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

    fn canvas(id: u128) -> TestTab {
        TestTab::Canvas(Uuid::from_u128(id))
    }

    fn tree_one_canvas() -> Tree<TestTab> {
        default_tree(
            TestTab::Tools,
            vec![canvas(1)],
            TestTab::SideTop,
            TestTab::SideBottom,
        )
    }

    #[test]
    fn default_layout_contains_every_pane() {
        let tree = default_tree(
            TestTab::Tools,
            vec![canvas(1), canvas(2)],
            TestTab::SideTop,
            TestTab::SideBottom,
        );
        for tab in [
            TestTab::Tools,
            TestTab::SideTop,
            TestTab::SideBottom,
            canvas(1),
            canvas(2),
        ] {
            assert!(has_tab(&tree, tab), "panneau manquant : {tab:?}");
        }
    }

    #[test]
    fn side_panes_share_the_right_column() {
        let tree = tree_one_canvas();
        let top = find_tile(&tree, TestTab::SideTop).expect("haut présent");
        let bottom = find_tile(&tree, TestTab::SideBottom).expect("bas présent");
        let top_parent = tree.tiles.parent_of(top).expect("parent haut");
        let bottom_parent = tree.tiles.parent_of(bottom).expect("parent bas");
        assert_eq!(top_parent, bottom_parent, "même colonne droite");
        assert!(matches!(
            tree.tiles.get(top_parent),
            Some(egui_tiles::Tile::Container(Container::Linear(linear)))
                if linear.dir == egui_tiles::LinearDir::Vertical
        ));
    }

    #[test]
    fn push_canvas_tab_groups_canvases_in_one_tabs_container() {
        let mut tree = tree_one_canvas();
        push_canvas_tab(&mut tree, canvas(2));
        let first = find_tile(&tree, canvas(1)).expect("canevas 1 présent");
        let second = find_tile(&tree, canvas(2)).expect("canevas 2 présent");
        assert_eq!(
            tree.tiles.parent_of(first),
            tree.tiles.parent_of(second),
            "canevas en onglets côte à côte"
        );
    }

    /// Removes every empty tab container (what egui_tiles'
    /// simplification does between frames).
    fn prune_empty_tabs(tree: &mut Tree<TestTab>) {
        let empty: Vec<TileId> = tree
            .tiles
            .iter()
            .filter_map(|(id, tile)| match tile {
                egui_tiles::Tile::Container(Container::Tabs(tabs)) if tabs.children.is_empty() => {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        for id in empty {
            tree.remove_recursively(id);
        }
    }

    #[test]
    fn push_canvas_tab_reuses_the_empty_canvas_slot() {
        let mut tree = tree_one_canvas();
        remove_canvas_tab(&mut tree, Uuid::from_u128(1));
        push_canvas_tab(&mut tree, canvas(2));
        let side = find_tile(&tree, TestTab::SideBottom).expect("bas présent");
        let new = find_tile(&tree, canvas(2)).expect("canevas présent");
        assert_ne!(tree.tiles.parent_of(new), tree.tiles.parent_of(side));
        assert_eq!(
            tree.tiles
                .parent_of(new)
                .and_then(|p| tree.tiles.parent_of(p)),
            tree.root()
        );
    }

    #[test]
    fn push_canvas_tab_rebuilds_a_pruned_canvas_slot_in_the_center() {
        let mut tree = tree_one_canvas();
        remove_canvas_tab(&mut tree, Uuid::from_u128(1));
        prune_empty_tabs(&mut tree);
        // Side panels in their own tab container: the old fallback
        // ("first tab container") could pick it.
        let bottom = find_tile(&tree, TestTab::SideBottom).expect("bas présent");
        let tabs = tree.tiles.insert_tab_tile(vec![]);
        if let Some(parent) = tree.tiles.parent_of(bottom)
            && let Some(egui_tiles::Tile::Container(Container::Linear(linear))) =
                tree.tiles.get_mut(parent)
            && let Some(pos) = linear.children.iter().position(|c| *c == bottom)
        {
            linear.children[pos] = tabs;
        }
        if let Some(egui_tiles::Tile::Container(Container::Tabs(t))) = tree.tiles.get_mut(tabs) {
            t.add_child(bottom);
        }
        push_canvas_tab(&mut tree, canvas(3));
        let new = find_tile(&tree, canvas(3)).expect("canevas présent");
        let slot = tree.tiles.parent_of(new).expect("conteneur");
        assert_ne!(slot, tabs, "jamais avec les panneaux latéraux");
        let Some(root) = tree.root() else {
            panic!("racine attendue");
        };
        let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tree.tiles.get(root)
        else {
            panic!("racine horizontale attendue");
        };
        assert_eq!(
            linear.children.get(1),
            Some(&slot),
            "au centre, après les outils"
        );
    }

    #[test]
    fn hiding_then_ensuring_tab_restores_visibility() {
        let mut tree = tree_one_canvas();
        let side = find_tile(&tree, TestTab::SideTop).expect("haut présent");
        tree.set_visible(side, false);
        assert!(has_tab(&tree, TestTab::SideTop));
        assert!(!tree.is_visible(side));
        ensure_tab(&mut tree, TestTab::SideTop);
        assert!(tree.is_visible(side));
    }

    #[test]
    fn remove_canvas_tab_drops_only_the_matching_canvas() {
        let mut tree = default_tree(
            TestTab::Tools,
            vec![canvas(1), canvas(2)],
            TestTab::SideTop,
            TestTab::SideBottom,
        );
        remove_canvas_tab(&mut tree, Uuid::from_u128(1));
        assert!(!has_tab(&tree, canvas(1)));
        assert!(has_tab(&tree, canvas(2)));
        remove_canvas_tab(&mut tree, Uuid::from_u128(9));
    }

    #[test]
    fn reconcile_drops_stale_canvases_and_adds_missing_ones() {
        let mut tree = tree_one_canvas();
        reconcile_canvases(&mut tree, &[Uuid::from_u128(2)]);
        assert!(!has_tab(&tree, canvas(1)));
        assert!(has_tab(&tree, canvas(2)));
        for tab in [TestTab::Tools, TestTab::SideTop, TestTab::SideBottom] {
            assert!(has_tab(&tree, tab), "panneau perdu : {tab:?}");
        }
    }
}
