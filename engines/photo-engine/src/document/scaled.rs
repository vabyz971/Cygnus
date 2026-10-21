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

//! Reduced-scale compositing (display path).
//!
//! The canvas never shows more than a capped preview, yet the regular
//! pass composites the whole plane at FULL resolution and then shrinks
//! it: on a 4K document most of the work is thrown away. This module
//! composites directly at the display scale: every appearance (filters
//! and masks already baked, full resolution, cached by the renderer) is
//! downscaled once and kept while its `Arc` identity holds, and a light
//! "scaled view" of the tree places those rasters with offsets × scale.
//!
//! Transforms pivot around the image center, so a raster shrunk by `s`
//! with its offset multiplied by `s` lands exactly where the full pass
//! would put it, scaled. Canvas-space data that would need resampling
//! too (vector/group/adjustment masks, transformed group filters) is not
//! handled: [`Document::scaled_view`] returns `None` and the caller
//! keeps the full-resolution pass. Spatial filter parameters evaluated on
//! composites (group / adjustment `radius`) are scaled with the view.
//! Exports never use this path.

use std::collections::HashMap;
use std::sync::Arc;

use datatypes::ParamValue;
use image::DynamicImage;
use uuid::Uuid;

use super::compositing::{apply_coverage, combined_mask_coverage, fast_thumbnail_rgba8};
use super::model::{FilterLayer, FilterNode, LayerMask, LayerNode, PixelLayer, Transform2D};
use super::tree::Document;

/// Filter parameters measured in pixels (scaled with the view).
const SPATIAL_PARAMS: &[&str] = &["radius"];

fn scale_offset(t: &Transform2D, s: f32) -> Transform2D {
    Transform2D {
        offset_x: t.offset_x * s,
        offset_y: t.offset_y * s,
        ..*t
    }
}

fn scale_params(params: &mut HashMap<String, ParamValue>, s: f32) {
    for key in SPATIAL_PARAMS {
        if let Some(ParamValue::Float(v)) = params.get_mut(*key) {
            *v *= s;
        }
    }
}

fn scale_filter_nodes(filters: &[FilterNode], s: f32) -> Vec<FilterNode> {
    filters
        .iter()
        .cloned()
        .map(|mut f| {
            scale_params(&mut f.params, s);
            f
        })
        .collect()
}

/// Group filter sub-layers: only identity-placed, mask-free ones can be
/// evaluated on a scaled composite.
fn scale_filter_layers(filters: &[FilterLayer], s: f32) -> Option<Vec<FilterLayer>> {
    filters
        .iter()
        .map(|f| {
            if !f.enabled {
                return Some(f.clone());
            }
            if f.transform != Transform2D::default() || f.masks.iter().any(|m| m.enabled) {
                return None;
            }
            let mut f = f.clone();
            scale_params(&mut f.params, s);
            Some(f)
        })
        .collect()
}

/// Scaled copy of one node (`None` = unsupported, see module docs).
fn scale_node(node: &LayerNode, s: f32) -> Option<LayerNode> {
    Some(match node {
        LayerNode::Pixel(l) => {
            let mut l = l.clone();
            l.transform = scale_offset(&l.transform, s);
            l.styles = l.styles.scaled(s);
            l.children = scale_nodes(&l.children, s)?;
            LayerNode::Pixel(l)
        }
        LayerNode::Vector(v) => {
            if v.masks.iter().any(|m| m.enabled) {
                return None;
            }
            // Its raster is resolved by id: a plain pixel node at the
            // vector's placement (bounds origin folded in) draws it the
            // same way.
            let placed = v.placement().unwrap_or(v.transform);
            let mut p = PixelLayer::new(v.name.clone(), Arc::new(DynamicImage::new_rgba8(1, 1)));
            p.id = v.id;
            p.transform = scale_offset(&placed, s);
            p.opacity = v.opacity;
            p.blend_mode = v.blend_mode;
            p.visible = v.visible;
            LayerNode::Pixel(p)
        }
        LayerNode::Group(g) => {
            if g.masks.iter().any(|m| m.enabled) {
                return None;
            }
            let mut g = g.clone();
            g.children = scale_nodes(&g.children, s)?;
            g.filter_layers = scale_filter_layers(&g.filter_layers, s)?;
            LayerNode::Group(g)
        }
        LayerNode::Adjustment(a) => {
            if a.masks.iter().any(|m| m.enabled) {
                return None;
            }
            let mut a = a.clone();
            a.filters = scale_filter_nodes(&a.filters, s);
            LayerNode::Adjustment(a)
        }
    })
}

