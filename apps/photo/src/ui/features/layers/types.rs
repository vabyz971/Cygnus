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

#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.
//! Types d'affichage des calques photo (vue UI dérivée du moteur).
//!
//! L'app convertit les `LayerNode` de `photo-engine` en
//! [`PhotoLayerInfo`] avant d'appeler les widgets. Seul le niveau
//! racine est exposé en Phase 3 (les enfants de groupes restent
//! repliés).

use photo_engine::{BlendMode, Document, LayerNode};
use ui_kit::icons::Icon;
use ui_kit::utils::DropPosition;
use uuid::Uuid;

/// Type sémantique d'un calque photo (miroir des `LayerNode` moteur).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoLayerKind {
    /// Calque pixels.
    Pixel,
    /// Groupe de calques.
    Group,
    /// Calque d'ajustement.
    Adjustment,
}

/// Toutes les variantes, pour les tests d'exhaustivité.
pub const ALL_PHOTO_LAYER_KINDS: &[PhotoLayerKind] = &[
    PhotoLayerKind::Pixel,
    PhotoLayerKind::Group,
    PhotoLayerKind::Adjustment,
];

impl PhotoLayerKind {
    /// Icône sémantique (via `Icon` de ui-kit uniquement).
    pub fn icon(self) -> Icon {
        match self {
            Self::Pixel => Icon::ImageIcon,
            Self::Group => Icon::Layers,
            Self::Adjustment => Icon::Settings,
        }
    }

    /// Libellé du type (français, ASCII).
    pub fn icon_label(self) -> &'static str {
        match self {
            Self::Pixel => "Pixels",
            Self::Group => "Groupe",
            Self::Adjustment => "Reglage",
        }
    }

    /// Dérive le type d'affichage d'un nœud moteur.
    pub fn from_node(node: &LayerNode) -> Self {
        match node {
            LayerNode::Pixel(_) => Self::Pixel,
            LayerNode::Group(_) => Self::Group,
            LayerNode::Adjustment(_) => Self::Adjustment,
        }
    }
}

/// Sous-calque affiché (filtre live ou masque) sous son porteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoSubLayerInfo {
    /// Identifiant moteur (filtre ou masque).
    pub id: Uuid,
    /// Nom affiché.
    pub name: String,
    /// Vrai = filtre live, faux = masque.
    pub is_filter: bool,
}

/// Miniature d'un calque pour le panneau (buffer pur, converti en
/// texture côté app et mis en cache par `appearance_version`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoLayerThumb {
    /// Largeur en pixels (environ 48).
    pub width: u32,
    /// Hauteur en pixels (environ 32, ratio préservé).
    pub height: u32,
    /// Pixels RGBA8 ligne par ligne.
    pub rgba: Vec<u8>,
    /// Version d'apparence : l'UI ne re-téléverse que si elle change.
    pub version: u64,
}

/// Miniature téléversée d'un calque, pour la ligne du panneau
/// (construite par l'app depuis son cache de textures).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerThumbView {
    /// Texture GPU de la miniature.
    pub texture_id: egui::TextureId,
    /// Dimensions de la miniature en pixels.
    pub size: egui::Vec2,
}

/// Données d'affichage d'un calque photo (vue UI, détenue par l'app).
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoLayerInfo {
    /// Identifiant moteur (`Uuid` de `photo-engine`).
    pub id: Uuid,
    /// Nom affiché.
    pub name: String,
    /// Type sémantique.
    pub kind: PhotoLayerKind,
    /// Visibilité (œil).
    pub visible: bool,
    /// Opacité (unités moteur 0..=100).
    pub opacity: f32,
    /// Mode de fusion du calque.
    pub blend_mode: BlendMode,
    /// Le calque porte des filtres live (badge FX).
    pub has_filters: bool,
    /// Le calque porte des masques (badge).
    pub has_masks: bool,
    /// Filtres live du calque (ordre d'application), pour la HUD.
    pub filters: Vec<PhotoSubLayerInfo>,
    /// Masques du calque, pour la HUD.
    pub masks: Vec<PhotoSubLayerInfo>,
    /// Enfants directs (groupes uniquement, récursif) : l'UI les
    /// affiche indentés sous leur parent, repliés si `collapsed`.
    /// N'importe quel calque peut être enfant d'un groupe (nesting
    /// libre côté moteur : `reorder_before` / `move_into`).
    pub children: Vec<PhotoLayerInfo>,
    /// Groupe replié (état moteur) : l'UI masque `children`.
    pub collapsed: bool,
    /// Miniature pixels (`None` = groupes/ajustements : icône de type).
    pub thumb: Option<PhotoLayerThumb>,
    /// Sélection courante (état UI).
    pub selected: bool,
    /// Feedback interactif affichable en overlay pendant un drag (Phase
    /// 6G.3P) : vrai ssi la transform du calque est plaçable à l'écran
    /// (identité/translation/échelle — calculé côté moteur, jamais deviné
    /// par l'UI). Faux pour rotation/skew (présentation désactivée, moteur
    /// et commit exacts quand même).
    pub overlay_live: bool,
    /// Transform du calque (identité pour groupes/ajustements) : placement
    /// écran de l'overlay (calque→document), translation/échelle seulement
    /// quand `overlay_live` est vrai.
    pub transform: photo_engine::Transform2D,
}

