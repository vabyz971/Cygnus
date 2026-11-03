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

// Adjustments: the only place where adjustment formulas are written
// (docs/decisions/0028). Kind codes ADJUST_* are generated from
// photo/render/src/adjust.rs, which also encodes the parameters read here
// from `params` (32-bit words) and the curve lookup tables of
// `curve_tables`, both bound by the composite pass.
//
// Every adjustment acts on a straight sRGB-encoded color (0021, Q5) and
// returns a color bounded to 0..1. `Lum` and `SetLum` come from
// blend.wgsl, not repeated here.

fn word(offset: u32) -> f32 {
    return bitcast<f32>(params[offset]);
}

// Dead zone above input black, 2^-20: a sixteenth of a 16-bit unit, much
// larger than the f32 rounding of the drivers (about 1.5e-8 measured on
// Metal) and smaller than the step between two 16-bit levels.
const BLACK_DEAD_ZONE: f32 = 9.5367431640625e-7;

// Levels of one channel at `o`: input black, input white, gamma, output
// black, output white. Input black equal to input white (refused by the
// commands) gives a threshold, never a division by zero.
//
// The gamma curve has an infinite slope at input black: the f32 rounding
// of the GPU (about 1e-8 after unpremultiplying, depending on the driver)
// would become (1e-8)^(1/gamma), 0.16 for gamma 10. Inputs less than
// BLACK_DEAD_ZONE above input black give 0 (docs/decisions/0028).
fn levels_channel(c: f32, o: u32) -> f32 {
    let input_black = word(o);
    let input_white = word(o + 1u);
    let gamma = word(o + 2u);
    let output_black = word(o + 3u);
    let output_white = word(o + 4u);
    let above = c - input_black;
    var t = clamp(above / max(input_white - input_black, 1e-7), 0.0, 1.0);
    if above < BLACK_DEAD_ZONE {
        t = 0.0;
    }
    if gamma != 1.0 && t > 0.0 {
        t = pow(t, 1.0 / gamma);
    }
    return output_black + t * (output_white - output_black);
}

// Words: red, green, blue, master (5 each). Each channel goes through its
// own levels, then through the master.
fn levels(c: vec3<f32>, o: u32) -> vec3<f32> {
    return vec3<f32>(
        levels_channel(levels_channel(c.r, o), o + 15u),
        levels_channel(levels_channel(c.g, o + 5u), o + 15u),
        levels_channel(levels_channel(c.b, o + 10u), o + 15u),
    );
}

// Curve whose table is described at `o`: domain start and end, first value
// and size in `curve_tables`. The input is bounded to the domain, where
// the curve is flat outside, then the table is read with linear
// interpolation (never the nearest value).
fn curve_value(x: f32, o: u32) -> f32 {
    let start = word(o);
    let end = word(o + 1u);
    let first = params[o + 2u];
    let size = params[o + 3u];
    let u = (clamp(x, start, end) - start) / (end - start) * f32(size - 1u);
    let i = min(u32(u), size - 2u);
    let f = u - f32(i);
    return mix(curve_tables[first + i], curve_tables[first + i + 1u], f);
}

// Words: red, green, blue, master (4 each). Each channel goes through its
// own curve, then through the master.
fn curves(c: vec3<f32>, o: u32) -> vec3<f32> {
    return vec3<f32>(
        curve_value(curve_value(c.r, o), o + 12u),
        curve_value(curve_value(c.g, o + 4u), o + 12u),
        curve_value(curve_value(c.b, o + 8u), o + 12u),
    );
}

