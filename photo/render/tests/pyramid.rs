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

//! Tile pyramid and bounded cache of the canvas (docs/decisions/0018):
//! eviction and reload, fallback to a coarser level when the budget is too
//! small, coherence of reduced views, large documents.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, draw_on, setup};
use cygnus_commands::{ChangeSet, CommandBus, Source};
use cygnus_photo_core::commands::PixelsFill;
use cygnus_photo_core::{Document, NodeId, PhotoChanges, PhotoCommand, Rect};
use cygnus_photo_render::gpu;
use cygnus_photo_render::{Canvas, CanvasView, ExportBits};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn view(origin: [f32; 2], scale: f32, size: (u32, u32)) -> CanvasView {
    CanvasView {
        origin,
        scale,
        viewport: [0.0, 0.0, size.0 as f32, size.1 as f32],
        background: [0.0, 0.0, 0.0],
        checker: [[0.8; 3], [0.6; 3]],
        checker_size: 8.0,
    }
}

/// An opaque square document of `side` pixels split in four colored
/// quadrants, 8 bits.
fn quadrants(side: u32) -> Document {
    let half = side / 2;
    build_json(&format!(
        r#"[
        {{"command": "document.create", "params": {{"width": {side}, "height": {side}, "depth": "u8"}}}},
        {{"command": "layer.create", "params": {{"name": "q"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": {half}, "height": {half}}}, "color": [0.9, 0.1, 0.1, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": {half}, "y": 0, "width": {half}, "height": {half}}}, "color": [0.1, 0.8, 0.2, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": {half}, "width": {half}, "height": {half}}}, "color": [0.2, 0.3, 0.9, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": {half}, "y": {half}, "width": {half}, "height": {half}}}, "color": [0.95, 0.9, 0.3, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 100, "y": 130, "width": 7, "height": 3}}, "color": [0.0, 0.0, 0.0, 1.0]}}}}
    ]"#
    ))
}

fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [image[i], image[i + 1], image[i + 2], image[i + 3]]
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
}

#[test]
fn evicted_tiles_reload_identically() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = quadrants(2048);
    let mut canvas = Canvas::with_capacity(&renderer, FORMAT, 16).unwrap();
    assert_eq!(canvas.capacity(), 16);
    let size = (512, 512);
    let none = PhotoChanges::default();
    let a = view([0.0, 0.0], 1.0, size);
    let b = view([-1536.0, -1536.0], 1.0, size);

    let first = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        a,
    );
    let after_first = canvas.stats();
    draw_on(&gpu, &renderer, &mut canvas, &document, &none, size, b);
    let again = draw_on(&gpu, &renderer, &mut canvas, &document, &none, size, a);
    let stats = canvas.stats();

    assert!(stats.evicted > 0, "nothing was evicted: {stats:?}");
    assert!(
        stats.composited > after_first.composited,
        "tiles were not rebuilt: {stats:?}"
    );
    assert!(first == again, "reloaded tiles differ");
    // And both still equal the export, pixel for pixel.
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    for (x, y) in [(0, 0), (103, 131), (300, 400), (511, 511)] {
        let expected = export.pixel(x, y).map(|v| v as u8);
        assert_eq!(pixel(&again, 512, x, y), expected, "({x}, {y})");
    }
}

#[test]
fn too_small_a_cache_shows_a_coarser_level_without_failing() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = quadrants(2048);
    let mut canvas = Canvas::with_capacity(&renderer, FORMAT, 16).unwrap();
    // 100 % over 1024 × 1024 needs 36 tiles with the margin: more than 16.
    let size = (1024, 1024);
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        view([0.0, 0.0], 1.0, size),
    );
    assert!(
        canvas.level() > 0,
        "level {} with a 16-tile cache",
        canvas.level()
    );
    // The coarser view still shows the right colors away from edges.
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    let expected = export.pixel(500, 500).map(|v| v as u8);
    assert!(close(pixel(&shown, 1024, 500, 500), expected));
}

