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

//! `brush.stroke` on the GPU (docs/decisions/0035 and 0036, step P1 of J5):
//! against a reference computed here in f64 with the same rules (profile,
//! accumulation of the coverage, Normal blend, one rounding), a stroke at
//! 1 % opacity repeated 100 times against the analytic value, the limit
//! cases, and the replay of a stroke bit for bit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, setup};
use cygnus_commands::{CommandBus, Source};
use cygnus_photo_core::brush::{self, Brush, BrushStroke, Dab, StrokePoint};
use cygnus_photo_core::pixels::TileCoord;
use cygnus_photo_core::resolve::{Resolvers, resolve_and_execute};
use cygnus_photo_core::{CoreError, Document, NodeId, PhotoCommand};
use cygnus_photo_render::Renderer;

fn point(x: f64, y: f64) -> StrokePoint {
    StrokePoint {
        x,
        y,
        pressure: None,
        tilt_x: None,
        tilt_y: None,
        time_ms: None,
    }
}

fn paint(
    renderer: &Renderer,
    document: &mut Document,
    target: u64,
    points: Vec<StrokePoint>,
    brush: Brush,
) -> Result<(), CoreError> {
    let mut bus = CommandBus::new();
    let resolvers = Resolvers {
        paint: Some(renderer),
        ..Resolvers::NONE
    };
    let command = PhotoCommand::BrushStroke(BrushStroke {
        target: NodeId(target),
        tool: Default::default(),
        points,
        brush,
        resolved: None,
    });
    resolve_and_execute(&mut bus, document, command, Source::Test, &resolvers).map(|_| ())
}

/// Straight 16-bit values of pixel (x, y) of `layer`, zeros where it has no
/// tile.
fn pixel(document: &Document, layer: u64, x: u32, y: u32) -> [u16; 4] {
    document.pixels.pixel(NodeId(layer), x, y)
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn profile(dab: &Dab, hardness: f64, px: f64, py: f64) -> f64 {
    let d = ((px - dab.x).powi(2) + (py - dab.y).powi(2)).sqrt();
    if d >= dab.radius {
        return 0.0;
    }
    let inner = (hardness * dab.radius).min((dab.radius - 1.0).max(0.0));
    if d <= inner {
        return 1.0;
    }
    1.0 - smoothstep(inner, dab.radius, d)
}

fn coverage(dab: &Dab, hardness: f64, px: f64, py: f64) -> f64 {
    if dab.radius >= 2.0 {
        return profile(dab, hardness, px, py);
    }
    let mut sum = 0.0;
    for j in 0..4 {
        for i in 0..4 {
            let ox = (f64::from(i) + 0.5) / 4.0 - 0.5;
            let oy = (f64::from(j) + 0.5) / 4.0 - 0.5;
            sum += profile(dab, hardness, px + ox, py + oy);
        }
    }
    sum / 16.0
}

/// Coverage of the stroke on every pixel of a `width` × `height` document,
/// row by row: each dab, in order, updates the pixels of its box, so that
/// every pixel sees the dabs in the order the shader does.
fn stroke_coverage(dabs: &[Dab], hardness: f64, width: u32, height: u32) -> Vec<f64> {
    let mut t = vec![0.0; (width * height) as usize];
    for dab in dabs {
        let reach = dab.radius + 1.0;
        let x0 = (dab.x - reach).floor().max(0.0) as u32;
        let y0 = (dab.y - reach).floor().max(0.0) as u32;
        let x1 = ((dab.x + reach).ceil().max(0.0) as u32).min(width);
        let y1 = ((dab.y + reach).ceil().max(0.0) as u32).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let c = coverage(dab, hardness, f64::from(x) + 0.5, f64::from(y) + 0.5);
                let i = (y * width + x) as usize;
                t[i] += c * dab.flow * (1.0 - t[i]);
            }
        }
    }
    t
}

