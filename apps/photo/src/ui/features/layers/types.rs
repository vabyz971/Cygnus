// Cygnus — Professional open-source creative suite
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

#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.
//! Photo layer display types (UI view derived from the engine).
//!
//! The app converts `photo-engine`'s `LayerNode`s into
//! [`PhotoLayerInfo`] before calling the widgets. Only the root
//! level is exposed in Phase 3 (group children stay
//! collapsed).

use cygnus_ui::icons::Icon;
use cygnus_ui::utils::DropPosition;
use photo_engine::{BlendMode, Document, LayerNode};
use std::collections::HashMap;
use uuid::Uuid;

/// Semantic type of a photo layer (mirror of engine `LayerNode`s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoLayerKind {
    /// Pixel layer.
    Pixel,
    /// Vector layer.
    Vector,
    /// Layer group.
    Group,
    /// Adjustment layer.
    Adjustment,
}

/// Every variant, for exhaustiveness tests.
pub const ALL_PHOTO_LAYER_KINDS: &[PhotoLayerKind] = &[
    PhotoLayerKind::Pixel,
    PhotoLayerKind::Vector,
    PhotoLayerKind::Group,
    PhotoLayerKind::Adjustment,
];

impl PhotoLayerKind {
    /// Semantic icon (via cygnus-ui's `Icon` only).
    pub fn icon(self) -> Icon {
        match self {
            Self::Pixel => Icon::ImageIcon,
            Self::Vector => Icon::Shape,
            Self::Group => Icon::Layers,
            Self::Adjustment => Icon::Settings,
        }
    }

    /// Type label (via the photo catalog).
    pub fn icon_label(self, texts: cygnus_i18n::PhotoCatalog) -> &'static str {
        match self {
            Self::Pixel => texts.get(cygnus_i18n::PhotoTextKey::LayerKindPixel),
            Self::Vector => texts.get(cygnus_i18n::PhotoTextKey::LayerKindVector),
            Self::Group => texts.get(cygnus_i18n::PhotoTextKey::LayerKindGroup),
            Self::Adjustment => texts.get(cygnus_i18n::PhotoTextKey::LayerKindAdjustment),
        }
    }

    /// Derives the display type of an engine node.
    pub fn from_node(node: &LayerNode) -> Self {
        match node {
            LayerNode::Pixel(_) => Self::Pixel,
            LayerNode::Vector(_) => Self::Vector,
            LayerNode::Group(_) => Self::Group,
            LayerNode::Adjustment(_) => Self::Adjustment,
        }
    }
}

/// Displayed sub-layer (live filter or mask) under its owner.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoSubLayerInfo {
    /// Engine id (filter or mask).
    pub id: Uuid,
    /// Displayed name.
    pub name: String,
    /// True = live filter, false = mask.
    pub is_filter: bool,
    /// Effect enabled (row eye).
    pub enabled: bool,
    /// Effect-registry `type_id` (filters only, e.g.
    /// `"brightness_contrast"`; empty for masks): the inspector
    /// derives the setting sliders from it.
    pub filter_type: String,
    /// Current float parameters of the filter (sliders); empty for
    /// masks. Only `Float`s travel (the registry's `Enum`/`Text`
    /// are not slider-editable).
    pub params: HashMap<String, f32>,
    /// Masks owned by a live filter (empty for masks and for plain
    /// layer piles): rendered indented one level deeper, focusable
    /// with the filter as owner.
    pub masks: Vec<PhotoSubLayerInfo>,
    /// Grayscale thumbnail of a mask (black/white coverage preview);
    /// `None` for filters (FX badge instead).
    pub thumb: Option<PhotoLayerThumb>,
}

/// Displayed filter with its own masks, from an engine filter layer.
fn sub_filter(f: &photo_engine::FilterLayer) -> PhotoSubLayerInfo {
    PhotoSubLayerInfo {
        id: f.id,
        name: f.name.clone(),
        is_filter: true,
        enabled: f.enabled,
        filter_type: f.type_id.clone(),
        params: f
            .params
            .iter()
            .filter_map(|(key, value)| value.as_float().map(|v| (key.clone(), v)))
            .collect(),
        masks: f.masks.iter().map(sub_mask).collect(),
        thumb: None,
    }
}

