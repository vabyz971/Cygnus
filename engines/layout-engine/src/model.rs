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

//! Modèle layout : document de cadres typés, sans géométrie calculée.
//!
//! Un [`Frame`] décrit une intention (conteneur, feuille, page) ; les
//! [`Rect`] calculés vivent dans [`ComputedLayout`](crate::ComputedLayout).
//! Le façonnage du texte n'est PAS ici : les feuilles texte portent la
//! chaîne et sa taille, mesurées via [`TextMeasurer`](crate::TextMeasurer).

use std::collections::HashMap;

use ids::EntityId;
use scene::SceneNodeId;

/// Identifiant stable d'un [`Frame`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct FrameId(EntityId);

impl FrameId {
    /// Nouvel identifiant aléatoire.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul (sentinelle, jamais attribué).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'entité sous-jacente.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Enveloppe une entité existante.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for FrameId {
    /// Identifiant nul — même valeur que [`FrameId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for FrameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "l{}", self.0.as_uuid())
    }
}

/// Axe d'empilement d'un conteneur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Horizontal (x croissant).
    Row,
    /// Vertical (y croissant).
    Column,
}

/// Alignement sur l'axe transverse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// Début (haut/gauche).
    Start,
    /// Centré.
    Center,
    /// Fin (bas/droite).
    End,
}

/// Marges internes (paddings) d'un cadre.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Insets {
    /// Gauche.
    pub left: f32,
    /// Haut.
    pub top: f32,
    /// Droite.
    pub right: f32,
    /// Bas.
    pub bottom: f32,
}

impl Insets {
    /// Zéro partout.
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    /// Uniforme (négatifs ramenés à 0).
    #[must_use]
    pub fn uniform(v: f32) -> Self {
        let v = v.max(0.0);
        Self {
            left: v,
            top: v,
            right: v,
            bottom: v,
        }
    }

    /// Largeur horizontale totale.
    #[must_use]
    pub fn horizontal(self) -> f32 {
        (self.left + self.right).max(0.0)
    }

    /// Hauteur verticale totale.
    #[must_use]
    pub fn vertical(self) -> f32 {
        (self.top + self.bottom).max(0.0)
    }
}

/// Bornes min/max appliquées à la taille calculée.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Constraints {
    /// Largeur min.
    pub min_width: f32,
    /// Hauteur min.
    pub min_height: f32,
    /// Largeur max (`INFINITY` = libre).
    pub max_width: f32,
    /// Hauteur max.
    pub max_height: f32,
}

impl Constraints {
    /// Contraintes libres (zéro min, max infini).
    #[must_use]
    pub fn free() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
        }
    }

    /// Taille fixe (min = max).
    #[must_use]
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            min_width: width.max(0.0),
            min_height: height.max(0.0),
            max_width: width.max(0.0),
            max_height: height.max(0.0),
        }
    }

    /// Borne une taille (min puis max, NaN → 0).
    #[must_use]
    pub fn clamp_size(self, w: f32, h: f32) -> (f32, f32) {
        let w = sanitize(w).clamp(self.min_width.max(0.0), self.max_width.max(0.0));
        let h = sanitize(h).clamp(self.min_height.max(0.0), self.max_height.max(0.0));
        (w.min(self.max_width), h.min(self.max_height))
    }
}

fn sanitize(v: f32) -> f32 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// Contenu d'une feuille.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Content {
    /// Vide (taille nulle).
    Empty,
    /// Boîte de taille intrinsèque fixe.
    Box {
        /// Largeur.
        width: f32,
        /// Hauteur.
        height: f32,
    },
    /// Texte (mesuré via `TextMeasurer` — jamais façonné ici).
    Text {
        /// Chaîne (retours `\n` = lignes).
        text: String,
        /// Corps en points.
        font_size: f32,
    },
}

/// Nature d'un cadre.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    /// Conteneur d'empilement (enfants positionnés par l'algo).
    Container {
        /// Axe.
        direction: Direction,
        /// Espacement entre enfants (≥ 0).
        gap: f32,
        /// Alignement transverse.
        alignment: Alignment,
    },
    /// Feuille (taille intrinsèque ou mesurée).
    Leaf {
        /// Contenu.
        content: Content,
    },
    /// Page à taille imposée (enfants empilés en colonne dans les marges).
    Page {
        /// Largeur imposée.
        width: f32,
        /// Hauteur imposée.
        height: f32,
        /// Marges internes.
        margins: Insets,
    },
}

