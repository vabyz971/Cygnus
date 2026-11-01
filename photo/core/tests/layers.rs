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

//! Commands of J3 (docs/decisions/0020): groups, masks, adjustment and
//! filter layers, duplication, grouping, clipping. Exact inverses, tree
//! invariants and refusals.

// Test helpers outside `#[test]` functions unwrap too.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use cygnus_commands::{Command, CommandBus, Source};
use cygnus_photo_core::adjust::{Curve, Curves, HueSaturation};
use cygnus_photo_core::commands::*;
use cygnus_photo_core::pixels::TileCoord;
use cygnus_photo_core::{
    Adjustment, BlendMode, CoreError, Depth, Document, Filter, Isolation, NodeId, NodeKind,
    PhotoCommand, Rect,
};

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn layer(name: &str, kind: LayerKind, parent: Option<u64>) -> PhotoCommand {
    PhotoCommand::LayerCreate(LayerCreate {
        name: name.to_owned(),
        parent: parent.map(NodeId),
        index: None,
        kind,
    })
}

fn curves() -> Adjustment {
    Adjustment::Curves(Curves {
        master: Curve {
            points: vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]],
        },
        ..Curves::default()
    })
}

fn run(document: &mut Document, commands: Vec<PhotoCommand>) {
    let mut bus = CommandBus::new();
    for command in commands {
        let name = command.name();
        bus.execute(document, command, Source::Test)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    document.validate().unwrap();
}

/// Equality of content, the identifier counter left out (it never goes back).
fn same(a: &Document, b: &Document) -> bool {
    let mut b = b.clone();
    b.next_id = a.next_id;
    *a == b
}

/// 512 × 512, 16 bits:
/// 1 pixel "fond" (filled), 2 pixel "haut" (filled, with a gradient mask 3),
/// 4 curves adjustment, 5 isolated group "groupe" containing 6 pixel
/// "dedans" (filled, black mask 7 partly filled), 8 gaussian blur filter.
fn sample() -> Document {
    let mut document = Document::default();
    run(
        &mut document,
        vec![
            PhotoCommand::DocumentCreate(DocumentCreate {
                width: 512,
                height: 512,
                resolution: 300.0,
                depth: Depth::U16,
                profile: Default::default(),
            }),
            layer("fond", LayerKind::Pixel, None),
            PhotoCommand::PixelsFill(PixelsFill {
                layer: NodeId(1),
                rect: rect(0, 0, 512, 512),
                color: [0.2, 0.4, 0.6, 1.0],
            }),
            layer("haut", LayerKind::Pixel, None),
            PhotoCommand::PixelsFill(PixelsFill {
                layer: NodeId(2),
                rect: rect(100, 100, 300, 300),
                color: [0.9, 0.1, 0.1, 1.0],
            }),
            PhotoCommand::MaskCreate(MaskCreate {
                node: NodeId(2),
                fill: MaskFill::White,
            }),
            PhotoCommand::MaskGradient(MaskGradient {
                mask: NodeId(3),
                start: [0.0, 0.0],
                end: [512.0, 0.0],
                start_value: 0.0,
                end_value: 1.0,
            }),
            layer(
                "courbes",
                LayerKind::Adjustment {
                    adjustment: curves(),
                },
                None,
            ),
            layer(
                "groupe",
                LayerKind::Group {
                    isolation: Isolation::Isolated,
                },
                None,
            ),
            layer("dedans", LayerKind::Pixel, Some(5)),
            PhotoCommand::PixelsFill(PixelsFill {
                layer: NodeId(6),
                rect: rect(0, 0, 256, 256),
                color: [0.1, 0.8, 0.2, 0.5],
            }),
            PhotoCommand::MaskCreate(MaskCreate {
                node: NodeId(6),
                fill: MaskFill::Black,
            }),
            PhotoCommand::MaskFill(MaskFillRect {
                mask: NodeId(7),
                rect: rect(0, 0, 300, 300),
                value: 1.0,
            }),
            layer(
                "flou",
                LayerKind::Filter {
                    filter: Filter::GaussianBlur { radius: 4.0 },
                },
                None,
            ),
        ],
    );
    document
}

#[test]
fn sample_has_the_expected_tree() {
    let document = sample();
    let root = document.root;
    assert_eq!(
        document.children(root),
        &[NodeId(1), NodeId(2), NodeId(4), NodeId(5), NodeId(8)]
    );
    assert_eq!(document.children(NodeId(5)), &[NodeId(6)]);
    assert_eq!(document.node(NodeId(2)).unwrap().mask, Some(NodeId(3)));
    assert_eq!(document.node(NodeId(6)).unwrap().mask, Some(NodeId(7)));
    // A new group is pass-through by default (Q2).
    let mut fresh = Document::default();
    run(
        &mut fresh,
        vec![layer(
            "g",
            LayerKind::Group {
                isolation: Default::default(),
            },
            None,
        )],
    );
    assert!(matches!(
        fresh.node(NodeId(1)).unwrap().kind,
        NodeKind::Group {
            isolation: Isolation::PassThrough,
            ..
        }
    ));
}

#[test]
fn every_command_of_j3_has_an_exact_inverse() {
    let commands = vec![
        layer("p", LayerKind::Pixel, Some(5)),
        layer(
            "g",
            LayerKind::Group {
                isolation: Isolation::PassThrough,
            },
            None,
        ),
        layer(
            "ts",
            LayerKind::Adjustment {
                adjustment: Adjustment::HueSaturation(HueSaturation {
                    hue: 30.0,
                    saturation: -20.0,
                    lightness: 5.0,
                }),
            },
            None,
        ),
        layer(
            "net",
            LayerKind::Filter {
                filter: Filter::UnsharpMask {
                    radius: 2.0,
                    amount: 1.2,
                    threshold: 3.0,
                },
            },
            None,
        ),
        PhotoCommand::LayerDuplicate(LayerDuplicate { id: NodeId(5) }),
        PhotoCommand::LayerDuplicate(LayerDuplicate { id: NodeId(2) }),
        PhotoCommand::LayerDelete(LayerDelete { id: NodeId(5) }),
        PhotoCommand::LayerDelete(LayerDelete { id: NodeId(2) }),
        PhotoCommand::LayerGroup(LayerGroup {
            ids: vec![NodeId(4), NodeId(1)],
            name: "nouveau".to_owned(),
            isolation: Isolation::Isolated,
        }),
        PhotoCommand::LayerUngroup(LayerUngroup { id: NodeId(5) }),
        PhotoCommand::LayerMove(LayerMove {
            id: NodeId(1),
            parent: Some(NodeId(5)),
            index: 0,
        }),
        PhotoCommand::LayerUpdate(LayerUpdate {
            clip: Some(true),
            blend: Some(BlendMode::Screen),
            ..LayerUpdate::new(NodeId(4))
        }),
        PhotoCommand::LayerUpdate(LayerUpdate {
            isolation: Some(Isolation::PassThrough),
            opacity: Some(0.4),
            ..LayerUpdate::new(NodeId(5))
        }),
        PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
            id: NodeId(4),
            adjustment: Adjustment::HueSaturation(HueSaturation::default()),
        }),
        PhotoCommand::FilterUpdate(FilterUpdate {
            id: NodeId(8),
            filter: Filter::GaussianBlur { radius: 50.0 },
        }),
        PhotoCommand::MaskCreate(MaskCreate {
            node: NodeId(1),
            fill: MaskFill::Black,
        }),
        PhotoCommand::MaskCreate(MaskCreate {
            node: NodeId(5),
            fill: MaskFill::White,
        }),
        PhotoCommand::MaskDelete(MaskDelete { node: NodeId(2) }),
        PhotoCommand::MaskDelete(MaskDelete { node: NodeId(6) }),
        PhotoCommand::MaskUpdate(MaskUpdate {
            id: NodeId(3),
            density: Some(0.5),
            inverted: Some(true),
            enabled: Some(false),
        }),
        PhotoCommand::MaskFill(MaskFillRect {
            mask: NodeId(3),
            rect: rect(200, 200, 100, 400),
            value: 0.25,
        }),
        PhotoCommand::MaskGradient(MaskGradient {
            mask: NodeId(7),
            start: [0.0, 512.0],
            end: [0.0, 0.0],
            start_value: 1.0,
            end_value: 0.0,
        }),
    ];
    for command in commands {
        let name = command.name();
        let before = sample();
        let mut document = before.clone();
        let applied = command
            .apply(&mut document)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        document
            .validate()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let after = document.clone();

        let redo = applied.inverse.apply(&mut document).unwrap();
        document
            .validate()
            .unwrap_or_else(|error| panic!("undo of {name}: {error}"));
        assert!(same(&document, &before), "undo of {name}");
        redo.inverse.apply(&mut document).unwrap();
        assert!(same(&document, &after), "redo of {name}");
    }
}