impl PhotoLayerInfo {
    /// Dérive une entrée d'affichage d'un nœud moteur.
    pub fn from_node(document: &Document, node: &LayerNode) -> Self {
        Self::from_node_with_lookup(node, &|id| document.thumb(id))
    }

    /// Dérive une entrée d'affichage avec une miniature déjà résolue
    /// (partage inter-passes : aucune nouvelle résolution d'apparence).
    pub fn from_node_with_thumb(node: &LayerNode, thumb: Option<photo_engine::RgbaBuf>) -> Self {
        let id = node.id();
        Self::from_node_with_lookup(node, &|query| {
            if query == id { thumb.clone() } else { None }
        })
    }

    /// Dérive une entrée d'affichage avec résolveur de miniatures
    /// (récursif : les enfants de groupes sont exposés indentés).
    pub fn from_node_with_lookup(
        node: &LayerNode,
        lookup: &dyn Fn(Uuid) -> Option<photo_engine::RgbaBuf>,
    ) -> Self {
        let (filters, masks) = match node {
            LayerNode::Pixel(pixels) => (
                pixels
                    .filter_layers
                    .iter()
                    .map(|f| PhotoSubLayerInfo {
                        id: f.id,
                        name: f.name.clone(),
                        is_filter: true,
                    })
                    .collect(),
                pixels
                    .masks
                    .iter()
                    .map(|m| PhotoSubLayerInfo {
                        id: m.id,
                        name: m.name.clone(),
                        is_filter: false,
                    })
                    .collect(),
            ),
            LayerNode::Group(group) => (
                Vec::new(),
                group
                    .masks
                    .iter()
                    .map(|m| PhotoSubLayerInfo {
                        id: m.id,
                        name: m.name.clone(),
                        is_filter: false,
                    })
                    .collect(),
            ),
            LayerNode::Adjustment(_) => (Vec::new(), Vec::new()),
        };
        let has_filters = !filters.is_empty();
        let has_masks = !masks.is_empty();
        // Miniature servie par le résolveur (zéro recalcul à chaud) :
        // seuls les pixels en ont une.
        let thumb = match node {
            LayerNode::Pixel(pixels) => lookup(node.id()).map(|buf| PhotoLayerThumb {
                width: buf.width,
                height: buf.height,
                rgba: buf.data.to_vec(),
                version: pixels.appearance_version,
            }),
            LayerNode::Group(_) | LayerNode::Adjustment(_) => None,
        };
        Self {
            id: node.id(),
            name: node.name().to_owned(),
            kind: PhotoLayerKind::from_node(node),
            visible: node.visible(),
            opacity: node.opacity(),
            blend_mode: node.blend_mode().unwrap_or(BlendMode::Normal),
            has_filters,
            has_masks,
            filters,
            masks,
            children: match node {
                LayerNode::Group(group) => group
                    .children
                    .iter()
                    .rev()
                    .map(|child| Self::from_node_with_lookup(child, lookup))
                    .collect(),
                LayerNode::Pixel(_) | LayerNode::Adjustment(_) => Vec::new(),
            },
            collapsed: match node {
                LayerNode::Group(group) => group.collapsed,
                LayerNode::Pixel(_) | LayerNode::Adjustment(_) => false,
            },
            thumb,
            selected: false,
            overlay_live: match node {
                LayerNode::Pixel(pixels) => {
                    photo_engine::interaction::overlay_displayable(&pixels.transform)
                }
                LayerNode::Group(_) | LayerNode::Adjustment(_) => false,
            },
            transform: match node {
                LayerNode::Pixel(pixels) => pixels.transform,
                LayerNode::Group(_) | LayerNode::Adjustment(_) => {
                    photo_engine::Transform2D::default()
                }
            },
        }
    }
}

/// Drop hiérarchique résolu en lignes : ce qui est déplacé, sur
/// quoi, et où (avant / dedans / après). Le panneau le convertit en
/// commandes worker (`ReorderNodes`, `MoveIntoGroup`) ; les pièces
/// jointes ne se déplacent pas par DnD (no-op documenté).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerDrop {
    /// Ligne déplacée.
    pub dragged: FlatRow,
    /// Ligne cible.
    pub target: FlatRow,
    /// Position par rapport à la cible.
    pub position: DropPosition,
}

