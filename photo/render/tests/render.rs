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

//! Renderer behaviour on the GPU: properties of the Normal blend, values
//! computed by hand from W3C Compositing and Blending Level 1, tiling.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{assert_close, build_json, export16, setup};
use cygnus_photo_render::ExportBits;

/// 600 × 400, 16 bits: a horizontal gradient layer (written row 0 then
/// filled) and a second layer whose commands are given by `top`.
fn with_top(top: &str) -> String {
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 600, "height": 400, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "bottom"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 300, "height": 400}}, "color": [0.1, 0.6, 0.3, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 300, "y": 0, "width": 300, "height": 400}}, "color": [0.9, 0.2, 0.7, 0.5]}}}},
        {{"command": "layer.create", "params": {{"name": "top"}}}}
        {top}
    ]"#
    )
}

#[test]
fn opaque_layer_covers_what_is_below() {
    let Some((_, renderer)) = setup() else { return };
    let document = build_json(&with_top(
        r#", {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 600, "height": 400}, "color": [0.0, 0.0, 1.0, 1.0]}}"#,
    ));
    let image = export16(&renderer, &document);
    for (x, y) in [(0, 0), (299, 10), (300, 399), (599, 399)] {
        assert_eq!(image.pixel(x, y), [0, 0, 65535, 65535], "pixel ({x}, {y})");
    }
}

#[test]
fn zero_opacity_hidden_and_empty_layers_have_no_effect() {
    let Some((_, renderer)) = setup() else { return };
    let reference = export16(&renderer, &build_json(&with_top("")));
    let cases = [
        // Opaque red at opacity 0.
        r#", {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 600, "height": 400}, "color": [1.0, 0.0, 0.0, 1.0]}},
           {"command": "layer.update", "params": {"id": 2, "opacity": 0.0}}"#,
        // Opaque red, hidden.
        r#", {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 600, "height": 400}, "color": [1.0, 0.0, 0.0, 1.0]}},
           {"command": "layer.update", "params": {"id": 2, "visible": false}}"#,
        // Fully transparent pixels.
        r#", {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 600, "height": 400}, "color": [1.0, 0.0, 0.0, 0.0]}}"#,
    ];
    for top in cases {
        let image = export16(&renderer, &build_json(&with_top(top)));
        // Same adapter: bit-identical, no tolerance.
        assert!(image == reference, "case changed the image: {top}");
    }
}

#[test]
fn normal_blend_matches_hand_computed_values() {
    let Some((_, renderer)) = setup() else { return };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "backdrop"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 1, "height": 1}, "color": [1.0, 1.0, 1.0, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 2, "y": 0, "width": 1, "height": 1}, "color": [0.0, 0.0, 0.0, 1.0]}},
        {"command": "layer.create", "params": {"name": "red"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 0, "width": 2, "height": 1}, "color": [1.0, 0.0, 0.0, 0.5]}},
        {"command": "layer.create", "params": {"name": "gray"}},
        {"command": "pixels.fill", "params": {"layer": 3, "rect": {"x": 2, "y": 0, "width": 1, "height": 1}, "color": [0.5, 0.5, 0.5, 1.0]}},
        {"command": "layer.update", "params": {"id": 3, "opacity": 0.5}}
    ]"#,
    );
    let image = export16(&renderer, &document);

    // Source-over, W3C section 9.1.4 (simple alpha compositing):
    //   co = cs × αs + cb × αb × (1 − αs),  αo = αs + αb × (1 − αs),
    // applied to sRGB-encoded values (docs/decisions/0021, Q1), stored in
    // the 16-bit integer cache (0026) and quantized to 16 bits.
    //
    // A fill alpha of 0.5 is stored as round(0.5 × 65535) = 32768, so
    // αs = 32768 / 65535.

    // Pixel 0: red (1, 0, 0), αs = 32768 / 65535, over opaque white.
    //   R: 1 × αs + 1 × 1 × (1 − αs) = 1                 -> 65535
    //   G, B: 0 × αs + 1 × 1 × (1 − αs) = 32767 / 65535  -> 32767
    //   α: αs + 1 × (1 − αs) = 1                         -> 65535
    let p = image.pixel(0, 0);
    assert_close(p[0], 65535, "red over white, R");
    assert_close(p[1], 32767, "red over white, G");
    assert_close(p[2], 32767, "red over white, B");
    assert_close(p[3], 65535, "red over white, alpha");

    // Pixel 1: the same red over a transparent backdrop (αb = 0):
    //   αo = αs -> 32768 ; straight color co / αo = (1, 0, 0).
    let p = image.pixel(1, 0);
    assert_close(p[0], 65535, "red over nothing, R");
    assert_close(p[1], 0, "red over nothing, G");
    assert_close(p[3], 32768, "red over nothing, alpha");

    // Pixel 2: gray sRGB 0.5 (stored 32768), opacity 0.5, over opaque
    //   black: αs = 0.5,
    //   co = 32768 / 65535 × 0.5 + 0 = 16384 / 65535      -> 16384
    //   αo = 0.5 + 1 × 0.5 = 1                            -> 65535
    let p = image.pixel(2, 0);
    for (channel, value) in p.iter().take(3).enumerate() {
        assert_close(
            *value,
            16384,
            &format!("half gray over black, channel {channel}"),
        );
    }
    assert_close(p[3], 65535, "half gray over black, alpha");
}

