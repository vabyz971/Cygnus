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

//! Adjustment layers on the GPU (REG-1, docs/decisions/0028): values
//! computed by hand from the formulas of 0028, on sRGB-encoded straight
//! colors (0021, Q5).
//!
//! Most cases use three opaque pixels whose colors are exact in 16 bits
//! (multiples of 0.2, as in the blend tests):
//!
//! - A = (0.2, 0.6, 0.8): HSL (200°, 0.6, 0.5), Lum = 0.502;
//! - B = (0.6, 1.0, 0.2): HSL (90°, 1, 0.6), Lum = 0.792;
//! - G = (0.4, 0.4, 0.4): gray.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{assert_close, build, build_json, export16, setup};
use cygnus_commands::Source;
use cygnus_photo_core::adjust::{
    Adjustment, BrightnessContrast, ColorBalance, ColorShift, Curve, Curves, HueSaturation, Levels,
    LevelsChannel,
};
use cygnus_photo_core::commands::{DocumentCreate, LayerCreate, LayerKind, PixelsWrite};
use cygnus_photo_core::script::{SCRIPT_VERSION, Script};
use cygnus_photo_core::{Depth, NodeId, NodeKind, PhotoCommand, Rect};
use cygnus_photo_render::Image;

const A: [f64; 3] = [0.2, 0.6, 0.8];
const B: [f64; 3] = [0.6, 1.0, 0.2];
const G: [f64; 3] = [0.4, 0.4, 0.4];

/// Strength of the color balance (docs/decisions/0028), provisional until
/// the user has judged the comparison sheets.
const COLOR_BALANCE_STRENGTH: f64 = 0.7;

fn json(color: [f64; 3], alpha: f64) -> String {
    format!("[{}, {}, {}, {alpha}]", color[0], color[1], color[2])
}

fn adjustment_json(adjustment: &Adjustment) -> String {
    serde_json::to_string(adjustment).unwrap()
}

/// `layer.create` of an adjustment layer named "réglage" in `parent`.
fn create_adjustment(adjustment: &Adjustment, parent: Option<u64>) -> String {
    let parent = parent.map_or(String::new(), |p| format!(r#", "parent": {p}"#));
    format!(
        r#"{{"command": "layer.create", "params": {{"name": "réglage"{parent}, "kind": {{"type": "adjustment", "adjustment": {}}}}}}}"#,
        adjustment_json(adjustment)
    )
}

fn fill(layer: u64, x: u32, color: [f64; 3], alpha: f64) -> String {
    format!(
        r#"{{"command": "pixels.fill", "params": {{"layer": {layer}, "rect": {{"x": {x}, "y": 0, "width": 1, "height": 1}}, "color": {}}}}}"#,
        json(color, alpha)
    )
}

/// 3 × 1 document: backdrop (1) with A, B and G, opaque; then `extra`
/// commands (the adjustment is usually layer 2).
fn three_pixels(extra: &[String]) -> String {
    let mut commands = vec![
        r#"{"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}}"#
            .to_owned(),
        r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
        fill(1, 0, A, 1.0),
        fill(1, 1, B, 1.0),
        fill(1, 2, G, 1.0),
    ];
    commands.extend(extra.iter().cloned());
    format!("[{}]", commands.join(",\n"))
}

fn assert_pixel(image: &Image, x: u32, expected: [u16; 4], what: &str) {
    let actual = image.pixel(x, 0);
    for c in 0..4 {
        assert_close(
            actual[c],
            expected[c],
            &format!("{what}, pixel {x}, channel {c}: {actual:?}"),
        );
    }
}

fn opaque(rgb: [u16; 3]) -> [u16; 4] {
    [rgb[0], rgb[1], rgb[2], 65535]
}

fn units(value: f64) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0).round() as u16
}

fn channel(
    input_black: f32,
    input_white: f32,
    gamma: f32,
    output_black: f32,
    output_white: f32,
) -> LevelsChannel {
    LevelsChannel {
        input_black,
        input_white,
        gamma,
        output_black,
        output_white,
    }
}

fn curve(points: &[[f32; 2]]) -> Curve {
    Curve {
        points: points.to_vec(),
    }
}

fn shift(cyan_red: f32, magenta_green: f32, yellow_blue: f32) -> ColorShift {
    ColorShift {
        cyan_red,
        magenta_green,
        yellow_blue,
    }
}

/// Levels that invert every channel (output black 1, output white 0).
fn invert() -> Adjustment {
    Adjustment::Levels(Levels {
        master: channel(0.0, 1.0, 1.0, 1.0, 0.0),
        ..Levels::default()
    })
}

