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

// Brush stroke (docs/decisions/0035): one invocation per pixel of a target
// tile. The coverage of the dabs reaching the tile is accumulated in f32,
// in order, by source-over (t = t + c × flow × (1 − t)); the stroke, a
// flat color of alpha t × opacity, is then blended over the pixel by
// blend() and written once, rounded to the depth of the document. A pixel
// no dab covers keeps its exact value; a pixel whose alpha rounds to 0 is
// written as zeros.

struct Params {
    // Straight sRGB-encoded color of the brush.
    color: vec4<f32>,
    // Largest coverage of the stroke.
    opacity: f32,
    // Part of the radius at full coverage.
    hardness: f32,
    // Blend mode code (BLEND_*).
    mode: u32,
    // Largest value of the document depth: 255 or 65535.
    max_value: f32,
}

struct Job {
    // First dab of the tile in `dabs`, and their number.
    first: u32,
    count: u32,
    // Width and height of the part of the tile inside the layer.
    width: u32,
    height: u32,
    // First word of the tile in `input` and `output` (two per pixel).
    offset: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct Dab {
    // Center relative to the tile origin, in pixels.
    x: f32,
    y: f32,
    radius: f32,
    // Coverage laid.
    flow: f32,
}

@group(0) @binding(0) var<storage, read> params: Params;
@group(0) @binding(1) var<storage, read> jobs: array<Job>;
@group(0) @binding(2) var<storage, read> dabs: array<Dab>;
// Straight 16-bit values of the target (8-bit ones times 257): R | G << 16,
// then B | A << 16.
@group(0) @binding(3) var<storage, read> input: array<u32>;
// The same layout, in the document depth.
@group(0) @binding(4) var<storage, read_write> output: array<u32>;

// Coverage of `dab` at the point `p`: 1 up to `hardness × radius`, then a
// smoothstep down to 0 at the radius, over one pixel at least.
fn profile(dab: Dab, p: vec2<f32>) -> f32 {
    let d = distance(p, vec2<f32>(dab.x, dab.y));
    if d >= dab.radius {
        return 0.0;
    }
    let inner = min(params.hardness * dab.radius, max(dab.radius - 1.0, 0.0));
    if d <= inner {
        return 1.0;
    }
    return 1.0 - smoothstep(inner, dab.radius, d);
}

// Coverage of `dab` on the pixel whose center is `p`; dabs under 4 pixels
// wide are averaged over 4 × 4 points of the pixel, so that thin strokes do
// not flicker.
fn coverage(dab: Dab, p: vec2<f32>) -> f32 {
    if dab.radius >= 2.0 {
        return profile(dab, p);
    }
    var sum = 0.0;
    for (var j = 0u; j < 4u; j++) {
        for (var i = 0u; i < 4u; i++) {
            let offset = (vec2<f32>(f32(i), f32(j)) + 0.5) / 4.0 - 0.5;
            sum += profile(dab, p + offset);
        }
    }
    return sum / 16.0;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let word = job.offset + (id.y * TILE_SIZE + id.x) * 2u;
    let rg = input[word];
    let ba = input[word + 1u];
    if id.x >= job.width || id.y >= job.height {
        output[word] = rg;
        output[word + 1u] = ba;
        return;
    }
    let p = vec2<f32>(id.xy) + 0.5;
    var t = 0.0;
    for (var k = 0u; k < job.count; k++) {
        let dab = dabs[job.first + k];
        let c = coverage(dab, p);
        t = t + c * dab.flow * (1.0 - t);
    }
    if t <= 0.0 {
        output[word] = rg;
        output[word + 1u] = ba;
        return;
    }
    let straight = vec4<f32>(
        f32(rg & 0xffffu),
        f32(rg >> 16u),
        f32(ba & 0xffffu),
        f32(ba >> 16u),
    ) / 65535.0;
    let alpha = t * params.opacity * params.color.a;
    let source = vec4<f32>(params.color.rgb * alpha, alpha);
    let result = unpremultiply(blend(params.mode, premultiply(straight), source));
    var q = vec4<u32>(
        quantize(result.r, params.max_value),
        quantize(result.g, params.max_value),
        quantize(result.b, params.max_value),
        quantize(result.a, params.max_value),
    );
    // A transparent pixel has no color (0021): a coverage that rounds to
    // no alpha leaves zeros, whatever the last bits of f32 gave.
    if q.a == 0u {
        q = vec4<u32>(0u);
    }
    output[word] = q.r | (q.g << 16u);
    output[word + 1u] = q.b | (q.a << 16u);
}
