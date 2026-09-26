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

//! Tests du Scene Graph : création, hiérarchie, transforms monde,
//! identifiants et révisions.

use super::*;
use crate::{Mask, NodeKind, Revision, SceneNodeId, ShapeKind, TextData};

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

fn offset_scene() -> (Scene, SceneNodeId, SceneNodeId) {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let child = scene
        .create_child(root, NodeKind::Image)
        .expect("parent exists");
    (scene, root, child)
}

#[test]
fn creation_noeud_racine_valeurs_par_defaut() {
    let mut scene = Scene::new();
    assert!(scene.is_empty());
    assert_eq!(scene.tree_revision(), Revision::NONE);

    let id = scene.create_node(NodeKind::Group);
    assert_eq!(scene.len(), 1);
    assert!(scene.contains(id));
    assert_eq!(scene.roots(), &[id]);

    let node = scene.find(id).expect("just created");
    assert_eq!(node.kind, NodeKind::Group);
    assert_eq!(node.parent, None);
    assert!(node.children.is_empty());
    assert_eq!(node.local, Transform2D::default());
    assert!(node.visible);
    assert!(approx(node.opacity, 100.0));
    assert!(node.masks.is_empty());
    assert!(node.metadata.is_empty());
    assert!(scene.tree_revision().is_newer_than(Revision::NONE));
}

#[test]
fn ids_uniques_stables_et_ordonnes() {
    let mut scene = Scene::new();
    let a = scene.create_node(NodeKind::Group);
    let b = scene.create_node(NodeKind::Image);
    assert_ne!(a, b);
    assert!(a != SceneNodeId::nil());

    // Round-trip via l'Uuid sous-jacent (persistance, ponts inter-moteurs).
    let rebuilt = SceneNodeId::from_uuid(a.as_uuid());
    assert_eq!(rebuilt, a);
    assert_eq!(rebuilt.as_uuid(), a.as_uuid());

    // Utilisables comme clés (Hash + Ord).
    let mut map = std::collections::HashMap::new();
    map.insert(a, "a");
    map.insert(b, "b");
    assert_eq!(map.len(), 2);
    let mut sorted = [b, a];
    sorted.sort();
    assert_eq!(sorted, [a.min(b), a.max(b)]);
}

#[test]
fn hierarchie_parent_enfant_profondeur_taille() {
    let (scene, root, child) = offset_scene();

    assert_eq!(scene.parent_of(child), Some(root));
    assert_eq!(scene.parent_of(root), None);
    assert_eq!(scene.children_of(root), Some(&[child][..]));
    assert_eq!(scene.children_of(child), Some(&[][..]));
    assert_eq!(scene.depth(root), Some(0));
    assert_eq!(scene.depth(child), Some(1));
    assert_eq!(scene.subtree_len(root), Some(2));
    assert_eq!(scene.subtree_len(child), Some(1));
    assert!(scene.is_ancestor_of(root, child));
    assert!(!scene.is_ancestor_of(child, root));
}

#[test]
fn creation_enfant_parent_inconnu_rejetee() {
    let mut scene = Scene::new();
    assert_eq!(
        scene.create_child(SceneNodeId::nil(), NodeKind::Group),
        None
    );
    assert!(scene.is_empty());
}

#[test]
fn rattachement_deplace_le_sous_arbre() {
    let mut scene = Scene::new();
    let a = scene.create_node(NodeKind::Group);
    let b = scene.create_node(NodeKind::Group);
    let leaf = scene
        .create_child(a, NodeKind::Image)
        .expect("parent exists");

    let rev = scene.attach(b, leaf).expect("move ok");
    assert_eq!(scene.parent_of(leaf), Some(b));
    assert_eq!(scene.children_of(a), Some(&[][..]));
    assert_eq!(scene.children_of(b), Some(&[leaf][..]));
    assert_eq!(scene.node_revision(leaf), Some(rev));
    // La nouvelle branche est estampillée avec la révision du déplacement…
    assert_eq!(scene.node_revision(b), Some(rev));
    // …et l'ancienne branche (sous-arbre amputé) avance à son tour.
    let rev_old = scene.node_revision(a).expect("exists");
    assert!(rev_old.is_newer_than(rev));
    assert_eq!(scene.tree_revision(), rev_old);
}

#[test]
fn rattachement_rejete_cycle_soi_meme_et_inconnus() {
    let (mut scene, root, child) = offset_scene();

    // Soi-même.
    assert_eq!(scene.attach(root, root), None);
    // Cycle : le parent sous son propre descendant.
    assert_eq!(scene.attach(child, root), None);
    // Inconnus.
    assert_eq!(scene.attach(SceneNodeId::nil(), child), None);
    assert_eq!(scene.attach(root, SceneNodeId::nil()), None);

    // Structure intacte après les rejets.
    assert_eq!(scene.parent_of(child), Some(root));
    assert_eq!(scene.children_of(root), Some(&[child][..]));
}

