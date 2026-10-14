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

//! Unified Scene Graph (spec §2) — incremental migration target.
//!
//! [`Layer`] carries the fields common to every kind (identity,
//! visibility, lock, opacity, blend, transform, effects, children)
//! and [`LayerContent`] the kind-specific payload. Raster / group /
//! adjustment payloads reuse the legacy model types field by field
//! (no duplication); vector / text are minimal placeholders until
//! the Vector persona lands.
//!
//! No behavior lives here yet: compositing, history and `.cygp`
//! still run on [`crate::document::LayerNode`]. Conversions are
//! lossless exactly when the value is representable in the legacy
//! model (see [`Layer::to_legacy`]); new kinds and new fields
//! report `None` instead of silently degrading pixels.
//!
//! Follow-ups: compositing/effects on [`Layer`], `.cygp`
//! persistence (`FORMAT_VERSION` bump), history commands per
//! persona, then `core/scene` hierarchy convergence.

use std::sync::Arc;

use image::DynamicImage;
use uuid::Uuid;

use crate::document::{
    AdjustmentLayer as LegacyAdjustment, BlendMode, FilterLayer, FilterNode,
    GroupLayer as LegacyGroup, LayerMask, LayerNode, PixelLayer as LegacyPixel, Transform2D,
};
use datatypes::Vec2;

/// Non-destructive layer effect (spec §5 item 4).
///
/// Evaluated at draw time only (state-only rendering): settings
/// never regenerate pixels. Compositing support is a follow-up —
/// any non-empty [`Layer::effects`] makes [`Layer::to_legacy`]
/// return `None` so legacy compositing can't silently drop them.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayerEffect {
    /// Soft shadow behind the layer content.
    DropShadow {
        /// Horizontal offset in pixels.
        dx: f32,
        /// Vertical offset in pixels.
        dy: f32,
        /// Blur radius in pixels.
        blur: f32,
        /// Effect strength (0..=100, like opacity).
        opacity: f32,
        /// Shadow color (RGBA8).
        color: [u8; 4],
    },
    /// Outer glow around the layer content.
    OuterGlow {
        /// Glow radius in pixels.
        radius: f32,
        /// Effect strength (0..=100).
        opacity: f32,
        /// Glow color (RGBA8).
        color: [u8; 4],
    },
    /// Vector-style outline (SDF at draw time).
    Outline {
        /// Stroke width in pixels.
        width: f32,
        /// Effect strength (0..=100).
        opacity: f32,
        /// Outline color (RGBA8).
        color: [u8; 4],
    },
}

/// One Bézier-ish path of a [`VectorLayer`].
///
/// Minimal placeholder: polyline points + closed flag. Full curve
/// storage (verbs, fills, booleans) arrives with the Vector persona
/// and converges with `vector-engine` — no parallel geometry model
/// is built here on purpose.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VectorPath {
    /// Control/anchor points in layer space.
    pub points: Vec<Vec2>,
    /// Closed contour or open stroke.
    pub closed: bool,
}

/// Vector content: resolution-independent paths, SDF at draw time.
///
/// Placeholder until the Vector persona (§5 item 1). Never
/// pre-triangulated: zoom stays crisp by construction.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VectorLayer {
    /// Paths in paint order (index 0 = bottom).
    pub paths: Vec<VectorPath>,
}

/// Text content: editable runs, vectorized at draw time.
///
/// Placeholder: plain string + size. Rich spans come from
/// `text-engine` (`TextModel`) with the Layout persona.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextLayer {
    /// Plain text (runs arrive with `text-engine`).
    pub text: String,
    /// Font size in pixels.
    pub font_size: f32,
}

/// Raster content: pixels + live-filter chain + masks.
///
/// Kind-specific slice of the legacy [`LegacyPixel`] — common
/// fields (id, name, opacity, …) live on [`Layer`], never here.
#[derive(Debug, Clone)]
pub struct RasterLayer {
    /// Non-destructive source image.
    pub source_image: Arc<DynamicImage>,
    /// Affinity-style live-filter sub-layers (application order).
    pub filter_layers: Vec<FilterLayer>,
    /// Coverage masks, merged multiplicatively.
    pub masks: Vec<LayerMask>,
    /// Appearance-cache invalidation key.
    pub appearance_version: u64,
}

