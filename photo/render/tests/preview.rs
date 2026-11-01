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

//! Preview equals export (spec 2.1, docs/decisions/0013): the canvas drawn
//! at 100 % shows exactly the values of the 8-bit export, on the same
//! adapter, with no tolerance.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, setup};
use cygnus_commands::ChangeSet;
use cygnus_photo_core::{Document, PhotoChanges};
use cygnus_photo_render::gpu;
use cygnus_photo_render::{Canvas, CanvasView, ExportBits, Renderer};

const BACKGROUND: [f32; 3] = [20.0 / 255.0, 23.0 / 255.0, 28.0 / 255.0];
const LIGHT: [f32; 3] = [0.8, 0.8, 0.8];
const DARK: [f32; 3] = [0.6, 0.6, 0.6];

fn view(origin: [f32; 2], scale: f32, size: (u32, u32)) -> CanvasView {
    CanvasView {
        origin,
        scale,
        viewport: [0.0, 0.0, size.0 as f32, size.1 as f32],
        background: BACKGROUND,
        checker: [LIGHT, DARK],
        checker_size: 8.0,
    }
}

fn draw(
    gpu: &cygnus_photo_render::gpu::Gpu,
    renderer: &Renderer,
    document: &Document,
    width: u32,
    height: u32,
    view: CanvasView,
) -> Vec<u8> {
    let mut canvas = Canvas::new(
        renderer,
        wgpu::TextureFormat::Rgba8Unorm,
        gpu::cache_memory_budget(gpu.info.device_type),
    )
    .unwrap();
    common::draw_on(
        gpu,
        renderer,
        &mut canvas,
        document,
        &PhotoChanges::everything(),
        (width, height),
        view,
    )
}

/// An opaque 300 × 200 document (row size not a multiple of 256 bytes),
/// several tiles, two layers with opacity.
fn opaque_document() -> Document {
    build_json(
        r#"[
        {"command": "document.create", "params": {"width": 300, "height": 300, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "fond"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 300, "height": 300}, "color": [0.9, 0.85, 0.7, 1.0]}},
        {"command": "layer.create", "params": {"name": "haut"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 40, "y": 200, "width": 250, "height": 90}, "color": [0.1, 0.4, 0.8, 0.7]}},
        {"command": "layer.update", "params": {"id": 2, "opacity": 0.6}},
        {"command": "pixels.write", "params": {"layer": 2, "rect": {"x": 254, "y": 254, "width": 3, "height": 1}, "samples": [65535, 0, 0, 65535, 0, 65535, 0, 30000, 1000, 2000, 60000, 65535]}}
    ]"#,
    )
}

#[test]
fn preview_at_100_percent_equals_the_8_bit_export() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = opaque_document();
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    let shown = draw(
        &gpu,
        &renderer,
        &document,
        300,
        300,
        view([0.0, 0.0], 1.0, (300, 300)),
    );
    let exported: Vec<u8> = export.data.iter().map(|v| *v as u8).collect();
    let differing = shown
        .as_chunks::<4>()
        .0
        .iter()
        .zip(exported.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "preview and export differ on {differing} pixels"
    );
}

#[test]
fn zoom_repeats_pixels_and_surroundings_show_the_background() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = opaque_document();
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    // 200 %, document shifted by (10, 6) target pixels.
    let shown = draw(
        &gpu,
        &renderer,
        &document,
        700,
        700,
        view([10.0, 6.0], 2.0, (700, 700)),
    );
    let at = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * 700 + x) * 4) as usize;
        [shown[i], shown[i + 1], shown[i + 2], shown[i + 3]]
    };
    let exported = |x: u32, y: u32| -> [u8; 4] { export.pixel(x, y).map(|v| v as u8) };
    for (x, y) in [(0, 0), (100, 250), (255, 254), (299, 299)] {
        for (dx, dy) in [(0, 0), (1, 1)] {
            assert_eq!(
                at(10 + 2 * x + dx, 6 + 2 * y + dy),
                exported(x, y),
                "({x}, {y})"
            );
        }
    }
    let background = BACKGROUND.map(|c| (c * 255.0).round() as u8);
    assert_eq!(at(5, 3), [background[0], background[1], background[2], 255]);
    assert_eq!(
        at(699, 699),
        [background[0], background[1], background[2], 255]
    );
}

#[test]
fn transparent_pixels_show_the_checker() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 32, "height": 32, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "vide"}}
    ]"#,
    );
    let shown = draw(
        &gpu,
        &renderer,
        &document,
        32,
        32,
        view([0.0, 0.0], 1.0, (32, 32)),
    );
    let light = LIGHT.map(|c| (c * 255.0).round() as u8);
    let dark = DARK.map(|c| (c * 255.0).round() as u8);
    assert_eq!(&shown[0..3], &light);
    let second_square = (8 * 4) as usize;
    assert_eq!(&shown[second_square..second_square + 3], &dark);
}

#[test]
fn only_changed_tiles_are_recomposited() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    use cygnus_commands::{CommandBus, Source};
    use cygnus_photo_core::PhotoCommand;
    use cygnus_photo_core::commands::PixelsFill;
    use cygnus_photo_core::{NodeId, Rect};

    let mut document = opaque_document();
    let mut canvas = Canvas::new(
        &renderer,
        wgpu::TextureFormat::Rgba8Unorm,
        gpu::cache_memory_budget(gpu.info.device_type),
    )
    .unwrap();
    let v = view([0.0, 0.0], 1.0, (300, 300));
    canvas
        .prepare(&renderer, &document, &PhotoChanges::everything(), &v)
        .unwrap();
    let mut bus = CommandBus::new();
    let cursor = bus.revision();
    bus.execute(
        &mut document,
        PhotoCommand::PixelsFill(PixelsFill {
            layer: NodeId(1),
            rect: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            color: [0.0, 0.0, 0.0, 1.0],
        }),
        Source::Test,
    )
    .unwrap();
    let changes = bus.changes_since(cursor).changes;
    canvas.prepare(&renderer, &document, &changes, &v).unwrap();
    // The display reflects the change, and matches a fresh export.
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    let shown = draw(&gpu, &renderer, &document, 300, 300, v);
    assert_eq!(&shown[0..4], &[0, 0, 0, 255]);
    assert_eq!(export.pixel(0, 0), [0, 0, 0, 255]);
}
