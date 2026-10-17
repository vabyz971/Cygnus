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

//! Drag proxy, canvas side: live layer moves at display rate.
//!
//! At gesture start the worker splits the stack around the dragged
//! layer (`PrepareDragProxy`). Once ready, the canvas stops streaming
//! `SetTransform` frames and draws, every frame and GPU-only:
//! flattened below → the layer texture through the live transform
//! (textured quad, opacity as tint) → flattened above. The exact
//! composite runs once on release; the proxy stays on screen until that
//! render lands, so the hand-off never flashes the old position.
//!
//! Lifecycle: `Requested` → `Ready` | `Unavailable` (worker refused:
//! the drag keeps live renders) → `Ending` (released, waiting for the
//! commit render) → dropped.

use egui::{Color32, Mesh, Pos2, Rect, Shape, TextureHandle, TextureOptions, epaint::Vertex};
use photo_engine::Transform2D;
use uuid::Uuid;

use crate::state::PreviewGeom;
use crate::ui::engine_bridge::{PreviewImage, ProxyLayerImage};
use crate::ui::{doc_to_screen, frame_corners};

/// One flattened part: texture + its document-space rectangle.
pub struct ProxyPartTexture {
    texture: TextureHandle,
    /// Document-space bounds (pixels): min corner, max corner.
    doc_min: (f32, f32),
    doc_max: (f32, f32),
}

/// Textures of a ready proxy.
pub struct DragProxy {
    /// Dragged layer.
    pub layer: Uuid,
    below: Option<ProxyPartTexture>,
    above: Option<ProxyPartTexture>,
    layer_texture: TextureHandle,
    /// Carrier dimensions the quad spans (must equal the drag's).
    pub source_dims: (u32, u32),
    /// Layer opacity (0..=100).
    opacity: f32,
}

/// Proxy lifecycle (see module docs).
pub enum DragProxyState {
    /// Split requested from the worker for `layer`.
    Requested(Uuid),
    /// The worker cannot split around `layer`: live renders.
    Unavailable(Uuid),
    /// Drawn GPU-side during the drag.
    Ready(DragProxy),
    /// Released with this final transform: drawn until the commit render.
    Ending(DragProxy, Transform2D),
}

impl DragProxyState {
    /// Layer the state refers to.
    pub fn layer(&self) -> Uuid {
        match self {
            Self::Requested(id) | Self::Unavailable(id) => *id,
            Self::Ready(proxy) | Self::Ending(proxy, _) => proxy.layer,
        }
    }

    /// True when the drag of `layer` is drawn by the proxy (live
    /// `SetTransform` frames are then unnecessary).
    pub fn drives(&self, layer: Uuid) -> bool {
        matches!(self, Self::Ready(proxy) if proxy.layer == layer)
    }
}

/// Document-space bounds of a part preview (inverse of the preview
/// geometry: origin and size are in thumbnail pixels, the scale is
/// `full / thumb`).
fn part_doc_bounds(image: &PreviewImage) -> ((f32, f32), (f32, f32)) {
    let k = if image.width > 0 {
        image.full_width as f32 / image.width as f32
    } else {
        1.0
    };
    let min = (-image.origin_x * k, -image.origin_y * k);
    (
        min,
        (
            min.0 + image.full_width as f32,
            min.1 + image.full_height as f32,
        ),
    )
}

fn part_texture(ctx: &egui::Context, name: &str, image: PreviewImage) -> ProxyPartTexture {
    let (doc_min, doc_max) = part_doc_bounds(&image);
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [image.width as usize, image.height as usize],
        &image.rgba,
    );
    ProxyPartTexture {
        texture: ctx.load_texture(name, color, TextureOptions::LINEAR),
        doc_min,
        doc_max,
    }
}

impl DragProxy {
    /// Uploads the worker's split (three textures, once per gesture).
    pub fn upload(
        ctx: &egui::Context,
        layer: Uuid,
        below: Option<PreviewImage>,
        image: ProxyLayerImage,
        above: Option<PreviewImage>,
    ) -> Self {
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        );
        Self {
            layer,
            below: below.map(|b| part_texture(ctx, "photo_proxy_below", b)),
            above: above.map(|a| part_texture(ctx, "photo_proxy_above", a)),
            layer_texture: ctx.load_texture("photo_proxy_layer", color, TextureOptions::LINEAR),
            source_dims: (image.source_width, image.source_height),
            opacity: image.opacity,
        }
    }
}

/// Screen placement inputs (same mapping as the canvas overlays).
#[derive(Clone, Copy)]
pub struct ProxyMapping {
    /// On-screen rect of the main preview texture.
    pub dest: Rect,
    /// Main preview geometry.
    pub geom: PreviewGeom,
}

impl ProxyMapping {
    fn to_screen(self, doc: (f32, f32)) -> Option<Pos2> {
        doc_to_screen(
            doc,
            self.dest,
            self.geom.origin,
            self.geom.thumb_to_doc(),
            self.geom.thumb_size.x,
        )
    }
}