/// Group content: isolated blend over the children composite.
///
/// Kind-specific slice of [`LegacyGroup`]: no transform (children
/// stay in canvas coordinates, like Affinity), no opacity/blend
/// (those live on [`Layer`]).
#[derive(Debug, Clone)]
pub struct GroupLayer {
    /// Nested filters applied to the children composite.
    pub filter_layers: Vec<FilterLayer>,
    /// Layers-panel display state only (never serialized as geometry).
    pub collapsed: bool,
    /// Coverage masks, merged multiplicatively.
    pub masks: Vec<LayerMask>,
}

/// Adjustment content: effect chain over the composite below.
///
/// Kind-specific slice of [`LegacyAdjustment`]: weight-as-opacity
/// and visibility live on [`Layer`]; blend is meaningless here and
/// ignored by compositing (stored as `Normal`).
#[derive(Debug, Clone)]
pub struct AdjustmentLayer {
    /// Plain filter chain applied to the composite below.
    pub filters: Vec<FilterNode>,
    /// Masks restricting the effect zone.
    pub masks: Vec<LayerMask>,
}

/// Kind-specific payload of a [`Layer`].
#[derive(Debug, Clone)]
pub enum LayerContent {
    /// Pixels + masks (sparse 256×256 tiles at render).
    Raster(RasterLayer),
    /// Bézier curves + fills, SDF at draw.
    Vector(VectorLayer),
    /// Text runs (`cosmic-text`), vectorized at draw.
    Text(TextLayer),
    /// Own transform, isolated blend.
    Group(GroupLayer),
    /// LUT/exposure/curves over the parent composite.
    Adjustment(AdjustmentLayer),
}

/// Unified scene-graph layer (spec §2).
///
/// Same document holds raster, vector, text and layout layers;
/// personas only switch tools — never convert or flatten.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Stable identity (v4 on creation).
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// Skipped by compositing when false.
    pub visible: bool,
    /// Edit protection (display-level). The legacy model has no
    /// equivalent: dropped by [`Layer::to_legacy`] (documented,
    /// never silent for pixels).
    pub locked: bool,
    /// 0..=100, like the legacy model and `core/scene`.
    pub opacity: f32,
    /// Stored for every kind; ignored for adjustments (legacy has
    /// no blend there — compositing must keep ignoring it).
    pub blend_mode: BlendMode,
    /// Canonical affine transform. Legacy groups/adjustments have
    /// none: conversions require identity there.
    pub transform: Transform2D,
    /// Kind-specific payload.
    pub content: LayerContent,
    /// Non-destructive effects, draw-time only.
    pub effects: Vec<LayerEffect>,
    /// Children, bottom → top (index 0 = below). Raster children
    /// are clipped (`Alpha_child × Alpha_parent`, same space);
    /// adjustments must stay childless (legacy constraint).
    pub children: Vec<Layer>,
}

impl Layer {
    /// New layer with default common fields (visible, unlocked,
    /// opaque, `Normal`, identity, no effects/children).
    pub fn new(name: impl Into<String>, content: LayerContent) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            transform: Transform2D::default(),
            content,
            effects: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Lossless lift from the legacy model: new fields take their
    /// defaults (`locked = false`, no effects); adjustment blend
    /// (inexistent) is stored as `Normal` and must stay ignored.
    pub fn from_legacy(node: &LayerNode) -> Self {
        match node {
            LayerNode::Pixel(p) => Self {
                id: p.id,
                name: p.name.clone(),
                visible: p.visible,
                locked: false,
                opacity: p.opacity,
                blend_mode: p.blend_mode,
                transform: p.transform,
                content: LayerContent::Raster(RasterLayer {
                    source_image: p.source_image.clone(),
                    filter_layers: p.filter_layers.clone(),
                    masks: p.masks.clone(),
                    appearance_version: p.appearance_version,
                }),
                effects: Vec::new(),
                children: p.children.iter().map(Self::from_legacy).collect(),
            },
            LayerNode::Group(g) => Self {
                id: g.id,
                name: g.name.clone(),
                visible: g.visible,
                locked: false,
                opacity: g.opacity,
                blend_mode: g.blend_mode,
                transform: Transform2D::default(),
                content: LayerContent::Group(GroupLayer {
                    filter_layers: g.filter_layers.clone(),
                    collapsed: g.collapsed,
                    masks: g.masks.clone(),
                }),
                effects: Vec::new(),
                children: g.children.iter().map(Self::from_legacy).collect(),
            },
            LayerNode::Adjustment(a) => Self {
                id: a.id,
                name: a.name.clone(),
                visible: a.visible,
                locked: false,
                opacity: a.opacity,
                blend_mode: BlendMode::Normal,
                transform: Transform2D::default(),
                content: LayerContent::Adjustment(AdjustmentLayer {
                    filters: a.filters.clone(),
                    masks: a.masks.clone(),
                }),
                effects: Vec::new(),
                children: Vec::new(),
            },
        }
    }

