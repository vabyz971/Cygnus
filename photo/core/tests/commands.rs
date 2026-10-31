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

//! Photo model and commands: invariants, exact inverses, refusals, catalog.

// Test helpers outside `#[test]` functions unwrap too.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use cygnus_commands::{Catalog, Command, CommandBus, Source, Stability, catalog_json};
use cygnus_photo_core::commands::{
    DocumentCreate, LayerCreate, LayerDelete, LayerMove, LayerUpdate, PixelsFill, PixelsWrite,
};
use cygnus_photo_core::pixels::{TILE_SIZE, TileCoord};
use cygnus_photo_core::{BlendMode, CoreError, Depth, Document, NodeId, PhotoCommand, Rect};

fn create(width: u32, height: u32, depth: Depth) -> PhotoCommand {
    PhotoCommand::DocumentCreate(DocumentCreate {
        width,
        height,
        resolution: 300.0,
        depth,
        profile: Default::default(),
    })
}

fn new_layer(name: &str) -> PhotoCommand {
    PhotoCommand::LayerCreate(LayerCreate {
        name: name.to_owned(),
        parent: None,
        index: None,
    })
}

fn fill(layer: u64, rect: Rect, color: [f32; 4]) -> PhotoCommand {
    PhotoCommand::PixelsFill(PixelsFill {
        layer: NodeId(layer),
        rect,
        color,
    })
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Equality of content. The identifier counter is left out: identifiers are
/// never reused, so undoing a creation does not move the counter back.
fn same(a: &Document, b: &Document) -> bool {
    let mut b = b.clone();
    b.next_id = a.next_id;
    *a == b
}

fn run(document: &mut Document, commands: Vec<PhotoCommand>) {
    let mut bus = CommandBus::new();
    for command in commands {
        bus.execute(document, command, Source::Test).unwrap();
    }
    document.validate().unwrap();
}

/// A 600 × 400 16-bit document with two filled layers, ids 1 and 2.
fn sample() -> Document {
    let mut document = Document::default();
    run(
        &mut document,
        vec![
            create(600, 400, Depth::U16),
            new_layer("Fond"),
            new_layer("Haut"),
            fill(1, rect(0, 0, 600, 400), [0.2, 0.4, 0.6, 1.0]),
            fill(2, rect(100, 50, 300, 200), [1.0, 0.0, 0.0, 0.5]),
        ],
    );
    document
}

#[test]
fn creates_document_and_layers() {
    let document = sample();
    assert_eq!((document.width, document.height), (600, 400));
    assert_eq!(document.children(document.root), &[NodeId(1), NodeId(2)]);
    assert_eq!(document.node(NodeId(1)).unwrap().name, "Fond");
    assert_eq!(document.peek_next_id(), NodeId(3));
}

#[test]
fn every_command_has_an_exact_inverse() {
    let commands = vec![
        create(32, 32, Depth::U8),
        new_layer("Nouveau"),
        PhotoCommand::LayerCreate(LayerCreate {
            name: "Dessous".to_owned(),
            parent: None,
            index: Some(0),
        }),
        PhotoCommand::LayerUpdate(LayerUpdate {
            id: NodeId(2),
            name: Some("Renommé".to_owned()),
            visible: Some(false),
            opacity: Some(0.25),
            blend: Some(BlendMode::Normal),
            locked: Some(true),
        }),
        PhotoCommand::LayerDelete(LayerDelete { id: NodeId(1) }),
        PhotoCommand::LayerDelete(LayerDelete { id: NodeId(2) }),
        PhotoCommand::LayerMove(LayerMove {
            id: NodeId(1),
            parent: None,
            index: 1,
        }),
        fill(1, rect(0, 0, 600, 400), [0.0; 4]),
        fill(2, rect(250, 250, 20, 20), [0.1, 0.2, 0.3, 0.4]),
        PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(1),
            rect: rect(255, 10, 2, 1),
            samples: vec![1, 2, 3, 4, 65535, 0, 0, 65535],
        }),
    ];
    for command in commands {
        let name = command.name();
        let before = sample();
        let mut document = before.clone();
        let applied = command.apply(&mut document).unwrap();
        document.validate().unwrap();
        let after = document.clone();

        let redo = applied.inverse.apply(&mut document).unwrap();
        assert!(same(&document, &before), "undo of {name}");
        redo.inverse.apply(&mut document).unwrap();
        assert!(same(&document, &after), "redo of {name}");
        assert!(
            document.next_id >= after.next_id,
            "{name} reused an identifier"
        );
    }
}

