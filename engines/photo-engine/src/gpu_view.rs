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

//! Document → GPU frame (via `gpu-compositor`), on the caller's device.
//!
//! Turns the layer stack into compositor layers: each appearance (filters
//! and masks baked, from the resolver) becomes a texture, uploaded once
//! and kept while its `Arc` identity holds — moving a layer or changing
//! its opacity only re-runs the GPU blend. The frame covers the same
//! symmetric plane as the CPU composite (same origin), at full document
//! resolution, so the canvas can show real pixels when zoomed in.
//!
//! Scope: pixel and vector layers (any transform, opacity, blend mode)
//! and groups, composited in isolation like the CPU path. Adjustment
//! layers, group live filters / masks, clipped children and canvas-space
//! vector masks keep the CPU path ([`GpuView::supports`]).

use std::collections::HashMap;
use std::sync::Arc;

pub use gpu_compositor::request_headless_device;
use gpu_compositor::{Affine, Compositor, DrawGroup, DrawItem, DrawLayer, LayerTexture};
use image::DynamicImage;
use uuid::Uuid;

use crate::document::{Document, LayerNode, Transform2D};

/// One composited frame, ready to display.
#[derive(Clone, Debug)]
pub struct GpuFrame {
    /// Premultiplied, gamma-space RGBA8 texture with mips (egui ready).
    pub texture: wgpu::Texture,
    /// Frame size in pixels (document resolution).
    pub size: (u32, u32),
    /// Document top-left corner inside the frame (pixels).
    pub origin: (f32, f32),
    /// Document size.
    pub doc_size: (u32, u32),
}

/// Target pixel → layer pixel mapping of a placed layer (`None` when the
/// transform is degenerate). Scales clamped like the CPU path.
fn to_layer(transform: &Transform2D, size: (u32, u32), origin: (f32, f32)) -> Option<Affine> {
    let t = Transform2D {
        scale_x: transform.scale_x.clamp(0.05, 8.0),
        scale_y: transform.scale_y.clamp(0.05, 8.0),
        ..*transform
    };
    let (w0, h0) = (size.0 as f32, size.1 as f32);
    let f = |x: f32, y: f32| t.doc_to_local(w0, h0, x, y);
    let o = f(0.0, 0.0)?;
    let ax = f(1.0, 0.0)?;
    let ay = f(0.0, 1.0)?;
    let a = (ax.0 - o.0, ax.1 - o.1);
    let b = (ay.0 - o.0, ay.1 - o.1);
    // q = o + a * (p.x - origin.x) + b * (p.y - origin.y)
    Some(Affine {
        m: [a.0, b.0, a.1, b.1],
        t: [
            o.0 - a.0 * origin.0 - b.0 * origin.1,
            o.1 - a.1 * origin.0 - b.1 * origin.1,
        ],
    })
}

/// What one frame draws, before textures are borrowed.
enum Plan {
    Layer {
        id: Uuid,
        transform: Transform2D,
        opacity: f32,
        blend: u32,
    },
    Group {
        items: Vec<Plan>,
        opacity: f32,
        blend: u32,
    },
}

/// Plan → compositor items (textures borrowed from the cache).
fn draw_items<'a>(
    textures: &'a HashMap<Uuid, (Arc<DynamicImage>, LayerTexture)>,
    plan: &[Plan],
    origin: (f32, f32),
) -> Vec<DrawItem<'a>> {
    plan.iter()
        .filter_map(|step| match step {
            Plan::Layer {
                id,
                transform,
                opacity,
                blend,
            } => {
                let (_, texture) = textures.get(id)?;
                Some(DrawItem::Layer(DrawLayer {
                    texture,
                    to_layer: to_layer(transform, texture.size(), origin)?,
                    opacity: *opacity,
                    blend: *blend,
                }))
            }
            Plan::Group {
                items,
                opacity,
                blend,
            } => Some(DrawItem::Group(DrawGroup {
                items: draw_items(textures, items, origin),
                opacity: *opacity,
                blend: *blend,
            })),
        })
        .collect()
}

/// Recursive support check (see the module docs).
fn supported(nodes: &[LayerNode]) -> bool {
    nodes.iter().filter(|n| n.visible()).all(|node| match node {
        LayerNode::Pixel(l) => l.children.is_empty(),
        LayerNode::Vector(v) => !v.masks.iter().any(|m| m.enabled),
        LayerNode::Group(g) => {
            !g.filter_layers.iter().any(|f| f.enabled)
                && !g.masks.iter().any(|m| m.enabled)
                && supported(&g.children)
        }
        LayerNode::Adjustment(_) => false,
    })
}

