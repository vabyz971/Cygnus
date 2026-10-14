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

//! "LayerTree" document model (hierarchical tree, Affinity-style) +
//! fast compositing.
//!
//! TODO workstream 4: god-file (~2300 lines) to split into
//! `document/model.rs` + `document/tree.rs` + `document/compositing.rs` + local
//! tests. To do just before the next major feature (masks already shipped,
//! so plan the split in a dedicated PR — no external API must change).
//!
//! Hybrid architecture:
//! - [`Document`] owns a [`LayerNode`] tree: pixel layers,
//!   groups and adjustment layers, nestable at will.
//! - Each [`PixelLayer`] keeps its SOURCE image intact; edits live
//!   in [`FilterLayer`] sub-layers (Affinity-style),
//!   evaluated sequentially (see [`crate::filters`]).
//! - The derived appearance (source × filters) is computed lazily and
//!   cached by appearance version: a setting only recomputes the
//!   affected layer's chain; undo/redo recomputes only if needed.
//!
//! Designed for real-time interaction, like pro editors:
//! - DIRECT blending into an accumulator buffer (no intermediate copy)
//! - rayon RGBA8 CPU path: blending is memory-bound, the GPU round-trip
//!   cost more than the computation itself — the GPU stays reserved for
//!   heavy filters (blur…)
//!
//! This module is PURE: no UI framework. Previews/thumbnails are
//! [`RgbaBuf`] shareable without copying; the app converts them to textures.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use uuid::Uuid;

/// RGBA8 buffer shareable with the UI WITHOUT copying: canonical definition
/// in `datatypes` (raster vocabulary shared with video), re-exported
/// here for compatibility (`use crate::document::RgbaBuf` unchanged).
///
/// Blend mode: canonical definition in `datatypes` (same labels,
/// same numeric ids — `.cygp` projects unchanged),
/// re-exported here for compatibility.
pub use datatypes::{BlendMode, RgbaBuf};

// ---------------------------------------------------------------------------
// Core model types
// ---------------------------------------------------------------------------

/// 2D affine transform: canonical definition in `math-utils`
/// (shared with `cygnus-ui`, which cannot depend on engines).
/// Re-exported here to keep `use crate::document::Transform2D` working.
pub use math_utils::Transform2D;

/// A dynamic filter (live filter): named reference to a registry effect
/// + its parameters. Used AS IS by adjustment layers; pixel layers
/// carry [`FilterLayer`] sub-layers
/// (full-fledged sub-layers, Affinity-style).
#[derive(Debug, Clone)]
pub struct FilterNode {
    pub id: Uuid,
    /// Effect registry type_id (e.g. « brightness_contrast », « blur »)
    pub type_id: String,
    pub params: HashMap<String, datatypes::ParamValue>,
    pub enabled: bool,
}

impl FilterNode {
    /// Create a new enabled filter with empty params.
    pub fn new(type_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            type_id: type_id.into(),
            params: HashMap::new(),
            enabled: true,
        }
    }
}

/// A live filter as a SUB-LAYER of a pixel layer (Affinity-style:
/// nested live filters, reorderable, each with its own opacity, blend,
/// transform and masks). The effect (`type_id` + `params`) applies
/// to the accumulated result below it in the parent chain,
/// then is composited with its layer attributes.
///
/// Parent `Vec` order = application order (index 0 = first).
#[derive(Debug, Clone)]
pub struct FilterLayer {
    /// Stable id (selection, history, project).
    pub id: Uuid,
    /// Display name (default = definition name, renamable).
    pub name: String,
    /// Effect registry type_id (e.g. « brightness_contrast », « blur »)
    pub type_id: String,
    pub params: HashMap<String, datatypes::ParamValue>,
    /// Effect switch (Layers panel eye) — disabled =
    /// transparent, settings kept.
    pub enabled: bool,
    /// 0..=100 — weight of the filtered result over the accumulated image.
    pub opacity: f32,
    pub blend_mode: BlendMode,
    /// Parent space (identity by default — like a nested Affinity
    /// filter following its layer).
    pub transform: Transform2D,
    /// Coverage masks — merged multiplicatively in the parent space,
    /// BEFORE the sub-layer transform (same space as paint,
    /// which uses the carrier transform).
    pub masks: Vec<LayerMask>,
}

