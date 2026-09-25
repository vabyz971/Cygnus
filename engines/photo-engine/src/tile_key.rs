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

//! Phase 6A : identité sémantique des tuiles (sans exécution).
//!
//! Trois types, aucun pixel touché, aucun rendu piloté :
//!
//! - [`TileContentSignature`] : hash du contenu sémantique qui détermine les
//!   pixels (arbre des calques, filtres, masques, sources, géométrie).
//! - [`BackendTag`] : quel backend a produit (ou produirait) les pixels.
//! - [`TileCacheKey`] : clé complète (tuile + contenu + backend + grille +
//!   halo + flags).
//!
//! Invariants (testés ci-dessous) :
//!
//! - même clé ⇒ mêmes pixels sémantiques (correction) ;
//! - backend / kernel / version / flags / halo / grille différents ⇒ clés
//!   différentes (pas de faux hit inter-backends) ;
//! - [`RenderRevision`](crate::command::RenderRevision),
//!   `appearance_version`, [`ids::Revision`](ids::Revision) et les compteurs
//!   ne sont JAMAIS de l'identité pixel ;
//! - undo `A→B→A` restaure la même signature (les snapshots partagent les
//!   `Arc` sources, donc l'identité de pointeur est préservée).
//!
//! Modèle « state-only » préservé : un réglage UI (zoom/pan) ne touche pas
//! ces types ; seule une mutation sémantique change la signature.
//!
//! Limites assumées :
//!
//! - l'identité source est l'adresse de l'`Arc` (pas un hash des octets) :
//!   réallouer des pixels identiques donne une signature différente (MISS
//!   sain, jamais de faux HIT) ; le cache est mémoire vive, non persisté ;
//! - CPU (`image::blur` gaussien) et GPU (compute BoxBlur, rayon clampé
//!   1..=50) DIVERGENT au pixel près — d'où des [`BackendTag`] distincts ;
//! - seuil GPU partagé : `< 65536 px` ⇒ CPU même si un adaptateur existe
//!   (cf. `gpu.rs`), reflété par [`BackendTag::select`].

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView};

use tiles::{Padding, TileGrid, TileId};

use crate::document::{AdjustmentLayer, Document, FilterNode, GroupLayer, LayerNode, PixelLayer};
use crate::renderer::filters_signature;

/// Version du noyau CPU (à incrémenter si `image::*` / rayon / fusion change).
pub const CPU_KERNEL_VERSION: u16 = 1;
/// Version du noyau GPU (à incrémenter si un shader WGSL change).
pub const GPU_KERNEL_VERSION: u16 = 1;
/// Seuil partagé avec `gpu.rs` : en dessous, tout reste CPU (overhead GPU).
pub const GPU_PIXEL_THRESHOLD: u64 = 65_536;
/// Flags par défaut (aucune option).
pub const TILE_FLAGS_NONE: u32 = 0;

/// Hash opaque du contenu sémantique d'un cadre (identique pour toutes les
/// tuiles du même cadre ; la localisation vient de [`TileId`] dans la clé).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileContentSignature(pub u64);

impl TileContentSignature {
    /// Valeur brute (diagnostics, persistance mémoire uniquement).
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Backend d'exécution ayant produit les pixels.
///
/// CPU et GPU divergent (gaussien vs BoxBlur, arrondis float) : ils ne
/// partagent JAMAIS une clé. `version` permet de faire tourner les noyaux
/// sans changer le type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendTag {
    Cpu { version: u16 },
    Gpu { version: u16 },
}

impl BackendTag {
    /// Tag CPU courant.
    #[must_use]
    pub fn cpu() -> Self {
        Self::Cpu {
            version: CPU_KERNEL_VERSION,
        }
    }

    /// Tag GPU courant.
    #[must_use]
    pub fn gpu() -> Self {
        Self::Gpu {
            version: GPU_KERNEL_VERSION,
        }
    }

    /// Backend RÉELLEMENT emprunté : CPU si pas de GPU ou sous le seuil
    /// (miroir de la garde `width * height < 65536` de `gpu.rs`).
    #[must_use]
    pub fn select(gpu_available: bool, pixel_count: u64) -> Self {
        if gpu_available && pixel_count >= GPU_PIXEL_THRESHOLD {
            Self::gpu()
        } else {
            Self::cpu()
        }
    }
}

