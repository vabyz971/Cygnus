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

//! Commands of Cygnus Photo (docs/decisions/0010 and 0020).
//!
//! Every change to a [`Document`] is one of these commands, applied through
//! a [`cygnus_commands::CommandBus`]. Each command checks everything before
//! changing anything, and produces its inverse. Structural changes are
//! undone by restoring a snapshot of the nodes (and their pixels) they
//! touched, which makes their inverse exact by construction.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cygnus_commands::{Applied, Catalog, Command, CommandInfo, Stability};

use crate::adjust::{Adjustment, Filter};
use crate::changes::PhotoChanges;
use crate::model::{
    BlendMode, ColorProfile, Depth, Document, Isolation, MAX_SIDE, Node, NodeId, NodeKind, Rect,
};
use crate::pixels::{LayerTiles, TileBackup, TileFormat, backup_bytes};

/// Why a command was refused. A refused command changes nothing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoreError {
    /// No node has this identifier.
    #[error("unknown node {}", .0.0)]
    UnknownNode(NodeId),
    /// The node is not a pixel layer.
    #[error("node {} is not a pixel layer", .0.0)]
    NotAPixelLayer(NodeId),
    /// The node is not a group.
    #[error("node {} is not a group", .0.0)]
    NotAGroup(NodeId),
    /// The node is a mask, where a layer is expected.
    #[error("node {} is a mask, not a layer", .0.0)]
    NotALayer(NodeId),
    /// The node is not a mask.
    #[error("node {} is not a mask", .0.0)]
    NotAMask(NodeId),
    /// The node is not of the kind the command changes.
    #[error("node {} is not {}", .0.0, .1)]
    WrongKind(NodeId, &'static str),
    /// The node already has a mask.
    #[error("node {} already has a mask", .0.0)]
    AlreadyMasked(NodeId),
    /// The node has no mask.
    #[error("node {} has no mask", .0.0)]
    NoMask(NodeId),
    /// The root group cannot be changed this way.
    #[error("the root group cannot be changed")]
    RootNode,
    /// The node is locked.
    #[error("node {} is locked", .0.0)]
    Locked(NodeId),
    /// A parameter is out of its range.
    #[error("invalid {param}: {reason}")]
    InvalidValue {
        /// Parameter name.
        param: &'static str,
        /// What is wrong.
        reason: String,
    },
    /// The operation is not supported yet.
    #[error("{0} is not supported yet")]
    Unsupported(&'static str),
    /// A position in a group is out of range.
    #[error("index {index} is out of range (0..={len})")]
    IndexOutOfRange {
        /// Requested index.
        index: usize,
        /// Largest valid index.
        len: usize,
    },
    /// Moving a group into itself or one of its descendants.
    #[error("a group cannot be moved into itself")]
    Cycle,
}

fn invalid(param: &'static str, reason: impl Into<String>) -> CoreError {
    CoreError::InvalidValue {
        param,
        reason: reason.into(),
    }
}

/// Parameters of `document.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentCreate {
    /// Width in pixels, 1 to 65535.
    pub width: u32,
    /// Height in pixels, 1 to 65535.
    pub height: u32,
    /// Resolution in pixels per inch.
    #[serde(default = "default_resolution")]
    pub resolution: f32,
    /// Bits per channel.
    pub depth: Depth,
    /// Color profile.
    #[serde(default)]
    pub profile: ColorProfile,
}

fn default_resolution() -> f32 {
    72.0
}

/// Kind of a new layer, with its parameters.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerKind {
    /// An empty pixel layer.
    #[default]
    Pixel,
    /// An empty group.
    Group {
        /// How the group composes its children; pass-through by default.
        #[serde(default)]
        isolation: Isolation,
    },
    /// An adjustment layer.
    Adjustment {
        /// The adjustment and its parameters.
        adjustment: Adjustment,
    },
    /// A filter layer.
    Filter {
        /// The filter and its parameters.
        filter: Filter,
    },
}

/// Parameters of `layer.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerCreate {
    /// Name of the layer.
    #[serde(default)]
    pub name: String,
    /// Parent group; the root group when absent.
    #[serde(default)]
    pub parent: Option<NodeId>,
    /// Position in the parent, 0 being the bottom; the top when absent.
    #[serde(default)]
    pub index: Option<usize>,
    /// Kind of layer; a pixel layer when absent.
    #[serde(default)]
    pub kind: LayerKind,
}

/// Parameters of `layer.update`. Absent properties are left unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerUpdate {
    /// Layer to change.
    pub id: NodeId,
    /// New name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    /// New opacity (intensity of an adjustment), from 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// New blend mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<BlendMode>,
    /// New lock state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Clip to the layer below (CAL-5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<bool>,
    /// New isolation, for groups only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<Isolation>,
}

impl LayerUpdate {
    /// An update of `id` that changes nothing yet.
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            name: None,
            visible: None,
            opacity: None,
            blend: None,
            locked: None,
            clip: None,
            isolation: None,
        }
    }
}

/// Parameters of `layer.delete`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerDelete {
    /// Layer to delete, with everything it contains.
    pub id: NodeId,
}

/// Parameters of `layer.duplicate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerDuplicate {
    /// Layer to copy, with everything it contains; the copy goes just above.
    pub id: NodeId,
}

/// Parameters of `layer.move`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerMove {
    /// Layer to move.
    pub id: NodeId,
    /// New parent group; the root group when absent.
    #[serde(default)]
    pub parent: Option<NodeId>,
    /// New position in the parent, 0 being the bottom, counted once the
    /// layer is removed from its current place.
    pub index: usize,
}

