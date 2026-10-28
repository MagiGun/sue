// Sue UI batch shader: one instanced quad per box, glyph or image.
//
// kind 0: rounded box (signed-distance antialiasing, optional border)
// kind 1: glyph (R8 coverage atlas, tinted)
// kind 2: RGBA image (sRGB texture)
// kind 3: gray image (R8 byte map, sRGB-encoded values)

struct Globals {
    viewport: vec2<f32>,
    encode_srgb: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) clip: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) size: vec2<f32>,
    @location(6) clip: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: Instance) -> VsOut {
    let corner = vec2<f32>(f32(index & 1u), f32((index >> 1u) & 1u));
    let pixel = instance.rect.xy + corner * instance.rect.zw;
    let ndc = vec2<f32>(
        pixel.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / globals.viewport.y * 2.0,
    );
    var out: VsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.local = corner * instance.rect.zw;
    out.uv = mix(instance.uv.xy, instance.uv.zw, corner);
    out.color = instance.color;
    out.border = instance.border;
    out.params = instance.params;
    out.size = instance.rect.zw;
    out.clip = instance.clip;
    return out;
}

fn rounded_box(p: vec2<f32>, half: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb(value: vec3<f32>) -> vec3<f32> {
    let low = value * 12.92;
    let high = 1.055 * pow(max(value, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, value <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = in.position.xy;
    if p.x < in.clip.x || p.y < in.clip.y || p.x >= in.clip.z || p.y >= in.clip.w {
        discard;
    }
    let kind = i32(in.params.z + 0.5);
    var color = vec4<f32>(0.0);
    if kind == 0 {
        let half = in.size * 0.5;
        let radius = min(in.params.x, min(half.x, half.y));
        let distance = rounded_box(in.local - half, half, radius);
        let coverage = clamp(0.5 - distance, 0.0, 1.0);
        let fill = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
        var shaded = fill;
        if in.params.y > 0.0 {
            let border = vec4<f32>(in.border.rgb * in.border.a, in.border.a);
            let inner = clamp(0.5 - (distance + in.params.y), 0.0, 1.0);
            shaded = mix(border, fill, inner);
        }
        color = shaded * coverage;
    } else if kind == 1 {
        let coverage = textureSampleLevel(tex, samp, in.uv, 0.0).r;
        color = vec4<f32>(in.color.rgb, 1.0) * in.color.a * coverage;
    } else if kind == 2 {
        let texel = textureSampleLevel(tex, samp, in.uv, 0.0);
        color = vec4<f32>(texel.rgb * texel.a, texel.a) * in.color.a;
    } else {
        let gray = srgb_to_linear(textureSampleLevel(tex, samp, in.uv, 0.0).r);
        color = vec4<f32>(vec3<f32>(gray), 1.0) * in.color.a;
    }
    if globals.encode_srgb > 0.5 && color.a > 0.0 {
        color = vec4<f32>(linear_to_srgb(color.rgb / color.a) * color.a, color.a);
    }
    return color;
}
