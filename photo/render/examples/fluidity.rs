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

//! Headless fluidity benchmark (docs/decisions/0014).
//!
//! cargo run --release -p cygnus-photo-render --example fluidity -- \
//!     [--out report.json] [--viewport 2560x1440] [--cache-slots N]
//!     [--no-frame-budget] [--large-layers 40]
//!
//! `--cache-slots` fixes the canvas cache instead of the computed budget,
//! to measure panning over tiles that are not in the cache.
//! `--no-frame-budget` finishes every frame, as before docs/decisions/0034,
//! for comparisons. `--large-layers N` plays the path on the 8 GiB scenario
//! of docs/decisions/0024 (N pixel layers of 24 Mpx, 16 bits) instead.
//!
//! Each frame updates the canvas and draws it on an offscreen target, then
//! waits for the GPU: the frame time includes all GPU work, but neither the
//! presentation to a screen nor the vertical synchronization. The run in
//! the application (`CYGNUS_BENCH=fluidity`) is the reference measurement.

use std::error::Error;
use std::time::Instant;

use cygnus_commands::{ChangeSet, CommandBus, Source};
use cygnus_photo_core::PhotoChanges;
use cygnus_photo_render::bench::{self, FrameSample, RunInfo};
use cygnus_photo_render::{Canvas, CanvasView, FrameBudget, Renderer, gpu};

/// Repetitions of a settling frame at most.
const MAX_REPEATS: u32 = 10_000;

fn main() -> Result<(), Box<dyn Error>> {
    let mut out = String::from("fluidity-headless.json");
    let mut viewport = [2560u32, 1440];
    let mut cache_slots: Option<u32> = None;
    let mut frame_budget = Some(FrameBudget::DEFAULT);
    let mut large: Option<u64> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().ok_or("--out expects a path")?,
            "--viewport" => {
                let value = args.next().ok_or("--viewport expects WIDTHxHEIGHT")?;
                let (w, h) = value
                    .split_once('x')
                    .ok_or("--viewport expects WIDTHxHEIGHT")?;
                viewport = [w.parse()?, h.parse()?];
            }
            "--cache-slots" => {
                cache_slots = Some(
                    args.next()
                        .ok_or("--cache-slots expects a number")?
                        .parse()?,
                );
            }
            "--no-frame-budget" => frame_budget = None,
            "--large-layers" => {
                large = Some(
                    args.next()
                        .ok_or("--large-layers expects a number")?
                        .parse()?,
                );
            }
            other => return Err(format!("unknown argument {other:?}").into()),
        }
    }

    let started = Instant::now();
    let mut document = match large {
        Some(layers) => {
            eprintln!("building the 24 Mpx, {layers}-layer document...");
            bench::build_large_document(layers)?
        }
        None => {
            eprintln!("building the 24 Mpx, 10-layer document...");
            bench::build_document()?
        }
    };
    eprintln!("built in {:.1} s", started.elapsed().as_secs_f64());

    let gpu = gpu::open_headless()?;
    eprintln!("adapter: {} ({:?})", gpu.info.name, gpu.info.backend);
    let renderer = Renderer::with_memory_budget(
        &gpu.device,
        &gpu.queue,
        gpu::source_memory_budget(gpu.info.device_type),
    )?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut canvas = match cache_slots {
        Some(slots) => Canvas::with_capacity(&renderer, format, slots)?,
        None => Canvas::new(
            &renderer,
            format,
            gpu::cache_memory_budget(gpu.info.device_type),
        )?,
    };
    canvas.set_frame_budget(frame_budget);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("benchmark target"),
        size: wgpu::Extent3d {
            width: viewport[0],
            height: viewport[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let rect = [0.0, 0.0, viewport[0] as f32, viewport[1] as f32];

    let mut bus = CommandBus::new();
    let mut changes = PhotoChanges::everything();
    let mut samples = Vec::new();
    let path = bench::camera_path([rect[2], rect[3]]);
    let mut index = 0;
    let mut repeats = 0;
    while let Some(frame) = path.get(index).copied() {
        let start = Instant::now();
        // The 8 GiB document has no curves adjustment: no curve edit.
        let curve = |edit: &bench::Edit| matches!(edit, bench::Edit::Curve(_));
        if let Some(edit) = frame
            .edit
            .filter(|edit| repeats == 0 && !(large.is_some() && curve(edit)))
        {
            let cursor = bus.revision();
            bus.execute(&mut document, bench::edit_command(edit), Source::Test)?;
            changes = bus.changes_since(cursor).changes;
        }
        let view = CanvasView {
            origin: bench::origin(&frame, rect),
            scale: frame.scale,
            viewport: rect,
            background: [0.08, 0.09, 0.11],
            checker: [[0.29, 0.31, 0.35], [0.23, 0.24, 0.28]],
            checker_size: 8.0,
        };
        canvas.set_preview(frame.preview);
        canvas.prepare(&renderer, &document, &changes, &view)?;
        changes = PhotoChanges::default();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
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
        gpu.queue.submit([encoder.finish()]);
        gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        samples.push(FrameSample {
            phase: frame.phase,
            time: start.elapsed(),
            stats: canvas.stats(),
            sources: renderer.source_stats(),
            covered: canvas.is_covered(),
        });
        if frame.settle && canvas.is_pending() && repeats < MAX_REPEATS {
            repeats += 1;
        } else {
            index += 1;
            repeats = 0;
        }
    }

    let info = RunInfo {
        mode: "headless".to_owned(),
        timing: "wall time of canvas update and draw on an offscreen target, waiting for the GPU; no presentation, no vsync".to_owned(),
        adapter: format!("{} | {:?} | {} {}", gpu.info.name, gpu.info.backend, gpu.info.driver, gpu.info.driver_info),
        viewport,
        cache_bytes: u64::from(canvas.capacity()) * 256 * 256 * 8,
        document_bytes: bench::document_bytes(&document),
        source_slots: renderer.source_stats().capacity,
        layers: large.map(|layers| (layers, layers)),
    };
    let report = bench::report(&info, &samples);
    std::fs::write(&out, serde_json::to_string_pretty(&report)? + "\n")?;
    eprintln!("report written to {out}");
    Ok(())
}