/// Parameters of `layer.group`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerGroup {
    /// Layers to group, all in the same group.
    pub ids: Vec<NodeId>,
    /// Name of the new group.
    #[serde(default)]
    pub name: String,
    /// How the new group composes its children; pass-through by default.
    #[serde(default)]
    pub isolation: Isolation,
}

/// Parameters of `layer.ungroup`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerUngroup {
    /// Group to undo: its children take its place, its mask is removed.
    pub id: NodeId,
}

/// Parameters of `adjustment.update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdjustmentUpdate {
    /// Adjustment layer to change.
    pub id: NodeId,
    /// New adjustment and parameters.
    pub adjustment: Adjustment,
}

/// Parameters of `filter.update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FilterUpdate {
    /// Filter layer to change.
    pub id: NodeId,
    /// New filter and parameters.
    pub filter: Filter,
}

/// Initial content of a new mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaskFill {
    /// White: everything shows.
    #[default]
    White,
    /// Black: everything is hidden.
    Black,
}

/// Parameters of `mask.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaskCreate {
    /// Layer or group receiving the mask.
    pub node: NodeId,
    /// Initial content.
    #[serde(default)]
    pub fill: MaskFill,
}

/// Parameters of `mask.delete`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaskDelete {
    /// Layer or group whose mask is removed.
    pub node: NodeId,
}

/// Parameters of `mask.update`. Absent properties are left unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaskUpdate {
    /// Mask to change.
    pub id: NodeId,
    /// Effect of the mask, 0 (none) to 1 (full).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<f32>,
    /// Inversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverted: Option<bool>,
    /// Whether the mask applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Parameters of `mask.fill`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaskFillRect {
    /// Mask to fill.
    pub mask: NodeId,
    /// Area to fill, clipped to the mask.
    pub rect: Rect,
    /// Gray value, 0 (black) to 1 (white).
    pub value: f32,
}

/// Parameters of `mask.gradient`: a linear gradient over the whole mask
/// (MAS-1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaskGradient {
    /// Mask to fill.
    pub mask: NodeId,
    /// Start point, in document pixels.
    pub start: [f32; 2],
    /// End point, in document pixels.
    pub end: [f32; 2],
    /// Gray value at the start and before it, 0 to 1.
    pub start_value: f32,
    /// Gray value at the end and after it, 0 to 1.
    pub end_value: f32,
}

/// Parameters of `pixels.fill`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PixelsFill {
    /// Pixel layer to fill.
    pub layer: NodeId,
    /// Area to fill, clipped to the layer.
    pub rect: Rect,
    /// Color as straight RGBA in the document profile, each channel from 0
    /// to 1.
    pub color: [f32; 4],
}

/// Parameters of `pixels.write`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PixelsWrite {
    /// Pixel layer to write.
    pub layer: NodeId,
    /// Area to write, inside the layer.
    pub rect: Rect,
    /// Straight RGBA values in the document depth and profile, row by row,
    /// `rect.width * rect.height * 4` values.
    pub samples: Vec<u16>,
}

/// Saved state of some nodes and of their pixels: the inverse of
/// structural commands. Each entry holds the node as it was (`None` if it
/// did not exist) and its tiles.
#[derive(Debug, Clone, PartialEq)]
pub struct NodesRestore {
    entries: Vec<(NodeId, Option<Node>, LayerTiles)>,
    /// After the entries are restored, put this node back at this index of
    /// this group (inverse of a deletion, without touching its siblings).
    insert: Option<(NodeId, usize, NodeId)>,
}

/// Restores overwritten tiles (inverse of the pixel and mask fills).
#[derive(Debug, Clone, PartialEq)]
pub struct PixelsRestore {
    layer: NodeId,
    backup: TileBackup,
}

/// A command of Cygnus Photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "params")]
pub enum PhotoCommand {
    /// Replaces the document with a new, empty one.
    #[serde(rename = "document.create")]
    DocumentCreate(DocumentCreate),
    /// Adds a layer, group, adjustment or filter.
    #[serde(rename = "layer.create")]
    LayerCreate(LayerCreate),
    /// Changes properties of a layer.
    #[serde(rename = "layer.update")]
    LayerUpdate(LayerUpdate),
    /// Deletes a layer and everything it contains.
    #[serde(rename = "layer.delete")]
    LayerDelete(LayerDelete),
    /// Copies a layer and everything it contains.
    #[serde(rename = "layer.duplicate")]
    LayerDuplicate(LayerDuplicate),
    /// Moves a layer in the tree.
    #[serde(rename = "layer.move")]
    LayerMove(LayerMove),
    /// Groups layers.
    #[serde(rename = "layer.group")]
    LayerGroup(LayerGroup),
    /// Undoes a group.
    #[serde(rename = "layer.ungroup")]
    LayerUngroup(LayerUngroup),
    /// Changes the parameters of an adjustment layer.
    #[serde(rename = "adjustment.update")]
    AdjustmentUpdate(AdjustmentUpdate),
    /// Changes the parameters of a filter layer.
    #[serde(rename = "filter.update")]
    FilterUpdate(FilterUpdate),
    /// Adds a mask to a layer or group.
    #[serde(rename = "mask.create")]
    MaskCreate(MaskCreate),
    /// Removes the mask of a layer or group.
    #[serde(rename = "mask.delete")]
    MaskDelete(MaskDelete),
    /// Changes the density, inversion or activation of a mask.
    #[serde(rename = "mask.update")]
    MaskUpdate(MaskUpdate),
    /// Fills a rectangle of a mask with a gray value.
    #[serde(rename = "mask.fill")]
    MaskFill(MaskFillRect),
    /// Fills a mask with a linear gradient.
    #[serde(rename = "mask.gradient")]
    MaskGradient(MaskGradient),
    /// Development: fills a rectangle of a pixel layer with a color.
    #[serde(rename = "pixels.fill")]
    PixelsFill(PixelsFill),
    /// Development: writes a block of pixels into a pixel layer.
    #[serde(rename = "pixels.write")]
    PixelsWrite(PixelsWrite),
    /// Internal: inverse of `document.create`.
    #[serde(skip)]
    DocumentRestore(Box<Document>),
    /// Internal: inverse of structural commands.
    #[serde(skip)]
    NodesRestore(Box<NodesRestore>),
    /// Internal: inverse of the pixel and mask fills.
    #[serde(skip)]
    PixelsRestore(PixelsRestore),
}