/// Retrouve un calque par id dans la hiérarchie (récursif).
pub fn find_layer_in(layers: &[PhotoLayerInfo], id: Uuid) -> Option<&PhotoLayerInfo> {
    for layer in layers {
        if layer.id == id {
            return Some(layer);
        }
        if let Some(found) = find_layer_in(&layer.children, id) {
            return Some(found);
        }
    }
    None
}

/// Retrouve une pièce jointe `(info, is_filter)` par `(owner, id)`.
pub fn find_attachment_in(
    layers: &[PhotoLayerInfo],
    owner: Uuid,
    id: Uuid,
) -> Option<(&PhotoSubLayerInfo, bool)> {
    let porteur = find_layer_in(layers, owner)?;
    if let Some(filtre) = porteur.filters.iter().find(|f| f.id == id) {
        return Some((filtre, true));
    }
    porteur
        .masks
        .iter()
        .find(|m| m.id == id)
        .map(|masque| (masque, false))
}

/// Nom affiché d'une ligne aplatie (fantôme de drag).
pub fn flat_row_name(layers: &[PhotoLayerInfo], row: &FlatRow) -> String {
    match row.kind {
        FlatRowKind::Layer => find_layer_in(layers, row.id)
            .map(|layer| layer.name.clone())
            .unwrap_or_default(),
        FlatRowKind::Filter { owner } | FlatRowKind::Mask { owner } => {
            find_attachment_in(layers, owner, row.id)
                .map(|(sub, _)| sub.name.clone())
                .unwrap_or_default()
        }
    }
}

/// Photographie l'état du niveau racine pour l'UI, **haut de pile
/// d'abord** (index 0 = calque du dessus, comme affiché), hiérarchie
/// récursive incluse (`children`).
pub fn snapshot_layers(document: &Document) -> Vec<PhotoLayerInfo> {
    document
        .root
        .iter()
        .rev()
        .map(|node| PhotoLayerInfo::from_node(document, node))
        .collect()
}

/// Variante de [`snapshot_layers`] avec miniatures déjà résolues :
/// `lookup` fournit le `RgbaBuf` miniature d'un calque pixels (issu
/// du partage inter-passes), sans nouvelle résolution d'apparence.
/// La hiérarchie est exposée récursivement (`children`, haut de pile
/// d'abord à chaque niveau).
pub fn snapshot_layers_with(
    document: &Document,
    lookup: &dyn Fn(Uuid) -> Option<photo_engine::RgbaBuf>,
) -> Vec<PhotoLayerInfo> {
    document
        .root
        .iter()
        .rev()
        .map(|node| PhotoLayerInfo::from_node_with_lookup(node, lookup))
        .collect()
}

/// Ligne aplatie de la liste des calques : un calque, un filtre ou
/// un masque, avec sa profondeur d'indentation. Partagée par le rendu
/// (`layer_list`) et la résolution des drops hiérarchiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatRowKind {
    /// Calque (pixels, groupe, ajustement).
    Layer,
    /// Filtre live porté par `owner`.
    Filter {
        /// Calque porteur.
        owner: Uuid,
    },
    /// Masque porté par `owner`.
    Mask {
        /// Calque porteur.
        owner: Uuid,
    },
}

/// Une ligne affichée du panneau Calques.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatRow {
    /// Id de la ligne (calque, filtre ou masque).
    pub id: Uuid,
    /// Nature de la ligne.
    pub kind: FlatRowKind,
    /// Profondeur d'indentation (0 = racine).
    pub depth: usize,
    /// Vrai si un calque peut être déposé DEDANS (groupes dépliés).
    pub nestable: bool,
}

/// Aplatit la hiérarchie pour l'affichage : chaque calque puis, s'il
/// est déplié, ses enfants récursifs, ses filtres et ses masques
/// (lignes à part entière, indentées). Haut de pile d'abord.
pub fn flatten_layers(layers: &[PhotoLayerInfo]) -> Vec<FlatRow> {
    let mut out = Vec::new();
    for layer in layers {
        flatten_layer(layer, 0, &mut out);
    }
    out
}

/// Nombre de lignes affichées pour `layers` (sans les construire).
pub fn flattened_len(layers: &[PhotoLayerInfo]) -> usize {
    flatten_layers(layers).len()
}