#[test]
fn reduced_view_is_coherent() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = quadrants(1024);
    let mut canvas = Canvas::new(
        &renderer,
        FORMAT,
        gpu::cache_memory_budget(gpu.info.device_type),
    )
    .unwrap();
    let size = (256, 256);
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        view([0.0, 0.0], 0.25, size),
    );
    assert_eq!(canvas.level(), 2);
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    // Centers of the four quadrants keep their color within one level.
    for (sx, sy) in [(64, 64), (192, 64), (64, 192), (192, 192)] {
        let expected = export.pixel(sx * 4, sy * 4).map(|v| v as u8);
        let got = pixel(&shown, 256, sx, sy);
        assert!(
            close(got, expected),
            "({sx}, {sy}): {got:?} vs {expected:?}"
        );
    }
}

#[test]
fn a_24_megapixel_document_is_displayed() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 6000, "height": 4000, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "plein"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 6000, "height": 4000}, "color": [0.3, 0.5, 0.7, 1.0]}}
    ]"#,
    );
    let mut canvas = Canvas::new(
        &renderer,
        FORMAT,
        gpu::cache_memory_budget(gpu.info.device_type),
    )
    .unwrap();
    let size = (1200, 800);
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        view([0.0, 0.0], 0.2, size),
    );
    assert_eq!(canvas.level(), 2);
    let stats = canvas.stats();
    assert_eq!(stats.composited, 24 * 16, "{stats:?}");
    let expected = [77, 128, 179, 255];
    assert!(
        close(pixel(&shown, 1200, 600, 400), expected),
        "{:?}",
        pixel(&shown, 1200, 600, 400)
    );
}

#[test]
fn pixel_changes_reach_reduced_levels() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let mut document = quadrants(1024);
    let mut canvas = Canvas::new(
        &renderer,
        FORMAT,
        gpu::cache_memory_budget(gpu.info.device_type),
    )
    .unwrap();
    let size = (256, 256);
    let reduced = view([0.0, 0.0], 0.25, size);
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        reduced,
    );

    let mut bus = CommandBus::new();
    let cursor = bus.revision();
    bus.execute(
        &mut document,
        PhotoCommand::PixelsFill(PixelsFill {
            layer: NodeId(1),
            rect: Rect {
                x: 512,
                y: 512,
                width: 512,
                height: 512,
            },
            color: [0.0, 0.0, 0.0, 1.0],
        }),
        Source::Test,
    )
    .unwrap();
    let changes = bus.changes_since(cursor).changes;
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &changes,
        size,
        reduced,
    );
    assert!(close(pixel(&shown, 256, 192, 192), [0, 0, 0, 255]));
    // The untouched quadrant did not change.
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    let expected = export.pixel(256, 256).map(|v| v as u8);
    assert!(close(pixel(&shown, 256, 64, 64), expected));
}

#[test]
fn a_4k_window_keeps_the_whole_pyramid_of_a_24_megapixel_document() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    if gpu.device.limits().max_texture_array_layers < 513 {
        println!("skipped: the adapter offers fewer than 513 array layers");
        return;
    }
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 6000, "height": 4000, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "plein"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 6000, "height": 4000}, "color": [0.3, 0.5, 0.7, 1.0]}}
    ]"#,
    );
    let mut canvas = Canvas::new(
        &renderer,
        FORMAT,
        gpu::cache_memory_budget(gpu.info.device_type),
    )
    .unwrap();
    let size = (3840, 2160);
    // "Fit to screen" in a 4K window, then 100 % at the center.
    let fit = 0.92 * (3840.0f32 / 6000.0).min(2160.0 / 4000.0);
    let fit_origin = [
        (1920.0 - 3000.0 * fit).round(),
        (1080.0 - 2000.0 * fit).round(),
    ];
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        view(fit_origin, fit, size),
    );
    assert!(
        canvas.capacity() >= 513,
        "cache of {} tiles",
        canvas.capacity()
    );
    let none = PhotoChanges::default();
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &none,
        size,
        view([-1080.0, -920.0], 1.0, size),
    );
    assert_eq!(canvas.level(), 0, "100 % shows full resolution");
    let stats = canvas.stats();
    assert_eq!(stats.evicted, 0, "{stats:?}");
    assert_eq!(stats.composited, 384, "no tile composited twice: {stats:?}");
    assert!(close(pixel(&shown, 3840, 1920, 1080), [77, 128, 179, 255]));
}
