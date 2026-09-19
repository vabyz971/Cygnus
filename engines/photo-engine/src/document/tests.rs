//! Tests « golden » du compositing CPU : images synthétiques minuscules
//! dont la sortie est vérifiée pixel par pixel (tolérance ±1 pour les
//! arrondis f32→u8). Toute régression de blend/transform est visible ici.
//! Portés tels quels sur le modèle LayerTree, plus couverture arbre.

use super::compositing::{DrawItem, needs_fallback_in, prepare_top};
use super::*;
use crate::history::Snapshot;
use datatypes::ParamValue;
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use std::sync::Arc;
use uuid::Uuid;

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> DynamicImage {
    DynamicImage::ImageRgba8(ImageBuffer::from_pixel(w, h, Rgba(rgba)))
}

fn arc(img: &DynamicImage) -> Arc<DynamicImage> {
    Arc::new(img.clone())
}

fn pixel_node(img: &DynamicImage, opacity: f32, mode: BlendMode, ox: f32, oy: f32) -> LayerNode {
    let mut l = PixelLayer::new("test", arc(img));
    l.opacity = opacity;
    l.blend_mode = mode;
    l.transform.offset_x = ox;
    l.transform.offset_y = oy;
    LayerNode::Pixel(l)
}

fn doc_of(nodes: Vec<LayerNode>, w: u32, h: u32) -> Document {
    let mut doc = Document::new(w, h);
    doc.root = nodes;
    doc
}

fn px(img: &DynamicImage, x: u32, y: u32) -> [u8; 4] {
    let rgba = img.to_rgba8();
    let p = rgba.get_pixel(x, y);
    [p[0], p[1], p[2], p[3]]
}

#[test]
fn blank_document_starts_with_sized_transparent_layer() {
    let doc = Document::with_blank_layer(800, 600);
    assert_eq!((doc.width, doc.height), (800, 600));
    assert_eq!(doc.pixel_count(), 1);
    let layer = &doc.iter_pixels()[0];
    assert_eq!(layer.name, "Calque 1");
    assert_eq!(layer.dimensions(), (800, 600));
    assert_eq!(px(&layer.source_image, 0, 0)[3], 0);
    assert_eq!(px(&layer.source_image, 799, 599)[3], 0);
}

#[test]
fn blank_document_clamps_to_1x1() {
    let doc = Document::with_blank_layer(0, 0);
    assert_eq!((doc.width, doc.height), (1, 1));
    assert_eq!(doc.iter_pixels()[0].dimensions(), (1, 1));
}

fn assert_close(got: [u8; 4], exp: [u8; 4]) {
    for c in 0..4 {
        assert!(
            (got[c] as i16 - exp[c] as i16).abs() <= 1,
            "canal {c} : {got:?} ≠ {exp:?}"
        );
    }
}

#[test]
fn normal_opaque_recouvre_et_deborde() {
    let base = solid(4, 4, [255, 0, 0, 255]);
    let top = solid(2, 2, [0, 255, 0, 255]);
    // Pile : rouge en bas, vert au-dessus décalé en (2,2)
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, BlendMode::Normal, 2.0, 2.0),
        ],
        4,
        4,
    );
    let out = doc.composite().expect("composite non vide");
    assert_close(px(&out, 3, 3), [0, 255, 0, 255]); // zone recouverte
    assert_close(px(&out, 0, 0), [255, 0, 0, 255]); // zone de base
}

#[test]
fn calque_hors_document_n_influence_pas_le_crop() {
    let base = solid(4, 4, [10, 20, 30, 255]);
    let top = solid(2, 2, [255, 255, 255, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, BlendMode::Normal, -10.0, -10.0),
        ],
        4,
        4,
    );
    let out = doc.composite().expect("composite non vide");
    assert_close(px(&out, 1, 1), [10, 20, 30, 255]);
}

#[test]
fn modes_de_fusion_valeurs_connues() {
    // Base grise 50 % + top gris clair : valeurs canoniques des modes
    let base = solid(1, 1, [128, 128, 128, 255]);
    let top = solid(1, 1, [192, 192, 192, 255]);
    let stack_with = |mode: BlendMode| {
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, mode, 0.0, 0.0),
        ]
    };

    let cases = [
        (BlendMode::Multiply, (128 * 192) / 255), // ≈ 96
        (BlendMode::Screen, 255 - ((255 - 128) * (255 - 192)) / 255), // ≈ 224
        (BlendMode::Darken, 128),
        (BlendMode::Lighten, 192),
    ];
    for (mode, expected) in cases {
        let doc = doc_of(stack_with(mode), 1, 1);
        let out = doc.composite().expect("composite");
        let got = px(&out, 0, 0);
        assert_close(got, [expected as u8, expected as u8, expected as u8, 255]);
    }

    // Overlay sur base < 0.5 : 2·b·t
    let dark = solid(1, 1, [64, 64, 64, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&dark, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, BlendMode::Overlay, 0.0, 0.0),
        ],
        1,
        1,
    );
    let out = doc.composite().expect("composite");
    let exp = (2 * 64 * 192 / 255) as u8;
    assert_close(px(&out, 0, 0), [exp, exp, exp, 255]);
}

#[test]
fn opacite_50_normal_sur_blanc() {
    let base = solid(2, 2, [255, 255, 255, 255]);
    let top = solid(2, 2, [0, 0, 0, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 50.0, BlendMode::Normal, 0.0, 0.0),
        ],
        2,
        2,
    );
    let out = doc.composite().expect("composite");
    assert_close(px(&out, 0, 0), [127, 127, 127, 255]);
}

#[test]
fn calque_seul_translucide_sur_transparent() {
    // Un seul calque 50 % au-dessus du vide : l'alpha de sortie est
    // semi-transparent (plan de travail infini) — pas de fond magique.
    let top = solid(2, 2, [0, 0, 0, 255]);
    let doc = doc_of(
        vec![pixel_node(&top, 50.0, BlendMode::Normal, 0.0, 0.0)],
        2,
        2,
    );
    let out = doc.composite_preview().expect("composite");
    assert_close(px(&out, 0, 0), [0, 0, 0, 127]);
}

#[test]
fn opacite_nulle_ou_cache_ignores() {
    let base = solid(2, 2, [9, 9, 9, 255]);
    let top = solid(2, 2, [250, 250, 250, 255]);
    let mut hidden = pixel_node(&top, 100.0, BlendMode::Normal, 0.0, 0.0);
    hidden.set_visible(false);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            hidden,
        ],
        2,
        2,
    );
    let out = doc.composite().expect("composite");
    assert_close(px(&out, 0, 0), [9, 9, 9, 255]);

    let transparent = doc_of(
        vec![pixel_node(&top, 0.0, BlendMode::Normal, 0.0, 0.0)],
        2,
        2,
    );
    assert!(transparent.composite_preview().is_none(), "rien de visible");
}

