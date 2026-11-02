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

//! Source pool residency (docs/decisions/0021): layer and mask tiles stay
//! on the GPU from one render to the next, a property change uploads
//! nothing, a pixel change uploads only its tiles, and a pool far smaller
//! than the document still renders identically, with counted evictions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, draw_on, export16, setup};
use cygnus_commands::{ChangeSet, Command};
use cygnus_photo_core::{Document, PhotoChanges, PhotoCommand};
use cygnus_photo_render::{Canvas, CanvasView, ExportBits, Renderer};

/// 768 × 512 (3 × 2 tiles), four layers over every tile, one of them
/// masked by a gradient: 24 layer tiles and 6 mask tiles.
fn document() -> Document {
    build_json(
        r#"[
        {"command": "document.create", "params": {"width": 768, "height": 512, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "fond"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 768, "height": 512}, "color": [0.9, 0.85, 0.7, 1.0]}},
        {"command": "layer.create", "params": {"name": "a"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 768, "height": 512}, "color": [0.8, 0.1, 0.2, 0.5]}},
        {"command": "layer.create", "params": {"name": "b", "kind": {"type": "group", "isolation": "isolated"}}},
        {"command": "layer.create", "params": {"name": "c", "parent": 3}},
        {"command": "pixels.fill", "params": {"layer": 4, "rect": {"x": 0, "y": 0, "width": 768, "height": 512}, "color": [0.1, 0.3, 0.9, 0.6]}},
        {"command": "mask.create", "params": {"node": 4}},
        {"command": "mask.gradient", "params": {"mask": 5, "start": [0.0, 0.0], "end": [768.0, 0.0], "start_value": 0.0, "end_value": 1.0}},
        {"command": "layer.create", "params": {"name": "d"}},
        {"command": "pixels.fill", "params": {"layer": 6, "rect": {"x": 0, "y": 0, "width": 768, "height": 512}, "color": [0.2, 0.7, 0.3, 0.3]}},
        {"command": "layer.update", "params": {"id": 6, "clip": true}}
    ]"#,
    )
}

fn apply(document: &mut Document, command: &str) {
    let command: PhotoCommand = serde_json::from_str(command).unwrap();
    command.apply(document).map_err(|_| "refused").unwrap();
}

#[test]
fn a_reduced_source_pool_renders_identically_with_evictions() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = document();
    let reference = export16(&renderer, &document);
    // One output tile needs 4 layer tiles and 1 mask tile at once.
    let small = Renderer::with_source_slots(&gpu.device, &gpu.queue, 4, 1).unwrap();
    let image = export16(&small, &document);
    let stats = small.source_stats();
    println!("reduced pool: {stats:?}");
    assert!(image == reference, "reduced pool changed the export");
    assert!(stats.evictions > 0, "no eviction counted: {stats:?}");
    assert_eq!(stats.capacity, (4, 1));
}

#[test]
fn a_tile_needing_more_sources_than_the_pool_is_refused() {
    let Some((gpu, _)) = setup() else { return };
    let tiny = Renderer::with_source_slots(&gpu.device, &gpu.queue, 3, 1).unwrap();
    let error = tiny.export(&document(), ExportBits::Eight).unwrap_err();
    assert!(
        matches!(
            error,
            cygnus_photo_render::RenderError::TooManyLayers { count: 4 }
        ),
        "{error}"
    );
}

#[test]
fn resident_sources_are_not_uploaded_again() {
    let Some((_, renderer)) = setup() else { return };
    let mut document = document();
    let first = export16(&renderer, &document);
    let uploads = renderer.source_stats().uploads;
    assert_eq!(uploads, 24 + 6, "first render uploads every tile once");

    // Rendering again uploads nothing.
    let again = export16(&renderer, &document);
    assert!(again == first);
    assert_eq!(renderer.source_stats().uploads, uploads);

    // A property change only recomposes.
    apply(
        &mut document,
        r#"{"command": "layer.update", "params": {"id": 2, "opacity": 0.3}}"#,
    );
    let changed = export16(&renderer, &document);
    assert!(changed != first);
    assert_eq!(renderer.source_stats().uploads, uploads);

    // A pixel change in one tile uploads that tile only.
    apply(
        &mut document,
        r#"{"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 10, "y": 10, "width": 20, "height": 20}, "color": [0.0, 0.0, 0.0, 1.0]}}"#,
    );
    export16(&renderer, &document);
    assert_eq!(renderer.source_stats().uploads, uploads + 1);
}

#[test]
fn the_canvas_with_a_reduced_source_pool_equals_the_export() {
    let Some((gpu, _)) = setup() else { return };
    let document = document();
    let renderer = Renderer::with_source_slots(&gpu.device, &gpu.queue, 5, 2).unwrap();
    let export = renderer.export(&document, ExportBits::Eight).unwrap();
    let mut canvas = Canvas::with_capacity(&renderer, wgpu::TextureFormat::Rgba8Unorm, 64).unwrap();
    let view = CanvasView {
        origin: [0.0, 0.0],
        scale: 1.0,
        viewport: [0.0, 0.0, 768.0, 512.0],
        background: [0.0; 3],
        checker: [[0.8; 3], [0.6; 3]],
        checker_size: 8.0,
    };
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        (768, 512),
        view,
    );
    let exported: Vec<u8> = export.data.iter().map(|v| *v as u8).collect();
    assert!(shown == exported, "canvas differs from the 8-bit export");
    assert!(renderer.source_stats().evictions > 0);
}
