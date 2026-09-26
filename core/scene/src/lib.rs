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

//! Scene Graph sémantique de Cygnus — le modèle commun aux domaines visuels.
//!
//! Séparation visée :
//!
//! ```text
//! Document (métier, par domaine : photo, vectoriel, layout, video)
//!   ↓ dérive / synchronise
//! Scene (ici : hiérarchie sémantique — groupes, images, formes, texte, video)
//!   ↓ dérive (futur Render Graph, hors périmètre)
//! Rendering (textures GPU, caches, passes — jamais ici)
//! ```
//!
//! Règles de cette crate :
//!
//! - AUCUNE dépendance `wgpu`, texture GPU, `RenderPass`, buffer GPU, `egui`
//!   ou état spécifique à un renderer. La scène représente le document,
//!   pas son rendu.
//! - Le transform LOCAL ([`Transform2D`], réutilisé depuis `math-utils`)
//!   est la seule source de vérité géométrique stockée. Le transform MONDE
//!   ([`Affine2`]) est toujours DÉRIVÉ par composition parent → enfant,
//!   jamais stocké.
//! - Convention géométrique : la matrice locale vaut
//!   `T(offset) · R(rotation) · K(skew) · S(scale)` autour de l'ORIGINE
//!   (voir [`Affine2::from_transform`]). C'est la même décomposition que
//!   `photo-engine`, sans sa convention « autour du centre de l'image »
//!   qui dépend des dimensions du contenu et n'a pas de sens pour un
//!   groupe ou un cadre de mise en page. L'adaptation vers le placement
//!   raster existant (`Transform2D::local_to_doc`) reste à la charge des
//!   moteurs au moment de dériver leurs opérations de rendu.
//! - Opacité en `0.0..=100.0` (même unité que `photo-engine`) pour une
//!   future migration sans conversion silencieuse.
//! - Audio est volontairement EXCLU : aucun variant audio dans [`NodeKind`].
//!   L'audio partage le nodal générique (`datatypes`) et les futurs
//!   `ids`/`time`, jamais ce graphe.
//!
//! # Examples
//!
//! ```
//! use scene::{NodeKind, Scene};
//!
//! let mut scene = Scene::new();
//! let root = scene.create_node(NodeKind::Group);
//! let child = scene.create_child(root, NodeKind::Image).expect("parent exists");
//! assert_eq!(scene.parent_of(child), Some(root));
//! assert!(scene.world_affine(child).is_some());
//! ```

pub mod affine;
pub mod id;
pub mod node;
pub mod revision;
pub mod scene;

#[cfg(test)]
mod tests;

pub use affine::Affine2;
pub use id::SceneNodeId;
pub use math_utils::Transform2D;
pub use node::{Mask, NodeKind, SceneNode, ShapeKind, TextData};
pub use revision::Revision;
pub use scene::Scene;