#[test]
fn grouping_keeps_order_and_takes_the_place_of_the_topmost_layer() {
    let mut document = sample();
    run(
        &mut document,
        vec![PhotoCommand::LayerGroup(LayerGroup {
            ids: vec![NodeId(4), NodeId(1)],
            name: "g".to_owned(),
            isolation: Isolation::PassThrough,
        })],
    );
    let group = NodeId(9);
    assert_eq!(
        document.children(document.root),
        &[NodeId(2), group, NodeId(5), NodeId(8)]
    );
    assert_eq!(document.children(group), &[NodeId(1), NodeId(4)]);

    run(
        &mut document,
        vec![PhotoCommand::LayerUngroup(LayerUngroup { id: group })],
    );
    assert_eq!(
        document.children(document.root),
        &[NodeId(2), NodeId(1), NodeId(4), NodeId(5), NodeId(8)]
    );
}

#[test]
fn duplicate_shares_pixels_and_gives_new_identifiers() {
    let mut document = sample();
    let applied = PhotoCommand::LayerDuplicate(LayerDuplicate { id: NodeId(5) })
        .apply(&mut document)
        .unwrap();
    document.validate().unwrap();
    let copy = applied.changes.created[0];
    assert_eq!(document.children(document.root)[4], copy);
    let inner = document.children(copy)[0];
    assert_ne!(inner, NodeId(6));
    let mask = document.node(inner).unwrap().mask.unwrap();
    assert_ne!(mask, NodeId(7));
    // Tiles are shared, not copied.
    let coord = TileCoord { tx: 0, ty: 0 };
    let original = document.pixels.tile(NodeId(6), coord).unwrap();
    let copied = document.pixels.tile(inner, coord).unwrap();
    assert!(Arc::ptr_eq(original, copied));
}

