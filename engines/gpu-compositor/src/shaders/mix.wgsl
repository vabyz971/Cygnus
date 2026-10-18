// Adjustment mix: original and adjusted blended per channel (alpha
// included) by weight x coverage, rounded like the CPU reference.

struct Params {
    misc: vec4<f32>,   // weight (0..1), unused, unused, unused
    cover: vec4<f32>,  // offset x, offset y, outside value, enabled (0/1)
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var adj: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> p: Params;
@group(0) @binding(4) var cover: texture_2d<f32>;

fn coverage(px: vec2<i32>) -> f32 {
    if (p.cover.w < 0.5) { return 1.0; }
    let mp = px - vec2<i32>(p.cover.xy);
    let size = vec2<i32>(textureDimensions(cover));
    if (mp.x < 0 || mp.y < 0 || mp.x >= size.x || mp.y >= size.y) {
        return p.cover.z;
    }
    return textureLoad(cover, mp, 0).r;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(src);
    if (id.x >= dims.x || id.y >= dims.y) { return; }
    let px = vec2<i32>(id.xy);
    let s = textureLoad(src, px, 0);
    let a = textureLoad(adj, px, 0);
    let w = p.misc.x * coverage(px);
    let out = round((s * (1.0 - w) + a * w) * 255.0) / 255.0;
    textureStore(dst, px, clamp(out, vec4<f32>(0.0), vec4<f32>(1.0)));
}
