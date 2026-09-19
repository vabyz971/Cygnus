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
    /// Miniature pixels (`None` = groupes/ajustements : icône de type).
    pub thumb: Option<PhotoLayerThumb>,
    /// Sélection courante (état UI).
    pub selected: bool,
}

impl PhotoLayerInfo {
    /// Dérive une entrée d'affichage d'un nœud moteur.
    pub fn from_node(document: &Document, node: &LayerNode) -> Self {
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
        // Miniature servie par le cache d'apparences moteur (zéro
        // recalcul à chaud) : seuls les pixels en ont une.
        let thumb = match node {
            LayerNode::Pixel(pixels) => document.thumb(node.id()).map(|buf| PhotoLayerThumb {
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
            thumb,
            selected: false,
        }
    }
}

/// Photographie l'état du niveau racine pour l'UI, **haut de pile
/// d'abord** (index 0 = calque du dessus, comme affiché).
///
/// Les indices d'affichage sont l'unité des commandes `ReorderLayer` ;
/// le worker les reconvertit en indices document (voir
/// [`crate::ui::engine_bridge`]).
pub fn snapshot_layers(document: &Document) -> Vec<PhotoLayerInfo> {
    document
        .root
        .iter()
        .rev()
        .map(|node| PhotoLayerInfo::from_node(document, node))
        .collect()
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
}
