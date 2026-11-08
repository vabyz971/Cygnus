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

// Display pass: draws the canvas into egui's frame (docs/decisions/0012,
// 0018 and 0034). Each screen pixel looks up the composite tiles of the
// pyramid level chosen for the zoom; where that level has no tile shown
// yet, the first coarser level that has one is shown enlarged (nearest),
// else the pixel is transparent. At 100 % and above, sampling is nearest:
// each screen pixel is exactly one document pixel at 100 %, and the shown
// value is the 8-bit export value (same quantization). Below 100 %, the
// level is sampled bilinearly in linear light, then encoded again (0021).

struct Display {
    // Framebuffer position of the document origin, in pixels.
    origin: vec2<f32>,
    // Framebuffer pixels per document pixel.
    scale: f32,
    // Document pixels per pixel of the displayed level (2^level).
    level_scale: f32,
    // Size of the displayed level, in its pixels.
    level_size: vec2<u32>,
    // Displayed level.
    level: u32,
    // Side of a transparency checker square, in framebuffer pixels.
    checker: f32,
    // sRGB colors: around the document, and the two checker squares.
    background: vec4<f32>,
    checker_light: vec4<f32>,
    checker_dark: vec4<f32>,
}

// Page tables of every level: word 0 is the number of levels; then for
// each level, the offset of its table, its tiles per row, its width and
// its height; then the tables, one slot (or -1) per tile.
@group(0) @binding(0) var composites: texture_2d_array<COMPUTE_SAMPLED>;
@group(0) @binding(1) var<storage, read> pages: array<i32>;
@group(0) @binding(2) var<uniform> display: Display;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the viewport set by egui.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

// Composite value of pixel `p` of the displayed level; where its tile is
// not shown, the pixel covering it in the first coarser level shown;
// transparent outside the level or where no level has a tile.
fn fetch(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(display.level_size);
    if p.x < 0 || p.y < 0 || p.x >= size.x || p.y >= size.y {
        return vec4<f32>(0.0);
    }
    let levels = u32(pages[0]);
    for (var level = display.level; level < levels; level++) {
        let header = 1u + 4u * level;
        // Pixel of `level` covering the center of `p`.
        let shift = level - display.level;
        let pixel = vec2<u32>(p) >> vec2<u32>(shift);
        let level_size = vec2<u32>(u32(pages[header + 2u]), u32(pages[header + 3u]));
        if pixel.x >= level_size.x || pixel.y >= level_size.y {
            return vec4<f32>(0.0);
        }
        let tile = pixel / TILE_SIZE;
        let slot = pages[u32(pages[header]) + tile.y * u32(pages[header + 1u]) + tile.x];
        if slot >= 0 {
            return unpack_compute(textureLoad(composites, vec2<i32>(pixel % TILE_SIZE), slot, 0));
        }
    }
    return vec4<f32>(0.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let offset = position.xy - display.origin;
    let point = offset / display.scale;
    let size = vec2<f32>(display.level_size) * display.level_scale;
    if point.x < 0.0 || point.y < 0.0 || point.x >= size.x || point.y >= size.y {
        return vec4<f32>(display.background.rgb, 1.0);
    }
    let level_point = point / display.level_scale;
    var color: vec4<f32>;
    if display.scale >= 1.0 {
        color = fetch(vec2<i32>(floor(level_point)));
    } else {
        // Resampling in linear light (0021): the four neighbours are
        // decoded, interpolated, then encoded again.
        let q = level_point - 0.5;
        let p = vec2<i32>(floor(q));
        let f = q - floor(q);
        let top = mix(
            encoded_to_linear(fetch(p)),
            encoded_to_linear(fetch(p + vec2<i32>(1, 0))),
            f.x,
        );
        let bottom = mix(
            encoded_to_linear(fetch(p + vec2<i32>(0, 1))),
            encoded_to_linear(fetch(p + vec2<i32>(1, 1))),
            f.x,
        );
        color = linear_to_encoded(mix(top, bottom, f.y));
    }
    let straight = unpremultiply(color);
    let shown = vec4<f32>(
        f32(quantize(straight.r, 255.0)),
        f32(quantize(straight.g, 255.0)),
        f32(quantize(straight.b, 255.0)),
        f32(quantize(straight.a, 255.0)),
    ) / 255.0;
    let cell = vec2<u32>(floor(offset / display.checker));
    var checker = display.checker_dark.rgb;
    if ((cell.x + cell.y) & 1u) == 0u {
        checker = display.checker_light.rgb;
    }
    return vec4<f32>(shown.rgb * shown.a + checker * (1.0 - shown.a), 1.0);
}
