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

//! Logique de dock générique sur `egui_tiles`.
//!
//! Les fonctions ci-dessous ne connaissent que le trait [`DockTab`] :
//! recherche et réaffichage de panneaux, onglets de canevas groupés,
//! réconciliation avec les documents ouverts, parts des conteneurs
//! linéaires et layout photo à quatre volets ([`default_tree`], simple
//! commodité : les apps libres de composer autrement avec les mêmes
//! primitives). Le `Behavior` (titres, contenus des panneaux) reste
//! dans chaque app car il est métier.

use egui_tiles::{Container, TileId, Tiles, Tree};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

/// Panneau ancrable d'une app (implémenté par ex. par
/// `PhotoDockTab`). Exige la sérialisation : les layouts persistent
/// en JSON via les préférences.
pub trait DockTab: Clone + PartialEq + Serialize + DeserializeOwned {
    /// Document porté par ce panneau, s'il s'agit d'un canevas.
    fn canvas_id(&self) -> Option<Uuid>;
    /// Construit le canevas du document `id`.
    fn canvas(id: Uuid) -> Self;
}

/// Parts de largeur : outils ~1/30 (valeurs photo d'origine, simple
/// défaut réutilisable).
pub const SHARE_TOOLS: f32 = 1.0;
/// Parts de largeur : canevas ~22/30.
pub const SHARE_CANVAS: f32 = 22.0;
/// Parts de largeur : colonne latérale ~7/30.
pub const SHARE_SIDE: f32 = 7.0;
/// Parts de hauteur : haut de la colonne latérale (40 %).
pub const SHARE_INSPECTOR: f32 = 2.0;
/// Parts de hauteur : bas de la colonne latérale (60 %).
pub const SHARE_LAYERS: f32 = 3.0;

/// Layout par défaut : outils à gauche, canevas en onglets au
/// centre, deux panneaux empilés à droite. `canvases` ne porte que
/// des canevas (vérifié en debug).
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

/// Assigne les parts d'un conteneur linéaire (ignore silencieusement
/// un conteneur manquant : arbre restauré d'une ancienne version).
pub fn set_linear_shares<T>(tiles: &mut Tiles<T>, container: TileId, shares: &[(TileId, f32)]) {
    if let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tiles.get_mut(container) {
        for (child, share) in shares {
            linear.shares.set_share(*child, *share);
        }
    }
}

/// Vrai si `tab` existe quelque part dans l'arbre.
pub fn has_tab<T: DockTab>(tree: &Tree<T>, tab: T) -> bool {
    tree.tiles.find_pane(&tab).is_some()
}

/// Trouve le `TileId` du panneau `tab`, s'il existe.
fn find_tile<T: DockTab>(tree: &Tree<T>, tab: T) -> Option<TileId> {
    tree.tiles.find_pane(&tab)
}

/// Réaffiche `tab` s'il a été masqué (idempotent), et l'active dans
/// son onglet. Les panneaux masqués gardent leur place grâce aux
/// tuiles invisibles d'`egui_tiles`.
pub fn ensure_tab<T: DockTab>(tree: &mut Tree<T>, tab: T) {
    if let Some(tile) = find_tile(tree, tab) {
        tree.set_visible(tile, true);
        activate_tile(tree, tile);
    }
}

/// Active `tile` dans son conteneur d'onglets, s'il y en a un.
fn activate_tile<T: DockTab>(tree: &mut Tree<T>, tile: TileId) {
    if let Some(parent) = tree.tiles.parent_of(tile)
        && let Some(egui_tiles::Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(parent)
    {
        tabs.set_active(tile);
    }
}

/// Ajoute l'onglet canevas d'un document : dans le conteneur des
/// autres canevas si possible (onglets côte à côte, nouveau actif),
/// sinon dans le premier conteneur d'onglets trouvé.
pub fn push_canvas_tab<T: DockTab>(tree: &mut Tree<T>, tab: T) {
    // Feuille des canevas existants d'abord…
    let mut target = tree
        .tiles
        .iter()
        .filter_map(|(id, tile)| match tile {
            egui_tiles::Tile::Pane(pane) if pane.canvas_id().is_some() => tree.tiles.parent_of(*id),
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
pub fn remove_canvas_tab<T: DockTab>(tree: &mut Tree<T>, id: Uuid) {
    if let Some(tile) = find_tile(tree, T::canvas(id)) {
        tree.remove_recursively(tile);
    }
}

/// Réconcilie un layout restauré avec les documents réellement
/// ouverts : les ids changent à chaque lancement, donc les canevas
/// persistés sont orphelins — on retire ceux sans document et on
/// ajoute ceux manquants (outils et panneaux latéraux gardent leurs
/// places et visibilités persistées).
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

    /// Panneau de test : rail, canevas, deux panneaux latéraux.
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