type Outcome = Result<Applied<PhotoCommand>, CoreError>;

fn applied(inverse: PhotoCommand, inverse_bytes: usize, changes: PhotoChanges) -> Outcome {
    Ok(Applied {
        inverse,
        inverse_bytes,
        changes,
    })
}

fn tiles_bytes(tiles: &LayerTiles) -> usize {
    tiles.values().map(|tile| tile.bytes()).sum()
}

fn store_bytes(document: &Document) -> usize {
    document
        .pixels
        .layers()
        .flat_map(|layer| document.pixels.tiles(layer))
        .map(|(_, tile)| tile.bytes())
        .sum()
}

/// The node `id`, which must exist and not be the root.
fn any_node(document: &Document, id: NodeId) -> Result<&Node, CoreError> {
    if id == document.root {
        return Err(CoreError::RootNode);
    }
    document.node(id).ok_or(CoreError::UnknownNode(id))
}

/// The layer `id` (any kind but a mask), which must exist and not be the
/// root.
fn layer_node(document: &Document, id: NodeId) -> Result<&Node, CoreError> {
    let node = any_node(document, id)?;
    if !node.kind.is_layer() {
        return Err(CoreError::NotALayer(id));
    }
    Ok(node)
}

/// Like [`layer_node`], refusing a locked layer.
fn unlocked_layer(document: &Document, id: NodeId) -> Result<&Node, CoreError> {
    let node = layer_node(document, id)?;
    if node.locked {
        return Err(CoreError::Locked(id));
    }
    Ok(node)
}

/// The pixel layer `id`, which must exist, not be locked, and have bounds.
fn editable_pixel_layer(document: &Document, id: NodeId) -> Result<Rect, CoreError> {
    let node = layer_node(document, id)?;
    let NodeKind::Pixel { bounds } = node.kind else {
        return Err(CoreError::NotAPixelLayer(id));
    };
    if node.locked {
        return Err(CoreError::Locked(id));
    }
    Ok(bounds)
}

/// The mask `id` and its bounds; its owner must not be locked.
fn editable_mask(document: &Document, id: NodeId) -> Result<Rect, CoreError> {
    let node = any_node(document, id)?;
    let NodeKind::Mask { bounds, .. } = node.kind else {
        return Err(CoreError::NotAMask(id));
    };
    let owner = node.parent.ok_or(CoreError::NotAMask(id))?;
    if document.node(owner).is_some_and(|owner| owner.locked) {
        return Err(CoreError::Locked(owner));
    }
    Ok(bounds)
}

/// The group `id` (the root when `None`), which must exist and be unlocked.
fn target_group(document: &Document, id: Option<NodeId>) -> Result<NodeId, CoreError> {
    let id = id.unwrap_or(document.root);
    let node = document.node(id).ok_or(CoreError::UnknownNode(id))?;
    if !matches!(node.kind, NodeKind::Group { .. }) {
        return Err(CoreError::NotAGroup(id));
    }
    if node.locked {
        return Err(CoreError::Locked(id));
    }
    Ok(id)
}

fn children_mut(document: &mut Document, group: NodeId) -> Option<&mut Vec<NodeId>> {
    match document.nodes.get_mut(&group).map(|node| &mut node.kind) {
        Some(NodeKind::Group { children, .. }) => Some(children),
        _ => None,
    }
}

fn check_index(index: usize, len: usize) -> Result<(), CoreError> {
    if index > len {
        return Err(CoreError::IndexOutOfRange { index, len });
    }
    Ok(())
}

fn check_unit(param: &'static str, value: f32) -> Result<(), CoreError> {
    if !(value.is_finite() && (0.0..=1.0).contains(&value)) {
        return Err(invalid(param, format!("{value} is not in 0..=1")));
    }
    Ok(())
}

/// Position of `id` among the children of its parent.
fn position(document: &Document, id: NodeId) -> Result<(NodeId, usize), CoreError> {
    let parent = document
        .node(id)
        .and_then(|node| node.parent)
        .ok_or(CoreError::RootNode)?;
    let index = document
        .children(parent)
        .iter()
        .position(|child| *child == id)
        .ok_or(CoreError::UnknownNode(id))?;
    Ok((parent, index))
}

/// `id` and every node below it: children, recursively, and masks.
fn subtree(document: &Document, id: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![id];
    while let Some(current) = stack.pop() {
        found.push(current);
        if let Some(node) = document.node(current) {
            stack.extend(node.mask);
        }
        stack.extend(document.children(current).iter().copied());
    }
    found
}