#[test]
fn groupe_opacite_s_applique_aux_enfants_composes() {
    let rouge = solid(2, 2, [200, 0, 0, 255]);
    let bleu = solid(2, 2, [0, 0, 200, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(pixel_node(&rouge, 100.0, BlendMode::Normal, 0.0, 0.0));
    let mut group = GroupLayer::new(
        "g",
        vec![pixel_node(&bleu, 100.0, BlendMode::Normal, 0.0, 0.0)],
    );
    group.opacity = 50.0;
    doc.push_layer(LayerNode::Group(group));

    let out = doc.composite().expect("composite");
    // Groupe Normal 50 % sur rouge : mix (200,0,0)/(0,0,200) → (100,0,100)
    assert_close(px(&out, 0, 0), [100, 0, 100, 255]);
}

#[test]
fn composite_sans_sous_arbre_reste_en_cache_chaud() {
    // Deux calques filtrés : le premier warm-up paie les MISS, ensuite
    // une composite EXCLUANT un sous-arbre ne doit générer AUCUN nouveau
    // miss — c'est ce qui rend le fond de drag instantané.
    use datatypes::ParamValue;
    let img = solid(2, 2, [120, 120, 120, 255]);
    let mut doc = Document::new(2, 2);
    for name in ["a", "b"] {
        let mut l = PixelLayer::new(name, arc(&img));
        let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
        f.params
            .insert("brightness".into(), ParamValue::Float(10.0));
        l.filter_layers.push(f);
        doc.push_layer(LayerNode::Pixel(l));
    }
    let id_b = doc.root[1].id();

    let _ = doc.composite_preview(); // warm-up : remplit le cache
    let (hits0, misses0) = doc.renderer_stats();
    assert_eq!(misses0, 2, "warm-up = un miss par calque filtré");

    let bg = doc
        .composite_preview_without(id_b)
        .expect("composite d'exclusion");
    let p = px(&bg, 0, 0);
    // Le calque b est masqué : seul a (+ son filtre) reste → 120+25 = 145
    assert_close(p, [145, 145, 145, 255]);

    // ZÉRO nouveau miss : tout est servi depuis le cache chaud
    let (_, misses1) = doc.renderer_stats();
    assert_eq!(misses1, misses0, "composite d'exclusion sans recalcul");
    assert!(
        doc.renderer_stats().0 > hits0,
        "les résolutions sont des hits"
    );
}

#[test]
fn groupe_multiply_fond_la_composite_des_enfants() {
    // Enfant blanc seul dans un groupe Multiply → blanc × base = base
    let base = solid(2, 2, [128, 128, 128, 255]);
    let blanc = solid(2, 2, [255, 255, 255, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0));
    let mut group = GroupLayer::new(
        "g",
        vec![pixel_node(&blanc, 100.0, BlendMode::Normal, 0.0, 0.0)],
    );
    group.blend_mode = BlendMode::Multiply;
    doc.push_layer(LayerNode::Group(group));

    let out = doc.composite().expect("composite");
    assert_close(px(&out, 0, 0), [128, 128, 128, 255]);
}

#[test]
fn ajustement_applique_son_effet_a_la_pile_dessous() {
    let gris = solid(1, 1, [100, 100, 100, 255]);
    let mut doc = Document::new(1, 1);
    doc.push_layer(pixel_node(&gris, 100.0, BlendMode::Normal, 0.0, 0.0));
    let mut f = FilterNode::new("brightness_contrast");
    f.params
        .insert("brightness".into(), ParamValue::Float(40.0));
    doc.push_layer(LayerNode::Adjustment(AdjustmentLayer::new(
        "ajust",
        vec![f],
    )));
    let out = doc.composite().expect("composite");
    // 100 + 40*2.55 = 202
    assert_close(px(&out, 0, 0), [202, 202, 202, 255]);
}

#[test]
fn ajustement_opacite_mixe_lineairement() {
    let gris = solid(1, 1, [100, 100, 100, 255]);
    let mut doc = Document::new(1, 1);
    doc.push_layer(pixel_node(&gris, 100.0, BlendMode::Normal, 0.0, 0.0));
    let mut f = FilterNode::new("brightness_contrast");
    f.params
        .insert("brightness".into(), ParamValue::Float(40.0));
    let mut adj = AdjustmentLayer::new("ajust", vec![f]);
    adj.opacity = 50.0;
    doc.push_layer(LayerNode::Adjustment(adj));
    let out = doc.composite().expect("composite");
    // mix 100 ↔ 202 à 50 % ≈ 151
    assert_close(px(&out, 0, 0), [151, 151, 151, 255]);
}

#[test]
fn needs_fallback_detecte_groupes_et_ajustements() {
    let img = solid(1, 1, [1, 1, 1, 255]);

    // Pile plate Normal : rendu rapide possible
    let mut doc = Document::new(1, 1);
    doc.push_layer(pixel_node(&img, 100.0, BlendMode::Normal, 0.0, 0.0));
    assert!(!doc.needs_fallback());

    // Mode non-Normal sur un calque
    doc.root[0].set_blend_mode(BlendMode::Screen);
    assert!(doc.needs_fallback());

    // Groupe en mode non-Normal (même vide d'enfants)
    doc.root[0].set_blend_mode(BlendMode::Normal);
    let mut group = GroupLayer::new("g", vec![]);
    group.blend_mode = BlendMode::Overlay;
    doc.push_layer(LayerNode::Group(group));
    assert!(doc.needs_fallback());

    // Ajustement actif au-dessus d'une pile Normal
    let mut doc2 = Document::new(1, 1);
    doc2.push_layer(pixel_node(&img, 100.0, BlendMode::Normal, 0.0, 0.0));
    doc2.push_layer(LayerNode::Adjustment(AdjustmentLayer::new(
        "a",
        vec![FilterNode::new("blur")],
    )));
    assert!(doc2.needs_fallback());
}

#[test]
fn arbre_operations_structurelles() {
    let a = solid(1, 1, [1, 0, 0, 255]);
    let b = solid(1, 1, [2, 0, 0, 255]);
    let c = solid(1, 1, [3, 0, 0, 255]);
    let mut doc = Document::new(4, 4);
    let na = pixel_node(&a, 100.0, BlendMode::Normal, 0.0, 0.0);
    let nb = pixel_node(&b, 100.0, BlendMode::Normal, 0.0, 0.0);
    let nc = pixel_node(&c, 100.0, BlendMode::Normal, 0.0, 0.0);
    let (ida, idb, idc) = (na.id(), nb.id(), nc.id());
    doc.push_layer(na);
    doc.push_layer(nb);
    doc.push_layer(nc);

    // move_up : b passe au-dessus de c
    assert!(doc.move_up(idb));
    assert_eq!(doc.root[2].id(), idb);
    // move_down deux fois : b revient en bas
    assert!(doc.move_down(idb));
    assert!(doc.move_down(idb));
    assert_eq!(doc.root[0].id(), idb);
    assert!(!doc.move_down(idb), "déjà en bas");

    // group(c, b) donné en désordre → ordre pile préservé [b, c]? Non :
    // b est en bas (index 0), c au-dessus (index 2 après moves ? vérifions)
    // État courant : [b, a, c] → grouper b et c donne groupe [b, c]
    let gid = doc.group(&[idc, idb]).expect("group");
    let LayerNode::Group(g) = &doc.root[0] else {
        panic!("groupe attendu en bas");
    };
    assert_eq!(g.children.len(), 2);
    assert_eq!(g.children[0].id(), idb, "ordre relatif préservé");
    assert_eq!(g.children[1].id(), idc);
    assert_eq!(doc.root[1].id(), ida);

    // duplicate du groupe : nouveaux ids partout
    let first_child_id = match &doc.root[0] {
        LayerNode::Group(g) => g.children[0].id(),
        _ => panic!("groupe attendu"),
    };
    let dup = doc.duplicate(gid).expect("duplicate");
    assert_ne!(dup, gid);
    let LayerNode::Group(g2) = doc.find(dup).expect("copie") else {
        panic!("groupe dupliqué attendu");
    };
    assert_ne!(g2.children[0].id(), first_child_id);

    // ungroup → enfants remontés, plus de groupe
    let freed = doc.ungroup(gid).expect("ungroup");
    assert_eq!(freed.len(), 2);
    assert!(doc.find(gid).is_none());
    // État : [b, c, copie_du_groupe(2 px), a]
    assert_eq!(doc.pixel_count(), 5);
    assert_eq!(doc.root.len(), 4);

    // remove d'une feuille racine
    let removed = doc.remove(ida).expect("remove");
    assert_eq!(removed.id(), ida);
    assert!(doc.find(ida).is_none());
    assert_eq!(doc.pixel_count(), 4);
}

fn ids(doc: &Document) -> Vec<Uuid> {
    doc.root.iter().map(LayerNode::id).collect()
}

fn groupe_imbrique() -> (Document, Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
    let fond = solid(1, 1, [1, 1, 1, 255]);
    let mut doc = Document::new(4, 4);
    let image1 = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let image2 = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let image3 = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let image4 = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let (id1, id2, id3, id4) = (image1.id(), image2.id(), image3.id(), image4.id());
    let groupe_b = GroupLayer::new("B", vec![image3, image4]);
    let gid_b = groupe_b.id;
    let groupe_a = GroupLayer::new("A", vec![image1, image2, LayerNode::Group(groupe_b)]);
    let gid_a = groupe_a.id;
    doc.push_layer(LayerNode::Group(groupe_a));
    (doc, id1, id2, id3, id4, gid_a, gid_b)
}

#[test]
fn reorder_before_apres_cibles_unitaires() {
    let fond = solid(1, 1, [1, 1, 1, 255]);
    let a = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let b = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let c = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let (ida, idb, idc) = (a.id(), b.id(), c.id());
    let mut doc = doc_of(vec![a, b, c], 4, 4);

    assert!(doc.can_reorder_before(idc, ida));
    assert!(doc.reorder_before(idc, ida, true));
    assert_eq!(ids(&doc), vec![idc, ida, idb]);

    assert!(doc.can_reorder_before(ida, idb));
    assert!(doc.reorder_before(ida, idb, false));
    assert_eq!(ids(&doc), vec![idc, idb, ida]);
}

#[test]
fn reorder_adjacent_sans_effet_est_noop() {
    let fond = solid(1, 1, [1, 1, 1, 255]);
    let a = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let b = pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0);
    let (ida, idb) = (a.id(), b.id());
    let mut doc = doc_of(vec![a, b], 4, 4);

    assert!(!doc.reorder_before(idb, ida, false));
    assert_eq!(ids(&doc), vec![ida, idb]);
    assert!(!doc.reorder_before(ida, idb, true));
    assert_eq!(ids(&doc), vec![ida, idb]);
}

#[test]
fn move_into_groupe_et_imbrication() {
    let (mut doc, id1, id2, id3, id4, gid_a, gid_b) = groupe_imbrique();

    assert!(doc.can_move_into(id1, gid_b));
    assert!(doc.move_into(id1, gid_b));
    let groupe_a = match doc.find(gid_a).expect("groupe A") {
        LayerNode::Group(groupe) => groupe,
        _ => unreachable!("groupe A"),
    };
    assert_eq!(groupe_a.children.len(), 2);
    let groupe_b = match &groupe_a.children[1] {
        LayerNode::Group(groupe) => groupe,
        _ => unreachable!("groupe B"),
    };
    assert_eq!(
        groupe_b
            .children
            .iter()
            .map(LayerNode::id)
            .collect::<Vec<_>>(),
        vec![id1, id3, id4]
    );
    assert_eq!(doc.find(id2).map(LayerNode::id), Some(id2));
}

#[test]
fn move_into_premier_enfant_est_noop() {
    let (mut doc, id1, _, _, _, _, gid_b) = groupe_imbrique();
    assert!(doc.move_into(id1, gid_b));
    assert!(!doc.move_into(id1, gid_b));
}

#[test]
fn drops_illegaux_refuses() {
    let (mut doc, _, _, id3, _, gid_a, gid_b) = groupe_imbrique();

    assert!(!doc.can_reorder_before(gid_a, gid_a));
    assert!(!doc.reorder_before(gid_a, gid_a, true));
    assert!(!doc.can_reorder_before(gid_a, id3));
    assert!(!doc.reorder_before(gid_a, id3, false));
    assert!(!doc.can_move_into(gid_a, gid_a));
    assert!(!doc.move_into(gid_a, gid_a));
    assert!(!doc.can_move_into(gid_a, gid_b));
    assert!(!doc.move_into(gid_a, gid_b));
    assert!(!doc.can_move_into(id3, id3));
    assert!(!doc.move_into(id3, id3));
}

#[test]
fn snapshot_aller_retour_conserve_l_arbre() {
    let img = solid(2, 2, [7, 7, 7, 255]);
    let mut doc = Document::new(2, 2);
    let mut l = PixelLayer::new("fond", arc(&img));
    l.opacity = 80.0;
    l.blend_mode = BlendMode::Multiply;
    l.transform.offset_x = 1.0;
    doc.push_layer(LayerNode::Pixel(l));
    let id = doc.root[0].id();

    let snap: Snapshot = doc.snapshot();
    let mut restored = Document::new(0, 0);
    restored.restore_snapshot(snap);

    assert_eq!((restored.width, restored.height), (2, 2));
    assert_eq!(restored.pixel_count(), 1);
    let l = restored.pixel_layer(id).expect("calque restauré");
    assert_eq!(l.opacity, 80.0);
    assert_eq!(l.blend_mode, BlendMode::Multiply);
    assert_eq!(l.transform.offset_x, 1.0);
    assert_eq!(*l.source_image, *arc(&img));
}

#[test]
fn live_filter_modifie_l_apparence_pas_la_source() {
    let img = solid(2, 2, [100, 100, 100, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(LayerNode::Pixel(PixelLayer::new("filtre", arc(&img))));
    let id = doc.root[0].id();

    let fid = doc
        .add_filter(
            id,
            FilterLayer::neutral("brightness_contrast", Default::default()),
        )
        .expect("add_filter");
    assert!(
        doc.set_filter_param(id, fid, "brightness", ParamValue::Float(50.0)),
        "set_filter_param"
    );

    let appearance = doc.appearance(id).expect("apparence");
    assert_close(px(&appearance.image, 0, 0), [227, 227, 227, 255]); // 100 + 50*2.55
    // La source reste intacte (non destructif)
    assert_eq!(*doc.pixel_layer(id).unwrap().source_image, *arc(&img));

    // Désactivation → retour à la source
    assert!(doc.set_filter_enabled(id, fid, false));
    let off = doc.appearance(id).expect("apparence off");
    assert_close(px(&off.image, 0, 0), [100, 100, 100, 255]);

    // Suppression du filtre
    assert!(doc.remove_filter(id, fid).is_some());
    assert!(doc.pixel_layer(id).unwrap().filter_layers.is_empty());
}

#[test]
fn sous_calque_opacite_mixe_l_apparence() {
    // Façon Affinity : opacité 50 % sur le sous-calque = mix source↔filtré.
    let img = solid(2, 2, [100, 100, 100, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(LayerNode::Pixel(PixelLayer::new("filtre", arc(&img))));
    let id = doc.root[0].id();

    let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
    f.params
        .insert("brightness".into(), ParamValue::Float(40.0)); // plein = 202
    f.opacity = 50.0;
    doc.add_filter(id, f).expect("add_filter");

    let appearance = doc.appearance(id).expect("apparence");
    assert_close(px(&appearance.image, 0, 0), [151, 151, 151, 255]);
}

#[test]
fn peinture_masque_sous_calque_rafraichit_l_apparence() {
    // Non-régression : vider le masque au pinceau doit changer l'apparence
    // SANS toggle du calque (le masque est baké dans l'apparence).
    let img = solid(2, 2, [100, 100, 100, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(LayerNode::Pixel(PixelLayer::new("filtre", arc(&img))));
    let id = doc.root[0].id();

    let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
    f.params
        .insert("brightness".into(), ParamValue::Float(40.0)); // plein = 202
    f.masks.push(LayerMask::full(2, 2));
    let fid = doc.add_filter(id, f).expect("add_filter");

    let full = doc.appearance(id).expect("apparence");
    assert_close(px(&full.image, 0, 0), [202, 202, 202, 255]);

    // Coup de pinceau noir : couverture → 0 sur tout le masque.
    {
        let fl = doc.find_filter_layer_mut(fid).expect("sous-calque");
        let mut buf = ImageBuffer::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        for px in buf.pixels_mut() {
            *px = Rgba([0, 0, 0, 255]);
        }
        fl.masks[0].image = Arc::new(buf);
        fl.masks[0].touch();
    }
    let wiped = doc.appearance(id).expect("apparence après peinture");
    assert_close(px(&wiped.image, 0, 0), [100, 100, 100, 255]);
}

#[test]
fn sous_calque_reordonnable_et_duplicable() {
    let img = solid(2, 2, [100, 100, 100, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(LayerNode::Pixel(PixelLayer::new("filtre", arc(&img))));
    let id = doc.root[0].id();

    let mut a = FilterLayer::neutral("brightness_contrast", Default::default());
    a.params
        .insert("brightness".into(), ParamValue::Float(40.0));
    let mut b = FilterLayer::neutral("color_correct", Default::default());
    b.params.insert("saturation".into(), ParamValue::Float(2.0));
    let ida = doc.add_filter(id, a).expect("add a");
    let idb = doc.add_filter(id, b).expect("add b");
    fn order(doc: &Document, id: Uuid) -> Vec<Uuid> {
        doc.pixel_layer(id)
            .unwrap()
            .filter_layers
            .iter()
            .map(|f| f.id)
            .collect()
    }
    assert_eq!(order(&doc, id), vec![ida, idb]);

    // Monter le premier = l'appliquer en dernier
    assert!(doc.move_filter(id, ida, true));
    assert_eq!(order(&doc, id), vec![idb, ida]);

    // Dupliquer = clone avec nouvel id juste au-dessus
    let idc = doc.duplicate_filter(id, idb).expect("duplicate");
    assert_ne!(idc, idb);
    assert_eq!(order(&doc, id), vec![idb, idc, ida]);
}

#[test]
fn filtre_inconnu_est_transparent() {
    let img = solid(2, 2, [42, 42, 42, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(LayerNode::Pixel(PixelLayer::new("x", arc(&img))));
    let id = doc.root[0].id();
    doc.add_filter(
        id,
        FilterLayer::neutral("effet_qui_n_existe_pas", Default::default()),
    );
    let appearance = doc.appearance(id).expect("apparence");
    assert_close(px(&appearance.image, 0, 0), [42, 42, 42, 255]);
}

#[test]
fn crop_compense_le_transform_monde() {
    let mut doc = Document::new(4, 4);
    let mut b = ImageBuffer::from_pixel(4, 2, Rgba([0, 0, 0, 255]));
    b.put_pixel(3, 0, Rgba([200, 10, 20, 255]));
    let l = PixelLayer::new("crop", Arc::new(DynamicImage::ImageRgba8(b)));
    let id = l.id;
    doc.push_layer(LayerNode::Pixel(l));

    doc.crop(id, 2, 0, 2, 2).expect("crop valide");
    let l = doc.pixel_layer(id).expect("calque");
    assert_eq!((l.transform.offset_x, l.transform.offset_y), (2.0, 0.0));
    let img = l.source_image.to_rgba8();
    assert_eq!((img.width(), img.height()), (2, 2));
    // Le pixel rouge d'origine (3,0) devient (1,0) dans le calque rogné
    let p = img.get_pixel(1, 0);
    assert_eq!([p[0], p[1], p[2]], [200, 10, 20]);

    // Crop hors bornes → erreur propre
    assert!(doc.crop(id, -1, 0, 1, 1).is_err());
    assert!(doc.crop(id, 0, 0, 0, 5).is_err());
}

#[test]
fn plan_infini_agrandit_autour_du_document() {
    let doc_img = solid(4, 4, [0, 0, 0, 255]);
    let big = solid(8, 8, [255, 255, 255, 255]);
    // Calque dépassant à gauche/haut : le composite preview ne doit pas rogner
    let doc = doc_of(
        vec![
            pixel_node(&doc_img, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&big, 100.0, BlendMode::Normal, -6.0, -6.0),
        ],
        4,
        4,
    );
    let out = doc.composite_preview().expect("preview non vide");
    let rgba = out.to_rgba8();
    assert!(
        rgba.width() >= 8 && rgba.height() >= 8,
        "plan infini trop petit"
    );
    // Coin haut-gauche du grand calque visible hors document :
    // le centre du buffer correspond au centre document → pixel blanc à (0,0)
    assert_eq!(rgba.get_pixel(0, 0)[0], 255);
}

#[test]
fn flip_est_destructif_et_symetrique() {
    let mut doc = Document::new(4, 4);
    let mut b = ImageBuffer::from_pixel(2, 1, Rgba([0, 0, 0, 255]));
    b.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    let l = PixelLayer::new("flip", Arc::new(DynamicImage::ImageRgba8(b)));
    let id = l.id;
    doc.push_layer(LayerNode::Pixel(l));
    doc.flip(id, true).expect("flip");
    let l = doc.pixel_layer(id).expect("calque");
    let img = l.source_image.to_rgba8();
    let avant_gauche = [255, 0, 0];
    // Après miroir horizontal, le rouge est passé à droite
    let p0 = img.get_pixel(0, 0);
    assert_ne!([p0[0], p0[1], p0[2]], avant_gauche);
    let p1 = img.get_pixel(1, 0);
    assert_eq!([p1[0], p1[1], p1[2]], avant_gauche);
}

// --- Masques (§8) ---

fn masked_node(
    img: &DynamicImage,
    mask_color: [u8; 4],
    enabled: bool,
    inverted: bool,
) -> LayerNode {
    let mut l = PixelLayer::new("masked", arc(img));
    let (w, h) = img.dimensions();
    let mut m = LayerMask::full(w, h);
    // remplit le masque uniformément
    let buf = ImageBuffer::from_pixel(w, h, Rgba(mask_color));
    m.image = Arc::new(buf);
    m.enabled = enabled;
    m.inverted = inverted;
    l.masks.push(m);
    LayerNode::Pixel(l)
}

#[test]
fn masque_blanc_est_noop() {
    let base = solid(2, 2, [10, 20, 30, 255]);
    let top = solid(2, 2, [200, 100, 50, 255]);
    let doc_plain = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, BlendMode::Normal, 0.0, 0.0),
        ],
        2,
        2,
    );
    let doc_masked = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_node(&top, [255, 255, 255, 255], true, false),
        ],
        2,
        2,
    );
    assert_eq!(
        doc_plain.composite().unwrap().to_rgba8().as_raw(),
        doc_masked.composite().unwrap().to_rgba8().as_raw()
    );
}

#[test]
fn masque_noir_cache_le_calque() {
    let base = solid(1, 1, [10, 20, 30, 255]);
    let top = solid(1, 1, [200, 100, 50, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_node(&top, [0, 0, 0, 255], true, false),
        ],
        1,
        1,
    );
    let out = doc.composite().unwrap();
    assert_close(px(&out, 0, 0), [10, 20, 30, 255]);
}

#[test]
fn masque_gris_diminue_alpha() {
    let base = solid(1, 1, [0, 0, 0, 255]);
    let top = solid(1, 1, [255, 0, 0, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_node(&top, [128, 128, 128, 255], true, false),
        ],
        1,
        1,
    );
    let out = doc.composite().unwrap();
    // alpha 0.5 → blend 50% rouge sur noir ≈ 128
    assert_close(px(&out, 0, 0), [128, 0, 0, 255]);
}

#[test]
fn masque_inverted_inverse_couverture() {
    let base = solid(1, 1, [10, 20, 30, 255]);
    let top = solid(1, 1, [200, 100, 50, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_node(&top, [0, 0, 0, 255], true, true),
        ],
        1,
        1,
    );
    // noir inversé = blanc → no-op, top visible
    let out = doc.composite().unwrap();
    assert_close(px(&out, 0, 0), [200, 100, 50, 255]);
}

#[test]
fn masque_desactive_est_noop() {
    let base = solid(1, 1, [10, 20, 30, 255]);
    let top = solid(1, 1, [200, 100, 50, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_node(&top, [0, 0, 0, 255], false, false),
        ],
        1,
        1,
    );
    let out = doc.composite().unwrap();
    assert_close(px(&out, 0, 0), [200, 100, 50, 255]);
}

#[test]
fn un_masque_de_calque_en_normal_n_impose_plus_le_fallback() {
    // Baké dans l'apparence (source × filtres × masques) : un calque masqué
    // (Normal, pas de skew) reste sur le chemin rapide GPU.
    let img = solid(1, 1, [0, 0, 0, 255]);
    let mut doc = Document::new(1, 1);
    doc.push_layer(masked_node(&img, [255, 255, 255, 255], true, false));
    assert!(!doc.needs_fallback(), "masque baké → chemin rapide");
    // Un calque masqué en mode non-Normal reste en fallback (le blend l'exige).
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(doc.root[0].id()) {
        l.blend_mode = BlendMode::Multiply;
    }
    assert!(doc.needs_fallback());
}

#[test]
fn masques_de_calque_bakes_dans_l_apparence() {
    let img = solid(2, 2, [255, 0, 0, 255]);
    let mut doc = Document::new(2, 2);
    doc.push_layer(masked_node(&img, [128, 128, 128, 255], true, false));
    let id = doc.root[0].id();
    let ap = doc.appearance(id).expect("apparence");
    // alpha 0.5 appliqué au buffer de l'apparence (masque baké)
    assert_close(px(&ap.image, 0, 0), [255, 0, 0, 128]);

    // Désactivé → nouvelle signature → apparence non atténuée
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.masks[0].enabled = false;
    }
    let ap_off = doc.appearance(id).expect("apparence");
    assert_close(px(&ap_off.image, 0, 0), [255, 0, 0, 255]);

    // Peinture (version bump) → signature → cache invalidé à nouveau
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.masks[0].image = Arc::new(ImageBuffer::from_pixel(2, 2, Rgba([0, 0, 0, 255])));
        l.masks[0].enabled = true;
        l.masks[0].touch();
    }
    let ap_black = doc.appearance(id).expect("apparence après peinture");
    assert_close(px(&ap_black.image, 0, 0), [255, 0, 0, 0]);
}

fn masked_group(mask_color: [u8; 4], enabled: bool, inverted: bool) -> LayerNode {
    let red = solid(1, 1, [200, 0, 0, 255]);
    let blue = solid(1, 1, [0, 0, 200, 255]);
    let mut g = GroupLayer::new(
        "g",
        vec![
            pixel_node(&red, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&blue, 100.0, BlendMode::Normal, 0.0, 0.0),
        ],
    );
    let mut m = LayerMask::full(1, 1);
    m.image = Arc::new(ImageBuffer::from_pixel(1, 1, Rgba(mask_color)));
    m.enabled = enabled;
    m.inverted = inverted;
    g.masks.push(m);
    LayerNode::Group(g)
}

#[test]
fn masque_de_groupe_blanc_est_noop() {
    let base = solid(1, 1, [10, 10, 10, 255]);
    let top_plain = GroupLayer::new(
        "g",
        vec![
            pixel_node(
                &solid(1, 1, [200, 0, 0, 255]),
                100.0,
                BlendMode::Normal,
                0.0,
                0.0,
            ),
            pixel_node(
                &solid(1, 1, [0, 0, 200, 255]),
                100.0,
                BlendMode::Normal,
                0.0,
                0.0,
            ),
        ],
    );
    let doc_plain = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Group(top_plain),
        ],
        1,
        1,
    );
    let doc_masked = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_group([255, 255, 255, 255], true, false),
        ],
        1,
        1,
    );
    assert_eq!(
        doc_plain.composite().unwrap().to_rgba8().as_raw(),
        doc_masked.composite().unwrap().to_rgba8().as_raw()
    );
}

