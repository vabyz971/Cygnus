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

//! Resolved commands (docs/decisions/0020): `source.import` reads a file and
//! `layer.merge_down` renders two layers, which `photo/core` cannot do. They
//! go through one path, `resolve_and_execute`, with the reader and the
//! renderer given by the caller; applied without it, they are refused.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::sync::Arc;

use cygnus_commands::{CommandBus, Source as Origin};
use cygnus_photo_core::pixels::{LayerTiles, Tile, TileCoord};
use cygnus_photo_core::resolve::{Import, Merge, Resolvers, resolve_and_execute};
use cygnus_photo_core::script::Script;
use cygnus_photo_core::{
    BlendMode, CoreError, Depth, Document, NodeId, NodeKind, PhotoCommand, Source, SourceId,
};

/// Imports a 2 × 2 source whatever the path, and counts its calls.
struct FakeImport {
    calls: Cell<u32>,
}

impl Import for FakeImport {
    fn import(&self, path: &str) -> Result<(SourceId, Source), String> {
        self.calls.set(self.calls.get() + 1);
        if path.ends_with("missing.png") {
            return Err(format!("cannot read {path}"));
        }
        let mut tiles = LayerTiles::new();
        tiles.insert(
            TileCoord { tx: 0, ty: 0 },
            Arc::new(Tile::filled(
                Depth::U8,
                cygnus_photo_core::pixels::Channels::Rgba,
                [10, 20, 30, 255],
            )),
        );
        Ok((
            SourceId(format!("hash-of-{path}")),
            Source {
                name: path.to_owned(),
                width: 2,
                height: 2,
                depth: Depth::U8,
                tiles,
            },
        ))
    }
}

/// Merges by filling the result with one gray, and counts its calls.
struct FakeMerge {
    calls: Cell<u32>,
}

impl Merge for FakeMerge {
    fn merge(&self, document: &Document, upper: NodeId) -> Result<LayerTiles, String> {
        self.calls.set(self.calls.get() + 1);
        assert!(document.node(upper).is_some());
        let mut tiles = LayerTiles::new();
        tiles.insert(
            TileCoord { tx: 0, ty: 0 },
            Arc::new(Tile::filled(
                document.depth,
                cygnus_photo_core::pixels::Channels::Rgba,
                [100, 100, 100, 255],
            )),
        );
        Ok(tiles)
    }
}

fn command(json: &str) -> PhotoCommand {
    serde_json::from_str(json).unwrap_or_else(|error| panic!("{json}: {error}"))
}

