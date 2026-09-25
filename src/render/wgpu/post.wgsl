// Post shader: the fullscreen vertex stage shared by the two bloom blurs, the
// resolve and the plain present copy, and their fragment stages.
//
// Reproduces the reference's `PRESENT_VERTEX_SHADER_SRC`,
// `BLOOM_BLUR_FRAGMENT_SHADER_SRC` and `RESOLVE_FRAGMENT_SHADER_SRC`; every
// fragment entry point quotes the reference GLSL it reproduces. Two backend
// conventions differ and are spelled out where they are applied:
//
// * Colour space. The scene target is raw `Rgba8Unorm`, exactly like the
//   reference's RGBA8 scene: the world shader writes display-space values
//   directly, so the bloom add, exposure, tone and grade all land on the same
//   values the reference used. Only the write to the sRGB surface converts
//   (`srgb_to_linear`); the plain present copy does the same. The emissive and
//   blur targets are raw too, so the blur kernel runs on display values.
// * Coordinates. The reference's quad carries `(x, y)` in `[0, 1]` as both the
//   `present_matrix` input and the texture coordinate, on a bottom-up
//   framebuffer. WebGPU clips y-up but its framebuffer and texture rows are
//   top-down, so the same quad negates y for clip space: `v = 0` is the top row
//   of the source and lands at the top of the target.

struct PostParams {
    // The reference's `u_bloom_strength`.
    bloom_strength: f32,
    // The reference's `u_exposure`.
    exposure: f32,
    // The reference's `u_tone_knee`.
    tone_knee: f32,
    // The reference's `u_grade_saturation`.
    grade_saturation: f32,
    // The reference's `u_grade_contrast`.
    grade_contrast: f32,
    // Explicit tail padding; the struct is 32 bytes, 16-byte-aligned.
    _padding0: f32,
    _padding1: f32,
    _padding2: f32,
};

struct BlurParams {
    // One texel of the *source*, `xy`; `zw` is unused padding so the struct is
    // one 16-byte uniform slot.
    texel: vec4<f32>,
};

// Group 0 is the pass's source image and sampler. The resolve and the present
// copy bind the scene target; the first blur pass binds the full-size emissive
// image, the second binds blur buffer A.
@group(0) @binding(0)
var source_texture: texture_2d<f32>;
@group(0) @binding(1)
var source_sampler: sampler;

// The resolve parameters. Unused by the blur and present entry points, so their
// pipelines do not declare the binding.
@group(0) @binding(2)
var<uniform> post: PostParams;

// The blurred emissive image the resolve adds back. Bound but not sampled when
// `post.bloom_strength` is zero, exactly like the reference's zero strength.
@group(0) @binding(3)
var bloom_texture: texture_2d<f32>;
@group(0) @binding(4)
var bloom_sampler: sampler;

// The blur step. Unused by the resolve and present entry points.
@group(0) @binding(5)
var<uniform> blur: BlurParams;

struct QuadOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// The fullscreen quad. `corner` is the reference `PRESENT_QUAD`'s `(x, y)` in
// `[0, 1]`; `present_matrix` mapped it to `[0, 1] -> [-1, 1]` on both axes, and
// the y term is negated here for WebGPU's top-down framebuffer and texture
// rows.
@vertex
fn vs_main(@location(0) corner: vec2<f32>) -> QuadOut {
    var out: QuadOut;
    out.uv = corner;
    out.clip = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    return out;
}

// The IEC 61966-2-1 transfer functions, exact inverses of the world shader's.
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

// The scene copied straight into the raw presented image: no conversion, the
// reference's identity copy.
@fragment
fn fs_present_raw(in: QuadOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(source_texture, source_sampler, in.uv).rgb, 1.0);
}

// The reference's plain presentation copy:
//
//     gl_FragColor = vec4(texture2D(u_scene, v_uv).rgb, 1.0);
//
// The presented image is raw display space and the surface is sRGB, so the one
// conversion here is the output encode.
@fragment
fn fs_present(in: QuadOut) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, in.uv).rgb;
    return vec4<f32>(srgb_to_linear(color), 1.0);
}

