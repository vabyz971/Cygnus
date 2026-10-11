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

use photo_engine::{BlendMode, Document, LayerNode};
use std::collections::HashMap;
use ui_kit::icons::Icon;
use ui_kit::utils::DropPosition;
use uuid::Uuid;

/// Semantic type of a photo layer (mirror of engine `LayerNode`s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoLayerKind {
    /// Pixel layer.
    Pixel,
    /// Layer group.
    Group,
    /// Adjustment layer.
    Adjustment,
}

/// Every variant, for exhaustiveness tests.
pub const ALL_PHOTO_LAYER_KINDS: &[PhotoLayerKind] = &[
    PhotoLayerKind::Pixel,
    PhotoLayerKind::Group,
    PhotoLayerKind::Adjustment,
];

impl PhotoLayerKind {
    /// Semantic icon (via ui-kit's `Icon` only).
    pub fn icon(self) -> Icon {
        match self {
            Self::Pixel => Icon::ImageIcon,
            Self::Group => Icon::Layers,
            Self::Adjustment => Icon::Settings,
        }
    }

    /// Type label (via the photo catalog).
    pub fn icon_label(self, texts: crate::i18n::PhotoCatalog) -> &'static str {
        match self {
            Self::Pixel => texts.get(crate::i18n::PhotoTextKey::LayerKindPixel),
            Self::Group => texts.get(crate::i18n::PhotoTextKey::LayerKindGroup),
            Self::Adjustment => texts.get(crate::i18n::PhotoTextKey::LayerKindAdjustment),
        }
    }

    /// Derives the display type of an engine node.
    pub fn from_node(node: &LayerNode) -> Self {
        match node {
            LayerNode::Pixel(_) => Self::Pixel,
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

/// Layers panel translation catalogs (generic ui-kit +
/// photo-specific), built once per frame.
#[derive(Debug, Clone, Copy)]
pub struct LayerTexts {
    /// Generic labels (Rename, Delete…).
    pub catalog: ui_kit::i18n::Catalog,
    /// Photo domain strings.
    pub texts: crate::i18n::PhotoCatalog,
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
    /// Direct children (groups only, recursive): the UI shows
    /// them indented under their parent, collapsed when `collapsed`.
    /// Any layer may be a child of a group (free
    /// engine-side nesting: `reorder_before` / `move_into`).
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
                    .map(|f| PhotoSubLayerInfo {
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
                    })
                    .collect(),
                pixels
                    .masks
                    .iter()
                    .map(|m| PhotoSubLayerInfo {
                        id: m.id,
                        name: m.name.clone(),
                        is_filter: false,
                        enabled: m.enabled,
                        filter_type: String::new(),
                        params: HashMap::new(),
                    })
                    .collect(),
            ),
            LayerNode::Group(group) => (
                Vec::new(),
                group
                    .masks
                    .iter()
                    .map(|m| PhotoSubLayerInfo {
                        id: m.id,
                        name: m.name.clone(),
                        is_filter: false,
                        enabled: m.enabled,
                        filter_type: String::new(),
                        params: HashMap::new(),
                    })
                    .collect(),
            ),
            LayerNode::Adjustment(_) => (Vec::new(), Vec::new()),
        };
        let has_filters = !filters.is_empty();
        let has_masks = !masks.is_empty();
        // Thumbnail served by the resolver (zero hot recompute):
        // only pixels have one.
        let thumb = match node {
            LayerNode::Pixel(pixels) => lookup(node.id()).map(|buf| PhotoLayerThumb {
                width: buf.width,
                height: buf.height,
                rgba: buf.data.to_vec(),
                version: pixels.appearance_version,
            }),
            LayerNode::Group(_) | LayerNode::Adjustment(_) => None,
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
                LayerNode::Pixel(_) | LayerNode::Adjustment(_) => Vec::new(),
            },
            collapsed: match node {
                LayerNode::Group(group) => group.collapsed,
                LayerNode::Pixel(_) | LayerNode::Adjustment(_) => false,
            },
            thumb,
            selected: false,
            overlay_live: match node {
                LayerNode::Pixel(pixels) => {
                    photo_engine::interaction::overlay_displayable(&pixels.transform)
                }
                LayerNode::Group(_) | LayerNode::Adjustment(_) => false,
            },
            transform: match node {
                LayerNode::Pixel(pixels) => pixels.transform,
                LayerNode::Group(_) | LayerNode::Adjustment(_) => {
                    photo_engine::Transform2D::default()
                }
            },
        }
    }
}

/// Hierarchical drop resolved to rows: what is moved, onto
/// what, and where (before / inside / after). The panel converts it to
/// worker commands (`ReorderNodes`, `MoveIntoGroup`); attachments
/// do not move via DnD (documented no-op).
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

/// Finds an attachment `(info, is_filter)` by `(owner, id)`.
pub fn find_attachment_in(
    layers: &[PhotoLayerInfo],
    owner: Uuid,
    id: Uuid,
) -> Option<(&PhotoSubLayerInfo, bool)> {
    let porteur = find_layer_in(layers, owner)?;
    if let Some(filtre) = porteur.filters.iter().find(|f| f.id == id) {
        return Some((filtre, true));
    }
    porteur
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
        FlatRowKind::Filter { owner } | FlatRowKind::Mask { owner } => {
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
    /// Live filter owned by `owner`.
    Filter {
        /// Owning layer.
        owner: Uuid,
    },
    /// Mask owned by `owner`.
    Mask {
        /// Owning layer.
        owner: Uuid,
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
    /// True when a layer may be dropped INSIDE (expanded groups).
    pub nestable: bool,
}

/// Flattens the hierarchy for display: each layer then, when
/// expanded, its recursive children, its filters and its masks
/// (full rows, indented). Top of stack first.
pub fn flatten_layers(layers: &[PhotoLayerInfo]) -> Vec<FlatRow> {
    let mut out = Vec::new();
    for layer in layers {
        flatten_layer(layer, 0, &mut out);
    }
    out
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
        nestable: layer.kind == PhotoLayerKind::Group && !layer.collapsed,
    });
    if layer.collapsed {
        return;
    }
    for child in &layer.children {
        flatten_layer(child, depth + 1, out);
    }
    for filter in &layer.filters {
        out.push(FlatRow {
            id: filter.id,
            kind: FlatRowKind::Filter { owner: layer.id },
            depth: depth + 1,
            nestable: false,
        });
    }
    for mask in &layer.masks {
        out.push(FlatRow {
            id: mask.id,
            kind: FlatRowKind::Mask { owner: layer.id },
            depth: depth + 1,
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
        let _ = ui_kit::icons::IconRegistry::new().text(fond.kind.icon());
    }

    #[test]
    fn layer_kind_icons_come_from_registry() {
        for kind in ALL_PHOTO_LAYER_KINDS {
            let _ = ui_kit::icons::IconRegistry::new().text(kind.icon());
        }
    }

    fn grouped_doc() -> (Document, uuid::Uuid, uuid::Uuid) {
        use photo_engine::GroupLayer;
        let mut doc = Document::new(8, 8);
        let inner = LayerNode::Pixel(PixelLayer::new("dedans", test_image()));
        let inner_id = inner.id();
        let group = GroupLayer::new("groupe", vec![inner]);
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
        assert!(!rows[1].nestable);
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
}
