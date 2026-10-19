struct Uniforms { brightness: f32, contrast: f32 };
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var output_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> u: Uniforms;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(input_tex, 0);
    if (gid.x >= dims.x || gid.y >= dims.y) { return; }
    let c = textureLoad(input_tex, gid.xy, 0);
    var rgb = c.rgb;
    rgb = (rgb - vec3<f32>(0.5)) * u.contrast + vec3<f32>(0.5) + vec3<f32>(u.brightness);
    // Alpha préservé, rgb borné (rgba8unorm sature aussi à l'écriture)
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    textureStore(output_tex, gid.xy, vec4<f32>(rgb, c.a));
}