// The reference's bloom blur, unchanged:
//
//     vec2 step0 = u_texel * 1.0;
//     vec2 step1 = u_texel * 2.0;
//     vec3 sum = texture2D(u_source, v_uv).rgb * 0.375;
//     sum += (texture2D(u_source, v_uv + step0).rgb + texture2D(u_source, v_uv - step0).rgb) * 0.25;
//     sum += (texture2D(u_source, v_uv + step1).rgb + texture2D(u_source, v_uv - step1).rgb) * 0.0625;
//     gl_FragColor = vec4(sum, 1.0);
//
// `u_texel` is one texel of the *source*: pass 1 reads the scene-sized emissive
// image into the quarter-size buffer A (so the down-sample and the horizontal
// blur are one pass), pass 2 reads buffer A vertically into buffer B. The
// source and target are raw RGBA8, so no transfer function is applied.
@fragment
fn fs_blur(in: QuadOut) -> @location(0) vec4<f32> {
    let step0 = blur.texel.xy * 1.0;
    let step1 = blur.texel.xy * 2.0;
    var sum = textureSample(source_texture, source_sampler, in.uv).rgb * 0.375;
    sum += (textureSample(source_texture, source_sampler, in.uv + step0).rgb
        + textureSample(source_texture, source_sampler, in.uv - step0).rgb) * 0.25;
    sum += (textureSample(source_texture, source_sampler, in.uv + step1).rgb
        + textureSample(source_texture, source_sampler, in.uv - step1).rgb) * 0.0625;
    return vec4<f32>(sum, 1.0);
}

// The reference's resolve, unchanged, in display space:
//
//     vec3 color = texture2D(u_scene, v_uv).rgb;
//     if (u_bloom_strength > 0.0) {
//         color += texture2D(u_bloom, v_uv).rgb * u_bloom_strength;
//     }
//     color *= u_exposure;
//     // Soft shoulder: identity at and below the knee, asymptotic to white above.
//     vec3 above = max(color - u_tone_knee, vec3(0.0));
//     float span = max(1.0 - u_tone_knee, 1.0e-3);
//     color = min(color, vec3(u_tone_knee)) + span * (above / (above + span));
//     float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
//     color = mix(vec3(luma), color, u_grade_saturation);
//     color = clamp((color - 0.5) * u_grade_contrast + 0.5, 0.0, 1.0);
//     gl_FragColor = vec4(color, 1.0);
//
// The one conversion is at the output (`srgb_to_linear`, because the resolve
// target is an sRGB surface); every knob between scene and surface lands on the
// reference's raw display-space values.
@fragment
fn fs_resolve_raw(in: QuadOut) -> @location(0) vec4<f32> {
    var color = textureSample(source_texture, source_sampler, in.uv).rgb;
    if (post.bloom_strength > 0.0) {
        color += textureSample(bloom_texture, bloom_sampler, in.uv).rgb * post.bloom_strength;
    }
    color *= post.exposure;
    let above = max(color - vec3<f32>(post.tone_knee), vec3<f32>(0.0));
    let span = max(1.0 - post.tone_knee, 1.0e-3);
    color = min(color, vec3<f32>(post.tone_knee)) + span * (above / (above + span));
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, post.grade_saturation);
    color = clamp((color - 0.5) * post.grade_contrast + 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_resolve(in: QuadOut) -> @location(0) vec4<f32> {
    var color = textureSample(source_texture, source_sampler, in.uv).rgb;
    if (post.bloom_strength > 0.0) {
        color += textureSample(bloom_texture, bloom_sampler, in.uv).rgb * post.bloom_strength;
    }
    color *= post.exposure;
    let above = max(color - vec3<f32>(post.tone_knee), vec3<f32>(0.0));
    let span = max(1.0 - post.tone_knee, 1.0e-3);
    color = min(color, vec3<f32>(post.tone_knee)) + span * (above / (above + span));
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, post.grade_saturation);
    color = clamp((color - 0.5) * post.grade_contrast + 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(srgb_to_linear(color), 1.0);
}
