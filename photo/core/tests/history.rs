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

//! History of Photo documents (docs/decisions/0022): random sequences of
//! valid commands with gestures and interleaved undo/redo, tile sharing of
//! snapshots, bounded memory, saturation warning.

// Test helpers outside `#[test]` functions unwrap too.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use cygnus_commands::{BusLimits, CommandBus, Source};
use cygnus_photo_core::adjust::HueSaturation;
use cygnus_photo_core::commands::*;
use cygnus_photo_core::pixels::TileCoord;
use cygnus_photo_core::{
    Adjustment, BlendMode, Depth, Document, Filter, Isolation, NodeId, NodeKind, PhotoCommand, Rect,
};

/// Small deterministic generator (xorshift), so that a failing seed can be
/// replayed.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f32 {
        (self.next() % 1001) as f32 / 1000.0
    }

    fn pick(&mut self, ids: &[NodeId]) -> Option<NodeId> {
        (!ids.is_empty()).then(|| ids[self.below(ids.len())])
    }
}

fn same(a: &Document, b: &Document) -> bool {
    let mut b = b.clone();
    b.next_id = a.next_id;
    *a == b
}

fn start() -> Document {
    let mut document = Document::default();
    let mut bus = CommandBus::new();
    for command in [
        PhotoCommand::DocumentCreate(DocumentCreate {
            width: 300,
            height: 300,
            resolution: 72.0,
            depth: Depth::U8,
            profile: Default::default(),
        }),
        PhotoCommand::LayerCreate(LayerCreate {
            name: "fond".to_owned(),
            parent: None,
            index: None,
            kind: LayerKind::Pixel,
        }),
        PhotoCommand::PixelsFill(PixelsFill {
            layer: NodeId(1),
            rect: Rect::of_size(300, 300),
            color: [0.5, 0.5, 0.5, 1.0],
        }),
    ] {
        bus.execute(&mut document, command, Source::Test).unwrap();
    }
    document
}

fn layers(document: &Document) -> Vec<NodeId> {
    document
        .nodes
        .values()
        .filter(|node| node.kind.is_layer() && node.id != document.root)
        .map(|node| node.id)
        .collect()
}

fn of_kind(document: &Document, test: fn(&NodeKind) -> bool) -> Vec<NodeId> {
    document
        .nodes
        .values()
        .filter(|node| test(&node.kind) && node.id != document.root)
        .map(|node| node.id)
        .collect()
}

fn random_rect(random: &mut Random) -> Rect {
    Rect {
        x: random.below(300) as u32,
        y: random.below(300) as u32,
        width: 1 + random.below(299) as u32,
        height: 1 + random.below(299) as u32,
    }
}