/// The reference value of a pixel of coverage `t` after the stroke, from
/// its value `before`, in a document of largest value `max`.
fn reference(t: f64, brush: &Brush, before: [u16; 4], max: f64) -> [u16; 4] {
    if t <= 0.0 {
        return before;
    }
    let scale = if max == 255.0 { 257.0 } else { 1.0 };
    let straight = before.map(|v| f64::from(v) * scale / 65535.0);
    let dst = [
        straight[0] * straight[3],
        straight[1] * straight[3],
        straight[2] * straight[3],
        straight[3],
    ];
    let alpha = t * f64::from(brush.opacity) * f64::from(brush.color[3]);
    let src = [
        f64::from(brush.color[0]) * alpha,
        f64::from(brush.color[1]) * alpha,
        f64::from(brush.color[2]) * alpha,
        alpha,
    ];
    let out: Vec<f64> = (0..4).map(|c| src[c] + dst[c] * (1.0 - alpha)).collect();
    let a = out[3].clamp(0.0, 1.0);
    let mut result = [0u16; 4];
    if a > 0.0 {
        for c in 0..3 {
            result[c] = ((out[c] / a).clamp(0.0, 1.0) * max).round() as u16;
        }
    }
    result[3] = (a * max).round() as u16;
    result
}

/// Largest gap between the painted layer and the reference over the whole
/// document.
fn largest_gap(
    before: &Document,
    after: &Document,
    layer: u64,
    dabs: &[Dab],
    brush: &Brush,
) -> (u16, (u32, u32)) {
    let max = f64::from(after.depth.max_value());
    let (width, height) = (after.width, after.height);
    let t = stroke_coverage(dabs, f64::from(brush.hardness), width, height);
    let mut worst = (0, (0, 0));
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let want = reference(t[i], brush, pixel(before, layer, x, y), max);
            let got = pixel(after, layer, x, y);
            for c in 0..4 {
                let gap = got[c].abs_diff(want[c]);
                if gap > worst.0 {
                    worst = (gap, (x, y));
                }
            }
        }
    }
    worst
}

/// 600 × 300, 16 bits: an opaque background (layer 1) and an empty layer
/// above (layer 2).
fn scene(depth: &str) -> Document {
    build_json(&format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 600, "height": 300, "depth": "{depth}"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 600, "height": 300}}, "color": [0.2, 0.5, 0.7, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 300, "y": 0, "width": 300, "height": 300}}, "color": [0.9, 0.8, 0.3, 1.0]}}}},
        {{"command": "layer.create", "params": {{"name": "peinture"}}}}
    ]"#
    ))
}

fn curve() -> Vec<StrokePoint> {
    vec![
        point(30.0, 40.0),
        point(150.0, 260.0),
        point(270.0, 60.0),
        point(330.0, 200.0),
        point(560.0, 250.0),
    ]
}

#[test]
fn a_stroke_matches_the_reference_on_opaque_and_empty_layers() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let brushes = [
        // Soft, translucent, across tiles.
        Brush {
            diameter: 60.0,
            hardness: 0.3,
            flow: 0.6,
            opacity: 0.8,
            ..Brush::new([0.9, 0.3, 0.1, 0.9])
        },
        // Thin: dabs under 4 pixels are supersampled.
        Brush {
            diameter: 2.5,
            hardness: 1.0,
            spacing: 0.25,
            ..Brush::new([0.05, 0.1, 0.95, 1.0])
        },
    ];
    for depth in ["u16", "u8"] {
        for brush in brushes {
            for layer in [1, 2] {
                let before = scene(depth);
                let mut after = before.clone();
                paint(&renderer, &mut after, layer, curve(), brush).unwrap();
                let dabs = brush::dabs(&curve(), &brush).unwrap();
                let (gap, at) = largest_gap(&before, &after, layer, &dabs, &brush);
                println!(
                    "{depth}, diameter {}, layer {layer}: {gap} units at {at:?}",
                    brush.diameter
                );
                assert!(gap <= 1, "{depth}, layer {layer}: {gap} units at {at:?}");
            }
        }
    }
}

