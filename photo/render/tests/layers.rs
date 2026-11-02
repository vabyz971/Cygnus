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

//! Groups, masks and clipping on the GPU (docs/decisions/0021): properties
//! (a group at full opacity equals its layers flattened) and values
//! computed by hand, in sRGB-encoded premultiplied values (Q1), stored in
//! the 16-bit integer cache (0026).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TOLERANCE_UNITS, assert_close, build_json, compare, export16, setup};
use cygnus_photo_core::{Isolation, MAX_NESTING, NodeId, NodeKind};
use cygnus_photo_render::{Image, RenderError};

/// Asserts that two exports of one adapter differ by at most
/// [`TOLERANCE_UNITS`] per channel (f32 rounding of an equivalent but
/// reordered computation).
fn assert_equivalent(a: &Image, b: &Image, what: &str) {
    let comparison = compare(a, b);
    assert!(
        comparison.max_units <= TOLERANCE_UNITS,
        "{what}: {comparison:?}"
    );
}

/// A 300 × 200 document (two tiles wide) with an opaque backdrop and two
/// overlapping semi-transparent layers, flat or inside a group of the given
/// isolation (`None`: flat).
fn three_layers(isolation: Option<&str>) -> String {
    let (group, parent) = match isolation {
        Some(isolation) => (
            format!(
                r#"{{"command": "layer.create", "params": {{"name": "groupe", "kind": {{"type": "group", "isolation": "{isolation}"}}}}}},"#
            ),
            r#", "parent": 2"#,
        ),
        None => (String::new(), ""),
    };
    // Layer ids: 1 backdrop, then 2 the group if any, then the two layers.
    let first = if isolation.is_some() { 3 } else { 2 };
    let second = first + 1;
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 300, "height": 200, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 300, "height": 200}}, "color": [0.9, 0.85, 0.7, 1.0]}}}},
        {group}
        {{"command": "layer.create", "params": {{"name": "a"{parent}}}}},
        {{"command": "pixels.fill", "params": {{"layer": {first}, "rect": {{"x": 20, "y": 20, "width": 250, "height": 120}}, "color": [0.8, 0.1, 0.2, 0.7]}}}},
        {{"command": "layer.update", "params": {{"id": {first}, "opacity": 0.8}}}},
        {{"command": "layer.create", "params": {{"name": "b"{parent}}}}},
        {{"command": "pixels.fill", "params": {{"layer": {second}, "rect": {{"x": 150, "y": 60, "width": 140, "height": 130}}, "color": [0.1, 0.3, 0.9, 0.6]}}}}
    ]"#
    )
}

#[test]
fn a_group_at_full_opacity_equals_its_layers_flattened() {
    let Some((_, renderer)) = setup() else { return };
    let flat = export16(&renderer, &build_json(&three_layers(None)));
    for isolation in ["isolated", "pass_through"] {
        let grouped = export16(&renderer, &build_json(&three_layers(Some(isolation))));
        assert_equivalent(&flat, &grouped, isolation);
    }
}

/// Opaque white backdrop (layer 1), then a group (2) holding opaque red
/// (3), with the group at opacity 0.25.
fn red_in_group(isolation: &str, extra: &str) -> String {
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 2, "height": 1, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 2, "height": 1}}, "color": [1.0, 1.0, 1.0, 1.0]}}}},
        {{"command": "layer.create", "params": {{"name": "groupe", "kind": {{"type": "group", "isolation": "{isolation}"}}}}}},
        {{"command": "layer.create", "params": {{"name": "rouge", "parent": 2}}}},
        {{"command": "pixels.fill", "params": {{"layer": 3, "rect": {{"x": 0, "y": 0, "width": 1, "height": 1}}, "color": [1.0, 0.0, 0.0, 1.0]}}}},
        {{"command": "layer.update", "params": {{"id": 2, "opacity": 0.25}}}}
        {extra}
    ]"#
    )
}

#[test]
fn group_opacity_matches_hand_computed_values_in_both_isolations() {
    let Some((_, renderer)) = setup() else { return };
    for isolation in ["isolated", "pass_through"] {
        let image = export16(&renderer, &build_json(&red_in_group(isolation, "")));
        // Isolated: the group (red, alpha 1) at opacity 0.25 over white:
        //   co = (0.25, 0, 0) + (1, 1, 1) × (1 − 0.25) = (1, 0.75, 0.75)
        // Pass-through: mix(white, red over white, 0.25), the same in
        // Normal mode: (1, 0.75, 0.75). 0.75 × 65535 = 49151.25 -> 49151.
        let p = image.pixel(0, 0);
        let what = format!("{isolation}, pixel 0");
        assert_close(p[0], 65535, &what);
        assert_close(p[1], 49151, &what);
        assert_close(p[2], 49151, &what);
        assert_close(p[3], 65535, &what);
        // Pixel 1: nothing in the group, the backdrop stays white.
        assert_eq!(image.pixel(1, 0), [65535; 4], "{isolation}, pixel 1");
    }
}

