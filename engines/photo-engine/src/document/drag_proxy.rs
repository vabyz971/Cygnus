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

//! Drag proxy (live layer moves without recompositing).
//!
//! While a layer is dragged, scaled or rotated, the only thing that
//! changes is its transform. Instead of a full CPU composite per pointer
//! tick, the stack is split ONCE at gesture start into three parts:
//! everything below the layer (flattened), the layer's own appearance
//! (filters and masks baked, carrier space) and everything above it
//! (flattened over transparency). The UI then draws the three as GPU
//! textures, the middle one through the live transform — the
//! "state-only" model applied to transforms. The exact composite runs
//! once, on release.
//!
//! Exactness: Normal "over" compositing is associative, so
//! `above ∘ layer ∘ below` equals the full stack as long as nothing above
//! the layer reads what lies beneath it. [`Document::drag_proxy_index`]
//! only accepts such stacks (root pixel layer, Normal blend, every
//! visible node above Normal and adjustment-free); anything else keeps
//! the regular per-tick composite.

use std::ops::Range;
use std::sync::Arc;

use image::DynamicImage;
use uuid::Uuid;

use super::compositing::CompositeStats;
use super::model::{BlendMode, LayerNode};
use super::tree::Document;

/// Flattened part of the stack and where it sits in document space.
#[derive(Clone, Debug)]
pub struct ProxyPart {
    /// Composite pixels (straight RGBA).
    pub image: DynamicImage,
    /// Document top-left corner inside `image` (buffer pixels).
    pub origin_x: f32,
    /// Vertical counterpart of `origin_x`.
    pub origin_y: f32,
}

/// True when every visible node reads nothing beneath it: Normal blend,
/// no adjustment layer, recursively through children.
fn normal_over(nodes: &[LayerNode]) -> bool {
    nodes.iter().filter(|n| n.visible()).all(|node| {
        !matches!(node, LayerNode::Adjustment(_))
            && node.blend_mode().is_none_or(|m| m == BlendMode::Normal)
            && normal_over(node.children())
    })
}

impl Document {
    /// Root index of `layer_id` when it can be dragged through a proxy
    /// (see the module docs), `None` otherwise.
    #[must_use]
    pub fn drag_proxy_index(&self, layer_id: Uuid) -> Option<usize> {
        let index = self.root.iter().position(|n| n.id() == layer_id)?;
        let LayerNode::Pixel(layer) = &self.root[index] else {
            return None;
        };
        let eligible = layer.visible
            && layer.blend_mode == BlendMode::Normal
            && layer.children.is_empty()
            && normal_over(&self.root[index + 1..]);
        eligible.then_some(index)
    }