/// One representative of each adjustment, with its hand-computed result
/// on A (see `every_adjustment_matches_hand_computed_values`).
fn representatives() -> Vec<(&'static str, Adjustment, [f64; 3])> {
    vec![
        (
            "levels",
            Adjustment::Levels(Levels {
                master: channel(0.2, 0.8, 2.0, 0.0, 1.0),
                ..Levels::default()
            }),
            [0.0, 0.816_497, 1.0],
        ),
        (
            "curves",
            Adjustment::Curves(Curves {
                master: curve(&[[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]]),
                ..Curves::default()
            }),
            [0.076, 0.368, 0.676],
        ),
        (
            "hue_saturation",
            Adjustment::HueSaturation(HueSaturation {
                hue: 120.0,
                saturation: -50.0,
                lightness: 20.0,
            }),
            [0.72, 0.48, 0.64],
        ),
        (
            "brightness_contrast",
            Adjustment::BrightnessContrast(BrightnessContrast {
                brightness: 20.0,
                contrast: 50.0,
            }),
            [0.162_010, 0.934_558, 1.0],
        ),
        (
            "color_balance",
            Adjustment::ColorBalance(ColorBalance {
                midtones: shift(50.0, 0.0, 0.0),
                ..ColorBalance::default()
            }),
            [0.445, 0.495, 0.695],
        ),
    ]
}