#[test]
fn masque_de_groupe_noir_cache_tout_le_sous_arbre() {
    let base = solid(1, 1, [10, 10, 10, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_group([0, 0, 0, 255], true, false),
        ],
        1,
        1,
    );
    let out = doc.composite().unwrap();
    assert_close(px(&out, 0, 0), [10, 10, 10, 255]);
}

#[test]
fn masque_de_groupe_50_attenue_globalement() {
    let base = solid(1, 1, [0, 0, 0, 255]);
    // Groupe avec 2 enfants rouge+bleu → le dernier (bleu) recouvre le rouge → bleu pur
    // Masque 50% sur le groupe → bleu à 50% sur noir → 0,0,100
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_group([128, 128, 128, 255], true, false),
        ],
        1,
        1,
    );
    let out = doc.composite().unwrap();
    assert_close(px(&out, 0, 0), [0, 0, 100, 255]);
}

#[test]
fn masque_de_groupe_inverted_et_desactive() {
    let base = solid(1, 1, [10, 10, 10, 255]);
    let doc_inv = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_group([0, 0, 0, 255], true, true),
        ],
        1,
        1,
    );
    // noir inversé = blanc → groupe visible (bleu)
    let out = doc_inv.composite().unwrap();
    assert_close(px(&out, 0, 0), [0, 0, 200, 255]);

    let doc_off = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_group([0, 0, 0, 255], false, false),
        ],
        1,
        1,
    );
    let out2 = doc_off.composite().unwrap();
    assert_close(px(&out2, 0, 0), [0, 0, 200, 255]);
}

