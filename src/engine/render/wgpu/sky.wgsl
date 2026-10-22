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

struct Celestial { source: vec4<f32>, right: vec4<f32>, up: vec4<f32>, };

struct Sky {
    // Inverse clip-space view-projection; wgpu depth range (z in [0, 1]).
    inverse_view_projection: mat4x4<f32>,
    // x = brightness multiplier, yzw reserved (zero).
    params: vec4<f32>,
    storm: vec4<f32>,
    clear_color: vec4<f32>,
    celestial: array<Celestial, 8>,
};

// Group 0 is the sky pass's own uniform.
@group(0) @binding(0)
var<uniform> sky: Sky;

// Group 1 is the sky sheet and the sky sampler (repeat in U, clamp in V).
@group(1) @binding(0)
var sky_texture: texture_2d<f32>;
@group(1) @binding(1)
var sky_sampler: sampler;
@group(2) @binding(0) var sun_texture: texture_2d<f32>;
@group(2) @binding(1) var sun_sampler: sampler;
@group(3) @binding(0) var moon_texture: texture_2d<f32>;
@group(3) @binding(1) var moon_sampler: sampler;

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
fn view_direction(ndc: vec2<f32>) -> vec3<f32> {
    let near_h = sky.inverse_view_projection * vec4<f32>(ndc, 0.0, 1.0);
    let far_h = sky.inverse_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let near = near_h.xyz / near_h.w;
    let far = far_h.xyz / far_h.w;
    return normalize(far - near);
}

fn sky_uv(ndc: vec2<f32>) -> vec2<f32> {
    let direction = view_direction(ndc);
    let u = atan2(direction.x, -direction.z) * 0.15915494309189535 + 0.5;
    let v = acos(clamp(direction.y, -1.0, 1.0)) * 0.3183098861837907;
    return vec2<f32>(u, v);
}

fn sample_sky(ndc: vec2<f32>) -> vec4<f32> {
    let uv = sky_uv(ndc);
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    // atan2 jumps by one turn at the panorama join. Remove that jump from
    // the footprint so both sides select the same mip while retaining filtering.
    let continuous_dx = vec2<f32>(dx.x - round(dx.x), dx.y);
    let continuous_dy = vec2<f32>(dy.x - round(dy.x), dy.y);
    return textureSampleGrad(sky_texture, sky_sampler, uv, continuous_dx, continuous_dy);
}

// Immutable level projections; no generated imagery, geometry rebuild or lighting solve.
fn background(ndc: vec2<f32>) -> vec3<f32> {
    let sheet = sample_sky(ndc).rgb * sky.params.x;
    var color = select(sky.clear_color.rgb, sheet, sky.params.y > 0.5);
    let ray = view_direction(ndc);
    for (var i = 0u; i < min(u32(sky.params.z), 8u); i = i + 1u) {
        let body = sky.celestial[i];
        let facing = dot(ray, body.source.xyz);
        if (body.source.w > 0.0 && facing > 0.0) {
            let uv = vec2<f32>(dot(ray, body.right.xyz), -dot(ray, body.up.xyz)) * body.source.w / facing + 0.5;
            if (all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))) {
                var image: vec4<f32>;
                if (body.up.w > 0.5) { image = textureSampleLevel(moon_texture, moon_sampler, uv, 0.0); }
                else { image = textureSampleLevel(sun_texture, sun_sampler, uv, 0.0); }
                color = mix(color, image.rgb * body.right.w, image.a);
            }
        }
    }
    return mix(color, srgb_to_linear(sky.storm.rgb), sky.storm.a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(background(in.ndc), 1.0);
}
@fragment
fn fs_main_raw(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(target_color(background(in.ndc)), 1.0);
}

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055, color * 12.92, color <= vec3<f32>(0.0031308));
}
fn target_color(color: vec3<f32>) -> vec3<f32> {
    if (display_target) { return linear_to_srgb(color); }
    return color;
}