#[test]
fn identifiers_are_stable_and_never_reused() {
    let mut document = sample();
    let deleted = PhotoCommand::LayerDelete(LayerDelete { id: NodeId(1) })
        .apply(&mut document)
        .unwrap();
    // A new layer does not take the identifier of the deleted one.
    let created = new_layer("Autre").apply(&mut document).unwrap();
    assert_eq!(created.changes.created, vec![NodeId(3)]);
    // Undoing the deletion brings the layer back under its identifier.
    deleted.inverse.apply(&mut document).unwrap();
    assert_eq!(document.node(NodeId(1)).unwrap().name, "Fond");
    assert_eq!(document.children(document.root)[0], NodeId(1));
    document.validate().unwrap();
}

#[test]
fn refused_commands_change_nothing() {
    let mut locked = sample();
    run(
        &mut locked,
        vec![PhotoCommand::LayerUpdate(LayerUpdate {
            id: NodeId(2),
            locked: Some(true),
            ..LayerUpdate::new(NodeId(2))
        })],
    );
    let cases = vec![
        (
            sample(),
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(1),
                opacity: Some(1.5),
                ..LayerUpdate::new(NodeId(1))
            }),
        ),
        (
            sample(),
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(1),
                blend: Some(BlendMode::Multiply),
                ..LayerUpdate::new(NodeId(1))
            }),
        ),
        (
            sample(),
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(1),
                opacity: Some(f32::NAN),
                ..LayerUpdate::new(NodeId(1))
            }),
        ),
        (
            sample(),
            PhotoCommand::LayerDelete(LayerDelete { id: NodeId(99) }),
        ),
        (
            sample(),
            PhotoCommand::LayerDelete(LayerDelete { id: NodeId(0) }),
        ),
        (
            sample(),
            PhotoCommand::LayerMove(LayerMove {
                id: NodeId(1),
                parent: None,
                index: 2,
            }),
        ),
        (
            sample(),
            PhotoCommand::LayerMove(LayerMove {
                id: NodeId(1),
                parent: Some(NodeId(2)),
                index: 0,
            }),
        ),
        (
            sample(),
            PhotoCommand::LayerCreate(LayerCreate {
                name: String::new(),
                parent: None,
                index: Some(3),
            }),
        ),
        (sample(), fill(1, rect(0, 0, 1, 1), [0.0, 0.0, 0.0, 2.0])),
        (sample(), fill(0, rect(0, 0, 1, 1), [0.0; 4])),
        (
            sample(),
            PhotoCommand::PixelsWrite(PixelsWrite {
                layer: NodeId(1),
                rect: rect(0, 0, 2, 2),
                samples: vec![0; 15],
            }),
        ),
        (
            sample(),
            PhotoCommand::PixelsWrite(PixelsWrite {
                layer: NodeId(1),
                rect: rect(599, 0, 2, 1),
                samples: vec![0; 8],
            }),
        ),
        (sample(), create(0, 10, Depth::U8)),
        (sample(), create(70_000, 10, Depth::U8)),
        (locked.clone(), fill(2, rect(0, 0, 1, 1), [0.0; 4])),
        (
            locked.clone(),
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(2),
                opacity: Some(0.5),
                ..LayerUpdate::new(NodeId(2))
            }),
        ),
        (
            locked.clone(),
            PhotoCommand::LayerDelete(LayerDelete { id: NodeId(2) }),
        ),
        (
            locked.clone(),
            PhotoCommand::LayerMove(LayerMove {
                id: NodeId(2),
                parent: None,
                index: 0,
            }),
        ),
    ];
    for (before, command) in cases {
        let name = command.name();
        let mut document = before.clone();
        assert!(command.apply(&mut document).is_err(), "{name} accepted");
        assert!(document == before, "{name} changed the document");
    }
}

