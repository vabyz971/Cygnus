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

#[test]
fn rotation_echange_les_dimensions_et_garde_le_centre() {
    let mut doc = Document::new(8, 8);
    // 3×1 : rouge | vert | bleu.
    let mut b = ImageBuffer::from_pixel(3, 1, Rgba([0, 0, 0, 255]));
    b.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    b.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
    b.put_pixel(2, 0, Rgba([0, 0, 255, 255]));
    let l = PixelLayer::new("tourne", Arc::new(DynamicImage::ImageRgba8(b)));
    let id = l.id;
    doc.push_layer(LayerNode::Pixel(l));
    doc.rotate(id, true).expect("rotation horaire");
    let l = doc.pixel_layer(id).expect("calque");
    // Dimensions échangées.
    assert_eq!(l.dimensions(), (1, 3));
    // Horaire : (x, 0) → (0, x) — l'ordre rouge/vert/bleu se lit de haut en bas.
    let img = l.source_image.to_rgba8();
    let haut = img.get_pixel(0, 0);
    assert_eq!([haut[0], haut[1], haut[2]], [255, 0, 0]);
    let milieu = img.get_pixel(0, 1);
    assert_eq!([milieu[0], milieu[1], milieu[2]], [0, 255, 0]);
    let bas = img.get_pixel(0, 2);
    assert_eq!([bas[0], bas[1], bas[2]], [0, 0, 255]);
    // Centre conservé : (0 + 3)/2 = (nouveau 0 + 1)/2 = 1.5.
    assert_eq!((l.transform.offset_x, l.transform.offset_y), (1.0, -1.0));
    // Antihoraire : retour exact à l'origine.
    doc.rotate(id, false).expect("rotation antihoraire");
    let l = doc.pixel_layer(id).expect("calque");
    assert_eq!(l.dimensions(), (3, 1));
    let img = l.source_image.to_rgba8();
    let gauche = img.get_pixel(0, 0);
    assert_eq!([gauche[0], gauche[1], gauche[2]], [255, 0, 0]);
    let droite = img.get_pixel(2, 0);
    assert_eq!([droite[0], droite[1], droite[2]], [0, 0, 255]);
    assert_eq!((l.transform.offset_x, l.transform.offset_y), (0.0, 0.0));
    // Calque inconnu : erreur propre, jamais de panique.
    assert!(doc.rotate(Uuid::new_v4(), true).is_err());
}

#[test]
fn bascule_actif_desactive_filtre_et_masque() {
    let mut doc = Document::new(4, 4);
    let mut pixels = PixelLayer::new(
        "fond",
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            2,
            2,
            Rgba([10, 20, 30, 255]),
        ))),
    );
    let filtre = FilterLayer::neutral("brightness_contrast", Default::default());
    let fid = filtre.id;
    pixels.filter_layers.push(filtre);
    let mut masque = LayerMask::full(2, 2);
    let mid = masque.id;
    masque.enabled = false;
    pixels.masks.push(masque);
    let id = pixels.id;
    doc.push_layer(LayerNode::Pixel(pixels));
    // Filtre : actif → inactif → actif.
    assert!(doc.set_filter_enabled(id, fid, false));
    assert!(!doc.pixel_layer(id).expect("calque").filter_layers[0].enabled);
    assert!(doc.set_filter_enabled(id, fid, true));
    assert!(doc.pixel_layer(id).expect("calque").filter_layers[0].enabled);
    // Idempotent : même valeur = vrai sans toucher.
    assert!(doc.set_filter_enabled(id, fid, true));
    // Masque : inactif → actif.
    assert!(doc.set_mask_enabled(id, mid, true));
    assert!(doc.pixel_layer(id).expect("calque").masks[0].enabled);
    // Inconnus : faux propre.
    assert!(!doc.set_filter_enabled(id, Uuid::new_v4(), true));
    assert!(!doc.set_mask_enabled(id, Uuid::new_v4(), true));
    assert!(!doc.set_mask_enabled(Uuid::new_v4(), mid, true));
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

// ---------------------------------------------------------------------------
// Composite régional expérimental (vertical slice tuiles) : le pleine cadre
// reste la référence, le régional doit être bit-identique sur sa zone.
// Document déterministe SANS calque d'ajustement (un flou verrait un
// accumulateur réduit et divergerait au bord — limitation documentée
// dans `composite_region_with`, pas contournée ici).
// ---------------------------------------------------------------------------

use super::compositing::CompositeStats;
use crate::tiles::plan_stroke_tiles;
use tiles::TileRegion;

/// Document 512² : fond rouge opaque + carré vert 200² translucide en
/// (100,50) + groupe (carré bleu 200² en (300,300), fusion Screen).
/// Tout tient dans le document → scope == doc, origine (0,0).
fn doc_regional() -> Document {
    let fond = solid(512, 512, [200, 40, 40, 255]);
    let carre = solid(200, 200, [40, 200, 40, 255]);
    let mut vert = PixelLayer::new("vert", arc(&carre));
    vert.opacity = 60.0;
    vert.transform.offset_x = 100.0;
    vert.transform.offset_y = 50.0;
    let bleu = solid(200, 200, [40, 40, 200, 255]);
    let mut dans_groupe = PixelLayer::new("bleu", arc(&bleu));
    dans_groupe.blend_mode = BlendMode::Screen;
    dans_groupe.transform.offset_x = 300.0;
    dans_groupe.transform.offset_y = 300.0;
    let groupe = LayerNode::Group(crate::document::GroupLayer::new(
        "g",
        vec![LayerNode::Pixel(dans_groupe)],
    ));
    doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Pixel(vert),
            groupe,
        ],
        512,
        512,
    )
}

/// Compare le régional à la découpe correspondante du pleine cadre
/// (égalité stricte des octets : même arithmétique par construction).
/// Retourne (pixels pleine cadre, pixels régionaux).
fn assert_region_matches(doc: &Document, region: &TileRegion) -> (u64, u64) {
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats_full = CompositeStats::default();
    let full = doc
        .composite_preview_with_stats(&resolver, &mut stats_full)
        .expect("pleine cadre");
    assert_eq!(
        (full.width(), full.height()),
        (512, 512),
        "scope == doc (hypothèse du test)"
    );
    let mut stats_reg = CompositeStats::default();
    let reg = doc
        .composite_region_with(&resolver, &mut stats_reg, region)
        .expect("région valide");
    let (rw, rh) = (reg.image.width(), reg.image.height());
    assert!(rw > 0 && rh > 0, "région utile non vide");
    let full_rgba = full.to_rgba8();
    let cropped = image::imageops::crop_imm(&full_rgba, reg.origin_x, reg.origin_y, rw, rh)
        .to_image()
        .into_raw();
    assert_eq!(
        cropped,
        reg.image.to_rgba8().into_raw(),
        "régional bit-identique au pleine cadre sur sa zone"
    );
    (stats_full.scope_px, stats_reg.scope_px)
}

#[test]
fn regional_interieur_dimensions_offset_pixels() {
    // Cas B : région centrale 256² en (128,128).
    let doc = doc_regional();
    let region = TileRegion::new(128, 128, 256, 256);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let reg = doc
        .composite_region_with(&resolver, &mut stats, &region)
        .expect("région intérieure");
    assert_eq!((reg.image.width(), reg.image.height()), (256, 256));
    assert_eq!((reg.origin_x, reg.origin_y), (128, 128));
    let (full_px, reg_px) = assert_region_matches(&doc, &region);
    assert_eq!((full_px, reg_px), (512 * 512, 256 * 256));
}

#[test]
fn regional_bords_coin_et_rive() {
    // Cas C : coin (0,0) et rive droite/basse avec clamp.
    let doc = doc_regional();
    let (full_px, reg_px) = assert_region_matches(&doc, &TileRegion::new(0, 0, 128, 128));
    assert_eq!((full_px, reg_px), (512 * 512, 128 * 128));
    // (400,400,200,200) dépasse : clampé à (400,400,112,112).
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let reg = doc
        .composite_region_with(&resolver, &mut stats, &TileRegion::new(400, 400, 200, 200))
        .expect("rive clampée");
    assert_eq!((reg.image.width(), reg.image.height()), (112, 112));
    assert_eq!((reg.origin_x, reg.origin_y), (400, 400));
    assert_region_matches(&doc, &TileRegion::new(400, 400, 200, 200));
}

#[test]
fn regional_debordement_clampe_ou_rejete() {
    // Cas D : partiellement hors cadre → clampé ; entièrement hors → None.
    let doc = doc_regional();
    assert_region_matches(&doc, &TileRegion::new(-100, -100, 200, 200));
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let reg = doc
        .composite_region_with(
            &resolver,
            &mut stats,
            &TileRegion::new(-100, -100, 200, 200),
        )
        .expect("chevauchement");
    assert_eq!((reg.image.width(), reg.image.height()), (100, 100));
    assert_eq!((reg.origin_x, reg.origin_y), (0, 0));
    assert!(
        doc.composite_region_with(&resolver, &mut stats, &TileRegion::new(600, 600, 10, 10))
            .is_none(),
        "hors cadre → fallback pleine image"
    );
}

#[test]
fn regional_plan_vide_rejette() {
    // Cas E : région vide → None (fallback automatique FULL_FRAME).
    let doc = doc_regional();
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    assert!(
        doc.composite_region_with(&resolver, &mut stats, &TileRegion::EMPTY)
            .is_none()
    );
}

#[test]
fn plan_stroke_pilote_le_regional() {
    // Vertical slice bout à bout : le plan consommé donne une région dont
    // le rendu égale le pleine cadre — sans toucher au chemin historique.
    let mut doc = doc_regional();
    // Peint un trait réel (même primitive que le worker) puis compare.
    let target = match &doc.root[0] {
        LayerNode::Pixel(l) => l.id,
        _ => panic!("fond pixels attendu"),
    };
    let points = vec![(150.0, 150.0), (350.0, 350.0)];
    let layer = doc.pixel_layer(target).expect("calque");
    let mut buf = layer.source_image.to_rgba8().into_raw();
    let (w, h) = (layer.dimensions().0, layer.dimensions().1);
    crate::paint::paint_stroke_rgba(
        &mut buf,
        w,
        h,
        &points,
        &crate::paint::BrushParams {
            radius: 12.0,
            color: [255, 255, 0],
            opacity: 1.0,
            mode: crate::paint::StrokeMode::Paint,
        },
    );
    let painted = image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(w, h, buf).expect("dimensions conservées"),
    );
    doc.set_source_image(target, painted);
    let plan = plan_stroke_tiles(&points, 12.0, doc.width, doc.height);
    assert!(plan.dirty_tiles > 0);
    assert!(!plan.bounds.is_empty());
    assert_region_matches(&doc, &plan.bounds);
}