#[test]
fn opacity_caps_a_stroke_and_repeated_strokes_follow_the_analytic_value() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let mut document = scene("u16");
    // Hard dabs overlapping along a line: the coverage reaches 1 inside the
    // line, the opacity caps the stroke at 1 %.
    let brush = Brush {
        diameter: 40.0,
        hardness: 1.0,
        flow: 1.0,
        opacity: 0.01,
        ..Brush::new([0.4, 0.4, 0.4, 1.0])
    };
    let line = vec![point(20.0, 50.0), point(200.0, 50.0)];
    // Expected alpha after n strokes: each blends 1 % over the previous
    // value, rounded to 16 bits (one rounding per stroke, none inside it).
    let mut rounded = 0.0f64;
    for _ in 0..100 {
        paint(&renderer, &mut document, 2, line.clone(), brush).unwrap();
        let a = rounded / 65535.0;
        rounded = ((0.01 + a * 0.99) * 65535.0).round();
    }
    let got = pixel(&document, 2, 100, 50);
    let analytic = (1.0 - 0.99f64.powi(100)) * 65535.0;
    println!(
        "alpha after 100 strokes: {} (rounded recursion {rounded}, analytic {analytic:.2}), color {}",
        got[3], got[0]
    );
    assert!(
        (f64::from(got[3]) - rounded).abs() <= 1.0,
        "alpha {} against {rounded}",
        got[3]
    );
    // One rounding per stroke drifts at most 0.5 × Σ 0.99^k from the
    // analytic value.
    let bound = 0.5 * (1.0 - 0.99f64.powi(100)) / 0.01 + 1.0;
    assert!((f64::from(got[3]) - analytic).abs() <= bound);
    // The straight color stays the brush color.
    assert!(
        got[0].abs_diff((0.4f64 * 65535.0).round() as u16) <= 1,
        "{got:?}"
    );
}

#[test]
fn a_single_point_lays_one_dab() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let mut document = scene("u16");
    let brush = Brush {
        diameter: 21.0,
        hardness: 1.0,
        ..Brush::new([1.0, 0.0, 0.0, 1.0])
    };
    paint(
        &renderer,
        &mut document,
        2,
        vec![point(100.0, 100.0)],
        brush,
    )
    .unwrap();
    assert_eq!(pixel(&document, 2, 100, 100), [65535, 0, 0, 65535]);
    assert_eq!(
        pixel(&document, 2, 100, 115)[3],
        0,
        "painted beyond the radius"
    );
    let tiles: Vec<TileCoord> = document.pixels.tiles(NodeId(2)).map(|(c, _)| c).collect();
    assert_eq!(tiles, vec![TileCoord { tx: 0, ty: 0 }]);
}

#[test]
fn a_stroke_partly_outside_stays_within_the_document() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    // 300 × 300: the right tiles stick out of the document.
    let mut document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 300, "height": 300, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "peinture"}}
    ]"#,
    );
    let brush = Brush {
        diameter: 40.0,
        hardness: 1.0,
        ..Brush::new([0.0, 1.0, 0.0, 1.0])
    };
    paint(
        &renderer,
        &mut document,
        1,
        vec![point(280.0, 100.0), point(400.0, 100.0)],
        brush,
    )
    .unwrap();
    assert_eq!(pixel(&document, 1, 299, 100), [0, 65535, 0, 65535]);
    // Beyond the document, inside the stored tile: untouched.
    let tile = document
        .pixels
        .tile(NodeId(1), TileCoord { tx: 1, ty: 0 })
        .unwrap();
    assert_eq!(tile.get(300 - 256, 100), [0; 4]);
}

