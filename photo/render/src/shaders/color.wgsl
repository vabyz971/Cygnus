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

// Color conversions: the only place where they are written
// (docs/decisions/0011 and 0021). Every shader that converts colors
// includes this module.
//
// Conventions (0021, Q1): composition happens in sRGB-encoded values with
// premultiplied alpha; stored and exported pixels are sRGB-encoded with
// straight alpha. Only resampling (pyramid reduction, display filtering)
// goes through linear light.

// sRGB transfer function (IEC 61966-2-1), encoded value to linear light.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// Inverse sRGB transfer function, linear light to encoded value.
fn linear_to_srgb(c: f32) -> f32 {
    let v = clamp(c, 0.0, 1.0);
    if v <= 0.0031308 {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

// Straight RGBA to premultiplied RGBA, in the same color space.
fn premultiply(straight: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(straight.rgb * straight.a, straight.a);
}

// Premultiplied RGBA to straight RGBA, in the same color space. A fully
// transparent pixel has no color: it gives zero.
fn unpremultiply(premultiplied: vec4<f32>) -> vec4<f32> {
    let alpha = clamp(premultiplied.a, 0.0, 1.0);
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(clamp(premultiplied.rgb / alpha, vec3<f32>(0.0), vec3<f32>(1.0)), alpha);
}

// Premultiplied sRGB-encoded RGBA to premultiplied linear RGBA, for
// resampling.
fn encoded_to_linear(premultiplied: vec4<f32>) -> vec4<f32> {
    let straight = unpremultiply(premultiplied);
    let linear = vec3<f32>(
        srgb_to_linear(straight.r),
        srgb_to_linear(straight.g),
        srgb_to_linear(straight.b),
    );
    return vec4<f32>(linear * straight.a, straight.a);
}

// Premultiplied linear RGBA back to premultiplied sRGB-encoded RGBA.
fn linear_to_encoded(premultiplied: vec4<f32>) -> vec4<f32> {
    let straight = unpremultiply(premultiplied);
    let encoded = vec3<f32>(
        linear_to_srgb(straight.r),
        linear_to_srgb(straight.g),
        linear_to_srgb(straight.b),
    );
    return vec4<f32>(encoded * straight.a, straight.a);
}

// Quantizes a 0..1 value to an integer from 0 to `max_value`.
fn quantize(v: f32, max_value: f32) -> u32 {
    return u32(round(clamp(v, 0.0, 1.0) * max_value));
}
