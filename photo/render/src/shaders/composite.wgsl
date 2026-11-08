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
const OP_ADJUST: u32 = 7u;
const OP_FILTER: u32 = 8u;

const NO_SLOT: u32 = 0xffffffffu;
const FLAG_INVERTED: u32 = 1u;

struct Op {
    code: u32,
    // Slot of the layer tile in the layer pool (OP_LAYER, OP_CLIPPED), first
    // word of the adjustment parameters in `params` (OP_ADJUST), or slot of
    // the filtered tile in `filtered` (OP_FILTER).
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
    // FLAG_INVERTED; adjustment kind (ADJUST_*) in bits 8 and up.
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
// Read by adjust.wgsl.
@group(0) @binding(5) var<storage, read> params: array<u32>;
@group(0) @binding(6) var<storage, read> curve_tables: array<f32>;
// Filtered tiles of the filter stages (docs/decisions/0023).
@group(0) @binding(7) var filtered: texture_2d_array<COMPUTE_SAMPLED>;

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
            case OP_ADJUST: {
                // docs/decisions/0028: the straight color is adjusted, mixed
                // by the blend mode on an opaque backdrop,
                // M = (1 − w) × C + w × B(C, C'), then premultiplied again
                // by the unchanged alpha.
                if accumulator.a > 0.0 {
                    let straight = unpremultiply(accumulator).rgb;
                    let adjusted = adjust(op.flags >> 8u, op.slot, straight);
                    let mixed = blend(op.mode, vec4<f32>(straight, 1.0), vec4<f32>(adjusted * weight, weight));
                    accumulator = vec4<f32>(mixed.rgb * accumulator.a, accumulator.a);
                }
            }
            case OP_FILTER: {
                // The filtered accumulator replaces the accumulator, mixed
                // by opacity and mask; in another mode than Normal, it is
                // first blended over the accumulator with that mode.
                let result = unpack_compute(textureLoad(filtered, position, i32(op.slot), 0));
                var mixed = result;
                if op.mode != BLEND_NORMAL {
                    mixed = blend(op.mode, accumulator, result);
                }
                accumulator = accumulator + (mixed - accumulator) * weight;
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
