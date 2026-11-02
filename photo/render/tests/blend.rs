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

//! The twelve blend modes of CAL-3 on the GPU (docs/decisions/0021): values
//! computed by hand from W3C Compositing and Blending Level 1, never from
//! the legacy code, in sRGB-encoded values (Q1).
//!
//! Colors are multiples of 0.2, exact in 16 bits (65535 = 3 × 5 × 17 × 257,
//! so 0.2 is stored as 13107 exactly). Two colors are crossed on two
//! pixels so that every branch of Overlay, Soft light and ClipColor is
//! reached:
//!
//! - A = (0.2, 0.6, 0.8): Lum(A) = 0.3 × 0.2 + 0.59 × 0.6 + 0.11 × 0.8
//!   = 0.502, Sat(A) = 0.6;
//! - B = (0.6, 1.0, 0.2): Lum(B) = 0.18 + 0.59 + 0.022 = 0.792,
//!   Sat(B) = 0.8.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TOLERANCE_UNITS, assert_close, build_json, compare, export16, setup};
use cygnus_photo_core::BlendMode;
use cygnus_photo_render::Image;

const A: &str = "[0.2, 0.6, 0.8";
const B: &str = "[0.6, 1.0, 0.2";

/// JSON name of a mode, as in the commands.
fn name(mode: BlendMode) -> String {
    serde_json::to_value(mode)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

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

/// A 2 × 1 document: backdrop A on pixel 0 and B on pixel 1, with alpha
/// `backdrop_alpha`; above it a layer B on pixel 0 and A on pixel 1, with
/// alpha `layer_alpha`, opacity `opacity` and mode `mode`.
fn crossed(mode: BlendMode, backdrop_alpha: f32, layer_alpha: f32, opacity: f32) -> String {
    let mode = name(mode);
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 2, "height": 1, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 1, "height": 1}}, "color": {A}, {backdrop_alpha}]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 1, "y": 0, "width": 1, "height": 1}}, "color": {B}, {backdrop_alpha}]}}}},
        {{"command": "layer.create", "params": {{"name": "calque"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 2, "rect": {{"x": 0, "y": 0, "width": 1, "height": 1}}, "color": {B}, {layer_alpha}]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 2, "rect": {{"x": 1, "y": 0, "width": 1, "height": 1}}, "color": {A}, {layer_alpha}]}}}},
        {{"command": "layer.update", "params": {{"id": 2, "opacity": {opacity}, "blend": "{mode}"}}}}
    ]"#
    )
}