fn setup(commands: &[&str]) -> (CommandBus<PhotoCommand>, Document) {
    let mut bus = CommandBus::new();
    let mut document = Document::default();
    let resolvers = Resolvers::NONE;
    for json in
        [r#"{"command": "document.create", "params": {"width": 8, "height": 8, "depth": "u8"}}"#]
            .iter()
            .chain(commands)
    {
        resolve_and_execute(
            &mut bus,
            &mut document,
            command(json),
            Origin::Test,
            &resolvers,
        )
        .unwrap();
    }
    (bus, document)
}

const IMPORT: &str = r#"{"command": "source.import", "params": {"path": "photo.png"}}"#;

#[test]
fn an_unresolved_import_or_merge_is_refused_by_the_bus() {
    let (mut bus, mut document) = setup(&[&pixel("a"), &pixel("b")]);
    let before = document.clone();
    for json in [
        IMPORT,
        r#"{"command": "layer.merge_down", "params": {"id": 2}}"#,
    ] {
        let error = bus
            .execute(&mut document, command(json), Origin::Test)
            .unwrap_err();
        assert!(matches!(error, CoreError::Unresolved(_)), "{json}: {error}");
        assert!(error.to_string().contains("resolve_and_execute"), "{error}");
    }
    assert_eq!(document, before);
}

#[test]
fn import_embeds_the_source_once_and_redo_does_not_read_the_file_again() {
    let (mut bus, mut document) = setup(&[]);
    let importer = FakeImport {
        calls: Cell::new(0),
    };
    let resolvers = Resolvers {
        import: Some(&importer),
        merge: None,
    };
    for _ in 0..2 {
        resolve_and_execute(
            &mut bus,
            &mut document,
            command(IMPORT),
            Origin::Agent,
            &resolvers,
        )
        .unwrap();
    }
    assert_eq!(importer.calls.get(), 2);
    // One source, two embedded layers designating it.
    assert_eq!(document.sources.len(), 1);
    let id = SourceId("hash-of-photo.png".to_owned());
    for node in [NodeId(1), NodeId(2)] {
        match &document.node(node).unwrap().kind {
            NodeKind::Embedded { source, placement } => {
                assert_eq!(*source, id);
                assert_eq!((placement.x, placement.y), (0, 0));
            }
            other => panic!("node {node:?} is {other:?}"),
        }
        assert_eq!(document.node(node).unwrap().name, "photo.png");
    }
    // The history keeps the public command, with its path and its origin.
    let last = bus.entries().last().unwrap();
    assert_eq!(last.name, "source.import");
    assert_eq!(last.source, Origin::Agent);
    match &last.command {
        PhotoCommand::SourceImport(params) => assert_eq!(params.path, "photo.png"),
        other => panic!("recorded {other:?}"),
    }
    // Undo removes the layer and keeps the source; redo brings the same
    // layer back without reading the file.
    let after = document.clone();
    assert!(bus.undo(&mut document, Origin::User).unwrap());
    assert!(document.node(NodeId(2)).is_none());
    assert_eq!(document.sources.len(), 1);
    assert!(bus.redo(&mut document, Origin::User).unwrap());
    assert_eq!(document, after);
    assert_eq!(importer.calls.get(), 2);
}

#[test]
fn a_failed_import_changes_nothing_and_says_why() {
    let (mut bus, mut document) = setup(&[]);
    let before = document.clone();
    let importer = FakeImport {
        calls: Cell::new(0),
    };
    let resolvers = Resolvers {
        import: Some(&importer),
        merge: None,
    };
    let error = resolve_and_execute(
        &mut bus,
        &mut document,
        command(r#"{"command": "source.import", "params": {"path": "missing.png"}}"#),
        Origin::Test,
        &resolvers,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("cannot read missing.png"),
        "{error}"
    );
    assert_eq!(document, before);

    // Without a file reader at all.
    let error = resolve_and_execute(
        &mut bus,
        &mut document,
        command(IMPORT),
        Origin::Test,
        &Resolvers::NONE,
    )
    .unwrap_err();
    assert!(error.to_string().contains("source.import"), "{error}");
    assert_eq!(document, before);
}

#[test]
fn a_script_cannot_import_without_a_file_reader() {
    let script = Script::parse(&format!(
        r#"{{"version": 1, "commands": [
            {{"command": "document.create", "params": {{"width": 8, "height": 8, "depth": "u8"}}}},
            {IMPORT}
        ]}}"#
    ))
    .unwrap();
    let error = script.build(Origin::Test).unwrap_err().to_string();
    assert!(error.contains("source.import"), "{error}");
}