#[test]
fn masque_de_groupe_avec_decalage_origine() {
    // Document 4x4, groupe avec enfant décalé hors document → buffer agrandi
    // Le masque doit rester aligné malgré origin_x/y non nul
    let base = solid(4, 4, [10, 10, 10, 255]);
    let red = solid(2, 2, [200, 0, 0, 255]);
    let mut g = GroupLayer::new(
        "g",
        vec![pixel_node(&red, 100.0, BlendMode::Normal, 5.0, 5.0)],
    );
    let mut m = LayerMask::full(2, 2);
    m.image = Arc::new(ImageBuffer::from_pixel(2, 2, Rgba([128, 128, 128, 255])));
    g.masks.push(m);
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Group(g),
        ],
        4,
        4,
    );
    // Le groupe décalé + masque 50% doit contribuer mais atténué, pas crash ni décalage
    let out = doc.composite_preview().expect("preview");
    assert!(out.width() >= 4 && out.height() >= 4);
}

#[test]
fn multi_masques_fusionnent_multiplicativement() {
    // Deux masques à 50 % chacun → couverture effective ~25 % : le calque
    // ressort plus transparent (plus proche du fond) qu'avec un seul masque.
    let base = solid(1, 1, [10, 20, 30, 255]);
    let top = solid(1, 1, [200, 100, 50, 255]);

    let single = masked_node(&top, [128, 128, 128, 255], true, false);
    let mut double = PixelLayer::new("double", arc(&top));
    for _ in 0..2 {
        let mut m = LayerMask::full(1, 1);
        m.image = Arc::new(ImageBuffer::from_pixel(1, 1, Rgba([128, 128, 128, 255])));
        double.masks.push(m);
    }

    let doc_single = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            single,
        ],
        1,
        1,
    );
    let doc_double = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Pixel(double),
        ],
        1,
        1,
    );

    let r_single = px(&doc_single.composite().unwrap(), 0, 0)[0];
    let r_double = px(&doc_double.composite().unwrap(), 0, 0)[0];
    assert!(r_single < 200, "mono-masque doit atténuer le calque");
    assert!(
        r_double < r_single,
        "2 masques atténuent plus qu'1 (multiplicatif)"
    );
    assert!(r_double > 10, "reste partiellement visible");
}