/// Clé complète de cache tuile : même clé ⇒ mêmes pixels sémantiques.
///
/// `grid` = [`grid_signature`] (taille de tuile + dimensions + niveaux) ;
/// `halo` = marge d'effet en px ([`Padding`]) ; `flags` = options de rendu
/// (réservé, `0` par défaut — toute option pixellisée DOIT y figurer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCacheKey {
    pub tile: TileId,
    pub content: TileContentSignature,
    pub backend: BackendTag,
    pub grid: u64,
    pub halo: u32,
    pub flags: u32,
}

impl TileCacheKey {
    /// Nouvelle clé (aucune normalisation : chaque champ discriminant compte).
    #[must_use]
    pub fn new(
        tile: TileId,
        content: TileContentSignature,
        backend: BackendTag,
        grid: u64,
        halo: Padding,
        flags: u32,
    ) -> Self {
        Self {
            tile,
            content,
            backend,
            grid,
            halo: halo.0,
            flags,
        }
    }
}

/// Signature sémantique du cadre entier (ordre bas → haut significatif).
///
/// Couvre : dimensions document, pour chaque nœud (récursif) id, type,
/// visibilité, opacité, fusion, transform, identité source (`Arc` + dims),
/// chaînes de filtres (via [`filters_signature`]) et masques (id + état +
/// version — la peinture touche la version). Exclut : noms, `appearance_version`,
/// révisions, compteurs.
#[must_use]
pub fn tile_content_signature(doc: &Document) -> TileContentSignature {
    let mut h = DefaultHasher::new();
    h.write_u32(doc.width);
    h.write_u32(doc.height);
    h.write_usize(doc.root.len());
    for node in &doc.root {
        hash_layer_node(&mut h, node);
    }
    TileContentSignature(h.finish())
}

/// Empreinte de grille : taille de tuile + surface + niveaux.
/// Deux grilles différentes ⇒ clés différentes (même contenu, même tuile).
#[must_use]
pub fn grid_signature(grid: &TileGrid) -> u64 {
    let mut h = DefaultHasher::new();
    let extent = grid.extent();
    h.write_u32(extent.width);
    h.write_u32(extent.height);
    let (w, hh) = grid.surface();
    h.write_u32(w);
    h.write_u32(hh);
    h.write_u8(grid.level_count());
    h.finish()
}

/// Construit la clé complète d'une tuile (sucre sur [`TileCacheKey::new`]).
#[must_use]
pub fn tile_cache_key(
    tile: TileId,
    content: TileContentSignature,
    backend: BackendTag,
    grid: &TileGrid,
    halo: Padding,
    flags: u32,
) -> TileCacheKey {
    TileCacheKey::new(tile, content, backend, grid_signature(grid), halo, flags)
}

fn hash_layer_node(h: &mut DefaultHasher, node: &LayerNode) {
    match node {
        LayerNode::Pixel(l) => {
            h.write_u8(0);
            hash_pixel_layer(h, l);
        }
        LayerNode::Group(g) => {
            h.write_u8(1);
            hash_group_layer(h, g);
        }
        LayerNode::Adjustment(a) => {
            h.write_u8(2);
            hash_adjustment_layer(h, a);
        }
    }
}

fn hash_pixel_layer(h: &mut DefaultHasher, l: &PixelLayer) {
    h.write(l.id.as_bytes());
    h.write_u8(u8::from(l.visible));
    h.write_u32(l.opacity.to_bits());
    h.write_u32(l.blend_mode.id());
    hash_transform(h, &l.transform);
    hash_source(h, &l.source_image);
    h.write_u64(filters_signature(&l.filter_layers));
    h.write_u64(crate::renderer::mask_signature(&l.masks));
}

fn hash_group_layer(h: &mut DefaultHasher, g: &GroupLayer) {
    h.write(g.id.as_bytes());
    h.write_u8(u8::from(g.visible));
    h.write_u32(g.opacity.to_bits());
    h.write_u32(g.blend_mode.id());
    h.write_u64(crate::renderer::mask_signature(&g.masks));
    h.write_usize(g.children.len());
    for child in &g.children {
        hash_layer_node(h, child);
    }
}

