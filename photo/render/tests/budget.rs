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

//! Frame budget of the canvas and coarse-first display (docs/decisions/0024
//! and 0034, T1 and T2): the work is spread over frames without changing
//! the final image, and meanwhile older or coarser tiles are shown instead
//! of holes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{build_json, draw_on, read_canvas, setup};
use cygnus_commands::{ChangeSet, Command};
use cygnus_photo_core::{Document, PhotoChanges, PhotoCommand};
use cygnus_photo_render::{Canvas, CanvasView, FrameBudget};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SIZE: (u32, u32) = (512, 512);

/// Checker colors no pixel of the document has: a hole shows them.
const CHECKER: [[f32; 3]; 2] = [[1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];

fn view(scale: f32) -> CanvasView {
    CanvasView {
        origin: [0.0, 0.0],
        scale,
        viewport: [0.0, 0.0, SIZE.0 as f32, SIZE.1 as f32],
        background: [0.0; 3],
        checker: CHECKER,
        checker_size: 8.0,
    }
}

/// An opaque 2048 × 2048 document (8 × 8 tiles) in four flat quadrants,
/// with a translucent layer above.
fn document() -> Document {
    build_json(
        r#"[
        {"command": "document.create", "params": {"width": 2048, "height": 2048, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "q"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 1024, "height": 1024}, "color": [0.9, 0.1, 0.1, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 1024, "y": 0, "width": 1024, "height": 1024}, "color": [0.1, 0.8, 0.2, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 1024, "width": 1024, "height": 1024}, "color": [0.2, 0.3, 0.9, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 1024, "y": 1024, "width": 1024, "height": 1024}, "color": [0.95, 0.9, 0.3, 1.0]}},
        {"command": "layer.create", "params": {"name": "voile"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 2048, "height": 2048}, "color": [0.5, 0.5, 0.5, 0.25]}}
    ]"#,
    )
}

fn budget(max_tiles: u32) -> Option<FrameBudget> {
    Some(FrameBudget {
        cpu: Duration::from_secs(60),
        gpu: Duration::from_secs(60),
        max_tiles,
    })
}

/// Whether `image` shows a checker color somewhere: a hole.
fn has_hole(image: &[u8]) -> bool {
    let checker: Vec<[u8; 4]> = CHECKER
        .iter()
        .map(|c| [c[0], c[1], c[2], 1.0].map(|v| (v * 255.0).round() as u8))
        .collect();
    image
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| checker.contains(pixel))
}

#[test]
fn a_frame_budget_spreads_the_work_without_changing_the_image() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = document();
    let mut reference = Canvas::with_capacity(&renderer, FORMAT, 256).unwrap();
    let expected = draw_on(
        &gpu,
        &renderer,
        &mut reference,
        &document,
        &PhotoChanges::everything(),
        SIZE,
        view(1.0),
    );

    let mut canvas = Canvas::with_capacity(&renderer, FORMAT, 256).unwrap();
    canvas.set_frame_budget(budget(2));
    canvas
        .prepare(
            &renderer,
            &document,
            &PhotoChanges::everything(),
            &view(1.0),
        )
        .unwrap();
    assert!(canvas.is_pending(), "the whole view fitted in one frame");
    assert_eq!(canvas.stats().composited, 2, "the budget was not kept");
    let mut frames = 1;
    while canvas.is_pending() {
        canvas
            .prepare(&renderer, &document, &PhotoChanges::default(), &view(1.0))
            .unwrap();
        frames += 1;
        assert!(frames < 100, "the work never ends");
    }
    // 4 × 4 visible tiles at 100 % (one of margin), two per frame.
    assert_eq!(frames, 8);
    assert_eq!(read_canvas(&gpu, &canvas, SIZE), expected);
}

#[test]
fn a_view_with_only_a_coarse_level_is_shown_enlarged_without_holes() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let document = document();
    // Room for the 16 tiles of 100 %, too little to keep the 64 tiles of
    // level 0 composed to build level 2: most are evicted.
    let mut canvas = Canvas::with_capacity(&renderer, FORMAT, 24).unwrap();
    // The whole document at a quarter: level 2.
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        SIZE,
        view(0.25),
    );
    // Zoom to 100 % with the work paused: no tile of level 0 exists.
    canvas.set_frame_budget(budget(0));
    canvas
        .prepare(&renderer, &document, &PhotoChanges::default(), &view(1.0))
        .unwrap();
    assert!(canvas.is_pending());
    let coarse = read_canvas(&gpu, &canvas, SIZE);
    assert!(!has_hole(&coarse), "a hole where a coarser level exists");

    // Flat areas: the enlarged level shows the exact colors.
    let mut exact_canvas = Canvas::with_capacity(&renderer, FORMAT, 256).unwrap();
    let exact = draw_on(
        &gpu,
        &renderer,
        &mut exact_canvas,
        &document,
        &PhotoChanges::everything(),
        SIZE,
        view(1.0),
    );
    for (a, b) in coarse.iter().zip(&exact) {
        assert!(a.abs_diff(*b) <= 1, "coarse {a} against exact {b}");
    }
}

#[test]
fn stale_tiles_stay_shown_until_they_are_rebuilt() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let mut document = document();
    let mut canvas = Canvas::with_capacity(&renderer, FORMAT, 256).unwrap();
    let before = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        SIZE,
        view(1.0),
    );
    let command: PhotoCommand =
        serde_json::from_str(r#"{"command": "layer.update", "params": {"id": 2, "opacity": 0.5}}"#)
            .unwrap();
    let changes = command
        .apply(&mut document)
        .map_err(|_| "refused")
        .unwrap()
        .changes;
    canvas.set_frame_budget(budget(0));
    canvas
        .prepare(&renderer, &document, &changes, &view(1.0))
        .unwrap();
    assert!(canvas.is_pending());
    assert_eq!(
        read_canvas(&gpu, &canvas, SIZE),
        before,
        "stale tiles were not shown"
    );
    // Resumed, the work brings the new image.
    canvas.set_frame_budget(None);
    let after = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::default(),
        SIZE,
        view(1.0),
    );
    assert_ne!(after, before);
    assert!(!has_hole(&after));
}
