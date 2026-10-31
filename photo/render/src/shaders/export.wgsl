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

// Export pass: composite tiles (linear, premultiplied) to straight sRGB
// integers of 8 or 16 bits, packed in u32 words for the readback.

struct ExportJob {
    // Source slot in the composite pool.
    slot: u32,
    // Offset of the tile in `output`, in u32 words.
    offset: u32,
    // 8 or 16.
    bits: u32,
    _pad: u32,
}

@group(0) @binding(0) var composites: texture_2d_array<f32>;
@group(0) @binding(1) var<storage, read> jobs: array<ExportJob>;
@group(0) @binding(2) var<storage, read_write> output: array<u32>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let pixel = id.y * TILE_SIZE + id.x;
    let color = encode_srgb(textureLoad(composites, vec2<i32>(id.xy), i32(job.slot), 0));
    if job.bits == 8u {
        let r = quantize(color.r, 255.0);
        let g = quantize(color.g, 255.0);
        let b = quantize(color.b, 255.0);
        let a = quantize(color.a, 255.0);
        output[job.offset + pixel] = r | (g << 8u) | (b << 16u) | (a << 24u);
    } else {
        let r = quantize(color.r, 65535.0);
        let g = quantize(color.g, 65535.0);
        let b = quantize(color.b, 65535.0);
        let a = quantize(color.a, 65535.0);
        output[job.offset + 2u * pixel] = r | (g << 16u);
        output[job.offset + 2u * pixel + 1u] = b | (a << 16u);
    }
}
