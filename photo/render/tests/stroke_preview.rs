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

//! Preview of a stroke while it is drawn (docs/decisions/0036, step P2 of
//! J5): fed point by point as the interface does, it shows and finally
//! writes exactly what the command computes in one go, repaints only what
//! new points change, and gives one command whose replay is identical.
//! Also the eyedropper, which reads the composite value of a pixel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, setup};
use cygnus_commands::{CommandBus, Source};
use cygnus_photo_core::brush::{Brush, BrushStroke, StrokePoint};
use cygnus_photo_core::resolve::{Resolvers, resolve_and_execute};
use cygnus_photo_core::{Document, NodeId, PhotoCommand};
use cygnus_photo_render::stroke::StrokePreview;

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

/// 1024 × 300, 16 bits: an opaque background and an empty layer above.
fn scene() -> Document {
    build_json(
        r#"[
        {"command": "document.create", "params": {"width": 1024, "height": 300, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "fond"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 1024, "height": 300}, "color": [0.2, 0.5, 0.7, 1.0]}},
        {"command": "layer.create", "params": {"name": "peinture"}}
    ]"#,
    )
}

fn brush() -> Brush {
    Brush {
        diameter: 40.0,
        hardness: 0.5,
        flow: 0.6,
        opacity: 0.9,
        ..Brush::new([0.9, 0.2, 0.1, 1.0])
    }
}

/// A wavy path from left to right, as input events would give it.
fn path() -> Vec<StrokePoint> {
    (0..60)
        .map(|i| {
            let t = f64::from(i) / 59.0;
            point(30.0 + 960.0 * t, 150.0 + 80.0 * (t * 9.0).sin())
        })
        .collect()
}

#[test]
fn a_preview_fed_point_by_point_equals_the_command_computed_at_once() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    for layer in [1, 2] {
        let mut preview = StrokePreview::begin(&scene(), NodeId(layer), brush()).unwrap();
        for chunk in path().chunks(3) {
            preview.extend(&renderer, chunk).unwrap();
        }
        let shown = preview.document().clone();
        let command = preview.finish().unwrap();

        // The command of the preview is applied without any renderer: its
        // tiles are already resolved.
        let mut sent = scene();
        let mut bus = CommandBus::new();
        resolve_and_execute(&mut bus, &mut sent, command, Source::User, &Resolvers::NONE).unwrap();
        assert_eq!(
            sent, shown,
            "layer {layer}: the shown stroke differs from the written one"
        );
        assert_eq!(bus.entries().count(), 1, "one entry per stroke");

        // The same stroke replayed by a script, resolved by the renderer.
        let mut replayed = scene();
        let replay = PhotoCommand::BrushStroke(BrushStroke {
            target: NodeId(layer),
            tool: Default::default(),
            points: path(),
            brush: brush(),
            resolved: None,
        });
        let resolvers = Resolvers {
            paint: Some(&renderer),
            ..Resolvers::NONE
        };
        resolve_and_execute(
            &mut CommandBus::new(),
            &mut replayed,
            replay,
            Source::Test,
            &resolvers,
        )
        .unwrap();
        assert_eq!(
            replayed, sent,
            "layer {layer}: the replay differs bit for bit"
        );
    }
}

#[test]
fn new_points_repaint_only_the_tiles_they_change() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let mut preview = StrokePreview::begin(&scene(), NodeId(2), brush()).unwrap();
    // Along the top row of tiles, from the first tile to the third.
    let line: Vec<StrokePoint> = (0..=20)
        .map(|i| point(30.0 + 30.0 * f64::from(i), 60.0))
        .collect();
    preview.extend(&renderer, &line[..18]).unwrap();
    let first = preview.repainted();
    assert!(first >= 3, "{first} tiles");
    // The next points stay in the third tile: the first one is left alone.
    let changes = preview.extend(&renderer, &line[18..]).unwrap();
    let tiles: Vec<_> = changes.pixels.values().flatten().collect();
    assert!(
        tiles.iter().all(|coord| coord.tx >= 1),
        "repainted {tiles:?}"
    );
    assert!(preview.repainted() <= 2, "{} tiles", preview.repainted());
}

#[test]
fn a_stroke_on_a_locked_layer_does_not_start_and_an_empty_one_sends_nothing() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let mut locked = scene();
    locked.nodes.get_mut(&NodeId(2)).unwrap().locked = true;
    assert!(StrokePreview::begin(&locked, NodeId(2), brush()).is_err());
    let mut preview = StrokePreview::begin(&scene(), NodeId(2), brush()).unwrap();
    preview
        .extend(&renderer, &[point(-500.0, -500.0), point(-400.0, -400.0)])
        .unwrap();
    assert!(preview.finish().is_none());
}

#[test]
fn the_eyedropper_reads_the_composite_value() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 600, "height": 300, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "fond"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 600, "height": 300}, "color": [0.2, 0.4, 0.6, 1.0]}},
        {"command": "layer.create", "params": {"name": "dessus"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 300, "y": 0, "width": 300, "height": 300}, "color": [1.0, 1.0, 1.0, 1.0]}},
        {"command": "layer.update", "params": {"id": 2, "opacity": 0.5}}
    ]"#,
    );
    let below = renderer.sample(&document, 10, 10).unwrap();
    assert_eq!(below, [13107, 26214, 39321, 65535]);
    // Half white over the background, composed in encoded values.
    let mixed = renderer.sample(&document, 400, 270).unwrap();
    for (got, want) in mixed.iter().zip([0.6, 0.7, 0.8, 1.0]) {
        assert!(
            got.abs_diff((want * 65535.0f64).round() as u16) <= 1,
            "{mixed:?}"
        );
    }
    assert_eq!(renderer.sample(&document, 700, 10).unwrap(), [0; 4]);
}