/// A hand-computed case: name, adjustment, results on A, B and G.
type HandCase = (&'static str, Adjustment, [u16; 3], [u16; 3], [u16; 3]);

#[test]
fn every_adjustment_matches_hand_computed_values() {
    let Some((_, renderer)) = setup() else { return };
    // (what, adjustment, A', B', G'), in 16-bit units (v × 65535).
    let cases: Vec<HandCase> = vec![
        // Master ib 0.2, iw 0.8, gamma 2: t = clamp((C − 0.2) / 0.6),
        // t' = t^(1/2). A: t = (0, 2/3, 1), t' = (0, 0.81650, 1).
        // B: (0.81650, 1, 0). G: t = 1/3, t' = 0.57735.
        (
            "levels",
            representatives()[0].1.clone(),
            [0, 53509, 65535],
            [53509, 65535, 0],
            [37837, 37837, 37837],
        ),
        // Red: output black 0.2, R1 = 0.2 + 0.8 R; then the master inverts
        // (output black 1, output white 0): 1 − R1 = 0.8 − 0.8 R, and
        // 1 − C for green and blue (channel then master).
        // A: (0.64, 0.4, 0.2); B: (0.32, 0, 0.8); G: (0.48, 0.6, 0.6).
        (
            "levels, channel then master",
            Adjustment::Levels(Levels {
                master: channel(0.0, 1.0, 1.0, 1.0, 0.0),
                red: channel(0.0, 1.0, 1.0, 0.2, 1.0),
                ..Levels::default()
            }),
            [41942, 26214, 13107],
            [20971, 0, 52428],
            [31457, 39321, 39321],
        ),
        // Master (0, 0), (0.5, 0.25), (1, 1), Fritsch-Carlson slopes
        // (0.5, 1, 1.5). x = 0.2 on [0, 0.5], t = 0.4: Hermite basis
        // h00 = 0.648, h10 = 0.144, h01 = 0.352, h11 = −0.096:
        // 0.144 × 0.5 × 0.5 + 0.352 × 0.25 − 0.096 × 0.5 × 1 = 0.076.
        // x = 0.6 on [0.5, 1], t = 0.2: 0.896 × 0.25 + 0.128 × 0.5 × 1
        // + 0.104 × 1 − 0.032 × 0.5 × 1.5 = 0.368. x = 0.8, t = 0.6:
        // 0.352 × 0.25 + 0.096 × 0.5 + 0.648 − 0.144 × 0.75 = 0.676.
        // x = 0.4 on [0, 0.5], t = 0.8: h00 = 0.104, h10 = 0.032,
        // h01 = 0.896, h11 = −0.128:
        // 0.032 × 0.5 × 0.5 + 0.896 × 0.25 − 0.128 × 0.5 × 1 = 0.168.
        (
            "curves",
            representatives()[1].1.clone(),
            [4981, 24117, 44302],
            [24117, 65535, 4981],
            [11010, 11010, 11010],
        ),
        // Red channel (0, 0), (1, 0.6): a straight line, R × 0.6.
        (
            "curves, channel",
            Adjustment::Curves(Curves {
                red: curve(&[[0.0, 0.0], [1.0, 0.6]]),
                ..Curves::default()
            }),
            [7864, 39321, 52428],
            [23593, 65535, 13107],
            [15728, 26214, 26214],
        ),
        // h 120, s −50, l 20: A (200°, 0.6, 0.5) -> (320°, 0.3, 0.6):
        // C = (1 − |2 × 0.6 − 1|) × 0.3 = 0.24, m = 0.6 − 0.12 = 0.48,
        // 320° in the magenta sector: (C, 0, X) + m with X = C × (1 − |320 /
        // 60 mod 2 − 1|) = 0.24 × 2/3 = 0.16: (0.72, 0.48, 0.64).
        // B (90°, 1, 0.6) -> (210°, 0.5, 0.68): C = 0.32, m = 0.52,
        // X = 0.32 × (1 − |3.5 mod 2 − 1|) = 0.16, blue sector (0, X, C) + m:
        // (0.52, 0.68, 0.84). G stays gray, L 0.4 -> 0.52.
        (
            "hue/saturation",
            representatives()[2].1.clone(),
            [47185, 31457, 41942],
            [34078, 44564, 55049],
            [34078, 34078, 34078],
        ),
        // h 90, s 100: a gray has no hue and no saturation, G unchanged.
        // A -> (290°, 1, 0.5): (0.83333, 0, 1). B -> (180°, 1, 0.6), S'
        // bounded to 1: C = 0.8, m = 0.2, (0, C, C) + m = (0.2, 1, 1).
        (
            "hue/saturation on gray",
            Adjustment::HueSaturation(HueSaturation {
                hue: 90.0,
                saturation: 100.0,
                lightness: 0.0,
            }),
            [54613, 0, 65535],
            [13107, 65535, 65535],
            [26214, 26214, 26214],
        ),
        // l −50: L × 0.5, the color scales by 0.5 (S unchanged).
        (
            "hue/saturation, darker",
            Adjustment::HueSaturation(HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: -50.0,
            }),
            [6553, 19660, 26214],
            [19660, 39321, 0],
            [13107, 13107, 13107],
        ),
        // b 20: C1 = C + (1 − C) × 0.2; c 50: k = tan(67.5°) = 2.41421,
        // C' = (C1 − 0.5) × k + 0.5. A: C1 = (0.36, 0.68, 0.84) ->
        // (0.16201, 0.93456, 1). G: C1 = 0.52 -> 0.54828.
        (
            "brightness/contrast",
            representatives()[3].1.clone(),
            [10617, 61246, 65535],
            [61246, 65535, 10617],
            [35932, 35932, 35932],
        ),
        // c −100: k = 0, uniform 0.5 gray.
        (
            "contrast −100",
            Adjustment::BrightnessContrast(BrightnessContrast {
                brightness: 0.0,
                contrast: -100.0,
            }),
            [32768, 32768, 32768],
            [32768, 32768, 32768],
            [32768, 32768, 32768],
        ),
        // c +100: k = 131072, a threshold at 0.5.
        (
            "contrast +100",
            Adjustment::BrightnessContrast(BrightnessContrast {
                brightness: 0.0,
                contrast: 100.0,
            }),
            [0, 65535, 65535],
            [65535, 65535, 0],
            [0, 0, 0],
        ),
        // b −40: C × 0.6.
        (
            "brightness −40",
            Adjustment::BrightnessContrast(BrightnessContrast {
                brightness: -40.0,
                contrast: 0.0,
            }),
            [7864, 23593, 31457],
            [23593, 39321, 7864],
            [15728, 15728, 15728],
        ),
        // Midtones cyan-red +50, keep luminosity. Weights at L:
        // A, L = 0.5: (0, 1, 0), ΔR = 0.7 × 0.5 = 0.35, C1 = (0.55, 0.6,
        // 0.8), Lum 0.607 -> SetLum to 0.502: (0.445, 0.495, 0.695).
        // B, L = 0.6: midtones weight clamp(0.5 + 0.26667 / 0.25) ×
        // clamp(0.5 + 0.06667 / 0.25) = 1 × 0.76667, ΔR = 0.26833,
        // C1 = (0.86833, 1, 0.2), Lum 0.8725 -> 0.792, d = −0.0805:
        // (0.78783, 0.9195, 0.1195). G, L = 0.4: weight 0.76667, C1 = (0.66833, 0.4, 0.4),
        // Lum 0.48050 -> 0.4: (0.58783, 0.3195, 0.3195).
        (
            "color balance",
            representatives()[4].1.clone(),
            [29163, 32440, 45547],
            [51631, 60259, 7831],
            [38524, 20938, 20938],
        ),
        // The same without keeping luminosity: C1.
        (
            "color balance, luminosity not kept",
            Adjustment::ColorBalance(ColorBalance {
                midtones: shift(50.0, 0.0, 0.0),
                preserve_luminosity: false,
                ..ColorBalance::default()
            }),
            [36044, 39321, 52428],
            [56906, 65535, 13107],
            [43799, 26214, 26214],
        ),
        // Shadows yellow-blue −100, highlights magenta-green +100, without
        // keeping luminosity. A (L 0.5): weights (0, 1, 0), unchanged.
        // B (L 0.6): highlights weight clamp(0.5 − 0.06667 / 0.25) =
        // 0.23333, G already 1. G (L 0.4): shadows weight 0.23333,
        // B − 0.7 × 0.23333 = 0.23667.
        (
            "color balance, shadows and highlights",
            Adjustment::ColorBalance(ColorBalance {
                shadows: shift(0.0, 0.0, -100.0),
                highlights: shift(0.0, 100.0, 0.0),
                preserve_luminosity: false,
                ..ColorBalance::default()
            }),
            [13107, 39321, 52428],
            [39321, 65535, 13107],
            [26214, 26214, 15510],
        ),
    ];
    for (what, adjustment, a, b, g) in cases {
        let image = export16(
            &renderer,
            &build_json(&three_pixels(&[create_adjustment(&adjustment, None)])),
        );
        assert_pixel(&image, 0, opaque(a), what);
        assert_pixel(&image, 1, opaque(b), what);
        assert_pixel(&image, 2, opaque(g), what);
    }
}

