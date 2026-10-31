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

//! Commands of Cygnus Photo (docs/decisions/0010).
//!
//! Every change to a [`Document`] is one of these commands, applied through
//! a [`cygnus_commands::CommandBus`]. Each command checks everything before
//! changing anything, and produces its inverse.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cygnus_commands::{Applied, Catalog, Command, CommandInfo, Stability};

use crate::changes::PhotoChanges;
use crate::model::{
    BlendMode, ColorProfile, Depth, Document, MAX_SIDE, Node, NodeId, NodeKind, Rect,
};
use crate::pixels::{LayerTiles, TileBackup, backup_bytes};

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
    /// The blend mode is not rendered yet (milestone J3).
    #[error("blend mode {0:?} is not supported yet")]
    UnsupportedBlendMode(BlendMode),
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
    /// New opacity, from 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// New blend mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<BlendMode>,
    /// New lock state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
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
        }
    }
}

/// Parameters of `layer.delete`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerDelete {
    /// Layer to delete.
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

/// Restores a deleted layer (inverse of `layer.delete`).
#[derive(Debug, Clone, PartialEq)]
pub struct LayerRestore {
    node: Node,
    index: usize,
    tiles: LayerTiles,
}

/// Restores overwritten tiles (inverse of the `pixels.*` commands).
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
    /// Adds an empty pixel layer.
    #[serde(rename = "layer.create")]
    LayerCreate(LayerCreate),
    /// Changes properties of a layer.
    #[serde(rename = "layer.update")]
    LayerUpdate(LayerUpdate),
    /// Deletes a layer.
    #[serde(rename = "layer.delete")]
    LayerDelete(LayerDelete),
    /// Moves a layer in the tree.
    #[serde(rename = "layer.move")]
    LayerMove(LayerMove),
    /// Development: fills a rectangle of a pixel layer with a color.
    #[serde(rename = "pixels.fill")]
    PixelsFill(PixelsFill),
    /// Development: writes a block of pixels into a pixel layer.
    #[serde(rename = "pixels.write")]
    PixelsWrite(PixelsWrite),
    /// Internal: inverse of `document.create`.
    #[serde(skip)]
    DocumentRestore(Box<Document>),
    /// Internal: inverse of `layer.delete`.
    #[serde(skip)]
    LayerRestore(Box<LayerRestore>),
    /// Internal: inverse of the `pixels.*` commands.
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

fn store_bytes(document: &Document) -> usize {
    document
        .pixels
        .layers()
        .flat_map(|layer| document.pixels.tiles(layer))
        .map(|(_, tile)| tile.bytes())
        .sum()
}

/// The node `id`, which must exist and not be the root.
fn layer_node(document: &Document, id: NodeId) -> Result<&Node, CoreError> {
    if id == document.root {
        return Err(CoreError::RootNode);
    }
    document.node(id).ok_or(CoreError::UnknownNode(id))
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
        Some(NodeKind::Group { children }) => Some(children),
        _ => None,
    }
}