// ---------------------------------------------------------------------------
// Phase 6C — compositing fenêtré : `regional == crop(full)` en octets.
//
// Tous les documents ci-dessous tiennent sous 65536 px : les chemins GPU
// (seuil `gpu.rs`) ne s'activent jamais — CPU déterministe partout, avec ou
// sans adaptateur. Les calques restent dans le document (scope == doc,
// origine (0,0)) sauf mention contraire.
// ---------------------------------------------------------------------------

use super::compositing::{
    ScopeWindow, SpatialScope, blur_support_px, filter_spatial_scope, scope_adjustment_window,
};
use crate::document::FilterNode;

/// Fond opaque 200² + carré contrasté 80² en (60,40) : bords francs pour
/// les flous et les masques. Scope == doc, origine (0,0).
fn doc_windowed() -> Document {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let carre = solid(80, 80, [40, 200, 40, 255]);
    doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 100.0, BlendMode::Normal, 60.0, 40.0),
        ],
        200,
        200,
    )
}

fn bc_node(brightness: f32, contrast: f32) -> FilterNode {
    let mut f = FilterNode::new("brightness_contrast");
    f.params
        .insert("brightness".to_string(), ParamValue::Float(brightness));
    f.params
        .insert("contrast".to_string(), ParamValue::Float(contrast));
    f
}

fn sat_node(saturation: f32) -> FilterNode {
    let mut f = FilterNode::new("color_correct");
    f.params
        .insert("saturation".to_string(), ParamValue::Float(saturation));
    f
}

fn adj_blur_node(radius: f32) -> FilterNode {
    let mut f = FilterNode::new("blur");
    f.params
        .insert("radius".to_string(), ParamValue::Float(radius));
    f
}

fn adj_layer(filters: Vec<FilterNode>) -> LayerNode {
    LayerNode::Adjustment(AdjustmentLayer::new("ajust", filters))
}

/// Même contrat que `assert_region_matches` sans l'hypothèse 512² :
/// octets régionaux == découpe pleine cadre. Retourne
/// (pixels pleine cadre, pixels de dépendance traités).
fn assert_region_crop(doc: &Document, region: &TileRegion) -> (u64, u64) {
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats_full = CompositeStats::default();
    let full = doc
        .composite_preview_with_stats(&resolver, &mut stats_full)
        .expect("pleine cadre");
    let (fw, fh) = (full.width(), full.height());
    let mut stats_reg = CompositeStats::default();
    let reg = doc
        .composite_region_with(&resolver, &mut stats_reg, region)
        .expect("région valide");
    let (rw, rh) = (reg.image.width(), reg.image.height());
    assert!(rw > 0 && rh > 0, "région utile non vide");
    let full_rgba = full.to_rgba8();
    let cropped = image::imageops::crop_imm(&full_rgba, reg.origin_x, reg.origin_y, rw, rh)
        .to_image()
        .into_raw();
    assert_eq!(
        cropped,
        reg.image.to_rgba8().into_raw(),
        "régional bit-identique au pleine cadre sur sa zone"
    );
    assert!(
        reg.origin_x + rw <= fw && reg.origin_y + rh <= fh,
        "fenêtre dans le cadre"
    );
    (stats_full.scope_px, stats_reg.scope_px)
}

#[test]
fn windowed_background() {
    let fond = solid(200, 200, [10, 20, 30, 255]);
    let doc = doc_of(
        vec![pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0)],
        200,
        200,
    );
    assert_eq!(
        assert_region_crop(&doc, &TileRegion::new(50, 50, 64, 64)),
        (200 * 200, 64 * 64)
    );
}

#[test]
fn windowed_paint() {
    let mut doc = doc_windowed();
    let target = doc.root[0].id();
    let layer = doc.pixel_layer(target).expect("calque");
    let mut buf = layer.source_image.to_rgba8().into_raw();
    let (w, h) = (layer.dimensions().0, layer.dimensions().1);
    crate::paint::paint_stroke_rgba(
        &mut buf,
        w,
        h,
        &[(30.0, 150.0), (170.0, 150.0)],
        &crate::paint::BrushParams {
            radius: 8.0,
            color: [255, 255, 0],
            opacity: 1.0,
            mode: crate::paint::StrokeMode::Paint,
        },
    );
    doc.set_source_image(
        target,
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(w, h, buf).expect("dimensions conservées"),
        ),
    );
    assert_region_crop(&doc, &TileRegion::new(0, 100, 200, 100));
}

#[test]
fn windowed_opacity() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let voile = solid(200, 200, [40, 40, 200, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&voile, 50.0, BlendMode::Normal, 0.0, 0.0),
        ],
        200,
        200,
    );
    assert_region_crop(&doc, &TileRegion::new(20, 20, 100, 100));
}

#[test]
fn windowed_blend_mode() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let carre = solid(80, 80, [40, 200, 40, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 100.0, BlendMode::Multiply, 60.0, 40.0),
        ],
        200,
        200,
    );
    assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120));
}

#[test]
fn windowed_group() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let a = solid(80, 80, [40, 200, 40, 255]);
    let b = solid(80, 80, [40, 40, 200, 255]);
    let mut la = PixelLayer::new("a", arc(&a));
    la.transform.offset_x = 10.0;
    la.transform.offset_y = 10.0;
    let mut lb = PixelLayer::new("b", arc(&b));
    lb.blend_mode = BlendMode::Screen;
    lb.transform.offset_x = 110.0;
    lb.transform.offset_y = 110.0;
    let groupe = LayerNode::Group(GroupLayer::new(
        "g",
        vec![LayerNode::Pixel(la), LayerNode::Pixel(lb)],
    ));
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            groupe,
        ],
        200,
        200,
    );
    assert_region_crop(&doc, &TileRegion::new(0, 0, 128, 128));
    assert_region_crop(&doc, &TileRegion::new(100, 100, 100, 100));
}

#[test]
fn windowed_nested_group() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let a = solid(60, 60, [40, 200, 40, 255]);
    let inner = LayerNode::Group(GroupLayer::new(
        "in",
        vec![pixel_node(&a, 100.0, BlendMode::Normal, 70.0, 70.0)],
    ));
    let outer = LayerNode::Group(GroupLayer::new("out", vec![inner]));
    let doc = doc_of(
        vec![pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0), outer],
        200,
        200,
    );
    assert_region_crop(&doc, &TileRegion::new(50, 50, 100, 100));
}

fn masked_layer() -> LayerNode {
    let carre = solid(80, 80, [40, 40, 200, 255]);
    let mut l = PixelLayer::new("masqué", arc(&carre));
    l.transform.offset_x = 60.0;
    l.transform.offset_y = 40.0;
    // Masque moitié gauche visible (bords francs en x=40 espace calque).
    let mut cover = ImageBuffer::from_pixel(80, 80, Rgba([255, 255, 255, 255]));
    for y in 0..80 {
        for x in 40..80 {
            cover.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }
    l.masks.push(LayerMask {
        id: Uuid::new_v4(),
        name: String::from("m"),
        image: Arc::new(cover),
        enabled: true,
        inverted: false,
        version: next_appearance_version(),
    });
    LayerNode::Pixel(l)
}

#[test]
fn windowed_mask() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_layer(),
        ],
        200,
        200,
    );
    // À cheval sur le bord du masque (x=100 en doc).
    assert_region_crop(&doc, &TileRegion::new(60, 0, 120, 200));
}

#[test]
fn windowed_transform_scale() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let petit = solid(40, 40, [40, 200, 40, 255]);
    let mut l = PixelLayer::new("zoom", arc(&petit));
    l.transform.scale_x = 2.0;
    l.transform.scale_y = 2.0;
    l.transform.offset_x = 60.0;
    l.transform.offset_y = 40.0;
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Pixel(l),
        ],
        200,
        200,
    );
    assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120));
}

#[test]
fn windowed_rotation() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let carre = solid(80, 40, [40, 200, 40, 255]);
    let mut l = PixelLayer::new("tourné", arc(&carre));
    l.transform.rotation_deg = 90.0;
    l.transform.offset_x = 60.0;
    l.transform.offset_y = 80.0;
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Pixel(l),
        ],
        200,
        200,
    );
    assert_region_crop(&doc, &TileRegion::new(40, 60, 120, 120));
}

#[test]
fn windowed_filter_blur_appearance() {
    // Flou en sous-calque de filtre : apparence calculée pleine taille par
    // le renderer — le repli régional l'échantillonne, halo inutile.
    let mut doc = doc_windowed();
    if let LayerNode::Pixel(l) = doc.find_mut(doc.root[1].id()).expect("carré") {
        let mut f = crate::document::FilterLayer::neutral("blur", Default::default());
        f.params
            .insert("radius".to_string(), ParamValue::Float(3.0));
        l.filter_layers.push(f);
    }
    assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120));
}

#[test]
fn windowed_adjust_blur_halo() {
    let mut doc = doc_windowed();
    doc.root.push(adj_layer(vec![adj_blur_node(3.0)]));
    let (full_px, reg_px) = assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120));
    assert_eq!(full_px, 200 * 200);
    // Dépendance = région + support exact (halo 8 pour σ=3).
    let dep = 120 + 2 * blur_support_px(3.0) as u64;
    assert_eq!(reg_px, dep * dep);
}

#[test]
fn windowed_adjust_multi_blur() {
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![adj_blur_node(2.0), adj_blur_node(3.0)]));
    let (full_px, reg_px) = assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120));
    assert_eq!(full_px, 200 * 200);
    // Chaîne séquentielle : halos additionnés (règle Phase 6B).
    let dep = 120 + 2 * (blur_support_px(2.0) + blur_support_px(3.0)) as u64;
    assert_eq!(reg_px, dep * dep);
}

#[test]
fn windowed_adjust_local() {
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![bc_node(20.0, 10.0), sat_node(1.5)]));
    // Ponctuel : dépendance == requête, aucun pixel superflu.
    assert_eq!(
        assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120)),
        (200 * 200, 120 * 120)
    );
}

#[test]
fn windowed_adjust_global_unknown() {
    // Effet inconnu (version future) : passthrough côté rendu, mais repli
    // pleine cadre + découpe — pixels justes, coût pleine cadre assumé.
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![FilterNode::new("futur_effet")]));
    assert_eq!(
        assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120)),
        (200 * 200, 200 * 200)
    );
}

