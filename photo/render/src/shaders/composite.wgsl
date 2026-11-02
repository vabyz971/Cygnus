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

// Composite pass: interprets the composition program of each output tile
// (docs/decisions/0012 and 0021, photo/render/src/program.rs). Sources are
// read as integers from the source pools (sRGB-encoded, straight alpha),
// premultiplied without transfer function, and composed in f32 with a
// stack of registers; the result is rounded to the compute format once
// (pack_compute).

const OP_LAYER: u32 = 0u;
const OP_CLIPPED: u32 = 1u;
const OP_PUSH_ISOLATED: u32 = 2u;
const OP_PUSH_PASS_THROUGH: u32 = 3u;
const OP_POP_ISOLATED: u32 = 4u;
const OP_POP_PASS_THROUGH: u32 = 5u;
const OP_POP_CLIPPED: u32 = 6u;

const NO_SLOT: u32 = 0xffffffffu;
const FLAG_INVERTED: u32 = 1u;

struct Op {
    code: u32,
    // Slot of the layer tile in the layer pool (OP_LAYER, OP_CLIPPED).
    slot: u32,
    // Blend mode code (BLEND_*).
    mode: u32,
    // Opacity, 0..1.
    opacity: f32,
    // Slot of the mask tile in the mask pool, or NO_SLOT.
    mask_slot: u32,
    // Mask coverage where it has no tile.
    mask_value: f32,
    // Mask density, 0 (no effect) to 1.
    density: f32,
    // FLAG_INVERTED.
    flags: u32,
}

struct CompositeJob {
    // First operation of the job in `ops`.
    first: u32,
    // Number of operations.
    count: u32,
    // Destination slot in the target pool.
    slot: u32,
    _pad: u32,
}

@group(0) @binding(0) var layers: texture_2d_array<u32>;
@group(0) @binding(1) var masks: texture_2d_array<u32>;
@group(0) @binding(2) var<storage, read> ops: array<Op>;
@group(0) @binding(3) var<storage, read> jobs: array<CompositeJob>;
@group(0) @binding(4) var output: texture_storage_2d_array<COMPUTE_FORMAT, write>;

// Coverage of the mask of `op` at `position`.
fn coverage(op: Op, position: vec2<i32>) -> f32 {
    var m = op.mask_value;
    if op.mask_slot != NO_SLOT {
        m = unpack_mask(textureLoad(masks, position, i32(op.mask_slot), 0));
    }
    if (op.flags & FLAG_INVERTED) != 0u {
        m = 1.0 - m;
    }
    return 1.0 - op.density * (1.0 - m);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.z];
    let position = vec2<i32>(id.xy);
    var accumulator = vec4<f32>(0.0);
    var stack: array<vec4<f32>, STACK_DEPTH>;
    var depth = 0u;
    for (var i = 0u; i < job.count; i++) {
        let op = ops[job.first + i];
        let weight = op.opacity * coverage(op, position);
        switch op.code {
            case OP_LAYER, OP_CLIPPED: {
                let straight = unpack_source(textureLoad(layers, position, i32(op.slot), 0));
                let source = premultiply(straight) * weight;
                if op.code == OP_LAYER {
                    accumulator = blend(op.mode, accumulator, source);
                } else {
                    accumulator = blend_atop(op.mode, accumulator, source);
                }
            }
            case OP_PUSH_ISOLATED, OP_PUSH_PASS_THROUGH: {
                stack[min(depth, STACK_DEPTH - 1u)] = accumulator;
                depth += 1u;
                if op.code == OP_PUSH_ISOLATED {
                    accumulator = vec4<f32>(0.0);
                }
            }
            case OP_POP_ISOLATED, OP_POP_CLIPPED: {
                depth = max(depth, 1u) - 1u;
                let below = stack[min(depth, STACK_DEPTH - 1u)];
                if op.code == OP_POP_ISOLATED {
                    accumulator = blend(op.mode, below, accumulator * weight);
                } else {
                    accumulator = blend_atop(op.mode, below, accumulator * weight);
                }
            }
            case OP_POP_PASS_THROUGH: {
                depth = max(depth, 1u) - 1u;
                let below = stack[min(depth, STACK_DEPTH - 1u)];
                accumulator = below + (accumulator - below) * weight;
            }
            default: {}
        }
    }
    textureStore(output, position, i32(job.slot), pack_compute(accumulator));
}