#[test]
fn eight_bit_values_above_255_are_refused() {
    let mut document = Document::default();
    run(&mut document, vec![create(4, 4, Depth::U8), new_layer("a")]);
    let error = PhotoCommand::PixelsWrite(PixelsWrite {
        layer: NodeId(1),
        rect: rect(0, 0, 1, 1),
        samples: vec![256, 0, 0, 255],
    })
    .apply(&mut document)
    .unwrap_err();
    assert!(matches!(
        error,
        CoreError::InvalidValue {
            param: "samples",
            ..
        }
    ));
}

#[test]
fn locked_layer_can_be_hidden_and_unlocked() {
    let mut document = sample();
    run(
        &mut document,
        vec![
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(1),
                locked: Some(true),
                ..LayerUpdate::new(NodeId(1))
            }),
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(1),
                visible: Some(false),
                ..LayerUpdate::new(NodeId(1))
            }),
            PhotoCommand::LayerUpdate(LayerUpdate {
                id: NodeId(1),
                locked: Some(false),
                ..LayerUpdate::new(NodeId(1))
            }),
        ],
    );
    let layer = document.node(NodeId(1)).unwrap();
    assert!(!layer.visible && !layer.locked);
}

#[test]
fn fill_is_clipped_and_quantized() {
    let mut document = Document::default();
    run(
        &mut document,
        vec![
            create(300, 300, Depth::U8),
            new_layer("a"),
            fill(1, rect(250, 0, 1000, 10), [0.5, 0.0, 1.0, 1.0]),
        ],
    );
    // 0.5 × 255 = 127.5, rounded to the nearest value: 128.
    assert_eq!(document.pixels.pixel(NodeId(1), 299, 0), [128, 0, 255, 255]);
    assert_eq!(document.pixels.pixel(NodeId(1), 249, 0), [0; 4]);
    assert_eq!(document.pixels.pixel(NodeId(1), 250, 10), [0; 4]);
}

#[test]
fn write_places_samples_row_by_row() {
    let mut document = Document::default();
    run(
        &mut document,
        vec![
            create(300, 300, Depth::U16),
            new_layer("a"),
            PhotoCommand::PixelsWrite(PixelsWrite {
                layer: NodeId(1),
                rect: rect(255, 0, 2, 2),
                samples: (0..16).collect(),
            }),
        ],
    );
    let pixels = &document.pixels;
    assert_eq!(pixels.pixel(NodeId(1), 255, 0), [0, 1, 2, 3]);
    assert_eq!(pixels.pixel(NodeId(1), 256, 0), [4, 5, 6, 7]);
    assert_eq!(pixels.pixel(NodeId(1), 255, 1), [8, 9, 10, 11]);
    assert_eq!(pixels.pixel(NodeId(1), 256, 1), [12, 13, 14, 15]);
}

#[test]
fn changes_report_what_was_touched() {
    let mut document = sample();
    let filled = fill(1, rect(250, 0, 10, 10), [1.0; 4])
        .apply(&mut document)
        .unwrap();
    let tiles: Vec<_> = filled.changes.pixels[&NodeId(1)].iter().copied().collect();
    assert_eq!(
        tiles,
        vec![TileCoord { tx: 0, ty: 0 }, TileCoord { tx: 1, ty: 0 }]
    );
    assert!(!filled.changes.structure);

    let updated = PhotoCommand::LayerUpdate(LayerUpdate {
        id: NodeId(2),
        opacity: Some(0.5),
        ..LayerUpdate::new(NodeId(2))
    })
    .apply(&mut document)
    .unwrap();
    assert!(updated.changes.properties.contains(&NodeId(2)));

    let moved = PhotoCommand::LayerMove(LayerMove {
        id: NodeId(2),
        parent: None,
        index: 0,
    })
    .apply(&mut document)
    .unwrap();
    assert!(moved.changes.structure);
    assert_eq!(document.children(document.root), &[NodeId(2), NodeId(1)]);
}

