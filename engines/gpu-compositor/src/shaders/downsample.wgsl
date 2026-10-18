// One mip level from the previous one: 2x2 box filter on premultiplied
// colors (edge texels repeated for odd sizes).

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(dst);
    if (id.x >= dims.x || id.y >= dims.y) { return; }
    let maxc = vec2<i32>(textureDimensions(src)) - vec2<i32>(1);
    let s = vec2<i32>(id.xy) * 2;
    let a = textureLoad(src, clamp(s, vec2<i32>(0), maxc), 0);
    let b = textureLoad(src, clamp(s + vec2<i32>(1, 0), vec2<i32>(0), maxc), 0);
    let c = textureLoad(src, clamp(s + vec2<i32>(0, 1), vec2<i32>(0), maxc), 0);
    let d = textureLoad(src, clamp(s + vec2<i32>(1, 1), vec2<i32>(0), maxc), 0);
    textureStore(dst, vec2<i32>(id.xy), (a + b + c + d) * 0.25);
}
