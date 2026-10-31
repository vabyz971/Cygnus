// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! The document model: a tree of nodes stored in an arena
//! (docs/decisions/0009).
//!
//! Nodes are addressed by stable identifiers, never by position. Pixels are
//! stored outside the tree, in a [`PixelStore`].

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pixels::PixelStore;

/// Version of the serialized model.
pub const MODEL_VERSION: u32 = 1;

/// Largest document side, in pixels.
pub const MAX_SIDE: u32 = 65_535;

/// Stable identifier of a node, unique within its document and never
/// reused.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct NodeId(pub u64);

/// Layer blend modes (CAL-3). Only `Normal` is rendered before milestone J3;
/// the others are declared now so that the serialized form is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    /// Normal (source over).
    #[default]
    Normal,
    /// Multiply.
    Multiply,
    /// Screen.
    Screen,
    /// Overlay.
    Overlay,
    /// Soft light.
    SoftLight,
    /// Darken.
    Darken,
    /// Lighten.
    Lighten,
    /// Difference.
    Difference,
    /// Hue.
    Hue,
    /// Saturation.
    Saturation,
    /// Color.
    Color,
    /// Luminosity.
    Luminosity,
}

impl BlendMode {
    /// Every mode, in the order of CAL-3.
    pub const ALL: [Self; 12] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::SoftLight,
        Self::Darken,
        Self::Lighten,
        Self::Difference,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];

    /// Numeric code of the mode, shared with the WGSL blend module.
    pub fn code(self) -> u32 {
        self as u32
    }

    /// Whether the renderer supports the mode yet.
    pub fn is_supported(self) -> bool {
        matches!(self, Self::Normal)
    }
}

/// Bits per channel of the document pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Depth {
    /// 8 bits per channel.
    U8,
    /// 16 bits per channel.
    U16,
}

impl Depth {
    /// Largest channel value.
    pub fn max_value(self) -> u16 {
        match self {
            Self::U8 => u8::MAX as u16,
            Self::U16 => u16::MAX,
        }
    }
}

/// Color profile of the document. Only sRGB before milestone J6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorProfile {
    /// sRGB IEC 61966-2-1.
    #[default]
    Srgb,
}

/// A rectangle of document pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl Rect {
    /// Rectangle at the origin with the given size.
    pub fn of_size(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Whether the rectangle has no pixel.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Right edge (exclusive).
    pub fn right(&self) -> u64 {
        u64::from(self.x) + u64::from(self.width)
    }

    /// Bottom edge (exclusive).
    pub fn bottom(&self) -> u64 {
        u64::from(self.y) + u64::from(self.height)
    }

    /// Intersection with `other`, or an empty rectangle.
    pub fn intersect(&self, other: &Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= u64::from(x) || bottom <= u64::from(y) {
            return Self::of_size(0, 0);
        }
        // Both edges come from u32 rectangles, so the sizes fit in u32.
        Self {
            x,
            y,
            width: u32::try_from(right - u64::from(x)).unwrap_or(0),
            height: u32::try_from(bottom - u64::from(y)).unwrap_or(0),
        }
    }
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A group of nodes, drawn bottom to top. Only the root before J3.
    Group {
        /// Children, bottom first.
        children: Vec<NodeId>,
    },
    /// A pixel layer; its pixels live in the document [`PixelStore`].
    Pixel {
        /// Area the layer may hold pixels in.
        bounds: Rect,
    },
}

/// A node of the layer tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Identifier.
    pub id: NodeId,
    /// Parent group; `None` only for the root.
    pub parent: Option<NodeId>,
    /// Name shown in the Layers panel.
    pub name: String,
    /// Visibility.
    pub visible: bool,
    /// Opacity, from 0 to 1.
    pub opacity: f32,
    /// Blend mode.
    pub blend: BlendMode,
    /// A locked node refuses changes, except to its visibility and lock, or
    /// changes made by a command that also unlocks it.
    pub locked: bool,
    /// Mask node. Reserved: always `None` before milestone J3.
    pub mask: Option<NodeId>,
    /// What the node is.
    pub kind: NodeKind,
}