/// A command that is usually valid for `document`; refused ones are fine,
/// they change nothing.
fn random_command(random: &mut Random, document: &Document) -> Option<PhotoCommand> {
    let layers = layers(document);
    let groups: Vec<NodeId> = std::iter::once(document.root)
        .chain(of_kind(document, |kind| {
            matches!(kind, NodeKind::Group { .. })
        }))
        .collect();
    let pixels = of_kind(document, |kind| matches!(kind, NodeKind::Pixel { .. }));
    let masks = of_kind(document, |kind| matches!(kind, NodeKind::Mask { .. }));
    let adjustments = of_kind(document, |kind| matches!(kind, NodeKind::Adjustment { .. }));
    let parent = random.pick(&groups);
    Some(match random.below(17) {
        0 | 1 => PhotoCommand::LayerCreate(LayerCreate {
            name: "p".to_owned(),
            parent,
            index: None,
            kind: LayerKind::Pixel,
        }),
        2 => PhotoCommand::LayerCreate(LayerCreate {
            name: "g".to_owned(),
            parent,
            index: None,
            kind: LayerKind::Group {
                isolation: if random.below(2) == 0 {
                    Isolation::Isolated
                } else {
                    Isolation::PassThrough
                },
            },
        }),
        3 => PhotoCommand::LayerCreate(LayerCreate {
            name: "r".to_owned(),
            parent,
            index: None,
            kind: LayerKind::Adjustment {
                adjustment: Adjustment::HueSaturation(HueSaturation {
                    hue: random.unit() * 360.0 - 180.0,
                    saturation: random.unit() * 200.0 - 100.0,
                    lightness: 0.0,
                }),
            },
        }),
        4 => PhotoCommand::LayerCreate(LayerCreate {
            name: "f".to_owned(),
            parent,
            index: None,
            kind: LayerKind::Filter {
                filter: Filter::GaussianBlur {
                    radius: 0.1 + random.unit() * 99.0,
                },
            },
        }),
        5 => PhotoCommand::LayerUpdate(LayerUpdate {
            opacity: Some(random.unit()),
            visible: Some(random.below(4) != 0),
            blend: Some(BlendMode::ALL[random.below(12)]),
            clip: Some(random.below(3) == 0),
            ..LayerUpdate::new(random.pick(&layers)?)
        }),
        6 => PhotoCommand::LayerMove(LayerMove {
            id: random.pick(&layers)?,
            parent,
            index: 0,
        }),
        7 => PhotoCommand::LayerDuplicate(LayerDuplicate {
            id: random.pick(&layers)?,
        }),
        8 => PhotoCommand::LayerDelete(LayerDelete {
            id: random.pick(&layers)?,
        }),
        9 => {
            let id = random.pick(&layers)?;
            let parent = document.node(id)?.parent?;
            let siblings = document.children(parent);
            let other = siblings[random.below(siblings.len())];
            PhotoCommand::LayerGroup(LayerGroup {
                ids: if other == id {
                    vec![id]
                } else {
                    vec![id, other]
                },
                name: "rg".to_owned(),
                isolation: Isolation::PassThrough,
            })
        }
        10 => PhotoCommand::LayerUngroup(LayerUngroup {
            id: random.pick(&groups[1..])?,
        }),
        11 => PhotoCommand::MaskCreate(MaskCreate {
            node: random.pick(&layers)?,
            fill: if random.below(2) == 0 {
                MaskFill::White
            } else {
                MaskFill::Black
            },
        }),
        12 => PhotoCommand::MaskUpdate(MaskUpdate {
            id: random.pick(&masks)?,
            density: Some(random.unit()),
            inverted: Some(random.below(2) == 0),
            enabled: None,
        }),
        13 => PhotoCommand::MaskFill(MaskFillRect {
            mask: random.pick(&masks)?,
            rect: random_rect(random),
            value: random.unit(),
        }),
        14 => PhotoCommand::MaskGradient(MaskGradient {
            mask: random.pick(&masks)?,
            start: [random.unit() * 300.0, 0.0],
            end: [random.unit() * 300.0 + 1.0, 300.0],
            start_value: random.unit(),
            end_value: random.unit(),
        }),
        15 => PhotoCommand::PixelsFill(PixelsFill {
            layer: random.pick(&pixels)?,
            rect: random_rect(random),
            color: [random.unit(), random.unit(), random.unit(), random.unit()],
        }),
        _ => match random.pick(&adjustments) {
            Some(id) => PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
                id,
                adjustment: Adjustment::HueSaturation(HueSaturation {
                    hue: random.unit() * 360.0 - 180.0,
                    saturation: 0.0,
                    lightness: random.unit() * 200.0 - 100.0,
                }),
            }),
            None => PhotoCommand::MaskDelete(MaskDelete {
                node: random.pick(&layers)?,
            }),
        },
    })
}

