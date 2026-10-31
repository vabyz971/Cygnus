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

// Composite pass: runs the composition program of each output tile
// (docs/decisions/0012). Each operation blends one layer tile over the
// accumulator, in f32; the result is rounded to the compute format once.

struct Op {
    // Slot of the layer tile in the layer pool.
    slot: u32,
    // Blend mode code (BLEND_*).
    mode: u32,
    // Layer opacity, 0..1.
    opacity: f32,
    _pad: u32,
}

struct CompositeJob {
    // First operation of the job in `ops`.
    first: u32,
    // Number of operations.
    count: u32,
    // Destination slot in the composite pool.
    slot: u32,
    _pad: u32,
}

@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var<storage, read> ops: array<Op>;
@group(0) @binding(2) var<storage, read> jobs: array<CompositeJob>;
@group(0) @binding(3) var output: texture_storage_2d_array<COMPUTE_FORMAT, write>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let position = vec2<i32>(id.xy);
    var accumulator = vec4<f32>(0.0);
    for (var i = 0u; i < job.count; i++) {
        let op = ops[job.first + i];
        let source = textureLoad(layers, position, i32(op.slot), 0) * op.opacity;
        accumulator = blend(op.mode, accumulator, source);
    }
    textureStore(output, position, i32(job.slot), accumulator);
}