/// Displayed mask, from an engine mask (with a grayscale coverage
/// thumbnail for its row).
fn sub_mask(m: &photo_engine::LayerMask) -> PhotoSubLayerInfo {
    PhotoSubLayerInfo {
        id: m.id,
        name: m.name.clone(),
        is_filter: false,
        enabled: m.enabled,
        filter_type: String::new(),
        params: HashMap::new(),
        masks: Vec::new(),
        thumb: mask_thumb(m),
    }
}

/// Grayscale thumbnail of a mask buffer (nearest stride downscale,
/// fits 48×32): black hides, white reveals. Cheap enough for the
/// snapshot path (a few thousand samples at most).
fn mask_thumb(m: &photo_engine::LayerMask) -> Option<PhotoLayerThumb> {
    const MAX_W: u32 = 48;
    const MAX_H: u32 = 32;
    let (sw, sh) = (m.image.width(), m.image.height());
    if sw == 0 || sh == 0 {
        return None;
    }
    let stride = ((sw + MAX_W - 1) / MAX_W)
        .max((sh + MAX_H - 1) / MAX_H)
        .max(1);
    let (w, h) = ((sw + stride - 1) / stride, (sh + stride - 1) / stride);
    let src = m.image.as_raw();
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let sx = (x * stride).min(sw - 1) as usize;
            let sy = (y * stride).min(sh - 1) as usize;
            let v = src[(sy * sw as usize + sx) * 4];
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Some(PhotoLayerThumb {
        width: w,
        height: h,
        rgba,
        version: m.version,
    })
}

/// Designation of a focused attachment (secondary selection
/// of the Layers panel: the owner stays the primary selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentRef {
    /// Owning layer.
    pub owner: Uuid,
    /// Targeted filter or mask.
    pub id: Uuid,
}

/// Layer-tree selection: primary layer + focused
/// attachment (grouped to limit draw arity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayerTreeSelection {
    /// Selected layer.
    pub selected: Option<Uuid>,
    /// Focused attachment.
    pub focused: Option<AttachmentRef>,
}

/// Layers panel translation catalogs (generic cygnus-ui +
/// photo-specific), built once per frame.
#[derive(Debug, Clone, Copy)]
pub struct LayerTexts {
    /// Generic labels (Rename, Delete…).
    pub catalog: cygnus_i18n::Catalog,
    /// Photo domain strings.
    pub texts: cygnus_i18n::PhotoCatalog,
}

/// Layer thumbnail for the panel (pure buffer, converted to a
/// texture app-side and cached by `appearance_version`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoLayerThumb {
    /// Width in pixels (about 48).
    pub width: u32,
    /// Height in pixels (about 32, ratio preserved).
    pub height: u32,
    /// RGBA8 pixels row by row.
    pub rgba: Vec<u8>,
    /// Appearance version: the UI re-uploads only on change.
    pub version: u64,
}

/// Uploaded thumbnail of a layer, for the panel row
/// (built by the app from its texture cache).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerThumbView {
    /// Thumbnail GPU texture.
    pub texture_id: egui::TextureId,
    /// Thumbnail dimensions in pixels.
    pub size: egui::Vec2,
}