#[test]
fn hidden_empty_and_masked_out_groups_have_no_effect() {
    let Some((_, renderer)) = setup() else { return };
    let backdrop = export16(
        &renderer,
        &build_json(
            r#"[
            {"command": "document.create", "params": {"width": 2, "height": 1, "depth": "u16"}},
            {"command": "layer.create", "params": {"name": "fond"}},
            {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 2, "height": 1}, "color": [1.0, 1.0, 1.0, 1.0]}}
        ]"#,
        ),
    );
    for isolation in ["isolated", "pass_through"] {
        for (what, extra) in [
            (
                "hidden",
                r#", {"command": "layer.update", "params": {"id": 2, "visible": false}}"#,
            ),
            (
                "child hidden",
                r#", {"command": "layer.update", "params": {"id": 3, "visible": false}}"#,
            ),
            (
                "black mask",
                r#", {"command": "mask.create", "params": {"node": 2, "fill": "black"}}"#,
            ),
            (
                "opacity 0",
                r#", {"command": "layer.update", "params": {"id": 2, "opacity": 0.0}}"#,
            ),
        ] {
            let image = export16(&renderer, &build_json(&red_in_group(isolation, extra)));
            assert!(image == backdrop, "{isolation}, {what}");
        }
    }
}

/// White backdrop (layer 1), then one opaque red layer per case, each on
/// its own pixel and with its own mask: layer 2 + 2k, mask 3 + 2k.
fn masked_cases(cases: &[&str]) -> String {
    let mut commands = vec![
        format!(
            r#"{{"command": "document.create", "params": {{"width": {}, "height": 1, "depth": "u16"}}}}"#,
            cases.len()
        ),
        r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
        format!(
            r#"{{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": {}, "height": 1}}, "color": [1.0, 1.0, 1.0, 1.0]}}}}"#,
            cases.len()
        ),
    ];
    for (k, case) in cases.iter().enumerate() {
        let layer = 2 + 2 * k;
        let mask = layer + 1;
        commands.push(format!(
            r#"{{"command": "layer.create", "params": {{"name": "cas {k}"}}}}"#
        ));
        commands.push(format!(
            r#"{{"command": "pixels.fill", "params": {{"layer": {layer}, "rect": {{"x": {k}, "y": 0, "width": 1, "height": 1}}, "color": [1.0, 0.0, 0.0, 1.0]}}}}"#
        ));
        commands.push(
            case.replace("LAYER", &layer.to_string())
                .replace("MASK", &mask.to_string()),
        );
    }
    format!("[{}]", commands.join(",\n"))
}

#[test]
fn masks_match_hand_computed_values() {
    let Some((_, renderer)) = setup() else { return };
    let gray = r#"{"command": "mask.create", "params": {"node": LAYER}},
        {"command": "mask.fill", "params": {"mask": MASK, "rect": {"x": 0, "y": 0, "width": 8, "height": 1}, "value": 0.5}}"#;
    let cases = [
        // 0: gray mask 0.5, stored round(0.5 × 65535) = 32768: m = 32768 / 65535,
        //    red at alpha m over white: G = B = 1 × (1 − m) = 32767 / 65535.
        gray.to_owned(),
        // 1: the same, inverted: m = 32767 / 65535, G = B = 32768.
        format!(
            r#"{gray}, {{"command": "mask.update", "params": {{"id": MASK, "inverted": true}}}}"#
        ),
        // 2: black mask at density 0.25: m = 1 − 0.25 × (1 − 0) = 0.75,
        //    G = B = 0.25 × 65535 = 16383.75 -> 16384.
        r#"{"command": "mask.create", "params": {"node": LAYER, "fill": "black"}},
           {"command": "mask.update", "params": {"id": MASK, "density": 0.25}}"#
            .to_owned(),
        // 3: black mask, disabled: no effect, pure red.
        r#"{"command": "mask.create", "params": {"node": LAYER, "fill": "black"}},
           {"command": "mask.update", "params": {"id": MASK, "enabled": false}}"#
            .to_owned(),
        // 4: black mask without tiles (its empty value): the layer is hidden.
        r#"{"command": "mask.create", "params": {"node": LAYER, "fill": "black"}}"#.to_owned(),
        // 5: white mask: no effect, pure red.
        r#"{"command": "mask.create", "params": {"node": LAYER}}"#.to_owned(),
    ];
    let cases: Vec<&str> = cases.iter().map(String::as_str).collect();
    let image = export16(&renderer, &build_json(&masked_cases(&cases)));
    let expected = [
        [65535, 32767, 32767, 65535],
        [65535, 32768, 32768, 65535],
        [65535, 16384, 16384, 65535],
        [65535, 0, 0, 65535],
        [65535, 65535, 65535, 65535],
        [65535, 0, 0, 65535],
    ];
    for (k, pixel) in expected.iter().enumerate() {
        let actual = image.pixel(k as u32, 0);
        for c in 0..4 {
            assert_close(actual[c], pixel[c], &format!("mask case {k}, channel {c}"));
        }
    }
}