#[test]
fn transform_legacy_scale_uniforme_deserialise_en_deux_axes() {
    // Projet v2/v3 : champ `scale` uniforme — doit charger sur scale_x & scale_y
    let json = r#"{"offset_x":5.0,"offset_y":3.0,"rotation_deg":0.0,"scale":2.0}"#;
    let t: Transform2D = serde_json::from_str(json).expect("deserialisation legacy");
    assert_eq!(t.offset_x, 5.0);
    assert_eq!(t.offset_y, 3.0);
    assert_eq!(t.scale_x, 2.0);
    assert_eq!(t.scale_y, 2.0);
    assert!(!t.has_skew());

    // Round-trip courant : scale_x/scale_y distincts + skew préservés
    let current = Transform2D {
        scale_x: 1.5,
        scale_y: 0.8,
        skew_x: 10.0,
        offset_x: -2.0,
        ..Transform2D::default()
    };
    let round: Transform2D =
        serde_json::from_str(&serde_json::to_string(&current).unwrap()).unwrap();
    assert_eq!(round.scale_x, 1.5);
    assert_eq!(round.scale_y, 0.8);
    assert_eq!(round.skew_x, 10.0);
    assert!(round.has_skew());
}

#[test]
fn echelle_non_uniforme_agit_sur_les_axes_separement() {
    // 2×2 rouge, scale_x 2 (→w=4), scale_y 0.5 (→h=1), sans rotation : le
    // composite s'étire horizontalement, pas verticalement.
    let base = solid(4, 4, [0, 0, 255, 255]);
    let top = solid(2, 2, [255, 0, 0, 255]);
    let mut doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, BlendMode::Normal, 0.0, 0.0),
        ],
        4,
        4,
    );
    {
        let l = doc.pixel_layer_mut(doc.root[1].id()).unwrap();
        l.transform.scale_x = 2.0;
        l.transform.scale_y = 0.5;
    }
    let out = doc.composite().unwrap();
    assert_eq!((out.width(), out.height()), (4, 4));
    assert_close(px(&out, 3, 0), [255, 0, 0, 255]); // haut-droite : rouge étiré
    assert_close(px(&out, 3, 3), [0, 0, 255, 255]); // bas : bleu intact
}