/// GPU renderer of one document: compositor + layer texture cache.
pub struct GpuView {
    compositor: Compositor,
    textures: HashMap<Uuid, (Arc<DynamicImage>, LayerTexture)>,
}

impl GpuView {
    /// Binds a renderer to `device` (the UI's device in the app).
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            compositor: Compositor::new(device, queue),
            textures: HashMap::new(),
        }
    }

    /// True when the document only holds what the GPU path renders
    /// (see the module docs); hidden nodes never block it.
    #[must_use]
    pub fn supports(document: &Document) -> bool {
        supported(&document.root)
    }

    /// Texture of `id`'s appearance, uploaded when it changed.
    fn texture(&mut self, id: Uuid, image: &Arc<DynamicImage>) -> Option<()> {
        let fresh = self
            .textures
            .get(&id)
            .is_none_or(|(source, _)| !Arc::ptr_eq(source, image));
        if fresh {
            let rgba = image.to_rgba8();
            let texture =
                self.compositor
                    .upload_rgba(rgba.width(), rgba.height(), rgba.as_raw())?;
            self.textures.insert(id, (Arc::clone(image), texture));
        }
        Some(())
    }

    /// Plans `nodes` (bottom first), uploading changed appearances.
    fn plan(
        &mut self,
        nodes: &[LayerNode],
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        live: &mut Vec<Uuid>,
    ) -> Option<Vec<Plan>> {
        let mut out = Vec::new();
        for node in nodes.iter().filter(|n| n.visible()) {
            let (id, transform, opacity, mode) = match node {
                LayerNode::Pixel(l) => (l.id, l.transform, l.opacity, l.blend_mode),
                LayerNode::Vector(v) => match v.placement() {
                    Some(placed) => (v.id, placed, v.opacity, v.blend_mode),
                    None => continue,
                },
                LayerNode::Group(g) => {
                    if g.opacity <= 0.01 {
                        continue;
                    }
                    out.push(Plan::Group {
                        items: self.plan(&g.children, resolve, live)?,
                        opacity: g.opacity / 100.0,
                        blend: g.blend_mode.id(),
                    });
                    continue;
                }
                LayerNode::Adjustment(_) => return None,
            };
            if opacity <= 0.01 {
                continue;
            }
            let Some(image) = resolve(id) else {
                continue;
            };
            self.texture(id, &image)?;
            live.push(id);
            out.push(Plan::Layer {
                id,
                transform,
                opacity: opacity / 100.0,
                blend: mode.id(),
            });
        }
        Some(out)
    }

    /// Composites the document (appearances from `resolve`). `None` when
    /// unsupported, empty, or larger than the device allows (the caller
    /// keeps the CPU path).
    pub fn render(
        &mut self,
        document: &Document,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<GpuFrame> {
        if !Self::supports(document) {
            return None;
        }
        let nodes = &document.root;
        let (half_w, half_h) = crate::document::compositing::scope_half_extents(
            nodes,
            document.width,
            document.height,
            resolve,
        );
        // Same plane as `composite_scope_with` (truncated size, origin).
        let size = (
            (half_w * 2.0).clamp(1.0, 16384.0) as u32,
            (half_h * 2.0).clamp(1.0, 16384.0) as u32,
        );
        let origin = (
            half_w - document.width as f32 / 2.0,
            half_h - document.height as f32 / 2.0,
        );
        let mut live = Vec::new();
        let plan = self.plan(nodes, resolve, &mut live)?;
        self.textures.retain(|id, _| live.contains(id));
        let items = draw_items(&self.textures, &plan, origin);
        let texture = self.compositor.composite_tree(size, &items)?.clone();
        Some(GpuFrame {
            texture,
            size,
            origin,
            doc_size: (document.width, document.height),
        })
    }

    /// Last frame as straight-alpha RGBA8 (blocking readback).
    #[must_use]
    pub fn read_rgba(&self) -> Option<Vec<u8>> {
        self.compositor.read_rgba()
    }

    /// Cached layer textures (diagnostics, tests).
    #[must_use]
    pub fn cached_layers(&self) -> usize {
        self.textures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::compositing::CompositeStats;
    use crate::document::{BlendMode, GroupLayer, PixelLayer};
    use image::{GenericImageView, ImageBuffer, Rgba};

    fn view() -> Option<GpuView> {
        let Some((device, queue)) = gpu_compositor::request_headless_device() else {
            eprintln!("no GPU adapter: test skipped");
            return None;
        };
        Some(GpuView::new(&device, &queue))
    }

    /// Gradient image (distinct values per pixel exercise every blend).
    fn gradient(w: u32, h: u32, alpha: u8, seed: u32) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_fn(
            w,
            h,
            |x, y| {
                Rgba([
                    ((x * 37 + seed * 11) % 256) as u8,
                    ((y * 53 + seed * 7) % 256) as u8,
                    (((x + y) * 29 + seed) % 256) as u8,
                    alpha,
                ])
            },
        )))
    }

    fn layer(img: Arc<DynamicImage>, x: f32, y: f32, opacity: f32, mode: BlendMode) -> LayerNode {
        let mut l = PixelLayer::new("l", img);
        l.transform.offset_x = x;
        l.transform.offset_y = y;
        l.opacity = opacity;
        l.blend_mode = mode;
        LayerNode::Pixel(l)
    }

    /// GPU frame vs CPU reference, pixel by pixel (same plane).
    fn assert_parity(doc: &Document, view: &mut GpuView, tolerance: i16) {
        let resolve = |id| doc.appearance_image_any(id);
        let mut stats = CompositeStats::default();
        let cpu = doc
            .composite_root_range_with(0..doc.root.len(), &resolve, &mut stats)
            .expect("composite CPU");
        let frame = view.render(doc, &resolve).expect("frame GPU");
        assert_eq!(frame.size, cpu.image.dimensions(), "même plan");
        assert_eq!(frame.origin, (cpu.origin_x, cpu.origin_y), "même origine");
        let gpu = view.read_rgba().expect("relecture");
        let cpu = cpu.image.to_rgba8();
        let mut worst = 0i16;
        for (g, c) in gpu
            .as_chunks::<4>()
            .0
            .iter()
            .zip(cpu.as_raw().as_chunks::<4>().0.iter())
        {
            for k in 0..4 {
                worst = worst.max((i16::from(g[k]) - i16::from(c[k])).abs());
            }
        }
        assert!(worst <= tolerance, "écart max {worst} > {tolerance}");
    }

    #[test]
    fn every_blend_mode_matches_the_cpu_reference() {
        let Some(mut view) = view() else { return };
        for mode in BlendMode::ALL {
            let mut doc = Document::new(24, 16);
            doc.push_layer(layer(
                gradient(24, 16, 255, 1),
                0.0,
                0.0,
                100.0,
                BlendMode::Normal,
            ));
            doc.push_layer(layer(gradient(10, 8, 200, 2), 5.0, 3.0, 70.0, mode));
            doc.push_layer(layer(gradient(6, 6, 160, 3), 12.0, 9.0, 100.0, mode));
            assert_parity(&doc, &mut view, 1);
        }
    }

    #[test]
    fn layers_outside_the_document_grow_the_plane_like_the_cpu() {
        let Some(mut view) = view() else { return };
        let mut doc = Document::new(16, 16);
        doc.push_layer(layer(
            gradient(16, 16, 255, 4),
            0.0,
            0.0,
            100.0,
            BlendMode::Normal,
        ));
        doc.push_layer(layer(
            gradient(8, 8, 255, 5),
            14.0,
            -6.0,
            100.0,
            BlendMode::Screen,
        ));
        assert_parity(&doc, &mut view, 1);
    }

    /// Low-frequency gradient (a half-pixel shift moves values by ~2).
    fn smooth(w: u32, h: u32) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_fn(
            w,
            h,
            |x, y| Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]),
        )))
    }

    #[test]
    fn rotated_layers_stay_close_to_the_cpu() {
        // Same geometry (orientation, placement); the CPU samples at pixel
        // corners, the GPU at pixel centers: half a pixel apart, hence a
        // smooth image and a small bound.
        let Some(mut view) = view() else { return };
        let mut doc = Document::new(32, 32);
        doc.push_layer(layer(smooth(32, 32), 0.0, 0.0, 100.0, BlendMode::Normal));
        let mut rotated = PixelLayer::new("r", smooth(12, 12));
        rotated.transform.offset_x = 10.0;
        rotated.transform.offset_y = 10.0;
        rotated.transform.rotation_deg = 30.0;
        doc.push_layer(LayerNode::Pixel(rotated));
        let resolve = |id| doc.appearance_image_any(id);
        let frame = view.render(&doc, &resolve).expect("frame");
        assert_eq!(frame.doc_size, (32, 32));
        let gpu = view.read_rgba().expect("relecture");
        let mut stats = CompositeStats::default();
        let cpu = doc
            .composite_root_range_with(0..doc.root.len(), &resolve, &mut stats)
            .expect("cpu");
        // Center of the rotated layer: identical within resampling noise.
        let (cx, cy) = (
            (16.0 + frame.origin.0) as u32,
            (16.0 + frame.origin.1) as u32,
        );
        let g = &gpu[((cy * frame.size.0 + cx) * 4) as usize..][..4];
        let c = cpu.image.get_pixel(cx, cy).0;
        for k in 0..4 {
            assert!(
                (i16::from(g[k]) - i16::from(c[k])).abs() <= 6,
                "{g:?} vs {c:?}"
            );
        }
    }

    #[test]
    fn textures_are_reused_until_the_appearance_changes() {
        let Some(mut view) = view() else { return };
        let mut doc = Document::new(8, 8);
        doc.push_layer(layer(
            gradient(8, 8, 255, 8),
            0.0,
            0.0,
            100.0,
            BlendMode::Normal,
        ));
        doc.push_layer(layer(
            gradient(4, 4, 255, 9),
            1.0,
            1.0,
            100.0,
            BlendMode::Normal,
        ));
        let resolve = |id| doc.appearance_image_any(id);
        view.render(&doc, &resolve).expect("frame");
        assert_eq!(view.cached_layers(), 2);
        // A move keeps every appearance: no upload, same cache.
        if let LayerNode::Pixel(l) = &mut doc.root[1] {
            l.transform.offset_x = 3.0;
        }
        let resolve = |id| doc.appearance_image_any(id);
        view.render(&doc, &resolve).expect("frame");
        assert_eq!(view.cached_layers(), 2);
        assert_parity(&doc, &mut view, 1);
    }

    #[test]
    fn groups_match_the_cpu_reference() {
        let Some(mut view) = view() else { return };
        for (group_mode, child_mode) in [
            (BlendMode::Normal, BlendMode::Normal),
            (BlendMode::Multiply, BlendMode::Normal),
            (BlendMode::Normal, BlendMode::Screen),
            (BlendMode::Overlay, BlendMode::Darken),
        ] {
            let mut doc = Document::new(24, 16);
            doc.push_layer(layer(
                gradient(24, 16, 255, 10),
                0.0,
                0.0,
                100.0,
                BlendMode::Normal,
            ));
            let mut inner = GroupLayer::new(
                "dedans",
                vec![layer(gradient(6, 6, 220, 11), 2.0, 2.0, 100.0, child_mode)],
            );
            inner.opacity = 80.0;
            let mut group = GroupLayer::new(
                "groupe",
                vec![
                    layer(
                        gradient(12, 10, 255, 12),
                        4.0,
                        3.0,
                        100.0,
                        BlendMode::Normal,
                    ),
                    layer(gradient(8, 8, 180, 13), 9.0, 5.0, 70.0, child_mode),
                    LayerNode::Group(inner),
                ],
            );
            group.opacity = 60.0;
            group.blend_mode = group_mode;
            doc.push_layer(LayerNode::Group(group));
            assert_parity(&doc, &mut view, 1);
        }
    }

    #[test]
    fn unsupported_content_keeps_the_cpu_path() {
        let mut doc = Document::new(8, 8);
        doc.push_layer(LayerNode::Group(GroupLayer::new("g", Vec::new())));
        assert!(GpuView::supports(&doc), "groupe simple pris en charge");
        let mut masked = GroupLayer::new("m", Vec::new());
        masked.masks.push(crate::document::LayerMask::full(8, 8));
        doc.push_layer(LayerNode::Group(masked));
        assert!(!GpuView::supports(&doc), "masque de groupe : chemin CPU");
        doc.root[1].set_visible(false);
        assert!(GpuView::supports(&doc), "un nœud masqué ne bloque pas");
        doc.push_layer(LayerNode::Adjustment(
            crate::document::AdjustmentLayer::new("réglage", Vec::new()),
        ));
        assert!(!GpuView::supports(&doc), "calque de réglage : chemin CPU");
    }
}
