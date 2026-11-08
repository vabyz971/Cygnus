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

/// Largest difference allowed between adapters on one channel of a 16-bit
/// export, in 16-bit units (docs/decisions/0016). The export path has no
/// transcendental function since 0021 (composition in encoded values); the
/// f32 divisions of unpacking and unpremultiplying may still round a tie
/// differently from one driver to another, by one unit.
pub const TOLERANCE_UNITS: u16 = 1;

/// Largest mean difference between adapters over all channels of an
/// image, in 16-bit units (docs/decisions/0016).
pub const MEAN_UNITS_BOUND: f64 = 0.01;

/// Largest difference allowed between adapters for images with filters
/// (docs/decisions/0023 and 0040, cases named `filter-*`): the blur rounds
/// its two passes to 16 bits, where a tie may round differently from one
/// adapter to another (2 units measured with the blur alone), and the
/// unsharp mask amplifies that gap by 1 + its amount (3 units measured at
/// 150 % between the RTX 3080 Ti and lavapipe).
#[allow(dead_code)]
pub const FILTER_TOLERANCE_UNITS: u16 = 4;

/// Outcome of comparing two 16-bit exports.
#[derive(Debug)]
pub struct Comparison {
    /// Pixels with any difference.
    pub differing: usize,
    /// Pixels with a channel beyond [`TOLERANCE_UNITS`].
    pub beyond_tolerance: usize,
    /// Largest difference of a channel, in 16-bit units.
    pub max_units: u16,
    /// Mean difference over all channels, in 16-bit units.
    pub mean_units: f64,
    /// Total pixels.
    pub total: usize,
}

impl Comparison {
    /// Tolerance of docs/decisions/0016: no channel of any pixel beyond
    /// [`TOLERANCE_UNITS`], and a bounded mean difference
    /// ([`MEAN_UNITS_BOUND`]). The share of differing pixels is reported,
    /// not limited: one color off by one unit fills a whole flat area.
    pub fn passes(&self) -> bool {
        self.beyond_tolerance == 0 && self.mean_units <= MEAN_UNITS_BOUND
    }

    /// Tolerance of the images with filters: [`FILTER_TOLERANCE_UNITS`],
    /// the same mean bound.
    #[allow(dead_code)]
    pub fn passes_with_filters(&self) -> bool {
        self.max_units <= FILTER_TOLERANCE_UNITS && self.mean_units <= MEAN_UNITS_BOUND
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
        max_units: 0,
        mean_units: 0.0,
        total: (expected.width * expected.height) as usize,
    };
    let mut sum = 0u64;
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
        let mut beyond = false;
        for c in 0..4 {
            let units = a[c].abs_diff(b[c]);
            sum += u64::from(units);
            comparison.max_units = comparison.max_units.max(units);
            beyond |= units > TOLERANCE_UNITS;
        }
        if beyond {
            comparison.beyond_tolerance += 1;
        }
    }
    comparison.mean_units = sum as f64 / (comparison.total * 4) as f64;
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

/// Asserts that a 16-bit channel equals a hand-computed value within
/// [`TOLERANCE_UNITS`].
pub fn assert_close(actual: u16, expected: u16, what: &str) {
    assert!(
        actual.abs_diff(expected) <= TOLERANCE_UNITS,
        "{what}: got {actual}, expected {expected} (tolerance {TOLERANCE_UNITS})"
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
    canvas.prepare(renderer, document, changes, &view).unwrap();
    // Frames until the work left by a frame budget is done.
    let mut frames = 0;
    while canvas.is_pending() {
        frames += 1;
        assert!(frames < 100_000, "the canvas never finishes its work");
        if frames % 64 == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        canvas
            .prepare(renderer, document, &PhotoChanges::default(), &view)
            .unwrap();
    }
    read_canvas(gpu, canvas, (width, height))
}

/// Draws what the canvas shows now, without preparing it, on an offscreen
/// 8-bit target and reads it back as RGBA bytes, row by row.
#[allow(dead_code)]
pub fn read_canvas(gpu: &Gpu, canvas: &Canvas, (width, height): (u32, u32)) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
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