#[test]
fn deleting_a_group_and_undoing_after_other_changes() {
    let mut document = sample();
    let deleted = PhotoCommand::LayerDelete(LayerDelete { id: NodeId(5) })
        .apply(&mut document)
        .unwrap();
    assert!(document.node(NodeId(6)).is_none() && document.node(NodeId(7)).is_none());
    assert_eq!(document.pixels.tiles(NodeId(6)).count(), 0);
    // Another change in the same parent, then the deletion is undone: the
    // group comes back at its place, the new layer stays.
    let created = layer("autre", LayerKind::Pixel, None)
        .apply(&mut document)
        .unwrap();
    deleted.inverse.apply(&mut document).unwrap();
    document.validate().unwrap();
    assert_eq!(document.children(document.root)[3], NodeId(5));
    assert!(document.node(created.changes.created[0]).is_some());
    assert!(document.pixels.tiles(NodeId(6)).count() > 0);
}

#[test]
fn masks_store_only_what_differs_from_their_empty_value() {
    let document = sample();
    // White mask filled by a gradient: tiles everywhere but where uniform.
    assert!(document.pixels.tiles(NodeId(3)).count() > 0);
    let max = 65535;
    let at = |id: u64, x, y, empty| document.pixels.pixel_or(NodeId(id), x, y, [empty; 4])[0];
    // Gradient from 0 (left) to 1 (right), at pixel centers.
    assert!(at(3, 0, 10, max) < 200);
    assert!(at(3, 511, 10, max) > 65335);
    let middle = at(3, 255, 10, max);
    assert!((32_600..32_900).contains(&middle), "{middle}");
    // Black mask, white inside its filled rectangle only.
    assert_eq!(at(7, 10, 10, 0), max);
    assert_eq!(at(7, 400, 400, 0), 0);
    // A fresh white mask stores nothing.
    let mut fresh = sample();
    run(
        &mut fresh,
        vec![PhotoCommand::MaskCreate(MaskCreate {
            node: NodeId(1),
            fill: MaskFill::White,
        })],
    );
    assert_eq!(fresh.pixels.tiles(NodeId(9)).count(), 0);
}

