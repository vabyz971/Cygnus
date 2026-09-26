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

//! Tests du graphe de dépendances : création, propagation, cycles, ordre.

use super::*;
use crate::{Edge, GraphError, Node, NodeId};

fn chaine_abc() -> (Graph, NodeId, NodeId, NodeId) {
    let mut graph = Graph::new();
    let a = graph.add_node();
    let b = graph.add_node();
    let c = graph.add_node();
    graph.add_edge(a, b).expect("acyclic");
    graph.add_edge(b, c).expect("acyclic");
    (graph, a, b, c)
}

#[test]
fn creation_noeuds_identifiants_stables() {
    let mut graph = Graph::new();
    assert!(graph.is_empty());
    assert_eq!(graph.tree_revision(), Revision::NONE);

    let a = graph.add_node();
    let b = graph.add_node();
    assert_ne!(a, b);
    assert_ne!(a, NodeId::nil());
    assert_eq!(graph.len(), 2);
    assert!(graph.contains(a));
    assert!(!graph.contains(NodeId::nil()));

    let node = graph.find(a).expect("just created");
    assert_eq!(node.id, a);
    assert!(!node.is_dirty());
    assert!(!graph.is_dirty(a).expect("exists"));

    // Insertion d'identifiant stable (restauration) + doublon rejeté.
    let stable = NodeId::new();
    assert!(graph.insert(stable));
    assert!(!graph.insert(stable));
    assert_eq!(graph.len(), 3);

    // Les nœuds sont Copy : instantanés sans clones.
    fn assert_copy<T: Copy>() {}
    assert_copy::<Node>();
    assert_copy::<Edge>();
    assert_copy::<NodeId>();
}

#[test]
fn creation_aretes_et_adjacences() {
    let (graph, a, b, c) = chaine_abc();

    assert_eq!(graph.dependencies(b), Some(&[a][..]));
    assert_eq!(graph.dependents(b), Some(&[c][..]));
    assert_eq!(graph.dependencies(a), Some(&[][..]));
    assert_eq!(graph.dependents(c), Some(&[][..]));
    assert_eq!(graph.edge_count(), 2);
    assert_eq!(graph.all_edges().len(), 2);
    assert!(graph.all_edges().contains(&Edge::new(a, b)));

    // Inconnus : None, pas de panique.
    assert_eq!(graph.dependencies(NodeId::nil()), None);
    assert_eq!(graph.is_dirty(NodeId::nil()), None);
    assert_eq!(graph.node_revision(NodeId::nil()), None);
}

#[test]
fn arete_dupliquee_no_op_idempotent() {
    let (mut graph, a, b, _) = chaine_abc();
    let before = graph.tree_revision();
    let rev = graph.add_edge(a, b).expect("duplicate tolerated");
    assert_eq!(rev, before);
    assert_eq!(graph.tree_revision(), before);
    assert_eq!(graph.edge_count(), 2);
}

#[test]
fn aretes_rejetees_laissent_le_graphe_intact() {
    let (mut graph, a, b, _) = chaine_abc();
    let before = graph.tree_revision();

    assert_eq!(graph.add_edge(a, a), Err(GraphError::SelfLoop(a)));
    assert_eq!(
        graph.add_edge(a, NodeId::nil()),
        Err(GraphError::UnknownNode(NodeId::nil()))
    );
    assert_eq!(
        graph.add_edge(NodeId::nil(), b),
        Err(GraphError::UnknownNode(NodeId::nil()))
    );
    assert_eq!(graph.tree_revision(), before);
    assert_eq!(graph.edge_count(), 2);
}

#[test]
fn propagation_salete_aval_un_seul_timbre() {
    let (mut graph, a, b, c) = chaine_abc();
    let independant = graph.add_node();

    let rev = graph.mark_dirty(a).expect("exists");
    assert!(graph.is_dirty(a).expect("exists"));
    assert!(graph.is_dirty(b).expect("exists"));
    assert!(graph.is_dirty(c).expect("exists"));
    assert!(!graph.is_dirty(independant).expect("exists"));

    // Un changement logique = un timbre partagé sur toute la fermeture.
    assert_eq!(graph.node_revision(a), Some(rev));
    assert_eq!(graph.node_revision(b), Some(rev));
    assert_eq!(graph.node_revision(c), Some(rev));
    assert_ne!(graph.node_revision(independant), Some(rev));

    // Salissure ciblée en milieu de chaîne : l'amont est épargné.
    graph.clear_dirty(a);
    graph.clear_dirty(b);
    graph.clear_dirty(c);
    let rev2 = graph.mark_dirty(b).expect("exists");
    assert!(rev2.is_newer_than(rev));
    assert!(!graph.is_dirty(a).expect("exists"));
    assert!(graph.is_dirty(b).expect("exists"));
    assert!(graph.is_dirty(c).expect("exists"));

    // Inconnu.
    assert_eq!(graph.mark_dirty(NodeId::nil()), None);
}

#[test]
fn nettoyage_salete_sans_bump_de_revision() {
    let (mut graph, a, _, _) = chaine_abc();
    let rev = graph.mark_dirty(a).expect("exists");

    assert_eq!(graph.clear_dirty(a), Some(true));
    assert_eq!(graph.clear_dirty(a), Some(false));
    // La révision identifie l'état calculé : inchangée après nettoyage.
    assert_eq!(graph.node_revision(a), Some(rev));
    assert_eq!(graph.tree_revision(), rev);
    assert_eq!(graph.clear_dirty(NodeId::nil()), None);
}