    /// Composite of `range` root nodes alone (same pass as the full
    /// composite, own symmetric scope). `None` when nothing contributes.
    #[must_use]
    pub fn composite_root_range_with(
        &self,
        range: Range<usize>,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        stats: &mut CompositeStats,
    ) -> Option<ProxyPart> {
        let nodes = self.root.get(range)?;
        let (half_w, half_h) =
            super::compositing::scope_half_extents(nodes, self.width, self.height, resolve);
        let image = self.composite_scope_with(nodes, resolve, stats)?;
        Some(ProxyPart {
            image,
            origin_x: half_w - self.width as f32 / 2.0,
            origin_y: half_h - self.height as f32 / 2.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{AdjustmentLayer, FilterNode, GroupLayer, PixelLayer};
    use image::{GenericImageView, ImageBuffer, Rgba};

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            w,
            h,
            Rgba(rgba),
        )))
    }

    fn pixel(name: &str, img: Arc<DynamicImage>, opacity: f32, ox: f32, oy: f32) -> LayerNode {
        let mut layer = PixelLayer::new(name, img);
        layer.opacity = opacity;
        layer.transform.offset_x = ox;
        layer.transform.offset_y = oy;
        LayerNode::Pixel(layer)
    }

    fn three_layers() -> Document {
        let mut doc = Document::new(40, 30);
        doc.push_layer(pixel(
            "fond",
            solid(40, 30, [200, 40, 40, 255]),
            100.0,
            0.0,
            0.0,
        ));
        doc.push_layer(pixel(
            "cible",
            solid(12, 10, [40, 200, 40, 200]),
            70.0,
            5.0,
            4.0,
        ));
        doc.push_layer(pixel(
            "dessus",
            solid(20, 8, [40, 40, 200, 160]),
            60.0,
            10.0,
            6.0,
        ));
        doc
    }

    fn resolve(doc: &Document) -> impl Fn(Uuid) -> Option<Arc<DynamicImage>> + '_ {
        |id| doc.appearance_image_only(id)
    }

    /// Document-space pixel of a proxy part (transparent outside).
    fn at(part: &ProxyPart, x: f32, y: f32) -> [u8; 4] {
        let (bx, by) = ((x + part.origin_x) as i64, (y + part.origin_y) as i64);
        let (w, h) = part.image.dimensions();
        if bx < 0 || by < 0 || bx >= i64::from(w) || by >= i64::from(h) {
            return [0, 0, 0, 0];
        }
        part.image.get_pixel(bx as u32, by as u32).0
    }

    #[test]
    fn eligibility_rules() {
        let mut doc = three_layers();
        let ids: Vec<Uuid> = doc.root.iter().map(LayerNode::id).collect();
        assert_eq!(doc.drag_proxy_index(ids[1]), Some(1));
        assert_eq!(doc.drag_proxy_index(ids[0]), Some(0));
        assert_eq!(doc.drag_proxy_index(Uuid::new_v4()), None);
        // Non-normal layer ABOVE reads the dragged layer: refused.
        doc.root[2].set_blend_mode(BlendMode::Multiply);
        assert_eq!(doc.drag_proxy_index(ids[1]), None);
        // …unless hidden.
        doc.root[2].set_visible(false);
        assert_eq!(doc.drag_proxy_index(ids[1]), Some(1));
        // The dragged layer itself must blend Normal.
        doc.root[1].set_blend_mode(BlendMode::Screen);
        assert_eq!(doc.drag_proxy_index(ids[1]), None);
    }

    #[test]
    fn adjustments_and_nested_blends_above_refuse() {
        let mut doc = three_layers();
        let target = doc.root[0].id();
        doc.push_layer(LayerNode::Adjustment(AdjustmentLayer::new(
            "réglage",
            vec![FilterNode::new("brightness_contrast")],
        )));
        assert_eq!(doc.drag_proxy_index(target), None);
        let mut doc = three_layers();
        let mut inner = PixelLayer::new("dans le groupe", solid(4, 4, [1, 2, 3, 255]));
        inner.blend_mode = BlendMode::Multiply;
        doc.push_layer(LayerNode::Group(GroupLayer::new(
            "groupe",
            vec![LayerNode::Pixel(inner)],
        )));
        assert_eq!(doc.drag_proxy_index(target), None);
    }

    #[test]
    fn split_recomposes_the_full_stack() {
        // below ∘ layer ∘ above, re-stacked as plain Normal layers, must
        // match the original composite (associativity of "over").
        let doc = three_layers();
        let r = resolve(&doc);
        let mut stats = CompositeStats::default();
        let index = doc.drag_proxy_index(doc.root[1].id()).expect("éligible");
        let full = doc
            .composite_root_range_with(0..doc.root.len(), &r, &mut stats)
            .expect("composite");
        let below = doc
            .composite_root_range_with(0..index, &r, &mut stats)
            .expect("dessous");
        let above = doc
            .composite_root_range_with(index + 1..doc.root.len(), &r, &mut stats)
            .expect("dessus");
        let mut rebuilt = Document::new(doc.width, doc.height);
        rebuilt.push_layer(pixel(
            "dessous",
            Arc::new(below.image.clone()),
            100.0,
            -below.origin_x,
            -below.origin_y,
        ));
        rebuilt.push_layer(doc.root[index].clone());
        rebuilt.push_layer(pixel(
            "dessus",
            Arc::new(above.image.clone()),
            100.0,
            -above.origin_x,
            -above.origin_y,
        ));
        let rr = resolve(&rebuilt);
        let again = rebuilt
            .composite_root_range_with(0..rebuilt.root.len(), &rr, &mut stats)
            .expect("recomposé");
        for y in 0..doc.height {
            for x in 0..doc.width {
                let (a, b) = (
                    at(&full, x as f32, y as f32),
                    at(&again, x as f32, y as f32),
                );
                for c in 0..4 {
                    assert!(
                        (i16::from(a[c]) - i16::from(b[c])).abs() <= 2,
                        "({x},{y}) {a:?} vs {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_ranges_contribute_nothing() {
        let doc = three_layers();
        let r = resolve(&doc);
        let mut stats = CompositeStats::default();
        assert!(
            doc.composite_root_range_with(0..0, &r, &mut stats)
                .is_none()
        );
        assert!(
            doc.composite_root_range_with(3..3, &r, &mut stats)
                .is_none()
        );
        assert!(
            doc.composite_root_range_with(2..9, &r, &mut stats)
                .is_none()
        );
    }
}