/// Saves the current state of `ids`: their nodes (or their absence) and
/// their tiles.
fn snapshot(document: &Document, ids: impl IntoIterator<Item = NodeId>) -> NodesRestore {
    let mut entries: Vec<(NodeId, Option<Node>, LayerTiles)> = Vec::new();
    for id in ids {
        if entries.iter().any(|(saved, _, _)| *saved == id) {
            continue;
        }
        let tiles: LayerTiles = document
            .pixels
            .tiles(id)
            .map(|(coord, tile)| (coord, tile.clone()))
            .collect();
        entries.push((id, document.node(id).cloned(), tiles));
    }
    NodesRestore {
        entries,
        insert: None,
    }
}

fn restored(document: &Document, restore: NodesRestore, mut changes: PhotoChanges) -> Outcome {
    let _ = document;
    let bytes = restore
        .entries
        .iter()
        .map(|(_, _, tiles)| tiles_bytes(tiles))
        .sum();
    changes.structure = true;
    applied(
        PhotoCommand::NodesRestore(Box::new(restore)),
        bytes,
        changes,
    )
}

fn restore_nodes(document: &mut Document, restore: NodesRestore) -> Outcome {
    let ids: Vec<NodeId> = restore.entries.iter().map(|(id, _, _)| *id).collect();
    let inverse = snapshot(document, ids);
    let insert = restore.insert;
    for (id, node, tiles) in restore.entries {
        match node {
            Some(node) => {
                document.nodes.insert(id, node);
            }
            None => {
                document.nodes.remove(&id);
            }
        }
        document.pixels.insert_layer(id, tiles);
    }
    if let Some((parent, index, child)) = insert {
        if let Some(children) = children_mut(document, parent) {
            let index = index.min(children.len());
            children.insert(index, child);
        }
        // Undoing the restoration deletes the node again.
        return applied(
            PhotoCommand::LayerDelete(LayerDelete { id: child }),
            0,
            PhotoChanges::structure(),
        );
    }
    restored(document, inverse, PhotoChanges::structure())
}

fn create_document(document: &mut Document, params: DocumentCreate) -> Outcome {
    for (param, side) in [("width", params.width), ("height", params.height)] {
        if side == 0 || side > MAX_SIDE {
            return Err(invalid(param, format!("{side} is not in 1..={MAX_SIDE}")));
        }
    }
    if !params.resolution.is_finite() || params.resolution <= 0.0 {
        return Err(invalid("resolution", "must be a positive number"));
    }
    let new = Document::new(
        params.width,
        params.height,
        params.resolution,
        params.depth,
        params.profile,
    );
    let previous = std::mem::replace(document, new);
    let bytes = store_bytes(&previous);
    applied(
        PhotoCommand::DocumentRestore(Box::new(previous)),
        bytes,
        cygnus_commands::ChangeSet::everything(),
    )
}

fn restore_document(document: &mut Document, previous: Document) -> Outcome {
    let replaced = std::mem::replace(document, previous);
    let bytes = store_bytes(&replaced);
    applied(
        PhotoCommand::DocumentRestore(Box::new(replaced)),
        bytes,
        cygnus_commands::ChangeSet::everything(),
    )
}

/// A new node with default properties, not yet in the tree.
fn new_node(document: &mut Document, parent: NodeId, name: String, kind: NodeKind) -> Node {
    let id = document.peek_next_id();
    document.next_id += 1;
    Node {
        id,
        parent: Some(parent),
        name,
        visible: true,
        opacity: 1.0,
        blend: BlendMode::Normal,
        locked: false,
        mask: None,
        clip: false,
        kind,
    }
}

fn create_layer(document: &mut Document, params: LayerCreate) -> Outcome {
    let parent = target_group(document, params.parent)?;
    let len = document.children(parent).len();
    let index = params.index.unwrap_or(len);
    check_index(index, len)?;
    let kind = match params.kind {
        LayerKind::Pixel => NodeKind::Pixel {
            bounds: document.bounds(),
        },
        LayerKind::Group { isolation } => NodeKind::Group {
            children: Vec::new(),
            isolation,
        },
        LayerKind::Adjustment { adjustment } => {
            adjustment
                .validate()
                .map_err(|reason| invalid("adjustment", reason))?;
            NodeKind::Adjustment { adjustment }
        }
        LayerKind::Filter { filter } => {
            filter
                .validate()
                .map_err(|reason| invalid("filter", reason))?;
            NodeKind::Filter { filter }
        }
    };

    let node = new_node(document, parent, params.name, kind);
    let id = node.id;
    document.nodes.insert(id, node);
    if let Some(children) = children_mut(document, parent) {
        children.insert(index, id);
    }
    let mut changes = PhotoChanges::structure();
    changes.created.push(id);
    applied(PhotoCommand::LayerDelete(LayerDelete { id }), 0, changes)
}

fn delete_layer(document: &mut Document, params: LayerDelete) -> Outcome {
    let id = params.id;
    unlocked_layer(document, id)?;
    let (parent, index) = position(document, id)?;
    let nodes = subtree(document, id);
    let mut saved = snapshot(document, nodes.iter().copied());
    saved.insert = Some((parent, index, id));

    if let Some(children) = children_mut(document, parent) {
        children.remove(index);
    }
    for node in nodes {
        document.nodes.remove(&node);
        document.pixels.take_layer(node);
    }
    restored(document, saved, PhotoChanges::structure())
}

