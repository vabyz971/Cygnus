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

//! Render Graph tests: derivation, invalidation, partiality, execution.

use super::*;
use crate::{EffectKey, NullBackend, RenderOp, RenderPort};
use scene::{Mask, NodeKind, SceneNodeId};

fn overlay_vide() -> EffectOverlay {
    EffectOverlay::new()
}

/// Scene: root group with two sibling images.
fn scene_groupe_deux_images() -> (Scene, SceneNodeId, SceneNodeId, SceneNodeId) {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let a = scene
        .create_child(root, NodeKind::Image)
        .expect("parent exists");
    let b = scene
        .create_child(root, NodeKind::Image)
        .expect("parent exists");
    (scene, root, a, b)
}

fn ops_of(graph: &RenderGraph, scene: SceneNodeId) -> Vec<RenderOp> {
    graph
        .derived_of(scene)
        .unwrap_or(&[])
        .iter()
        .map(|rid| graph.find(*rid).expect("mapped").op.clone())
        .collect()
}

#[test]
fn construction_chaine_complete_et_puits() {
    let (scene, root, a, b) = scene_groupe_deux_images();
    let graph = RenderGraph::build(&scene, &overlay_vide());

    // Leaf: Source → Transform → Blend (neither effect nor mask here).
    assert_eq!(
        ops_of(&graph, a),
        vec![RenderOp::Source, RenderOp::Transform, RenderOp::Blend]
    );
    // Group: a Blend folding its children.
    assert_eq!(ops_of(&graph, root), vec![RenderOp::Blend]);
    // Single sink fed by the root group's top.
    let output = graph.output();
    assert_eq!(graph.find(output).expect("output").op, RenderOp::Output);
    let fg: Vec<_> = graph
        .inputs_of(output)
        .unwrap_or(&[])
        .iter()
        .filter(|e| e.port == RenderPort::Foreground)
        .map(|e| e.from)
        .collect();
    assert_eq!(fg, vec![graph.top_of(root).expect("group top")]);

    // Execution order: each input precedes its reader.
    let plan = graph.plan();
    assert_eq!(plan.len(), graph.node_count());
    let pos = |id| plan.order.iter().position(|&n| n == id).expect("planned");
    for id in &plan.order {
        for input in graph.inputs_of(*id).unwrap_or(&[]) {
            assert!(pos(input.from) < pos(*id), "topo violé");
        }
    }
    // Background (a) precedes foreground (b): paint order.
    assert!(pos(graph.top_of(a).expect("top")) < pos(graph.top_of(b).expect("top")));
}

#[test]
fn overlay_effets_insere_dans_l_ordre() {
    let (scene, _root, a, _b) = scene_groupe_deux_images();
    let mut overlay = overlay_vide();
    overlay.insert(
        a,
        vec![
            EffectKey::new("brightness_contrast"),
            EffectKey::new("blur"),
        ],
    );
    let graph = RenderGraph::build(&scene, &overlay);
    assert_eq!(
        ops_of(&graph, a),
        vec![
            RenderOp::Source,
            RenderOp::Transform,
            RenderOp::Effect(EffectKey::new("brightness_contrast")),
            RenderOp::Effect(EffectKey::new("blur")),
            RenderOp::Blend,
        ]
    );
}

#[test]
fn masque_semantique_derive_noeud_mask() {
    let (mut scene, _root, a, b) = scene_groupe_deux_images();
    scene
        .add_mask(a, Mask::new(SceneNodeId::new()))
        .expect("owner exists");
    let graph = RenderGraph::build(&scene, &overlay_vide());
    assert!(ops_of(&graph, a).contains(&RenderOp::Mask));
    assert!(!ops_of(&graph, b).contains(&RenderOp::Mask));
}

#[test]
fn modification_scene_salit_sans_reconstruire() {
    let (mut scene, _root, a, _b) = scene_groupe_deux_images();
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let before_tree = graph.tree_revision();
    let before_ids: Vec<_> = graph.plan().order;

    scene.set_opacity(a, 50.0).expect("exists");
    let report = graph.sync(&scene, &overlay_vide());
    assert!(report.dirtied > 0);
    assert_eq!(report.rebuilt_scopes, 0);
    assert_eq!(report.appended, 0);
    assert_eq!(report.removed, 0);
    // Identities preserved, revisions advanced, dirty propagated.
    assert_eq!(graph.plan().order, before_ids);
    assert!(graph.tree_revision().is_newer_than(before_tree));
    for rid in graph.derived_of(a).unwrap_or(&[]) {
        assert!(graph.is_dirty(*rid).expect("known"), "dirty attendu");
    }
    // Idempotent: second sync clean.
    assert!(graph.sync(&scene, &overlay_vide()).is_clean());
}