#[test]
fn rattachement_sans_effet_ne_bumpe_pas() {
    let (mut scene, root, child) = offset_scene();
    let before_node = scene.node_revision(child).expect("exists");
    let before_tree = scene.tree_revision();
    let rev = scene.attach(root, child).expect("already last");
    assert_eq!(rev, before_node);
    assert_eq!(scene.tree_revision(), before_tree);
}

#[test]
fn rattachement_meme_parent_deplace_en_fin() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let a = scene.create_child(root, NodeKind::Image).expect("ok");
    let b = scene.create_child(root, NodeKind::Image).expect("ok");
    assert_eq!(scene.children_of(root), Some(&[a, b][..]));

    // Même parent, autre position : déplacé en fin (réordre).
    let rev = scene.attach(root, a).expect("move ok");
    assert_eq!(scene.children_of(root), Some(&[b, a][..]));
    assert_eq!(scene.node_revision(a), Some(rev));
    // Déjà dernier : sans effet.
    assert_eq!(scene.attach(root, a).expect("ok"), rev);
}

#[test]
fn detachement_redevient_racine_sous_arbre_conserve() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let mid = scene
        .create_child(root, NodeKind::Group)
        .expect("parent exists");
    let leaf = scene
        .create_child(mid, NodeKind::Text(TextData::new("t", 12.0)))
        .expect("ok");

    scene.detach(mid).expect("detach ok");
    assert_eq!(scene.parent_of(mid), None);
    assert!(scene.roots().contains(&mid));
    // Sous-arbre intact sous le détaché.
    assert_eq!(scene.parent_of(leaf), Some(mid));
    assert_eq!(scene.depth(leaf), Some(1));
    // L'ancien parent n'a plus d'enfant.
    assert_eq!(scene.children_of(root), Some(&[][..]));

    // Détacher une racine ou un inconnu : sans effet.
    assert_eq!(scene.detach(root), None);
    assert_eq!(scene.detach(SceneNodeId::nil()), None);
}

#[test]
fn suppression_sous_arbre_recursive() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let mid = scene
        .create_child(root, NodeKind::Group)
        .expect("parent exists");
    let leaf = scene.create_child(mid, NodeKind::Image).expect("ok");
    let other = scene.create_child(root, NodeKind::Video).expect("ok");

    assert!(scene.remove_subtree(mid));
    assert!(!scene.contains(mid));
    assert!(!scene.contains(leaf));
    assert!(scene.contains(root));
    assert!(scene.contains(other));
    assert_eq!(scene.children_of(root), Some(&[other][..]));
    assert_eq!(scene.len(), 2);

    // Inconnu : faux, sans effet.
    assert!(!scene.remove_subtree(SceneNodeId::nil()));
}

#[test]
fn suppression_racine_vide_les_racines() {
    let (mut scene, root, child) = offset_scene();
    assert!(scene.remove_subtree(root));
    assert!(scene.is_empty());
    assert!(scene.roots().is_empty());
    assert!(!scene.contains(child));
}

#[test]
fn modification_transform_alloue_une_revision() {
    let (mut scene, _root, child) = offset_scene();
    let before = scene.node_revision(child).expect("exists");

    let t = Transform2D {
        offset_x: 10.0,
        ..Transform2D::default()
    };
    let rev = scene.set_local_transform(child, t).expect("exists");
    assert!(rev.is_newer_than(before));
    assert_eq!(scene.local_transform(child), Some(t));

    // Même valeur : révision inchangée, pas de bump global.
    let again = scene.set_local_transform(child, t).expect("exists");
    assert_eq!(again, rev);
    assert_eq!(scene.tree_revision(), rev);

    // Inconnu.
    assert_eq!(scene.set_local_transform(SceneNodeId::nil(), t), None);
}