fn duplicate_layer(document: &mut Document, params: LayerDuplicate) -> Outcome {
    let id = params.id;
    layer_node(document, id)?;
    let (parent, index) = position(document, id)?;
    target_group(document, Some(parent))?;

    // New identifiers, in tree order, for every node of the subtree.
    let originals = subtree(document, id);
    let mut mapping = std::collections::BTreeMap::new();
    for original in &originals {
        mapping.insert(*original, NodeId(document.next_id));
        document.next_id += 1;
    }
    let map = |old: NodeId| mapping.get(&old).copied().unwrap_or(old);
    for original in &originals {
        let Some(mut node) = document.node(*original).cloned() else {
            continue;
        };
        node.id = map(node.id);
        node.parent = if *original == id {
            Some(parent)
        } else {
            node.parent.map(map)
        };
        node.mask = node.mask.map(map);
        if let NodeKind::Group { children, .. } = &mut node.kind {
            for child in children.iter_mut() {
                *child = map(*child);
            }
        }
        let tiles: LayerTiles = document
            .pixels
            .tiles(*original)
            .map(|(coord, tile)| (coord, tile.clone()))
            .collect();
        document.pixels.insert_layer(node.id, tiles);
        document.nodes.insert(node.id, node);
    }
    let copy = map(id);
    if let Some(children) = children_mut(document, parent) {
        children.insert(index + 1, copy);
    }
    let mut changes = PhotoChanges::structure();
    changes.created.push(copy);
    applied(
        PhotoCommand::LayerDelete(LayerDelete { id: copy }),
        0,
        changes,
    )
}

fn update_layer(document: &mut Document, params: LayerUpdate) -> Outcome {
    let id = params.id;
    let node = layer_node(document, id)?;
    let changes_content = params.name.is_some()
        || params.opacity.is_some()
        || params.blend.is_some()
        || params.clip.is_some()
        || params.isolation.is_some();
    // A locked layer refuses content changes, unless the same command
    // unlocks it (which is also how the inverse of a locking update works).
    let stays_locked = node.locked && params.locked != Some(false);
    if stays_locked && changes_content {
        return Err(CoreError::Locked(id));
    }
    if let Some(opacity) = params.opacity {
        check_unit("opacity", opacity)?;
    }
    let isolation = match (&node.kind, params.isolation) {
        (_, None) => None,
        (NodeKind::Group { isolation, .. }, Some(_)) => Some(*isolation),
        (_, Some(_)) => return Err(CoreError::NotAGroup(id)),
    };

    let inverse = LayerUpdate {
        id,
        name: params.name.as_ref().map(|_| node.name.clone()),
        visible: params.visible.map(|_| node.visible),
        opacity: params.opacity.map(|_| node.opacity),
        blend: params.blend.map(|_| node.blend),
        locked: params.locked.map(|_| node.locked),
        clip: params.clip.map(|_| node.clip),
        isolation,
    };
    let node = document
        .nodes
        .get_mut(&id)
        .ok_or(CoreError::UnknownNode(id))?;
    if let Some(name) = params.name {
        node.name = name;
    }
    if let Some(visible) = params.visible {
        node.visible = visible;
    }
    if let Some(opacity) = params.opacity {
        node.opacity = opacity;
    }
    if let Some(blend) = params.blend {
        node.blend = blend;
    }
    if let Some(locked) = params.locked {
        node.locked = locked;
    }
    if let Some(clip) = params.clip {
        node.clip = clip;
    }
    if let (Some(new), NodeKind::Group { isolation, .. }) = (params.isolation, &mut node.kind) {
        *isolation = new;
    }
    applied(
        PhotoCommand::LayerUpdate(inverse),
        0,
        PhotoChanges::properties(id),
    )
}

fn move_layer(document: &mut Document, params: LayerMove) -> Outcome {
    let id = params.id;
    unlocked_layer(document, id)?;
    let (old_parent, old_index) = position(document, id)?;
    let new_parent = target_group(document, params.parent)?;
    // The new parent must not be the node itself or one of its descendants.
    let mut ancestor = Some(new_parent);
    while let Some(current) = ancestor {
        if current == id {
            return Err(CoreError::Cycle);
        }
        ancestor = document.node(current).and_then(|node| node.parent);
    }
    let len_after_removal =
        document.children(new_parent).len() - usize::from(new_parent == old_parent);
    check_index(params.index, len_after_removal)?;

    if let Some(children) = children_mut(document, old_parent) {
        children.remove(old_index);
    }
    if let Some(children) = children_mut(document, new_parent) {
        children.insert(params.index, id);
    }
    if let Some(node) = document.nodes.get_mut(&id) {
        node.parent = Some(new_parent);
    }
    applied(
        PhotoCommand::LayerMove(LayerMove {
            id,
            parent: Some(old_parent),
            index: old_index,
        }),
        0,
        PhotoChanges::structure(),
    )
}