#[test]
fn frere_epargne_par_la_salissure() {
    let (mut scene, _root, a, b) = scene_groupe_deux_images();
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let b_revs: Vec<_> = graph
        .derived_of(b)
        .unwrap_or(&[])
        .iter()
        .map(|rid| graph.node_revision(*rid).expect("known"))
        .collect();

    scene.set_opacity(a, 10.0).expect("exists");
    let report = graph.sync(&scene, &overlay_vide());
    assert!(report.dirtied > 0);
    // Sibling's content (Source, Transform): revisions and cleanliness intact…
    let after: Vec<_> = graph
        .derived_of(b)
        .unwrap_or(&[])
        .iter()
        .map(|rid| graph.node_revision(*rid).expect("known"))
        .collect();
    assert_eq!(after[..2], b_revs[..2]);
    for rid in graph.derived_of(b).unwrap_or(&[])[..2].iter() {
        assert!(!graph.is_dirty(*rid).expect("known"));
    }
    // …but its Blend follows the new background: legitimate dirty.
    let blend_b = graph.top_of(b).expect("top");
    assert!(graph.is_dirty(blend_b).expect("known"));
    assert_ne!(after[2], b_revs[2]);
}

#[test]
fn execution_backend_nul_suit_le_plan() {
    let (scene, _, _, _) = scene_groupe_deux_images();
    let graph = RenderGraph::build(&scene, &overlay_vide());
    let plan = graph.plan();
    let mut backend = NullBackend::new();
    let report = graph.execute(&mut backend, &plan).expect("null ok");
    assert_eq!(report.executed, graph.node_count());
    assert_eq!(backend.executed(), graph.node_count());
    assert_eq!(backend.order(), plan.order.as_slice());
}

#[test]
fn plan_dirty_sautes_les_propres() {
    let (mut scene, _root, a, _b) = scene_groupe_deux_images();
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    assert!(graph.plan_dirty().is_empty());

    scene.set_visible(a, false).expect("exists");
    let report = graph.sync(&scene, &overlay_vide());
    assert!(report.dirtied > 0);
    let dirty = graph.plan_dirty();
    assert!(!dirty.is_empty());
    assert!(dirty.len() < graph.node_count());
    // Each scheduled node is dirty; partial execution possible.
    for id in &dirty.order {
        assert!(graph.is_dirty(*id).expect("known"));
    }
    let mut backend = NullBackend::new();
    let report = graph.execute(&mut backend, &dirty).expect("null ok");
    assert_eq!(report.executed, dirty.len());
}

#[test]
fn ajout_feuille_preserve_l_existant() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let a = scene.create_child(root, NodeKind::Image).expect("ok");
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let old_top_a = graph.top_of(a).expect("top");
    let old_rev_a = graph.node_revision(old_top_a).expect("known");

    let b = scene.create_child(root, NodeKind::Image).expect("ok");
    let report = graph.sync(&scene, &overlay_vide());
    assert_eq!(report.appended, 1);
    assert_eq!(report.rebuilt_scopes, 0);
    // The existing: same ids, same revisions, clean.
    assert_eq!(graph.top_of(a), Some(old_top_a));
    assert_eq!(graph.node_revision(old_top_a), Some(old_rev_a));
    assert!(!graph.is_dirty(old_top_a).expect("known"));
    // The group's Blend follows the new last top, chained onto the old one.
    let new_top_b = graph.top_of(b).expect("top");
    let group_top = graph.top_of(root).expect("group top");
    let fg = graph
        .inputs_of(group_top)
        .unwrap_or(&[])
        .iter()
        .find(|e| e.port == RenderPort::Foreground)
        .map(|e| e.from);
    assert_eq!(fg, Some(new_top_b));
    let bg = graph
        .inputs_of(new_top_b)
        .unwrap_or(&[])
        .iter()
        .find(|e| e.port == RenderPort::Background)
        .map(|e| e.from);
    assert_eq!(bg, Some(old_top_a));
}

#[test]
fn suppression_feuille_recable_sans_reconstruire() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let a = scene.create_child(root, NodeKind::Image).expect("ok");
    let b = scene.create_child(root, NodeKind::Image).expect("ok");
    let c = scene.create_child(root, NodeKind::Image).expect("ok");
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let top_a = graph.top_of(a).expect("top");
    let top_b = graph.top_of(b).expect("top");
    let top_c = graph.top_of(c).expect("top");

    assert!(scene.remove_subtree(b));
    let report = graph.sync(&scene, &overlay_vide());
    assert!(report.removed > 0);
    assert_eq!(report.rebuilt_scopes, 0);
    // Survivors keep their identities…
    assert_eq!(graph.top_of(a), Some(top_a));
    assert_eq!(graph.top_of(c), Some(top_c));
    assert!(graph.find(top_b).is_none());
    // …and C's background now points to A.
    let bg = graph
        .inputs_of(top_c)
        .unwrap_or(&[])
        .iter()
        .find(|e| e.port == RenderPort::Background)
        .map(|e| e.from);
    assert_eq!(bg, Some(top_a));
}