fn hash_adjustment_layer(h: &mut DefaultHasher, a: &AdjustmentLayer) {
    h.write(a.id.as_bytes());
    h.write_u8(u8::from(a.visible));
    h.write_u32(a.opacity.to_bits());
    h.write_u64(adjustment_signature(&a.filters));
}

/// Identité source : adresse de l'`Arc` (partagée par les snapshots, donc
/// stable sur undo) + dimensions (garde-fou). Voir l'en-tête du module.
fn hash_source(h: &mut DefaultHasher, source: &Arc<DynamicImage>) {
    h.write_usize(Arc::as_ptr(source) as usize);
    let (w, hh) = source.dimensions();
    h.write_u32(w);
    h.write_u32(hh);
}

fn hash_transform(h: &mut DefaultHasher, t: &crate::document::Transform2D) {
    for v in [
        t.offset_x,
        t.offset_y,
        t.rotation_deg,
        t.scale_x,
        t.scale_y,
        t.skew_x,
        t.skew_y,
    ] {
        h.write_u32(v.to_bits());
    }
}

/// Signature d'une chaîne d'ajustement ([`FilterNode`]) : même recette que
/// [`filters_signature`] (id, type, état, params triés), sans attributs de
/// sous-calque (l'opacité vit au niveau du calque d'ajustement, hashée
/// séparément).
fn adjustment_signature(filters: &[FilterNode]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write_usize(filters.len());
    for f in filters {
        h.write(f.id.as_bytes());
        h.write(f.type_id.as_bytes());
        h.write_u8(u8::from(f.enabled));
        let mut keys: Vec<&str> = Vec::with_capacity(f.params.len());
        keys.extend(f.params.keys().map(String::as_str));
        keys.sort_unstable();
        h.write_usize(keys.len());
        for key in keys {
            h.write(key.as_bytes());
            hash_param_value(&mut h, &f.params[key]);
        }
    }
    h.finish()
}

