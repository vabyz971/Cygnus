// Cygnus GPU compositor: blends ONE layer over the accumulator.
//
// Straight-alpha, gamma-space values, same math as the CPU reference
// (`photo_engine::document::compositing::blend_pixel`), including the
// per-layer 8-bit truncation so both paths agree pixel for pixel on
// translations.

struct Params {
    // Target pixel center -> layer pixel space: q = M * p + t.
    m: vec4<f32>,        // m00, m01, m10, m11
    t_size: vec4<f32>,   // tx, ty, layer width, layer height
    misc: vec4<f32>,     // opacity (0..1), blend mode id, target w, target h
    // Coverage mask (R channel) multiplied into the layer alpha:
    // mask pixel = target pixel - offset; `outside` beyond its bounds.
    cover: vec4<f32>,    // offset x, offset y, outside value, enabled (0/1)
};

@group(0) @binding(0) var acc_in: texture_2d<f32>;
@group(0) @binding(1) var layer: texture_2d<f32>;
@group(0) @binding(2) var acc_out: texture_storage_2d<rgba8unorm, write>;
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

fn blend_channel(b: f32, t: f32, mode: u32) -> f32 {
    if (mode == 1u) { return b * t; }
    if (mode == 2u) { return 1.0 - (1.0 - b) * (1.0 - t); }
    if (mode == 3u) {
        if (b < 0.5) { return 2.0 * b * t; }
        return 1.0 - 2.0 * (1.0 - b) * (1.0 - t);
    }
    if (mode == 4u) { return min(b, t); }
    if (mode == 5u) { return max(b, t); }
    // Separable modes 6..13: mirror `photo_engine` `blend_separable`.
    if (mode == 6u) {
        if (b <= 0.0) { return 0.0; }
        if (t >= 1.0) { return 1.0; }
        return min(b / (1.0 - t), 1.0);
    }
    if (mode == 7u) {
        if (b >= 1.0) { return 1.0; }
        if (t <= 0.0) { return 0.0; }
        return 1.0 - min((1.0 - b) / t, 1.0);
    }
    if (mode == 8u) {
        if (t < 0.5) { return 2.0 * b * t; }
        return 1.0 - 2.0 * (1.0 - b) * (1.0 - t);
    }
    if (mode == 9u) {
        if (t <= 0.5) { return b - (1.0 - 2.0 * t) * b * (1.0 - b); }
        var d = sqrt(b);
        if (b <= 0.25) { d = ((16.0 * b - 12.0) * b + 4.0) * b; }
        return b + (2.0 * t - 1.0) * (d - b);
    }
    if (mode == 10u) { return abs(b - t); }
    if (mode == 11u) { return b + t - 2.0 * b * t; }
    if (mode == 12u) { return min(b + t, 1.0); }
    if (mode == 13u) { return max(b + t - 1.0, 0.0); }
    return t;
}

// Bilinear tap at layer pixel coords `q` (centers at +0.5), edge-clamped
// taps, transparent outside the layer (CPU convention).
fn sample_layer(q: vec2<f32>) -> vec4<f32> {
    let size = p.t_size.zw;
    if (q.x < 0.0 || q.y < 0.0 || q.x >= size.x || q.y >= size.y) {
        return vec4<f32>(0.0);
    }
    let c = q - vec2<f32>(0.5);
    let f = fract(c);
    let base = vec2<i32>(floor(c));
    let maxc = vec2<i32>(size) - vec2<i32>(1);
    let p00 = textureLoad(layer, clamp(base, vec2<i32>(0), maxc), 0);
    let p10 = textureLoad(layer, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), maxc), 0);
    let p01 = textureLoad(layer, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), maxc), 0);
    let p11 = textureLoad(layer, clamp(base + vec2<i32>(1, 1), vec2<i32>(0), maxc), 0);
    return mix(mix(p00, p10, f.x), mix(p01, p11, f.x), f.y);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = vec2<u32>(p.misc.zw);
    if (id.x >= dims.x || id.y >= dims.y) { return; }
    let px = vec2<i32>(id.xy);
    let b = textureLoad(acc_in, px, 0);
    let center = vec2<f32>(id.xy) + vec2<f32>(0.5);
    let q = vec2<f32>(
        p.m.x * center.x + p.m.y * center.y + p.t_size.x,
        p.m.z * center.x + p.m.w * center.y + p.t_size.y,
    );
    var t = sample_layer(q);
    t.a = t.a * p.misc.x * coverage(px);
    if (t.a <= 0.001) {
        textureStore(acc_out, px, b);
        return;
    }
    let mode = u32(p.misc.y);
    let blended = vec3<f32>(
        blend_channel(b.r, t.r, mode),
        blend_channel(b.g, t.g, mode),
        blend_channel(b.b, t.b, mode),
    );
    let a = t.a + b.a * (1.0 - t.a);
    var out = vec4<f32>(b.rgb, a);
    if (a > 0.0001) {
        out = vec4<f32>((blended * t.a + b.rgb * b.a * (1.0 - t.a)) / a, a);
    }
    // CPU stores `(v * 255) as u8` (truncation) after each layer.
    out = floor(clamp(out, vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0) / 255.0;
    textureStore(acc_out, px, out);
}
