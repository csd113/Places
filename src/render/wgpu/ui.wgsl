// Stage 9 renderer-owned UI shader: the 480x272 HUD pass.
//
// The wgpu port of the OpenGL reference's `render_ui`, which draws the HUD with
// its World program reset to a neutral material state. This pipeline is that
// neutral state made explicit instead of inherited:
//
// * `camera.view_projection` is the reference's orthographic projection in the
//   480x272 UI pixel space: (0, 0) is the top-left corner and +y points down.
//   The CPU applies the same single clip-space correction the world path uses
//   (`render::wgpu::world::clip_correction`); it only remaps z, which this
//   depth-test-free pass never reads.
// * The vertex stage reads only the position, UV and RGBA colour of the shared
//   world vertex layout, with the colour quantised to bytes exactly like the
//   reference upload.
// * The fragment stage is exactly `texture2D(u_texture, v_uv) * v_color`: the
//   font atlas is raw display-space artwork (uploaded `Rgba8Unorm`, so no
//   hardware decode), the vertex colour is the reference's shade, and the one
//   IEC 61966-2-1 conversion produces the linear value the sRGB target's
//   hardware encode turns back into the reference's displayed byte.
// * Alpha is straight (`tex.a * color.a`, never multiplied into the colour):
//   the pipeline's `SrcAlpha`/`OneMinusSrcAlpha` blend applies it, exactly like
//   the reference's single `glBlendFunc` call.
//
// There is no world material state here at all: no atlas sampling, no surface
// response, no added terms, no cut-out discard and no fog mix. None of it can
// leak in, because none of it is declared.

struct Camera {
    // Clip-space UI orthographic projection.
    view_projection: mat4x4<f32>,
    // Unused by the HUD; the shared `CameraUniform` layout occupies the bytes.
    position: vec3<f32>,
    // Explicit tail padding so the struct is 80 bytes, 16-byte aligned.
    _padding: f32,
};

// Group 0 is the frame camera, the same binding the world pipeline uses.
@group(0) @binding(0)
var<uniform> camera: Camera;

// Group 1 is the renderer-owned font atlas and its clamped nearest sampler;
// both are created and bound once, never per frame.
@group(1) @binding(0)
var font_texture: texture_2d<f32>;
@group(1) @binding(1)
var font_sampler: sampler;

struct UiVertex {
    // Position in 480x272 UI pixels, (0, 0) top-left, +y down.
    @location(0) position: vec3<f32>,
    // Atlas UV; the reserved white cell draws solid rectangles and frames.
    @location(2) uv: vec2<f32>,
    // RGBA shade, quantised to bytes exactly like the reference upload.
    @location(3) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(vertex: UiVertex) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    out.uv = vertex.uv;
    out.color = vertex.color;
    return out;
}

// The IEC 61966-2-1 transfer function: the one conversion the surface-facing
// entry point needs, because the font artwork is display-space bytes and an
// sRGB surface decodes what it is written.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex_color = textureSample(font_texture, font_sampler, in.uv);
    // The reference's whole UI fragment: `texture2D(u_texture, v_uv) *
    // v_color`, assembled in display space.
    let display = tex_color.rgb * in.color.rgb;
    // The reference's alpha, untouched by the colour conversion.
    let alpha = tex_color.a * in.color.a;
    // This target is sRGB and the font art is display space, so the value is
    // converted once; nothing else is applied, and the blend applies the alpha.
    return vec4<f32>(srgb_to_linear(display), alpha);
}

// The same fragment for the raw presented target: the reference's own
// framebuffer convention. The hardware blend then applies alpha in display
// space, exactly like the reference's `glBlendFunc` on its RGBA8 framebuffer,
// and the presented image is encoded once when it is copied to the surface.
@fragment
fn fs_main_raw(in: VsOut) -> @location(0) vec4<f32> {
    let tex_color = textureSample(font_texture, font_sampler, in.uv);
    return vec4<f32>(tex_color.rgb * in.color.rgb, tex_color.a * in.color.a);
}