/// Display data of a photo layer (UI view, owned by the app).
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoLayerInfo {
    /// Engine id (`photo-engine` `Uuid`).
    pub id: Uuid,
    /// Displayed name.
    pub name: String,
    /// Semantic type.
    pub kind: PhotoLayerKind,
    /// Visibility (eye).
    pub visible: bool,
    /// Opacity (engine units 0..=100).
    pub opacity: f32,
    /// Layer blend mode.
    pub blend_mode: BlendMode,
    /// The layer carries live filters (FX badge).
    pub has_filters: bool,
    /// The layer carries masks (badge).
    pub has_masks: bool,
    /// Layer live filters (application order), for the HUD.
    pub filters: Vec<PhotoSubLayerInfo>,
    /// Layer masks, for the HUD.
    pub masks: Vec<PhotoSubLayerInfo>,
    /// Direct children (groups and clipped pixel children alike,
    /// recursive): the UI shows them indented under their parent,
    /// collapsed when `collapsed`. Any layer may be a child of a
    /// group or a pixel layer (free engine-side nesting).
    pub children: Vec<PhotoLayerInfo>,
    /// Collapsed group (engine state): the UI hides `children`.
    pub collapsed: bool,
    /// Pixel thumbnail (`None` = groups/adjustments: type icon).
    pub thumb: Option<PhotoLayerThumb>,
    /// Current selection (UI state).
    pub selected: bool,
    /// Interactive feedback displayable as an overlay during a drag (Phase
    /// 6G.3P): true iff the layer transform is placeable on screen
    /// (identity/translation/scale — computed engine-side, never guessed
    /// by the UI). False for rotation/skew (presentation disabled, engine
    /// and commit still exact).
    pub overlay_live: bool,
    /// Layer transform (identity for groups/adjustments): overlay
    /// screen placement (layer→document), translation/scale only
    /// when `overlay_live` is true.
    pub transform: photo_engine::Transform2D,
    /// Carrier dimensions in source pixels (`Some` for pixel layers
    /// only): canvas transform overlay geometry (corners via
    /// `transform.doc_corners`) and click hit-testing.
    pub dims: Option<(u32, u32)>,
    /// Vector paths (`Some` for vector layers only): node editor
    /// overlay (handles + drags, layer space).
    pub paths: Option<Vec<photo_engine::VectorPath>>,
}

impl PhotoLayerInfo {
    /// Derives a display entry from an engine node.
    pub fn from_node(document: &Document, node: &LayerNode) -> Self {
        Self::from_node_with_lookup(node, &|id| document.thumb(id))
    }

    /// Derives a display entry with an already-resolved thumbnail
    /// (inter-pass sharing: no new appearance resolution).
    pub fn from_node_with_thumb(node: &LayerNode, thumb: Option<photo_engine::RgbaBuf>) -> Self {
        let id = node.id();
        Self::from_node_with_lookup(node, &|query| {
            if query == id { thumb.clone() } else { None }
        })
    }