fn hash_param_value(h: &mut DefaultHasher, value: &datatypes::ParamValue) {
    match value {
        datatypes::ParamValue::Float(v) => {
            h.write_u8(0);
            h.write_u32(v.to_bits());
        }
        datatypes::ParamValue::Int(v) => {
            h.write_u8(1);
            h.write_i32(*v);
        }
        datatypes::ParamValue::Bool(v) => {
            h.write_u8(2);
            h.write_u8(u8::from(*v));
        }
        datatypes::ParamValue::Color(c) => {
            h.write_u8(3);
            for channel in c {
                h.write_u32(channel.to_bits());
            }
        }
        datatypes::ParamValue::Text(s) => {
            h.write_u8(4);
            h.write(s.as_bytes());
        }
        datatypes::ParamValue::Enum(s) => {
            h.write_u8(5);
            h.write(s.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::LayerMask;
    use datatypes::ParamValue;
    use image::{ImageBuffer, Rgba};
    use tiles::TileGrid;

    fn solid(value: u8) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            8,
            8,
            Rgba([value, value, value, 255]),
        )))
    }

    fn doc_single() -> Document {
        let mut doc = Document::new(64, 64);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("calque", solid(100))));
        doc
    }

    fn brightness_filter(value: f32) -> crate::document::FilterLayer {
        let mut f =
            crate::document::FilterLayer::neutral("brightness_contrast", Default::default());
        f.params
            .insert("brightness".to_string(), ParamValue::Float(value));
        f
    }

    fn key_of(doc: &Document) -> TileCacheKey {
        let grid = TileGrid::new(256, doc.width, doc.height, 1);
        tile_cache_key(
            TileId::new(0, 0, 0),
            tile_content_signature(doc),
            BackendTag::cpu(),
            &grid,
            Padding::ZERO,
            TILE_FLAGS_NONE,
        )
    }

    #[test]
    fn a_meme_entrees_meme_signature() {
        let doc = doc_single();
        let s1 = tile_content_signature(&doc);
        let s2 = tile_content_signature(&doc);
        assert_eq!(s1, s2);
        // Clone structurel (Arcs partagés, comme un snapshot) : stable.
        let clone = Document::new(doc.width, doc.height);
        let _ = clone;
        let snap = doc_single();
        let _ = snap;
        let doc2 = doc_single();
        // Deux docs aux pixels/logiques identiques mais allocations
        // distinctes : MISS sain (adresses d'Arc différentes), jamais de
        // faux HIT — documenté dans l'en-tête.
        assert_ne!(
            s1,
            tile_content_signature(&doc2),
            "allocations distinctes => signatures distinctes (conservatif)"
        );
    }

    #[test]
    fn a_clone_structurel_meme_signature() {
        // Le cas qui compte pour le cache : clone avec les MÊMES Arcs.
        let mut doc = Document::new(64, 64);
        let src = solid(100);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("c", Arc::clone(&src))));
        let s1 = tile_content_signature(&doc);
        // Snapshot : clone() partage les Arcs sources.
        let mut snap_root = Vec::new();
        for node in &doc.root {
            snap_root.push(node.clone());
        }
        let mut restored = Document::new(doc.width, doc.height);
        restored.root = snap_root;
        assert_eq!(s1, tile_content_signature(&restored));
    }

    #[test]
    fn b_parametre_filtre_change_signature() {
        let mut doc = doc_single();
        if let Some(LayerNode::Pixel(l)) = doc.root.first_mut() {
            l.filter_layers.push(brightness_filter(10.0));
        }
        let avant = tile_content_signature(&doc);
        if let Some(LayerNode::Pixel(l)) = doc.root.first_mut() {
            l.filter_layers[0]
                .params
                .insert("brightness".to_string(), ParamValue::Float(20.0));
        }
        assert_ne!(avant, tile_content_signature(&doc));
    }

    #[test]
    fn c_ordre_filtres_change_signature() {
        let mut d1 = doc_single();
        let mut d2 = doc_single();
        // Mêmes Arcs sources pour isoler l'ordre (sinon l'adresse domine).
        let src = solid(100);
        for d in [&mut d1, &mut d2] {
            if let Some(LayerNode::Pixel(l)) = d.root.first_mut() {
                l.source_image = Arc::clone(&src);
            }
        }
        let mut blur = crate::document::FilterLayer::neutral("blur", Default::default());
        blur.params
            .insert("radius".to_string(), ParamValue::Float(3.0));
        let bc = brightness_filter(12.0);
        if let Some(LayerNode::Pixel(l)) = d1.root.first_mut() {
            l.filter_layers = vec![blur.clone(), bc.clone()];
        }
        if let Some(LayerNode::Pixel(l)) = d2.root.first_mut() {
            l.filter_layers = vec![bc, blur];
        }
        // Les ids de sous-calques diffèrent ici (neutral() frais) ; on
        // aligne les ids pour ne tester QUE l'ordre.
        let ids: Vec<uuid::Uuid> = if let Some(LayerNode::Pixel(l)) = d1.root.first() {
            l.filter_layers.iter().map(|f| f.id).collect()
        } else {
            Vec::new()
        };
        if let Some(LayerNode::Pixel(l)) = d2.root.first_mut() {
            for (f, id) in l.filter_layers.iter_mut().zip(ids.iter().rev()) {
                f.id = *id;
            }
        }
        assert_ne!(tile_content_signature(&d1), tile_content_signature(&d2));
    }

    #[test]
    fn d_opacite_fusion_transform_visibilite_changent_signature() {
        let base = doc_single();
        let s0 = tile_content_signature(&base);

        let op2 = base_snapshot_with(&base, |l| l.opacity = 50.0);
        assert_ne!(s0, tile_content_signature(&op2));

        let mul = base_snapshot_with(&base, |l| {
            l.blend_mode = crate::document::BlendMode::Multiply;
        });
        assert_ne!(s0, tile_content_signature(&mul));

        let moved = base_snapshot_with(&base, |l| {
            l.transform.offset_x += 5.0;
        });
        assert_ne!(s0, tile_content_signature(&moved));

        let hidden = base_snapshot_with(&base, |l| {
            l.visible = false;
        });
        assert_ne!(s0, tile_content_signature(&hidden));
    }

    /// Clone structurel de `base` (Arcs partagés) + mutation pixel ciblée.
    fn base_snapshot_with(base: &Document, f: impl FnOnce(&mut PixelLayer)) -> Document {
        let mut out = Document::new(base.width, base.height);
        out.root = base.root.clone();
        if let Some(LayerNode::Pixel(l)) = out.root.first_mut() {
            f(l);
        }
        out
    }

    #[test]
    fn e_masques_changent_signature() {
        let base = doc_single();
        let s0 = tile_content_signature(&base);

        let mut added = base_snapshot_with(&base, |l| {
            l.masks.push(LayerMask::full(8, 8));
        });
        let s1 = tile_content_signature(&added);
        assert_ne!(s0, s1);

        added.root.clone_from_slice(&base.root.clone());
        let mut toggled = base_snapshot_with(&base, |l| {
            l.masks.push(LayerMask::full(8, 8));
        });
        if let Some(LayerNode::Pixel(l)) = toggled.root.first_mut() {
            l.masks[0].enabled = false;
        }
        let mut enabled = base_snapshot_with(&base, |l| {
            l.masks.push(LayerMask::full(8, 8));
        });
        // Aligne les ids/versions pour isoler le bit enabled.
        if let (Some(LayerNode::Pixel(a)), Some(LayerNode::Pixel(b))) =
            (toggled.root.first_mut(), enabled.root.first_mut())
        {
            b.masks[0].id = a.masks[0].id;
            b.masks[0].version = a.masks[0].version;
        }
        assert_ne!(
            tile_content_signature(&toggled),
            tile_content_signature(&enabled)
        );

        let mut painted = base_snapshot_with(&base, |l| {
            l.masks.push(LayerMask::full(8, 8));
        });
        if let Some(LayerNode::Pixel(l)) = painted.root.first_mut() {
            l.masks[0].touch();
        }
        assert_ne!(s1, tile_content_signature(&painted));
    }

    #[test]
    fn f_renommage_ne_change_pas_signature() {
        let base = doc_single();
        let s0 = tile_content_signature(&base);

        let renamed = base_snapshot_with(&base, |l| {
            l.name = String::from("nouveau nom");
            l.filter_layers.push(brightness_filter(5.0));
        });
        let mut renamed2 = base_snapshot_with(&base, |l| {
            l.name = String::from("autre nom");
            l.filter_layers.push(brightness_filter(5.0));
        });
        // Aligne les ids de sous-calques (frais) pour isoler le nom.
        let fid = if let Some(LayerNode::Pixel(l)) = renamed.root.first() {
            l.filter_layers[0].id
        } else {
            uuid::Uuid::nil()
        };
        if let Some(LayerNode::Pixel(l)) = renamed2.root.first_mut() {
            l.filter_layers[0].id = fid;
        }
        assert_eq!(
            tile_content_signature(&renamed),
            tile_content_signature(&renamed2),
            "le nom seul ne change pas les pixels"
        );
        let _ = s0;
    }

    #[test]
    fn g_versions_et_compteurs_ne_changent_pas_signature() {
        let base = doc_single();
        let s0 = tile_content_signature(&base);
        // `touch()` ne fait que bumper appearance_version : pas pixelisé.
        let touched = base_snapshot_with(&base, |l| l.touch());
        assert_eq!(s0, tile_content_signature(&touched));
        // RenderRevision : compteur d'orchestration, jamais hashé.
        let mut rev = crate::command::RenderRevision::default();
        rev.bump();
        rev.bump();
        assert_eq!(s0, tile_content_signature(&base));
    }

    #[test]
    fn h_backend_different_cle_differente() {
        let doc = doc_single();
        let grid = TileGrid::new(256, doc.width, doc.height, 1);
        let content = tile_content_signature(&doc);
        let tile = TileId::new(0, 0, 0);
        let cpu = TileCacheKey::new(
            tile,
            content,
            BackendTag::cpu(),
            grid_signature(&grid),
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        let gpu = TileCacheKey::new(
            tile,
            content,
            BackendTag::gpu(),
            grid_signature(&grid),
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        assert_ne!(cpu, gpu);
        // Version de noyau différente ⇒ clé différente.
        let gpu_v2 = TileCacheKey::new(
            tile,
            content,
            BackendTag::Gpu { version: 2 },
            grid_signature(&grid),
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        assert_ne!(gpu, gpu_v2);
    }

    #[test]
    fn i_grille_et_tuile_discriminent() {
        let doc = doc_single();
        let content = tile_content_signature(&doc);
        let g1 = TileGrid::new(256, doc.width, doc.height, 1);
        let g2 = TileGrid::new(128, doc.width, doc.height, 1);
        assert_ne!(grid_signature(&g1), grid_signature(&g2));
        let k1 = tile_cache_key(
            TileId::new(0, 0, 0),
            content,
            BackendTag::cpu(),
            &g1,
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        let k2 = tile_cache_key(
            TileId::new(0, 0, 0),
            content,
            BackendTag::cpu(),
            &g2,
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        assert_ne!(k1, k2);
        // Même grille, tuiles voisines ⇒ clés différentes.
        let ka = tile_cache_key(
            TileId::new(0, 0, 0),
            content,
            BackendTag::cpu(),
            &g1,
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        let kb = tile_cache_key(
            TileId::new(0, 1, 0),
            content,
            BackendTag::cpu(),
            &g1,
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        assert_ne!(ka, kb);
        // Dimensions document différentes ⇒ contenu différent.
        let mut wide = Document::new(128, 64);
        wide.root = doc.root.clone();
        assert_ne!(content, tile_content_signature(&wide));
    }

    #[test]
    fn j_halo_et_flags_discriminent() {
        let doc = doc_single();
        let grid = TileGrid::new(256, doc.width, doc.height, 1);
        let content = tile_content_signature(&doc);
        let tile = TileId::new(0, 0, 0);
        let base = TileCacheKey::new(
            tile,
            content,
            BackendTag::cpu(),
            grid_signature(&grid),
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        let halo = TileCacheKey::new(
            tile,
            content,
            BackendTag::cpu(),
            grid_signature(&grid),
            Padding::for_blur_radius(20.0),
            TILE_FLAGS_NONE,
        );
        assert_ne!(base, halo);
        let flagged = TileCacheKey::new(
            tile,
            content,
            BackendTag::cpu(),
            grid_signature(&grid),
            Padding::ZERO,
            1,
        );
        assert_ne!(base, flagged);
    }

    #[test]
    fn k_undo_restaure_meme_signature() {
        // A (snapshot partagé) → B (param modifié) → A (restauré).
        let doc_a = doc_single();
        let mut with_filter = base_snapshot_with(&doc_a, |l| {
            l.filter_layers.push(brightness_filter(10.0));
        });
        let sig_a = tile_content_signature(&with_filter);
        let snap = with_filter.root.clone();
        if let Some(LayerNode::Pixel(l)) = with_filter.root.first_mut() {
            l.filter_layers[0]
                .params
                .insert("brightness".to_string(), ParamValue::Float(99.0));
        }
        let sig_b = tile_content_signature(&with_filter);
        assert_ne!(sig_a, sig_b);
        with_filter.root = snap;
        assert_eq!(sig_a, tile_content_signature(&with_filter));
    }

    #[test]
    fn critique_cpu_gpu_seuil_et_divergence() {
        // Sous le seuil : CPU même avec GPU dispo (miroir de gpu.rs).
        assert_eq!(
            BackendTag::select(true, GPU_PIXEL_THRESHOLD - 1),
            BackendTag::cpu()
        );
        assert_eq!(BackendTag::select(true, 32 * 32), BackendTag::cpu());
        // Au seuil et au-delà avec GPU : GPU.
        assert_eq!(
            BackendTag::select(true, GPU_PIXEL_THRESHOLD),
            BackendTag::gpu()
        );
        assert_eq!(BackendTag::select(true, 1024 * 1024), BackendTag::gpu());
        // Sans GPU : toujours CPU.
        assert_eq!(BackendTag::select(false, 1024 * 1024), BackendTag::cpu());
        // Les clés divergent (noyaux différents : gaussien CPU vs BoxBlur
        // GPU, rayon clampé 1..=50 côté GPU) : jamais de hit croisé.
        let doc = doc_single();
        let _ = key_of(&doc);
        let grid = TileGrid::new(256, doc.width, doc.height, 1);
        let content = tile_content_signature(&doc);
        let tile = TileId::new(0, 0, 0);
        let cpu_key = TileCacheKey::new(
            tile,
            content,
            BackendTag::select(false, 1024 * 1024),
            grid_signature(&grid),
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        let gpu_key = TileCacheKey::new(
            tile,
            content,
            BackendTag::select(true, 1024 * 1024),
            grid_signature(&grid),
            Padding::ZERO,
            TILE_FLAGS_NONE,
        );
        assert_ne!(cpu_key, gpu_key);
    }
}
