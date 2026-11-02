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

//! Nesting limit (docs/decisions/0021): until the filter stages of step B10,
//! the renderer composes at most `MAX_NESTING` levels of groups and
//! clipping sets in one pass. Commands that could go beyond refuse with a
//! clear error and leave the document untouched.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cygnus_commands::Command;
use cygnus_photo_core::commands::*;
use cygnus_photo_core::{CoreError, Depth, Document, Isolation, MAX_NESTING, NodeId, PhotoCommand};

fn apply(document: &mut Document, command: PhotoCommand) -> Result<(), CoreError> {
    command.apply(document).map(|_| ())
}

fn create(name: &str, kind: LayerKind, parent: Option<u64>) -> PhotoCommand {
    PhotoCommand::LayerCreate(LayerCreate {
        name: name.to_owned(),
        parent: parent.map(NodeId),
        index: None,
        kind,
    })
}

fn group() -> LayerKind {
    LayerKind::Group {
        isolation: Isolation::Isolated,
    }
}

/// A document whose groups 1..=depth are nested in a chain (group k + 1
/// inside group k), with a pixel layer `depth + 1` in the innermost one.
fn chain(depth: u64) -> Document {
    let mut document = Document::default();
    apply(
        &mut document,
        PhotoCommand::DocumentCreate(DocumentCreate {
            width: 8,
            height: 8,
            resolution: 72.0,
            depth: Depth::U8,
            profile: Default::default(),
        }),
    )
    .unwrap();
    for k in 1..=depth {
        let parent = (k > 1).then_some(k - 1);
        apply(&mut document, create(&format!("g{k}"), group(), parent)).unwrap();
    }
    let parent = (depth > 0).then_some(depth);
    apply(&mut document, create("p", LayerKind::Pixel, parent)).unwrap();
    document
}

/// Asserts that `command` is refused for nesting, without any effect.
fn assert_refused(mut document: Document, command: PhotoCommand) {
    let before = document.clone();
    let name = command.name();
    let error = apply(&mut document, command).unwrap_err();
    assert!(
        matches!(error, CoreError::TooDeep { .. }),
        "{name}: unexpected error {error}"
    );
    assert!(
        error.to_string().contains("nested"),
        "{name}: unclear message {error}"
    );
    assert_eq!(
        document, before,
        "{name}: a refused command changed the document"
    );
}

#[test]
fn the_limit_matches_the_register_stack_of_the_renderer() {
    assert_eq!(MAX_NESTING, 8);
}

#[test]
fn nesting_depth_counts_groups_and_clipping_sets() {
    assert_eq!(chain(0).nesting_depth(), 0);
    assert_eq!(chain(3).nesting_depth(), 3);
    let mut document = chain(2);
    // A second pixel layer clipped to "p" in group 2: one more level.
    apply(&mut document, create("c", LayerKind::Pixel, Some(2))).unwrap();
    apply(
        &mut document,
        PhotoCommand::LayerUpdate(LayerUpdate {
            id: NodeId(4),
            name: None,
            visible: None,
            opacity: None,
            blend: None,
            locked: None,
            clip: Some(true),
            isolation: None,
        }),
    )
    .unwrap();
    assert_eq!(document.nesting_depth(), 3);
}

#[test]
fn creating_a_group_beyond_the_limit_is_refused() {
    let document = chain(MAX_NESTING as u64);
    // A pixel layer at the bottom of the chain is fine, a group is not.
    let mut ok = document.clone();
    apply(
        &mut ok,
        create("q", LayerKind::Pixel, Some(MAX_NESTING as u64)),
    )
    .unwrap();
    assert_refused(document, create("g", group(), Some(MAX_NESTING as u64)));
}

#[test]
fn grouping_beyond_the_limit_is_refused() {
    let document = chain(MAX_NESTING as u64);
    // Grouping the pixel layer of the innermost group adds a ninth level.
    let command = PhotoCommand::LayerGroup(LayerGroup {
        ids: vec![NodeId(MAX_NESTING as u64 + 1)],
        name: "trop".to_owned(),
        isolation: Isolation::PassThrough,
    });
    assert_refused(document.clone(), command.clone());
    // One level less is accepted.
    let mut shallower = chain(MAX_NESTING as u64 - 1);
    let command = PhotoCommand::LayerGroup(LayerGroup {
        ids: vec![NodeId(MAX_NESTING as u64)],
        name: "juste".to_owned(),
        isolation: Isolation::PassThrough,
    });
    apply(&mut shallower, command).unwrap();
    assert_eq!(shallower.nesting_depth(), MAX_NESTING);
}

#[test]
fn moving_beyond_the_limit_is_refused() {
    // Chain of 7, plus a group 9 holding a group 10 at the root: moving
    // group 9 (two levels) into group 7 makes nine.
    let mut document = chain(7);
    apply(&mut document, create("a", group(), None)).unwrap();
    apply(&mut document, create("b", group(), Some(9))).unwrap();
    assert_eq!(document.nesting_depth(), 7);
    let command = PhotoCommand::LayerMove(LayerMove {
        id: NodeId(9),
        parent: Some(NodeId(7)),
        index: 0,
    });
    assert_refused(document.clone(), command);
    // Into group 6 it makes eight: accepted.
    let command = PhotoCommand::LayerMove(LayerMove {
        id: NodeId(9),
        parent: Some(NodeId(6)),
        index: 0,
    });
    apply(&mut document, command).unwrap();
    assert_eq!(document.nesting_depth(), MAX_NESTING);
}

#[test]
fn clipping_beyond_the_limit_is_refused() {
    // Chain of 8; a second layer clipped to "p" in group 8 adds a level.
    let mut document = chain(MAX_NESTING as u64);
    apply(
        &mut document,
        create("c", LayerKind::Pixel, Some(MAX_NESTING as u64)),
    )
    .unwrap();
    let command = PhotoCommand::LayerUpdate(LayerUpdate {
        id: NodeId(MAX_NESTING as u64 + 2),
        name: None,
        visible: None,
        opacity: None,
        blend: None,
        locked: None,
        clip: Some(true),
        isolation: None,
    });
    assert_refused(document, command);
}

#[test]
fn duplicating_at_the_limit_keeps_the_depth() {
    // A copy goes next to its original: duplication never adds a level,
    // and is accepted at the limit.
    let mut document = chain(MAX_NESTING as u64);
    let command = PhotoCommand::LayerDuplicate(LayerDuplicate {
        id: NodeId(MAX_NESTING as u64),
    });
    apply(&mut document, command).unwrap();
    assert_eq!(document.nesting_depth(), MAX_NESTING);
}

#[test]
fn a_document_nested_too_deep_stays_valid_and_editable() {
    // A document read from elsewhere (file, older version) may be nested
    // deeper: it opens (validation passes), the renderer reports the limit,
    // and ungrouping brings it back within it.
    let mut document = chain(MAX_NESTING as u64);
    let inner = NodeId(MAX_NESTING as u64 + 1);
    if let Some(node) = document.nodes.get_mut(&inner) {
        node.kind = cygnus_photo_core::NodeKind::Group {
            children: Vec::new(),
            isolation: Isolation::Isolated,
        };
    }
    assert_eq!(document.nesting_depth(), MAX_NESTING + 1);
    document.validate().unwrap();
    apply(
        &mut document,
        PhotoCommand::LayerUngroup(LayerUngroup { id: NodeId(1) }),
    )
    .unwrap();
    assert_eq!(document.nesting_depth(), MAX_NESTING);
}