fn scale_nodes(nodes: &[LayerNode], s: f32) -> Option<Vec<LayerNode>> {
    nodes.iter().map(|n| scale_node(n, s)).collect()
}

/// Shrinks an image by `scale` (dimensions rounded, at least 1 px).
fn shrink(image: &DynamicImage, scale: f32) -> DynamicImage {
    let nw = ((image.width() as f32 * scale).round() as u32).max(1);
    let nh = ((image.height() as f32 * scale).round() as u32).max(1);
    DynamicImage::ImageRgba8(fast_thumbnail_rgba8(&image.to_rgba8(), nw, nh))
}

/// Mask shrunk with its carrier (coverage keeps its meaning).
fn shrink_mask(mask: &LayerMask, scale: f32) -> LayerMask {
    let nw = ((mask.image.width() as f32 * scale).round() as u32).max(1);
    let nh = ((mask.image.height() as f32 * scale).round() as u32).max(1);
    LayerMask {
        image: Arc::new(fast_thumbnail_rgba8(&mask.image, nw, nh)),
        // Feather is in mask pixels: it shrinks with the buffer. Own
        // cache: sharing the full-size one would thrash it.
        feather: mask.feather * scale,
        effective_cache: crate::document::EffectiveCache::default(),
        ..mask.clone()
    }
}

/// Live-filter sub-layers re-expressed at `scale` (source space shrinks
/// with the carrier: offsets, masks and pixel-sized parameters follow).
fn scale_sub_layers(filters: &[FilterLayer], s: f32) -> Vec<FilterLayer> {
    filters
        .iter()
        .map(|f| {
            let mut f = f.clone();
            f.transform = scale_offset(&f.transform, s);
            f.masks = f.masks.iter().map(|m| shrink_mask(m, s)).collect();
            scale_params(&mut f.params, s);
            f
        })
        .collect()
}

/// Display-scale appearance of one pixel layer, with the inputs it was
/// derived from (validity keys).
struct PixelEntry {
    source: Arc<DynamicImage>,
    scale: u32,
    small_source: Arc<DynamicImage>,
    filters: u64,
    masks: u64,
    appearance: Arc<DynamicImage>,
}

/// Downscaled appearances, reused while their inputs hold (a move or an
/// opacity tick re-serves them; a filter edit re-runs the chain on the
/// already-shrunk source only).
#[derive(Default)]
pub struct ScaledAppearances {
    entries: HashMap<Uuid, (Arc<DynamicImage>, u32, Arc<DynamicImage>)>,
    pixels: HashMap<Uuid, PixelEntry>,
}

impl ScaledAppearances {
    /// Scale bucket (1/4096 steps): same document, same key.
    fn key(scale: f32) -> u32 {
        (scale * 4096.0).round() as u32
    }

    /// `full` shrunk by `scale` (dimensions rounded, at least 1 px).
    pub fn get(&mut self, id: Uuid, full: &Arc<DynamicImage>, scale: f32) -> Arc<DynamicImage> {
        let key = Self::key(scale);
        if let Some((source, k, small)) = self.entries.get(&id)
            && Arc::ptr_eq(source, full)
            && *k == key
        {
            return Arc::clone(small);
        }
        let small = Arc::new(shrink(full, scale));
        self.entries
            .insert(id, (Arc::clone(full), key, Arc::clone(&small)));
        small
    }