/// Levels with input black equal to input white cannot be made by the
/// commands; a document holding them anyway (read from elsewhere) renders
/// a threshold, without NaN.
#[test]
fn levels_with_black_equal_to_white_are_refused_and_never_produce_nan() {
    let levels = |point: f32| {
        Adjustment::Levels(Levels {
            master: channel(point, point, 1.0, 0.0, 1.0),
            ..Levels::default()
        })
    };
    let script = format!(
        r#"{{"version": {SCRIPT_VERSION}, "commands": {}}}"#,
        three_pixels(&[create_adjustment(&levels(0.5), None)])
    );
    let error = Script::parse(&script)
        .unwrap()
        .build(Source::Test)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("input black must be below input white"),
        "{error}"
    );

    let Some((_, renderer)) = setup() else { return };
    let mut document = build_json(&three_pixels(&[create_adjustment(&invert(), None)]));
    if let Some(node) = document.nodes.get_mut(&NodeId(2)) {
        node.kind = NodeKind::Adjustment {
            adjustment: levels(0.5),
        };
    }
    let image = export16(&renderer, &document);
    // A = (0.2, 0.6, 0.8) -> (0, 1, 1); B -> (1, 1, 0); G -> 0.
    assert_pixel(&image, 0, [0, 65535, 65535, 65535], "threshold");
    assert_pixel(&image, 1, [65535, 65535, 0, 65535], "threshold");
    assert_pixel(&image, 2, [0, 0, 0, 65535], "threshold");
}

/// Straight value after the 16-bit premultiplied cache (0026): the color
/// premultiplied by `alpha_units` / 65535 is rounded to 16 bits, then
/// divided back at export.
fn through_cache(color: f64, alpha_units: u16) -> u16 {
    let alpha = f64::from(alpha_units) / 65535.0;
    let premultiplied = (color * alpha * 65535.0).round();
    units(premultiplied / f64::from(alpha_units))
}

/// Alpha 0, a tiny alpha (one 16-bit unit) and alpha 0.6, through every
/// adjustment: the alpha is kept, and nothing turns into NaN.
#[test]
fn transparent_and_nearly_transparent_pixels_stay_finite() {
    let Some((_, renderer)) = setup() else { return };
    let mut all = representatives();
    all.push(("invert", invert(), [0.8, 0.4, 0.2]));
    for (what, adjustment, a) in all {
        let script = format!(
            "[{}]",
            [
                r#"{"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}}"#.to_owned(),
                r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
                // Pixel 0: one unit of alpha; pixel 1: nothing; pixel 2: 0.6.
                fill(1, 0, A, 1.0 / 65535.0),
                fill(1, 2, A, 0.6),
                create_adjustment(&adjustment, None),
            ]
            .join(",\n")
        );
        let image = export16(&renderer, &build_json(&script));
        let tiny = image.pixel(0, 0);
        assert_eq!(tiny[3], 1, "{what}: alpha of the tiny pixel {tiny:?}");
        for c in 0..3 {
            // One unit of alpha holds 0 or 1 unit of premultiplied color:
            // the straight color is 0 or 65535.
            let expected = through_cache(a[c], 1);
            assert_eq!(tiny[c], expected, "{what}: tiny pixel {tiny:?}");
        }
        assert_eq!(image.pixel(1, 0), [0; 4], "{what}: transparent pixel");
        // 0.6 is stored as 39321 exactly.
        let expected = [
            through_cache(a[0], 39321),
            through_cache(a[1], 39321),
            through_cache(a[2], 39321),
            39321,
        ];
        assert_pixel(&image, 2, expected, what);
    }
}

