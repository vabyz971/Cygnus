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
// Level 1.

// Normal: source-over (W3C, section 5.1 and 9.1.4), premultiplied form:
// co = cs + cb * (1 - as), ao = as + ab * (1 - as).
fn blend_normal(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    return src + dst * (1.0 - src.a);
}

fn blend(mode: u32, dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    switch mode {
        case BLEND_NORMAL: {
            return blend_normal(dst, src);
        }
        // Other modes arrive at milestone J3; the renderer refuses them
        // before any GPU work, so this branch is never taken.
        default: {
            return blend_normal(dst, src);
        }
    }
}

// Source-atop (W3C, section 9.1.4, Porter-Duff), premultiplied form, for
// clipped layers (CAL-5): co = cs * ab + cb * (1 - as), ao = ab. The
// source keeps only the coverage of the backdrop.
fn blend_atop(mode: u32, dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    switch mode {
        case BLEND_NORMAL: {
            return vec4<f32>(src.rgb * dst.a + dst.rgb * (1.0 - src.a), dst.a);
        }
        // Other modes arrive with step B4 of J3; refused before any GPU
        // work until then.
        default: {
            return vec4<f32>(src.rgb * dst.a + dst.rgb * (1.0 - src.a), dst.a);
        }
    }
}