    /// Appearance of a pixel layer evaluated AT `scale`: same recipe as
    /// the renderer (filter chain, then the layer's mask coverage) on the
    /// shrunk source, never at full resolution. Re-served while source,
    /// filters, masks and scale are unchanged.
    pub fn pixel_appearance(&mut self, layer: &PixelLayer, scale: f32) -> Arc<DynamicImage> {
        let key = Self::key(scale);
        let filters = crate::renderer::filters_signature(&layer.filter_layers);
        let masks = crate::renderer::mask_signature(&layer.masks);
        let small_source = match self.pixels.get(&layer.id) {
            Some(e) if Arc::ptr_eq(&e.source, &layer.source_image) && e.scale == key => {
                if e.filters == filters && e.masks == masks {
                    return Arc::clone(&e.appearance);
                }
                Arc::clone(&e.small_source)
            }
            _ => Arc::new(shrink(&layer.source_image, scale)),
        };
        let unmasked = crate::filters::render_chain(
            &small_source,
            &scale_sub_layers(&layer.filter_layers, scale),
        );
        let shrunk: Vec<LayerMask> = layer.masks.iter().map(|m| shrink_mask(m, scale)).collect();
        let appearance = match combined_mask_coverage(&shrunk) {
            Some(cover) => apply_coverage(unmasked, &cover),
            None => unmasked,
        };
        self.pixels.insert(
            layer.id,
            PixelEntry {
                source: Arc::clone(&layer.source_image),
                scale: key,
                small_source,
                filters,
                masks,
                appearance: Arc::clone(&appearance),
            },
        );
        appearance
    }

    /// Drops entries of layers that no longer exist.
    pub fn retain(&mut self, live: impl Fn(Uuid) -> bool) {
        self.entries.retain(|id, _| live(*id));
        self.pixels.retain(|id, _| live(*id));
    }

    /// Cached layer count (diagnostics).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len() + self.pixels.len()
    }

    /// True when nothing is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.pixels.is_empty()
    }
}

impl Document {
    /// Display scale for a preview capped at `max_dim` (document-based,
    /// stable across moves so the appearance cache keeps hitting);
    /// `None` when the document already fits.
    #[must_use]
    pub fn display_scale(&self, max_dim: u32) -> Option<f32> {
        let largest = self.width.max(self.height);
        (largest > max_dim).then(|| max_dim as f32 / largest as f32)
    }

