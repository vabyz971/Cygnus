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

// Histogram pass (REG-4, docs/decisions/0025): counts the straight
// sRGB-encoded values of the composite tiles shown by the canvas, in 256
// bins per channel. Transparent pixels (outside the document) are skipped.

@group(0) @binding(0) var tiles: texture_2d_array<COMPUTE_SAMPLED>;
@group(0) @binding(1) var<storage, read> slots: array<u32>;
// Red, green and blue bins (256 each), then the number of pixels counted.
@group(0) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;

fn bin(v: f32) -> u32 {
    return min(u32(clamp(v, 0.0, 1.0) * 256.0), 255u);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let slot = slots[id.z];
    let color = unpremultiply(unpack_compute(textureLoad(tiles, vec2<i32>(id.xy), i32(slot), 0)));
    if color.a <= 0.0 {
        return;
    }
    atomicAdd(&counts[bin(color.r)], 1u);
    atomicAdd(&counts[256u + bin(color.g)], 1u);
    atomicAdd(&counts[512u + bin(color.b)], 1u);
    atomicAdd(&counts[768u], 1u);
}
