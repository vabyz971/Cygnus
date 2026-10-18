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

//! Displaying the engine's GPU frames (zero-copy canvas).
//!
//! The session composites on eframe's own device; the frame texture is
//! registered ONCE with egui's renderer (mip-mapped sampler: smooth when
//! zoomed out, real pixels when zoomed in) and only re-registered when
//! the engine replaces the texture (frame size changed). Its content is
//! updated in place on the shared queue, so the canvas never uploads
//! pixels from the CPU while the GPU path is active.

use eframe::egui_wgpu::RenderState;
use photo_engine::gpu_view::GpuFrame;

/// A frame texture registered with egui.
pub struct GpuDisplay {
    /// Registered texture (compared to detect replacements).
    pub texture: wgpu::Texture,
    /// egui handle of the texture.
    pub id: egui::TextureId,
}

/// Linear filtering across mips (the frame carries a full chain).
fn sampler() -> wgpu::SamplerDescriptor<'static> {
    wgpu::SamplerDescriptor {
        label: Some("photo-gpu-frame"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    }
}

/// Registers `frame`'s texture (or re-points the existing handle when the
/// engine replaced it). No-op when it is already the registered one.
pub fn sync(render_state: &RenderState, display: &mut Option<GpuDisplay>, frame: &GpuFrame) {
    if display.as_ref().is_some_and(|d| d.texture == frame.texture) {
        return;
    }
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut renderer = render_state.renderer.write();
    match display {
        Some(current) => {
            renderer.update_egui_texture_from_wgpu_texture_with_sampler_options(
                &render_state.device,
                &view,
                sampler(),
                current.id,
            );
            current.texture = frame.texture.clone();
        }
        None => {
            let id = renderer.register_native_texture_with_sampler_options(
                &render_state.device,
                &view,
                sampler(),
            );
            *display = Some(GpuDisplay {
                texture: frame.texture.clone(),
                id,
            });
        }
    }
}

/// Texture to draw for `frame`, when its registration is current.
#[must_use]
pub fn texture_for(
    display: Option<&GpuDisplay>,
    frame: &GpuFrame,
) -> Option<(egui::TextureId, egui::Vec2)> {
    display
        .filter(|d| d.texture == frame.texture)
        .map(|d| (d.id, egui::vec2(frame.size.0 as f32, frame.size.1 as f32)))
}