#[test]
fn windowed_group_plus_adjust() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let a = solid(80, 80, [40, 200, 40, 255]);
    let groupe = LayerNode::Group(GroupLayer::new(
        "g",
        vec![pixel_node(&a, 100.0, BlendMode::Normal, 60.0, 40.0)],
    ));
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            groupe,
        ],
        200,
        200,
    );
    doc.root.push(adj_layer(vec![bc_node(-10.0, 5.0)]));
    assert_eq!(
        assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120)),
        (200 * 200, 120 * 120)
    );
}

#[test]
fn windowed_mask_plus_adjust() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_layer(),
        ],
        200,
        200,
    );
    doc.root.push(adj_layer(vec![sat_node(0.5)]));
    assert_region_crop(&doc, &TileRegion::new(60, 0, 120, 200));
}

#[test]
fn windowed_group_mask_fallback_global() {
    // Masque de groupe actif : aligné sur l'origine de l'accumulateur, donc
    // non invariant par décalage — repli pleine cadre + découpe (juste).
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let a = solid(80, 80, [40, 40, 200, 255]);
    let mut g = GroupLayer::new(
        "g",
        vec![pixel_node(&a, 100.0, BlendMode::Normal, 60.0, 40.0)],
    );
    g.masks.push(LayerMask::full(200, 200));
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            LayerNode::Group(g),
        ],
        200,
        200,
    );
    assert_eq!(
        assert_region_crop(&doc, &TileRegion::new(40, 20, 120, 120)),
        (200 * 200, 200 * 200)
    );
}

#[test]
fn windowed_partial_outside() {
    let doc = doc_windowed();
    let (full_px, reg_px) = assert_region_crop(&doc, &TileRegion::new(-50, -50, 150, 150));
    assert_eq!(full_px, 200 * 200);
    assert_eq!(reg_px, 100 * 100, "clampé au cadre");
}

#[test]
fn windowed_fully_outside_is_none() {
    let doc = doc_windowed();
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    assert!(
        doc.composite_region_with(&resolver, &mut stats, &TileRegion::new(600, 600, 10, 10))
            .is_none()
    );
    assert!(
        doc.composite_region_with(&resolver, &mut stats, &TileRegion::EMPTY)
            .is_none()
    );
}

#[test]
fn windowed_tile_boundary() {
    // Document 512² sans ajustement (aucun noyau GPU en jeu) : région à
    // cheval sur les frontières x=256 et y=256.
    let doc = doc_regional();
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let reg = doc
        .composite_region_with(&resolver, &mut stats, &TileRegion::new(128, 128, 256, 256))
        .expect("région à cheval");
    assert_eq!((reg.image.width(), reg.image.height()), (256, 256));
    assert_eq!((reg.origin_x, reg.origin_y), (128, 128));
    assert_region_matches(&doc, &TileRegion::new(128, 128, 256, 256));
}

#[test]
fn windowed_local_adjust_avoids_full_frame() {
    // Test critique : petite région + ajustement local ⇒ le calcul traité
    // (dépendance) reste la fenêtre, très inférieur au pleine cadre.
    // Document 224² = 50176 px (< 65536 : CPU garanti, même avec GPU).
    let fond = solid(224, 224, [200, 40, 40, 255]);
    let carre = solid(80, 80, [40, 200, 40, 255]);
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 100.0, BlendMode::Normal, 60.0, 40.0),
        ],
        224,
        224,
    );
    doc.root.push(adj_layer(vec![bc_node(15.0, 0.0)]));
    let (full_px, reg_px) = assert_region_crop(&doc, &TileRegion::new(0, 0, 64, 64));
    assert_eq!(full_px, 224 * 224);
    assert_eq!(reg_px, 64 * 64, "fenêtre seule, sans halo");
    assert!(reg_px * 12 < full_px, "ordre de grandeur du gain");
}

#[test]
fn classification_spatiale_des_effets() {
    assert_eq!(
        filter_spatial_scope(&bc_node(10.0, 0.0)),
        SpatialScope::Local
    );
    assert_eq!(filter_spatial_scope(&sat_node(2.0)), SpatialScope::Local);
    assert_eq!(
        filter_spatial_scope(&adj_blur_node(3.0)),
        SpatialScope::Neighborhood {
            halo_px: blur_support_px(3.0)
        }
    );
    // Flou quasi nul ⇒ passthrough ⇒ local.
    assert_eq!(
        filter_spatial_scope(&adj_blur_node(0.05)),
        SpatialScope::Local
    );
    assert_eq!(
        filter_spatial_scope(&FilterNode::new("futur_effet")),
        SpatialScope::Global
    );
    // Supports exacts du noyau gaussien CPU (image 0.25).
    assert_eq!(blur_support_px(0.0), 0);
    assert_eq!(blur_support_px(1.0), 2);
    assert_eq!(blur_support_px(2.0), 5);
    assert_eq!(blur_support_px(3.0), 8);
    assert_eq!(blur_support_px(10.0), 32);
}

#[test]
fn fenetre_ajustement_somme_et_replis() {
    // Chaîne : halos additionnés, pas de repli.
    let nodes = vec![adj_layer(vec![adj_blur_node(2.0), adj_blur_node(3.0)])];
    assert_eq!(
        scope_adjustment_window(&nodes),
        ScopeWindow {
            halo_px: blur_support_px(2.0) + blur_support_px(3.0),
            global: false
        }
    );
    // Inconnu ⇒ repli global (halo conservé pour la mesure).
    let nodes = vec![adj_layer(vec![bc_node(1.0, 0.0), FilterNode::new("x")])];
    assert!(scope_adjustment_window(&nodes).global);
    // Désactivés : ignorés (comme render_nodes).
    let mut f = adj_blur_node(10.0);
    f.enabled = false;
    let mut g = FilterNode::new("y");
    g.enabled = false;
    let nodes = vec![adj_layer(vec![f, g])];
    assert_eq!(scope_adjustment_window(&nodes), ScopeWindow::default());
    // Groupe invisible : ignoré.
    let mut g = GroupLayer::new("g", nodes);
    g.visible = false;
    assert_eq!(
        scope_adjustment_window(&[LayerNode::Group(g)]),
        ScopeWindow::default()
    );
}

// ---------------------------------------------------------------------------
// Phase 6D — TileCache + DirtyRegion : `cache(R) == crop(full, R)` en octets.
//
// Règle de validité : le dirty décide (§6) — après une mutation marquée,
// seules les tuiles intersectant la zone sale sont recalculées ; les autres
// restent des hits (signature rafraîchie). Contrat : marquer AVANT relire,
// `clear_dirty` après présentation.
// ---------------------------------------------------------------------------

use super::tile_cache::{TILE_CACHE_FLAGS_NONE, TileCache, get_or_render};
use super::tree::RegionalComposite;
use crate::tile_key::{BackendTag, tile_content_signature};

/// Rend R via le cache en comptant les évaluations régionales (1 miss avec
/// contenu contribuant == 1 rendu). Retourne (tuile, nouveaux renders).
fn render_cached(
    cache: &mut TileCache,
    doc: &Document,
    region: &TileRegion,
) -> (Option<RegionalComposite>, u64) {
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let content = tile_content_signature(doc);
    let misses_avant = cache.stats().misses;
    let out = get_or_render(
        cache,
        doc,
        &resolver,
        &mut stats,
        region,
        BackendTag::cpu(),
        TILE_CACHE_FLAGS_NONE,
        1.0,
        content,
    );
    (out, cache.stats().misses - misses_avant)
}

/// La tuile égale la découpe pleine cadre à son origine (octets).
fn assert_tile_matches_full(doc: &Document, tile: &RegionalComposite) {
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let full = doc
        .composite_preview_with_stats(&resolver, &mut stats)
        .expect("pleine cadre");
    let (w, h) = (tile.image.width(), tile.image.height());
    let (fw, fh) = (full.width(), full.height());
    assert!(
        tile.origin_x + w <= fw && tile.origin_y + h <= fh,
        "fenêtre dans le cadre"
    );
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), tile.origin_x, tile.origin_y, w, h)
        .to_image()
        .into_raw();
    let raw = tile.image.to_rgba8().into_raw();
    assert_eq!(cropped.len(), raw.len(), "mêmes dimensions");
    if cropped != raw {
        let n = cropped
            .iter()
            .zip(raw.iter())
            .filter(|(a, b)| a != b)
            .count();
        let i = cropped
            .iter()
            .zip(raw.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "cache(R) == crop(full, R) : {} octets diffèrent sur {}, premier à {} (pixel {},{}, origine {},{})",
            n,
            cropped.len(),
            i,
            (i / 4) % w as usize,
            (i / 4) / w as usize,
            tile.origin_x,
            tile.origin_y
        );
    }
}

fn cache_4mo() -> TileCache {
    TileCache::new(1 << 24)
}

#[test]
fn cache_miss_puis_hit_un_seul_rendu() {
    let doc = doc_windowed();
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, renders) = render_cached(&mut cache, &doc, &region);
    let tile = tile.expect("contribue");
    assert_eq!(renders, 1, "cold = 1 rendu");
    assert_tile_matches_full(&doc, &tile);
    let (tile2, renders2) = render_cached(&mut cache, &doc, &region);
    assert_eq!(renders2, 0, "warm = 0 rendu");
    assert_eq!(
        tile2.expect("hit").image.to_rgba8().into_raw(),
        tile.image.to_rgba8().into_raw()
    );
    assert_eq!(cache.stats().renders_avoided(), 1);
}

#[test]
fn cache_tuiles_distinctes_et_repetition() {
    let doc = doc_windowed();
    let mut cache = cache_4mo();
    let regions = [
        TileRegion::new(0, 0, 100, 100),
        TileRegion::new(100, 0, 100, 100),
        TileRegion::new(0, 100, 100, 100),
    ];
    let mut renders = 0;
    for r in &regions {
        let (tile, n) = render_cached(&mut cache, &doc, r);
        renders += n;
        assert_tile_matches_full(&doc, &tile.expect("contribue"));
    }
    assert_eq!(renders, 3);
    for _ in 0..2 {
        for r in &regions {
            let (_, n) = render_cached(&mut cache, &doc, r);
            renders += n;
        }
    }
    assert_eq!(renders, 3, "secondes passes = 0 rendu");
    assert_eq!((cache.stats().hits, cache.stats().misses), (6, 3));
}

