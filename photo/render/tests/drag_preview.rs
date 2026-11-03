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

//! Preview during a parameter drag (docs/decisions/0027, B7-lite): the
//! displayed level is composed from reduced sources, built once; once the
//! preview stops, the view is recomputed exactly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, draw_on, setup};
use cygnus_commands::{ChangeSet, Command};
use cygnus_photo_core::{Document, PhotoChanges, PhotoCommand};
use cygnus_photo_render::{Canvas, CanvasView};

/// 2048 × 2048 (8 × 8 tiles): two layers, the upper one masked by a
/// gradient, and a curves adjustment on top.
fn document() -> Document {
    build_json(
        r#"[
        {"command": "document.create", "params": {"width": 2048, "height": 2048, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "fond"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 2048, "height": 2048}, "color": [0.31, 0.52, 0.74, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 300, "y": 300, "width": 900, "height": 700}, "color": [0.83, 0.42, 0.21, 1.0]}},
        {"command": "layer.create", "params": {"name": "dessus"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 900, "y": 100, "width": 1000, "height": 1700}, "color": [0.12, 0.81, 0.33, 0.7]}},
        {"command": "mask.create", "params": {"node": 2}},
        {"command": "mask.gradient", "params": {"mask": 3, "start": [0, 0], "end": [2048, 0], "start_value": 0.1, "end_value": 1.0}},
        {"command": "layer.create", "params": {"name": "courbe", "kind": {"type": "adjustment", "adjustment": {"kind": "curves", "master": {"points": [[0, 0], [0.5, 0.5], [1, 1]]}, "red": {"points": [[0, 0], [1, 1]]}, "green": {"points": [[0, 0], [1, 1]]}, "blue": {"points": [[0, 0], [1, 1]]}}}}}
    ]"#,
    )
}

fn curve(document: &mut Document, middle: f32) -> PhotoChanges {
    let command: PhotoCommand = serde_json::from_str(&format!(
        r#"{{"command": "adjustment.update", "params": {{"id": 4, "adjustment": {{"kind": "curves", "master": {{"points": [[0, 0], [0.5, {middle}], [1, 1]]}}, "red": {{"points": [[0, 0], [1, 1]]}}, "green": {{"points": [[0, 0], [1, 1]]}}, "blue": {{"points": [[0, 0], [1, 1]]}}}}}}}}"#
    ))
    .unwrap();
    command
        .apply(document)
        .map_err(|_| "refused")
        .unwrap()
        .changes
}

/// The whole document at a quarter: displayed level 2.
fn view() -> CanvasView {
    CanvasView {
        origin: [0.0, 0.0],
        scale: 0.25,
        viewport: [0.0, 0.0, 512.0, 512.0],
        background: [0.0; 3],
        checker: [[0.5; 3], [0.4; 3]],
        checker_size: 8.0,
    }
}

fn largest_gap(a: &[u8], b: &[u8]) -> (u8, f64) {
    let max = a
        .iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    let mean = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(x.abs_diff(*y)))
        .sum::<f64>()
        / a.len() as f64;
    (max, mean)
}

#[test]
fn a_drag_previews_from_reduced_sources_then_refines_exactly() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let mut document = document();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut canvas = Canvas::with_capacity(&renderer, format, 256).unwrap();
    let size = (512, 512);
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        size,
        view(),
    );

    // First step of the drag: reduced sources are built, no full-size
    // tile is composed.
    canvas.set_preview(true);
    let changes = curve(&mut document, 0.62);
    let composited = canvas.stats().composited;
    let reduced = renderer.source_stats().reduced;
    let preview = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &changes,
        size,
        view(),
    );
    // 2 × 2 tiles of level 2, each a single composition.
    assert_eq!(canvas.stats().composited - composited, 4);
    assert!(
        renderer.source_stats().reduced > reduced,
        "no reduced tile built"
    );

    // Next steps: nothing is built or uploaded again.
    let uploads = renderer.source_stats().uploads;
    let reduced = renderer.source_stats().reduced;
    let changes = curve(&mut document, 0.7);
    draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &changes,
        size,
        view(),
    );
    assert_eq!(
        renderer.source_stats().reduced,
        reduced,
        "reduced tiles built again"
    );
    assert_eq!(
        renderer.source_stats().uploads,
        uploads,
        "sources uploaded again"
    );

    // Release: the view is recomputed exactly, as a fresh canvas shows it.
    canvas.set_preview(false);
    let exact = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::default(),
        size,
        view(),
    );
    let mut fresh = Canvas::with_capacity(&renderer, format, 256).unwrap();
    let reference = draw_on(
        &gpu,
        &renderer,
        &mut fresh,
        &document,
        &PhotoChanges::everything(),
        size,
        view(),
    );
    assert_eq!(
        exact, reference,
        "the refined view differs from the exact one"
    );

    // The preview was close to the exact image of its own parameters.
    let mut at_62 = document.clone();
    curve(&mut at_62, 0.62);
    let mut other = Canvas::with_capacity(&renderer, format, 256).unwrap();
    let exact_62 = draw_on(
        &gpu,
        &renderer,
        &mut other,
        &at_62,
        &PhotoChanges::everything(),
        size,
        view(),
    );
    let (max, mean) = largest_gap(&preview, &exact_62);
    println!("preview against exact: max {max} levels, mean {mean:.3}");
    assert!(
        mean < 1.0,
        "preview too far from the exact image: mean {mean}"
    );
}
