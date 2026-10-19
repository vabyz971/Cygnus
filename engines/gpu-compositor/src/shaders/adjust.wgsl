// One point-wise adjustment step (straight alpha, alpha untouched), with
// the CPU reference's quantization:
// - mode 0: per-channel 256-entry LUT (exact);
// - mode 1: saturation around Rec.601 gray, truncated (`as u8`);
// - mode 2: HSL shift, rounded (`(x * 255).round()`).

struct Params {
    op: vec4<f32>,   // mode, a, b, c
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var lut: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> p: Params;

fn lut_at(v: f32) -> f32 {
    let i = u32(clamp(round(v * 255.0), 0.0, 255.0));
    return textureLoad(lut, vec2<i32>(i32(i), 0), 0).r;
}

fn rgb_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let mx = max(max(c.r, c.g), c.b);
    let mn = min(min(c.r, c.g), c.b);
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if (d < 1e-6) { return vec3<f32>(0.0, 0.0, l); }
    var s = d / (mx + mn);
    if (l > 0.5) { s = d / (2.0 - mx - mn); }
    var h = 0.0;
    if (abs(mx - c.r) < 1e-6) {
        h = (c.g - c.b) / d;
        if (c.g < c.b) { h = h + 6.0; }
    } else if (abs(mx - c.g) < 1e-6) {
        h = (c.b - c.r) / d + 2.0;
    } else {
        h = (c.r - c.g) / d + 4.0;
    }
    return vec3<f32>(h / 6.0, s, l);
}

fn rem1(t: f32) -> f32 {
    return t - floor(t);
}

fn channel(t0: f32, p_: f32, q: f32) -> f32 {
    let t = rem1(t0);
    if (t < 1.0 / 6.0) { return p_ + (q - p_) * 6.0 * t; }
    if (t < 0.5) { return q; }
    if (t < 2.0 / 3.0) { return p_ + (q - p_) * (2.0 / 3.0 - t) * 6.0; }
    return p_;
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x;
    let s = hsl.y;
    let l = hsl.z;
    if (s < 1e-6) { return vec3<f32>(l); }
    var q = l + s - l * s;
    if (l < 0.5) { q = l * (1.0 + s); }
    let p_ = 2.0 * l - q;
    return vec3<f32>(channel(h + 1.0 / 3.0, p_, q), channel(h, p_, q), channel(h - 1.0 / 3.0, p_, q));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(src);
    if (id.x >= dims.x || id.y >= dims.y) { return; }
    let px = vec2<i32>(id.xy);
    let c = textureLoad(src, px, 0);
    let mode = u32(p.op.x);
    var rgb = c.rgb;
    if (mode == 0u) {
        rgb = vec3<f32>(lut_at(c.r), lut_at(c.g), lut_at(c.b));
    } else if (mode == 1u) {
        let s = p.op.y;
        let v = c.rgb * 255.0;
        let gray = 0.299 * v.r + 0.587 * v.g + 0.114 * v.b;
        rgb = floor(clamp(vec3<f32>(gray) + (v - vec3<f32>(gray)) * s, vec3<f32>(0.0), vec3<f32>(255.0))) / 255.0;
    } else {
        var hsl = rgb_to_hsl(c.rgb);
        hsl.x = rem1(hsl.x + p.op.y / 360.0);
        let ks = clamp(p.op.z / 100.0, -1.0, 1.0);
        if (ks < 0.0) { hsl.y = hsl.y * (1.0 + ks); } else { hsl.y = hsl.y + (1.0 - hsl.y) * ks * hsl.y; }
        let kl = clamp(p.op.w / 100.0, -1.0, 1.0);
        if (kl < 0.0) { hsl.z = hsl.z * (1.0 + kl); } else { hsl.z = hsl.z + (1.0 - hsl.z) * kl; }
        hsl = vec3<f32>(hsl.x, clamp(hsl.y, 0.0, 1.0), clamp(hsl.z, 0.0, 1.0));
        rgb = clamp(round(hsl_to_rgb(hsl) * 255.0), vec3<f32>(0.0), vec3<f32>(255.0)) / 255.0;
    }
    textureStore(dst, px, vec4<f32>(rgb, c.a));
}
