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

// Downsample pass: builds a tile of pyramid level L from its four children
// of level L - 1 (docs/decisions/0018). Each job fills one quadrant of a
// scratch tile with the 2 × 2 box average of one child, in linear
// premultiplied light. A missing child (outside the document) gives a
// transparent quadrant.

struct DownsampleJob {
    // Slot of the child in the cache, or -1 when there is none.
    child: i32,
    // Destination slot in the scratch pool.
    scratch: u32,
    // Quadrant of the parent: 0 top left, 1 top right, 2 bottom left,
    // 3 bottom right.
    quadrant: u32,
    _pad: u32,
}

@group(0) @binding(0) var source: texture_2d_array<COMPUTE_SAMPLED>;
@group(0) @binding(1) var<storage, read> jobs: array<DownsampleJob>;
@group(0) @binding(2) var scratch: texture_storage_2d_array<COMPUTE_FORMAT, write>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let half = TILE_SIZE / 2u;
    let destination = vec2<i32>(
        i32((job.quadrant % 2u) * half + id.x),
        i32((job.quadrant / 2u) * half + id.y),
    );
    var color = vec4<f32>(0.0);
    if job.child >= 0 {
        let p = vec2<i32>(id.xy * 2u);
        // Averaged in linear light (0021), stored encoded like the rest.
        color = linear_to_encoded(0.25 * (
            encoded_to_linear(unpack_compute(textureLoad(source, p, job.child, 0)))
            + encoded_to_linear(unpack_compute(textureLoad(source, p + vec2<i32>(1, 0), job.child, 0)))
            + encoded_to_linear(unpack_compute(textureLoad(source, p + vec2<i32>(0, 1), job.child, 0)))
            + encoded_to_linear(unpack_compute(textureLoad(source, p + vec2<i32>(1, 1), job.child, 0)))
        ));
    }
    textureStore(scratch, destination, i32(job.scratch), pack_compute(color));
}