// HSL of a color: hue in 0..1, saturation, lightness.
fn rgb_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let x = max(max(c.r, c.g), c.b);
    let n = min(min(c.r, c.g), c.b);
    let l = (x + n) / 2.0;
    let d = x - n;
    if d <= 0.0 {
        return vec3<f32>(0.0, 0.0, l);
    }
    let s = d / (1.0 - abs(2.0 * l - 1.0));
    var h: f32;
    if x == c.r {
        h = (c.g - c.b) / d;
        if h < 0.0 {
            h += 6.0;
        }
    } else if x == c.g {
        h = (c.b - c.r) / d + 2.0;
    } else {
        h = (c.r - c.g) / d + 4.0;
    }
    return vec3<f32>(h / 6.0, s, l);
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let chroma = (1.0 - abs(2.0 * hsl.z - 1.0)) * hsl.y;
    let h = hsl.x * 6.0;
    let x = chroma * (1.0 - abs(h - 2.0 * floor(h / 2.0) - 1.0));
    let m = hsl.z - chroma / 2.0;
    var rgb: vec3<f32>;
    if h < 1.0 {
        rgb = vec3<f32>(chroma, x, 0.0);
    } else if h < 2.0 {
        rgb = vec3<f32>(x, chroma, 0.0);
    } else if h < 3.0 {
        rgb = vec3<f32>(0.0, chroma, x);
    } else if h < 4.0 {
        rgb = vec3<f32>(0.0, x, chroma);
    } else if h < 5.0 {
        rgb = vec3<f32>(x, 0.0, chroma);
    } else {
        rgb = vec3<f32>(chroma, 0.0, x);
    }
    return rgb + m;
}

// Words: hue / 360, saturation / 100, lightness / 100.
fn hue_saturation(c: vec3<f32>, o: u32) -> vec3<f32> {
    let hsl = rgb_to_hsl(c);
    let shifted = hsl.x + word(o);
    let hue = shifted - floor(shifted);
    let saturation = clamp(hsl.y * (1.0 + word(o + 1u)), 0.0, 1.0);
    let change = word(o + 2u);
    var lightness = hsl.z * (1.0 + change);
    if change > 0.0 {
        lightness = hsl.z + (1.0 - hsl.z) * change;
    }
    return hsl_to_rgb(vec3<f32>(hue, saturation, lightness));
}

// Words: brightness / 100, contrast slope k (computed in f64 on the CPU).
fn brightness_contrast(c: vec3<f32>, o: u32) -> vec3<f32> {
    let b = word(o);
    let k = word(o + 1u);
    var c1 = c * (1.0 + b);
    if b > 0.0 {
        c1 = c + (1.0 - c) * b;
    }
    return (c1 - 0.5) * k + 0.5;
}

// Words: shadows, midtones, highlights (cyan-red, magenta-green,
// yellow-blue, each / 100), keep luminosity (0 or 1), strength. Weights are
// ramps of width 0.25 on the HSL lightness, centered on 1/3 and 2/3.
fn color_balance(c: vec3<f32>, o: u32) -> vec3<f32> {
    let l = (max(max(c.r, c.g), c.b) + min(min(c.r, c.g), c.b)) / 2.0;
    let shadows = clamp(0.5 - (l - 1.0 / 3.0) / 0.25, 0.0, 1.0);
    let highlights = clamp(0.5 + (l - 2.0 / 3.0) / 0.25, 0.0, 1.0);
    let midtones = clamp(0.5 + (l - 1.0 / 3.0) / 0.25, 0.0, 1.0)
        * clamp(0.5 - (l - 2.0 / 3.0) / 0.25, 0.0, 1.0);
    let shift = vec3<f32>(word(o), word(o + 1u), word(o + 2u)) * shadows
        + vec3<f32>(word(o + 3u), word(o + 4u), word(o + 5u)) * midtones
        + vec3<f32>(word(o + 6u), word(o + 7u), word(o + 8u)) * highlights;
    let c1 = clamp(c + word(o + 10u) * shift, vec3<f32>(0.0), vec3<f32>(1.0));
    if params[o + 9u] != 0u {
        return set_lum(c1, lum(c));
    }
    return c1;
}

// Adjustment `kind` with its parameters at `o`, on a straight color.
fn adjust(kind: u32, o: u32, c: vec3<f32>) -> vec3<f32> {
    var out = c;
    switch kind {
        case ADJUST_LEVELS: {
            out = levels(c, o);
        }
        case ADJUST_CURVES: {
            out = curves(c, o);
        }
        case ADJUST_HUE_SATURATION: {
            out = hue_saturation(c, o);
        }
        case ADJUST_BRIGHTNESS_CONTRAST: {
            out = brightness_contrast(c, o);
        }
        case ADJUST_COLOR_BALANCE: {
            out = color_balance(c, o);
        }
        default: {}
    }
    return clamp(out, vec3<f32>(0.0), vec3<f32>(1.0));
}
