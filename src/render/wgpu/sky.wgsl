override display_target: bool = false;

// Night sky: the level's data-driven background, drawn as one fullscreen
// background pass before the world body.
//
// The sheet is equirectangular (2:1): `u` is yaw, wrapping at the seam, and
// `v` is pitch, `0` looking straight up and `1` straight down. A level only
// draws this pass when it declares a `sky`; without one the surface clear
// colour stays the background, exactly as before. The sky is a background, not
// a light: nothing here contributes illumination, and a solid ceiling always
// covers it because the world body draws over this pass with depth testing on.
//
// The sRGB sheet decodes at sampling, and brightness multiplies linear RGB.
// Storm display colour decodes before mixing into the linear HDR background.

struct Sky {
    // Inverse clip-space view-projection; wgpu depth range (z in [0, 1]).
    inverse_view_projection: mat4x4<f32>,
    // x = brightness multiplier, yzw reserved (zero).
    params: vec4<f32>,
    storm: vec4<f32>,
};

// Group 0 is the sky pass's own uniform.
@group(0) @binding(0)
var<uniform> sky: Sky;

// Group 1 is the sky sheet and the sky sampler (repeat in U, clamp in V).
@group(1) @binding(0)
var sky_texture: texture_2d<f32>;
@group(1) @binding(1)
var sky_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// A fullscreen triangle, so the background pass is one draw with no vertex or
// index buffer at all.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    let position = positions[index];
    out.clip = vec4<f32>(position, 1.0, 1.0);
    out.ndc = position;
    return out;
}

// The IEC 61966-2-1 transfer function, the same one `decals.wgsl` carries.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// The view direction of one pixel, and its equirectangular coordinate.
fn sky_uv(ndc: vec2<f32>) -> vec2<f32> {
    let near_h = sky.inverse_view_projection * vec4<f32>(ndc, 0.0, 1.0);
    let far_h = sky.inverse_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let near = near_h.xyz / near_h.w;
    let far = far_h.xyz / far_h.w;
    let direction = normalize(far - near);
    let u = atan2(direction.x, -direction.z) * 0.15915494309189535 + 0.5;
    let v = acos(clamp(direction.y, -1.0, 1.0)) * 0.3183098861837907;
    return vec2<f32>(u, v);
}

// The surface (sRGB) entry point: the authored display value converted once.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(sky_texture, sky_sampler, sky_uv(in.ndc));
    return vec4<f32>(mix(base.rgb * sky.params.x, srgb_to_linear(sky.storm.rgb), sky.storm.a), 1.0);
}

// The raw (non-sRGB) scene-target entry point: the authored value written
// directly, the same convention the world and decal raw stages use.
@fragment
fn fs_main_raw(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(sky_texture, sky_sampler, sky_uv(in.ndc));
    return vec4<f32>(target_color(mix(base.rgb * sky.params.x, srgb_to_linear(sky.storm.rgb), sky.storm.a)), 1.0);
}

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055, color * 12.92, color <= vec3<f32>(0.0031308));
}

fn target_color(color: vec3<f32>) -> vec3<f32> {
    if (display_target) { return linear_to_srgb(color); }
    return color;
}