#[test]
fn cache_paint_invalidation_partielle() {
    // §17 : petite peinture ⇒ seule la tuile sale est recalculée.
    let mut doc = doc_windowed();
    let mut cache = cache_4mo();
    let sale = TileRegion::new(0, 100, 200, 100);
    let intacte = TileRegion::new(0, 0, 200, 100);
    let (_, n0) = render_cached(&mut cache, &doc, &sale);
    let (_, n1) = render_cached(&mut cache, &doc, &intacte);
    assert_eq!(n0 + n1, 2);
    // Peinture réelle sur le fond (identité : layer == doc space).
    let target = doc.root[0].id();
    let points = vec![(30.0, 150.0), (170.0, 150.0)];
    let layer = doc.pixel_layer(target).expect("fond");
    let mut buf = layer.source_image.to_rgba8().into_raw();
    let (w, h) = (layer.dimensions().0, layer.dimensions().1);
    crate::paint::paint_stroke_rgba(
        &mut buf,
        w,
        h,
        &points,
        &crate::paint::BrushParams {
            radius: 8.0,
            color: [255, 255, 0],
            opacity: 1.0,
            mode: crate::paint::StrokeMode::Paint,
        },
    );
    doc.set_source_image(
        target,
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(w, h, buf).expect("dimensions conservées"),
        ),
    );
    let dirty = crate::tiles::stroke_dirty_region(&points, 8.0, tiles::Padding::ZERO)
        .expect("geste valide");
    cache.mark_dirty_scope(
        dirty,
        ScopeWindow {
            halo_px: 0,
            global: false,
        },
        doc.width,
        doc.height,
    );
    let (tile_sale, n_sale) = render_cached(&mut cache, &doc, &sale);
    assert_eq!(n_sale, 1, "tuile sale recalculée");
    assert_tile_matches_full(&doc, &tile_sale.expect("contribue"));
    let (tile_intacte, n_intacte) = render_cached(&mut cache, &doc, &intacte);
    assert_eq!(n_intacte, 0, "tuile intacte = hit malgré contenu changé");
    assert_tile_matches_full(&doc, &tile_intacte.expect("hit"));
}

#[test]
fn cache_opacity_invalidation() {
    let mut doc = doc_windowed();
    let mut cache = cache_4mo();
    let proche = TileRegion::new(40, 20, 120, 120);
    let loin = TileRegion::new(140, 120, 60, 80);
    let (_, n) = render_cached(&mut cache, &doc, &proche);
    let (_, m) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n + m, 2);
    let id = doc.root[1].id();
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.opacity = 30.0;
    }
    cache.mark_dirty(TileRegion::new(60, 40, 80, 80));
    let (tile, n) = render_cached(&mut cache, &doc, &proche);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (tile, n) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n, 0, "hors empreinte = hit");
    assert_tile_matches_full(&doc, &tile.expect("hit"));
}

#[test]
fn cache_transform_invalidation_old_union_new() {
    let mut doc = doc_windowed();
    let mut cache = cache_4mo();
    let proche = TileRegion::new(40, 20, 120, 120);
    let loin = TileRegion::new(0, 120, 60, 80);
    let (_, n) = render_cached(&mut cache, &doc, &proche);
    let (_, m) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n + m, 2);
    // Déplacement +40 px : union 6B réelle comme zone sale.
    let id = doc.root[1].id();
    let old_t = match doc.find(id) {
        Some(LayerNode::Pixel(l)) => l.transform,
        _ => panic!("carré attendu"),
    };
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.transform.offset_x += 40.0;
    }
    let new_t = match doc.find(id) {
        Some(LayerNode::Pixel(l)) => l.transform,
        _ => panic!("carré attendu"),
    };
    let union = crate::tiles::layer_move_dirty_region(&old_t, &new_t, 80, 80, tiles::Padding::ZERO);
    assert_eq!(union, TileRegion::new(60, 40, 120, 80));
    cache.mark_dirty(union);
    let (tile, n) = render_cached(&mut cache, &doc, &proche);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (tile, n) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n, 0);
    assert_tile_matches_full(&doc, &tile.expect("hit"));
}

#[test]
fn cache_mask_invalidation() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let mut doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            masked_layer(),
        ],
        200,
        200,
    );
    let mut cache = cache_4mo();
    let proche = TileRegion::new(60, 0, 120, 200);
    let loin = TileRegion::new(140, 120, 60, 80);
    let (_, n) = render_cached(&mut cache, &doc, &proche);
    let (_, m) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n + m, 2);
    let id = doc.root[1].id();
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.masks[0].enabled = false;
    }
    cache.mark_dirty(TileRegion::new(60, 40, 80, 80));
    let (tile, n) = render_cached(&mut cache, &doc, &proche);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (tile, n) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n, 0);
    assert_tile_matches_full(&doc, &tile.expect("hit"));
}

#[test]
fn cache_filter_invalidation() {
    let mut doc = doc_windowed();
    let mut cache = cache_4mo();
    let proche = TileRegion::new(40, 20, 120, 120);
    let loin = TileRegion::new(140, 120, 60, 80);
    let (_, n) = render_cached(&mut cache, &doc, &proche);
    let (_, m) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n + m, 2);
    let id = doc.root[1].id();
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        let mut f =
            crate::document::FilterLayer::neutral("brightness_contrast", Default::default());
        f.params
            .insert("brightness".to_string(), ParamValue::Float(25.0));
        l.filter_layers.push(f);
    }
    cache.mark_dirty(TileRegion::new(60, 40, 80, 80));
    let (tile, n) = render_cached(&mut cache, &doc, &proche);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (tile, n) = render_cached(&mut cache, &doc, &loin);
    assert_eq!(n, 0);
    assert_tile_matches_full(&doc, &tile.expect("hit"));
}

#[test]
fn cache_local_adjustment() {
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![bc_node(20.0, 10.0), sat_node(1.5)]));
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0, "ajustement local remarché = hit");
    // Nouveau réglage ⇒ tout l'accumulateur change ⇒ global.
    if let LayerNode::Adjustment(a) = &mut doc.root[2] {
        a.filters[0]
            .params
            .insert("brightness".to_string(), ParamValue::Float(99.0));
    }
    cache.mark_global(doc.width, doc.height);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
}

#[test]
fn cache_blur_adjustment() {
    let mut doc = doc_windowed();
    doc.root.push(adj_layer(vec![adj_blur_node(3.0)]));
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0, "même halo déterministe ⇒ hit");
}

#[test]
fn cache_multi_blur_adjustment() {
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![adj_blur_node(2.0), adj_blur_node(3.0)]));
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0);
}

#[test]
fn cache_mixed_adjust_blur() {
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![bc_node(10.0, 0.0), adj_blur_node(2.0)]));
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0);
}

#[test]
fn cache_global_adjustment_fallback() {
    // §15 : inconnu ⇒ full-frame + crop, pixels justes, tuile cachée.
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![FilterNode::new("futur_effet")]));
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0, "repli déterministe ⇒ hit");
    // Modification pertinente ⇒ global ⇒ invalide.
    if let LayerNode::Adjustment(a) = &mut doc.root[2] {
        a.filters.push(bc_node(5.0, 0.0));
    }
    let window = scope_adjustment_window(&doc.root);
    assert!(window.global, "inconnu ⇒ global");
    cache.mark_dirty_scope(region, window, doc.width, doc.height);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
}

#[test]
fn cache_group_nested() {
    let fond = solid(200, 200, [200, 40, 40, 255]);
    let a = solid(80, 80, [40, 200, 40, 255]);
    let inner = LayerNode::Group(GroupLayer::new(
        "in",
        vec![pixel_node(&a, 80.0, BlendMode::Screen, 60.0, 40.0)],
    ));
    let outer = LayerNode::Group(GroupLayer::new("out", vec![inner]));
    let doc = doc_of(
        vec![pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0), outer],
        200,
        200,
    );
    let mut cache = cache_4mo();
    let region = TileRegion::new(40, 20, 120, 120);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0);
}

#[test]
fn cache_partial_outside() {
    let doc = doc_windowed();
    let mut cache = cache_4mo();
    let region = TileRegion::new(-50, -50, 150, 150);
    let (tile, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 1);
    let tile = tile.expect("chevauche");
    assert_eq!((tile.image.width(), tile.image.height()), (100, 100));
    assert_tile_matches_full(&doc, &tile);
    let (_, n) = render_cached(&mut cache, &doc, &region);
    assert_eq!(n, 0);
}

#[test]
fn cache_outside_is_none() {
    let doc = doc_windowed();
    let mut cache = cache_4mo();
    let region = TileRegion::new(600, 600, 10, 10);
    let (tile, n1) = render_cached(&mut cache, &doc, &region);
    assert!(tile.is_none());
    let (_, n2) = render_cached(&mut cache, &doc, &region);
    assert!(tile.is_none());
    assert_eq!(n1 + n2, 2, "rien stocké ⇒ miss à chaque fois");
    assert!(cache.is_empty());
}

#[test]
fn cache_blur_halo_invalidation() {
    // §14 : flou σ=3 (support 8, mêmes valeurs que 6C) — pixel sale à 5 px
    // hors de R mais dans D ⇒ R invalide ; à 50 px ⇒ hit préservé.
    assert_eq!(blur_support_px(3.0), 8);
    let mut doc = doc_windowed();
    doc.root.push(adj_layer(vec![adj_blur_node(3.0)]));
    let window = scope_adjustment_window(&doc.root);
    assert_eq!(window.halo_px, blur_support_px(3.0));
    assert!(!window.global);
    let mut cache = cache_4mo();
    let r = TileRegion::new(60, 60, 80, 80);
    let (_, n) = render_cached(&mut cache, &doc, &r);
    assert_eq!(n, 1);
    // Sale à 5 px à gauche de R (dans D = R + 8) ⇒ miss.
    cache.mark_dirty_scope(TileRegion::new(50, 80, 5, 5), window, doc.width, doc.height);
    let (tile, n) = render_cached(&mut cache, &doc, &r);
    assert_eq!(n, 1, "dans la dépendance ⇒ invalide");
    assert_tile_matches_full(&doc, &tile.expect("contribue"));
    cache.clear_dirty();
    // Sale à 50 px (hors D) ⇒ hit.
    cache.mark_dirty_scope(TileRegion::new(0, 0, 5, 5), window, doc.width, doc.height);
    let (tile, n) = render_cached(&mut cache, &doc, &r);
    assert_eq!(n, 0, "hors dépendance ⇒ hit préservé");
    assert_tile_matches_full(&doc, &tile.expect("hit"));
}