#[test]
fn skew_cisaille_la_bbox_et_ne_change_pas_l_aire() {
    // 2×2 rouge, skew_x=45° (kx=1) : le carré devient un parallélogramme
    // englobé dans une bbox 4×2 aux offsets (-1, 0). L'aire couverte reste 4.
    let img = solid(2, 2, [255, 0, 0, 255]);
    let item = DrawItem::new(
        &img,
        Transform2D {
            skew_x: 45.0,
            ..Transform2D::default()
        },
    );
    let (buf, ox, oy) = prepare_top(&item);
    assert_eq!((buf.width(), buf.height()), (4, 2));
    assert!((ox - -1.0).abs() < 0.01, "offset x {}", ox);
    assert!((oy - 0.0).abs() < 0.01, "offset y {}", oy);
    let red = buf.pixels().filter(|p| p[0] == 255 && p[3] == 255).count();
    assert!(
        red >= 3,
        "parallélogramme couvert (aire conservée), red={red}"
    );
    assert!(red <= 8, "pas de débordement, red={red}");
    // Les coins de la bbox en dehors du parallélogramme restent transparents
    assert!(buf.get_pixel(3, 0)[3] == 0, "coin haut-droite");
    assert!(buf.get_pixel(0, 1)[3] == 0, "coin bas-gauche");
}

