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

//! Semantic node: identity, nature, placement, appearance, masks.
//!
//! A [`SceneNode`] carries ONLY document meaning: no pixels, no detailed
//! vector paths, no textures, no renderer state. Heavy payloads
//! stay in the domain engines (pixels in `photo-engine`, paths in
//! `vector-engine`…) and reference the node via [`SceneNodeId`].
//!
//! Covered variants: groups, raster images, vector shapes
//! (nature marker — detailed geometry belongs to the vector
//! engine), text, video clips, layout elements. No
//! audio variant: audio is not part of this graph.

use std::collections::HashMap;

use math_utils::Transform2D;

use crate::{Revision, SceneNodeId};

/// Semantic nature of a node.
///
/// Heavy variants (`Image`, `Shape`, `Video`) are
/// MARKERS: content (pixels, paths, media) lives in the domain
/// engine and points to the [`SceneNodeId`]. Only [`NodeKind::Text`]
/// embeds its data (string + size), light enough to stay here.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Hierarchical container (freely nested groups).
    Group,
    /// Raster image — pixels owned by the domain engine.
    Image,
    /// Vector shape — detailed path owned by `vector-engine`.
    Shape(ShapeKind),
    /// Text — embedded lightweight data.
    Text(TextData),
    /// Video clip — media and timeline owned by `video-engine`.
    Video,
    /// Layout element (frame, page, template) — details in
    /// `layout-engine`, which will never depend on `wgpu`.
    Layout,
}

/// Nature of a shape — marker only, no geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    /// Rectangle (filled or stroked per the vector engine's style).
    Rectangle,
    /// Ellipse / circle.
    Ellipse,
    /// Free path (Bézier curves) — defined in `vector-engine`.
    Path,
}

/// Lightweight text data embedded in the scene.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextData {
    /// Text content.
    pub text: String,
    /// Size in points.
    pub font_size: f32,
}

impl TextData {
    /// New text block.
    pub fn new(text: impl Into<String>, font_size: f32) -> Self {
        Self {
            text: text.into(),
            font_size: font_size.max(0.0),
        }
    }
}

/// Semantic mask attached to a node: reference + switches.
///
/// As with the variants, only SEMANTICS live here (what is masked,
/// active, inverted). Raster coverage stays computed by the engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Mask {
    /// Node (or asset) sourcing the mask.
    pub id: SceneNodeId,
    /// False = mask ignored, settings kept.
    pub enabled: bool,
    /// True = inverted coverage.
    pub inverted: bool,
}

impl Mask {
    /// Active non-inverted mask toward `id`.
    pub fn new(id: SceneNodeId) -> Self {
        Self {
            id,
            enabled: true,
            inverted: false,
        }
    }
}

/// Scene node: the semantic unit of the visual document.
///
/// Fields are publicly readable; every mutation goes through
/// [`crate::Scene`] to keep `parent`/`children` consistent and
/// to allocate [`Revision`]s (see [`crate::revision`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SceneNode {
    /// Stable identifier (persistence, selection, history).
    pub id: SceneNodeId,
    /// Semantic nature (fixed at creation).
    pub kind: NodeKind,
    /// Direct parent (`None` = scene root).
    pub parent: Option<SceneNodeId>,
    /// Direct children, in document order (index 0 = drawn first).
    pub children: Vec<SceneNodeId>,
    /// LOCAL transform — only stored geometry (see [`crate::affine`]).
    pub local: Transform2D,
    /// Visibility switch (AND-combined with ancestors).
    pub visible: bool,
    /// Opacity `0.0..=100.0` (multiplied with ancestors).
    pub opacity: f32,
    /// Attached semantic masks.
    pub masks: Vec<Mask>,
    /// Free metadata (`title`, `role`, inter-engine bridges…).
    pub metadata: HashMap<String, String>,
    /// Last revision having touched this node or its subtree.
    pub revision: Revision,
}

impl SceneNode {
    /// Default opacity (fully opaque).
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

    /// True for [`NodeKind::Group`].
    #[must_use]
    pub fn is_group(&self) -> bool {
        matches!(self.kind, NodeKind::Group)
    }
}