#[test]
fn cache_tile_assembly_3x3() {
    // §16 : 9 tuiles 128² assemblées == pleine cadre 384², octet par octet ;
    // seconde passe = 9 hits, 0 rendu.
    let fond = solid(384, 384, [200, 40, 40, 255]);
    let carre = solid(160, 160, [40, 200, 40, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 70.0, BlendMode::Multiply, 112.0, 112.0),
        ],
        384,
        384,
    );
    let mut cache = cache_4mo();
    let mut renders = 0u64;
    let mut assembled = image::ImageBuffer::from_pixel(384, 384, image::Rgba([0u8, 0, 0, 0]));
    for ty in 0..3 {
        for tx in 0..3 {
            let region = TileRegion::new(tx * 128, ty * 128, 128, 128);
            let (tile, n) = render_cached(&mut cache, &doc, &region);
            renders += n;
            let tile = tile.expect("contribue");
            let img = tile.image.to_rgba8();
            for (x, y, p) in img.enumerate_pixels() {
                assembled.put_pixel(tile.origin_x + x, tile.origin_y + y, *p);
            }
        }
    }
    assert_eq!(renders, 9);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let full = doc
        .composite_preview_with_stats(&resolver, &mut stats)
        .expect("pleine cadre");
    assert_eq!(
        assembled.into_raw(),
        full.to_rgba8().into_raw(),
        "assemblage == pleine cadre"
    );
    for ty in 0..3 {
        for tx in 0..3 {
            let (_, n) = render_cached(
                &mut cache,
                &doc,
                &TileRegion::new(tx * 128, ty * 128, 128, 128),
            );
            renders += n;
        }
    }
    assert_eq!(renders, 9, "seconde passe = 9 hits");
    assert_eq!(cache.stats().renders_avoided(), 9);
}

#[test]
fn mesures_full_regional_cache_avec_sans_blur() {
    // §22 : 512² sans ajustement (aucun noyau GPU : blends CPU seuls).
    let fond = solid(512, 512, [200, 40, 40, 255]);
    let carre = solid(200, 200, [40, 200, 40, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 60.0, BlendMode::Normal, 100.0, 50.0),
        ],
        512,
        512,
    );
    let resolver = |id: Uuid| doc.appearance_image(id);
    // Pleine cadre de référence.
    let mut stats_full = CompositeStats::default();
    doc.composite_preview_with_stats(&resolver, &mut stats_full)
        .expect("pleine cadre");
    assert_eq!(stats_full.scope_px, 512 * 512, "full = 262144 px");
    // Tuile froide 256² : dépendance == requête (local pur).
    let mut cache = cache_4mo();
    let region = TileRegion::new(128, 128, 256, 256);
    let mut stats_cold = CompositeStats::default();
    let content = tile_content_signature(&doc);
    let misses_avant = cache.stats().misses;
    let tile = get_or_render(
        &mut cache,
        &doc,
        &resolver,
        &mut stats_cold,
        &region,
        BackendTag::cpu(),
        TILE_CACHE_FLAGS_NONE,
        1.0,
        content,
    )
    .expect("contribue");
    assert_eq!(cache.stats().misses - misses_avant, 1);
    assert_eq!(stats_cold.scope_px, 256 * 256, "dépendance = requête");
    assert_tile_matches_full(&doc, &tile);
    // Tuile chaude : aucun calcul (stats vierges + hit).
    let mut stats_warm = CompositeStats::default();
    let misses_avant = cache.stats().misses;
    let tile2 = get_or_render(
        &mut cache,
        &doc,
        &resolver,
        &mut stats_warm,
        &region,
        BackendTag::cpu(),
        TILE_CACHE_FLAGS_NONE,
        1.0,
        content,
    )
    .expect("hit");
    assert_eq!(cache.stats().misses - misses_avant, 0);
    assert_eq!(stats_warm.scope_px, 0, "aucun pixel retraité");
    assert_eq!(cache.stats().renders_avoided(), 1);
    assert_eq!(
        tile2.image.to_rgba8().into_raw(),
        tile.image.to_rgba8().into_raw()
    );
    // Avec blur σ=3 : requête 256², dépendance (256+16)², rendu == dépendance.
    let mut doc_blur = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 60.0, BlendMode::Normal, 100.0, 50.0),
        ],
        512,
        512,
    );
    doc_blur.root.push(adj_layer(vec![adj_blur_node(3.0)]));
    let resolver_blur = |id: Uuid| doc_blur.appearance_image(id);
    let mut stats_blur = CompositeStats::default();
    let content = tile_content_signature(&doc_blur);
    let tile = get_or_render(
        &mut cache,
        &doc_blur,
        &resolver_blur,
        &mut stats_blur,
        &region,
        BackendTag::cpu(),
        TILE_CACHE_FLAGS_NONE,
        1.0,
        content,
    )
    .expect("contribue");
    let requested = 256u64 * 256;
    let dependency = (256 + 2 * u64::from(blur_support_px(3.0))).pow(2);
    assert_eq!(requested, 65_536);
    assert_eq!(dependency, 272 * 272);
    assert_eq!(
        stats_blur.scope_px, dependency,
        "pixels de dépendance réellement rendus"
    );
    assert_tile_matches_full(&doc_blur, &tile);
}

// ---------------------------------------------------------------------------
// Phase 6E — RenderWorker : viewport → tiles → cache → assemblage.
//
// Le viewport plein document assemblé égale le pleine cadre en octets ;
// seules les tuiles sales sont réévaluées (comptées, pas estimées).
// ---------------------------------------------------------------------------

use super::worker::{AssembledView, RenderWorker, RenderWorkerStats, ScopeGeom};
use crate::tiles::{VIEWPORT_TILE_PX, appearance_spread, dirty_tile_rects, node_footprint};

fn worker_4mo(tile_px: u32) -> RenderWorker {
    RenderWorker::new(1 << 24, tile_px)
}

/// Rend le viewport plein document (scope == doc : contenu intérieur) et
/// retourne (vue, stats worker, stats composite cumulées).
fn render_doc_viewport(
    worker: &mut RenderWorker,
    doc: &Document,
) -> (Option<AssembledView>, RenderWorkerStats, CompositeStats) {
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut render_stats = CompositeStats::default();
    let scope = ScopeGeom::document(doc.width, doc.height);
    let request = RenderRequest::new(
        TileRegion::new(0, 0, doc.width.max(1), doc.height.max(1)),
        1.0,
    );
    let mut stats = RenderWorkerStats::default();
    let out = worker.render(
        doc,
        &resolver,
        &mut render_stats,
        scope,
        &request,
        &RenderPriorityContext::idle(),
        &mut stats,
    );
    (out, stats, render_stats)
}

/// La vue assemblée égale le pleine cadre en octets.
fn assert_view_matches_full(doc: &Document, view: &AssembledView) {
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = CompositeStats::default();
    let full = doc
        .composite_preview_with_stats(&resolver, &mut stats)
        .expect("pleine cadre");
    assert_eq!((view.origin_x, view.origin_y), (0, 0));
    assert_eq!(
        (view.image.width(), view.image.height()),
        (full.width(), full.height())
    );
    assert_eq!(
        view.image.to_rgba8().into_raw(),
        full.to_rgba8().into_raw(),
        "assemblage == pleine cadre"
    );
}

/// Document 384² : fond + carré multiply (aucun noyau GPU en jeu).
fn doc_384() -> Document {
    let fond = solid(384, 384, [200, 40, 40, 255]);
    let carre = solid(160, 160, [40, 200, 40, 255]);
    doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 70.0, BlendMode::Multiply, 112.0, 112.0),
        ],
        384,
        384,
    )
}

#[test]
fn worker_cold_warm_3x3() {
    // §12 : 9 tuiles froides = 9 rendus ; seconde passe = 9 hits, 0 rendu.
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    let view = view.expect("contribue");
    assert_eq!(
        (
            stats.requested_tiles,
            stats.rendered_tiles,
            stats.cache_hits
        ),
        (9, 9, 0)
    );
    assert_view_matches_full(&doc, &view);
    let (view2, stats2, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(
        (
            stats2.requested_tiles,
            stats2.rendered_tiles,
            stats2.cache_hits
        ),
        (9, 0, 9)
    );
    assert_view_matches_full(&doc, &view2.expect("hit"));
}

#[test]
fn worker_petite_modification_minimale() {
    // §12/§14 : petite peinture ⇒ 1 tuile sale ⇒ 1 rendu ; le reste en hits.
    let mut doc = doc_384();
    let mut worker = worker_4mo(128);
    let (_, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 9);
    // Peinture 40×40 dans la tuile (0,0), calque fond (identité).
    let target = doc.root[0].id();
    let points = vec![(20.0, 20.0), (60.0, 60.0)];
    let layer = doc.pixel_layer(target).expect("fond");
    let mut buf = layer.source_image.to_rgba8().into_raw();
    let (w, h) = (layer.dimensions().0, layer.dimensions().1);
    crate::paint::paint_stroke_rgba(
        &mut buf,
        w,
        h,
        &points,
        &crate::paint::BrushParams {
            radius: 4.0,
            color: [255, 255, 0],
            opacity: 1.0,
            mode: crate::paint::StrokeMode::Paint,
        },
    );
    doc.set_source_image(
        target,
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(w, h, buf).expect("dimensions conservées"),
        ),
    );
    let dirty = crate::tiles::stroke_dirty_region(&points, 4.0, tiles::Padding::ZERO)
        .expect("geste valide");
    worker.cache_mut().mark_dirty(dirty);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.dirty_tiles, 1, "une seule tuile sale");
    assert_eq!(stats.rendered_tiles, 1, "un seul rendu");
    assert_eq!(stats.cache_hits, 8, "le reste réutilisé");
    assert_view_matches_full(&doc, &view.expect("contribue"));
}

#[test]
fn worker_viewport_scroll() {
    // §13 : viewport B chevauchant A ⇒ présentes = hits, nouvelles = miss.
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let mut render_stats = CompositeStats::default();
    let mut stats_a = RenderWorkerStats::default();
    let view_a = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(0, 0, 256, 256), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats_a,
        )
        .expect("viewport A");
    assert_eq!((stats_a.requested_tiles, stats_a.rendered_tiles), (4, 4));
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), 0, 0, 256, 256)
        .to_image()
        .into_raw();
    assert_eq!(view_a.image.to_rgba8().into_raw(), cropped);
    // B décalé de 128 px : 2 tuiles partagées, 2 nouvelles.
    let mut stats_b = RenderWorkerStats::default();
    let view_b = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(128, 0, 256, 256), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats_b,
        )
        .expect("viewport B");
    assert_eq!(stats_b.requested_tiles, 4);
    assert_eq!(stats_b.rendered_tiles, 2, "seules les nouvelles tuiles");
    assert_eq!(stats_b.cache_hits, 2, "présentes réutilisées");
    assert_eq!((view_b.origin_x, view_b.origin_y), (128, 0));
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), 128, 0, 256, 256)
        .to_image()
        .into_raw();
    assert_eq!(view_b.image.to_rgba8().into_raw(), cropped);
}