    /// Back-conversion for the legacy pipelines (compositing,
    /// history, `.cygp`).
    ///
    /// Returns `None` — never a degraded value — when the layer
    /// is not representable: vector/text content, non-identity
    /// transform on group/adjustment, children on adjustment, or
    /// any non-empty [`Layer::effects`]. `locked` is display-only
    /// and intentionally dropped (the legacy model can't express
    /// it and never enforced it).
    pub fn to_legacy(&self) -> Option<LayerNode> {
        if !self.effects.is_empty() {
            return None;
        }
        match &self.content {
            LayerContent::Raster(r) => {
                let mut children = Vec::with_capacity(self.children.len());
                for child in &self.children {
                    children.push(child.to_legacy()?);
                }
                Some(LayerNode::Pixel(LegacyPixel {
                    id: self.id,
                    name: self.name.clone(),
                    source_image: r.source_image.clone(),
                    filter_layers: r.filter_layers.clone(),
                    children,
                    transform: self.transform,
                    opacity: self.opacity,
                    blend_mode: self.blend_mode,
                    visible: self.visible,
                    masks: r.masks.clone(),
                    appearance_version: r.appearance_version,
                }))
            }
            LayerContent::Group(g) => {
                if self.transform != Transform2D::default() {
                    return None;
                }
                let mut children = Vec::with_capacity(self.children.len());
                for child in &self.children {
                    children.push(child.to_legacy()?);
                }
                Some(LayerNode::Group(LegacyGroup {
                    id: self.id,
                    name: self.name.clone(),
                    children,
                    filter_layers: g.filter_layers.clone(),
                    collapsed: g.collapsed,
                    opacity: self.opacity,
                    blend_mode: self.blend_mode,
                    visible: self.visible,
                    masks: g.masks.clone(),
                }))
            }
            LayerContent::Adjustment(a) => {
                if self.transform != Transform2D::default() || !self.children.is_empty() {
                    return None;
                }
                Some(LayerNode::Adjustment(LegacyAdjustment {
                    id: self.id,
                    name: self.name.clone(),
                    filters: a.filters.clone(),
                    opacity: self.opacity,
                    visible: self.visible,
                    masks: a.masks.clone(),
                }))
            }
            LayerContent::Vector(_) | LayerContent::Text(_) => None,
        }
    }

    /// Lift a whole legacy root (index 0 = bottom, like `Document`).
    pub fn from_legacy_tree(root: &[LayerNode]) -> Vec<Self> {
        root.iter().map(Self::from_legacy).collect()
    }

