// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Histogram pass (REG-4, docs/decisions/0025) and coarse fallback of the
//! canvas: counts of the tiles shown, read back without blocking.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, draw_on, setup};
use cygnus_commands::ChangeSet;
use cygnus_photo_core::PhotoChanges;
use cygnus_photo_render::{Canvas, CanvasView, HistogramPass};

fn view(scale: f32, size: (u32, u32)) -> CanvasView {
    CanvasView {
        origin: [0.0, 0.0],
        scale,
        viewport: [0.0, 0.0, size.0 as f32, size.1 as f32],
        background: [0.0; 3],
        checker: [[0.5; 3], [0.4; 3]],
        checker_size: 8.0,
    }
}

#[test]
fn the_histogram_counts_the_pixels_shown() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    // 64 × 32: left half red (204, 0, 0), right half gray 0.4 (102).
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 64, "height": 32, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "a"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 32, "height": 32}, "color": [0.8, 0.0, 0.0, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 32, "y": 0, "width": 32, "height": 32}, "color": [0.4, 0.4, 0.4, 1.0]}}
    ]"#,
    );
    let mut canvas = Canvas::with_capacity(&renderer, wgpu::TextureFormat::Rgba8Unorm, 32).unwrap();
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        (64, 32),
        view(1.0, (64, 32)),
    );
    let mut pass = HistogramPass::new(&renderer);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    pass.record(&renderer, &canvas, &mut encoder);
    gpu.queue.submit([encoder.finish()]);
    let mut histogram = None;
    for _ in 0..100 {
        histogram = pass.poll(&gpu.device);
        if histogram.is_some() {
            break;
        }
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
    let histogram = histogram.expect("histogram read back");
    assert_eq!(histogram.pixels, 64 * 32);
    let half = 32 * 32;
    // 0.8 × 256 = 204.8 -> bin 204; 0.4 × 256 = 102.4 -> bin 102.
    assert_eq!(histogram.bins[0][204], half);
    assert_eq!(histogram.bins[0][102], half);
    assert_eq!(histogram.bins[1][0], half);
    assert_eq!(histogram.bins[1][102], half);
    assert!(pass.is_idle());
}

#[test]
fn a_cache_too_small_for_the_view_is_reported_as_coarse() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 2048, "height": 2048, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "a"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 2048, "height": 2048}, "color": [0.5, 0.5, 0.5, 1.0]}}
    ]"#,
    );
    // 16 tiles cannot hold the 64 visible tiles of 100 %.
    let mut canvas = Canvas::with_capacity(&renderer, wgpu::TextureFormat::Rgba8Unorm, 16).unwrap();
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        (512, 512),
        view(1.0, (2048, 2048)),
    );
    assert!(canvas.is_coarse());
    let mut roomy = Canvas::with_capacity(&renderer, wgpu::TextureFormat::Rgba8Unorm, 128).unwrap();
    draw_on(
        &gpu,
        &renderer,
        &mut roomy,
        &document,
        &PhotoChanges::everything(),
        (512, 512),
        view(0.25, (512, 512)),
    );
    assert!(!roomy.is_coarse());
}
