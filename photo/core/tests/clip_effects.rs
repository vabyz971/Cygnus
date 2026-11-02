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

//! Clipping onto adjustment and filter layers (docs/decisions/0028, Q1):
//! they have no pixels, so nothing can be clipped onto them; every command
//! that would make one the base of a clipped node is refused with a clear
//! error and leaves the document untouched. An adjustment or a filter may
//! itself be clipped onto a pixel layer.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cygnus_commands::Command;
use cygnus_photo_core::{CoreError, Document, NodeId, PhotoCommand};

fn command(json: &str) -> PhotoCommand {
    serde_json::from_str(json).unwrap_or_else(|error| panic!("{json}: {error}"))
}

fn apply(document: &mut Document, json: &str) -> Result<(), CoreError> {
    command(json).apply(document).map(|_| ())
}

fn build(commands: &[&str]) -> Document {
    let mut document = Document::default();
    apply(
        &mut document,
        r#"{"command": "document.create", "params": {"width": 8, "height": 8, "depth": "u8"}}"#,
    )
    .unwrap();
    for json in commands {
        apply(&mut document, json).unwrap();
    }
    document
}

const PIXEL: &str = r#"{"command": "layer.create", "params": {"name": "p"}}"#;
const CURVES: &str = r#"{"command": "layer.create", "params": {"name": "r", "kind": {"type": "adjustment", "adjustment": {"kind": "curves", "master": {"points": [[0, 0], [1, 1]]}, "red": {"points": [[0, 0], [1, 1]]}, "green": {"points": [[0, 0], [1, 1]]}, "blue": {"points": [[0, 0], [1, 1]]}}}}}"#;
const BLUR: &str = r#"{"command": "layer.create", "params": {"name": "f", "kind": {"type": "filter", "filter": {"kind": "gaussian_blur", "radius": 2.0}}}}"#;

fn clip(id: u64) -> String {
    format!(r#"{{"command": "layer.update", "params": {{"id": {id}, "clip": true}}}}"#)
}

fn assert_refused(mut document: Document, json: &str) {
    let before = document.clone();
    let error = apply(&mut document, json).unwrap_err();
    assert!(
        matches!(error, CoreError::ClipOntoEffect { .. }),
        "{json}: unexpected error {error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("cannot be clipped onto an adjustment or filter layer"),
        "{json}: unclear message {message}"
    );
    assert_eq!(
        document, before,
        "{json}: a refused command changed the document"
    );
}

#[test]
fn clipping_a_layer_onto_an_adjustment_or_a_filter_is_refused() {
    for effect in [CURVES, BLUR] {
        // 1 effect, 2 pixel layer above it.
        let document = build(&[effect, PIXEL]);
        assert_refused(document, &clip(2));
    }
}

#[test]
fn an_adjustment_or_a_filter_may_be_clipped_onto_a_pixel_layer() {
    for effect in [CURVES, BLUR] {
        let mut document = build(&[PIXEL, effect]);
        apply(&mut document, &clip(2)).unwrap();
        assert!(document.node(NodeId(2)).unwrap().clip);
    }
}

#[test]
fn a_second_clipped_layer_above_a_clipped_effect_keeps_the_pixel_base() {
    // 1 pixel base, 2 curves clipped onto it, 3 pixel clipped too: the
    // base of the set is still the pixel layer.
    let mut document = build(&[PIXEL, CURVES, PIXEL]);
    apply(&mut document, &clip(2)).unwrap();
    apply(&mut document, &clip(3)).unwrap();
}

#[test]
fn creating_an_effect_under_a_clipped_layer_is_refused() {
    // 1 pixel base, 2 pixel clipped onto it; a curves layer inserted at
    // index 1 would become the base of 2.
    let mut document = build(&[PIXEL, PIXEL]);
    apply(&mut document, &clip(2)).unwrap();
    assert_refused(
        document,
        r#"{"command": "layer.create", "params": {"name": "r", "index": 1, "kind": {"type": "adjustment", "adjustment": {"kind": "brightness_contrast", "brightness": 10, "contrast": 0}}}}"#,
    );
}

#[test]
fn deleting_or_moving_the_pixel_base_onto_an_effect_is_refused() {
    // 1 curves, 2 pixel base, 3 pixel clipped onto 2.
    let mut document = build(&[CURVES, PIXEL, PIXEL]);
    apply(&mut document, &clip(3)).unwrap();
    // Without 2, layer 3 would clip onto the curves layer.
    assert_refused(
        document.clone(),
        r#"{"command": "layer.delete", "params": {"id": 2}}"#,
    );
    // Moving 2 to the bottom leaves 3 above the curves layer.
    assert_refused(
        document.clone(),
        r#"{"command": "layer.move", "params": {"id": 2, "index": 0}}"#,
    );
    // Moving the curves layer between the base and the clipped layer.
    assert_refused(
        document,
        r#"{"command": "layer.move", "params": {"id": 1, "index": 1}}"#,
    );
}

#[test]
fn duplicating_an_effect_under_a_clipped_layer_is_refused() {
    // 1 pixel, 2 curves clipped onto 1, 3 pixel clipped onto 1: a copy of
    // the clipped curves layer is clipped too, so it is allowed; a copy of
    // an unclipped effect placed under a clipped layer is not.
    let mut document = build(&[PIXEL, CURVES, PIXEL]);
    apply(&mut document, &clip(2)).unwrap();
    apply(&mut document, &clip(3)).unwrap();
    apply(
        &mut document,
        r#"{"command": "layer.duplicate", "params": {"id": 2}}"#,
    )
    .unwrap();

    // 1 pixel, 2 blur, 3 pixel clipped onto... nothing allowed: build a
    // set whose base is the top of a group, then ungroup it under a
    // clipped layer.
    let mut document = build(&[
        r#"{"command": "layer.create", "params": {"name": "g", "kind": {"type": "group", "isolation": "isolated"}}}"#,
        r#"{"command": "layer.create", "params": {"name": "f", "parent": 1, "kind": {"type": "filter", "filter": {"kind": "gaussian_blur", "radius": 2.0}}}}"#,
        PIXEL,
    ]);
    apply(&mut document, &clip(3)).unwrap();
    // 3 clips onto the group 1; ungrouping puts the blur 2 under it.
    assert_refused(
        document,
        r#"{"command": "layer.ungroup", "params": {"id": 1}}"#,
    );
}