impl FilterLayer {
    /// New sub-layer with its definition's DEFAULT parameters.
    pub fn new(
        type_id: impl Into<String>,
        name: impl Into<String>,
        params: HashMap<String, datatypes::ParamValue>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            type_id: type_id.into(),
            params,
            enabled: true,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            transform: Transform2D::default(),
            masks: Vec::new(),
        }
    }

    /// "neutral" sub-layer: full opacity, normal blend, no transform
    /// or mask — renders strictly identical to the old fold.
    pub fn neutral(
        type_id: impl Into<String>,
        params: HashMap<String, datatypes::ParamValue>,
    ) -> Self {
        let type_id = type_id.into();
        Self::new(type_id.clone(), type_id, params)
    }

    /// Sub-layer rebuilt from a plain filter (adjustments) — neutral
    /// layer attributes, identity (id, state) preserved.
    pub fn from_node(node: FilterNode) -> Self {
        Self {
            id: node.id,
            name: node.type_id.clone(),
            type_id: node.type_id,
            params: node.params,
            enabled: node.enabled,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            transform: Transform2D::default(),
            masks: Vec::new(),
        }
    }

    /// Fast path: no active layer attribute — the effect result
    /// replaces the accumulated image as is (zero blend, zero allocation).
    #[must_use]
    pub fn is_passthrough(&self) -> bool {
        (self.opacity - 100.0).abs() < 0.01
            && self.blend_mode == BlendMode::Normal
            && self.transform == Transform2D::default()
            && !self.masks.iter().any(|m| m.enabled)
    }
}

/// Global monotonic appearance-version counter: guarantees a number
/// never denotes two different contents (undo/redo included).
static APPEARANCE_VERSION: AtomicU64 = AtomicU64::new(1);

pub fn next_appearance_version() -> u64 {
    APPEARANCE_VERSION.fetch_add(1, Ordering::Relaxed)
}

/// Non-destructive raster mask attached to a PixelLayer, GroupLayer or
/// filter sub-layer (in that case: parent layer space). Lives in the
/// SAME space as its carrier (carrier dimensions, follows its
/// transform) — like the default "linked" Affinity/Photoshop mask. Stored
/// as RGBA8 (R=G=B=coverage, A=255) to reuse `paint::paint_stroke_rgba`
/// without duplicating rasterization.
/// TODO v2: pure Luma8 (÷4 memory), dedicated feather/blur, unlink transform.
#[derive(Clone, Debug)]
pub struct LayerMask {
    /// Stable id, used by the app to select / edit / delete
    /// THIS mask among a layer's N masks.
    pub id: Uuid,
    /// Display name (default « Masque N », renamable like a layer).
    pub name: String,
    /// RGBA8 coverage buffer (R = coverage, A = constant 255).
    pub image: Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>,
    pub enabled: bool,
    pub inverted: bool,
    pub version: u64,
}

impl LayerMask {
    /// Full-white mask (all visible) at the given dimensions.
    #[must_use]
    pub fn full(width: u32, height: u32) -> Self {
        let buf = ImageBuffer::from_pixel(width, height, Rgba([255, 255, 255, 255]));
        Self {
            id: Uuid::new_v4(),
            name: String::from("Masque"),
            image: Arc::new(buf),
            enabled: true,
            inverted: false,
            version: next_appearance_version(),
        }
    }

    pub fn touch(&mut self) {
        self.version = next_appearance_version();
    }

    /// 0.0..=1.0 coverage at pixel (x,y), accounting for `inverted`.
    /// Callers must check `enabled` first (returns 1.0 when disabled).
    #[inline]
    #[must_use]
    pub fn coverage_at(&self, x: u32, y: u32) -> f32 {
        let c = self.image.get_pixel(x, y)[0] as f32 / 255.0;
        if self.inverted { 1.0 - c } else { c }
    }
}

