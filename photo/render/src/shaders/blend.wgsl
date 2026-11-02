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

// Blend modes: the only place where blend formulas are written
// (AGENTS.md rule 1, docs/decisions/0012). Mode codes BLEND_* are generated
// from the Rust BlendMode enum when shaders are assembled.
//
// `dst` is the backdrop and `src` the layer, both sRGB-encoded and
// premultiplied (docs/decisions/0021, Q1), the layer opacity and mask
// already applied to `src`. Formulas follow W3C Compositing and Blending
// Level 1: the blend function B(Cb, Cs) works on straight colors (section
// 10), and is mixed with the source by the general formula of section 5.8,
// in the same encoded space.

// Separable modes (W3C, section 10.1), one channel.

fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        // Multiply(Cb, 2 × Cs).
        return cb * 2.0 * cs;
    }
    // Screen(Cb, 2 × Cs − 1).
    let s = 2.0 * cs - 1.0;
    return cb + s - cb * s;
}

fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    var d = sqrt(cb);
    if cb <= 0.25 {
        d = ((16.0 * cb - 12.0) * cb + 4.0) * cb;
    }
    return cb + (2.0 * cs - 1.0) * (d - cb);
}

// Non-separable modes (W3C, section 10.2).

fn lum(c: vec3<f32>) -> f32 {
    return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b;
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    var out = c;
    // l > n and x > l whenever the branches are taken, since l is a mean
    // of the channels with positive weights.
    if n < 0.0 && l > n {
        out = l + (out - l) * l / (l - n);
    }
    if x > 1.0 && x > l {
        out = l + (out - l) * (1.0 - l) / (x - l);
    }
    return out;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + (l - lum(c)));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
}

// The W3C form sorts the channels into Cmin, Cmid and Cmax: Cmax becomes s,
// Cmin 0 and Cmid (Cmid − Cmin) × s / (Cmax − Cmin). The same mapping
// applied to every channel gives the three at once.
fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    if x > n {
        return (c - n) * s / (x - n);
    }
    return vec3<f32>(0.0);
}

// Blend function B(Cb, Cs) of `mode`, on straight colors.
fn blend_function(mode: u32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    switch mode {
        case BLEND_MULTIPLY: {
            return cb * cs;
        }
        case BLEND_SCREEN: {
            return cb + cs - cb * cs;
        }
        case BLEND_OVERLAY: {
            // HardLight with the layers swapped.
            return vec3<f32>(hard_light(cs.r, cb.r), hard_light(cs.g, cb.g), hard_light(cs.b, cb.b));
        }
        case BLEND_SOFT_LIGHT: {
            return vec3<f32>(soft_light(cb.r, cs.r), soft_light(cb.g, cs.g), soft_light(cb.b, cs.b));
        }
        case BLEND_DARKEN: {
            return min(cb, cs);
        }
        case BLEND_LIGHTEN: {
            return max(cb, cs);
        }
        case BLEND_DIFFERENCE: {
            return abs(cb - cs);
        }
        case BLEND_HUE: {
            return set_lum(set_sat(cs, sat(cb)), lum(cb));
        }
        case BLEND_SATURATION: {
            return set_lum(set_sat(cb, sat(cs)), lum(cb));
        }
        case BLEND_COLOR: {
            return set_lum(cs, lum(cb));
        }
        case BLEND_LUMINOSITY: {
            return set_lum(cb, lum(cs));
        }
        // BLEND_NORMAL: B(Cb, Cs) = Cs.
        default: {
            return cs;
        }
    }
}

// Source mixed with the blend result, premultiplied by the source alpha
// (W3C, section 5.8): αs × ((1 − αb) × Cs + αb × B(Cb, Cs))
// = cs × (1 − αb) + αs × αb × B(Cb, Cs).
fn blended_source(mode: u32, dst: vec4<f32>, src: vec4<f32>) -> vec3<f32> {
    let b = blend_function(mode, unpremultiply(dst).rgb, unpremultiply(src).rgb);
    return src.rgb * (1.0 - dst.a) + src.a * dst.a * b;
}

// Source-over with a blend mode (W3C, sections 5.8 and 9.1.4),
// premultiplied form:
// co = cs × (1 − αb) + αs × αb × B(Cb, Cs) + cb × (1 − αs),
// ao = αs + αb × (1 − αs).
fn blend(mode: u32, dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    switch mode {
        // B(Cb, Cs) = Cs reduces the general form to co = cs + cb × (1 − αs),
        // computed directly.
        case BLEND_NORMAL: {
            return src + dst * (1.0 - src.a);
        }
        case BLEND_MULTIPLY, BLEND_SCREEN, BLEND_OVERLAY, BLEND_SOFT_LIGHT, BLEND_DARKEN,
            BLEND_LIGHTEN, BLEND_DIFFERENCE, BLEND_HUE, BLEND_SATURATION, BLEND_COLOR,
            BLEND_LUMINOSITY, default: {
            let rgb = blended_source(mode, dst, src) + dst.rgb * (1.0 - src.a);
            return vec4<f32>(rgb, src.a + dst.a * (1.0 - src.a));
        }
    }
}

// Source-atop with a blend mode (W3C, sections 5.8 and 9.1.4,
// Fa = αb, Fb = 1 − αs), premultiplied form, for clipped layers (CAL-5):
// co = αb × (cs × (1 − αb) + αs × αb × B(Cb, Cs)) + cb × (1 − αs),
// ao = αb. The source keeps only the coverage of the backdrop.
fn blend_atop(mode: u32, dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    switch mode {
        // Normal reduces to co = cs × αb + cb × (1 − αs).
        case BLEND_NORMAL: {
            return vec4<f32>(src.rgb * dst.a + dst.rgb * (1.0 - src.a), dst.a);
        }
        case BLEND_MULTIPLY, BLEND_SCREEN, BLEND_OVERLAY, BLEND_SOFT_LIGHT, BLEND_DARKEN,
            BLEND_LIGHTEN, BLEND_DIFFERENCE, BLEND_HUE, BLEND_SATURATION, BLEND_COLOR,
            BLEND_LUMINOSITY, default: {
            let rgb = blended_source(mode, dst, src) * dst.a + dst.rgb * (1.0 - src.a);
            return vec4<f32>(rgb, dst.a);
        }
    }
}