fn check_index(index: usize, len: usize) -> Result<(), CoreError> {
    if index > len {
        return Err(CoreError::IndexOutOfRange { index, len });
    }
    Ok(())
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

fn create_layer(document: &mut Document, params: LayerCreate) -> Outcome {
    let parent = target_group(document, params.parent)?;
    let len = document.children(parent).len();
    let index = params.index.unwrap_or(len);
    check_index(index, len)?;

    let id = document.peek_next_id();
    document.next_id += 1;
    document.nodes.insert(
        id,
        Node {
            id,
            parent: Some(parent),
            name: params.name,
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            locked: false,
            mask: None,
            kind: NodeKind::Pixel {
                bounds: document.bounds(),
            },
        },
    );
    if let Some(children) = children_mut(document, parent) {
        children.insert(index, id);
    }
    let mut changes = PhotoChanges::structure();
    changes.created.push(id);
    applied(PhotoCommand::LayerDelete(LayerDelete { id }), 0, changes)
}

fn delete_layer(document: &mut Document, params: LayerDelete) -> Outcome {
    let id = params.id;
    let node = layer_node(document, id)?;
    if !matches!(node.kind, NodeKind::Pixel { .. }) {
        return Err(CoreError::Unsupported("deleting a group"));
    }
    if node.locked {
        return Err(CoreError::Locked(id));
    }
    let parent = node.parent.ok_or(CoreError::RootNode)?;
    let index = document
        .children(parent)
        .iter()
        .position(|child| *child == id)
        .ok_or(CoreError::UnknownNode(id))?;

    if let Some(children) = children_mut(document, parent) {
        children.remove(index);
    }
    let node = document
        .nodes
        .remove(&id)
        .ok_or(CoreError::UnknownNode(id))?;
    let tiles = document.pixels.take_layer(id);
    let bytes = tiles.values().map(|tile| tile.bytes()).sum();
    applied(
        PhotoCommand::LayerRestore(Box::new(LayerRestore { node, index, tiles })),
        bytes,
        PhotoChanges::structure(),
    )
}

fn restore_layer(document: &mut Document, restore: LayerRestore) -> Outcome {
    let LayerRestore { node, index, tiles } = restore;
    let id = node.id;
    if document.nodes.contains_key(&id) || id.0 >= document.next_id {
        return Err(invalid("node", "cannot be restored in this document"));
    }
    let parent = node.parent.ok_or(CoreError::RootNode)?;
    target_group(document, Some(parent))?;
    check_index(index, document.children(parent).len())?;

    document.nodes.insert(id, node);
    if let Some(children) = children_mut(document, parent) {
        children.insert(index, id);
    }
    document.pixels.insert_layer(id, tiles);
    applied(
        PhotoCommand::LayerDelete(LayerDelete { id }),
        0,
        PhotoChanges::structure(),
    )
}

fn update_layer(document: &mut Document, params: LayerUpdate) -> Outcome {
    let id = params.id;
    let node = layer_node(document, id)?;
    let changes_content =
        params.name.is_some() || params.opacity.is_some() || params.blend.is_some();
    // A locked layer refuses content changes, unless the same command
    // unlocks it (which is also how the inverse of a locking update works).
    let stays_locked = node.locked && params.locked != Some(false);
    if stays_locked && changes_content {
        return Err(CoreError::Locked(id));
    }
    if let Some(opacity) = params.opacity
        && !(opacity.is_finite() && (0.0..=1.0).contains(&opacity))
    {
        return Err(invalid("opacity", format!("{opacity} is not in 0..=1")));
    }
    if let Some(blend) = params.blend
        && !blend.is_supported()
    {
        return Err(CoreError::UnsupportedBlendMode(blend));
    }

    let inverse = LayerUpdate {
        id,
        name: params.name.as_ref().map(|_| node.name.clone()),
        visible: params.visible.map(|_| node.visible),
        opacity: params.opacity.map(|_| node.opacity),
        blend: params.blend.map(|_| node.blend),
        locked: params.locked.map(|_| node.locked),
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
    applied(
        PhotoCommand::LayerUpdate(inverse),
        0,
        PhotoChanges::properties(id),
    )
}

fn move_layer(document: &mut Document, params: LayerMove) -> Outcome {
    let id = params.id;
    let node = layer_node(document, id)?;
    if node.locked {
        return Err(CoreError::Locked(id));
    }
    let old_parent = node.parent.ok_or(CoreError::RootNode)?;
    let new_parent = target_group(document, params.parent)?;
    // The new parent must not be the node itself or one of its descendants.
    let mut ancestor = Some(new_parent);
    while let Some(current) = ancestor {
        if current == id {
            return Err(CoreError::Cycle);
        }
        ancestor = document.node(current).and_then(|node| node.parent);
    }
    let old_index = document
        .children(old_parent)
        .iter()
        .position(|child| *child == id)
        .ok_or(CoreError::UnknownNode(id))?;
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

fn pixels_changed(layer: NodeId, backup: &TileBackup) -> PhotoChanges {
    PhotoChanges::pixels(layer, backup.iter().map(|(coord, _)| *coord))
}

fn fill_pixels(document: &mut Document, params: PixelsFill) -> Outcome {
    let bounds = editable_pixel_layer(document, params.layer)?;
    if params
        .color
        .iter()
        .any(|channel| !(channel.is_finite() && (0.0..=1.0).contains(channel)))
    {
        return Err(invalid("color", "each channel must be in 0..=1"));
    }
    let max = f32::from(document.depth.max_value());
    // Rounded to the nearest representable value of the document depth.
    let rgba = params.color.map(|channel| (channel * max).round() as u16);
    let area = params.rect.intersect(&bounds);
    let depth = document.depth;
    let backup = document
        .pixels
        .paint(params.layer, area, depth, |_, _| rgba);
    let changes = pixels_changed(params.layer, &backup);
    let bytes = backup_bytes(&backup);
    applied(
        PhotoCommand::PixelsRestore(PixelsRestore {
            layer: params.layer,
            backup,
        }),
        bytes,
        changes,
    )
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
    let depth = document.depth;
    let backup = document.pixels.paint(params.layer, rect, depth, |x, y| {
        let i = (((y - rect.y) as usize) * width + (x - rect.x) as usize) * 4;
        [samples[i], samples[i + 1], samples[i + 2], samples[i + 3]]
    });
    let changes = pixels_changed(params.layer, &backup);
    let bytes = backup_bytes(&backup);
    applied(
        PhotoCommand::PixelsRestore(PixelsRestore {
            layer: params.layer,
            backup,
        }),
        bytes,
        changes,
    )
}

fn restore_pixels(document: &mut Document, restore: PixelsRestore) -> Outcome {
    // Restoring old content is allowed on any existing pixel layer, even
    // locked: it only replays history.
    let node = layer_node(document, restore.layer)?;
    if !matches!(node.kind, NodeKind::Pixel { .. }) {
        return Err(CoreError::NotAPixelLayer(restore.layer));
    }
    let replaced = document.pixels.restore(restore.layer, restore.backup);
    let changes = pixels_changed(restore.layer, &replaced);
    let bytes = backup_bytes(&replaced);
    applied(
        PhotoCommand::PixelsRestore(PixelsRestore {
            layer: restore.layer,
            backup: replaced,
        }),
        bytes,
        changes,
    )
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
            Self::LayerMove(_) => "layer.move",
            Self::PixelsFill(_) => "pixels.fill",
            Self::PixelsWrite(_) => "pixels.write",
            Self::DocumentRestore(_) => "document.restore",
            Self::LayerRestore(_) => "layer.restore",
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
            Self::LayerMove(params) => move_layer(document, params),
            Self::PixelsFill(params) => fill_pixels(document, params),
            Self::PixelsWrite(params) => write_pixels(document, params),
            Self::DocumentRestore(previous) => restore_document(document, *previous),
            Self::LayerRestore(restore) => restore_layer(document, *restore),
            Self::PixelsRestore(restore) => restore_pixels(document, restore),
        }
    }
}

impl Catalog for PhotoCommand {
    fn commands() -> Vec<CommandInfo> {
        vec![
            CommandInfo::new::<DocumentCreate>(
                "document.create",
                "Replaces the document with a new, empty one.",
                Stability::Stable,
            ),
            CommandInfo::new::<LayerCreate>(
                "layer.create",
                "Adds an empty pixel layer to a group.",
                Stability::Stable,
            ),
            CommandInfo::new::<LayerUpdate>(
                "layer.update",
                "Changes the name, visibility, opacity, blend mode or lock of a layer.",
                Stability::Stable,
            ),
            CommandInfo::new::<LayerDelete>("layer.delete", "Deletes a layer.", Stability::Stable),
            CommandInfo::new::<LayerMove>(
                "layer.move",
                "Moves a layer to another position or group.",
                Stability::Stable,
            ),
            CommandInfo::new::<PixelsFill>(
                "pixels.fill",
                "Fills a rectangle of a pixel layer with a color.",
                Stability::Development,
            ),
            CommandInfo::new::<PixelsWrite>(
                "pixels.write",
                "Writes a block of pixel values into a pixel layer.",
                Stability::Development,
            ),
        ]
    }
}
