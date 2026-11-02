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

use crate::adjust::{Adjustment, Filter};
use crate::pixels::{LayerTiles, PixelStore, TileFormat};

/// Version of the serialized model.
pub const MODEL_VERSION: u32 = 2;

/// Largest document side, in pixels.
pub const MAX_SIDE: u32 = 65_535;

/// Stable identifier of a node, unique within its document and never
/// reused.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct NodeId(pub u64);

/// Layer blend modes (CAL-3), following W3C Compositing and Blending
/// Level 1.
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

/// Largest nesting of groups and clipping sets the renderer composes in
/// one pass (its register stack, docs/decisions/0021). Commands refuse to
/// go beyond until the filter stages of step B10 lift the limit.
pub const MAX_NESTING: usize = 8;

/// How a group composes its children (docs/decisions/0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    /// Children act directly on what is below; the group opacity and mask
    /// mix the result with the backdrop. The default (Q2).
    #[default]
    PassThrough,
    /// Children are composed apart, then the result is blended with the
    /// group mode and opacity.
    Isolated,
}

/// Identifier of an embedded source: the hexadecimal SHA-256 of its
/// decoded content.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct SourceId(pub String);

/// Where an embedded object is placed. Only an integer offset in J3; the
/// affine transform of J5 extends this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Placement {
    /// Horizontal offset of the source origin, in document pixels.
    pub x: i32,
    /// Vertical offset of the source origin, in document pixels.
    pub y: i32,
}

/// An embedded image, stored once and never modified (DOC-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Name of the file it came from.
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bits per channel.
    pub depth: Depth,
    /// Its pixels, RGBA tiles in its own coordinates.
    pub tiles: LayerTiles,
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A group of nodes, drawn bottom to top.
    Group {
        /// Children, bottom first.
        children: Vec<NodeId>,
        /// How the children are composed.
        #[serde(default)]
        isolation: Isolation,
    },
    /// A pixel layer; its pixels live in the document [`PixelStore`].
    Pixel {
        /// Area the layer may hold pixels in.
        bounds: Rect,
    },
    /// An adjustment layer, acting on what is below it in its group.
    Adjustment {
        /// The adjustment and its parameters.
        adjustment: Adjustment,
    },
    /// A filter layer, acting on what is below it in its group.
    Filter {
        /// The filter and its parameters.
        filter: Filter,
    },
    /// An embedded object: an unmodified source, placed in the document.
    Embedded {
        /// The source, in [`Document::sources`].
        source: SourceId,
        /// Where it is placed.
        placement: Placement,
    },
    /// A gray mask, attached to the node that is its parent.
    Mask {
        /// Area the mask may hold pixels in.
        bounds: Rect,
        /// Effect of the mask, 0 (none) to 1 (full).
        density: f32,
        /// Whether the mask is inverted.
        inverted: bool,
        /// Whether the mask applies.
        enabled: bool,
        /// Gray value of the mask where no tile is stored.
        empty: u16,
    },
}

impl NodeKind {
    /// Whether nodes of this kind can carry a mask, be blended and appear
    /// in a group (every kind but masks).
    pub fn is_layer(&self) -> bool {
        !matches!(self, Self::Mask { .. })
    }

