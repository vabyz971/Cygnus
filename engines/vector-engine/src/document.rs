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

//! Document vectoriel : formes identifiées, ordonnées, stylées.
//!
//! [`VectorScene`] possède les [`Shape`] (table + ordre de peinture) :
//! le backend les consomme en ordre via [`crate::render_scene`]. Les
//! composés booléens sont des arbres de formes (évaluation côté backend).

use std::collections::HashMap;

use datatypes::{Rect, Vec2};
use ids::EntityId;
use math_utils::Transform2D;

use crate::geometry::Path;
use crate::{BooleanOp, Fill, Stroke};

/// Identifiant stable d'une [`Shape`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct ShapeId(EntityId);

impl ShapeId {
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

impl Default for ShapeId {
    /// Identifiant nul — même valeur que [`ShapeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for ShapeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}", self.0.as_uuid())
    }
}

/// Géométrie d'une forme (le composé booléen est un arbre évalué par le
/// backend — le modèle en possède la STRUCTURE, pas le résultat).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeGeometry {
    /// Tracé Bézier libre.
    Path(Path),
    /// Rectangle axis-aligned.
    Rect(Rect),
    /// Ellipse (centre + rayons).
    Ellipse {
        /// Centre.
        center: Vec2,
        /// Rayon horizontal.
        rx: f32,
        /// Rayon vertical.
        ry: f32,
    },
    /// Combinaison booléenne de formes enfants.
    Compound {
        /// Opération.
        op: BooleanOp,
        /// Opérandes (enfants possédés).
        shapes: Vec<Shape>,
    },
}

impl ShapeGeometry {
    /// Englobant (conservatif pour les composés : union des enfants).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match self {
            ShapeGeometry::Path(p) => p.bounds(),
            ShapeGeometry::Rect(r) => *r,
            ShapeGeometry::Ellipse { center, rx, ry } => Rect::new(
                center.x - rx.abs(),
                center.y - ry.abs(),
                center.x + rx.abs(),
                center.y + ry.abs(),
            ),
            ShapeGeometry::Compound { shapes, .. } => {
                let mut bounds = Rect::EMPTY;
                for child in shapes {
                    bounds = bounds.union(child.bounds());
                }
                bounds
            }
        }
    }
}

/// Forme : géométrie + styles + placement + visibilité.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shape {
    /// Identifiant stable.
    pub id: ShapeId,
    /// Nom d'affichage.
    pub name: String,
    /// Géométrie.
    pub geometry: ShapeGeometry,
    /// Remplissage (`None` = non rempli).
    pub fill: Option<Fill>,
    /// Contour (`None` = sans trait).
    pub stroke: Option<Stroke>,
    /// Placement (local — le backend compose).
    pub transform: Transform2D,
    /// Interrupteur de visibilité.
    pub visible: bool,
    /// Opacité 0..=100 (même unité que le reste de la suite).
    pub opacity: f32,
}

impl Shape {
    /// Englobant local (AVANT transform — le backend place).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.geometry.bounds()
    }
}

/// Document vectoriel : formes ordonnées (index 0 = dessous de pile).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct VectorScene {
    shapes: HashMap<ShapeId, Shape>,
    order: Vec<ShapeId>,
}

impl VectorScene {
    /// Scène vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de formes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Vrai si la forme est suivie.
    #[must_use]
    pub fn contains(&self, id: ShapeId) -> bool {
        self.shapes.contains_key(&id)
    }

    /// Ordre de peinture (dessous → dessus).
    #[must_use]
    pub fn order(&self) -> &[ShapeId] {
        &self.order
    }

    /// Forme par id.
    #[must_use]
    pub fn find(&self, id: ShapeId) -> Option<&Shape> {
        self.shapes.get(&id)
    }

    /// Ajoute une forme au-dessus de la pile (valeurs par défaut :
    /// visible, opaque, sans style — à poser ensuite).
    pub fn add_shape(&mut self, name: impl Into<String>, geometry: ShapeGeometry) -> ShapeId {
        let id = ShapeId::new();
        self.shapes.insert(
            id,
            Shape {
                id,
                name: name.into(),
                geometry,
                fill: None,
                stroke: None,
                transform: Transform2D::default(),
                visible: true,
                opacity: 100.0,
            },
        );
        self.order.push(id);
        id
    }

    /// Retire une forme (faux si inconnue).
    pub fn remove(&mut self, id: ShapeId) -> bool {
        if self.shapes.remove(&id).is_none() {
            return false;
        }
        self.order.retain(|&s| s != id);
        true
    }

    /// Remplace le remplissage (faux si inconnue).
    pub fn set_fill(&mut self, id: ShapeId, fill: Option<Fill>) -> bool {
        match self.shapes.get_mut(&id) {
            Some(shape) => {
                shape.fill = fill;
                true
            }
            None => false,
        }
    }

    /// Remplace le contour (faux si inconnue).
    pub fn set_stroke(&mut self, id: ShapeId, stroke: Option<Stroke>) -> bool {
        match self.shapes.get_mut(&id) {
            Some(shape) => {
                shape.stroke = stroke;
                true
            }
            None => false,
        }
    }

    /// Remplace le placement (faux si inconnue).
    pub fn set_transform(&mut self, id: ShapeId, transform: Transform2D) -> bool {
        match self.shapes.get_mut(&id) {
            Some(shape) => {
                shape.transform = transform;
                true
            }
            None => false,
        }
    }

    /// Bascule la visibilité (faux si inconnue).
    pub fn set_visible(&mut self, id: ShapeId, visible: bool) -> bool {
        match self.shapes.get_mut(&id) {
            Some(shape) => {
                shape.visible = visible;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, PathVerb};
    use datatypes::Vec2;

    #[test]
    fn scene_ordre_et_retrait() {
        let mut scene = VectorScene::new();
        assert!(scene.is_empty());
        let a = scene.add_shape(
            "a",
            ShapeGeometry::Rect(Rect::from_xywh(0.0, 0.0, 5.0, 5.0)),
        );
        let b = scene.add_shape(
            "b",
            ShapeGeometry::Rect(Rect::from_xywh(1.0, 1.0, 2.0, 2.0)),
        );
        assert_eq!(scene.order(), &[a, b]);
        assert!(scene.set_fill(a, Some(Fill::Solid(Color::BLACK))));
        assert!(!scene.set_fill(ShapeId::nil(), None));
        assert!(scene.remove(a));
        assert!(!scene.remove(a));
        assert_eq!(scene.order(), &[b]);
        let _ = Vec2::new(0.0, 0.0);
        let _ = PathVerb::Close;
    }

    #[test]
    fn ellipse_et_compose_bornes() {
        let ellipse = ShapeGeometry::Ellipse {
            center: Vec2::new(5.0, 5.0),
            rx: 3.0,
            ry: 2.0,
        };
        assert_eq!(ellipse.bounds(), Rect::new(2.0, 3.0, 8.0, 7.0));
        let mut scene = VectorScene::new();
        let child = Shape {
            id: ShapeId::new(),
            name: String::from("enfant"),
            geometry: ShapeGeometry::Rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0)),
            fill: None,
            stroke: None,
            transform: Transform2D::default(),
            visible: true,
            opacity: 100.0,
        };
        let compound = ShapeGeometry::Compound {
            op: BooleanOp::Union,
            shapes: vec![child],
        };
        let id = scene.add_shape("bool", compound);
        assert_eq!(
            scene.find(id).expect("exists").bounds(),
            Rect::new(0.0, 0.0, 4.0, 4.0)
        );
    }
}