#[test]
fn locked_layers_and_strokes_without_coverage_are_refused() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let mut document = scene("u16");
    let before = document.clone();
    let mut light = point(100.0, 100.0);
    light.pressure = Some(0.0);
    let error = paint(
        &renderer,
        &mut document,
        2,
        vec![light, light],
        Brush::new([1.0; 4]),
    )
    .unwrap_err();
    assert!(matches!(error, CoreError::EmptyStroke), "{error}");
    let mut locked = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 300, "height": 300, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "verrou"}},
        {"command": "layer.update", "params": {"id": 1, "locked": true}}
    ]"#,
    );
    let error = paint(
        &renderer,
        &mut locked,
        1,
        vec![point(10.0, 10.0)],
        Brush::new([1.0; 4]),
    )
    .unwrap_err();
    assert!(matches!(error, CoreError::Locked(NodeId(1))), "{error}");
    assert_eq!(document, before);
}

#[test]
fn a_stroke_replayed_by_a_script_gives_the_same_pixels() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let brush = Brush {
        diameter: 35.0,
        hardness: 0.5,
        flow: 0.7,
        mode: cygnus_photo_core::BlendMode::Multiply,
        ..Brush::new([0.8, 0.2, 0.6, 1.0])
    };
    // As the interface sends it.
    let mut sent = scene("u16");
    paint(&renderer, &mut sent, 1, curve(), brush).unwrap();
    // As a script replays it: the command serialized, read back, resolved.
    let command = PhotoCommand::BrushStroke(BrushStroke {
        target: NodeId(1),
        tool: Default::default(),
        points: curve(),
        brush,
        resolved: None,
    });
    let json = serde_json::to_string(&command).unwrap();
    let replayed_command: PhotoCommand = serde_json::from_str(&json).unwrap();
    let mut replayed = scene("u16");
    let mut bus = CommandBus::new();
    let resolvers = Resolvers {
        paint: Some(&renderer),
        ..Resolvers::NONE
    };
    resolve_and_execute(
        &mut bus,
        &mut replayed,
        replayed_command,
        Source::Test,
        &resolvers,
    )
    .unwrap();
    assert_eq!(replayed, sent, "the replay differs bit for bit");
}

#[test]
fn a_stroke_of_2000_points_is_one_entry_undone_exactly() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let mut document = scene("u16");
    let before = document.clone();
    let mut bus = CommandBus::new();
    let resolvers = Resolvers {
        paint: Some(&renderer),
        ..Resolvers::NONE
    };
    let points: Vec<StrokePoint> = (0..2000)
        .map(|i| {
            let t = f64::from(i) / 1999.0;
            let mut p = point(20.0 + 560.0 * t, 150.0 + 110.0 * (t * 20.0).sin());
            p.pressure = Some((0.3 + 0.7 * (t * 7.0).sin().abs()) as f32);
            p
        })
        .collect();
    let command = PhotoCommand::BrushStroke(BrushStroke {
        target: NodeId(1),
        tool: Default::default(),
        points,
        brush: Brush {
            diameter: 24.0,
            ..Brush::new([0.1, 0.1, 0.1, 1.0])
        },
        resolved: None,
    });
    let entries = bus.entries().count();
    let retained = bus.retained_bytes();
    resolve_and_execute(&mut bus, &mut document, command, Source::User, &resolvers).unwrap();
    assert_eq!(bus.entries().count(), entries + 1, "one entry per stroke");
    // Undo data: copies of the touched tiles of the layer (6 tiles of
    // 512 KiB at most here), plus the points of the command.
    let tile = 256 * 256 * 8;
    let used = bus.retained_bytes() - retained;
    println!("undo data: {used} bytes");
    assert!(used <= 6 * tile + 2000 * 64, "{used} bytes");
    let after = document.clone();
    bus.undo(&mut document, Source::User).unwrap();
    assert_eq!(document, before, "undo did not restore the layer");
    bus.redo(&mut document, Source::User).unwrap();
    assert_eq!(document, after, "redo did not repaint the layer");
}