/// A document, a command it must refuse, and the expected error.
type RefusalCase = (Document, PhotoCommand, fn(&CoreError) -> bool);

#[test]
fn refused_commands_change_nothing() {
    let mut locked = sample();
    run(
        &mut locked,
        vec![PhotoCommand::LayerUpdate(LayerUpdate {
            locked: Some(true),
            ..LayerUpdate::new(NodeId(2))
        })],
    );
    let cases: Vec<RefusalCase> = vec![
        (
            sample(),
            PhotoCommand::MaskCreate(MaskCreate {
                node: NodeId(2),
                fill: MaskFill::White,
            }),
            |e| matches!(e, CoreError::AlreadyMasked(_)),
        ),
        (
            sample(),
            PhotoCommand::MaskCreate(MaskCreate {
                node: NodeId(3),
                fill: MaskFill::White,
            }),
            |e| matches!(e, CoreError::NotALayer(_)),
        ),
        (
            sample(),
            PhotoCommand::MaskCreate(MaskCreate {
                node: NodeId(0),
                fill: MaskFill::White,
            }),
            |e| matches!(e, CoreError::RootNode),
        ),
        (
            sample(),
            PhotoCommand::MaskDelete(MaskDelete { node: NodeId(1) }),
            |e| matches!(e, CoreError::NoMask(_)),
        ),
        (
            sample(),
            PhotoCommand::LayerMove(LayerMove {
                id: NodeId(3),
                parent: None,
                index: 0,
            }),
            |e| matches!(e, CoreError::NotALayer(_)),
        ),
        (
            sample(),
            PhotoCommand::LayerDelete(LayerDelete { id: NodeId(7) }),
            |e| matches!(e, CoreError::NotALayer(_)),
        ),
        (
            sample(),
            PhotoCommand::LayerUpdate(LayerUpdate {
                isolation: Some(Isolation::Isolated),
                ..LayerUpdate::new(NodeId(1))
            }),
            |e| matches!(e, CoreError::NotAGroup(_)),
        ),
        (
            sample(),
            PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
                id: NodeId(1),
                adjustment: curves(),
            }),
            |e| matches!(e, CoreError::WrongKind(..)),
        ),
        (
            sample(),
            PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
                id: NodeId(4),
                adjustment: Adjustment::HueSaturation(HueSaturation {
                    hue: 400.0,
                    ..Default::default()
                }),
            }),
            |e| matches!(e, CoreError::InvalidValue { .. }),
        ),
        (
            sample(),
            PhotoCommand::FilterUpdate(FilterUpdate {
                id: NodeId(8),
                filter: Filter::GaussianBlur { radius: 150.0 },
            }),
            |e| matches!(e, CoreError::InvalidValue { .. }),
        ),
        (
            sample(),
            PhotoCommand::LayerGroup(LayerGroup {
                ids: vec![NodeId(1), NodeId(6)],
                name: String::new(),
                isolation: Isolation::PassThrough,
            }),
            |e| matches!(e, CoreError::InvalidValue { .. }),
        ),
        (
            sample(),
            PhotoCommand::LayerGroup(LayerGroup {
                ids: vec![],
                name: String::new(),
                isolation: Isolation::PassThrough,
            }),
            |e| matches!(e, CoreError::InvalidValue { .. }),
        ),
        (
            sample(),
            PhotoCommand::LayerUngroup(LayerUngroup { id: NodeId(1) }),
            |e| matches!(e, CoreError::NotAGroup(_)),
        ),
        (
            sample(),
            PhotoCommand::MaskGradient(MaskGradient {
                mask: NodeId(3),
                start: [5.0, 5.0],
                end: [5.0, 5.0],
                start_value: 0.0,
                end_value: 1.0,
            }),
            |e| matches!(e, CoreError::InvalidValue { .. }),
        ),
        (
            sample(),
            PhotoCommand::MaskFill(MaskFillRect {
                mask: NodeId(1),
                rect: rect(0, 0, 1, 1),
                value: 1.0,
            }),
            |e| matches!(e, CoreError::NotAMask(_)),
        ),
        (
            locked.clone(),
            PhotoCommand::MaskFill(MaskFillRect {
                mask: NodeId(3),
                rect: rect(0, 0, 1, 1),
                value: 1.0,
            }),
            |e| matches!(e, CoreError::Locked(_)),
        ),
        (
            locked.clone(),
            PhotoCommand::MaskDelete(MaskDelete { node: NodeId(2) }),
            |e| matches!(e, CoreError::Locked(_)),
        ),
        (
            locked.clone(),
            PhotoCommand::LayerUpdate(LayerUpdate {
                clip: Some(true),
                ..LayerUpdate::new(NodeId(2))
            }),
            |e| matches!(e, CoreError::Locked(_)),
        ),
    ];
    for (before, command, expected) in cases {
        let name = command.name();
        let mut document = before.clone();
        let error = command.apply(&mut document).err();
        assert!(
            error.as_ref().is_some_and(expected),
            "{name}: unexpected outcome {error:?}"
        );
        assert!(document == before, "{name} changed the document");
    }
}