/// Textured quad through four screen corners (tl, tr, br, bl).
fn quad(texture: egui::TextureId, corners: [Pos2; 4], tint: Color32) -> Shape {
    let mut mesh = Mesh::with_texture(texture);
    let uvs = [
        egui::pos2(0.0, 0.0),
        egui::pos2(1.0, 0.0),
        egui::pos2(1.0, 1.0),
        egui::pos2(0.0, 1.0),
    ];
    for (pos, uv) in corners.iter().zip(uvs) {
        mesh.vertices.push(Vertex {
            pos: *pos,
            uv,
            color: tint,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    Shape::mesh(mesh)
}

fn part_shape(part: &ProxyPartTexture, mapping: ProxyMapping) -> Option<Shape> {
    let a = mapping.to_screen(part.doc_min)?;
    let b = mapping.to_screen(part.doc_max)?;
    Some(Shape::image(
        part.texture.id(),
        Rect::from_two_pos(a, b),
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    ))
}

/// Shapes of one frame, split by paint slot: `under` (background +
/// below), `layer` (the moving quad, clipped to the document when the
/// preview is clipped) and `over` (above).
pub struct ProxyShapes {
    pub under: Vec<Shape>,
    pub layer: Vec<Shape>,
    pub over: Vec<Shape>,
}

/// Builds the frame's shapes for `transform`. `backdrop` hides the main
/// preview texture (the viewport background color).
pub fn proxy_shapes(
    proxy: &DragProxy,
    transform: &Transform2D,
    mapping: ProxyMapping,
    backdrop: Color32,
) -> ProxyShapes {
    let mut under = vec![Shape::rect_filled(mapping.dest, 0.0, backdrop)];
    under.extend(proxy.below.as_ref().and_then(|b| part_shape(b, mapping)));
    let corners = frame_corners(transform, proxy.source_dims);
    let screen: Option<Vec<Pos2>> = corners.iter().map(|c| mapping.to_screen(*c)).collect();
    let tint = Color32::WHITE.gamma_multiply((proxy.opacity / 100.0).clamp(0.0, 1.0));
    let layer = match screen.as_deref() {
        Some([a, b, c, d]) => vec![quad(proxy.layer_texture.id(), [*a, *b, *c, *d], tint)],
        _ => Vec::new(),
    };
    let over = proxy
        .above
        .as_ref()
        .and_then(|a| part_shape(a, mapping))
        .into_iter()
        .collect();
    ProxyShapes { under, layer, over }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(width: u32, full_width: u32, origin: f32) -> PreviewImage {
        PreviewImage {
            width,
            height: width,
            rgba: vec![0; (width * width * 4) as usize],
            full_width,
            full_height: full_width,
            origin_x: origin,
            origin_y: origin,
            doc_width: 100,
            doc_height: 100,
        }
    }

    #[test]
    fn part_bounds_invert_the_preview_geometry() {
        // Unscaled: doc origin 10 px inside the buffer.
        assert_eq!(
            part_doc_bounds(&preview(120, 120, 10.0)),
            ((-10.0, -10.0), (110.0, 110.0))
        );
        // Half-size thumbnail: origin 5 thumb px = 10 doc px.
        assert_eq!(
            part_doc_bounds(&preview(60, 120, 5.0)),
            ((-10.0, -10.0), (110.0, 110.0))
        );
    }

    #[test]
    fn state_drives_only_when_ready_for_that_layer() {
        let id = Uuid::new_v4();
        assert!(!DragProxyState::Requested(id).drives(id));
        assert!(!DragProxyState::Unavailable(id).drives(id));
        assert_eq!(DragProxyState::Requested(id).layer(), id);
    }

    #[test]
    fn shapes_follow_the_transform() {
        let ctx = egui::Context::default();
        let layer = Uuid::new_v4();
        let proxy = DragProxy::upload(
            &ctx,
            layer,
            Some(preview(10, 10, 0.0)),
            ProxyLayerImage {
                width: 4,
                height: 2,
                rgba: vec![255; 32],
                source_width: 4,
                source_height: 2,
                opacity: 50.0,
            },
            None,
        );
        assert!(DragProxyState::Ready(proxy).drives(layer));
        let proxy = DragProxy::upload(
            &ctx,
            layer,
            None,
            ProxyLayerImage {
                width: 4,
                height: 2,
                rgba: vec![255; 32],
                source_width: 4,
                source_height: 2,
                opacity: 50.0,
            },
            None,
        );
        let mapping = ProxyMapping {
            dest: Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0)),
            geom: PreviewGeom {
                thumb_size: egui::vec2(100.0, 100.0),
                full_size: egui::vec2(100.0, 100.0),
                origin: egui::vec2(0.0, 0.0),
                doc_size: egui::vec2(100.0, 100.0),
            },
        };
        let moved = Transform2D {
            offset_x: 20.0,
            offset_y: 30.0,
            ..Transform2D::default()
        };
        let shapes = proxy_shapes(&proxy, &moved, mapping, Color32::TRANSPARENT);
        assert_eq!(shapes.under.len(), 1, "fond seul (pas de dessous)");
        assert!(shapes.over.is_empty());
        let Some(Shape::Mesh(mesh)) = shapes.layer.first() else {
            panic!("quad attendu");
        };
        assert_eq!(mesh.vertices[0].pos, egui::pos2(20.0, 30.0));
        assert_eq!(mesh.vertices[2].pos, egui::pos2(24.0, 32.0));
    }
}