/// Each adjustment behind a mask: black on pixel 0, 0.6 on pixel 1
/// (stored 39321 exactly), white on pixel 2. The result mixes A and A'
/// by the coverage: A + m × (A' − A).
#[test]
fn every_adjustment_follows_its_mask() {
    let Some((_, renderer)) = setup() else { return };
    let mask = |value: f64, x: u32| {
        format!(
            r#"{{"command": "mask.fill", "params": {{"mask": 3, "rect": {{"x": {x}, "y": 0, "width": 1, "height": 1}}, "value": {value}}}}}"#
        )
    };
    for (what, adjustment, a2) in representatives() {
        let script = format!(
            "[{}]",
            [
                r#"{"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}}"#.to_owned(),
                r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
                r#"{"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 3, "height": 1}, "color": [0.2, 0.6, 0.8, 1.0]}}"#.to_owned(),
                create_adjustment(&adjustment, None),
                r#"{"command": "mask.create", "params": {"node": 2}}"#.to_owned(),
                mask(0.0, 0),
                mask(0.6, 1),
            ]
            .join(",\n")
        );
        let image = export16(&renderer, &build_json(&script));
        let mix = |m: f64| -> [u16; 4] {
            let c = |i: usize| units(A[i] + m * (a2[i] - A[i]));
            [c(0), c(1), c(2), 65535]
        };
        assert_pixel(&image, 0, mix(0.0), what);
        assert_pixel(&image, 1, mix(0.6), what);
        assert_pixel(&image, 2, mix(1.0), what);
    }

    // Inverted at density 0.5: coverage 1 − 0.5 × (1 − (1 − m)) = 1 − 0.5 m:
    // 1, 0.7 and 0.5 on the three pixels, here with levels that invert.
    let script = format!(
        "[{}]",
        [
            r#"{"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}}"#.to_owned(),
            r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
            r#"{"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 3, "height": 1}, "color": [0.2, 0.6, 0.8, 1.0]}}"#.to_owned(),
            create_adjustment(&invert(), None),
            r#"{"command": "mask.create", "params": {"node": 2}}"#.to_owned(),
            mask(0.0, 0),
            mask(0.6, 1),
            r#"{"command": "mask.update", "params": {"id": 3, "inverted": true, "density": 0.5}}"#.to_owned(),
        ]
        .join(",\n")
    );
    let image = export16(&renderer, &build_json(&script));
    for (x, m) in [(0, 1.0), (1, 0.7), (2, 0.5)] {
        let c = |i: usize| units(A[i] + m * (1.0 - 2.0 * A[i]));
        assert_pixel(
            &image,
            x,
            [c(0), c(1), c(2), 65535],
            "inverted mask at density 0.5",
        );
    }
}

/// Each adjustment clipped onto a base: B opaque backdrop on three pixels;
/// base A at alpha 1 on pixel 0, 0.6 on pixel 1, absent on pixel 2. The
/// adjustment acts on the base only, where it has opacity: A' on pixel 0,
/// A' at 0.6 over B on pixel 1, B untouched on pixel 2.
#[test]
fn a_clipped_adjustment_acts_only_on_its_base_where_it_is_opaque() {
    let Some((_, renderer)) = setup() else { return };
    let mut all = representatives();
    all.push(("invert", invert(), [0.8, 0.4, 0.2]));
    for (what, adjustment, a2) in all {
        let script = format!(
            "[{}]",
            [
                r#"{"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}}"#.to_owned(),
                r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
                r#"{"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 3, "height": 1}, "color": [0.6, 1.0, 0.2, 1.0]}}"#.to_owned(),
                r#"{"command": "layer.create", "params": {"name": "base"}}"#.to_owned(),
                fill(2, 0, A, 1.0),
                fill(2, 1, A, 0.6),
                create_adjustment(&adjustment, None),
                r#"{"command": "layer.update", "params": {"id": 3, "clip": true}}"#.to_owned(),
            ]
            .join(",\n")
        );
        let image = export16(&renderer, &build_json(&script));
        let alpha = 39321.0 / 65535.0;
        let over = |i: usize| units(a2[i] * alpha + B[i] * (1.0 - alpha));
        assert_pixel(
            &image,
            0,
            opaque([units(a2[0]), units(a2[1]), units(a2[2])]),
            what,
        );
        assert_pixel(&image, 1, opaque([over(0), over(1), over(2)]), what);
        assert_pixel(
            &image,
            2,
            opaque([units(B[0]), units(B[1]), units(B[2])]),
            what,
        );
    }
}

