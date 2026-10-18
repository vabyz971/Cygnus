// Straight-alpha accumulator -> premultiplied output (egui's texture
// convention: gamma space, premultiplied alpha), mip level 0.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(src);
    if (id.x >= dims.x || id.y >= dims.y) { return; }
    let c = textureLoad(src, vec2<i32>(id.xy), 0);
    textureStore(dst, vec2<i32>(id.xy), vec4<f32>(c.rgb * c.a, c.a));
}
