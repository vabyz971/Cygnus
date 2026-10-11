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

//! Layout model: typed-frame document, no computed geometry.
//!
//! A [`Frame`] describes an intent (container, leaf, page); computed
//! [`Rect`]s live in [`ComputedLayout`](crate::ComputedLayout).
//! Text shaping is NOT here: text leaves carry the
//! string and its size, measured via [`TextMeasurer`](crate::TextMeasurer).

use std::collections::HashMap;

use ids::EntityId;
use scene::SceneNodeId;

/// Stable identifier of a [`Frame`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct FrameId(EntityId);

impl FrameId {
    /// New random identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Null identifier (sentinel, never assigned).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// The underlying entity.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Wraps an existing entity.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for FrameId {
    /// Null identifier — same value as [`FrameId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for FrameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "l{}", self.0.as_uuid())
    }
}

/// Stack axis of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Horizontal (increasing x).
    Row,
    /// Vertical (increasing y).
    Column,
}

/// Cross-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// Start (top/left).
    Start,
    /// Centered.
    Center,
    /// End (bottom/right).
    End,
}

/// Inner margins (padding) of a frame.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Insets {
    /// Left.
    pub left: f32,
    /// Top.
    pub top: f32,
    /// Right.
    pub right: f32,
    /// Bottom.
    pub bottom: f32,
}

impl Insets {
    /// Zero everywhere.
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    /// Uniform (negatives clamped to 0).
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

    /// Total horizontal width.
    #[must_use]
    pub fn horizontal(self) -> f32 {
        (self.left + self.right).max(0.0)
    }

    /// Total vertical height.
    #[must_use]
    pub fn vertical(self) -> f32 {
        (self.top + self.bottom).max(0.0)
    }
}

/// Min/max bounds applied to the computed size.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Constraints {
    /// Min width.
    pub min_width: f32,
    /// Min height.
    pub min_height: f32,
    /// Max width (`INFINITY` = unconstrained).
    pub max_width: f32,
    /// Max height.
    pub max_height: f32,
}

impl Constraints {
    /// Free constraints (zero min, infinite max).
    #[must_use]
    pub fn free() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
        }
    }

    /// Fixed size (min = max).
    #[must_use]
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            min_width: width.max(0.0),
            min_height: height.max(0.0),
            max_width: width.max(0.0),
            max_height: height.max(0.0),
        }
    }

    /// Clamps a size (min then max, NaN → 0).
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

/// Leaf content.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Content {
    /// Empty (zero size).
    Empty,
    /// Fixed intrinsic-size box.
    Box {
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// Text (measured via `TextMeasurer` — never shaped here).
    Text {
        /// String (returns `\n` = lines).
        text: String,
        /// Size in points.
        font_size: f32,
    },
}

/// Kind of a frame.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    /// Stack container (children positioned by the algorithm).
    Container {
        /// Axis.
        direction: Direction,
        /// Spacing between children (≥ 0).
        gap: f32,
        /// Cross-axis alignment.
        alignment: Alignment,
    },
    /// Leaf (intrinsic or measured size).
    Leaf {
        /// Content.
        content: Content,
    },
    /// Fixed-size page (children stacked in a column within the margins).
    Page {
        /// Imposed width.
        width: f32,
        /// Imposed height.
        height: f32,
        /// Inner margins.
        margins: Insets,
    },
}

/// Frame: layout intent + scene attachment.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    /// Stable identifier.
    pub id: FrameId,
    /// Display name.
    pub name: String,
    /// Kind.
    pub kind: FrameKind,
    /// Bounds applied to the computed size.
    pub constraints: Constraints,
    /// Inner margins (containers and pages; ignored by leaves).
    pub insets: Insets,
    /// Parent (`None` = document root).
    pub parent: Option<FrameId>,
    /// Children in order.
    pub children: Vec<FrameId>,
    /// Scene node receiving the placement (`None` = unmapped).
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

/// Layout document: hierarchical frames (table + roots).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LayoutDoc {
    frames: HashMap<FrameId, Frame>,
    roots: Vec<FrameId>,
}

impl LayoutDoc {
    /// Empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of frames.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// True if tracked.
    #[must_use]
    pub fn contains(&self, id: FrameId) -> bool {
        self.frames.contains_key(&id)
    }

    /// Roots in order.
    #[must_use]
    pub fn roots(&self) -> &[FrameId] {
        &self.roots
    }

    /// Frame by id.
    #[must_use]
    pub fn find(&self, id: FrameId) -> Option<&Frame> {
        self.frames.get(&id)
    }

    /// Creates a root frame.
    pub fn create_root(&mut self, name: impl Into<String>, kind: FrameKind) -> FrameId {
        let id = FrameId::new();
        self.frames.insert(id, Frame::new(id, name.into(), kind));
        self.roots.push(id);
        id
    }

    /// Creates a child frame (`None` if parent unknown).
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

    /// Attaches a frame to a scene node (false if frame unknown).
    pub fn map_to_scene(&mut self, id: FrameId, scene: SceneNodeId) -> bool {
        match self.frames.get_mut(&id) {
            Some(frame) => {
                frame.scene = Some(scene);
                true
            }
            None => false,
        }
    }

    /// Replaces the inner margins (false if frame unknown).
    pub fn set_insets(&mut self, id: FrameId, insets: Insets) -> bool {
        match self.frames.get_mut(&id) {
            Some(frame) => {
                frame.insets = insets;
                true
            }
            None => false,
        }
    }

    /// Replaces the cross-axis alignment (false if unknown or not a container).
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