#[test]
fn overwritten_tiles_are_counted_for_the_memory_budget() {
    let mut document = sample();
    let applied = fill(1, rect(0, 0, 300, 10), [1.0; 4])
        .apply(&mut document)
        .unwrap();
    // Two 16-bit RGBA tiles were overwritten and are kept for undo.
    let tile_bytes = (TILE_SIZE * TILE_SIZE * 4 * 2) as usize;
    assert_eq!(applied.inverse_bytes, 2 * tile_bytes);
    let write = PhotoCommand::PixelsWrite(PixelsWrite {
        layer: NodeId(1),
        rect: rect(0, 0, 1, 1),
        samples: vec![0; 4],
    });
    assert_eq!(write.payload_bytes(), 8);
}

#[test]
fn tree_survives_serialization() {
    let mut document = Document::default();
    run(
        &mut document,
        vec![create(64, 48, Depth::U8), new_layer("a"), new_layer("b")],
    );
    let json = serde_json::to_string(&document).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    // Pixels are not part of the serialized tree; this document has none.
    assert_eq!(back, document);
    back.validate().unwrap();
}

#[test]
fn public_commands_survive_serialization() {
    let commands = vec![
        create(10, 10, Depth::U16),
        new_layer("a"),
        PhotoCommand::LayerUpdate(LayerUpdate {
            id: NodeId(1),
            opacity: Some(0.5),
            ..LayerUpdate::new(NodeId(1))
        }),
        PhotoCommand::LayerDelete(LayerDelete { id: NodeId(1) }),
        PhotoCommand::LayerMove(LayerMove {
            id: NodeId(1),
            parent: Some(NodeId(0)),
            index: 0,
        }),
        fill(1, rect(0, 0, 1, 1), [0.0, 0.5, 1.0, 1.0]),
        PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(1),
            rect: rect(0, 0, 1, 1),
            samples: vec![1, 2, 3, 4],
        }),
    ];
    for command in commands {
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(json["command"], command.name());
        let back: PhotoCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, command);
    }
    // A script line as written by hand, with defaults.
    let parsed: PhotoCommand = serde_json::from_str(
        r#"{"command": "document.create", "params": {"width": 8, "height": 8, "depth": "u16"}}"#,
    )
    .unwrap();
    assert_eq!(parsed.name(), "document.create");
}

#[test]
fn catalog_lists_public_commands_with_their_stability() {
    let commands = PhotoCommand::commands();
    let names: Vec<_> = commands.iter().map(|info| info.name).collect();
    assert_eq!(
        names,
        [
            "document.create",
            "layer.create",
            "layer.update",
            "layer.delete",
            "layer.move",
            "pixels.fill",
            "pixels.write",
        ]
    );
    for info in &commands {
        let expected = if info.name.starts_with("pixels.") {
            Stability::Development
        } else {
            Stability::Stable
        };
        assert_eq!(info.stability, expected, "{}", info.name);
    }
}

#[test]
fn catalog_matches_its_snapshot() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/catalog.snapshot.json");
    let current = serde_json::to_string_pretty(&catalog_json::<PhotoCommand>()).unwrap() + "\n";
    if std::env::var("CYGNUS_UPDATE_SNAPSHOT").is_ok_and(|value| value == "1") {
        std::fs::write(path, &current).unwrap();
    }
    let stored = std::fs::read_to_string(path)
        .expect("missing snapshot: run with CYGNUS_UPDATE_SNAPSHOT=1 and review it");
    assert_eq!(
        current, stored,
        "the command catalog changed: review it, then run with CYGNUS_UPDATE_SNAPSHOT=1"
    );
}