#[test]
fn random_sequences_undo_to_the_start_and_redo_to_the_end() {
    for seed in 1..=40u64 {
        let initial = start();
        let mut document = initial.clone();
        let mut bus = CommandBus::new();
        let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut applied = 0;
        for _ in 0..80 {
            match random.below(10) {
                // Interleaved undo and redo.
                0 => {
                    bus.undo(&mut document, Source::Test).unwrap();
                }
                1 => {
                    bus.redo(&mut document, Source::Test).unwrap();
                }
                // A gesture of opacity changes on one layer, merged.
                2 => {
                    if let Some(id) = random.pick(&layers(&document)) {
                        bus.begin_gesture();
                        for _ in 0..5 {
                            let update = PhotoCommand::LayerUpdate(LayerUpdate {
                                opacity: Some(random.unit()),
                                ..LayerUpdate::new(id)
                            });
                            let _ = bus.execute(&mut document, update, Source::User);
                        }
                        bus.end_gesture();
                    }
                }
                _ => {
                    if let Some(command) = random_command(&mut random, &document)
                        && bus.execute(&mut document, command, Source::Test).is_ok()
                    {
                        applied += 1;
                    }
                }
            }
            document
                .validate()
                .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        }
        assert!(applied > 20, "seed {seed}: only {applied} commands applied");
        assert_eq!(bus.truncated(), 0, "seed {seed}: history was truncated");

        // Redo what is pending, then undo everything, then redo everything.
        while bus.redo(&mut document, Source::Test).unwrap() {}
        let end = document.clone();
        while bus.undo(&mut document, Source::Test).unwrap() {}
        assert!(same(&document, &initial), "seed {seed}: undoing all");
        while bus.redo(&mut document, Source::Test).unwrap() {}
        assert!(same(&document, &end), "seed {seed}: redoing all");
        document.validate().unwrap();
    }
}

/// A 2048 × 2048 16-bit document with an isolated group of `layers` fully
/// filled pixel layers (64 tiles of 512 KiB each per layer).
fn big_group(layers: u64) -> (Document, CommandBus<PhotoCommand>, NodeId) {
    let mut document = Document::default();
    let mut bus = CommandBus::with_limits(BusLimits {
        retained_bytes: 64 * 1024 * 1024,
        ..BusLimits::default()
    });
    let mut run = |command| bus.execute(&mut document, command, Source::Test).unwrap();
    run(PhotoCommand::DocumentCreate(DocumentCreate {
        width: 2048,
        height: 2048,
        resolution: 72.0,
        depth: Depth::U16,
        profile: Default::default(),
    }));
    run(PhotoCommand::LayerCreate(LayerCreate {
        name: "groupe".to_owned(),
        parent: None,
        index: None,
        kind: LayerKind::Group {
            isolation: Isolation::Isolated,
        },
    }));
    for i in 0..layers {
        run(PhotoCommand::LayerCreate(LayerCreate {
            name: format!("calque {i}"),
            parent: Some(NodeId(1)),
            index: None,
            kind: LayerKind::Pixel,
        }));
        run(PhotoCommand::PixelsFill(PixelsFill {
            layer: NodeId(2 + i),
            rect: Rect::of_size(2048, 2048),
            color: [0.3, 0.6, 0.9, 1.0],
        }));
    }
    drop(run);
    (document, bus, NodeId(1))
}

#[test]
fn snapshots_share_tiles_instead_of_copying_them() {
    let (mut document, mut bus, group) = big_group(1);
    let coord = TileCoord { tx: 3, ty: 5 };
    let tile = document.pixels.tile(NodeId(2), coord).unwrap().clone();
    assert_eq!(Arc::strong_count(&tile), 2, "the document and this test");

    bus.execute(
        &mut document,
        PhotoCommand::LayerDelete(LayerDelete { id: group }),
        Source::Test,
    )
    .unwrap();
    assert!(document.pixels.tile(NodeId(2), coord).is_none());
    // The undo record holds the same tile, not a copy.
    assert_eq!(Arc::strong_count(&tile), 2, "the undo record and this test");

    bus.undo(&mut document, Source::Test).unwrap();
    let restored = document.pixels.tile(NodeId(2), coord).unwrap();
    assert!(Arc::ptr_eq(restored, &tile), "undo puts back the same tile");

    // Writing into a shared tile copies it, leaving the history intact.
    bus.execute(
        &mut document,
        PhotoCommand::PixelsFill(PixelsFill {
            layer: NodeId(2),
            rect: Rect {
                x: 3 * 256,
                y: 5 * 256,
                width: 1,
                height: 1,
            },
            color: [1.0, 0.0, 0.0, 1.0],
        }),
        Source::Test,
    )
    .unwrap();
    let written = document.pixels.tile(NodeId(2), coord).unwrap();
    assert!(!Arc::ptr_eq(written, &tile));
    assert_ne!(written.get(0, 0), tile.get(0, 0));
}

