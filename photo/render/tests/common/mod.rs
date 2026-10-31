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

//! Helpers shared by the GPU tests of the renderer.
//!
//! Without a GPU the tests are skipped with a message, unless
//! `CYGNUS_REQUIRE_GPU=1` (CI GPU jobs), which makes them fail.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::mpsc;

use cygnus_commands::Source;
use cygnus_photo_core::script::{SCRIPT_VERSION, Script};
use cygnus_photo_core::{Document, PhotoChanges, PhotoCommand};
use cygnus_photo_render::gpu::{self, Gpu};
use cygnus_photo_render::{Canvas, CanvasView, ExportBits, Image, Renderer};

/// The headless device and a renderer, or `None` (test skipped).
pub fn setup() -> Option<(Gpu, Renderer)> {
    let required = std::env::var("CYGNUS_REQUIRE_GPU").is_ok_and(|value| value == "1");
    match gpu::open_headless() {
        Ok(gpu) => {
            let renderer = Renderer::new(&gpu.device, &gpu.queue).unwrap();
            Some((gpu, renderer))
        }
        Err(error) if !required => {
            println!("skipped, no GPU: {error}");
            None
        }
        Err(error) => panic!("GPU required but unavailable: {error}"),
    }
}

/// Builds a document from commands, through the bus.
pub fn build(commands: Vec<PhotoCommand>) -> Document {
    Script {
        version: SCRIPT_VERSION,
        commands,
    }
    .build(Source::Test)
    .unwrap()
}

/// Builds a document from a JSON command list (the `commands` of a script).
pub fn build_json(commands: &str) -> Document {
    let script = format!(r#"{{"version": {SCRIPT_VERSION}, "commands": {commands}}}"#);
    Script::parse(&script).unwrap().build(Source::Test).unwrap()
}

pub fn export16(renderer: &Renderer, document: &Document) -> Image {
    renderer.export(document, ExportBits::Sixteen).unwrap()
}

// Measuring distances needs the sRGB transfer function on the CPU. It is
// used only to express the tolerance in linear light, never to produce an
// expected image (docs/decisions/0013).
fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear value of a 16-bit export channel (alpha is already linear).
fn linear(value: u16, channel: usize) -> f64 {
    let v = f64::from(value) / 65535.0;
    if channel == 3 { v } else { srgb_to_linear(v) }
}

/// Distance between two consecutive f16 values around `x` (x >= 0).
fn f16_step(x: f64) -> f64 {
    let min_normal = 2f64.powi(-14);
    if x < min_normal {
        2f64.powi(-24)
    } else {
        2f64.powi(x.log2().floor() as i32 - 10)
    }
}

/// Largest linear distance two 16-bit exports may have on one channel:
/// one f16 step of the composite, plus one step of the 16-bit
/// quantization of the export.
fn tolerance(a: u16, b: u16, channel: usize) -> f64 {
    let high = a.max(b);
    let quantization = linear(high.saturating_add(1), channel) - linear(high, channel);
    f16_step(linear(high, channel)) + quantization.abs()
}

/// Outcome of comparing two 16-bit exports.
#[derive(Debug)]
pub struct Comparison {
    /// Pixels with any difference.
    pub differing: usize,
    /// Pixels with a channel beyond one f16 step.
    pub beyond_tolerance: usize,
    /// Total pixels.
    pub total: usize,
}

impl Comparison {
    /// Tolerance of docs/decisions/0013 as amended by 0016: no channel of
    /// any pixel beyond one f16 step (plus one export quantization step).
    /// The share of differing pixels is reported, not limited: one color
    /// off by one unit fills a whole flat area.
    pub fn passes(&self) -> bool {
        self.beyond_tolerance == 0
    }
}

pub fn compare(expected: &Image, actual: &Image) -> Comparison {
    assert_eq!(
        (expected.width, expected.height),
        (actual.width, actual.height)
    );
    assert_eq!(expected.bits, ExportBits::Sixteen);
    assert_eq!(actual.bits, ExportBits::Sixteen);
    let mut comparison = Comparison {
        differing: 0,
        beyond_tolerance: 0,
        total: (expected.width * expected.height) as usize,
    };
    for (a, b) in expected
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.data.as_chunks::<4>().0.iter())
    {
        if a == b {
            continue;
        }
        comparison.differing += 1;
        let beyond =
            (0..4).any(|c| (linear(a[c], c) - linear(b[c], c)).abs() > tolerance(a[c], b[c], c));
        if beyond {
            comparison.beyond_tolerance += 1;
        }
    }
    comparison
}

/// Image highlighting differences: white where pixels differ.
pub fn difference(expected: &Image, actual: &Image) -> Image {
    let mut data = Vec::with_capacity(expected.data.len());
    for (a, b) in expected
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.data.as_chunks::<4>().0.iter())
    {
        let value = if a == b { 0 } else { u16::MAX };
        data.extend([value, value, value, u16::MAX]);
    }
    Image {
        width: expected.width,
        height: expected.height,
        bits: ExportBits::Sixteen,
        data,
    }
}

/// Asserts that a 16-bit channel equals a hand-computed value within the
/// tolerance (one f16 step plus quantization).
pub fn assert_close(actual: u16, expected: f64, channel: usize, what: &str) {
    let expected_value = expected.round().clamp(0.0, 65535.0) as u16;
    let distance = (linear(actual, channel) - linear(expected_value, channel)).abs();
    let allowed = tolerance(actual, expected_value, channel);
    assert!(
        distance <= allowed,
        "{what}: got {actual}, expected {expected} (linear distance {distance}, allowed {allowed})"
    );
}

/// Draws the canvas on an offscreen 8-bit target and reads it back as RGBA
/// bytes, row by row.
pub fn draw_on(
    gpu: &Gpu,
    renderer: &Renderer,
    canvas: &mut Canvas,
    document: &Document,
    changes: &PhotoChanges,
    (width, height): (u32, u32),
    view: CanvasView,
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    canvas.prepare(renderer, document, changes, &view).unwrap();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    let row = (width * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        canvas.paint(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * row) as usize;
        pixels.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
    }
    pixels
}