#[test]
fn new_commands_read_from_json() {
    let command: PhotoCommand = serde_json::from_str(
        r#"{"command": "layer.create", "params": {"name": "niveaux",
            "kind": {"type": "adjustment", "adjustment": {"kind": "levels",
              "master": {"input_black": 0.1, "input_white": 0.9, "gamma": 1.2, "output_black": 0.0, "output_white": 1.0},
              "red": {"input_black": 0.0, "input_white": 1.0, "gamma": 1.0, "output_black": 0.0, "output_white": 1.0},
              "green": {"input_black": 0.0, "input_white": 1.0, "gamma": 1.0, "output_black": 0.0, "output_white": 1.0},
              "blue": {"input_black": 0.0, "input_white": 1.0, "gamma": 1.0, "output_black": 0.0, "output_white": 1.0}}}}}"#,
    )
    .unwrap();
    let mut document = sample();
    run(&mut document, vec![command]);
    assert!(matches!(
        document.node(NodeId(9)).unwrap().kind,
        NodeKind::Adjustment {
            adjustment: Adjustment::Levels(_)
        }
    ));
    let filter: PhotoCommand = serde_json::from_str(
        r#"{"command": "layer.create", "params": {"kind": {"type": "filter", "filter": {"kind": "gaussian_blur", "radius": 3.5}}}}"#,
    )
    .unwrap();
    run(&mut document, vec![filter]);
    // A plain layer.create of J2 still reads as a pixel layer.
    let plain: PhotoCommand =
        serde_json::from_str(r#"{"command": "layer.create", "params": {"name": "a"}}"#).unwrap();
    run(&mut document, vec![plain]);
    assert!(matches!(
        document.node(NodeId(11)).unwrap().kind,
        NodeKind::Pixel { .. }
    ));
}

#[test]
fn validation_finds_broken_structures() {
    let mut document = sample();
    // A mask listed as a child.
    if let Some(NodeKind::Group { children, .. }) = document
        .nodes
        .get_mut(&NodeId(0))
        .map(|node| &mut node.kind)
    {
        children.push(NodeId(3));
    }
    assert!(document.validate().is_err());

    let mut document = sample();
    // A mask whose owner does not point to it.
    if let Some(node) = document.nodes.get_mut(&NodeId(2)) {
        node.mask = None;
    }
    assert!(document.validate().is_err());
}