    /// Derives a display entry with a thumbnail resolver
    /// (recursive: group children are exposed indented).
    pub fn from_node_with_lookup(
        node: &LayerNode,
        lookup: &dyn Fn(Uuid) -> Option<photo_engine::RgbaBuf>,
    ) -> Self {
        let (filters, masks) = match node {
            LayerNode::Pixel(pixels) => (
                pixels
                    .filter_layers
                    .iter()
                    .map(sub_filter)
                    .collect::<Vec<_>>(),
                pixels.masks.iter().map(sub_mask).collect::<Vec<_>>(),
            ),
            LayerNode::Vector(vector) => (
                vector
                    .filter_layers
                    .iter()
                    .map(sub_filter)
                    .collect::<Vec<_>>(),
                vector.masks.iter().map(sub_mask).collect::<Vec<_>>(),
            ),
            LayerNode::Group(group) => (
                group
                    .filter_layers
                    .iter()
                    .map(sub_filter)
                    .collect::<Vec<_>>(),
                group.masks.iter().map(sub_mask).collect::<Vec<_>>(),
            ),
            LayerNode::Adjustment(adj) => (Vec::new(), adj.masks.iter().map(sub_mask).collect()),
        };
        let has_filters = !filters.is_empty();
        let has_masks = !masks.is_empty();
        // Thumbnail served by the resolver (zero hot recompute):
        // only pixels have one (vectors follow with appearance thumbs).
        let thumb = match node {
            LayerNode::Pixel(pixels) => lookup(node.id()).map(|buf| PhotoLayerThumb {
                width: buf.width,
                height: buf.height,
                rgba: buf.data.to_vec(),
                version: pixels.appearance_version,
            }),
            LayerNode::Vector(_) | LayerNode::Group(_) | LayerNode::Adjustment(_) => None,
        };
        Self {
            id: node.id(),
            name: node.name().to_owned(),
            kind: PhotoLayerKind::from_node(node),
            visible: node.visible(),
            opacity: node.opacity(),
            blend_mode: node.blend_mode().unwrap_or(BlendMode::Normal),
            has_filters,
            has_masks,
            filters,
            masks,
            children: match node {
                LayerNode::Group(group) => group
                    .children
                    .iter()
                    .rev()
                    .map(|child| Self::from_node_with_lookup(child, lookup))
                    .collect(),
                // Clipped children of a pixel layer (nesting): same
                // indented display as group children.
                LayerNode::Pixel(pixels) => pixels
                    .children
                    .iter()
                    .rev()
                    .map(|child| Self::from_node_with_lookup(child, lookup))
                    .collect(),
                // Vectors and adjustments are childless in v1.
                LayerNode::Vector(_) | LayerNode::Adjustment(_) => Vec::new(),
            },
            collapsed: match node {
                LayerNode::Group(group) => group.collapsed,
                LayerNode::Pixel(_) | LayerNode::Vector(_) | LayerNode::Adjustment(_) => false,
            },
            thumb,
            selected: false,
            overlay_live: match node {
                LayerNode::Pixel(pixels) => {
                    photo_engine::interaction::overlay_displayable(&pixels.transform)
                }
                LayerNode::Vector(vector) => {
                    photo_engine::interaction::overlay_displayable(&vector.transform)
                }
                LayerNode::Group(_) | LayerNode::Adjustment(_) => false,
            },
            transform: match node {
                LayerNode::Pixel(pixels) => pixels.transform,
                LayerNode::Vector(vector) => vector.transform,
                // Virtual placement (bounds origin as offset): display
                // and hit-test only — never committed (group moves go
                // through child translation, see `translate_by`).
                LayerNode::Group(group) => {
                    let region = photo_engine::tiles::group_footprint(group);
                    let mut placed = photo_engine::Transform2D::default();
                    placed.offset_x = region.x as f32;
                    placed.offset_y = region.y as f32;
                    placed
                }
                LayerNode::Adjustment(_) => photo_engine::Transform2D::default(),
            },
            dims: match node {
                LayerNode::Pixel(pixels) => Some(pixels.dimensions()),
                // Vectors are resolution-independent: overlay/hit-test
                // geometry is the raster bounds size (paths are
                // origin-normalized at creation, so the frame matches
                // the placed raster like a pixel source).
                LayerNode::Vector(vector) => vector.raster_dims(),
                // Groups have no stored transform: the frame is the
                // children union (virtual placement, recomputed per
                // snapshot — same visuals as pixels/vectors).
                LayerNode::Group(group) => {
                    let region = photo_engine::tiles::group_footprint(group);
                    (region.width > 0 && region.height > 0).then_some((region.width, region.height))
                }
                LayerNode::Adjustment(_) => None,
            },
            paths: match node {
                LayerNode::Vector(vector) => Some(vector.vector.paths.clone()),
                LayerNode::Pixel(_) | LayerNode::Group(_) | LayerNode::Adjustment(_) => None,
            },
        }
    }
}

/// Hierarchical drop resolved to rows: what is moved, onto
/// what, and where (before / inside / after). The panel converts it to
/// worker commands (`ReorderNodes`, `MoveIntoParent`, `MoveFilterTo`,
/// `MoveMaskTo`); every row kind is draggable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerDrop {
    /// Moved row.
    pub dragged: FlatRow,
    /// Target row.
    pub target: FlatRow,
    /// Position relative to the target.
    pub position: DropPosition,
}

/// Finds a layer by id in the hierarchy (recursive).
pub fn find_layer_in(layers: &[PhotoLayerInfo], id: Uuid) -> Option<&PhotoLayerInfo> {
    for layer in layers {
        if layer.id == id {
            return Some(layer);
        }
        if let Some(found) = find_layer_in(&layer.children, id) {
            return Some(found);
        }
    }
    None
}

/// Finds a mutable layer by id (optimistic settings
/// update before worker confirmation).
pub fn find_layer_in_mut(layers: &mut [PhotoLayerInfo], id: Uuid) -> Option<&mut PhotoLayerInfo> {
    for layer in layers {
        if layer.id == id {
            return Some(layer);
        }
        if let Some(found) = find_layer_in_mut(&mut layer.children, id) {
            return Some(found);
        }
    }
    None
}

/// Finds a live filter by id anywhere in the hierarchy (recursive
/// over layers AND their filter chains).
pub fn find_filter_in(layers: &[PhotoLayerInfo], id: Uuid) -> Option<(Uuid, &PhotoSubLayerInfo)> {
    for layer in layers {
        if let Some(f) = layer.filters.iter().find(|f| f.id == id) {
            return Some((layer.id, f));
        }
        if let Some(found) = find_filter_in(&layer.children, id) {
            return Some(found);
        }
    }
    None
}