#[test]
fn worker_blur_invalidation() {
    // §9 : halo 6C réel — seules les tuiles dans la dépendance sont refaites.
    let mut doc = doc_windowed();
    doc.root.push(adj_layer(vec![adj_blur_node(3.0)]));
    let mut worker = worker_4mo(100);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (4, 4));
    assert_view_matches_full(&doc, &view.expect("contribue"));
    let window = scope_adjustment_window(&doc.root);
    // Sale en (95,50) : hors de R=(100,0,100,100) mais dans D=R+8 ⇒ R sale ;
    // les tuiles basses (y≥100) restent propres.
    worker.cache_mut().mark_dirty_scope(
        TileRegion::new(95, 50, 3, 3),
        window,
        doc.width,
        doc.height,
    );
    let dirty = dirty_tile_rects(worker.cache().dirty(), doc.width, doc.height, 100);
    assert_eq!(dirty.len(), 2, "dépendance à cheval : 2 tuiles");
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.dirty_tiles, 2);
    assert_eq!(stats.rendered_tiles, 2);
    assert_eq!(stats.cache_hits, 2);
    assert_view_matches_full(&doc, &view.expect("contribue"));
}

#[test]
fn worker_global_invalidation() {
    // §10 : opération globale ⇒ toutes les tuiles requises refaites, pixels justes.
    let mut doc = doc_windowed();
    doc.root
        .push(adj_layer(vec![FilterNode::new("futur_effet")]));
    let mut worker = worker_4mo(100);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (4, 4));
    assert_view_matches_full(&doc, &view.expect("contribue"));
    worker.cache_mut().mark_global(doc.width, doc.height);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.dirty_tiles, 4);
    assert_eq!(stats.rendered_tiles, 4);
    assert_view_matches_full(&doc, &view.expect("contribue"));
}

#[test]
fn worker_matrice_pixel() {
    // §16 : chaque scénario assemblé == pleine cadre en octets.
    let scenarios: Vec<(&str, Document)> = vec![
        ("fond", {
            let fond = solid(200, 200, [10, 20, 30, 255]);
            doc_of(
                vec![pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0)],
                200,
                200,
            )
        }),
        ("peinture", {
            let mut doc = doc_windowed();
            let target = doc.root[0].id();
            let layer = doc.pixel_layer(target).expect("fond");
            let mut buf = layer.source_image.to_rgba8().into_raw();
            let (w, h) = (layer.dimensions().0, layer.dimensions().1);
            crate::paint::paint_stroke_rgba(
                &mut buf,
                w,
                h,
                &[(30.0, 150.0), (170.0, 150.0)],
                &crate::paint::BrushParams {
                    radius: 8.0,
                    color: [255, 255, 0],
                    opacity: 1.0,
                    mode: crate::paint::StrokeMode::Paint,
                },
            );
            doc.set_source_image(
                target,
                image::DynamicImage::ImageRgba8(
                    image::RgbaImage::from_raw(w, h, buf).expect("dimensions conservées"),
                ),
            );
            doc
        }),
        ("opacite", {
            let fond = solid(200, 200, [200, 40, 40, 255]);
            let voile = solid(200, 200, [40, 40, 200, 255]);
            doc_of(
                vec![
                    pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
                    pixel_node(&voile, 50.0, BlendMode::Normal, 0.0, 0.0),
                ],
                200,
                200,
            )
        }),
        ("fusion", {
            let fond = solid(200, 200, [200, 40, 40, 255]);
            let carre = solid(80, 80, [40, 200, 40, 255]);
            doc_of(
                vec![
                    pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
                    pixel_node(&carre, 100.0, BlendMode::Multiply, 60.0, 40.0),
                ],
                200,
                200,
            )
        }),
        ("groupe", {
            let fond = solid(200, 200, [200, 40, 40, 255]);
            let a = solid(80, 80, [40, 200, 40, 255]);
            let groupe = LayerNode::Group(GroupLayer::new(
                "g",
                vec![pixel_node(&a, 100.0, BlendMode::Normal, 60.0, 40.0)],
            ));
            doc_of(
                vec![
                    pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
                    groupe,
                ],
                200,
                200,
            )
        }),
        ("masque", {
            let fond = solid(200, 200, [200, 40, 40, 255]);
            doc_of(
                vec![
                    pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
                    masked_layer(),
                ],
                200,
                200,
            )
        }),
        ("transformation", {
            let fond = solid(200, 200, [200, 40, 40, 255]);
            let petit = solid(40, 40, [40, 200, 40, 255]);
            let mut l = PixelLayer::new("zoom", arc(&petit));
            l.transform.scale_x = 2.0;
            l.transform.scale_y = 2.0;
            l.transform.offset_x = 60.0;
            l.transform.offset_y = 40.0;
            doc_of(
                vec![
                    pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
                    LayerNode::Pixel(l),
                ],
                200,
                200,
            )
        }),
        ("ajustement-local", {
            let mut doc = doc_windowed();
            doc.root.push(adj_layer(vec![bc_node(20.0, 10.0)]));
            doc
        }),
        ("flou", {
            let mut doc = doc_windowed();
            doc.root.push(adj_layer(vec![adj_blur_node(3.0)]));
            doc
        }),
        ("multi-flou", {
            let mut doc = doc_windowed();
            doc.root
                .push(adj_layer(vec![adj_blur_node(2.0), adj_blur_node(3.0)]));
            doc
        }),
        ("global", {
            let mut doc = doc_windowed();
            doc.root
                .push(adj_layer(vec![FilterNode::new("futur_effet")]));
            doc
        }),
    ];
    for (name, doc) in &scenarios {
        let mut worker = worker_4mo(100);
        let (view, stats, _) = render_doc_viewport(&mut worker, doc);
        assert_eq!(stats.requested_tiles, 4, "{name} : 4 tuiles");
        assert_eq!(stats.rendered_tiles, 4, "{name} : froid complet");
        assert_view_matches_full(doc, &view.expect("contribue"));
        let (_, stats, _) = render_doc_viewport(&mut worker, doc);
        assert_eq!(stats.rendered_tiles, 0, "{name} : chaud sans rendu");
        assert_eq!(stats.cache_hits, 4, "{name} : 4 hits");
    }
}

#[test]
fn worker_bornes_partielles_et_hors_doc() {
    // §21 : viewport partiel clippé == découpe ; hors doc ⇒ None.
    let doc = doc_windowed();
    let mut worker = worker_4mo(100);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(-50, -50, 150, 150), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("chevauche");
    assert_eq!((view.image.width(), view.image.height()), (100, 100));
    assert_eq!((view.origin_x, view.origin_y), (0, 0));
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), 0, 0, 100, 100)
        .to_image()
        .into_raw();
    assert_eq!(view.image.to_rgba8().into_raw(), cropped);
    let mut stats = RenderWorkerStats::default();
    assert!(
        worker
            .render(
                &doc,
                &resolver,
                &mut render_stats,
                scope,
                &RenderRequest::new(TileRegion::new(600, 600, 10, 10), 1.0),
                &RenderPriorityContext::idle(),
                &mut stats,
            )
            .is_none(),
        "hors scope ⇒ None"
    );
}

#[test]
fn worker_eviction_reste_correct() {
    // §21 : budget de 2,5 tuiles sur 9 — évictions inévitables (le balayage
    // cyclique bat tout cache plus petit que le jeu de travail : 0 hit
    // attendu en seconde passe), pixels toujours justes, budget tenu.
    let doc = doc_384();
    let mut worker = RenderWorker::new((128 * 128 * 5) / 2, 128);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.requested_tiles, 9);
    assert!(
        worker.cache().stats().evictions > 0,
        "budget dépassé ⇒ évictions"
    );
    assert!(
        worker.cache().cached_pixels() <= worker.cache().max_pixels(),
        "budget tenu"
    );
    assert_view_matches_full(&doc, &view.expect("contribue"));
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 9, "tout évincé ⇒ tout recalculé");
    assert_view_matches_full(&doc, &view.expect("contribue"));
    assert!(
        worker.cache().cached_pixels() <= worker.cache().max_pixels(),
        "budget tenu après 2 passes"
    );
}

#[test]
fn worker_changement_scope_invalide() {
    // Débordement nouveau ⇒ périmètre suivi ⇒ invalidation totale, pixels justes.
    let mut doc = doc_windowed();
    let mut worker = worker_4mo(100);
    {
        let resolver = |id: Uuid| doc.appearance_image(id);
        let scope = worker.scope_for(&doc, &resolver);
        let mut render_stats = CompositeStats::default();
        let mut stats = RenderWorkerStats::default();
        let viewport = TileRegion::new(0, 0, doc.width, doc.height);
        worker
            .render(
                &doc,
                &resolver,
                &mut render_stats,
                scope,
                &RenderRequest::new(viewport, 1.0),
                &RenderPriorityContext::idle(),
                &mut stats,
            )
            .expect("passe 1");
        assert_eq!(stats.rendered_tiles, 4);
    }
    // Le carré sort du document : le scope grandit.
    let id = doc.root[1].id();
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.transform.offset_x = 500.0;
    }
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope2 = worker.scope_for(&doc, &resolver);
    assert!(scope2.w > doc.width, "le scope a grandi au-delà du doc");
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope2,
            &RenderWorker::scope_request(scope2, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("passe 2");
    assert_eq!(
        stats.rendered_tiles, stats.requested_tiles,
        "tout réévalué après changement de scope"
    );
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    assert_eq!(
        (view.image.width(), view.image.height()),
        (full.width(), full.height())
    );
    assert_eq!(
        view.image.to_rgba8().into_raw(),
        full.to_rgba8().into_raw(),
        "assemblage scope == pleine cadre"
    );
}

#[test]
fn worker_incremental_vs_full_baseline() {
    // §18 : 512², tuiles 256 — pleine cadre vs froid vs chaud, en scope_px.
    let fond = solid(512, 512, [200, 40, 40, 255]);
    let carre = solid(200, 200, [40, 200, 40, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 60.0, BlendMode::Normal, 100.0, 50.0),
        ],
        512,
        512,
    );
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats_full = CompositeStats::default();
    doc.composite_preview_with_stats(&resolver, &mut stats_full)
        .expect("pleine cadre");
    assert_eq!(stats_full.scope_px, 512 * 512);
    let mut worker = RenderWorker::new(1 << 24, 256);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let viewport = TileRegion::new(0, 0, 512, 512);
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("froid");
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (4, 4));
    assert_eq!(
        stats.dependency_px, stats_full.scope_px,
        "froid : même travail total (local pur, sans halo)"
    );
    assert_view_matches_full(&doc, &view);
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("chaud");
    assert_eq!((stats.rendered_tiles, stats.cache_hits), (0, 4));
    assert_eq!(render_stats.scope_px, 0, "chaud : 0 pixel retraité");
    assert_view_matches_full(&doc, &view);
}

