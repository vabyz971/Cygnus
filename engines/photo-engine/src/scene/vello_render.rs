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

//! GPU vector rendering via Vello (spec §2: SDF-grade curves at draw,
//! GPU for rendering).
//!
//! [`render_vector_gpu`] rasterizes a [`VectorLayer`] on the shared
//! [`GpuContext`](crate::gpu::GpuContext) device (wgpu 30, same major
//! as Vello 0.11 — one device, no duplication) and reads the pixels
//! back for the CPU composite pipeline. Any failure (no adapter,
//! render error, readback error) returns `None`: callers fall back
//! to the CPU scanline rasterizer ([`super::rasterize`]), so headless
//! machines and CI keep deterministic pixels.
//!
//! Conventions: even-odd fill (like the CPU path), transparent base,
//! straight-alpha RGBA8 output (Vello's premultiplied edges are
//! un-premultiplied on readback — the composite expects straight
//! alpha). Layer opacity applies at composite time, never here.

use std::sync::{Mutex, OnceLock};

use image::DynamicImage;

use super::{VectorLayer, to_kurbo, translate, vector_bounds};
use crate::gpu::GpuContext;

/// Cached Vello renderer (shader compilation happens once; `Renderer`
/// is `Send` but not `Sync`, hence the mutex).
static VELLO: OnceLock<Mutex<vello::Renderer>> = OnceLock::new();

/// Returns the shared renderer, initializing it on first use.
/// `None` without a GPU adapter or on shader-compile failure
/// (initialization is retried on the next call).
fn renderer() -> Option<std::sync::MutexGuard<'static, vello::Renderer>> {
    if VELLO.get().is_none() {
        let gpu = GpuContext::get()?;
        let renderer = vello::Renderer::new(&gpu.device, vello::RendererOptions::default()).ok()?;
        let _ = VELLO.set(Mutex::new(renderer));
    }
    VELLO.get()?.lock().ok()
}

/// Render a vector layer on the GPU, bounds-shifted like
/// [`super::rasterize_bounds`] (origin returned for placement).
///
/// `None` = render on CPU instead (no adapter or any GPU error).
pub fn render_vector_gpu(layer: &VectorLayer) -> Option<(DynamicImage, datatypes::Vec2)> {
    let bounds = vector_bounds(layer)?;
    let origin = datatypes::Vec2::new(bounds.x0.floor(), bounds.y0.floor());
    let w = (bounds.x1.ceil() - origin.x).max(1.0) as u32;
    let h = (bounds.y1.ceil() - origin.y).max(1.0) as u32;

    let mut scene = vello::Scene::new();
    let color = vello::peniko::Color::from_rgba8(
        layer.fill[0],
        layer.fill[1],
        layer.fill[2],
        layer.fill[3],
    );
    for path in layer.paths.iter().filter(|p| p.closed) {
        let shifted = translate(path, -origin.x, -origin.y);
        let bez = to_kurbo(&shifted);
        scene.fill(
            vello::peniko::Fill::EvenOdd,
            kurbo::Affine::IDENTITY,
            color,
            None,
            &bez,
        );
    }

    let gpu = GpuContext::get()?;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vector"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    {
        let mut renderer = renderer()?;
        renderer
            .render_to_texture(
                &gpu.device,
                &gpu.queue,
                &scene,
                &view,
                &vello::RenderParams {
                    base_color: vello::peniko::Color::from_rgba8(0, 0, 0, 0),
                    width: w,
                    height: h,
                    antialiasing_method: vello::AaConfig::Msaa8,
                },
            )
            .ok()?;
    }
    let pixels = readback_rgba8(&gpu, &texture, w, h)?;
    let buf = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, pixels)?;
    Some((DynamicImage::ImageRgba8(buf), origin))
}

/// Copies an RGBA8 texture back to CPU memory (straight alpha).
///
/// Returns `None` on any copy/map failure (caller falls back to CPU).
fn readback_rgba8(gpu: &GpuContext, texture: &wgpu::Texture, w: u32, h: u32) -> Option<Vec<u8>> {
    // COPY_BYTES_PER_ROW_ALIGNMENT (256): padded staging buffer.
    let pitch = w
        .checked_mul(4)?
        .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vector_readback"),
        size: u64::from(pitch) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vector_readback"),
        });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .ok()?;
    rx.recv().ok()?.ok()?;
    let mapped = staging.slice(..).get_mapped_range().ok()?;
    let stride = w as usize * 4;
    let mut out = vec![0u8; stride * h as usize];
    for (dst, src) in out
        .chunks_exact_mut(stride)
        .zip(mapped.chunks_exact(pitch as usize))
    {
        dst.copy_from_slice(&src[..stride]);
    }
    drop(mapped);
    staging.unmap();
    // Vello blends premultiplied: restore straight alpha for the
    // CPU composite (exact for opaque/transparent, rounded for AA
    // edges — same contract as every GPU→CPU path here).
    for px in out.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if a < 255 {
            let un = |c: u8| ((u32::from(c) * 255 + a / 2) / a).min(255) as u8;
            px[0] = un(px[0]);
            px[1] = un(px[1]);
            px[2] = un(px[2]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rect, star};

    #[test]
    fn gpu_render_matches_cpu_presence_or_falls_back() {
        // No adapter (headless CI) or a real GPU: either way no panic
        // and no silent pixels. With a GPU, opaque/transparent agree
        // with the CPU rasterizer on fully covered/empty pixels.
        let layer = rect(2.0, 2.0, 6.0, 6.0);
        let cpu = crate::rasterize_bounds(&layer).expect("cpu reference");
        match render_vector_gpu(&layer) {
            None => {}
            Some((gpu_img, origin)) => {
                assert_eq!(origin, datatypes::Vec2::new(2.0, 2.0));
                assert_eq!((gpu_img.width(), gpu_img.height()), (6, 6));
                let (g, c) = (gpu_img.to_rgba8(), cpu.0.to_rgba8());
                assert_eq!((g.width(), g.height()), (c.width(), c.height()));
                // Interior fully covered on both paths.
                assert_eq!(g.get_pixel(3, 3)[3], 255);
                assert_eq!(c.get_pixel(3, 3)[3], 255);
            }
        }
    }

    #[test]
    fn gpu_render_empty_layer_reports_none() {
        let empty = VectorLayer {
            paths: vec![],
            fill: [0, 0, 0, 255],
            filter_layers: Vec::new(),
            masks: Vec::new(),
            appearance_version: 0,
        };
        assert!(render_vector_gpu(&empty).is_none());
        let _ = star(0.0, 0.0, 1.0, 0.5, 2);
    }
}
