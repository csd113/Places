override display_target: bool = false;

// Ambient effect billboards: steam plumes over the world.
//
// The vertices are world-space and already camera-facing (the CPU evaluates
// each billboard's basis every frame), so the vertex stage only transforms
// them. The fragment stage samples the effect sheet, multiplies its alpha by
// the per-vertex alpha, and outputs *straight* alpha: the pass blends with
// `SrcAlpha`/`OneMinusSrcAlpha`, which applies the alpha once, exactly like the
// world translucent variant.

struct Camera {
    view_projection: mat4x4<f32>,
    // The shared camera uniform also carries the eye. The effect stage does not
    // read it, but the layout stays byte-identical to the world and decal
    // camera uniforms so one upload can serve every pass that needs a camera.
    position: vec3<f32>,
    _padding: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var effect_texture: texture_2d<f32>;
@group(1) @binding(1) var effect_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_projection * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(effect_texture, effect_sampler, in.uv);
    return vec4<f32>(target_color(sampled.rgb * in.color.rgb), sampled.a * in.color.a);
}

// Only the uncommon raw 8-bit window fallback needs software encoding.
fn target_color(color: vec3<f32>) -> vec3<f32> {
    if display_target {
        let c = max(color, vec3<f32>(0.0));
        return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3<f32>(0.0031308));
    }
    return color;
}