#[test]
fn skew_force_le_chemin_cpu_de_fallback() {
    let img = solid(1, 1, [1, 1, 1, 255]);
    let mut l = PixelLayer::new("incline", arc(&img));
    l.transform.skew_y = 15.0;
    assert!(
        needs_fallback_in(&[LayerNode::Pixel(l)]),
        "skew ⇒ fallback CPU"
    );
    let normal = PixelLayer::new("droit", arc(&img));
    assert!(
        !needs_fallback_in(&[LayerNode::Pixel(normal)]),
        "sans skew : chemin rapide conservé"
    );
}

#[test]
fn coins_transformes_cadrent_les_extents() {
    // Calque 2×2 tourné de 45° : la bbox des 4 coins englobe le carré pivoté.
    let img = solid(2, 2, [1, 1, 1, 255]);
    let mut l = PixelLayer::new("tourne", arc(&img));
    l.transform.rotation_deg = 45.0;
    let (tw, th) = {
        let clamped = Transform2D {
            scale_x: l.transform.scale_x.clamp(0.05, 8.0),
            scale_y: l.transform.scale_y.clamp(0.05, 8.0),
            ..l.transform
        };
        let corners = clamped.doc_corners(2.0, 2.0);
        let min_x = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
        let min_y = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min);
        let max_x = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
        let max_y = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max);
        (max_x - min_x, max_y - min_y)
    };
    // bbox d'un carré 2×2 pivoté 45° = 2√2 ≈ 2.83
    assert!((tw - 2.0_f32.sqrt() * 2.0).abs() < 0.01, "tw={tw}");
    assert!((th - 2.0_f32.sqrt() * 2.0).abs() < 0.01, "th={th}");
}

#[test]
fn sample_color_preleve_la_composite() {
    let base = solid(4, 4, [255, 0, 0, 255]);
    let top = solid(1, 1, [0, 255, 0, 255]);
    // Calque vert POINT ne couvrant que la case doc [2,3) — le plus petit
    // cas qui distingue floor() de round() dans la correspondance.
    let doc = doc_of(
        vec![
            pixel_node(&base, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&top, 100.0, BlendMode::Normal, 2.0, 2.0),
        ],
        4,
        4,
    );
    let c_rouge = doc.sample_color(1.0, 1.0).expect("point rouge");
    assert_eq!(c_rouge[0], 255);
    let c_vert = doc.sample_color(2.0, 2.0).expect("point vert");
    assert_eq!(c_vert[1], 255);
    assert!(c_vert[0] == 0, "au-dessus du rouge, vert opaque");
    // Un clic dans la case [2,3) doit rester dans le pixel vert, y compris
    // à droite de son centre (round() dériverait vers le voisin [3,4)).
    let c_frac_vert = doc.sample_color(2.9, 2.9).expect("vert à 2.9");
    assert_eq!(c_frac_vert[1], 255, "2.9 dans [2,3) → vert");
    let c_frac_rouge = doc.sample_color(3.1, 2.9).expect("rouge à droite");
    assert_eq!(c_frac_rouge[0], 255, "3.1 hors [2,3) → rouge");
    // Hors du plan composite (loin) → None
    assert!(
        doc.sample_color(1000.0, 1000.0).is_none(),
        "hors plan : None attendu"
    );
}

// --- Drag masqué d'un calque redimensionné (§8/drag) ---

/// Reproduit `drag_layer_composite_task` côté engine : le calque SEUL avec
/// son masque, AVEC son transform courant, composé via `composite_preview`
/// sur plan infini. Le contenu opache du buffer doit couvrir la TAILLE
/// REDIMENSIONNÉE du calque (scale ×2 sur 4×4 → contenu 8×8), PAS la taille
/// d'origine du masque (4×4). Sinon, le ghost de drag d'un calque redimensionné
/// « prend la taille du masque ».
#[test]
fn composite_masque_calque_redimensionne_conserve_l_echelle() {
    let img = solid(4, 4, [200, 30, 30, 255]);
    let mut node = masked_node(&img, [255, 255, 255, 255], true, false);
    if let LayerNode::Pixel(l) = &mut node {
        l.transform.scale_x = 2.0;
        l.transform.scale_y = 2.0;
        // Centré dans le doc 16×16 : 4×4 scale 2 → contenu monde [6,14].
        l.transform.offset_x = 6.0;
        l.transform.offset_y = 6.0;
    }
    let doc = doc_of(vec![node], 16, 16);
    let out = doc.composite_preview().expect("composite");
    let (w, h) = out.dimensions();
    let opaque = |x: u32, y: u32| out.get_pixel(x, y)[3] > 0;
    let min_x = (0..w).find(|x| opaque(*x, h / 2)).unwrap();
    let max_x = (0..w).rev().find(|x| opaque(*x, h / 2)).unwrap();
    let min_y = (0..h).find(|y| opaque(w / 2, *y)).unwrap();
    let max_y = (0..h).rev().find(|y| opaque(w / 2, *y)).unwrap();
    let w_content = max_x - min_x + 1;
    let h_content = max_y - min_y + 1;
    assert!(
        (8..=9).contains(&w_content),
        "contenu = size redimensionnée (scale 2 sur 4×4), obtenu {w_content}"
    );
    assert!(
        (8..=9).contains(&h_content),
        "contenu = size redimensionnée (scale 2 sur 4×4), obtenu {h_content}"
    );
    assert!(
        w_content != 4 && h_content != 4,
        "jamais la taille d'origine du masque (4×4)"
    );

    // Le masque est bien appliqué au buffer transformé : noir → rien.
    let mut node_black = masked_node(&img, [0, 0, 0, 255], true, false);
    if let LayerNode::Pixel(l) = &mut node_black {
        l.transform.scale_x = 2.0;
        l.transform.scale_y = 2.0;
        l.transform.offset_x = 6.0;
        l.transform.offset_y = 6.0;
    }
    let out_black = doc_of(vec![node_black], 16, 16)
        .composite_preview()
        .expect("composite masqué noir");
    assert!(
        out_black.pixels().all(|p| p.2[3] == 0),
        "masque noir → rien de visible"
    );
}

#[test]
fn preview_geometry_centre_le_document_quand_un_calque_sort() {
    // Document 8x8, calque opaque 8x8 décale de +10 px : le composite
    // couvre 0..18 en X (demi-extent 14 autour du centre 4).
    let img = solid(8, 8, [200, 40, 40, 255]);
    let doc = doc_of(
        vec![pixel_node(&img, 100.0, BlendMode::Normal, 10.0, 0.0)],
        8,
        8,
    );
    let (w, h, ox, oy) = doc.preview_geometry().expect("geometrie");
    assert_eq!((w, h), (28, 8));
    assert!(
        (ox - 10.0).abs() < 1e-3,
        "origine X = decalage, obtenu {ox}"
    );
    assert!(oy.abs() < 1e-3, "origine Y = 0, obtenu {oy}");
    // Le composite pleine résolution a les mêmes dimensions.
    let full = doc.composite_preview().expect("composite");
    assert_eq!(full.dimensions(), (w, h));
}