#[test]
fn deleting_a_big_group_keeps_memory_bounded_and_warns() {
    // Three layers of 32 MiB: deleting the group keeps 96 MiB for undo,
    // above the 64 MiB budget.
    let (mut document, mut bus, group) = big_group(3);
    assert_eq!(bus.truncated(), 0);
    bus.execute(
        &mut document,
        PhotoCommand::LayerDelete(LayerDelete { id: group }),
        Source::Test,
    )
    .unwrap();
    assert!(document.node(group).is_none(), "the command is applied");
    assert!(bus.retained_bytes() <= 64 * 1024 * 1024);
    // Older entries cannot be undone without this one: the history is
    // emptied from the oldest, and the removal is reported (0022).
    assert!(!bus.can_undo());
    assert!(bus.truncated() > 0);
    document.validate().unwrap();
}

#[test]
fn saturation_removes_the_oldest_entries_and_keeps_the_rest_undoable() {
    let (mut document, mut bus, _) = big_group(1);
    let before = bus.truncated();
    // Fills of the 32 MiB layer each keep 32 MiB of overwritten tiles:
    // with a 64 MiB budget, only the two latest stay.
    for i in 0..5 {
        bus.execute(
            &mut document,
            PhotoCommand::PixelsFill(PixelsFill {
                layer: NodeId(2),
                rect: Rect::of_size(2048, 2048),
                color: [i as f32 / 10.0, 0.0, 0.0, 1.0],
            }),
            Source::Test,
        )
        .unwrap();
    }
    assert!(bus.truncated() > before, "the oldest entries were removed");
    assert!(bus.retained_bytes() <= 64 * 1024 * 1024);
    let listed = bus.entries().count();
    let mut undone = 0;
    while bus.undo(&mut document, Source::Test).unwrap() {
        undone += 1;
        document.validate().unwrap();
    }
    assert_eq!(undone, listed, "every listed entry can be undone");
    assert_eq!(
        document.pixels.pixel(NodeId(2), 0, 0)[0],
        13107,
        "back to 0.2 × 65535"
    );
}

#[test]
fn history_commands_are_in_the_catalog_and_go_through_the_bus() {
    let mut document = start();
    let mut bus = CommandBus::new();
    bus.execute(
        &mut document,
        PhotoCommand::LayerUpdate(LayerUpdate {
            opacity: Some(0.3),
            ..LayerUpdate::new(NodeId(1))
        }),
        Source::User,
    )
    .unwrap();
    let undo: PhotoCommand =
        serde_json::from_str(r#"{"command": "history.undo", "params": {}}"#).unwrap();
    bus.execute(&mut document, undo, Source::Agent).unwrap();
    assert_eq!(document.node(NodeId(1)).unwrap().opacity, 1.0);
    let redo: PhotoCommand =
        serde_json::from_str(r#"{"command": "history.redo", "params": {}}"#).unwrap();
    bus.execute(&mut document, redo, Source::Agent).unwrap();
    assert_eq!(document.node(NodeId(1)).unwrap().opacity, 0.3);
    use cygnus_commands::Catalog;
    let names: Vec<_> = PhotoCommand::commands()
        .iter()
        .map(|info| info.name)
        .collect();
    assert!(names.contains(&"history.undo") && names.contains(&"history.redo"));
}