    /// Lower a whole new root; `None` if any layer is not
    /// representable (see [`Layer::to_legacy`]).
    pub fn to_legacy_tree(layers: &[Self]) -> Option<Vec<LayerNode>> {
        layers.iter().map(Self::to_legacy).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_image() -> Arc<DynamicImage> {
        Arc::new(DynamicImage::new_rgba8(1, 1))
    }

    #[test]
    fn raster_round_trip_preserves_common_fields() {
        let mut pixel = LegacyPixel::new("bg", tiny_image());
        pixel.opacity = 42.0;
        pixel.blend_mode = BlendMode::Multiply;
        pixel.visible = false;
        let node = LayerNode::Pixel(pixel);

        let layer = Layer::from_legacy(&node);
        assert!(!layer.locked);
        assert!(layer.effects.is_empty());
        assert!(matches!(layer.content, LayerContent::Raster(_)));
        assert_eq!(layer.opacity, 42.0);
        assert_eq!(layer.blend_mode, BlendMode::Multiply);
        assert!(!layer.visible);

        let back = layer.to_legacy().expect("raster is representable");
        match back {
            LayerNode::Pixel(p) => {
                assert_eq!(p.name, "bg");
                assert_eq!(p.opacity, 42.0);
                assert_eq!(p.blend_mode, BlendMode::Multiply);
                assert!(!p.visible);
            }
            _ => panic!("expected pixel"),
        }
    }

    #[test]
    fn group_round_trip_keeps_nesting_and_collapsed() {
        let child = LayerNode::Pixel(LegacyPixel::new("c", tiny_image()));
        let mut group = LegacyGroup::new("g", vec![child]);
        group.collapsed = true;
        let layer = Layer::from_legacy(&LayerNode::Group(group));
        assert_eq!(layer.children.len(), 1);
        assert_eq!(layer.transform, Transform2D::default());

        let back = layer.to_legacy().expect("group is representable");
        match back {
            LayerNode::Group(g) => {
                assert!(g.collapsed);
                assert_eq!(g.children.len(), 1);
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn adjustment_defaults_blend_normal_and_identity() {
        let node = LayerNode::Adjustment(LegacyAdjustment::new("a", Vec::new()));
        let layer = Layer::from_legacy(&node);
        assert_eq!(layer.blend_mode, BlendMode::Normal);
        assert_eq!(layer.transform, Transform2D::default());
        assert!(layer.children.is_empty());
        assert!(layer.to_legacy().is_some());
    }

    #[test]
    fn vector_and_text_are_not_legacy_representable() {
        let vector = Layer::new("v", LayerContent::Vector(VectorLayer { paths: vec![] }));
        let text = Layer::new(
            "t",
            LayerContent::Text(TextLayer {
                text: "hi".to_owned(),
                font_size: 16.0,
            }),
        );
        assert!(vector.to_legacy().is_none());
        assert!(text.to_legacy().is_none());
    }

    #[test]
    fn unrepresentable_shapes_report_none() {
        // Group with a transform has no legacy equivalent.
        let mut group = Layer::new(
            "g",
            LayerContent::Group(GroupLayer {
                filter_layers: Vec::new(),
                collapsed: false,
                masks: Vec::new(),
            }),
        );
        group.transform.offset_x = 3.0;
        assert!(group.to_legacy().is_none());

        // Adjustment with children has no legacy equivalent.
        let mut adj = Layer::new(
            "a",
            LayerContent::Adjustment(AdjustmentLayer {
                filters: Vec::new(),
                masks: Vec::new(),
            }),
        );
        adj.children.push(Layer::new(
            "c",
            LayerContent::Vector(VectorLayer { paths: vec![] }),
        ));
        assert!(adj.to_legacy().is_none());

        // Any effect blocks the legacy lowering (no silent pixel change).
        let mut raster = Layer::from_legacy(&LayerNode::Pixel(LegacyPixel::new("r", tiny_image())));
        raster.effects.push(LayerEffect::OuterGlow {
            radius: 4.0,
            opacity: 100.0,
            color: [255, 255, 255, 255],
        });
        assert!(raster.to_legacy().is_none());
    }

    #[test]
    fn locked_is_display_only_and_dropped_by_legacy_lowering() {
        let mut layer = Layer::from_legacy(&LayerNode::Pixel(LegacyPixel::new("r", tiny_image())));
        layer.locked = true;
        assert!(layer.to_legacy().is_some());
    }

    #[test]
    fn tree_helpers_round_trip_roots() {
        let root = vec![
            LayerNode::Pixel(LegacyPixel::new("a", tiny_image())),
            LayerNode::Group(LegacyGroup::new("g", Vec::new())),
        ];
        let layers = Layer::from_legacy_tree(&root);
        assert_eq!(layers.len(), 2);
        let back = Layer::to_legacy_tree(&layers).expect("tree is representable");
        assert_eq!(back.len(), 2);
    }
}