#[test]
fn detection_cycle_direct_et_indirect() {
    let (mut graph, a, b, c) = chaine_abc();

    // C → A refermerait A → B → C → A.
    assert_eq!(
        graph.add_edge(c, a),
        Err(GraphError::Cycle { from: c, to: a })
    );
    // B → A refermerait A → B → A.
    assert_eq!(
        graph.add_edge(b, a),
        Err(GraphError::Cycle { from: b, to: a })
    );
    // Le graphe est intact après les rejets.
    assert_eq!(graph.edge_count(), 2);
    assert_eq!(graph.topo_order().len(), 3);

    // Pré-validation sans mutation.
    assert!(graph.would_create_cycle(c, a));
    assert!(graph.would_create_cycle(a, a));
    assert!(!graph.would_create_cycle(a, c));
    assert!(!graph.would_create_cycle(a, NodeId::nil()));
}

#[test]
fn diamant_sans_cycle_propagation_complete() {
    let mut graph = Graph::new();
    let a = graph.add_node();
    let b = graph.add_node();
    let c = graph.add_node();
    let d = graph.add_node();
    graph.add_edge(a, b).expect("ok");
    graph.add_edge(a, c).expect("ok");
    graph.add_edge(b, d).expect("ok");
    graph.add_edge(c, d).expect("ok");

    graph.mark_dirty(a).expect("exists");
    for id in [a, b, c, d] {
        assert!(graph.is_dirty(id).expect("exists"), "{id}");
    }
    // Chaque nœud visité une fois malgré les deux chemins : ordre valide.
    let order = graph.topo_order();
    assert_eq!(order.len(), 4);
    let pos = |id| order.iter().position(|&n| n == id).expect("present");
    assert!(pos(a) < pos(b) && pos(a) < pos(c));
    assert!(pos(b) < pos(d) && pos(c) < pos(d));
}

#[test]
fn ordre_topologique_amont_avant_aval() {
    let (graph, a, b, c) = chaine_abc();
    assert_eq!(graph.topo_order(), vec![a, b, c]);
}

#[test]
fn ordre_topologique_deterministe_sans_aretes() {
    let mut graph = Graph::new();
    let ids: Vec<_> = (0..5).map(|_| graph.add_node()).collect();
    let mut expected = ids.clone();
    expected.sort();
    // Sans arêtes : ordre total déterministe par identifiant.
    assert_eq!(graph.topo_order(), expected);
    assert_eq!(graph.topo_order(), graph.topo_order());
}

#[test]
fn mutation_suppression_arete_coupe_la_propagation() {
    let (mut graph, a, b, c) = chaine_abc();

    assert!(graph.remove_edge(b, c));
    assert!(!graph.remove_edge(b, c));
    assert!(!graph.remove_edge(a, NodeId::nil()));
    assert_eq!(graph.dependencies(c), Some(&[][..]));

    graph.mark_dirty(a).expect("exists");
    assert!(graph.is_dirty(b).expect("exists"));
    assert!(!graph.is_dirty(c).expect("exists"));
    assert_eq!(graph.topo_order().len(), 3);
}

#[test]
fn mutation_suppression_noeud_purge_les_aretes() {
    let (mut graph, a, b, c) = chaine_abc();

    assert!(graph.remove_node(b));
    assert!(!graph.remove_node(b));
    assert!(!graph.remove_node(NodeId::nil()));
    assert_eq!(graph.len(), 2);
    assert_eq!(graph.edge_count(), 0);
    assert_eq!(graph.dependents(a), Some(&[][..]));
    assert_eq!(graph.dependencies(c), Some(&[][..]));

    // Le reste du graphe reste exploitable.
    graph.mark_dirty(a).expect("exists");
    assert!(!graph.is_dirty(c).expect("exists"));
    assert_eq!(graph.topo_order().len(), 2);
}

#[test]
fn revisions_avancent_aux_mutations_structurelles() {
    let mut graph = Graph::new();
    let r0 = graph.tree_revision();
    let a = graph.add_node();
    assert!(graph.tree_revision().is_newer_than(r0));

    let b = graph.add_node();
    let r1 = graph.tree_revision();
    let r2 = graph.add_edge(a, b).expect("ok");
    // L'insertion d'arête alloue : le compteur avance.
    assert!(r2.is_newer_than(r1));
    assert_eq!(graph.tree_revision(), r2);
    assert_eq!(graph.dependencies(b), Some(&[a][..]));
}

#[test]
fn serialisation_json_aller_retour() {
    let (mut graph, a, b, c) = chaine_abc();
    graph.mark_dirty(a).expect("exists");
    assert!(graph.is_dirty(b).expect("exists"));

    let json = serde_json::to_string(&graph).expect("serializable");
    let restored: Graph = serde_json::from_str(&json).expect("deserializable");
    assert_eq!(restored.len(), graph.len());
    assert_eq!(restored.edge_count(), graph.edge_count());
    assert_eq!(restored.find(a), graph.find(a));
    assert_eq!(restored.topo_order(), graph.topo_order());
    assert_eq!(restored.tree_revision(), graph.tree_revision());
    let _ = c;
}