fn flatten_layer(layer: &PhotoLayerInfo, depth: usize, out: &mut Vec<FlatRow>) {
    out.push(FlatRow {
        id: layer.id,
        kind: FlatRowKind::Layer,
        depth,
        nestable: layer.kind == PhotoLayerKind::Group && !layer.collapsed,
    });
    if layer.collapsed {
        return;
    }
    for child in &layer.children {
        flatten_layer(child, depth + 1, out);
    }
    for filter in &layer.filters {
        out.push(FlatRow {
            id: filter.id,
            kind: FlatRowKind::Filter { owner: layer.id },
            depth: depth + 1,
            nestable: false,
        });
    }
    for mask in &layer.masks {
        out.push(FlatRow {
            id: mask.id,
            kind: FlatRowKind::Mask { owner: layer.id },
            depth: depth + 1,
            nestable: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::PixelLayer;
    use std::sync::Arc;

    fn test_image() -> Arc<image::DynamicImage> {
        Arc::new(image::DynamicImage::new_rgba8(4, 4))
    }

    fn three_layer_doc() -> Document {
        let mut doc = Document::new(8, 8);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("milieu", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("dessus", test_image())));
        doc
    }

    #[test]
    fn snapshot_is_top_first() {
        let doc = three_layer_doc();
        let layers = snapshot_layers(&doc);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0].name, "dessus");
        assert_eq!(layers[2].name, "fond");
    }

    #[test]
    fn snapshot_preserves_visibility_and_kind() {
        let mut doc = three_layer_doc();
        let id = doc.root[0].id();
        doc.find_mut(id).expect("calque present").set_visible(false);
        let layers = snapshot_layers(&doc);
        let fond = layers
            .iter()
            .find(|l| l.id == id)
            .expect("snapshot present");
        assert!(!fond.visible);
        assert_eq!(fond.kind, PhotoLayerKind::Pixel);
        // Vérifier que l'icône peut être résolue via le registre.
        let _ = ui_kit::icons::IconRegistry::new().text(fond.kind.icon());
    }

    #[test]
    fn layer_kind_icons_come_from_registry() {
        for kind in ALL_PHOTO_LAYER_KINDS {
            let _ = ui_kit::icons::IconRegistry::new().text(kind.icon());
        }
    }

    fn grouped_doc() -> (Document, uuid::Uuid, uuid::Uuid) {
        use photo_engine::GroupLayer;
        let mut doc = Document::new(8, 8);
        let inner = LayerNode::Pixel(PixelLayer::new("dedans", test_image()));
        let inner_id = inner.id();
        let group = GroupLayer::new("groupe", vec![inner]);
        let group_id = group.id;
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Group(group));
        (doc, group_id, inner_id)
    }

    #[test]
    fn snapshot_exposes_group_children_top_first() {
        let (doc, group_id, inner_id) = grouped_doc();
        let layers = snapshot_layers(&doc);
        assert_eq!(layers.len(), 2);
        let group = layers.iter().find(|l| l.id == group_id).expect("groupe");
        assert_eq!(group.kind, PhotoLayerKind::Group);
        assert!(!group.collapsed);
        assert_eq!(group.children.len(), 1);
        assert_eq!(group.children[0].id, inner_id);
        assert_eq!(group.children[0].name, "dedans");
    }

    #[test]
    fn flatten_shows_children_indented_and_collapse_hides_them() {
        let (mut doc, group_id, inner_id) = grouped_doc();
        let rows = flatten_layers(&snapshot_layers(&doc));
        // dessus : groupe, enfant, fond.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, group_id);
        assert_eq!(rows[0].depth, 0);
        assert!(rows[0].nestable, "groupe déplié = cible de dépôt");
        assert_eq!(rows[1].id, inner_id);
        assert_eq!(rows[1].depth, 1);
        assert!(!rows[1].nestable);
        assert_eq!(rows[2].depth, 0);
        assert_eq!(flattened_len(&snapshot_layers(&doc)), 3);
        // Replié : l'enfant disparaît, le groupe reste non-nestable.
        doc.set_collapsed(group_id, true);
        let layers = snapshot_layers(&doc);
        assert!(
            layers
                .iter()
                .find(|l| l.id == group_id)
                .expect("groupe")
                .collapsed
        );
        let rows = flatten_layers(&layers);
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].nestable, "groupe replié = pas de dépôt dedans");
    }

    #[test]
    fn flatten_lists_filters_and_masks_as_child_rows() {
        use photo_engine::FilterLayer;
        use std::collections::HashMap;
        let mut doc = Document::new(8, 8);
        let mut pixels = PixelLayer::new("fond", test_image());
        let filter = FilterLayer::new("brightness_contrast", "luminosite", HashMap::new());
        let filter_id = filter.id;
        pixels.filter_layers.push(filter);
        let layer_id = pixels.id;
        doc.push_layer(LayerNode::Pixel(pixels));
        let rows = flatten_layers(&snapshot_layers(&doc));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, layer_id);
        assert_eq!(rows[0].kind, FlatRowKind::Layer);
        assert_eq!(
            rows[1].kind,
            FlatRowKind::Filter { owner: layer_id },
            "filtre = ligne enfant"
        );
        assert_eq!(rows[1].id, filter_id);
        assert_eq!(rows[1].depth, 1);
    }
}
