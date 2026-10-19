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

//! Layer-tree snapshots sent from the engine worker to the UI.
//!
//! Pure data derived from the document (names, kinds, visibility,
//! attachments, thumbnails as RGBA buffers, transforms): the UI renders
//! them without ever touching the `Document`. Presentation concerns
//! (icons, translated labels, row flattening) stay app-side.

use crate::{BlendMode, Document, LayerNode};
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
    /// Current float parameters of the filter (sliders). Masks carry
    /// [`MASK_DENSITY_KEY`], [`MASK_FEATHER_KEY`] and
    /// [`MASK_INVERTED_KEY`] (0 or 1). Only `Float`s travel (the
    /// registry's `Enum`/`Text` are not slider-editable).
    pub params: HashMap<String, f32>,
    /// Current text parameters of the filter (curve points, O005);
    /// empty for masks and for slider-only filters.
    pub text_params: HashMap<String, String>,
    /// Masks owned by a live filter (empty for masks and for plain
    /// layer piles): rendered indented one level deeper, focusable
    /// with the filter as owner.
    pub masks: Vec<PhotoSubLayerInfo>,
    /// Grayscale thumbnail of a mask (black/white coverage preview);
    /// `None` for filters (FX badge instead).
    pub thumb: Option<PhotoLayerThumb>,
}

/// Mask parameter key: density 0..=1.
pub const MASK_DENSITY_KEY: &str = "density";
/// Mask parameter key: feather radius in mask pixels.
pub const MASK_FEATHER_KEY: &str = "feather";
/// Mask parameter key: inversion (0 or 1).
pub const MASK_INVERTED_KEY: &str = "inverted";

/// Displayed filter with its own masks, from an engine filter layer.
fn sub_filter(f: &crate::FilterLayer) -> PhotoSubLayerInfo {
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
        text_params: f
            .params
            .iter()
            .filter_map(|(key, value)| match value {
                datatypes::ParamValue::Text(text) => Some((key.clone(), text.clone())),
                _ => None,
            })
            .collect(),
        masks: f.masks.iter().map(sub_mask).collect(),
        thumb: None,
    }
}

/// Displayed mask, from an engine mask (with a grayscale coverage
/// thumbnail for its row).
fn sub_mask(m: &crate::LayerMask) -> PhotoSubLayerInfo {
    PhotoSubLayerInfo {
        id: m.id,
        name: m.name.clone(),
        is_filter: false,
        enabled: m.enabled,
        filter_type: String::new(),
        params: HashMap::from([
            (MASK_DENSITY_KEY.to_owned(), m.density),
            (MASK_FEATHER_KEY.to_owned(), m.feather),
            (
                MASK_INVERTED_KEY.to_owned(),
                f32::from(u8::from(m.inverted)),
            ),
        ]),
        text_params: HashMap::new(),
        masks: Vec::new(),
        thumb: mask_thumb(m),
    }
}

/// Grayscale thumbnail of a mask buffer (nearest stride downscale,
/// fits 48×32): black hides, white reveals. Cheap enough for the
/// snapshot path (a few thousand samples at most).
fn mask_thumb(m: &crate::LayerMask) -> Option<PhotoLayerThumb> {
    const MAX_W: u32 = 48;
    const MAX_H: u32 = 32;
    let (sw, sh) = (m.image.width(), m.image.height());
    if sw == 0 || sh == 0 {
        return None;
    }
    let stride = sw.div_ceil(MAX_W).max(sh.div_ceil(MAX_H)).max(1);
    let (w, h) = (sw.div_ceil(stride), sh.div_ceil(stride));
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

/// Display data of a photo layer (snapshot sent to the UI).
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
    pub transform: crate::Transform2D,
    /// Carrier dimensions in source pixels (`Some` for pixel layers
    /// only): canvas transform overlay geometry (corners via
    /// `transform.doc_corners`) and click hit-testing.
    pub dims: Option<(u32, u32)>,
    /// Vector paths (`Some` for vector layers only): node editor
    /// overlay (handles + drags, layer space).
    pub paths: Option<Vec<crate::VectorPath>>,
}

impl PhotoLayerInfo {
    /// Derives a display entry from an engine node.
    pub fn from_node(document: &Document, node: &LayerNode) -> Self {
        Self::from_node_with_lookup(node, &|id| document.thumb(id))
    }

    /// Derives a display entry with an already-resolved thumbnail
    /// (inter-pass sharing: no new appearance resolution).
    pub fn from_node_with_thumb(node: &LayerNode, thumb: Option<crate::RgbaBuf>) -> Self {
        let id = node.id();
        Self::from_node_with_lookup(node, &|query| {
            if query == id { thumb.clone() } else { None }
        })
    }

    /// Derives a display entry with a thumbnail resolver
    /// (recursive: group children are exposed indented).
    pub fn from_node_with_lookup(
        node: &LayerNode,
        lookup: &dyn Fn(Uuid) -> Option<crate::RgbaBuf>,
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
                    crate::interaction::overlay_displayable(&pixels.transform)
                }
                LayerNode::Vector(vector) => {
                    crate::interaction::overlay_displayable(&vector.transform)
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
                    let region = crate::tiles::group_footprint(group);
                    crate::Transform2D {
                        offset_x: region.x as f32,
                        offset_y: region.y as f32,
                        ..crate::Transform2D::default()
                    }
                }
                LayerNode::Adjustment(_) => crate::Transform2D::default(),
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
                    let region = crate::tiles::group_footprint(group);
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
    lookup: &dyn Fn(Uuid) -> Option<crate::RgbaBuf>,
) -> Vec<PhotoLayerInfo> {
    document
        .root
        .iter()
        .rev()
        .map(|node| PhotoLayerInfo::from_node_with_lookup(node, lookup))
        .collect()
}

/// Number of rows the Layers panel lists for `layers` (layers, live
/// filters, masks; collapsed subtrees hidden). Mirrors the app's row
/// flattening, which asserts the two stay equal.
#[must_use]
pub fn row_count(layers: &[PhotoLayerInfo]) -> usize {
    layers
        .iter()
        .map(|layer| {
            1 + if layer.collapsed {
                0
            } else {
                child_rows(layer)
            }
        })
        .sum()
}

fn child_rows(layer: &PhotoLayerInfo) -> usize {
    let children: usize = layer
        .children
        .iter()
        .map(|child| {
            1 + if child.collapsed {
                0
            } else {
                child_rows(child)
            } + filter_rows(&child.filters)
                + child.masks.len()
        })
        .sum();
    children + filter_rows(&layer.filters) + layer.masks.len()
}

fn filter_rows(filters: &[PhotoSubLayerInfo]) -> usize {
    filters.iter().map(|f| 1 + f.masks.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_thumb_rend_noir_et_blanc() {
        // La miniature d'un masque code la couverture en gris (noir =
        // masqué, blanc = révélé), aux bonnes dimensions bornées.
        let mut m = crate::LayerMask::full(4, 4);
        let mut buf = image::ImageBuffer::from_pixel(4, 4, image::Rgba([255, 255, 255, 255]));
        buf.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        m.image = std::sync::Arc::new(buf);
        let info = sub_mask(&m);
        let thumb = info.thumb.expect("miniature");
        assert_eq!((thumb.width, thumb.height), (4, 4));
        assert_eq!(&thumb.rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&thumb.rgba[4..8], &[255, 255, 255, 255]);
    }
}
