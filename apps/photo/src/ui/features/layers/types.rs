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
pub use photo_engine::snapshot::{
    ALL_PHOTO_LAYER_KINDS, PhotoLayerInfo, PhotoLayerKind, PhotoLayerThumb, PhotoSubLayerInfo,
    find_attachment_in, find_filter_in, find_filter_position, find_layer_in, find_layer_in_mut,
    find_layer_of_attachment, find_mask_position, snapshot_layers, snapshot_layers_with,
};
use photo_engine::{BlendMode, Document, LayerNode};
use std::collections::HashMap;
use uuid::Uuid;

/// Presentation of a layer kind (icons and labels are UI concerns:
/// the engine snapshot only carries the kind).
pub trait LayerKindUi {
    /// Semantic icon (via cygnus-ui's `Icon` only).
    fn icon(self) -> Icon;
    /// Type label (via the photo catalog).
    fn icon_label(self, texts: cygnus_i18n::PhotoCatalog) -> &'static str;
}

impl LayerKindUi for PhotoLayerKind {
    /// Semantic icon (via cygnus-ui's `Icon` only).
    fn icon(self) -> Icon {
        match self {
            Self::Pixel => Icon::ImageIcon,
            Self::Vector => Icon::Shape,
            Self::Group => Icon::Layers,
            Self::Adjustment => Icon::Settings,
        }
    }

    /// Type label (via the photo catalog).
    fn icon_label(self, texts: cygnus_i18n::PhotoCatalog) -> &'static str {
        match self {
            Self::Pixel => texts.get(cygnus_i18n::PhotoTextKey::LayerKindPixel),
            Self::Vector => texts.get(cygnus_i18n::PhotoTextKey::LayerKindVector),
            Self::Group => texts.get(cygnus_i18n::PhotoTextKey::LayerKindGroup),
            Self::Adjustment => texts.get(cygnus_i18n::PhotoTextKey::LayerKindAdjustment),
        }
    }
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

/// Uploaded thumbnail of a layer, for the panel row
/// (built by the app from its texture cache).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerThumbView {
    /// Thumbnail GPU texture.
    pub texture_id: egui::TextureId,
    /// Thumbnail dimensions in pixels.
    pub size: egui::Vec2,
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
    fn row_count_matches_the_engine() {
        let (doc, _, _) = grouped_doc();
        let layers = snapshot_layers(&doc);
        assert_eq!(
            flattened_len(&layers),
            photo_engine::snapshot::row_count(&layers)
        );
        let mut folded = layers.clone();
        for layer in &mut folded {
            layer.collapsed = true;
        }
        assert_eq!(
            flattened_len(&folded),
            photo_engine::snapshot::row_count(&folded)
        );
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
