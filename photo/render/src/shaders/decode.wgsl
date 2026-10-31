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

// Decode pass: stored tiles (8 or 16 bits, straight sRGB) to the compute
// format (linear, premultiplied). One invocation per pixel, one workgroup
// layer (z) per tile job.

struct DecodeJob {
    // Offset of the tile in `raw`, in u32 words.
    offset: u32,
    // Destination slot in the layer pool.
    slot: u32,
    // 8 or 16.
    bits: u32,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read> raw: array<u32>;
@group(0) @binding(1) var<storage, read> jobs: array<DecodeJob>;
@group(0) @binding(2) var pool: texture_storage_2d_array<COMPUTE_FORMAT, write>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let pixel = id.y * TILE_SIZE + id.x;
    var straight: vec4<f32>;
    if job.bits == 8u {
        straight = unpack4x8unorm(raw[job.offset + pixel]);
    } else {
        let rg = unpack2x16unorm(raw[job.offset + 2u * pixel]);
        let ba = unpack2x16unorm(raw[job.offset + 2u * pixel + 1u]);
        straight = vec4<f32>(rg, ba);
    }
    textureStore(pool, vec2<i32>(id.xy), i32(job.slot), decode_srgb(straight));
}
