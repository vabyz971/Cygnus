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

// Reduced sources (docs/decisions/0027, B7-lite): one tile of a pyramid
// level built from the factor × factor source tiles it covers, read from
// the source pool, for the preview shown while a parameter is dragged.
// Layers: premultiplied box average of straight 16-bit values, back to
// straight values. Masks: average coverage. A missing child counts as
// transparent (layers) or as the empty value of the mask.

struct ReduceJob {
    // First child slot in `children` (factor × factor, row by row).
    first: u32,
    // Value of a missing child: 0 for layers, the empty value of a mask.
    empty: u32,
    // First word of the reduced tile in `output`.
    offset: u32,
    // 2 or 4.
    factor: u32,
}

const NO_CHILD: u32 = 0xffffffffu;

@group(0) @binding(0) var tiles: texture_2d_array<u32>;
@group(0) @binding(1) var<storage, read> jobs: array<ReduceJob>;
@group(0) @binding(2) var<storage, read> children: array<u32>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;

// Texel (x, y) of the area covered by the reduced tile, in source tiles.
fn child_texel(job: ReduceJob, x: u32, y: u32) -> vec4<u32> {
    let child = children[job.first + (y / TILE_SIZE) * job.factor + x / TILE_SIZE];
    if child == NO_CHILD {
        return vec4<u32>(job.empty, 0u, 0u, 0u);
    }
    let local = vec2<i32>(i32(x % TILE_SIZE), i32(y % TILE_SIZE));
    return textureLoad(tiles, local, i32(child), 0);
}

@compute @workgroup_size(16, 16, 1)
fn reduce_layers(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let f = job.factor;
    var alpha = 0.0;
    var color = vec3<f32>(0.0);
    for (var dy = 0u; dy < f; dy++) {
        for (var dx = 0u; dx < f; dx++) {
            let texel = vec4<f32>(child_texel(job, id.x * f + dx, id.y * f + dy)) / 65535.0;
            alpha += texel.a;
            color += texel.rgb * texel.a;
        }
    }
    var straight = vec3<f32>(0.0);
    if alpha > 0.0 {
        straight = color / alpha;
    }
    let value = vec4<u32>(round(clamp(vec4<f32>(straight, alpha / f32(f * f)), vec4<f32>(0.0), vec4<f32>(1.0)) * 65535.0));
    let word = job.offset + (id.y * TILE_SIZE + id.x) * 2u;
    output[word] = value.r | (value.g << 16u);
    output[word + 1u] = value.b | (value.a << 16u);
}

// One invocation per pair of mask texels, packed in one word.
@compute @workgroup_size(16, 16, 1)
fn reduce_masks(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let f = job.factor;
    var packed = 0u;
    for (var p = 0u; p < 2u; p++) {
        let x = id.x * 2u + p;
        var sum = 0.0;
        for (var dy = 0u; dy < f; dy++) {
            for (var dx = 0u; dx < f; dx++) {
                sum += f32(child_texel(job, x * f + dx, id.y * f + dy).r);
            }
        }
        let value = u32(round(sum / f32(f * f)));
        packed = packed | (min(value, 65535u) << (16u * p));
    }
    output[job.offset + id.y * (TILE_SIZE / 2u) + id.x] = packed;
}
