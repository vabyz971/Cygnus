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

//! Nœud sémantique : identité, nature, placement, apparence, masques.
//!
//! Un [`SceneNode`] ne porte QUE du sens document : ni pixels, ni chemins
//! vectoriels détaillés, ni textures, ni état renderer. Les charges lourdes
//! restent dans les moteurs de domaine (pixels en `photo-engine`, tracés en
//! `vector-engine`…) et référencent le nœud par [`SceneNodeId`].
//!
//! Variants couverts : groupes, images raster, formes vectorielles
//! (marqueur de nature — la géométrie détaillée appartient au moteur
//! vectoriel), texte, clips vidéo, éléments de mise en page. Pas de
//! variant audio : l'audio ne fait pas partie de ce graphe.

use std::collections::HashMap;

use math_utils::Transform2D;

use crate::{Revision, SceneNodeId};

/// Nature sémantique d'un nœud.
///
/// Les variants à charge lourde (`Image`, `Shape`, `Video`) sont des
/// MARQUEURS : le contenu (pixels, tracés, médias) vit dans le moteur du
/// domaine et pointe vers le [`SceneNodeId`]. Seul [`NodeKind::Text`]
/// embarque sa donnée (chaîne + taille), assez légère pour rester ici.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Conteneur hiérarchique (groupes imbriqués à volonté).
    Group,
    /// Image raster — pixels détenus par le moteur du domaine.
    Image,
    /// Forme vectorielle — tracé détaillé détenu par `vector-engine`.
    Shape(ShapeKind),
    /// Texte — donnée légère embarquée.
    Text(TextData),
    /// Clip vidéo — médias et timeline détenus par `video-engine`.
    Video,
    /// Élément de mise en page (cadre, page, gabarit) — détails en
    /// `layout-engine`, qui ne dépendra jamais de `wgpu`.
    Layout,
}

/// Nature d'une forme — marqueur uniquement, sans géométrie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    /// Rectangle (plein ou contour selon le style du moteur vectoriel).
    Rectangle,
    /// Ellipse / cercle.
    Ellipse,
    /// Tracé libre (courbes de Bézier) — défini en `vector-engine`.
    Path,
}

/// Donnée texte légère embarquée dans la scène.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextData {
    /// Contenu textuel.
    pub text: String,
    /// Corps en points.
    pub font_size: f32,
}

impl TextData {
    /// Nouveau bloc texte.
    pub fn new(text: impl Into<String>, font_size: f32) -> Self {
        Self {
            text: text.into(),
            font_size: font_size.max(0.0),
        }
    }
}

/// Masque sémantique attaché à un nœud : référence + interrupteurs.
///
/// Comme pour les variants, seule la SÉMANTIQUE vit ici (quoi est masqué,
/// actif, inversé). La couverture raster reste calculée par les moteurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Mask {
    /// Nœud (ou asset) source du masque.
    pub id: SceneNodeId,
    /// Faux = masque ignoré, réglages conservés.
    pub enabled: bool,
    /// Vrai = couverture inversée.
    pub inverted: bool,
}

impl Mask {
    /// Masque actif non inversé vers `id`.
    pub fn new(id: SceneNodeId) -> Self {
        Self {
            id,
            enabled: true,
            inverted: false,
        }
    }
}

/// Nœud de la scène : l'unité sémantique du document visuel.
///
/// Les champs sont publics en lecture ; toute mutation passe par
/// [`crate::Scene`] afin de maintenir `parent`/`children` cohérents et
/// d'allouer les [`Revision`] (voir [`crate::revision`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SceneNode {
    /// Identifiant stable (persistance, sélection, historique).
    pub id: SceneNodeId,
    /// Nature sémantique (fixée à la création).
    pub kind: NodeKind,
    /// Parent direct (`None` = racine de la scène).
    pub parent: Option<SceneNodeId>,
    /// Enfants directs, dans l'ordre de document (index 0 = dessiné en premier).
    pub children: Vec<SceneNodeId>,
    /// Transform LOCAL — seule géométrie stockée (voir [`crate::affine`]).
    pub local: Transform2D,
    /// Interrupteur de visibilité (combiné par ET avec les ancêtres).
    pub visible: bool,
    /// Opacité `0.0..=100.0` (combinée par produit avec les ancêtres).
    pub opacity: f32,
    /// Masques sémantiques attachés.
    pub masks: Vec<Mask>,
    /// Métadonnées libres (`titre`, `rôle`, ponts inter-moteurs…).
    pub metadata: HashMap<String, String>,
    /// Dernière révision ayant touché ce nœud ou sa sous-arborescence.
    pub revision: Revision,
}

impl SceneNode {
    /// Opacité par défaut (pleinement opaque).
    pub const FULL_OPACITY: f32 = 100.0;

    pub(crate) fn new(id: SceneNodeId, kind: NodeKind, revision: Revision) -> Self {
        Self {
            id,
            kind,
            parent: None,
            children: Vec::new(),
            local: Transform2D::default(),
            visible: true,
            opacity: Self::FULL_OPACITY,
            masks: Vec::new(),
            metadata: HashMap::new(),
            revision,
        }
    }

    /// Vrai pour [`NodeKind::Group`].
    #[must_use]
    pub fn is_group(&self) -> bool {
        matches!(self.kind, NodeKind::Group)
    }
}