/// Opaque backdrop and layer: the result is B(Cb, Cs) itself, rounded to
/// 16 bits (v × 65535).
#[test]
fn every_mode_matches_w3c_values_on_opaque_colors() {
    let Some((_, renderer)) = setup() else { return };
    // (mode, pixel 0 = B(A, B), pixel 1 = B(B, A)), W3C section 10.
    let cases: [(BlendMode, [u16; 3], [u16; 3]); 12] = [
        // B(Cb, Cs) = Cs.
        (
            BlendMode::Normal,
            [39321, 65535, 13107],
            [13107, 39321, 52428],
        ),
        // Cb × Cs: (0.2 × 0.6, 0.6 × 1, 0.8 × 0.2) = (0.12, 0.6, 0.16),
        // commutative. 0.12 × 65535 = 7864.2, 0.16 × 65535 = 10485.6.
        (
            BlendMode::Multiply,
            [7864, 39321, 10486],
            [7864, 39321, 10486],
        ),
        // Cb + Cs − Cb × Cs: (0.8 − 0.12, 1.6 − 0.6, 1.0 − 0.16)
        // = (0.68, 1, 0.84), commutative.
        (
            BlendMode::Screen,
            [44564, 65535, 55049],
            [44564, 65535, 55049],
        ),
        // HardLight(Cs, Cb): Cb ≤ 0.5 -> Cs × 2Cb, else
        // Screen(Cs, 2Cb − 1).
        // Pixel 0: 0.6 × 0.4 = 0.24 ; Screen(1, 0.2) = 1 ;
        //   Screen(0.2, 0.6) = 0.8 − 0.12 = 0.68.
        // Pixel 1: Screen(0.2, 0.2) = 0.4 − 0.04 = 0.36 ; Screen(0.6, 1) = 1 ;
        //   0.8 × 0.4 = 0.32.
        (
            BlendMode::Overlay,
            [15728, 65535, 44564],
            [23593, 65535, 20971],
        ),
        // Cs ≤ 0.5 -> Cb − (1 − 2Cs) × Cb × (1 − Cb), else
        // Cb + (2Cs − 1) × (D(Cb) − Cb), D(Cb) = ((16Cb − 12)Cb + 4)Cb if
        // Cb ≤ 0.25, else sqrt(Cb).
        // Pixel 0: Cb = 0.2, Cs = 0.6: D = ((3.2 − 12) × 0.2 + 4) × 0.2
        //   = 0.448, 0.2 + 0.2 × 0.248 = 0.2496 ;
        //   Cb = 0.6, Cs = 1: 0.6 + (sqrt(0.6) − 0.6) = 0.7745967 ;
        //   Cb = 0.8, Cs = 0.2: 0.8 − 0.6 × 0.8 × 0.2 = 0.704.
        // Pixel 1: Cb = 0.6, Cs = 0.2: 0.6 − 0.6 × 0.6 × 0.4 = 0.456 ;
        //   Cb = 1, Cs = 0.6: 1 + 0.2 × (1 − 1) = 1 ;
        //   Cb = 0.2, Cs = 0.8: 0.2 + 0.6 × (0.448 − 0.2) = 0.3488.
        (
            BlendMode::SoftLight,
            [16358, 50763, 46137],
            [29884, 65535, 22859],
        ),
        // min(Cb, Cs) = (0.2, 0.6, 0.2).
        (
            BlendMode::Darken,
            [13107, 39321, 13107],
            [13107, 39321, 13107],
        ),
        // max(Cb, Cs) = (0.6, 1, 0.8).
        (
            BlendMode::Lighten,
            [39321, 65535, 52428],
            [39321, 65535, 52428],
        ),
        // |Cb − Cs| = (0.4, 0.4, 0.6).
        (
            BlendMode::Difference,
            [26214, 26214, 39321],
            [26214, 26214, 39321],
        ),
        // SetLum(SetSat(Cs, Sat(Cb)), Lum(Cb)).
        // SetSat(C, s) = (C − min) × s / (max − min).
        // Pixel 0: SetSat(B, 0.6) = (0.4, 0.8, 0) × 0.6 / 0.8 = (0.3, 0.6, 0),
        //   Lum = 0.09 + 0.354 = 0.444, d = 0.502 − 0.444 = 0.058 ->
        //   (0.358, 0.658, 0.058), no clipping.
        // Pixel 1: SetSat(A, 0.8) = (0, 0.5333, 0.8), Lum = 0.31467 + 0.088
        //   = 0.40267, d = 0.792 − 0.40267 = 0.38933 ->
        //   (0.38933, 0.92267, 1.18933), max > 1: ClipColor with L = 0.792,
        //   C = L + (C − L) × (1 − L) / (x − L), x − L = 0.39733:
        //   (0.58121, 0.86040, 1).
        (BlendMode::Hue, [23462, 43122, 3801], [38089, 56386, 65535]),
        // SetLum(SetSat(Cb, Sat(Cs)), Lum(Cb)).
        // Pixel 0: SetSat(A, 0.8) = (0, 0.5333, 0.8), Lum = 0.40267,
        //   d = 0.502 − 0.40267 = 0.09933 -> (0.09933, 0.63267, 0.89933).
        // Pixel 1: SetSat(B, 0.6) = (0.3, 0.6, 0), Lum = 0.444,
        //   d = 0.792 − 0.444 = 0.348 -> (0.648, 0.948, 0.348).
        (
            BlendMode::Saturation,
            [6510, 41462, 58938],
            [42467, 62127, 22806],
        ),
        // SetLum(Cs, Lum(Cb)).
        // Pixel 0: B + (0.502 − 0.792) = (0.31, 0.71, −0.09), min < 0:
        //   C = L + (C − L) × L / (L − n), L = 0.502, L − n = 0.592:
        //   (0.33919, 0.67838, 0).
        // Pixel 1: A + (0.792 − 0.502) = (0.49, 0.89, 1.09), max > 1:
        //   L = 0.792, x − L = 0.298: (0.58121, 0.86040, 1).
        (BlendMode::Color, [22229, 44458, 0], [38089, 56386, 65535]),
        // SetLum(Cb, Lum(Cs)): Color with the two pixels swapped.
        (
            BlendMode::Luminosity,
            [38089, 56386, 65535],
            [22229, 44458, 0],
        ),
    ];
    for (mode, first, second) in cases {
        let image = export16(&renderer, &build_json(&crossed(mode, 1.0, 1.0, 1.0)));
        let what = format!("{mode:?}");
        assert_pixel(&image, 0, [first[0], first[1], first[2], 65535], &what);
        assert_pixel(&image, 1, [second[0], second[1], second[2], 65535], &what);
    }
}