#[test]
fn preview_geometry_vide_sans_calque_visible() {
    let img = solid(8, 8, [200, 40, 40, 0]);
    let doc = doc_of(
        vec![pixel_node(&img, 0.0, BlendMode::Normal, 0.0, 0.0)],
        8,
        8,
    );
    assert!(doc.preview_geometry().is_none());
    assert!(doc.composite_preview().is_none());
}

#[test]
fn opacite_ne_touche_que_les_bornes_du_calque() {
    // Couche 4x4 bleue à (4,4) sur fond rouge 8x8 : passer l'opacité à
    // 50 % ne doit modifier que les pixels couverts par le calque.
    let fond = solid(8, 8, [200, 40, 40, 255]);
    let carre = solid(4, 4, [40, 40, 200, 255]);
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 100.0, BlendMode::Normal, 4.0, 4.0),
        ],
        8,
        8,
    );
    let before = doc.composite_preview().expect("composite");
    let id = doc.root[1].id();
    doc.find_mut(id).expect("calque").set_opacity(50.0);
    let after = doc.composite_preview().expect("composite");
    // Hors bornes (0..4, 0..4) : rouge intact.
    assert_eq!(px(&before, 0, 0), px(&after, 0, 0));
    assert_eq!(px(&after, 0, 0), [200, 40, 40, 255]);
    // Dedans (6,6) : mélange 50/50, donc modifié.
    assert_ne!(px(&before, 6, 6), px(&after, 6, 6));
}

#[test]
fn deplacement_ne_touche_que_ancien_union_nouveau() {
    // Carré 2x2 déplacé de (0,0) à (2,0) : tout pixel hors des deux
    // positions reste identique.
    let fond = solid(8, 8, [200, 40, 40, 255]);
    let carre = solid(2, 2, [40, 40, 200, 255]);
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 100.0, BlendMode::Normal, 0.0, 0.0),
        ],
        8,
        8,
    );
    let before = doc.composite_preview().expect("composite");
    let id = doc.root[1].id();
    if let LayerNode::Pixel(l) = doc.find_mut(id).expect("calque") {
        l.transform.offset_x = 2.0;
    }
    let after = doc.composite_preview().expect("composite");
    // (7,7) hors ancien ∪ nouveau : inchangé.
    assert_eq!(px(&before, 7, 7), px(&after, 7, 7));
    // (0,0) révélée (rouge), (2,0) recouverte (bleu) : changées.
    assert_eq!(px(&after, 0, 0)[0..3], [200, 40, 40]);
    assert_eq!(px(&after, 2, 0)[0..3], [40, 40, 200]);
}

#[test]
fn reordonner_ne_change_que_les_regions_reordonnees() {
    // L0 rouge plein, L1 bleu 2x2 en (0,0), L2 vert 2x2 en (6,6).
    // Échanger L0/L1 : le vert du dessus et le champ rouge lointain
    // restent stables, seul le coin (0,0) change.
    let (r, b, g) = (
        solid(8, 8, [200, 40, 40, 255]),
        solid(2, 2, [40, 40, 200, 255]),
        solid(2, 2, [40, 200, 40, 255]),
    );
    let mut doc = doc_of(
        vec![
            pixel_node(&r, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&b, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&g, 100.0, BlendMode::Normal, 6.0, 6.0),
        ],
        8,
        8,
    );
    let before = doc.composite_preview().expect("composite");
    assert_eq!(px(&before, 0, 0)[0..3], [40, 40, 200]);
    doc.root.swap(0, 1);
    let after = doc.composite_preview().expect("composite");
    assert_eq!(px(&after, 6, 6)[0..3], [40, 200, 40], "dessus intact");
    assert_eq!(px(&after, 4, 4), px(&before, 4, 4), "champ intact");
    assert_eq!(px(&after, 0, 0)[0..3], [200, 40, 40], "coin recouvert");
}

#[test]
fn blend_multiply_depend_du_dessous_meme_opaque() {
    // Gris opaque Multiply sur fond noir vs blanc : résultats différents
    // (le mode lit toujours le dessous) ; Normal opaque : identiques.
    for (mode, same) in [(BlendMode::Multiply, false), (BlendMode::Normal, true)] {
        let gris = solid(4, 4, [128, 128, 128, 255]);
        let out_on_black = doc_of(
            vec![
                pixel_node(
                    &solid(4, 4, [0, 0, 0, 255]),
                    100.0,
                    BlendMode::Normal,
                    0.0,
                    0.0,
                ),
                pixel_node(&gris, 100.0, mode, 0.0, 0.0),
            ],
            4,
            4,
        )
        .composite_preview()
        .expect("composite");
        let out_on_white = doc_of(
            vec![
                pixel_node(
                    &solid(4, 4, [255, 255, 255, 255]),
                    100.0,
                    BlendMode::Normal,
                    0.0,
                    0.0,
                ),
                pixel_node(&gris, 100.0, mode, 0.0, 0.0),
            ],
            4,
            4,
        )
        .composite_preview()
        .expect("composite");
        assert_eq!(
            px(&out_on_black, 0, 0) == px(&out_on_white, 0, 0),
            same,
            "mode {mode:?}"
        );
    }
}

#[test]
fn masque_inverse_masque_le_calque_dans_ses_bornes() {
    // Masque noir (couverture 0) : le calque disparaît, le dessous
    // apparaît intact, y compris hors des bornes du calque.
    let fond = solid(8, 8, [200, 40, 40, 255]);
    let carre = solid(4, 4, [40, 40, 200, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_node(&carre, [0, 0, 0, 255], true, false),
        ],
        8,
        8,
    );
    // Le nœud masqué est à l'origine : le déplacer ne change rien ici ;
    // on vérifie le composite : tout rouge, même sous le carré.
    let out = doc.composite_preview().expect("composite");
    assert!(out.pixels().all(|p| p.2.0[..3] == [200, 40, 40]));
}

#[test]
fn filtre_ne_touche_que_les_bornes_du_calque() {
    // Brightness +50 sur le carré 4x4 en (4,4) : le champ rouge reste
    // intact, seuls les pixels du calque changent.
    use datatypes::ParamValue;
    let fond = solid(8, 8, [200, 40, 40, 255]);
    let carre = solid(4, 4, [40, 40, 200, 255]);
    let mut l = PixelLayer::new("filtre", arc(&carre));
    let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
    f.params
        .insert("brightness".into(), ParamValue::Float(50.0));
    l.filter_layers.push(f);
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Pixel(l),
        ],
        8,
        8,
    );
    // Le calque filtré doit être positionné à (4,4) comme le carré.
    if let LayerNode::Pixel(l) = doc.find_mut(doc.root[1].id()).expect("calque") {
        l.transform.offset_x = 4.0;
        l.transform.offset_y = 4.0;
    }
    let out = doc.composite_preview().expect("composite");
    assert_eq!(px(&out, 0, 0)[0..3], [200, 40, 40], "champ intact");
    assert_ne!(px(&out, 6, 6)[0..3], [40, 40, 200], "calque éclairci");
}