#[test]
fn reordre_par_recablage_identites_preservees() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let a = scene.create_child(root, NodeKind::Image).expect("ok");
    let b = scene.create_child(root, NodeKind::Image).expect("ok");
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let top_a = graph.top_of(a).expect("top");
    let top_b = graph.top_of(b).expect("top");

    // Moves A to the end of the sibling group: [A, B] → [B, A].
    scene.attach(root, a).expect("move ok");
    let report = graph.sync(&scene, &overlay_vide());
    assert_eq!(report.reordered, 1);
    assert_eq!(report.rebuilt_scopes, 0);
    assert_eq!(graph.top_of(a), Some(top_a));
    assert_eq!(graph.top_of(b), Some(top_b));
    // New paint order: B under A, group Blend over A.
    let bg_a = graph
        .inputs_of(top_a)
        .unwrap_or(&[])
        .iter()
        .find(|e| e.port == RenderPort::Background)
        .map(|e| e.from);
    assert_eq!(bg_a, Some(top_b));
    let group_top = graph.top_of(root).expect("group top");
    let group_fg = graph
        .inputs_of(group_top)
        .unwrap_or(&[])
        .iter()
        .find(|e| e.port == RenderPort::Foreground)
        .map(|e| e.from);
    assert_eq!(group_fg, Some(top_a));
}

#[test]
fn reconstruction_partielle_epargne_les_groupes_intacts() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let ga = scene.create_child(root, NodeKind::Group).expect("ok");
    let gb = scene.create_child(root, NodeKind::Group).expect("ok");
    let _la = scene.create_child(ga, NodeKind::Image).expect("ok");
    let lb = scene.create_child(gb, NodeKind::Image).expect("ok");
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let top_gb = graph.top_of(gb).expect("top");
    let rev_gb = graph.node_revision(top_gb).expect("known");
    let lb_nodes: Vec<_> = graph.derived_of(lb).unwrap_or(&[]).to_vec();

    // Addition into A: B is not rebuilt (identities preserved)…
    scene.create_child(ga, NodeKind::Video).expect("ok");
    let report = graph.sync(&scene, &overlay_vide());
    assert_eq!(report.rebuilt_scopes, 0);
    assert_eq!(graph.top_of(gb), Some(top_gb));
    assert_eq!(graph.derived_of(lb).unwrap_or(&[]), lb_nodes.as_slice());
    // B's inner chain (upstream of the group Blend) stays clean.
    for rid in graph.derived_of(lb).unwrap_or(&[]) {
        assert!(!graph.is_dirty(*rid).expect("known"));
    }
    // …but its Blend follows A's new background: legitimate dirty.
    assert!(
        graph
            .node_revision(top_gb)
            .expect("known")
            .is_newer_than(rev_gb)
    );
    assert!(graph.is_dirty(top_gb).expect("known"));
}

#[test]
fn changement_overlay_reconstruit_la_portee() {
    let (scene, _root, a, _b) = scene_groupe_deux_images();
    let mut graph = RenderGraph::build(&scene, &overlay_vide());
    let old_nodes: Vec<_> = graph.derived_of(a).unwrap_or(&[]).to_vec();

    let mut overlay = overlay_vide();
    overlay.insert(a, vec![EffectKey::new("blur")]);
    let report = graph.sync(&scene, &overlay);
    assert_eq!(report.rebuilt_scopes, 1);
    // The chain now holds the effect; the old one is purged.
    assert!(ops_of(&graph, a).contains(&RenderOp::Effect(EffectKey::new("blur"))));
    for rid in &old_nodes {
        assert!(!graph.contains(*rid), "ancien nœud purgé");
    }
    assert!(graph.sync(&scene, &overlay).is_clean());
}

#[test]
fn melange_retrait_ajout_reconstruit_la_portee() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let a = scene.create_child(root, NodeKind::Image).expect("ok");
    let _b = scene.create_child(root, NodeKind::Image).expect("ok");
    let mut graph = RenderGraph::build(&scene, &overlay_vide());

    assert!(scene.remove_subtree(a));
    scene.create_child(root, NodeKind::Video).expect("ok");
    let report = graph.sync(&scene, &overlay_vide());
    assert_eq!(report.rebuilt_scopes, 1);
    // Graph coherent and executable again.
    let plan = graph.plan();
    let mut backend = NullBackend::new();
    let result = graph.execute(&mut backend, &plan).expect("null ok");
    assert_eq!(result.executed, graph.node_count());
}