/// In a pass-through group, an adjustment acts on the backdrop below the
/// group, through its mask; in an isolated group, it has only the group
/// content below it (here nothing) and changes nothing.
#[test]
fn an_adjustment_in_a_group_acts_on_what_the_group_sees() {
    let Some((_, renderer)) = setup() else { return };
    for isolation in ["pass_through", "isolated"] {
        let script = format!(
            "[{}]",
            [
                r#"{"command": "document.create", "params": {"width": 3, "height": 1, "depth": "u16"}}"#.to_owned(),
                r#"{"command": "layer.create", "params": {"name": "fond"}}"#.to_owned(),
                r#"{"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 0, "y": 0, "width": 3, "height": 1}, "color": [0.2, 0.6, 0.8, 1.0]}}"#.to_owned(),
                format!(r#"{{"command": "layer.create", "params": {{"name": "groupe", "kind": {{"type": "group", "isolation": "{isolation}"}}}}}}"#),
                create_adjustment(&invert(), Some(2)),
                r#"{"command": "mask.create", "params": {"node": 3}}"#.to_owned(),
                r#"{"command": "mask.fill", "params": {"mask": 4, "rect": {"x": 0, "y": 0, "width": 1, "height": 1}, "value": 0.0}}"#.to_owned(),
                r#"{"command": "mask.fill", "params": {"mask": 4, "rect": {"x": 1, "y": 0, "width": 1, "height": 1}, "value": 0.6}}"#.to_owned(),
            ]
            .join(",\n")
        );
        let image = export16(&renderer, &build_json(&script));
        for (x, m) in [(0, 0.0), (1, 0.6), (2, 1.0)] {
            let m = if isolation == "isolated" { 0.0 } else { m };
            let c = |i: usize| units(A[i] + m * (1.0 - 2.0 * A[i]));
            assert_pixel(&image, x, [c(0), c(1), c(2), 65535], isolation);
        }
    }
}

/// The intensity is the opacity of the node, and its blend mode applies
/// to the adjusted color: M = (1 − w) × C + w × B(C, C').
#[test]
fn intensity_and_blend_mode_apply_to_the_adjustment() {
    let Some((_, renderer)) = setup() else { return };
    let case = |update: &str| {
        let image = export16(
            &renderer,
            &build_json(&three_pixels(&[
                create_adjustment(&invert(), None),
                format!(r#"{{"command": "layer.update", "params": {{"id": 2, {update}}}}}"#),
            ])),
        );
        image.pixel(0, 0)
    };
    // Intensity 0: identity.
    let p = case(r#""opacity": 0.0"#);
    assert_eq!(p, [13107, 39321, 52428, 65535], "intensity 0");
    // Intensity 0.5, Normal: A + 0.5 × (1 − 2A) = 0.5 everywhere.
    let p = case(r#""opacity": 0.5"#);
    for value in &p[..3] {
        assert_close(*value, 32768, "intensity 0.5");
    }
    // Luminosity: SetLum(A, Lum(1 − A)) = A + (0.498 − 0.502)
    // = (0.196, 0.596, 0.796).
    let p = case(r#""blend": "luminosity""#);
    let expected = [12845, 39059, 52166];
    for c in 0..3 {
        assert_close(p[c], expected[c], "luminosity mode");
    }
}

// Analytic chain in f64, for the stacked test (formulas of 0028).

fn levels_channel(c: f64, l: &LevelsChannel) -> f64 {
    let span = f64::from(l.input_white - l.input_black);
    let t = ((c - f64::from(l.input_black)) / span).clamp(0.0, 1.0);
    let t = if l.gamma == 1.0 || t <= 0.0 {
        t
    } else {
        t.powf(1.0 / f64::from(l.gamma))
    };
    f64::from(l.output_black) + t * f64::from(l.output_white - l.output_black)
}

fn hsl(c: [f64; 3]) -> (f64, f64, f64) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h6 = h * 6.0;
    let x = c * (1.0 - (h6.rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h6.floor() as i32 {
        0 | 6 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn set_lum(c: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(c);
    let mut c = c.map(|v| v + d);
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    if n < 0.0 && l > n {
        c = c.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 && x > l {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    c
}

fn apply(adjustment: &Adjustment, c: [f64; 3]) -> [f64; 3] {
    let out = match adjustment {
        Adjustment::Levels(l) => {
            let channels = [&l.red, &l.green, &l.blue];
            [0, 1, 2].map(|i| levels_channel(levels_channel(c[i], channels[i]), &l.master))
        }
        Adjustment::Curves(k) => {
            let channels = [&k.red, &k.green, &k.blue];
            [0, 1, 2].map(|i| {
                let v = channels[i].evaluate(c[i] as f32);
                f64::from(k.master.evaluate(v))
            })
        }
        Adjustment::HueSaturation(p) => {
            let (h, s, l) = hsl(c);
            let h = (h + f64::from(p.hue) / 360.0).rem_euclid(1.0);
            let s = (s * (1.0 + f64::from(p.saturation) / 100.0)).clamp(0.0, 1.0);
            let lightness = f64::from(p.lightness) / 100.0;
            let l = if lightness <= 0.0 {
                l * (1.0 + lightness)
            } else {
                l + (1.0 - l) * lightness
            };
            rgb(h, s, l)
        }
        Adjustment::BrightnessContrast(p) => {
            let b = f64::from(p.brightness) / 100.0;
            let k = (std::f64::consts::FRAC_PI_4 * (1.0 + f64::from(p.contrast) / 100.0))
                .tan()
                .min(131_072.0);
            c.map(|v| {
                let v1 = if b <= 0.0 {
                    v * (1.0 + b)
                } else {
                    v + (1.0 - v) * b
                };
                (v1 - 0.5) * k + 0.5
            })
        }
        Adjustment::ColorBalance(p) => {
            let l = (c[0].max(c[1]).max(c[2]) + c[0].min(c[1]).min(c[2])) / 2.0;
            let shadows = (0.5 - (l - 1.0 / 3.0) / 0.25).clamp(0.0, 1.0);
            let highlights = (0.5 + (l - 2.0 / 3.0) / 0.25).clamp(0.0, 1.0);
            let midtones = (0.5 + (l - 1.0 / 3.0) / 0.25).clamp(0.0, 1.0)
                * (0.5 - (l - 2.0 / 3.0) / 0.25).clamp(0.0, 1.0);
            let axis = |s: &ColorShift| [s.cyan_red, s.magenta_green, s.yellow_blue];
            let (sh, mid, hi) = (axis(&p.shadows), axis(&p.midtones), axis(&p.highlights));
            let c1 = [0, 1, 2].map(|i| {
                let delta = f64::from(sh[i]) * shadows
                    + f64::from(mid[i]) * midtones
                    + f64::from(hi[i]) * highlights;
                (c[i] + COLOR_BALANCE_STRENGTH * delta / 100.0).clamp(0.0, 1.0)
            });
            if p.preserve_luminosity {
                set_lum(c1, lum(c))
            } else {
                c1
            }
        }
    };
    out.map(|v| v.clamp(0.0, 1.0))
}

/// Five adjustments stacked, one of each kind, against the analytic chain
/// in f64: they are composed in f32 in one pass, without any 16-bit
/// rounding between them. The same chain rounded to 16 bits after each
/// adjustment moves away from the GPU result, which shows that no such
/// rounding happens (the last levels stretch errors about 3.3 times).
#[test]
fn five_stacked_adjustments_match_the_analytic_chain() {
    let Some((_, renderer)) = setup() else { return };
    let chain = [
        Adjustment::ColorBalance(ColorBalance {
            shadows: shift(0.0, 0.0, -15.0),
            midtones: shift(20.0, -10.0, 10.0),
            highlights: shift(-5.0, 0.0, 10.0),
            preserve_luminosity: true,
        }),
        Adjustment::HueSaturation(HueSaturation {
            hue: 25.0,
            saturation: 30.0,
            lightness: -10.0,
        }),
        Adjustment::BrightnessContrast(BrightnessContrast {
            brightness: 10.0,
            contrast: 25.0,
        }),
        Adjustment::Curves(Curves {
            master: curve(&[[0.0, 0.0], [0.25, 0.18], [0.75, 0.82], [1.0, 1.0]]),
            red: curve(&[[0.0, 0.05], [0.6, 0.55], [1.0, 1.0]]),
            ..Curves::default()
        }),
        Adjustment::Levels(Levels {
            master: channel(0.3, 0.6, 1.3, 0.0, 1.0),
            ..Levels::default()
        }),
    ];
    // 1024 pseudo-random opaque colors.
    const WIDTH: u32 = 1024;
    let mut seed = 0x2545_f491_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed >> 16) as u16
    };
    let colors: Vec<[u16; 3]> = (0..WIDTH).map(|_| [next(), next(), next()]).collect();
    let mut commands = vec![
        PhotoCommand::DocumentCreate(DocumentCreate {
            width: WIDTH,
            height: 1,
            resolution: 72.0,
            depth: Depth::U16,
            profile: Default::default(),
        }),
        PhotoCommand::LayerCreate(LayerCreate {
            name: "couleurs".to_owned(),
            parent: None,
            index: None,
            kind: LayerKind::Pixel,
        }),
        PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(1),
            rect: Rect {
                x: 0,
                y: 0,
                width: WIDTH,
                height: 1,
            },
            samples: colors
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], u16::MAX])
                .collect(),
        }),
    ];
    for adjustment in &chain {
        commands.push(PhotoCommand::LayerCreate(LayerCreate {
            name: "réglage".to_owned(),
            parent: None,
            index: None,
            kind: LayerKind::Adjustment {
                adjustment: adjustment.clone(),
            },
        }));
    }
    let image = export16(&renderer, &build(commands));
    let mut rounded_apart = 0;
    for (x, color) in colors.iter().enumerate() {
        let input = color.map(|v| f64::from(v) / 65535.0);
        let exact = chain.iter().fold(input, |c, a| apply(a, c));
        let rounded = chain.iter().fold(input, |c, a| {
            apply(a, c).map(|v| f64::from(units(v)) / 65535.0)
        });
        let actual = image.pixel(x as u32, 0);
        for c in 0..3 {
            assert_close(
                actual[c],
                units(exact[c]),
                &format!("pixel {x} {color:?}, channel {c}"),
            );
            if actual[c].abs_diff(units(rounded[c])) > 1 {
                rounded_apart += 1;
            }
        }
    }
    assert!(
        rounded_apart > 0,
        "rounding between adjustments would give the same result: the test proves nothing"
    );
}

/// A 16-bit gradient through a steep curve and through levels that
/// stretch the highlights ×10 (cases of docs/decisions/0026): every output
/// is within one unit of the formula, so no band is added, and the output
/// never goes down.
#[test]
fn steep_curves_and_levels_add_no_banding() {
    let Some((_, renderer)) = setup() else { return };
    const WIDTH: u32 = 4096;
    let gradient = |first: u16, step: u16, adjustment: Adjustment| {
        let samples = (0..WIDTH)
            .flat_map(|x| {
                let v = first + step * x as u16;
                [v, v, v, u16::MAX]
            })
            .collect();
        build(vec![
            PhotoCommand::DocumentCreate(DocumentCreate {
                width: WIDTH,
                height: 1,
                resolution: 72.0,
                depth: Depth::U16,
                profile: Default::default(),
            }),
            PhotoCommand::LayerCreate(LayerCreate {
                name: "dégradé".to_owned(),
                parent: None,
                index: None,
                kind: LayerKind::Pixel,
            }),
            PhotoCommand::PixelsWrite(PixelsWrite {
                layer: NodeId(1),
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: WIDTH,
                    height: 1,
                },
                samples,
            }),
            PhotoCommand::LayerCreate(LayerCreate {
                name: "réglage".to_owned(),
                parent: None,
                index: None,
                kind: LayerKind::Adjustment { adjustment },
            }),
        ])
    };
    let steep = curve(&[[0.0, 0.0], [0.45, 0.05], [0.55, 0.95], [1.0, 1.0]]);
    let cases = [
        (
            "steep curve, 0.4 to 0.525",
            26214,
            2,
            Adjustment::Curves(Curves {
                master: steep.clone(),
                ..Curves::default()
            }),
        ),
        (
            "levels, highlights ×10",
            58982,
            1,
            Adjustment::Levels(Levels {
                master: channel(0.9, 1.0, 1.0, 0.0, 1.0),
                ..Levels::default()
            }),
        ),
        (
            "levels, shadows ×10",
            0,
            1,
            Adjustment::Levels(Levels {
                master: channel(0.0, 0.1, 1.0, 0.0, 1.0),
                ..Levels::default()
            }),
        ),
    ];
    for (what, first, step, adjustment) in cases {
        let image = export16(&renderer, &gradient(first, step, adjustment.clone()));
        let mut previous = 0u16;
        let mut distinct = 0;
        for x in 0..WIDTH {
            let v = f64::from(first + step * x as u16) / 65535.0;
            let expected = units(apply(&adjustment, [v; 3])[0]);
            let actual = image.pixel(x, 0)[0];
            assert_close(actual, expected, &format!("{what}, pixel {x}"));
            assert!(actual >= previous, "{what}: goes down at pixel {x}");
            if actual != previous || x == 0 {
                distinct += 1;
            }
            previous = actual;
        }
        println!("{what}: {distinct} distinct output values for {WIDTH} inputs");
    }
}

/// The commands refuse to clip a layer onto an adjustment (0028); a
/// document holding one anyway (read from elsewhere) applies the
/// adjustment and shows the clipped layer nowhere.
#[test]
fn a_layer_clipped_onto_an_adjustment_shows_nowhere() {
    let Some((_, renderer)) = setup() else { return };
    let mut document = build_json(&three_pixels(&[
        create_adjustment(&invert(), None),
        r#"{"command": "layer.create", "params": {"name": "rouge"}}"#.to_owned(),
        r#"{"command": "pixels.fill", "params": {"layer": 3, "rect": {"x": 0, "y": 0, "width": 3, "height": 1}, "color": [1.0, 0.0, 0.0, 1.0]}}"#.to_owned(),
    ]));
    if let Some(node) = document.nodes.get_mut(&NodeId(3)) {
        node.clip = true;
    }
    assert_eq!(document.clipped_onto_effects(), vec![NodeId(3)]);
    let image = export16(&renderer, &document);
    for (x, color) in [A, B, G].into_iter().enumerate() {
        let inverted = color.map(|v| units(1.0 - v));
        assert_pixel(
            &image,
            x as u32,
            opaque(inverted),
            "clipped onto an adjustment",
        );
    }
}