/// A Cygnus Photo document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Model version, see [`MODEL_VERSION`].
    pub version: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Resolution in pixels per inch.
    pub resolution: f32,
    /// Bits per channel.
    pub depth: Depth,
    /// Color profile.
    pub profile: ColorProfile,
    /// Root group.
    pub root: NodeId,
    /// Every node, by identifier.
    pub nodes: BTreeMap<NodeId, Node>,
    /// Next identifier to hand out.
    pub next_id: u64,
    /// Pixels of the layers. Not part of the serialized tree: the file
    /// format stores them separately (milestone J6).
    #[serde(skip)]
    pub pixels: PixelStore,
}

impl Default for Document {
    /// An empty 1 × 1 document, the state of a bus before
    /// `document.create`.
    fn default() -> Self {
        Self::new(1, 1, 72.0, Depth::U8, ColorProfile::Srgb)
    }
}

impl Document {
    /// A document with an empty root group.
    pub fn new(
        width: u32,
        height: u32,
        resolution: f32,
        depth: Depth,
        profile: ColorProfile,
    ) -> Self {
        let root = NodeId(0);
        let mut nodes = BTreeMap::new();
        nodes.insert(
            root,
            Node {
                id: root,
                parent: None,
                name: String::new(),
                visible: true,
                opacity: 1.0,
                blend: BlendMode::Normal,
                locked: false,
                mask: None,
                kind: NodeKind::Group {
                    children: Vec::new(),
                },
            },
        );
        Self {
            version: MODEL_VERSION,
            width,
            height,
            resolution,
            depth,
            profile,
            root,
            nodes,
            next_id: 1,
            pixels: PixelStore::default(),
        }
    }

    /// Rectangle covering the whole document.
    pub fn bounds(&self) -> Rect {
        Rect::of_size(self.width, self.height)
    }

    /// The node `id`, if it exists.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Children of group `id`, bottom first; empty for other nodes.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        match self.nodes.get(&id).map(|node| &node.kind) {
            Some(NodeKind::Group { children }) => children,
            _ => &[],
        }
    }

    /// Identifier the next created node will receive.
    pub fn peek_next_id(&self) -> NodeId {
        NodeId(self.next_id)
    }

    /// Checks the invariants of the tree. Commands keep them; this is used
    /// by tests and when loading untrusted data.
    pub fn validate(&self) -> Result<(), String> {
        let root = self
            .nodes
            .get(&self.root)
            .ok_or_else(|| "missing root".to_owned())?;
        if root.parent.is_some() || !matches!(root.kind, NodeKind::Group { .. }) {
            return Err("the root must be a group without parent".to_owned());
        }
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                return Err(format!("node {} reached twice", id.0));
            }
            let node = self
                .nodes
                .get(&id)
                .ok_or_else(|| format!("unknown node {}", id.0))?;
            if node.id != id {
                return Err(format!("node {} stored under {}", node.id.0, id.0));
            }
            if !(0.0..=1.0).contains(&node.opacity) {
                return Err(format!("node {} has opacity {}", id.0, node.opacity));
            }
            if id.0 >= self.next_id && id != self.root {
                return Err(format!("node {} not below next_id", id.0));
            }
            for child in self.children(id) {
                let parent = self.nodes.get(child).and_then(|child| child.parent);
                if parent != Some(id) {
                    return Err(format!("node {} has a wrong parent", child.0));
                }
                stack.push(*child);
            }
        }
        if seen.len() != self.nodes.len() {
            return Err("unreachable nodes".to_owned());
        }
        for layer in self.pixels.layers() {
            let is_pixel = matches!(
                self.nodes.get(&layer).map(|node| &node.kind),
                Some(NodeKind::Pixel { .. })
            );
            if !is_pixel {
                return Err(format!("pixels stored for non-pixel node {}", layer.0));
            }
        }
        Ok(())
    }
}