/// Finds the owning layer of an attachment id (a filter id resolves
/// to its layer; a mask id resolves to its layer, whatever pile holds
/// it — layer pile or filter pile).
pub fn find_layer_of_attachment(layers: &[PhotoLayerInfo], id: Uuid) -> Option<Uuid> {
    for layer in layers {
        if layer.filters.iter().any(|f| f.id == id) {
            return Some(layer.id);
        }
        if layer.masks.iter().any(|m| m.id == id) {
            return Some(layer.id);
        }
        for f in &layer.filters {
            if f.masks.iter().any(|m| m.id == id) {
                return Some(layer.id);
            }
        }
        if let Some(found) = find_layer_of_attachment(&layer.children, id) {
            return Some(found);
        }
    }
    None
}

/// Position of a filter in its owner's chain.
pub fn find_filter_position(layers: &[PhotoLayerInfo], owner: Uuid, filter: Uuid) -> Option<usize> {
    let porteur = find_layer_in(layers, owner)?;
    porteur.filters.iter().position(|f| f.id == filter)
}

/// Position of a mask in its owner's pile (layer pile or live-filter
/// pile — `owner` may be either id).
pub fn find_mask_position(layers: &[PhotoLayerInfo], owner: Uuid, mask: Uuid) -> Option<usize> {
    if let Some(porteur) = find_layer_in(layers, owner) {
        return porteur.masks.iter().position(|m| m.id == mask);
    }
    let (_, filtre) = find_filter_in(layers, owner)?;
    filtre.masks.iter().position(|m| m.id == mask)
}

/// Finds an attachment `(info, is_filter)` by `(owner, id)`. The owner
/// may be a layer id (layer piles) or a filter id (filter-mask piles).
pub fn find_attachment_in(
    layers: &[PhotoLayerInfo],
    owner: Uuid,
    id: Uuid,
) -> Option<(&PhotoSubLayerInfo, bool)> {
    if let Some(porteur) = find_layer_in(layers, owner) {
        if let Some(filtre) = porteur.filters.iter().find(|f| f.id == id) {
            return Some((filtre, true));
        }
        if let Some(masque) = porteur.masks.iter().find(|m| m.id == id) {
            return Some((masque, false));
        }
    }
    let (_, filtre) = find_filter_in(layers, owner)?;
    filtre
        .masks
        .iter()
        .find(|m| m.id == id)
        .map(|masque| (masque, false))
}

/// Displayed name of a flattened row (drag ghost).
pub fn flat_row_name(layers: &[PhotoLayerInfo], row: &FlatRow) -> String {
    match row.kind {
        FlatRowKind::Layer => find_layer_in(layers, row.id)
            .map(|layer| layer.name.clone())
            .unwrap_or_default(),
        FlatRowKind::Filter { owner } | FlatRowKind::Mask { owner, .. } => {
            find_attachment_in(layers, owner, row.id)
                .map(|(sub, _)| sub.name.clone())
                .unwrap_or_default()
        }
    }
}

/// Snapshots the root-level state for the UI, **top of stack
/// first** (index 0 = top layer, as displayed), recursive
/// hierarchy included (`children`).
pub fn snapshot_layers(document: &Document) -> Vec<PhotoLayerInfo> {
    document
        .root
        .iter()
        .rev()
        .map(|node| PhotoLayerInfo::from_node(document, node))
        .collect()
}

/// [`snapshot_layers`] variant with already-resolved thumbnails:
/// `lookup` provides a pixel layer's thumbnail `RgbaBuf` (from
/// inter-pass sharing), with no new appearance resolution.
/// The hierarchy is exposed recursively (`children`, top of stack
/// first at each level).
pub fn snapshot_layers_with(
    document: &Document,
    lookup: &dyn Fn(Uuid) -> Option<photo_engine::RgbaBuf>,
) -> Vec<PhotoLayerInfo> {
    document
        .root
        .iter()
        .rev()
        .map(|node| PhotoLayerInfo::from_node_with_lookup(node, lookup))
        .collect()
}