    /// The tree at `scale` (offsets, document size and spatial filter
    /// parameters scaled), to composite with downscaled appearances.
    /// `None` when some canvas-space data cannot be scaled.
    #[must_use]
    pub fn scaled_view(&self, scale: f32) -> Option<Document> {
        if !(scale.is_finite() && scale > 0.0 && scale < 1.0) {
            return None;
        }
        let mut view = Document::new(
            ((self.width as f32 * scale).round() as u32).max(1),
            ((self.height as f32 * scale).round() as u32).max(1),
        );
        view.root = scale_nodes(&self.root, scale)?;
        Some(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::compositing::CompositeStats;
    use crate::document::{GroupLayer, LayerMask};
    use image::{GenericImageView, ImageBuffer, Rgba};

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            w,
            h,
            Rgba(rgba),
        )))
    }

    fn doc() -> Document {
        let mut doc = Document::new(400, 300);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new(
            "fond",
            solid(400, 300, [200, 40, 40, 255]),
        )));
        let mut top = PixelLayer::new("carré", solid(100, 80, [40, 200, 40, 255]));
        top.transform.offset_x = 120.0;
        top.transform.offset_y = 60.0;
        top.opacity = 80.0;
        doc.push_layer(LayerNode::Pixel(top));
        doc
    }

    #[test]
    fn display_scale_only_for_large_documents() {
        assert_eq!(doc().display_scale(400), None);
        assert_eq!(doc().display_scale(200), Some(0.5));
    }

    #[test]
    fn scaled_view_scales_offsets_and_size() {
        let view = doc().scaled_view(0.5).expect("vue réduite");
        assert_eq!((view.width, view.height), (200, 150));
        let LayerNode::Pixel(top) = &view.root[1] else {
            panic!("pixels attendus");
        };
        assert_eq!(
            (top.transform.offset_x, top.transform.offset_y),
            (60.0, 30.0)
        );
        assert!(doc().scaled_view(1.0).is_none());
        assert!(doc().scaled_view(f32::NAN).is_none());
    }

    #[test]
    fn canvas_space_masks_are_unsupported() {
        let mut d = doc();
        let mut group = GroupLayer::new("g", Vec::new());
        group.masks.push(LayerMask::full(4, 4));
        d.push_layer(LayerNode::Group(group));
        assert!(d.scaled_view(0.5).is_none());
    }

    #[test]
    fn scaled_composite_matches_downscaled_full_composite() {
        let d = doc();
        let s = 0.5;
        let resolve_full = |id| d.appearance_image_only(id);
        let mut stats = CompositeStats::default();
        let full = d
            .composite_root_range_with(0..d.root.len(), &resolve_full, &mut stats)
            .expect("composite");
        let view = d.scaled_view(s).expect("vue");
        let mut cache = ScaledAppearances::default();
        let small: HashMap<Uuid, Arc<DynamicImage>> = d
            .root
            .iter()
            .filter_map(|n| {
                let id = n.id();
                d.appearance_image_only(id)
                    .map(|img| (id, cache.get(id, &img, s)))
            })
            .collect();
        let resolve_small = |id| small.get(&id).cloned();
        let scaled = view
            .composite_root_range_with(0..view.root.len(), &resolve_small, &mut stats)
            .expect("composite réduit");
        // Sample interior points (away from edges: resampling differs
        // there by design): same colors as the full composite.
        for (x, y) in [(10.0, 10.0), (150.0, 90.0), (300.0, 250.0), (200.0, 120.0)] {
            let a = full
                .image
                .get_pixel((x + full.origin_x) as u32, (y + full.origin_y) as u32)
                .0;
            let b = scaled
                .image
                .get_pixel(
                    (x * s + scaled.origin_x) as u32,
                    (y * s + scaled.origin_y) as u32,
                )
                .0;
            for c in 0..4 {
                assert!(
                    (i16::from(a[c]) - i16::from(b[c])).abs() <= 3,
                    "({x},{y}) {a:?} vs {b:?}"
                );
            }
        }
    }

    #[test]
    fn pixel_appearance_runs_filters_at_scale_and_caches() {
        let mut layer = PixelLayer::new("photo", solid(400, 200, [100, 100, 100, 255]));
        let mut f = crate::filters::new_filter_layer("brightness_contrast").expect("connu");
        f.params
            .insert("brightness".into(), ParamValue::Float(40.0));
        layer.filter_layers.push(f);
        let mut cache = ScaledAppearances::default();
        let a = cache.pixel_appearance(&layer, 0.25);
        assert_eq!((a.width(), a.height()), (100, 50));
        // Same chain as the full-resolution renderer: 100 + 40×2.55.
        let p = a.get_pixel(50, 25).0;
        assert!((i16::from(p[0]) - 202).abs() <= 2, "{p:?}");
        // Moving the layer keeps every key: re-served.
        layer.transform.offset_x = 30.0;
        assert!(Arc::ptr_eq(&a, &cache.pixel_appearance(&layer, 0.25)));
        // A parameter edit re-runs the chain (on the shrunk source).
        if let Some(ParamValue::Float(v)) = layer.filter_layers[0].params.get_mut("brightness") {
            *v = 0.0;
        }
        let b = cache.pixel_appearance(&layer, 0.25);
        assert!(!Arc::ptr_eq(&a, &b));
        assert!((i16::from(b.get_pixel(50, 25).0[0]) - 100).abs() <= 2);
    }

    #[test]
    fn pixel_appearance_bakes_shrunk_masks() {
        let mut layer = PixelLayer::new("photo", solid(40, 40, [200, 200, 200, 255]));
        let mut mask = LayerMask::full(40, 40);
        // Black (hidden) left half.
        let mut img = (*mask.image).clone();
        for (x, _, px) in img.enumerate_pixels_mut() {
            if x < 20 {
                *px = Rgba([0, 0, 0, 255]);
            }
        }
        mask.image = Arc::new(img);
        layer.masks.push(mask);
        let mut cache = ScaledAppearances::default();
        let a = cache.pixel_appearance(&layer, 0.5);
        assert_eq!(a.get_pixel(2, 10).0[3], 0, "masqué");
        assert_eq!(a.get_pixel(17, 10).0[3], 255, "visible");
    }

    #[test]
    fn appearance_cache_reuses_until_the_source_changes() {
        let mut cache = ScaledAppearances::default();
        let id = Uuid::new_v4();
        let full = solid(40, 20, [1, 2, 3, 255]);
        let a = cache.get(id, &full, 0.5);
        let b = cache.get(id, &full, 0.5);
        assert!(Arc::ptr_eq(&a, &b), "même source : réutilisée");
        assert_eq!((a.width(), a.height()), (20, 10));
        let c = cache.get(id, &solid(40, 20, [1, 2, 3, 255]), 0.5);
        assert!(!Arc::ptr_eq(&a, &c), "nouvelle source : recalcul");
        cache.retain(|_| false);
        assert!(cache.is_empty());
    }
}