/// Pixel layer: non-destructive SOURCE image + filter sub-layers.
///
/// The source image only changes on explicit events (paint, import,
/// voluntary destructive edits); everything else lives in filters
/// or the transform.
///
/// `children` are Affinity-style nested layers: composited ABOVE
/// the parent then restricted to its alpha (clipping-mask
/// `Alpha_enfant × Alpha_parent`). Same space as the carrier
/// (no inherited transform, like masks).
#[derive(Clone, Debug)]
pub struct PixelLayer {
    pub id: Uuid,
    pub name: String,
    pub source_image: Arc<DynamicImage>,
    /// Non-destructive filter sub-layers (Affinity-style — order =
    /// application order, index 0 = first).
    pub filter_layers: Vec<FilterLayer>,
    /// Clipped children (nesting): same space, bottom → top order like
    /// the root (index 0 = below).
    pub children: Vec<LayerNode>,
    pub transform: Transform2D,
    /// 0..=100
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    /// Coverage masks — merged multiplicatively in compositing.
    pub masks: Vec<LayerMask>,
    /// Incremented on EVERY mutation affecting appearance (source or filters).
    /// Document appearance-cache invalidation key.
    pub appearance_version: u64,
}

impl PixelLayer {
    /// Create a new pixel layer with given name and source image.
    pub fn new(name: impl Into<String>, image: Arc<DynamicImage>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source_image: image,
            filter_layers: Vec::new(),
            children: Vec::new(),
            transform: Transform2D::default(),
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            masks: Vec::new(),
            appearance_version: next_appearance_version(),
        }
    }

    /// Source dimensions in pixels.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        self.source_image.dimensions()
    }

    /// Bumps the appearance version (call after any pixel/filter mutation)
    pub fn touch(&mut self) {
        self.appearance_version = next_appearance_version();
    }

    /// Replaces the pixel content (paint, destructive edits…)
    pub fn set_source_image(&mut self, image: DynamicImage) {
        self.source_image = Arc::new(image);
        self.touch();
    }
}

/// Layer group: its children composite together first, then the result
/// is blended into the parent with the GROUP's opacity/mode. Child
/// transforms stay in canvas coordinates (no inherited
/// transform — like Affinity).
///
/// `filter_layers` are the group's nested filters: applied to the
/// children composite AFTER composition, BEFORE the group masks
/// (same order as a pixel layer's sub-layers).
#[derive(Clone, Debug)]
pub struct GroupLayer {
    pub id: Uuid,
    pub name: String,
    pub children: Vec<LayerNode>,
    /// Group nested filters (application order, index 0 = first).
    pub filter_layers: Vec<FilterLayer>,
    /// Layers panel display state (collapsed/expanded)
    pub collapsed: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    /// Group coverage masks — merged multiplicatively.
    pub masks: Vec<LayerMask>,
}

impl GroupLayer {
    pub fn new(name: impl Into<String>, children: Vec<LayerNode>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            children,
            filter_layers: Vec::new(),
            collapsed: false,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            masks: Vec::new(),
        }
    }
}

/// Adjustment layer: its filter chain applies to the COMPOSITE
/// accumulated below it, within its group's bounds.
///
/// `masks` restrict the effect zone (coverage weighting, like layer
/// masks) — Affinity-style.
#[derive(Clone, Debug)]
pub struct AdjustmentLayer {
    pub id: Uuid,
    pub name: String,
    pub filters: Vec<FilterNode>,
    /// Adjustment weight (0 = invisible, 100 = full)
    pub opacity: f32,
    pub visible: bool,
    /// Coverage masks — restrict the effect zone.
    pub masks: Vec<LayerMask>,
}

impl AdjustmentLayer {
    pub fn new(name: impl Into<String>, filters: Vec<FilterNode>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            filters,
            opacity: 100.0,
            visible: true,
            masks: Vec::new(),
        }
    }
}