fn group_layers(document: &mut Document, params: LayerGroup) -> Outcome {
    let Some(first) = params.ids.first().copied() else {
        return Err(invalid("ids", "at least one layer is needed"));
    };
    let (parent, _) = position(document, first)?;
    target_group(document, Some(parent))?;
    for (index, id) in params.ids.iter().enumerate() {
        unlocked_layer(document, *id)?;
        if params.ids[..index].contains(id) {
            return Err(invalid("ids", "a layer is listed twice"));
        }
        if position(document, *id)?.0 != parent {
            return Err(invalid("ids", "the layers must be in the same group"));
        }
    }

    let group_id = document.peek_next_id();
    let saved = snapshot(
        document,
        [parent, group_id]
            .into_iter()
            .chain(params.ids.iter().copied()),
    );
    let siblings = document.children(parent).to_vec();
    // Selected layers keep their order; the group takes the place of the
    // topmost one.
    let members: Vec<NodeId> = siblings
        .iter()
        .copied()
        .filter(|child| params.ids.contains(child))
        .collect();
    let top = members.last().copied().unwrap_or(first);
    let mut remaining = Vec::with_capacity(siblings.len());
    for child in siblings {
        if child == top {
            remaining.push(group_id);
        } else if !params.ids.contains(&child) {
            remaining.push(child);
        }
    }
    let group = new_node(
        document,
        parent,
        params.name,
        NodeKind::Group {
            children: members.clone(),
            isolation: params.isolation,
        },
    );
    document.nodes.insert(group_id, group);
    for member in members {
        if let Some(node) = document.nodes.get_mut(&member) {
            node.parent = Some(group_id);
        }
    }
    if let Some(children) = children_mut(document, parent) {
        *children = remaining;
    }
    let mut changes = PhotoChanges::structure();
    changes.created.push(group_id);
    restored(document, saved, changes)
}

fn ungroup_layer(document: &mut Document, params: LayerUngroup) -> Outcome {
    let id = params.id;
    let node = unlocked_layer(document, id)?;
    let NodeKind::Group { children, .. } = &node.kind else {
        return Err(CoreError::NotAGroup(id));
    };
    let members = children.clone();
    let mask = node.mask;
    let (parent, index) = position(document, id)?;
    target_group(document, Some(parent))?;
    let saved = snapshot(
        document,
        [parent, id]
            .into_iter()
            .chain(mask)
            .chain(members.iter().copied()),
    );

    for member in &members {
        if let Some(node) = document.nodes.get_mut(member) {
            node.parent = Some(parent);
        }
    }
    if let Some(children) = children_mut(document, parent) {
        children.splice(index..=index, members);
    }
    document.nodes.remove(&id);
    if let Some(mask) = mask {
        document.nodes.remove(&mask);
        document.pixels.take_layer(mask);
    }
    restored(document, saved, PhotoChanges::structure())
}

fn update_adjustment(document: &mut Document, params: AdjustmentUpdate) -> Outcome {
    let id = params.id;
    let node = unlocked_layer(document, id)?;
    let NodeKind::Adjustment { adjustment } = &node.kind else {
        return Err(CoreError::WrongKind(id, "an adjustment layer"));
    };
    params
        .adjustment
        .validate()
        .map_err(|reason| invalid("adjustment", reason))?;
    let previous = adjustment.clone();
    if let Some(Node {
        kind: NodeKind::Adjustment { adjustment },
        ..
    }) = document.nodes.get_mut(&id)
    {
        *adjustment = params.adjustment;
    }
    applied(
        PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
            id,
            adjustment: previous,
        }),
        0,
        PhotoChanges::properties(id),
    )
}

fn update_filter(document: &mut Document, params: FilterUpdate) -> Outcome {
    let id = params.id;
    let node = unlocked_layer(document, id)?;
    let NodeKind::Filter { filter } = &node.kind else {
        return Err(CoreError::WrongKind(id, "a filter layer"));
    };
    params
        .filter
        .validate()
        .map_err(|reason| invalid("filter", reason))?;
    let previous = *filter;
    if let Some(Node {
        kind: NodeKind::Filter { filter },
        ..
    }) = document.nodes.get_mut(&id)
    {
        *filter = params.filter;
    }
    applied(
        PhotoCommand::FilterUpdate(FilterUpdate {
            id,
            filter: previous,
        }),
        0,
        PhotoChanges::properties(id),
    )
}

fn create_mask(document: &mut Document, params: MaskCreate) -> Outcome {
    let owner = params.node;
    let node = unlocked_layer(document, owner)?;
    if node.mask.is_some() {
        return Err(CoreError::AlreadyMasked(owner));
    }
    let empty = match params.fill {
        MaskFill::White => document.depth.max_value(),
        MaskFill::Black => 0,
    };
    let mask = new_node(
        document,
        owner,
        String::new(),
        NodeKind::Mask {
            bounds: document.bounds(),
            density: 1.0,
            inverted: false,
            enabled: true,
            empty,
        },
    );
    let mask_id = mask.id;
    document.nodes.insert(mask_id, mask);
    if let Some(node) = document.nodes.get_mut(&owner) {
        node.mask = Some(mask_id);
    }
    let mut changes = PhotoChanges::properties(owner);
    changes.structure = true;
    changes.created.push(mask_id);
    applied(
        PhotoCommand::MaskDelete(MaskDelete { node: owner }),
        0,
        changes,
    )
}

fn delete_mask(document: &mut Document, params: MaskDelete) -> Outcome {
    let owner = params.node;
    let node = unlocked_layer(document, owner)?;
    let mask = node.mask.ok_or(CoreError::NoMask(owner))?;
    let saved = snapshot(document, [owner, mask]);
    document.nodes.remove(&mask);
    document.pixels.take_layer(mask);
    if let Some(node) = document.nodes.get_mut(&owner) {
        node.mask = None;
    }
    restored(document, saved, PhotoChanges::properties(owner))
}