/// 4 × 1, white backdrop (1); base layer (2) on pixels 0 and 1; layer 3
/// clipped to it, covering all four pixels. `base` and `clipped` are the
/// fill colors; `extra` adds commands.
fn clipping(base: &str, clipped: &str, extra: &str) -> String {
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 4, "height": 1, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 4, "height": 1}}, "color": [1.0, 1.0, 1.0, 1.0]}}}},
        {{"command": "layer.create", "params": {{"name": "base"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 2, "rect": {{"x": 0, "y": 0, "width": 2, "height": 1}}, "color": {base}}}}},
        {{"command": "layer.create", "params": {{"name": "détouré"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 3, "rect": {{"x": 0, "y": 0, "width": 4, "height": 1}}, "color": {clipped}}}}},
        {{"command": "layer.update", "params": {{"id": 3, "clip": true}}}}
        {extra}
    ]"#
    )
}

const BLUE: &str = "[0.0, 0.0, 1.0, 1.0]";
const RED: &str = "[1.0, 0.0, 0.0, 1.0]";

fn assert_pixel(image: &Image, x: u32, expected: [u16; 4], what: &str) {
    let actual = image.pixel(x, 0);
    for c in 0..4 {
        assert_close(
            actual[c],
            expected[c],
            &format!("{what}, pixel {x}, channel {c}"),
        );
    }
}

#[test]
fn clipping_matches_hand_computed_values() {
    let Some((_, renderer)) = setup() else { return };
    const WHITE: [u16; 4] = [65535; 4];

    // Opaque red clipped to opaque blue: red where the base is, white
    // elsewhere (source-atop: αo = αb).
    let image = export16(&renderer, &build_json(&clipping(BLUE, RED, "")));
    assert_pixel(&image, 0, [65535, 0, 0, 65535], "red on blue");
    assert_pixel(&image, 1, [65535, 0, 0, 65535], "red on blue");
    assert_pixel(&image, 2, WHITE, "outside the base");
    assert_pixel(&image, 3, WHITE, "outside the base");

    // Base opacity 0.25 applies to the whole clipping set: red at 0.25
    // over white, G = B = 0.75 × 65535 = 49151.25 -> 49151.
    let image = export16(
        &renderer,
        &build_json(&clipping(
            BLUE,
            RED,
            r#", {"command": "layer.update", "params": {"id": 2, "opacity": 0.25}}"#,
        )),
    );
    assert_pixel(&image, 0, [65535, 49151, 49151, 65535], "base opacity");
    assert_pixel(&image, 2, WHITE, "base opacity, outside");

    // Clipped layer at opacity 0.25 over the opaque blue base, atop:
    //   co = (0.25, 0, 0) × 1 + (0, 0, 1) × (1 − 0.25) = (0.25, 0, 0.75)
    //   0.25 × 65535 = 16383.75 -> 16384 ; 0.75 × 65535 -> 49151.
    let image = export16(
        &renderer,
        &build_json(&clipping(
            BLUE,
            RED,
            r#", {"command": "layer.update", "params": {"id": 3, "opacity": 0.25}}"#,
        )),
    );
    assert_pixel(&image, 0, [16384, 0, 49151, 65535], "clipped opacity");

    // Semi-transparent base (alpha 0.5, stored 32768, αb = 32768 / 65535):
    // atop keeps αb, color red: red at αb over white,
    //   G = B = 1 × (1 − αb) = 32767 / 65535 -> 32767.
    let image = export16(
        &renderer,
        &build_json(&clipping("[0.0, 0.0, 1.0, 0.5]", RED, "")),
    );
    assert_pixel(&image, 0, [65535, 32767, 32767, 65535], "half base");

    // A hidden base hides its clipped layers.
    let image = export16(
        &renderer,
        &build_json(&clipping(
            BLUE,
            RED,
            r#", {"command": "layer.update", "params": {"id": 2, "visible": false}}"#,
        )),
    );
    for x in 0..4 {
        assert_pixel(&image, x, WHITE, "hidden base");
    }

    // A black mask on the base clips the clipped layer too.
    let image = export16(
        &renderer,
        &build_json(&clipping(
            BLUE,
            RED,
            r#", {"command": "mask.create", "params": {"node": 2, "fill": "black"}}"#,
        )),
    );
    for x in 0..4 {
        assert_pixel(&image, x, WHITE, "masked base");
    }
}