/// Tree node: pixel layer, group or adjustment.
#[derive(Clone, Debug)]
pub enum LayerNode {
    Pixel(PixelLayer),
    Group(GroupLayer),
    Adjustment(AdjustmentLayer),
}

impl LayerNode {
    pub fn id(&self) -> Uuid {
        match self {
            LayerNode::Pixel(l) => l.id,
            LayerNode::Group(g) => g.id,
            LayerNode::Adjustment(a) => a.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            LayerNode::Pixel(l) => &l.name,
            LayerNode::Group(g) => &g.name,
            LayerNode::Adjustment(a) => &a.name,
        }
    }

    pub fn set_name(&mut self, name: String) {
        match self {
            LayerNode::Pixel(l) => l.name = name,
            LayerNode::Group(g) => g.name = name,
            LayerNode::Adjustment(a) => a.name = name,
        }
    }

    pub fn visible(&self) -> bool {
        match self {
            LayerNode::Pixel(l) => l.visible,
            LayerNode::Group(g) => g.visible,
            LayerNode::Adjustment(a) => a.visible,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        match self {
            LayerNode::Pixel(l) => l.visible = visible,
            LayerNode::Group(g) => g.visible = visible,
            LayerNode::Adjustment(a) => a.visible = visible,
        }
    }

    pub fn opacity(&self) -> f32 {
        match self {
            LayerNode::Pixel(l) => l.opacity,
            LayerNode::Group(g) => g.opacity,
            LayerNode::Adjustment(a) => a.opacity,
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        let clamped = opacity.clamp(0.0, 100.0);
        match self {
            LayerNode::Pixel(l) => l.opacity = clamped,
            LayerNode::Group(g) => g.opacity = clamped,
            LayerNode::Adjustment(a) => a.opacity = clamped,
        }
    }

    /// Blend mode — absent from adjustment layers.
    pub fn blend_mode(&self) -> Option<BlendMode> {
        match self {
            LayerNode::Pixel(l) => Some(l.blend_mode),
            LayerNode::Group(g) => Some(g.blend_mode),
            LayerNode::Adjustment(_) => None,
        }
    }

    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        match self {
            LayerNode::Pixel(l) => l.blend_mode = mode,
            LayerNode::Group(g) => g.blend_mode = mode,
            LayerNode::Adjustment(_) => {}
        }
    }

    /// Filter chain of an ADJUSTMENT layer (pixels carry [`FilterLayer`]
    /// sub-layers — see `PixelLayer::filter_layers`).
    pub fn filters(&self) -> Option<&Vec<FilterNode>> {
        match self {
            LayerNode::Pixel(_) => None,
            LayerNode::Adjustment(a) => Some(&a.filters),
            LayerNode::Group(_) => None,
        }
    }

    pub fn filters_mut(&mut self) -> Option<&mut Vec<FilterNode>> {
        match self {
            LayerNode::Pixel(_) => None,
            LayerNode::Adjustment(a) => Some(&mut a.filters),
            LayerNode::Group(_) => None,
        }
    }

    /// All coverage masks of a pixel layer, group or
    /// adjustment.
    pub fn masks(&self) -> &[LayerMask] {
        match self {
            LayerNode::Pixel(l) => &l.masks,
            LayerNode::Group(g) => &g.masks,
            LayerNode::Adjustment(a) => &a.masks,
        }
    }

    pub fn masks_mut(&mut self) -> Option<&mut Vec<LayerMask>> {
        match self {
            LayerNode::Pixel(l) => Some(&mut l.masks),
            LayerNode::Group(g) => Some(&mut g.masks),
            LayerNode::Adjustment(a) => Some(&mut a.masks),
        }
    }

    /// Content children (nesting): pixel layers (clipped by the parent
    /// alpha) and groups. Adjustments have no children
    /// (`&[]` / `None`).
    pub fn children(&self) -> &[LayerNode] {
        match self {
            LayerNode::Pixel(l) => &l.children,
            LayerNode::Group(g) => &g.children,
            LayerNode::Adjustment(_) => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<LayerNode>> {
        match self {
            LayerNode::Pixel(l) => Some(&mut l.children),
            LayerNode::Group(g) => Some(&mut g.children),
            LayerNode::Adjustment(_) => None,
        }
    }

    /// Filter sub-layers ([`FilterLayer`]): pixel layers and groups.
    /// Adjustments carry plain [`FilterNode`]s (see
    /// [`Self::filters`]).
    pub fn filter_layers(&self) -> Option<&Vec<FilterLayer>> {
        match self {
            LayerNode::Pixel(l) => Some(&l.filter_layers),
            LayerNode::Group(g) => Some(&g.filter_layers),
            LayerNode::Adjustment(_) => None,
        }
    }

    pub fn filter_layers_mut(&mut self) -> Option<&mut Vec<FilterLayer>> {
        match self {
            LayerNode::Pixel(l) => Some(&mut l.filter_layers),
            LayerNode::Group(g) => Some(&mut g.filter_layers),
            LayerNode::Adjustment(_) => None,
        }
    }

    /// Mask identified by its id, if any.
    pub fn mask(&self, id: Uuid) -> Option<&LayerMask> {
        self.masks().iter().find(|m| m.id == id)
    }

    pub fn mask_mut(&mut self, id: Uuid) -> Option<&mut LayerMask> {
        self.masks_mut()?.iter_mut().find(|m| m.id == id)
    }

    /// Shifts the layer transform by `(dx, dy)` document pixels (move
    /// tool). Only pixel layers carry a transform: groups and
    /// adjustments return `false` (no-op). Returns `true` if the
    /// layer was moved.
    pub fn translate_by(&mut self, dx: f32, dy: f32) -> bool {
        match self {
            LayerNode::Pixel(l) => {
                l.transform.offset_x += dx;
                l.transform.offset_y += dy;
                l.touch();
                true
            }
            LayerNode::Group(_) | LayerNode::Adjustment(_) => false,
        }
    }

    /// Reassigns fresh ids (duplication) and invalidates appearance. Masks
    /// are included: they are addressed by id (UI thumbnails). Recursive
    /// over the whole nested hierarchy (children + filters).
    pub(crate) fn regenerate_ids(&mut self) {
        match self {
            LayerNode::Pixel(l) => {
                l.id = Uuid::new_v4();
                for f in &mut l.filter_layers {
                    f.id = Uuid::new_v4();
                    for m in &mut f.masks {
                        m.id = Uuid::new_v4();
                    }
                }
                for m in &mut l.masks {
                    m.id = Uuid::new_v4();
                }
                for c in &mut l.children {
                    c.regenerate_ids();
                }
                l.touch();
            }
            LayerNode::Group(g) => {
                g.id = Uuid::new_v4();
                for f in &mut g.filter_layers {
                    f.id = Uuid::new_v4();
                    for m in &mut f.masks {
                        m.id = Uuid::new_v4();
                    }
                }
                for m in &mut g.masks {
                    m.id = Uuid::new_v4();
                }
                for c in &mut g.children {
                    c.regenerate_ids();
                }
            }
            LayerNode::Adjustment(a) => {
                a.id = Uuid::new_v4();
                for f in &mut a.filters {
                    f.id = Uuid::new_v4();
                }
                for m in &mut a.masks {
                    m.id = Uuid::new_v4();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Document: tree root + appearance cache
// ---------------------------------------------------------------------------

/// Derived appearance of a pixel layer (source × active filters).
#[derive(Clone)]
pub struct Appearance {
    /// Full-resolution image (export, transforms)
    pub image: Arc<DynamicImage>,
    /// Interactive display buffer (downscaled beyond 2048 px)
    pub preview: RgbaBuf,
    /// 48×32 thumbnail for the Layers panel
    pub thumb: RgbaBuf,
}
