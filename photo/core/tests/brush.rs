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

//! `brush.stroke` in the core (docs/decisions/0035 and 0036, step P1 of
//! J5): positions of the dabs against values computed by hand, the checks
//! made before any render (target, empty stroke, sizes), and the history:
//! one entry per stroke, undone by tile copies of bounded memory. The
//! render itself is faked here; the GPU is tested in `photo/render`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::sync::Arc;

use cygnus_commands::{CommandBus, Source as Origin};
use cygnus_photo_core::brush::{self, Brush, BrushStroke, Dab, StrokePoint};
use cygnus_photo_core::commands::{DocumentCreate, LayerCreate, LayerUpdate};
use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileBackup, tiles_in};
use cygnus_photo_core::resolve::{Paint, Resolvers, resolve_and_execute};
use cygnus_photo_core::{CoreError, Depth, Document, NodeId, PhotoCommand, Rect};

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

fn brush(diameter: f32) -> Brush {
    Brush {
        diameter,
        spacing: 0.25,
        ..Brush::new([0.2, 0.4, 0.6, 1.0])
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_single_point_gives_one_dab_at_that_point() {
    let dabs = brush::dabs(&[point(12.5, 40.0)], &brush(10.0)).unwrap();
    assert_eq!(dabs.len(), 1);
    assert!(close(dabs[0].x, 12.5) && close(dabs[0].y, 40.0));
    assert!(close(dabs[0].radius, 5.0));
    assert!(close(dabs[0].flow, 1.0));
}

#[test]
fn dabs_follow_a_straight_line_at_the_spacing() {
    // Diameter 20, spacing 25 %: a dab every 5 pixels, from the start.
    let dabs = brush::dabs(&[point(0.0, 3.0), point(100.0, 3.0)], &brush(20.0)).unwrap();
    assert_eq!(dabs.len(), 21);
    for (i, dab) in dabs.iter().enumerate() {
        assert!(
            (dab.x - 5.0 * i as f64).abs() < 1e-6,
            "dab {i} at {}",
            dab.x
        );
        assert!(close(dab.y, 3.0));
    }
}

#[test]
fn the_spacing_carries_over_from_one_segment_to_the_next() {
    // Three aligned points 7 px apart: still one dab every 5 px.
    let dabs = brush::dabs(
        &[point(0.0, 0.0), point(7.0, 0.0), point(14.0, 0.0)],
        &brush(20.0),
    )
    .unwrap();
    let xs: Vec<f64> = dabs.iter().map(|d| d.x).collect();
    assert_eq!(xs.len(), 3);
    for (x, expected) in xs.iter().zip([0.0, 5.0, 10.0]) {
        assert!((x - expected).abs() < 1e-6, "{xs:?}");
    }
}

#[test]
fn the_curve_passes_through_every_point() {
    // Spacing small enough that a dab falls on each corner of the path.
    let points = [point(0.0, 0.0), point(40.0, 0.0), point(40.0, 40.0)];
    let dabs = brush::dabs(
        &points,
        &Brush {
            diameter: 10.0,
            spacing: 0.01,
            ..Brush::new([0.0, 0.0, 0.0, 1.0])
        },
    )
    .unwrap();
    for p in &points[1..] {
        let nearest = dabs
            .iter()
            .map(|d| ((d.x - p.x).powi(2) + (d.y - p.y).powi(2)).sqrt())
            .fold(f64::MAX, f64::min);
        assert!(nearest < 0.3, "no dab near ({}, {}): {nearest}", p.x, p.y);
    }
}

#[test]
fn pressure_drives_the_size_and_the_flow() {
    let mut p = point(10.0, 10.0);
    p.pressure = Some(0.5);
    let dabs = brush::dabs(&[p], &brush(20.0)).unwrap();
    // Diameter from 20 % to 100 %: 20 × (0.2 + 0.8 × 0.5) = 12.
    // f32 settings: 0.2 is 0.2000000029 in f64.
    assert!((dabs[0].radius - 6.0).abs() < 1e-6, "{}", dabs[0].radius);
    assert!(close(dabs[0].flow, 0.5));
}

#[test]
fn a_stroke_without_pressure_leaves_no_dab() {
    let mut p = point(10.0, 10.0);
    p.pressure = Some(0.0);
    let dabs = brush::dabs(&[p, p], &brush(20.0)).unwrap();
    assert!(dabs.is_empty(), "{dabs:?}");
}

#[test]
fn invalid_strokes_are_refused() {
    assert!(brush::dabs(&[], &brush(10.0)).is_err());
    assert!(brush::dabs(&[point(f64::NAN, 0.0)], &brush(10.0)).is_err());
    assert!(brush::dabs(&[point(0.0, 0.0)], &brush(0.5)).is_err());
    assert!(brush::dabs(&[point(0.0, 0.0)], &brush(6000.0)).is_err());
    let too_many = vec![point(0.0, 0.0); brush::MAX_POINTS + 1];
    assert!(brush::dabs(&too_many, &brush(10.0)).is_err());
    let mut bad = brush(10.0);
    bad.opacity = 1.5;
    assert!(brush::dabs(&[point(0.0, 0.0)], &bad).is_err());
}

/// Paints opaque white wherever a dab reaches, counting its calls.
struct FakePaint {
    calls: Cell<u32>,
}

impl Paint for FakePaint {
    fn paint(
        &self,
        document: &Document,
        target: NodeId,
        _brush: &Brush,
        dabs: &[Dab],
    ) -> Result<TileBackup, String> {
        self.calls.set(self.calls.get() + 1);
        let bounds = document.bounds();
        let mut coords = std::collections::BTreeSet::new();
        for dab in dabs {
            let r = dab.radius.ceil() as i64 + 1;
            let x0 = (dab.x.floor() as i64 - r).max(0) as u32;
            let y0 = (dab.y.floor() as i64 - r).max(0) as u32;
            let rect = Rect {
                x: x0,
                y: y0,
                width: (2 * r) as u32,
                height: (2 * r) as u32,
            }
            .intersect(&bounds);
            coords.extend(tiles_in(rect));
        }
        let _ = target;
        Ok(coords
            .into_iter()
            .map(|coord| {
                let tile = Tile::filled(
                    document.depth,
                    cygnus_photo_core::pixels::Channels::Rgba,
                    [65535; 4],
                );
                (coord, Some(Arc::new(tile)))
            })
            .collect())
    }
}

fn document() -> (CommandBus<PhotoCommand>, Document) {
    let mut bus = CommandBus::new();
    let mut document = Document::default();
    for command in [
        PhotoCommand::DocumentCreate(DocumentCreate {
            width: 1000,
            height: 600,
            resolution: 72.0,
            depth: Depth::U16,
            profile: Default::default(),
        }),
        PhotoCommand::LayerCreate(LayerCreate {
            name: "calque".to_owned(),
            parent: None,
            index: None,
            kind: Default::default(),
        }),
    ] {
        bus.execute(&mut document, command, Origin::Test).unwrap();
    }
    (bus, document)
}

fn stroke(points: Vec<StrokePoint>) -> PhotoCommand {
    PhotoCommand::BrushStroke(BrushStroke {
        target: NodeId(1),
        tool: Default::default(),
        points,
        brush: brush(20.0),
        resolved: None,
    })
}

fn run(
    bus: &mut CommandBus<PhotoCommand>,
    document: &mut Document,
    painter: &FakePaint,
    command: PhotoCommand,
) -> Result<(), CoreError> {
    let resolvers = Resolvers {
        paint: Some(painter),
        ..Resolvers::NONE
    };
    resolve_and_execute(bus, document, command, Origin::User, &resolvers).map(|_| ())
}

#[test]
fn an_unresolved_stroke_is_refused() {
    let (mut bus, mut document) = document();
    let error = bus
        .execute(&mut document, stroke(vec![point(10.0, 10.0)]), Origin::User)
        .unwrap_err();
    assert!(
        matches!(error, CoreError::Unresolved("brush.stroke")),
        "{error}"
    );
}

#[test]
fn a_locked_layer_is_refused_before_any_render() {
    let (mut bus, mut document) = document();
    bus.execute(
        &mut document,
        PhotoCommand::LayerUpdate(LayerUpdate {
            locked: Some(true),
            ..LayerUpdate::new(NodeId(1))
        }),
        Origin::Test,
    )
    .unwrap();
    let painter = FakePaint {
        calls: Cell::new(0),
    };
    let error = run(
        &mut bus,
        &mut document,
        &painter,
        stroke(vec![point(10.0, 10.0)]),
    )
    .unwrap_err();
    assert!(matches!(error, CoreError::Locked(NodeId(1))), "{error}");
    assert_eq!(painter.calls.get(), 0);
}

#[test]
fn a_stroke_outside_the_document_or_without_pressure_is_refused() {
    let (mut bus, mut document) = document();
    let painter = FakePaint {
        calls: Cell::new(0),
    };
    let entries = bus.entries().count();
    let outside = stroke(vec![point(-200.0, -200.0), point(-100.0, -150.0)]);
    let error = run(&mut bus, &mut document, &painter, outside).unwrap_err();
    assert!(matches!(error, CoreError::EmptyStroke), "{error}");
    let mut light = point(10.0, 10.0);
    light.pressure = Some(0.0);
    let error = run(&mut bus, &mut document, &painter, stroke(vec![light])).unwrap_err();
    assert!(matches!(error, CoreError::EmptyStroke), "{error}");
    assert_eq!(painter.calls.get(), 0);
    assert_eq!(
        bus.entries().count(),
        entries,
        "a refused stroke made an entry"
    );
}

#[test]
fn a_stroke_partly_outside_paints_the_document_only() {
    let (mut bus, mut document) = document();
    let painter = FakePaint {
        calls: Cell::new(0),
    };
    run(
        &mut bus,
        &mut document,
        &painter,
        stroke(vec![point(-50.0, 10.0), point(30.0, 10.0)]),
    )
    .unwrap();
    let tiles: Vec<_> = document
        .pixels
        .tiles(NodeId(1))
        .map(|(coord, _)| coord)
        .collect();
    assert_eq!(tiles.len(), 1, "{tiles:?}");
}

#[test]
fn a_long_stroke_is_one_history_entry_undone_by_tile_copies() {
    let (mut bus, mut document) = document();
    let painter = FakePaint {
        calls: Cell::new(0),
    };
    let before = document.clone();
    let entries = bus.entries().count();
    // 2000 points along a sine across the document.
    let points: Vec<StrokePoint> = (0..2000)
        .map(|i| {
            let t = f64::from(i) / 1999.0;
            point(20.0 + 960.0 * t, 300.0 + 200.0 * (t * 12.0).sin())
        })
        .collect();
    run(&mut bus, &mut document, &painter, stroke(points)).unwrap();
    assert_eq!(painter.calls.get(), 1);
    assert_eq!(bus.entries().count(), entries + 1, "one entry per stroke");
    // The undo data holds the previous content of the touched tiles only:
    // here none existed, so it costs no pixel memory.
    let touched = document.pixels.tiles(NodeId(1)).count();
    assert!(touched > 4 && touched <= 4 * 3, "{touched} tiles");
    let tile_bytes = (TILE_SIZE * TILE_SIZE * 8) as usize;
    assert!(
        bus.retained_bytes() <= touched * tile_bytes,
        "{}",
        bus.retained_bytes()
    );
    bus.undo(&mut document, Origin::User).unwrap();
    assert_eq!(document, before, "undo did not restore the document");
    bus.redo(&mut document, Origin::User).unwrap();
    assert_eq!(document.pixels.tiles(NodeId(1)).count(), touched);
}