/// Cadre : intention de mise en page + rattachement scène.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    /// Identifiant stable.
    pub id: FrameId,
    /// Nom d'affichage.
    pub name: String,
    /// Nature.
    pub kind: FrameKind,
    /// Bornes appliquées à la taille calculée.
    pub constraints: Constraints,
    /// Marges internes (conteneurs et pages ; ignorées des feuilles).
    pub insets: Insets,
    /// Parent (`None` = racine du document).
    pub parent: Option<FrameId>,
    /// Enfants dans l'ordre.
    pub children: Vec<FrameId>,
    /// Nœud de scène recevant le placement (`None` = non mappé).
    pub scene: Option<SceneNodeId>,
}

impl Frame {
    pub(crate) fn new(id: FrameId, name: String, kind: FrameKind) -> Self {
        Self {
            id,
            name,
            kind,
            constraints: Constraints::free(),
            insets: Insets::ZERO,
            parent: None,
            children: Vec::new(),
            scene: None,
        }
    }
}

/// Document layout : cadres hiérarchisés (table + racines).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LayoutDoc {
    frames: HashMap<FrameId, Frame>,
    roots: Vec<FrameId>,
}

impl LayoutDoc {
    /// Document vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de cadres.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Vrai si suivi.
    #[must_use]
    pub fn contains(&self, id: FrameId) -> bool {
        self.frames.contains_key(&id)
    }

    /// Racines dans l'ordre.
    #[must_use]
    pub fn roots(&self) -> &[FrameId] {
        &self.roots
    }

    /// Cadre par id.
    #[must_use]
    pub fn find(&self, id: FrameId) -> Option<&Frame> {
        self.frames.get(&id)
    }

    /// Crée un cadre racine.
    pub fn create_root(&mut self, name: impl Into<String>, kind: FrameKind) -> FrameId {
        let id = FrameId::new();
        self.frames.insert(id, Frame::new(id, name.into(), kind));
        self.roots.push(id);
        id
    }

    /// Crée un cadre enfant (`None` si parent inconnu).
    pub fn create_child(
        &mut self,
        parent: FrameId,
        name: impl Into<String>,
        kind: FrameKind,
    ) -> Option<FrameId> {
        if !self.frames.contains_key(&parent) {
            return None;
        }
        let id = FrameId::new();
        let mut frame = Frame::new(id, name.into(), kind);
        frame.parent = Some(parent);
        self.frames.insert(id, frame);
        if let Some(slot) = self.frames.get_mut(&parent) {
            slot.children.push(id);
        }
        Some(id)
    }

    /// Rattache un cadre à un nœud de scène (faux si cadre inconnu).
    pub fn map_to_scene(&mut self, id: FrameId, scene: SceneNodeId) -> bool {
        match self.frames.get_mut(&id) {
            Some(frame) => {
                frame.scene = Some(scene);
                true
            }
            None => false,
        }
    }

    /// Remplace les marges internes (faux si cadre inconnu).
    pub fn set_insets(&mut self, id: FrameId, insets: Insets) -> bool {
        match self.frames.get_mut(&id) {
            Some(frame) => {
                frame.insets = insets;
                true
            }
            None => false,
        }
    }

    /// Remplace l'alignement transverse (faux si inconnu ou non-conteneur).
    pub fn set_alignment(&mut self, id: FrameId, alignment: Alignment) -> bool {
        match self.frames.get_mut(&id) {
            Some(frame) => match &mut frame.kind {
                FrameKind::Container {
                    alignment: slot, ..
                } => {
                    *slot = alignment;
                    true
                }
                _ => false,
            },
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contraintes_bornent() {
        let c = Constraints::fixed(10.0, 20.0);
        assert_eq!(c.clamp_size(99.0, 99.0), (10.0, 20.0));
        assert_eq!(Constraints::free().clamp_size(3.0, 4.0), (3.0, 4.0));
        assert_eq!(Constraints::free().clamp_size(f32::NAN, -2.0), (0.0, 0.0));
    }

    #[test]
    fn document_hierarchie_et_mapping() {
        let mut doc = LayoutDoc::new();
        let root = doc.create_root(
            "page",
            FrameKind::Page {
                width: 100.0,
                height: 100.0,
                margins: Insets::ZERO,
            },
        );
        let child = doc
            .create_child(
                root,
                "bloc",
                FrameKind::Leaf {
                    content: Content::Empty,
                },
            )
            .expect("parent exists");
        assert_eq!(doc.roots(), &[root]);
        assert_eq!(doc.find(root).expect("exists").children, vec![child]);
        assert!(doc.map_to_scene(child, SceneNodeId::nil()));
        assert!(!doc.map_to_scene(FrameId::nil(), SceneNodeId::nil()));
        assert!(
            doc.create_child(
                FrameId::nil(),
                "x",
                FrameKind::Leaf {
                    content: Content::Empty
                }
            )
            .is_none()
        );
    }
}