/// General formula (W3C, section 5.8, premultiplied form):
/// co = cs × (1 − αb) + cb × (1 − αs) + αs × αb × B(Cb, Cs),
/// αo = αs + αb × (1 − αs).
/// Backdrop alpha 0.6, layer alpha 0.8 at opacity 0.5: αs = 0.4,
/// αo = 0.4 + 0.6 × 0.6 = 0.76. The composite is cached premultiplied in
/// 16 bits (0026), then divided by αo at export: the expected values below
/// round co and αo to 16 bits first (αo -> 49807), then co / αo.
#[test]
fn partial_alphas_follow_the_general_w3c_formula() {
    let Some((_, renderer)) = setup() else { return };
    let cases = [
        // B = (0.12, 0.6, 0.16): co = 0.4 × B × 0.4 + 0.6 × A × 0.6
        //   + 0.24 × B = (0.096 + 0.072 + 0.0288, 0.16 + 0.216 + 0.144,
        //   0.032 + 0.288 + 0.0384) = (0.1968, 0.52, 0.3584).
        (BlendMode::Multiply, [16970, 44839, 30905]),
        // B = (0.58121, 0.86040, 1): co = (0.096 + 0.072 + 0.13949,
        //   0.16 + 0.216 + 0.20650, 0.032 + 0.288 + 0.24)
        //   = (0.30749, 0.58250, 0.56).
        (BlendMode::Luminosity, [26514, 50229, 48289]),
        // B = (0.4, 0.4, 0.6): co = (0.096 + 0.072 + 0.096,
        //   0.16 + 0.216 + 0.096, 0.032 + 0.288 + 0.144)
        //   = (0.264, 0.472, 0.464).
        (BlendMode::Difference, [22764, 40701, 40010]),
    ];
    for (mode, expected) in cases {
        let image = export16(&renderer, &build_json(&crossed(mode, 0.6, 0.8, 0.5)));
        let what = format!("{mode:?}");
        assert_pixel(
            &image,
            0,
            [expected[0], expected[1], expected[2], 49807],
            &what,
        );
    }
}

/// White opaque backdrop; base A (alpha `base_alpha`) on pixel 0 only;
/// layer B clipped to it on both pixels, at opacity `opacity` in `mode`.
fn clipped(mode: BlendMode, base_alpha: f32, opacity: f32) -> String {
    let mode = name(mode);
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 2, "height": 1, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 2, "height": 1}}, "color": [1.0, 1.0, 1.0, 1.0]}}}},
        {{"command": "layer.create", "params": {{"name": "base"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 2, "rect": {{"x": 0, "y": 0, "width": 1, "height": 1}}, "color": {A}, {base_alpha}]}}}},
        {{"command": "layer.create", "params": {{"name": "détouré"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 3, "rect": {{"x": 0, "y": 0, "width": 2, "height": 1}}, "color": {B}, 1.0]}}}},
        {{"command": "layer.update", "params": {{"id": 3, "clip": true, "opacity": {opacity}, "blend": "{mode}"}}}}
    ]"#
    )
}

/// Clipped layers blend with their mode in source-atop (W3C, section 5.8
/// with Fa = αb, Fb = 1 − αs):
/// co = αb × (cs × (1 − αb) + αs × αb × B(Cb, Cs)) + cb × (1 − αs),
/// αo = αb. The set is then composed over white in Normal.
#[test]
fn clipped_layers_blend_atop_with_their_mode() {
    let Some((_, renderer)) = setup() else { return };
    let cases = [
        // Opaque base, αs = 0.6: co = 0.6 × B + 0.4 × A.
        // Multiply: (0.072 + 0.08, 0.36 + 0.24, 0.096 + 0.32)
        //   = (0.152, 0.6, 0.416).
        (BlendMode::Multiply, 1.0, 0.6, [9961, 39321, 27263]),
        // Luminosity: (0.34872 + 0.08, 0.51624 + 0.24, 0.6 + 0.32)
        //   = (0.42872, 0.75624, 0.92).
        (BlendMode::Luminosity, 1.0, 0.6, [28096, 49560, 60292]),
        // Base alpha 0.6, αs = 1: co = 0.6 × (0.4 × Cs + 0.6 × B), then
        // over white: + 0.4.
        // Multiply: 0.6 × (0.24 + 0.072, 0.4 + 0.36, 0.08 + 0.096) + 0.4
        //   = (0.5872, 0.856, 0.5056).
        (BlendMode::Multiply, 0.6, 1.0, [38482, 56098, 33134]),
        // Luminosity: 0.6 × (0.24 + 0.34872, 0.4 + 0.51624, 0.08 + 0.6)
        //   + 0.4 = (0.75323, 0.94974, 0.808).
        (BlendMode::Luminosity, 0.6, 1.0, [49363, 62242, 52952]),
    ];
    for (mode, base_alpha, opacity, expected) in cases {
        let image = export16(&renderer, &build_json(&clipped(mode, base_alpha, opacity)));
        let what = format!("{mode:?}, base alpha {base_alpha}, opacity {opacity}");
        assert_pixel(
            &image,
            0,
            [expected[0], expected[1], expected[2], 65535],
            &what,
        );
        // Outside the base the clipped layer shows nowhere.
        assert_pixel(&image, 1, [65535; 4], &what);
    }
}

