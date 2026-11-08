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

// Filter passes (docs/decisions/0023): a separable Gaussian blur, then the
// unsharp mask built on it. The input is a stage, the accumulator of the
// tile programs cut before the filter, stored in tiles of the compute
// format and read across tile borders through a page table. Values are
// sRGB-encoded with premultiplied alpha, as everywhere in the composition
// (0021); outside the document, the nearest pixel of the document is read
// (the edge is extended).

const KIND_BLUR: u32 = 0u;
const KIND_UNSHARP: u32 = 1u;

struct Params {
    // Size of the pyramid level, in its pixels.
    level_size: vec2<u32>,
    // Taps on each side of the kernel.
    radius: u32,
    // KIND_BLUR or KIND_UNSHARP.
    kind: u32,
    // Unsharp mask strength (0 to 5).
    amount: f32,
    // Smallest difference sharpened, 0..1.
    threshold: f32,
    _pad0: u32,
    _pad1: u32,
}

struct Job {
    // Destination slot of the tile.
    slot: u32,
    // Tile coordinates at the level.
    tx: u32,
    ty: u32,
    _pad: u32,
}

// Page tables: first tile x and y, width and height in tiles, then one slot
// (or -1) per tile, row by row.
@group(0) @binding(0) var input: texture_2d_array<COMPUTE_SAMPLED>;
@group(0) @binding(1) var<storage, read> input_pages: array<i32>;
@group(0) @binding(2) var<storage, read> weights: array<f32>;
@group(0) @binding(3) var<storage, read> jobs: array<Job>;
@group(0) @binding(4) var output: texture_storage_2d_array<COMPUTE_FORMAT, write>;
@group(0) @binding(5) var<storage, read> params: Params;
@group(0) @binding(6) var original: texture_2d_array<COMPUTE_SAMPLED>;
@group(0) @binding(7) var<storage, read> original_pages: array<i32>;

// Slot of the tile holding pixel `p` (clamped to the level) in a page
// table, and the pixel inside it; slot -1 when the table has no tile there.
fn locate(p: vec2<i32>, x0: i32, y0: i32, w: i32, h: i32, slot_at: ptr<function, i32>) -> vec2<i32> {
    let size = vec2<i32>(params.level_size);
    let q = clamp(p, vec2<i32>(0), size - vec2<i32>(1));
    let tile = q / i32(TILE_SIZE);
    let local = q - tile * i32(TILE_SIZE);
    let tx = tile.x - x0;
    let ty = tile.y - y0;
    *slot_at = -1;
    if tx >= 0 && ty >= 0 && tx < w && ty < h {
        *slot_at = ty * w + tx;
    }
    return local;
}

fn fetch_input(p: vec2<i32>) -> vec4<f32> {
    var index = -1;
    let local = locate(p, input_pages[0], input_pages[1], input_pages[2], input_pages[3], &index);
    if index < 0 {
        return vec4<f32>(0.0);
    }
    let slot = input_pages[4 + index];
    if slot < 0 {
        return vec4<f32>(0.0);
    }
    return unpack_compute(textureLoad(input, local, slot, 0));
}

fn fetch_original(p: vec2<i32>) -> vec4<f32> {
    var index = -1;
    let local = locate(p, original_pages[0], original_pages[1], original_pages[2], original_pages[3], &index);
    if index < 0 {
        return vec4<f32>(0.0);
    }
    let slot = original_pages[4 + index];
    if slot < 0 {
        return vec4<f32>(0.0);
    }
    return unpack_compute(textureLoad(original, local, slot, 0));
}

// Horizontal pass of the blur.
@compute @workgroup_size(16, 16, 1)
fn blur_h(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let p = vec2<i32>(vec2<u32>(job.tx, job.ty) * TILE_SIZE + id.xy);
    let r = i32(params.radius);
    var sum = weights[0] * fetch_input(p);
    for (var i = 1; i <= r; i++) {
        sum += weights[i] * (fetch_input(p + vec2<i32>(i, 0)) + fetch_input(p - vec2<i32>(i, 0)));
    }
    textureStore(output, vec2<i32>(id.xy), i32(job.slot), pack_compute(sum));
}

// Vertical pass of the blur, then the unsharp mask: where the original
// differs from the blur by more than the threshold, the difference is
// amplified (docs/decisions/0023), channel by channel; the color stays
// within the alpha.
@compute @workgroup_size(16, 16, 1)
fn blur_v(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let p = vec2<i32>(vec2<u32>(job.tx, job.ty) * TILE_SIZE + id.xy);
    let r = i32(params.radius);
    var sum = weights[0] * fetch_input(p);
    for (var i = 1; i <= r; i++) {
        sum += weights[i] * (fetch_input(p + vec2<i32>(0, i)) + fetch_input(p - vec2<i32>(0, i)));
    }
    var result = sum;
    if params.kind == KIND_UNSHARP {
        let source = fetch_original(p);
        let difference = source - sum;
        let sharpened = source + params.amount * difference;
        let keep = abs(difference) <= vec4<f32>(params.threshold);
        result = select(sharpened, source, keep);
        let alpha = clamp(result.a, 0.0, 1.0);
        result = vec4<f32>(clamp(result.rgb, vec3<f32>(0.0), vec3<f32>(alpha)), alpha);
    }
    textureStore(output, vec2<i32>(id.xy), i32(job.slot), pack_compute(result));
}