fn update_mask(document: &mut Document, params: MaskUpdate) -> Outcome {
    let id = params.id;
    editable_mask(document, id)?;
    if let Some(density) = params.density {
        check_unit("density", density)?;
    }
    let owner = document.node(id).and_then(|node| node.parent);
    let Some(Node {
        kind:
            NodeKind::Mask {
                density,
                inverted,
                enabled,
                ..
            },
        ..
    }) = document.nodes.get_mut(&id)
    else {
        return Err(CoreError::NotAMask(id));
    };
    let inverse = MaskUpdate {
        id,
        density: params.density.map(|_| *density),
        inverted: params.inverted.map(|_| *inverted),
        enabled: params.enabled.map(|_| *enabled),
    };
    if let Some(value) = params.density {
        *density = value;
    }
    if let Some(value) = params.inverted {
        *inverted = value;
    }
    if let Some(value) = params.enabled {
        *enabled = value;
    }
    let mut changes = PhotoChanges::properties(id);
    changes.properties.extend(owner);
    applied(PhotoCommand::MaskUpdate(inverse), 0, changes)
}

fn pixels_changed(layer: NodeId, backup: &TileBackup) -> PhotoChanges {
    PhotoChanges::pixels(layer, backup.iter().map(|(coord, _)| *coord))
}

fn painted(layer: NodeId, backup: TileBackup) -> Outcome {
    let changes = pixels_changed(layer, &backup);
    let bytes = backup_bytes(&backup);
    applied(
        PhotoCommand::PixelsRestore(PixelsRestore { layer, backup }),
        bytes,
        changes,
    )
}

fn quantize(document: &Document, value: f32) -> u16 {
    // Rounded to the nearest representable value of the document depth.
    (value * f32::from(document.depth.max_value())).round() as u16
}

fn fill_mask(document: &mut Document, params: MaskFillRect) -> Outcome {
    let bounds = editable_mask(document, params.mask)?;
    check_unit("value", params.value)?;
    let format = document
        .tile_format(params.mask)
        .ok_or(CoreError::NotAMask(params.mask))?;
    let gray = quantize(document, params.value);
    let area = params.rect.intersect(&bounds);
    let backup = document
        .pixels
        .paint(params.mask, area, format, |_, _| [gray; 4]);
    painted(params.mask, backup)
}

fn gradient_mask(document: &mut Document, params: MaskGradient) -> Outcome {
    let bounds = editable_mask(document, params.mask)?;
    check_unit("start_value", params.start_value)?;
    check_unit("end_value", params.end_value)?;
    let [sx, sy] = params.start;
    let [ex, ey] = params.end;
    if ![sx, sy, ex, ey].iter().all(|v| v.is_finite()) {
        return Err(invalid("start", "points must be finite"));
    }
    let (dx, dy) = (ex - sx, ey - sy);
    let length2 = dx * dx + dy * dy;
    if length2 <= 0.0 {
        return Err(invalid("end", "must differ from the start"));
    }
    let format = document
        .tile_format(params.mask)
        .ok_or(CoreError::NotAMask(params.mask))?;
    let max = f32::from(document.depth.max_value());
    let backup = document.pixels.paint(params.mask, bounds, format, |x, y| {
        // Projection of the pixel center on the gradient axis.
        let t =
            (((x as f32 + 0.5 - sx) * dx + (y as f32 + 0.5 - sy) * dy) / length2).clamp(0.0, 1.0);
        let value = params.start_value + (params.end_value - params.start_value) * t;
        [(value * max).round() as u16; 4]
    });
    painted(params.mask, backup)
}

fn fill_pixels(document: &mut Document, params: PixelsFill) -> Outcome {
    let bounds = editable_pixel_layer(document, params.layer)?;
    for channel in params.color {
        check_unit("color", channel)?;
    }
    let rgba = params.color.map(|channel| quantize(document, channel));
    let area = params.rect.intersect(&bounds);
    let format = TileFormat::rgba(document.depth);
    let backup = document
        .pixels
        .paint(params.layer, area, format, |_, _| rgba);
    painted(params.layer, backup)
}

fn write_pixels(document: &mut Document, params: PixelsWrite) -> Outcome {
    let bounds = editable_pixel_layer(document, params.layer)?;
    let rect = params.rect;
    if rect.intersect(&bounds) != rect {
        return Err(invalid("rect", "must lie inside the layer"));
    }
    let expected = u64::from(rect.width) * u64::from(rect.height) * 4;
    if params.samples.len() as u64 != expected {
        return Err(invalid(
            "samples",
            format!("{} values given, {expected} expected", params.samples.len()),
        ));
    }
    let max = document.depth.max_value();
    if params.samples.iter().any(|value| *value > max) {
        return Err(invalid("samples", format!("values must not exceed {max}")));
    }
    let width = rect.width as usize;
    let samples = &params.samples;
    let format = TileFormat::rgba(document.depth);
    let backup = document.pixels.paint(params.layer, rect, format, |x, y| {
        let i = (((y - rect.y) as usize) * width + (x - rect.x) as usize) * 4;
        [samples[i], samples[i + 1], samples[i + 2], samples[i + 3]]
    });
    painted(params.layer, backup)
}

fn restore_pixels(document: &mut Document, restore: PixelsRestore) -> Outcome {
    // Restoring old content is allowed on any existing node with pixels,
    // even locked: it only replays history.
    any_node(document, restore.layer)?;
    if document.tile_format(restore.layer).is_none() {
        return Err(CoreError::NotAPixelLayer(restore.layer));
    }
    let replaced = document.pixels.restore(restore.layer, restore.backup);
    painted(restore.layer, replaced)
}

impl Command for PhotoCommand {
    type Target = Document;
    type Changes = PhotoChanges;
    type Error = CoreError;

