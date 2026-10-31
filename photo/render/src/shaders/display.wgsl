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

// Display pass: draws the canvas into egui's frame (docs/decisions/0012).
// Each screen pixel looks up the composite tile under it. Sampling is
// nearest: at 100 %, each screen pixel is exactly one document pixel, and
// the shown value is the 8-bit export value (same encoding and
// quantization). Filtered views below 100 % arrive with the tile pyramid.

struct Display {
    // Framebuffer position of the document origin, in pixels.
    origin: vec2<f32>,
    // Framebuffer pixels per document pixel.
    scale: f32,
    // Tiles per row of the document.
    tiles_x: u32,
    // Document size in pixels.
    size: vec2<u32>,
    // Side of a transparency checker square, in framebuffer pixels.
    checker: f32,
    _pad: u32,
    // sRGB colors: around the document, and the two checker squares.
    background: vec4<f32>,
    checker_light: vec4<f32>,
    checker_dark: vec4<f32>,
}

@group(0) @binding(0) var composites: texture_2d_array<f32>;
@group(0) @binding(1) var<storage, read> pages: array<i32>;
@group(0) @binding(2) var<uniform> display: Display;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the viewport set by egui.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let offset = position.xy - display.origin;
    let point = offset / display.scale;
    let size = vec2<f32>(display.size);
    if point.x < 0.0 || point.y < 0.0 || point.x >= size.x || point.y >= size.y {
        return vec4<f32>(display.background.rgb, 1.0);
    }
    let pixel = vec2<u32>(floor(point));
    let tile = pixel / TILE_SIZE;
    let slot = pages[tile.y * display.tiles_x + tile.x];
    var color = vec4<f32>(0.0);
    if slot >= 0 {
        color = textureLoad(composites, vec2<i32>(pixel % TILE_SIZE), slot, 0);
    }
    let straight = encode_srgb(color);
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