#[test]
fn a_group_can_be_a_clipping_base_and_be_clipped() {
    let Some((_, renderer)) = setup() else { return };
    // Base: a pass-through group (2) holding the blue layer (3) on pixels 0
    // and 1. Clipped: a group (4) holding red (5) on all pixels. A
    // clipping base composes as an isolated group.
    let image = export16(
        &renderer,
        &build_json(
            r#"[
            {"command": "document.create", "params": {"width": 4, "height": 1, "depth": "u16"}},
            {"command": "layer.create", "params": {"name": "fond"}},
            {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 4, "height": 1}, "color": [1.0, 1.0, 1.0, 1.0]}},
            {"command": "layer.create", "params": {"name": "base", "kind": {"type": "group"}}},
            {"command": "layer.create", "params": {"name": "bleu", "parent": 2}},
            {"command": "pixels.fill", "params": {"layer": 3, "rect": {"x": 0, "y": 0, "width": 2, "height": 1}, "color": [0.0, 0.0, 1.0, 1.0]}},
            {"command": "layer.create", "params": {"name": "détouré", "kind": {"type": "group"}}},
            {"command": "layer.create", "params": {"name": "rouge", "parent": 4}},
            {"command": "pixels.fill", "params": {"layer": 5, "rect": {"x": 0, "y": 0, "width": 4, "height": 1}, "color": [1.0, 0.0, 0.0, 1.0]}},
            {"command": "layer.update", "params": {"id": 4, "clip": true}}
        ]"#,
        ),
    );
    assert_pixel(&image, 0, [65535, 0, 0, 65535], "group on group");
    assert_pixel(&image, 3, [65535; 4], "group on group, outside");
}

#[test]
fn a_document_nested_beyond_the_register_stack_is_reported() {
    let Some((_, renderer)) = setup() else { return };
    // Commands refuse to nest deeper than MAX_NESTING; a document read from
    // elsewhere may still be: built here by hand. Eight nested groups, then
    // the pixel layer at the bottom of the chain turned into a ninth group
    // holding a new layer.
    let mut commands = vec![
        r#"{"command": "document.create", "params": {"width": 2, "height": 2, "depth": "u8"}}"#
            .to_owned(),
    ];
    for depth in 0..MAX_NESTING as u64 {
        let parent = if depth == 0 {
            String::new()
        } else {
            format!(r#", "parent": {depth}"#)
        };
        commands.push(format!(
            r#"{{"command": "layer.create", "params": {{"name": "g{depth}", "kind": {{"type": "group", "isolation": "isolated"}}{parent}}}}}"#
        ));
    }
    let last = MAX_NESTING as u64;
    commands.push(format!(
        r#"{{"command": "layer.create", "params": {{"name": "p", "parent": {last}}}}}"#
    ));
    commands.push(format!(
        r#"{{"command": "pixels.fill", "params": {{"layer": {}, "rect": {{"x": 0, "y": 0, "width": 2, "height": 2}}, "color": [1.0, 0.0, 0.0, 1.0]}}}}"#,
        last + 1
    ));
    let mut document = build_json(&format!("[{}]", commands.join(",\n")));
    // Wrap the pixel layer in a ninth group, by hand.
    let pixel = NodeId(last + 1);
    let wrapper = NodeId(document.next_id);
    document.next_id += 1;
    let mut node = document.node(pixel).unwrap().clone();
    node.id = wrapper;
    node.name = "g9".to_owned();
    node.kind = NodeKind::Group {
        children: vec![pixel],
        isolation: Isolation::Isolated,
    };
    if let Some(NodeKind::Group { children, .. }) =
        document.nodes.get_mut(&NodeId(last)).map(|n| &mut n.kind)
    {
        *children = vec![wrapper];
    }
    document.nodes.get_mut(&pixel).unwrap().parent = Some(wrapper);
    document.nodes.insert(wrapper, node);
    document.validate().unwrap();
    assert_eq!(document.nesting_depth(), MAX_NESTING + 1);
    let error = renderer
        .export(&document, cygnus_photo_render::ExportBits::Eight)
        .unwrap_err();
    assert!(
        matches!(error, RenderError::Unsupported(message) if message.contains("nested")),
        "{error}"
    );
}