#[test]
fn world_transform_translations_cumulees() {
    let (mut scene, root, child) = offset_scene();

    let tr = Transform2D {
        offset_x: 100.0,
        offset_y: 50.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(root, tr).expect("ok");

    let tc = Transform2D {
        offset_x: 10.0,
        offset_y: -5.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(child, tc).expect("ok");

    let world = scene.world_affine(child).expect("exists");
    let (x, y) = world.apply(0.0, 0.0);
    assert!(approx(x, 110.0), "{x}");
    assert!(approx(y, 45.0), "{y}");
}

#[test]
fn world_transform_echelle_parent_dilate_enfant() {
    let (mut scene, root, child) = offset_scene();

    let tr = Transform2D {
        scale_x: 2.0,
        scale_y: 3.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(root, tr).expect("ok");

    let tc = Transform2D {
        offset_x: 5.0,
        offset_y: 7.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(child, tc).expect("ok");

    let (x, y) = scene.world_point(child, 0.0, 0.0).expect("exists");
    assert!(approx(x, 10.0), "{x}");
    assert!(approx(y, 21.0), "{y}");
    // L'échelle traverse aussi les points locaux.
    let (x, y) = scene.world_point(child, 1.0, 1.0).expect("exists");
    assert!(approx(x, 12.0), "{x}");
    assert!(approx(y, 24.0), "{y}");
}

#[test]
fn world_transform_rotation_parent_90_degres() {
    let (mut scene, root, child) = offset_scene();

    let tr = Transform2D {
        rotation_deg: 90.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(root, tr).expect("ok");

    let tc = Transform2D {
        offset_x: 10.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(child, tc).expect("ok");

    // (10, 0) tourné à 90° (sens horaire, convention du moteur) → (0, 10).
    let (x, y) = scene.world_point(child, 0.0, 0.0).expect("exists");
    assert!(approx(x, 0.0), "{x}");
    assert!(approx(y, 10.0), "{y}");
}

#[test]
fn world_transform_groupes_imbriques_trois_niveaux() {
    let mut scene = Scene::new();
    let a = scene.create_node(NodeKind::Group);
    let b = scene.create_child(a, NodeKind::Group).expect("ok");
    let c = scene.create_child(b, NodeKind::Layout).expect("ok");

    let ta = Transform2D {
        offset_x: 1.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(a, ta).expect("ok");
    let tb = Transform2D {
        offset_x: 10.0,
        scale_x: 2.0,
        scale_y: 2.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(b, tb).expect("ok");
    let tc = Transform2D {
        offset_x: 5.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(c, tc).expect("ok");

    // Monde de c : 1 + (10 + 2·5) = 21 sur X.
    let (x, y) = scene.world_point(c, 0.0, 0.0).expect("exists");
    assert!(approx(x, 21.0), "{x}");
    assert!(approx(y, 0.0), "{y}");
    assert_eq!(scene.depth(c), Some(2));
    assert_eq!(scene.subtree_len(a), Some(3));
}

#[test]
fn world_transform_racine_egale_locale() {
    let (mut scene, root, _child) = offset_scene();
    let t = Transform2D {
        offset_x: 3.0,
        rotation_deg: 180.0,
        ..Transform2D::default()
    };
    scene.set_local_transform(root, t).expect("ok");

    let world = scene.world_affine(root).expect("exists");
    assert_eq!(world, Affine2::from_transform(&t));
    assert_eq!(scene.world_affine(SceneNodeId::nil()), None);
}

#[test]
fn visibilite_monde_propagee_par_et() {
    let (mut scene, root, child) = offset_scene();
    assert_eq!(scene.world_visible(child), Some(true));

    scene.set_visible(root, false).expect("ok");
    assert_eq!(scene.is_visible(root), Some(false));
    assert_eq!(scene.is_visible(child), Some(true));
    assert_eq!(scene.world_visible(child), Some(false));
    assert_eq!(scene.world_visible(root), Some(false));

    // Réécrire la même valeur ne bumpe pas.
    let rev = scene.node_revision(root).expect("exists");
    assert_eq!(scene.set_visible(root, false).expect("ok"), rev);
}

#[test]
fn opacite_monde_produit_et_bornee() {
    let (mut scene, root, child) = offset_scene();
    scene.set_opacity(root, 50.0).expect("ok");
    scene.set_opacity(child, 50.0).expect("ok");
    assert_eq!(scene.world_opacity(child), Some(25.0));

    // Bornage à l'écriture.
    scene.set_opacity(child, 150.0).expect("ok");
    assert_eq!(scene.opacity(child), Some(100.0));
    scene.set_opacity(child, -20.0).expect("ok");
    assert_eq!(scene.opacity(child), Some(0.0));
    assert_eq!(scene.world_opacity(child), Some(0.0));

    // Inconnu.
    assert_eq!(scene.set_opacity(SceneNodeId::nil(), 10.0), None);
}

#[test]
fn revisions_bumpent_ancetres_pas_les_freres() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let left = scene.create_child(root, NodeKind::Image).expect("ok");
    let right = scene.create_child(root, NodeKind::Image).expect("ok");
    let rev_right_before = scene.node_revision(right).expect("exists");
    let rev_root_before = scene.node_revision(root).expect("exists");

    scene.set_opacity(left, 10.0).expect("ok");
    let rev_left = scene.node_revision(left).expect("exists");
    let rev_root = scene.node_revision(root).expect("exists");

    // Le touché et ses ancêtres avancent ensemble…
    assert!(rev_left.is_newer_than(rev_root_before));
    assert_eq!(rev_root, rev_left);
    // …mais pas le frère.
    assert_eq!(scene.node_revision(right), Some(rev_right_before));
    assert_eq!(scene.tree_revision(), rev_left);
}

#[test]
fn masques_ajout_remplacement_retrait() {
    let (mut scene, _root, child) = offset_scene();
    let target = SceneNodeId::new();
    let mask = Mask::new(target);

    let rev = scene.add_mask(child, mask).expect("owner exists");
    assert_eq!(scene.masks_of(child), Some(&[mask][..]));
    assert_eq!(scene.node_revision(child), Some(rev));

    // Même masque : pas de bump.
    assert_eq!(scene.add_mask(child, mask).expect("ok"), rev);

    // Remplacement (inversion) : bump.
    let mut inverted = mask;
    inverted.inverted = true;
    let rev2 = scene.add_mask(child, inverted).expect("ok");
    assert!(rev2.is_newer_than(rev));
    assert_eq!(scene.masks_of(child), Some(&[inverted][..]));

    // Retrait : bump ; retrait absent : sans bump.
    let rev3 = scene.remove_mask(child, target).expect("ok");
    assert!(rev3.is_newer_than(rev2));
    assert_eq!(scene.masks_of(child), Some(&[][..]));
    assert_eq!(scene.remove_mask(child, target).expect("ok"), rev3);

    // Propriétaire inconnu.
    assert_eq!(scene.add_mask(SceneNodeId::nil(), mask), None);
    assert_eq!(scene.remove_mask(SceneNodeId::nil(), target), None);
}

#[test]
fn metadonnees_pose_lecture_retrait() {
    let (mut scene, _root, child) = offset_scene();
    let rev = scene.set_meta(child, "role", "hero").expect("exists");
    assert_eq!(scene.meta(child, "role"), Some("hero"));
    assert_eq!(scene.meta(child, "absent"), None);

    // Paire identique : pas de bump.
    assert_eq!(scene.set_meta(child, "role", "hero").expect("ok"), rev);

    let rev2 = scene.remove_meta(child, "role").expect("ok");
    assert!(rev2.is_newer_than(rev));
    assert_eq!(scene.meta(child, "role"), None);
    // Clé absente : sans bump.
    assert_eq!(scene.remove_meta(child, "role").expect("ok"), rev2);

    assert_eq!(scene.set_meta(SceneNodeId::nil(), "k", "v"), None);
}

#[test]
fn toutes_les_natures_sont_representables() {
    let mut scene = Scene::new();
    let kinds = [
        NodeKind::Group,
        NodeKind::Image,
        NodeKind::Shape(ShapeKind::Rectangle),
        NodeKind::Shape(ShapeKind::Ellipse),
        NodeKind::Shape(ShapeKind::Path),
        NodeKind::Text(TextData::new("Bonjour", 24.0)),
        NodeKind::Video,
        NodeKind::Layout,
    ];
    let ids: Vec<_> = kinds.into_iter().map(|k| scene.create_node(k)).collect();
    assert_eq!(scene.len(), 8);
    assert!(matches!(
        scene.kind_of(ids[2]),
        Some(NodeKind::Shape(ShapeKind::Rectangle))
    ));
    if let Some(NodeKind::Text(data)) = scene.kind_of(ids[5]) {
        assert_eq!(data.text, "Bonjour");
        assert!(approx(data.font_size, 24.0));
    } else {
        panic!("text expected");
    }
    // TextData borne la taille.
    assert!(approx(TextData::new("x", -5.0).font_size, 0.0));
}

#[test]
fn serialisation_json_aller_retour() {
    let mut scene = Scene::new();
    let root = scene.create_node(NodeKind::Group);
    let child = scene
        .create_child(root, NodeKind::Text(TextData::new("titre", 16.0)))
        .expect("ok");
    scene.set_meta(child, "role", "hero").expect("ok");
    scene
        .add_mask(child, Mask::new(SceneNodeId::new()))
        .expect("ok");

    let json = serde_json::to_string(&scene).expect("serializable");
    let restored: Scene = serde_json::from_str(&json).expect("deserializable");
    assert_eq!(restored.len(), scene.len());
    assert_eq!(restored.roots(), scene.roots());
    assert_eq!(restored.find(child), scene.find(child));
    assert_eq!(restored.tree_revision(), scene.tree_revision());
}