#[test]
fn worker_viewport_1x1_et_decale() {
    // §19 : viewport d'une tuile et viewport non aligné sur la grille —
    // octets == découpe pleine cadre dans les deux cas.
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    for (name, vx, vy, vw, vh) in [("1x1", 128, 128, 128, 128), ("decale", 64, 64, 256, 256)] {
        let mut render_stats = CompositeStats::default();
        let mut stats = RenderWorkerStats::default();
        let view = worker
            .render(
                &doc,
                &resolver,
                &mut render_stats,
                scope,
                &RenderRequest::new(TileRegion::new(vx, vy, vw, vh), 1.0),
                &RenderPriorityContext::idle(),
                &mut stats,
            )
            .expect("viewport");
        assert_eq!(
            (view.origin_x, view.origin_y),
            (vx as u32, vy as u32),
            "{name}"
        );
        assert_eq!(
            (view.image.width(), view.image.height()),
            (vw, vh),
            "{name}"
        );
        let cropped = image::imageops::crop_imm(&full.to_rgba8(), vx as u32, vy as u32, vw, vh)
            .to_image()
            .into_raw();
        assert_eq!(view.image.to_rgba8().into_raw(), cropped, "{name}");
    }
}

#[test]
fn worker_zoom_cle_scale() {
    // §14 : même viewport, échelles 1.0 vs 2.0 ⇒ miss (clés distinctes),
    // mêmes octets (rendu 1.0), retour 1.0 ⇒ hit (conservé).
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let viewport = TileRegion::new(0, 0, 256, 256);
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    let v1 = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("échelle 1.0");
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (4, 4));
    let mut stats = RenderWorkerStats::default();
    let v2 = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(viewport, 2.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("échelle 2.0");
    assert_eq!(stats.rendered_tiles, 4, "échelle distincte ⇒ miss");
    assert_eq!(
        v1.image.to_rgba8().into_raw(),
        v2.image.to_rgba8().into_raw(),
        "rendu 1.0 dans les deux cas (pas de mipmaps 6F)"
    );
    let mut stats = RenderWorkerStats::default();
    let v1b = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("retour 1.0");
    assert_eq!((stats.rendered_tiles, stats.cache_hits), (0, 4));
    assert_eq!(
        v1b.image.to_rgba8().into_raw(),
        v1.image.to_rgba8().into_raw()
    );
}

#[test]
fn worker_petit_pan_conserve_tuiles() {
    // §12 : pan de 32 px sur tuiles 256 ⇒ même tuile ⇒ 0 rendu.
    let doc = doc_384();
    let mut worker = worker_4mo(256);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(0, 0, 128, 128), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("A");
    assert_eq!(stats.rendered_tiles, 1);
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(32, 0, 128, 128), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("B");
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (1, 0));
    assert_eq!(stats.cache_hits, 1, "tuile conservée");
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), 32, 0, 128, 128)
        .to_image()
        .into_raw();
    assert_eq!(view.image.to_rgba8().into_raw(), cropped);
}

#[test]
fn worker_meme_tuile_plusieurs_viewports() {
    // §4 : la tuile (0,0) demandée via deux viewports ⇒ 1 rendu, hits ensuite.
    let doc = doc_384();
    let mut worker = worker_4mo(256);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let mut render_stats = CompositeStats::default();
    let mut stats = RenderWorkerStats::default();
    worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(0, 0, 256, 256), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("A");
    assert_eq!(stats.rendered_tiles, 1);
    let mut stats = RenderWorkerStats::default();
    worker
        .render(
            &doc,
            &resolver,
            &mut render_stats,
            scope,
            &RenderRequest::new(TileRegion::new(0, 0, 128, 128), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("B");
    // B tient dans la tuile (0,0) : le worker ne demande que des rects
    // canoniques ⇒ même clé ⇒ hit.
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (1, 0));
    assert_eq!(stats.cache_hits, 1, "canonicité : même tuile, même clé");
}

#[test]
fn worker_ordre_plan_deterministe_survie_lru() {
    // §18 : budget 1 tuile — la survivante est la DERNIÈRE du plan (la moins
    // prioritaire), prouvant l'ordre d'exécution effectif.
    use super::scheduler::schedule_viewport;
    let doc = doc_windowed();
    let mut worker = RenderWorker::new(100 * 100, 100);
    let request = RenderRequest::new(TileRegion::new(0, 0, 200, 200), 1.0);
    let idle = RenderPriorityContext::idle();
    let plan = schedule_viewport(&request, worker.cache().dirty(), 100, &idle);
    assert_eq!(plan.len(), 4);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let last = plan.last().expect("plan");
    worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &request,
            &idle,
            &mut RenderWorkerStats::default(),
        )
        .expect("rendu");
    // Seule la dernière du plan survit au budget.
    let mut survivors = 0;
    for item in &plan {
        let key = super::tile_cache::DocTileKey::new(
            item.rect,
            crate::tile_key::BackendTag::cpu(),
            0,
            crate::tile_key::TILE_FLAGS_NONE,
            1.0,
        );
        if worker.cache().cached_content(key).is_some() {
            survivors += 1;
        }
    }
    assert_eq!(survivors, 1, "budget 1 tuile");
    let key = super::tile_cache::DocTileKey::new(
        last.rect,
        crate::tile_key::BackendTag::cpu(),
        0,
        crate::tile_key::TILE_FLAGS_NONE,
        1.0,
    );
    assert!(
        worker.cache().cached_content(key).is_some(),
        "la moins prioritaire (dernière) survit"
    );
}

#[test]
fn worker_camera_viewport_de_derive() {
    // §1 : viewport dérivé d'une vraie caméra ⇒ rendu == découpe.
    use super::scheduler::Camera;
    let doc = doc_windowed();
    let mut worker = worker_4mo(100);
    // Écran 200×200, zoom 2, pan pour cadrer l'origine : viewport (0,0,100,100).
    let cam = Camera::new(2.0, -100.0, -100.0);
    let viewport = cam.document_viewport(200.0, 200.0);
    assert_eq!(viewport, TileRegion::new(0, 0, 100, 100));
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("viewport caméra");
    assert_eq!(stats.requested_tiles, 1);
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), 0, 0, 100, 100)
        .to_image()
        .into_raw();
    assert_eq!(view.image.to_rgba8().into_raw(), cropped);
}

#[test]
fn surface_init_warm_partiel() {
    // §18 : init (full explicite) → warm (0 écriture) → partiel (1 tuile).
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (9, 9));
    let surf = worker.surface_stats();
    assert_eq!(surf.full_updates, 1, "une allocation initiale");
    assert_eq!(surf.tiles_applied, 9);
    assert_eq!(surf.pixels_written, 384 * 384, "surface complète écrite");
    assert_view_matches_full(&doc, &view.expect("contribue"));
    // Warm : mêmes octets, zéro écriture surface, zéro rendu.
    let written = worker.surface_stats().pixels_written;
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!((stats.rendered_tiles, stats.cache_hits), (0, 9));
    assert_eq!(
        worker.surface_stats().pixels_written,
        written,
        "hits n'écrivent rien"
    );
    assert_view_matches_full(&doc, &view.expect("hit"));
    // Petite zone sale simulée : 1 tuile réécrite, le reste intact
    // (la mutation réelle est couverte par local_mutation).
    worker.cache_mut().mark_dirty(TileRegion::new(0, 0, 32, 32));
    let written = worker.surface_stats().pixels_written;
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 1);
    assert_eq!(stats.cache_hits, 8);
    assert_eq!(
        worker.surface_stats().pixels_written - written,
        128 * 128,
        "une seule tuile réécrite"
    );
    assert_view_matches_full(&doc, &view.expect("contribue"));
}

#[test]
fn surface_local_mutation_chiffres() {
    // §21 : paint dans T4 → rendered 1, applied 1, written ≈ T4, reste intact.
    let mut doc = doc_384();
    let mut worker = worker_4mo(128);
    let (_, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 9);
    let written_full = worker.surface_stats().pixels_written;
    assert_eq!(written_full, 384 * 384);
    // Peinture réelle au centre de la tuile (1,1) : doc 384, tuile 128.
    let target = doc.root[0].id();
    let points = vec![(150.0, 150.0), (170.0, 170.0)];
    let layer = doc.pixel_layer(target).expect("fond");
    let mut buf = layer.source_image.to_rgba8().into_raw();
    crate::paint::paint_stroke_rgba(
        &mut buf,
        384,
        384,
        &points,
        &crate::paint::BrushParams {
            radius: 6.0,
            color: [255, 255, 0],
            opacity: 1.0,
            mode: crate::paint::StrokeMode::Paint,
        },
    );
    doc.set_source_image(
        target,
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(384, 384, buf).expect("dimensions conservées"),
        ),
    );
    let dirty = crate::tiles::stroke_dirty_region(&points, 6.0, tiles::Padding::ZERO)
        .expect("geste valide");
    worker.cache_mut().mark_dirty(dirty);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 1, "T4 seule");
    assert_eq!(stats.cache_hits, 8);
    let surf = worker.surface_stats();
    assert_eq!(surf.tiles_applied, 9 + 1, "9 init + 1 partiel");
    assert_eq!(surf.pixels_written - written_full, 128 * 128, "≈ aire T4");
    assert_eq!(surf.full_updates, 1, "aucun rebuild complet");
    assert_view_matches_full(&doc, &view.expect("contribue"));
}