    /// Whether the node acts on what is below it instead of having pixels:
    /// adjustment and filter layers.
    pub fn is_effect(&self) -> bool {
        matches!(self, Self::Adjustment { .. } | Self::Filter { .. })
    }
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
    /// Mask node attached to this node.
    pub mask: Option<NodeId>,
    /// Clipped to the first unclipped node below it in its group (CAL-5).
    #[serde(default)]
    pub clip: bool,
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
    /// Embedded sources, by identifier. Stored with the pixels (J6).
    #[serde(skip)]
    pub sources: BTreeMap<SourceId, Source>,
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
                clip: false,
                kind: NodeKind::Group {
                    children: Vec::new(),
                    isolation: Isolation::Isolated,
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
            sources: BTreeMap::new(),
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

    /// Levels of groups and clipping sets nested in the document, as the
    /// renderer composes them (docs/decisions/0021): each group is one
    /// level, each clipping set (a base and the clipped layers above it)
    /// one more around its members. Hidden or empty nodes are counted too:
    /// this is the largest depth any tile can need.
    pub fn nesting_depth(&self) -> usize {
        self.inner_depth(self.root)
    }

    /// Largest nesting inside the children of `parent`.
    fn inner_depth(&self, parent: NodeId) -> usize {
        let layers: Vec<&Node> = self
            .children(parent)
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .filter(|node| node.kind.is_layer())
            .collect();
        let node_depth = |node: &Node| match node.kind {
            NodeKind::Group { .. } => 1 + self.inner_depth(node.id),
            _ => 0,
        };
        let mut depth = 0;
        let mut i = 0;
        while i < layers.len() {
            let mut end = i + 1;
            while end < layers.len() && layers[end].clip {
                end += 1;
            }
            let members = layers[i..end].iter().map(|node| node_depth(node)).max();
            let set = usize::from(end > i + 1);
            depth = depth.max(members.unwrap_or(0) + set);
            i = end;
        }
        depth
    }

    /// Clipped nodes whose base, the first unclipped layer below them in
    /// their group, is an adjustment or a filter (docs/decisions/0028).
    /// Commands never create such a node; a document read from elsewhere
    /// may contain one, and the renderer then shows it nowhere.
    pub fn clipped_onto_effects(&self) -> Vec<NodeId> {
        let mut found = Vec::new();
        let mut groups = vec![self.root];
        while let Some(group) = groups.pop() {
            let mut base: Option<&Node> = None;
            for node in self
                .children(group)
                .iter()
                .filter_map(|id| self.nodes.get(id))
                .filter(|node| node.kind.is_layer())
            {
                if matches!(node.kind, NodeKind::Group { .. }) {
                    groups.push(node.id);
                }
                if !node.clip || base.is_none() {
                    base = Some(node);
                } else if base.is_some_and(|base| base.kind.is_effect()) {
                    found.push(node.id);
                }
            }
        }
        found
    }

    /// Children of group `id`, bottom first; empty for other nodes.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        match self.nodes.get(&id).map(|node| &node.kind) {
            Some(NodeKind::Group { children, .. }) => children,
            _ => &[],
        }
    }

    /// Tile format of the pixels of node `id`: RGBA for pixel layers, gray
    /// for masks; `None` for nodes without pixels of their own.
    pub fn tile_format(&self, id: NodeId) -> Option<TileFormat> {
        match self.nodes.get(&id).map(|node| &node.kind) {
            Some(NodeKind::Pixel { .. }) => Some(TileFormat::rgba(self.depth)),
            Some(NodeKind::Mask { empty, .. }) => Some(TileFormat::gray(self.depth, *empty)),
            _ => None,
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
                let child_node = self.nodes.get(child);
                if child_node.and_then(|child| child.parent) != Some(id) {
                    return Err(format!("node {} has a wrong parent", child.0));
                }
                if child_node.is_some_and(|child| !child.kind.is_layer()) {
                    return Err(format!("mask {} listed as a child", child.0));
                }
                stack.push(*child);
            }
            if let Some(mask) = node.mask {
                let mask_node = self
                    .nodes
                    .get(&mask)
                    .ok_or_else(|| format!("node {} has an unknown mask", id.0))?;
                if mask_node.kind.is_layer() || mask_node.parent != Some(id) || id == self.root {
                    return Err(format!("node {} has an invalid mask {}", id.0, mask.0));
                }
                if mask_node.mask.is_some() {
                    return Err(format!("mask {} has a mask", mask.0));
                }
                stack.push(mask);
            }
            match &node.kind {
                NodeKind::Adjustment { adjustment } => adjustment
                    .validate()
                    .map_err(|error| format!("node {}: {error}", id.0))?,
                NodeKind::Filter { filter } => filter
                    .validate()
                    .map_err(|error| format!("node {}: {error}", id.0))?,
                NodeKind::Embedded { source, .. } if !self.sources.contains_key(source) => {
                    return Err(format!("node {} uses an unknown source", id.0));
                }
                NodeKind::Mask { density, .. } if !(0.0..=1.0).contains(density) => {
                    return Err(format!("mask {} has density {density}", id.0));
                }
                NodeKind::Mask { .. } => {
                    let owned = node
                        .parent
                        .and_then(|parent| self.nodes.get(&parent))
                        .is_some_and(|parent| parent.mask == Some(id));
                    if !owned {
                        return Err(format!("mask {} is not attached to its parent", id.0));
                    }
                }
                _ => {}
            }
        }
        if seen.len() != self.nodes.len() {
            return Err("unreachable nodes".to_owned());
        }
        for layer in self.pixels.layers() {
            let Some(format) = self.tile_format(layer) else {
                return Err(format!("pixels stored for node {} without pixels", layer.0));
            };
            if self
                .pixels
                .tiles(layer)
                .any(|(_, tile)| tile.channels() != format.channels)
            {
                return Err(format!("node {} stores tiles of the wrong kind", layer.0));
            }
        }
        Ok(())
    }
}