/// Flattened layer-list row: a layer, filter, or
/// mask, with its indent depth. Shared by rendering
/// (`layer_list`) and hierarchical drop resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatRowKind {
    /// Layer (pixels, group, adjustment).
    Layer,
    /// Live filter owned by `owner` (a layer id).
    Filter {
        /// Owning layer.
        owner: Uuid,
    },
    /// Mask owned by `owner` (a layer id for layer piles, a filter
    /// id for filter-mask piles); `layer` is the subtree root used
    /// for attachment collapse gating.
    Mask {
        /// Immediate owner (layer or filter id).
        owner: Uuid,
        /// Owning layer (collapse scope).
        layer: Uuid,
    },
}

/// One displayed Layers panel row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatRow {
    /// Row id (layer, filter, or mask).
    pub id: Uuid,
    /// Row kind.
    pub kind: FlatRowKind,
    /// Indent depth (0 = root).
    pub depth: usize,
    /// Immediate container (layer for children/filters/layer-masks,
    /// filter for filter-masks; `None` at the root). Used to
    /// propagate collapse states down the visible rows.
    pub parent: Option<Uuid>,
    /// True when a layer may be dropped INSIDE (expanded groups and
    /// pixel layers — clipped children, Affinity-style).
    pub nestable: bool,
}

/// Flattens the hierarchy for display: each layer then, when
/// expanded, its recursive children, its filters (with their own
/// masks as deeper rows) and its masks (full rows, indented). Top of
/// stack first.
///
/// Attachment folding is NOT applied here (it needs egui memory):
/// callers filter with [`visible_rows`] afterwards.
pub fn flatten_layers(layers: &[PhotoLayerInfo]) -> Vec<FlatRow> {
    let mut out = Vec::new();
    for layer in layers {
        flatten_layer(layer, 0, &mut out);
    }
    out
}

/// Keeps only the rows no folded-shut ancestor hides: a row is
/// dropped when any id on its parent chain sits in `closed`
/// (attachments chevrons; engine-collapsed groups already prune
/// their subtree in [`flatten_layers`]). The row itself always
/// stays (folding hides contents, never the owner row).
pub fn visible_rows(rows: Vec<FlatRow>, closed: &std::collections::HashSet<Uuid>) -> Vec<FlatRow> {
    let parent_of: std::collections::HashMap<Uuid, Uuid> = rows
        .iter()
        .filter_map(|row| row.parent.map(|p| (row.id, p)))
        .collect();
    rows.into_iter()
        .filter(|row| {
            let mut cursor = row.parent;
            while let Some(parent) = cursor {
                if closed.contains(&parent) {
                    return false;
                }
                cursor = parent_of.get(&parent).copied();
            }
            true
        })
        .collect()
}

/// Number of displayed rows for `layers` (without building them).
pub fn flattened_len(layers: &[PhotoLayerInfo]) -> usize {
    flatten_layers(layers).len()
}

fn flatten_layer(layer: &PhotoLayerInfo, depth: usize, out: &mut Vec<FlatRow>) {
    out.push(FlatRow {
        id: layer.id,
        kind: FlatRowKind::Layer,
        depth,
        parent: None,
        nestable: !layer.collapsed
            && (layer.kind == PhotoLayerKind::Group || layer.kind == PhotoLayerKind::Pixel),
    });
    if layer.collapsed {
        return;
    }
    flatten_children(layer, depth, layer.id, out);
}

/// Children, filters and masks of a layer (shared by group children
/// and clipped pixel children alike).
fn flatten_children(layer: &PhotoLayerInfo, depth: usize, parent: Uuid, out: &mut Vec<FlatRow>) {
    for child in &layer.children {
        out.push(FlatRow {
            id: child.id,
            kind: FlatRowKind::Layer,
            depth: depth + 1,
            parent: Some(parent),
            nestable: !child.collapsed
                && (child.kind == PhotoLayerKind::Group || child.kind == PhotoLayerKind::Pixel),
        });
        if !child.collapsed {
            flatten_children(child, depth + 1, child.id, out);
        }
        for filter in &child.filters {
            flatten_filter(filter, child.id, depth + 1, out);
        }
        for mask in &child.masks {
            out.push(FlatRow {
                id: mask.id,
                kind: FlatRowKind::Mask {
                    owner: child.id,
                    layer: layer.id,
                },
                depth: depth + 2,
                parent: Some(child.id),
                nestable: false,
            });
        }
    }
    for filter in &layer.filters {
        flatten_filter(filter, layer.id, depth, out);
    }
    for mask in &layer.masks {
        out.push(FlatRow {
            id: mask.id,
            kind: FlatRowKind::Mask {
                owner: layer.id,
                layer: layer.id,
            },
            depth: depth + 1,
            parent: Some(layer.id),
            nestable: false,
        });
    }
}