#[test]
fn surface_multi_tile_update() {
    // §18 : trait à cheval sur 2 tuiles ⇒ 2 écritures, reste intact.
    let mut doc = doc_384();
    let mut worker = worker_4mo(128);
    let (_, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 9);
    let written = worker.surface_stats().pixels_written;
    let target = doc.root[0].id();
    // Trait vertical x=128 : chevauche les tuiles (0,*) et (1,*).
    let points = vec![(128.0, 10.0), (128.0, 370.0)];
    let layer = doc.pixel_layer(target).expect("fond");
    let mut buf = layer.source_image.to_rgba8().into_raw();
    crate::paint::paint_stroke_rgba(
        &mut buf,
        384,
        384,
        &points,
        &crate::paint::BrushParams {
            radius: 4.0,
            color: [0, 0, 255],
            opacity: 1.0,
            mode: crate::paint::StrokeMode::Paint,
        },
    );
    doc.set_source_image(
        target,
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(384, 384, buf).expect("dimensions conservées"),
        ),
    );
    let dirty = crate::tiles::stroke_dirty_region(&points, 4.0, tiles::Padding::ZERO)
        .expect("geste valide");
    worker.cache_mut().mark_dirty(dirty);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 6, "2 colonnes × 3 lignes");
    assert_eq!(stats.cache_hits, 3);
    assert_eq!(
        worker.surface_stats().pixels_written - written,
        6 * 128 * 128
    );
    assert_view_matches_full(&doc, &view.expect("contribue"));
}

#[test]
fn surface_rebuilt_from_cache_zero_compositing() {
    // §20 : reset puis repeuplement depuis le cache — 0 compositing.
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
    assert_eq!(stats.rendered_tiles, 9);
    let reference = view.expect("init").image.to_rgba8().into_raw();
    let misses_avant = worker.cache().stats().misses;
    worker.reset_surface();
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let repop = worker
        .repopulate_from_cache(scope, scope.rect(), 1.0)
        .expect("cache plein");
    assert_eq!(
        worker.cache().stats().misses - misses_avant,
        0,
        "zéro compositing"
    );
    // Compteurs par surface (pas lifetime) : la surface recréée compte sa
    // propre création (1) + les 9 tuiles repeuplées, sans aucun rendu.
    assert_eq!(worker.surface_stats().full_updates, 1, "surface recréée");
    assert_eq!(worker.surface_stats().tiles_applied, 9);
    assert_eq!(worker.surface_stats().pixels_written, 384 * 384);
    assert_eq!(
        repop.image.to_rgba8().into_raw(),
        reference,
        "repeuplée == rendue"
    );
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    assert_eq!(
        repop.image.to_rgba8().into_raw(),
        full.to_rgba8().into_raw(),
        "repeuplée == full"
    );
}

#[test]
fn surface_resize_recree() {
    // §9 : changement de périmètre ⇒ surface neuve, ancien contenu jamais réutilisé.
    let mut doc = doc_windowed();
    let mut worker = worker_4mo(100);
    {
        let resolver = |id: Uuid| doc.appearance_image(id);
        let scope = worker.scope_for(&doc, &resolver);
        let mut stats = RenderWorkerStats::default();
        worker
            .render(
                &doc,
                &resolver,
                &mut CompositeStats::default(),
                scope,
                &RenderRequest::new(TileRegion::new(0, 0, 200, 200), 1.0),
                &RenderPriorityContext::idle(),
                &mut stats,
            )
            .expect("init");
    }
    assert_eq!(worker.surface_stats().full_updates, 1);
    let id = doc.root[1].id();
    if let Some(LayerNode::Pixel(l)) = doc.find_mut(id) {
        l.transform.offset_x = 500.0;
    }
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope2 = worker.scope_for(&doc, &resolver);
    assert!(scope2.w > doc.width, "périmètre grandi au-delà du doc");
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope2,
            &RenderWorker::scope_request(scope2, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("resize");
    assert_eq!(worker.surface_stats().full_updates, 2, "surface recréée");
    assert_eq!(
        stats.rendered_tiles, stats.requested_tiles,
        "tout réévalué (cache invalidé par le scope)"
    );
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    assert_eq!(view.image.to_rgba8().into_raw(), full.to_rgba8().into_raw());
}

#[test]
fn surface_blur_et_global() {
    // §18 : flou + global — surface exacte dans les deux cas.
    for (name, filters) in [
        ("blur", vec![adj_blur_node(3.0)]),
        ("global", vec![FilterNode::new("futur_effet")]),
    ] {
        let mut doc = doc_windowed();
        doc.root.push(adj_layer(filters));
        let mut worker = worker_4mo(100);
        let (view, _, _) = render_doc_viewport(&mut worker, &doc);
        assert_view_matches_full(&doc, &view.expect("contribue"));
        let written = worker.surface_stats().pixels_written;
        // Seconde passe : hits ⇒ 0 écriture même avec halo/global.
        let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
        assert_eq!((stats.rendered_tiles, stats.cache_hits), (0, 4), "{name}");
        assert_eq!(
            worker.surface_stats().pixels_written,
            written,
            "{name} : warm sans écriture"
        );
        assert_view_matches_full(&doc, &view.expect("hit"));
    }
}

#[test]
fn surface_zoom_state_only() {
    // §19 : le zoom d'affichage n'atteint jamais le moteur — deux frames
    // identiques (simulant zoom 1.0 → 2.0 → 0.5 côté canvas) : 0 rendu,
    // 0 écriture, mêmes stats, même surface.
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let (view, _, _) = render_doc_viewport(&mut worker, &doc);
    let reference = view.expect("init").image.to_rgba8().into_raw();
    let written = worker.surface_stats().pixels_written;
    let misses_avant = worker.cache().stats().misses;
    for _ in ["zoom 1.0", "zoom 2.0", "zoom 0.5"] {
        // Aucune commande moteur : le canvas ne fait que re-désigner la même
        // requête (même viewport document, même échelle de rendu).
        let (view, stats, _) = render_doc_viewport(&mut worker, &doc);
        assert_eq!((stats.rendered_tiles, stats.cache_hits), (0, 9));
        assert_eq!(view.expect("hit").image.to_rgba8().into_raw(), reference);
    }
    assert_eq!(worker.surface_stats().pixels_written, written, "0 écriture");
    assert_eq!(
        worker.cache().stats().misses,
        misses_avant,
        "0 compositing : le zoom n'atteint pas le moteur"
    );
}

#[test]
fn surface_render_scale_change() {
    // §10/§18 : échelle de rendu 2.0 ⇒ miss (clés distinctes), mêmes octets
    // (rendu 1.0, pas de mipmaps) ; zoom d'affichage hors sujet (cf. zoom test).
    let doc = doc_384();
    let mut worker = worker_4mo(128);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let viewport = TileRegion::new(0, 0, 256, 256);
    let mut stats = RenderWorkerStats::default();
    let v1 = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("1.0");
    let written = worker.surface_stats().pixels_written;
    let mut stats = RenderWorkerStats::default();
    let v2 = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &RenderRequest::new(viewport, 2.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("2.0");
    assert_eq!(stats.rendered_tiles, 4, "échelle distincte ⇒ réévalué");
    assert_eq!(
        v1.image.to_rgba8().into_raw(),
        v2.image.to_rgba8().into_raw()
    );
    assert!(
        worker.surface_stats().pixels_written > written,
        "surface mise à jour pour la nouvelle échelle"
    );
}

#[test]
fn surface_viewport_partiel_hors_doc() {
    // §13 : viewport à cheval sur le bord ⇒ clip correct, == découpe.
    let doc = doc_windowed();
    let mut worker = worker_4mo(100);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let full = doc
        .composite_preview_with_stats(&resolver, &mut CompositeStats::default())
        .expect("pleine cadre");
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &RenderRequest::new(TileRegion::new(150, 150, 100, 100), 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("partiel");
    assert_eq!((view.image.width(), view.image.height()), (50, 50));
    assert_eq!((view.origin_x, view.origin_y), (150, 150));
    let cropped = image::imageops::crop_imm(&full.to_rgba8(), 150, 150, 50, 50)
        .to_image()
        .into_raw();
    assert_eq!(view.image.to_rgba8().into_raw(), cropped);
}

#[test]
fn mesures_surface_full_vs_partiel() {
    // §23 : 512², tuiles 256 — écriture surface pleine vs 1 tuile.
    let fond = solid(512, 512, [200, 40, 40, 255]);
    let carre = solid(200, 200, [40, 200, 40, 255]);
    let doc = doc_of(
        vec![
            pixel_node(&fond, 100.0, BlendMode::Normal, 0.0, 0.0),
            pixel_node(&carre, 60.0, BlendMode::Normal, 100.0, 50.0),
        ],
        512,
        512,
    );
    let mut worker = RenderWorker::new(1 << 24, 256);
    let scope = ScopeGeom::document(doc.width, doc.height);
    let viewport = TileRegion::new(0, 0, 512, 512);
    let resolver = |id: Uuid| doc.appearance_image(id);
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("froid");
    assert_eq!((stats.requested_tiles, stats.rendered_tiles), (4, 4));
    let surf = worker.surface_stats();
    assert_eq!(surf.full_updates, 1);
    assert_eq!(surf.pixels_written, 512 * 512, "pleine surface écrite");
    assert_view_matches_full(&doc, &view);
    // 1 tuile sale : seule elle est réécrite.
    worker
        .cache_mut()
        .mark_dirty(TileRegion::new(0, 0, 256, 256));
    let mut stats = RenderWorkerStats::default();
    let view = worker
        .render(
            &doc,
            &resolver,
            &mut CompositeStats::default(),
            scope,
            &RenderRequest::new(viewport, 1.0),
            &RenderPriorityContext::idle(),
            &mut stats,
        )
        .expect("partiel");
    assert_eq!((stats.rendered_tiles, stats.cache_hits), (1, 3));
    let surf = worker.surface_stats();
    assert_eq!(surf.full_updates, 1, "aucun rebuild complet");
    assert_eq!(
        surf.pixels_written,
        512 * 512 + 256 * 256,
        "seule la tuile sale réécrite"
    );
    assert_view_matches_full(&doc, &view);
}

#[test]
fn dirty_tile_rects_via_worker() {
    // §4 : découverte dirty → tuiles, forme en L (T5 T6 / T9 sur 4×4 fictif
    // ramené à 512² / 256 : sale x[300,400)×y[100,300) ⇒ (1,0),(1,1)).
    let doc = doc_384();
    let mut worker = worker_4mo(VIEWPORT_TILE_PX);
    assert!(
        dirty_tile_rects(
            worker.cache().dirty(),
            doc.width,
            doc.height,
            worker.tile_px()
        )
        .is_empty()
    );
    worker
        .cache_mut()
        .mark_dirty(TileRegion::new(300, 100, 100, 200));
    let tiles = dirty_tile_rects(
        worker.cache().dirty(),
        doc.width,
        doc.height,
        worker.tile_px(),
    );
    assert_eq!(tiles.len(), 2);
    // node_footprint / appearance_spread exposés au même grain (fumée).
    let id = doc.root[1].id();
    let node = doc.find(id).expect("carré");
    assert!(!node_footprint(node).is_empty());
    assert_eq!(appearance_spread(&[]), tiles::Padding::ZERO);
}