fn pixel(name: &str) -> String {
    format!(r#"{{"command": "layer.create", "params": {{"name": "{name}"}}}}"#)
}

fn fill(layer: u64) -> String {
    format!(
        r#"{{"command": "pixels.fill", "params": {{"layer": {layer}, "rect": {{"x": 0, "y": 0, "width": 4, "height": 4}}, "color": [0.5, 0.2, 0.1, 0.8]}}}}"#
    )
}

fn merge(id: u64) -> String {
    format!(r#"{{"command": "layer.merge_down", "params": {{"id": {id}}}}}"#)
}

#[test]
fn merge_down_replaces_both_layers_and_undoes_exactly() {
    // 1 bottom, 2 below (masked, half opacity), 3 upper (masked), 6 top.
    let (mut bus, mut document) = setup(&[
        &pixel("fond"),
        &pixel("dessous"),
        &fill(2),
        r#"{"command": "layer.update", "params": {"id": 2, "opacity": 0.5}}"#,
        &pixel("dessus"),
        &fill(3),
        r#"{"command": "mask.create", "params": {"node": 2}}"#,
        r#"{"command": "mask.create", "params": {"node": 3}}"#,
        &pixel("sommet"),
    ]);
    let before = document.clone();
    let merger = FakeMerge {
        calls: Cell::new(0),
    };
    let resolvers = Resolvers {
        import: None,
        merge: Some(&merger),
    };
    resolve_and_execute(
        &mut bus,
        &mut document,
        command(&merge(3)),
        Origin::Test,
        &resolvers,
    )
    .unwrap();
    assert_eq!(merger.calls.get(), 1);
    // Layer 2 holds the merge: a pixel layer at full opacity, in Normal,
    // without mask; layer 3 and both masks are gone.
    let merged = document.node(NodeId(2)).unwrap();
    assert_eq!(merged.name, "dessous");
    assert!(matches!(merged.kind, NodeKind::Pixel { .. }));
    assert_eq!(merged.opacity, 1.0);
    assert_eq!(merged.blend, BlendMode::Normal);
    assert_eq!(merged.mask, None);
    for gone in [3, 4, 5] {
        assert!(document.node(NodeId(gone)).is_none(), "node {gone}");
    }
    assert_eq!(
        document.children(document.root),
        [NodeId(1), NodeId(2), NodeId(6)]
    );
    assert_eq!(document.pixels.pixel(NodeId(2), 0, 0), [100, 100, 100, 255]);
    assert_eq!(bus.entries().last().unwrap().name, "layer.merge_down");

    let after = document.clone();
    assert!(bus.undo(&mut document, Origin::User).unwrap());
    assert_eq!(document, before);
    assert!(bus.redo(&mut document, Origin::User).unwrap());
    assert_eq!(document, after);
    assert_eq!(merger.calls.get(), 1, "redo renders nothing");
}

#[test]
fn merge_down_refuses_what_it_could_not_reproduce_exactly() {
    let refused = |commands: &[&str], id: u64, reason: &str| {
        let (mut bus, mut document) = setup(commands);
        let before = document.clone();
        let merger = FakeMerge {
            calls: Cell::new(0),
        };
        let resolvers = Resolvers {
            import: None,
            merge: Some(&merger),
        };
        let error = resolve_and_execute(
            &mut bus,
            &mut document,
            command(&merge(id)),
            Origin::Test,
            &resolvers,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(reason), "expected «{reason}», got «{error}»");
        assert_eq!(document, before, "{reason}");
        assert_eq!(merger.calls.get(), 0, "{reason}: rendered anyway");
    };
    let (a, b) = (pixel("a"), pixel("b"));
    refused(&[&a], 1, "no layer below");
    refused(
        &[
            &a,
            &b,
            r#"{"command": "layer.update", "params": {"id": 2, "blend": "multiply"}}"#,
        ],
        2,
        "Normal",
    );
    refused(
        &[
            &a,
            &b,
            r#"{"command": "layer.update", "params": {"id": 1, "blend": "screen"}}"#,
        ],
        2,
        "Normal",
    );
    refused(
        &[
            &a,
            &b,
            r#"{"command": "layer.update", "params": {"id": 1, "visible": false}}"#,
        ],
        2,
        "visible",
    );
    refused(
        &[
            &a,
            &b,
            r#"{"command": "layer.update", "params": {"id": 2, "locked": true}}"#,
        ],
        2,
        "locked",
    );
    refused(
        &[
            &a,
            r#"{"command": "layer.create", "params": {"name": "r", "kind": {"type": "adjustment", "adjustment": {"kind": "brightness_contrast", "brightness": 10, "contrast": 0}}}}"#,
        ],
        2,
        "pixel layers",
    );
    // Below is clipped, the upper layer is not: the result would be clipped.
    refused(&[&a, &b, &clip(2), &pixel("c")], 3, "clipped");
    // The upper layer is the base of a clipped layer above it.
    refused(&[&a, &b, &pixel("c"), &clip(3)], 2, "base");
}

fn clip(id: u64) -> String {
    format!(r#"{{"command": "layer.update", "params": {{"id": {id}, "clip": true}}}}"#)
}

#[test]
fn merging_a_clipped_layer_onto_its_base_gives_an_unclipped_layer() {
    let (mut bus, mut document) = setup(&[&pixel("base"), &pixel("détouré"), &clip(2)]);
    let merger = FakeMerge {
        calls: Cell::new(0),
    };
    let resolvers = Resolvers {
        import: None,
        merge: Some(&merger),
    };
    resolve_and_execute(
        &mut bus,
        &mut document,
        command(&merge(2)),
        Origin::Test,
        &resolvers,
    )
    .unwrap();
    assert!(!document.node(NodeId(1)).unwrap().clip);
}