/// The mode of a clipping base applies to the whole set: a set whose
/// clipped layer repeats the base color equals the base alone in that
/// mode.
#[test]
fn a_clipping_set_blends_with_the_mode_of_its_base() {
    let Some((_, renderer)) = setup() else { return };
    for mode in BlendMode::ALL {
        let single = crossed(mode, 1.0, 1.0, 1.0);
        let set = single.replacen(
            "\n    ]",
            r#",
        {"command": "layer.create", "params": {"name": "détouré"}},
        {"command": "pixels.fill", "params": {"layer": 3, "rect": {"x": 0, "y": 0, "width": 1, "height": 1}, "color": [0.6, 1.0, 0.2, 1.0]}},
        {"command": "pixels.fill", "params": {"layer": 3, "rect": {"x": 1, "y": 0, "width": 1, "height": 1}, "color": [0.2, 0.6, 0.8, 1.0]}},
        {"command": "layer.update", "params": {"id": 3, "clip": true}}
    ]"#,
            1,
        );
        assert_ne!(set, single, "the script was not extended");
        let expected = export16(&renderer, &build_json(&single));
        let actual = export16(&renderer, &build_json(&set));
        let comparison = compare(&expected, &actual);
        assert!(
            comparison.max_units <= TOLERANCE_UNITS,
            "{mode:?}: {comparison:?}"
        );
    }
}

/// A 300 × 200 document over two tiles: an opaque backdrop on the left,
/// semi-transparent on the right, and a semi-transparent layer at opacity
/// 0.8 in `mode`. With `group`, the layer is inside a group of that
/// isolation, and the mode is set on the group (isolated) or on the layer
/// (pass-through).
fn scene(mode: BlendMode, group: Option<&str>) -> String {
    let mode = name(mode);
    let (create, parent, target) = match group {
        Some(isolation) => (
            format!(
                r#"{{"command": "layer.create", "params": {{"name": "groupe", "kind": {{"type": "group", "isolation": "{isolation}"}}}}}},"#
            ),
            r#", "parent": 2"#,
            if isolation == "isolated" { 2 } else { 3 },
        ),
        None => (String::new(), "", 2),
    };
    let layer = if group.is_some() { 3 } else { 2 };
    format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 300, "height": 200, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 150, "height": 200}}, "color": [0.9, 0.5, 0.2, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 150, "y": 0, "width": 150, "height": 200}}, "color": [0.1, 0.4, 0.7, 0.5]}}}},
        {create}
        {{"command": "layer.create", "params": {{"name": "calque"{parent}}}}},
        {{"command": "pixels.fill", "params": {{"layer": {layer}, "rect": {{"x": 40, "y": 30, "width": 240, "height": 140}}, "color": [0.3, 0.8, 0.6, 0.7]}}}},
        {{"command": "layer.update", "params": {{"id": {target}, "opacity": 0.8, "blend": "{mode}"}}}}
    ]"#
    )
}

/// An isolated group holding one Normal layer, the group in mode M, equals
/// the layer alone in mode M; a pass-through group keeps the mode of the
/// layers inside it.
#[test]
fn groups_blend_like_their_single_layer_in_every_mode() {
    let Some((_, renderer)) = setup() else { return };
    for mode in BlendMode::ALL {
        let flat = export16(&renderer, &build_json(&scene(mode, None)));
        for isolation in ["isolated", "pass_through"] {
            let grouped = export16(&renderer, &build_json(&scene(mode, Some(isolation))));
            let comparison = compare(&flat, &grouped);
            assert!(
                comparison.max_units <= TOLERANCE_UNITS,
                "{mode:?}, {isolation}: {comparison:?}"
            );
        }
    }
}
