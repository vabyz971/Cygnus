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

//! Cygnus semantic Scene Graph — the model shared by visual domains.
//!
//! Intended separation:
//!
//! ```text
//! Document (business, per domain: photo, vector, layout, video)
//!   ↓ derives / synchronizes
//! Scene (here: semantic hierarchy — groups, images, shapes, text, video)
//!   ↓ derives (future Render Graph, out of scope)
//! Rendering (GPU textures, caches, passes — never here)
//! ```
//!
//! Rules of this crate:
//!
//! - NO `wgpu`, GPU texture, `RenderPass`, GPU buffer, `egui`
//!   or renderer-specific state dependency. The scene represents the document,
//!   not its rendering.
//! - The LOCAL transform ([`Transform2D`], reused from `math-utils`)
//!   is the only stored geometric source of truth. The WORLD transform
//!   ([`Affine2`]) is always DERIVED by parent → child composition,
//!   never stored.
//! - Geometric convention: the local matrix is
//!   `T(offset) · R(rotation) · K(skew) · S(scale)` around the ORIGIN
//!   (see [`Affine2::from_transform`]). This is the same decomposition as
//!   `photo-engine`, without its "around the image center" convention
//!   which depends on content dimensions and is meaningless for a
//!   group or a layout frame. Adapting to the existing raster
//!   placement (`Transform2D::local_to_doc`) stays the engines'
//!   job when deriving their render operations.
//! - Opacity in `0.0..=100.0` (same unit as `photo-engine`) for a
//!   future migration with no silent conversion.
//! - Audio is deliberately EXCLUDED: no audio variant in [`NodeKind`].
//!   Audio shares the generic nodal system (`datatypes`) and the future
//!   `ids`/`time`, never this graph.
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