/// A filter row plus its own mask rows. `owner` is the immediate
/// owner layer, `depth` its depth.
fn flatten_filter(filter: &PhotoSubLayerInfo, owner: Uuid, depth: usize, out: &mut Vec<FlatRow>) {
    out.push(FlatRow {
        id: filter.id,
        kind: FlatRowKind::Filter { owner },
        depth: depth + 1,
        parent: Some(owner),
        nestable: false,
    });
    for mask in &filter.masks {
        out.push(FlatRow {
            id: mask.id,
            kind: FlatRowKind::Mask {
                owner: filter.id,
                layer: owner,
            },
            depth: depth + 2,
            parent: Some(filter.id),
            nestable: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::PixelLayer;
    use std::sync::Arc;

    fn test_image() -> Arc<image::DynamicImage> {
        Arc::new(image::DynamicImage::new_rgba8(4, 4))
    }

    fn three_layer_doc() -> Document {
        let mut doc = Document::new(8, 8);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("milieu", test_image())));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("dessus", test_image())));
        doc
    }

    #[test]
    fn snapshot_is_top_first() {
        let doc = three_layer_doc();
        let layers = snapshot_layers(&doc);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0].name, "dessus");
        assert_eq!(layers[2].name, "fond");
    }

    #[test]
    fn snapshot_preserves_visibility_and_kind() {
        let mut doc = three_layer_doc();
        let id = doc.root[0].id();
        doc.find_mut(id).expect("calque present").set_visible(false);
        let layers = snapshot_layers(&doc);
        let fond = layers
            .iter()
            .find(|l| l.id == id)
            .expect("snapshot present");
        assert!(!fond.visible);
        assert_eq!(fond.kind, PhotoLayerKind::Pixel);
        // Check the icon resolves via the registry.
        let _ = cygnus_ui::icons::IconRegistry::new().text(fond.kind.icon());
    }

    #[test]
    fn layer_kind_icons_come_from_registry() {
        for kind in ALL_PHOTO_LAYER_KINDS {
            let _ = cygnus_ui::icons::IconRegistry::new().text(kind.icon());
        }
    }

    fn grouped_doc() -> (Document, uuid::Uuid, uuid::Uuid) {
        use photo_engine::GroupLayer;
        let mut doc = Document::new(8, 8);
        let inner = LayerNode::Pixel(PixelLayer::new("dedans", test_image()));
        let inner_id = inner.id();
        let group = photo_engine::GroupLayer::new("groupe", vec![inner]);
        let group_id = group.id;
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", test_image())));
        doc.push_layer(LayerNode::Group(group));
        (doc, group_id, inner_id)
    }

    #[test]
    fn snapshot_exposes_group_children_top_first() {
        let (doc, group_id, inner_id) = grouped_doc();
        let layers = snapshot_layers(&doc);
        assert_eq!(layers.len(), 2);
        let group = layers.iter().find(|l| l.id == group_id).expect("groupe");
        assert_eq!(group.kind, PhotoLayerKind::Group);
        assert!(!group.collapsed);
        assert_eq!(group.children.len(), 1);
        assert_eq!(group.children[0].id, inner_id);
        assert_eq!(group.children[0].name, "dedans");
    }

    #[test]
    fn flatten_shows_children_indented_and_collapse_hides_them() {
        let (mut doc, group_id, inner_id) = grouped_doc();
        let rows = flatten_layers(&snapshot_layers(&doc));
        // top: group, child, bottom.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, group_id);
        assert_eq!(rows[0].depth, 0);
        assert!(rows[0].nestable, "groupe déplié = cible de dépôt");
        assert_eq!(rows[1].id, inner_id);
        assert_eq!(rows[1].depth, 1);
        assert!(rows[1].nestable, "pixel = cible de dépôt (enfant clippé)");
        assert_eq!(rows[2].depth, 0);
        assert_eq!(flattened_len(&snapshot_layers(&doc)), 3);
        // Collapsed: the child disappears, the group stays non-nestable.
        doc.set_collapsed(group_id, true);
        let layers = snapshot_layers(&doc);
        assert!(
            layers
                .iter()
                .find(|l| l.id == group_id)
                .expect("groupe")
                .collapsed
        );
        let rows = flatten_layers(&layers);
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].nestable, "groupe replié = pas de dépôt dedans");
    }

    #[test]
    fn flatten_lists_filters_and_masks_as_child_rows() {
        use photo_engine::FilterLayer;
        use std::collections::HashMap;
        let mut doc = Document::new(8, 8);
        let mut pixels = PixelLayer::new("fond", test_image());
        let filter = FilterLayer::new("brightness_contrast", "luminosite", HashMap::new());
        let filter_id = filter.id;
        pixels.filter_layers.push(filter);
        let layer_id = pixels.id;
        doc.push_layer(LayerNode::Pixel(pixels));
        let rows = flatten_layers(&snapshot_layers(&doc));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, layer_id);
        assert_eq!(rows[0].kind, FlatRowKind::Layer);
        assert_eq!(
            rows[1].kind,
            FlatRowKind::Filter { owner: layer_id },
            "filtre = ligne enfant"
        );
        assert_eq!(rows[1].id, filter_id);
        assert_eq!(rows[1].depth, 1);
    }

    #[test]
    fn visible_rows_hides_folded_subtrees() {
        use std::collections::HashSet;
        let mut doc = Document::new(8, 8);
        let mut parent = PixelLayer::new("parent", test_image());
        parent
            .children
            .push(LayerNode::Pixel(PixelLayer::new("enfant", test_image())));
        let mut f = photo_engine::FilterLayer::new("blur", "flou", HashMap::new());
        f.masks.push(photo_engine::LayerMask::full(8, 8));
        parent.filter_layers.push(f);
        let pid = parent.id;
        doc.push_layer(LayerNode::Pixel(parent));
        let rows = flatten_layers(&snapshot_layers(&doc));
        // Parent + enfant + filtre + masque du filtre = 4 lignes.
        assert_eq!(rows.len(), 4);
        // Rien de replié : tout visible.
        let shown = visible_rows(rows.clone(), &HashSet::new());
        assert_eq!(shown.len(), 4);
        // Parent replié : lui seul reste.
        let shown = visible_rows(rows.clone(), &HashSet::from([pid]));
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].id, pid);
    }

    #[test]
    fn mask_thumb_rend_noir_et_blanc() {
        // La miniature d'un masque code la couverture en gris (noir =
        // masqué, blanc = révélé), aux bonnes dimensions bornées.
        let mut m = photo_engine::LayerMask::full(4, 4);
        let mut buf = image::ImageBuffer::from_pixel(4, 4, image::Rgba([255, 255, 255, 255]));
        buf.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        m.image = std::sync::Arc::new(buf);
        let info = sub_mask(&m);
        let thumb = info.thumb.expect("miniature");
        assert_eq!((thumb.width, thumb.height), (4, 4));
        assert_eq!(&thumb.rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&thumb.rgba[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn profondeur_trois_sans_duplicat() {
        // Three nested levels: grand + moyen + petit, exactly once
        // each (no duplication at depth ≥ 2).
        let mut doc = Document::new(8, 8);
        let petit = PixelLayer::new("petit", test_image());
        let pid = petit.id;
        let moyen = photo_engine::GroupLayer::new("moyen", vec![LayerNode::Pixel(petit)]);
        let mid = moyen.id;
        let grand = photo_engine::GroupLayer::new("grand", vec![LayerNode::Group(moyen)]);
        doc.push_layer(LayerNode::Group(grand));
        let layers = snapshot_layers(&doc);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].children.len(), 1);
        assert_eq!(layers[0].children[0].children.len(), 1);
        let rows = flatten_layers(&layers);
        for r in &rows {
            eprintln!("row {:?} depth={}", r.kind, r.depth);
        }
        assert_eq!(rows.len(), 3, "grand+moyen+petit, sans duplicat");
        let _ = (pid, mid);
    }
}