    fn name(&self) -> &'static str {
        match self {
            Self::DocumentCreate(_) => "document.create",
            Self::LayerCreate(_) => "layer.create",
            Self::LayerUpdate(_) => "layer.update",
            Self::LayerDelete(_) => "layer.delete",
            Self::LayerDuplicate(_) => "layer.duplicate",
            Self::LayerMove(_) => "layer.move",
            Self::LayerGroup(_) => "layer.group",
            Self::LayerUngroup(_) => "layer.ungroup",
            Self::AdjustmentUpdate(_) => "adjustment.update",
            Self::FilterUpdate(_) => "filter.update",
            Self::MaskCreate(_) => "mask.create",
            Self::MaskDelete(_) => "mask.delete",
            Self::MaskUpdate(_) => "mask.update",
            Self::MaskFill(_) => "mask.fill",
            Self::MaskGradient(_) => "mask.gradient",
            Self::PixelsFill(_) => "pixels.fill",
            Self::PixelsWrite(_) => "pixels.write",
            Self::DocumentRestore(_) => "document.restore",
            Self::NodesRestore(_) => "nodes.restore",
            Self::PixelsRestore(_) => "pixels.restore",
        }
    }

    fn payload_bytes(&self) -> usize {
        match self {
            Self::PixelsWrite(params) => params.samples.len() * size_of::<u16>(),
            _ => 0,
        }
    }

    fn apply(self, document: &mut Document) -> Outcome {
        match self {
            Self::DocumentCreate(params) => create_document(document, params),
            Self::LayerCreate(params) => create_layer(document, params),
            Self::LayerUpdate(params) => update_layer(document, params),
            Self::LayerDelete(params) => delete_layer(document, params),
            Self::LayerDuplicate(params) => duplicate_layer(document, params),
            Self::LayerMove(params) => move_layer(document, params),
            Self::LayerGroup(params) => group_layers(document, params),
            Self::LayerUngroup(params) => ungroup_layer(document, params),
            Self::AdjustmentUpdate(params) => update_adjustment(document, params),
            Self::FilterUpdate(params) => update_filter(document, params),
            Self::MaskCreate(params) => create_mask(document, params),
            Self::MaskDelete(params) => delete_mask(document, params),
            Self::MaskUpdate(params) => update_mask(document, params),
            Self::MaskFill(params) => fill_mask(document, params),
            Self::MaskGradient(params) => gradient_mask(document, params),
            Self::PixelsFill(params) => fill_pixels(document, params),
            Self::PixelsWrite(params) => write_pixels(document, params),
            Self::DocumentRestore(previous) => restore_document(document, *previous),
            Self::NodesRestore(restore) => restore_nodes(document, *restore),
            Self::PixelsRestore(restore) => restore_pixels(document, restore),
        }
    }
}

impl Catalog for PhotoCommand {
    fn commands() -> Vec<CommandInfo> {
        use Stability::{Development, Stable};
        vec![
            CommandInfo::new::<DocumentCreate>(
                "document.create",
                "Replaces the document with a new, empty one.",
                Stable,
            ),
            CommandInfo::new::<LayerCreate>(
                "layer.create",
                "Adds a pixel layer, group, adjustment layer or filter layer to a group.",
                Stable,
            ),
            CommandInfo::new::<LayerUpdate>(
                "layer.update",
                "Changes the name, visibility, opacity, blend mode, lock, clipping or isolation of a layer.",
                Stable,
            ),
            CommandInfo::new::<LayerDelete>(
                "layer.delete",
                "Deletes a layer and everything it contains.",
                Stable,
            ),
            CommandInfo::new::<LayerDuplicate>(
                "layer.duplicate",
                "Copies a layer and everything it contains just above it.",
                Stable,
            ),
            CommandInfo::new::<LayerMove>(
                "layer.move",
                "Moves a layer to another position or group.",
                Stable,
            ),
            CommandInfo::new::<LayerGroup>(
                "layer.group",
                "Puts layers of the same group into a new group.",
                Stable,
            ),
            CommandInfo::new::<LayerUngroup>(
                "layer.ungroup",
                "Replaces a group by its children.",
                Stable,
            ),
            CommandInfo::new::<AdjustmentUpdate>(
                "adjustment.update",
                "Changes the parameters of an adjustment layer.",
                Stable,
            ),
            CommandInfo::new::<FilterUpdate>(
                "filter.update",
                "Changes the parameters of a filter layer.",
                Stable,
            ),
            CommandInfo::new::<MaskCreate>(
                "mask.create",
                "Adds a white or black mask to a layer or group.",
                Stable,
            ),
            CommandInfo::new::<MaskDelete>(
                "mask.delete",
                "Removes the mask of a layer or group.",
                Stable,
            ),
            CommandInfo::new::<MaskUpdate>(
                "mask.update",
                "Changes the density, inversion or activation of a mask.",
                Stable,
            ),
            CommandInfo::new::<MaskFillRect>(
                "mask.fill",
                "Fills a rectangle of a mask with a gray value.",
                Stable,
            ),
            CommandInfo::new::<MaskGradient>(
                "mask.gradient",
                "Fills a mask with a linear gradient between two points.",
                Stable,
            ),
            CommandInfo::new::<PixelsFill>(
                "pixels.fill",
                "Fills a rectangle of a pixel layer with a color.",
                Development,
            ),
            CommandInfo::new::<PixelsWrite>(
                "pixels.write",
                "Writes a block of pixel values into a pixel layer.",
                Development,
            ),
        ]
    }
}
