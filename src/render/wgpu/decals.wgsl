// Decals: the reference's local surface markings, drawn as the last scene pass
// with a depth bias.
//
// The reference's decal program is its shared world vertex stage plus
// `DECAL_FRAGMENT_SHADER_SRC`: an unlit texture multiply with an alpha cut-out,
// no fog, no lightmap, no sheen and no reflection. Its decal sheets are plain
// (raw `Rgba8Unorm`) textures, so the sample is already the display-space value
// the reference multiplied; this shader keeps that display space for the raw
// scene target, and only the surface-facing entry point converts with
// `srgb_to_linear`, exactly like `world.wgsl`.

struct Camera {
    // Clip-space view-projection; wgpu depth range (z in [0, 1]).
    view_projection: mat4x4<f32>,
    // World-space eye position, the reference's `u_camera_pos`. The decal
    // fragment stage never reads it; it is declared because these are
    // `render::wgpu::world::CameraUniform`'s exact bytes.
    position: vec3<f32>,
    // Explicit tail padding so the struct is 80 bytes, 16-byte aligned.
    _padding: f32,
};

// Group 0 is the frame camera: the same uniform the world pass uploads, in the
// decal pass's own buffer so the two passes never share state.
@group(0) @binding(0)
var<uniform> camera: Camera;

// Group 1 is the decal sheet and the repeating sampler the player's filtering
// setting selects: the reference's REPEAT + mipmapped world-sheet policy for
// decal sheets.
@group(1) @binding(0)
var decal_texture: texture_2d<f32>;
@group(1) @binding(1)
var decal_sampler: sampler;

// The shared `WorldVertex` layout, from `world::world_vertex_layout`. The decal
// program uses position, UV and the quantised colour; the material frame
// attributes are declared so this input is the same object as the world's.
struct WorldVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(6) lightmap_uv: vec2<f32>,
    @location(7) lightmap_page: f32,
    @location(4) tangent: vec3<f32>,
    @location(5) handedness: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(vertex: WorldVertex) -> VsOut {
    var out: VsOut;
    // The reference never sets the decal program's `u_model`, and no decal
    // fragment term reads world position: decal geometry is already in world
    // space.
    out.clip = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    out.uv = vertex.uv;
    out.color = vertex.color;
    return out;
}

// The IEC 61966-2-1 transfer functions, the same two `world.wgsl` carries.
// Only `srgb_to_linear` runs here, on the finished product: an sRGB texel
// format would pre-decode the sheet and change the reference's multiply.
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// The reference's decal fragment, whose whole body is:
//
//     vec4 tex_color = texture2D(u_texture, v_uv);
//     if (tex_color.a < u_alpha_cutoff) discard;
//     gl_FragColor = tex_color * v_color;
//
// The cut-out tests the texture's alpha alone. Every decal quad's vertex alpha
// is 1.0 (`render::common::add_quad`), so the product could not change the
// test anyway; the written alpha still keeps the reference's
// `tex_color * v_color`.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(decal_texture, decal_sampler, in.uv);
    if (base.a < 0.5) {
        discard;
    }
    let alpha = base.a * in.color.a;
    // One conversion, on the display-space product.
    return vec4<f32>(srgb_to_linear(base.rgb * in.color.rgb), alpha);
}

// The same fragment for a raw (non-sRGB) scene or reflection target: the
// reference's framebuffer convention, written directly.
@fragment
fn fs_main_raw(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(decal_texture, decal_sampler, in.uv);
    if (base.a < 0.5) {
        discard;
    }
    let alpha = base.a * in.color.a;
    return vec4<f32>(base.rgb * in.color.rgb, alpha);
}