#[test]
fn partial_edge_tiles_are_cropped() {
    let Some((_, renderer)) = setup() else { return };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 300, "height": 200, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "corner"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 290, "y": 190, "width": 50, "height": 50}, "color": [1.0, 1.0, 0.0, 1.0]}}
    ]"#,
    );
    let image = export16(&renderer, &document);
    assert_eq!((image.width, image.height), (300, 200));
    assert_eq!(image.data.len(), 300 * 200 * 4);
    assert_eq!(image.pixel(299, 199), [65535, 65535, 0, 65535]);
    assert_eq!(image.pixel(289, 199), [0; 4]);
    assert_eq!(image.pixel(0, 0), [0; 4]);
}

#[test]
fn eight_bit_export() {
    let Some((_, renderer)) = setup() else { return };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 2, "height": 1, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "a"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 1, "height": 1}, "color": [1.0, 0.0, 1.0, 1.0]}}
    ]"#,
    );
    let image = renderer.export(&document, ExportBits::Eight).unwrap();
    assert_eq!(image.pixel(0, 0), [255, 0, 255, 255]);
    assert_eq!(image.pixel(1, 0), [0; 4]);
}

#[test]
fn large_documents_are_rendered_in_batches() {
    let Some((_, renderer)) = setup() else { return };
    // 16 × 9 = 144 output tiles: more than one batch of composites.
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 4096, "height": 2304, "depth": "u8"}},
        {"command": "layer.create", "params": {"name": "left"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 2048, "height": 2304}, "color": [1.0, 0.0, 0.0, 1.0]}},
        {"command": "layer.create", "params": {"name": "bottom"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 0, "y": 1152, "width": 4096, "height": 1152}, "color": [0.0, 0.0, 1.0, 1.0]}}
    ]"#,
    );
    let image = renderer.export(&document, ExportBits::Eight).unwrap();
    assert_eq!(image.pixel(0, 0), [255, 0, 0, 255]);
    assert_eq!(image.pixel(4095, 0), [0; 4]);
    assert_eq!(image.pixel(0, 2303), [0, 0, 255, 255]);
    assert_eq!(image.pixel(4095, 2303), [0, 0, 255, 255]);
    assert_eq!(image.pixel(2047, 1151), [255, 0, 0, 255]);
}

#[test]
fn document_without_pixels_exports_transparent() {
    let Some((_, renderer)) = setup() else { return };
    let document = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 20, "height": 10, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "vide"}}
    ]"#,
    );
    let image = export16(&renderer, &document);
    assert!(image.data.iter().all(|value| *value == 0));
}
