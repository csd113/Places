//! The GLSL sources, attribute slots and texture-unit assignments of the
//! OpenGL backend.
//!
//! Everything in this file is OpenGL-specific: the shader text is GLSL ES 1.00,
//! the attribute slots are bound before linking by `renderer::create_program`,
//! and the texture units are the numbers `glActiveTexture` selects. The
//! renderer-neutral view maths lives in `crate::render::common::view`.

pub const VERTEX_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
attribute vec4 a_color;
attribute vec2 a_uv;
attribute vec2 a_lightmap_uv;
attribute float a_lightmap_page;
attribute vec3 a_normal;
attribute vec3 a_tangent;
attribute float a_handedness;
uniform mat4 u_mvp;
// Model transform of the object being drawn, identity for the static world.
// Normals and the world position the surface-response term needs are taken
// through it, so the dynamic-object path can shade a rigidly transformed mesh
// with the same program.
uniform mat4 u_model;
varying vec4 v_color;
varying vec2 v_uv;
varying vec2 v_lightmap_uv;
varying float v_lightmap_page;
varying vec3 v_world_pos;
varying vec3 v_normal;
varying vec3 v_tangent;
varying float v_handedness;

void main() {
    v_color = a_color;
    v_uv = a_uv;
    v_lightmap_uv = a_lightmap_uv;
    v_lightmap_page = a_lightmap_page;
    v_world_pos = (u_model * vec4(a_pos, 1.0)).xyz;
    // The frame goes through the model's rotation. A mat4 multiply (rather than
    // a mat3 constructor) is what GLSL 110 and GLSL ES 100 both accept; the
    // model transform is a rigid motion, so the vectors stay unit length.
    v_normal = (u_model * vec4(a_normal, 0.0)).xyz;
    v_tangent = (u_model * vec4(a_tangent, 0.0)).xyz;
    v_handedness = a_handedness;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
";

/// The world fragment stage's body, compiled twice.
///
/// `u_alpha_cutoff` is only *used* when the source is compiled with
/// `ALPHA_CUTOUT` defined: a `discard` inside a program can disable early depth
/// testing for every draw that uses it, and the opaque world must keep it. The
/// same body therefore becomes two programs ([`fragment_shader_source`]) rather
/// than one branch.
const FRAGMENT_SHADER_BODY: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
uniform sampler2D u_emission_mask;
uniform sampler2D u_normal_map;
uniform sampler2D u_lightmap0;
uniform sampler2D u_lightmap1;
uniform vec3 u_emission_color;
uniform float u_emission_mask_enabled;
uniform float u_emission_vertex;
// Animated emission: a level can make a sign breathe or a tube stutter. The
// multiplier is 1.0 for every surface that declares no animation, and it scales
// the emissive term alone — never the baked light, the albedo or the alpha.
uniform float u_emission_scale;
uniform float u_lightmap_enabled;
uniform vec3 u_light_scale;
// Surface response. `u_response_enabled` is the master gate: it is zero for
// every material without a normal map or a sheen, for the HUD, and for the
// whole scene under the Low quality profile.
uniform float u_response_enabled;
uniform float u_normal_enabled;
uniform float u_normal_strength;
uniform vec3 u_specular;
uniform float u_roughness;
// Alpha. `u_opacity` scales the sampled alpha; `u_alpha_cutoff` is the
// cut-out threshold of the ALPHA_CUTOUT program.
uniform float u_opacity;
uniform float u_alpha_cutoff;
uniform vec3 u_camera_pos;
// Atmosphere. Exponential-squared distance fog with a mild height term, so
// depth reads inside the building without the interior turning milky. The
// density is a frame uniform: the HUD sets it to zero.
uniform vec3 u_fog_color;
uniform float u_fog_density;
uniform float u_fog_reference_y;
uniform float u_fog_height_gain;
// Reflections. `u_reflect_mode` is 0 for every material that authors no
// reflection, 1 for a static probe cubemap and 2 for a planar mirror surface.
// `u_reflect_strength` already folds the authored strength and the material's
// specular colour; roughness (the inverse of the authored shine) and the view
// angle are applied here, so a rough surface suppresses the response instead of
// mirroring.
uniform samplerCube u_probe_map;
uniform sampler2D u_planar_map;
uniform float u_reflect_mode;
uniform vec3 u_reflect_strength;
uniform mat4 u_planar_matrix;
// Plane of the active planar reflection: `xyz` is the unit normal, `w` the
// signed distance from the origin (`dot(normal, point) + w == 0`).
uniform vec4 u_planar_plane;
// The bloom path draws the emissive term alone into a quarter-resolution
// target. It is zero for every ordinary frame draw.
uniform float u_emission_only;
varying vec4 v_color;
varying vec2 v_uv;
varying vec2 v_lightmap_uv;
varying float v_lightmap_page;
varying vec3 v_world_pos;
varying vec3 v_normal;
varying vec3 v_tangent;
varying float v_handedness;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    vec3 mask = vec3(1.0);
    if (u_emission_mask_enabled > 0.5) {
        mask = texture2D(u_emission_mask, v_uv).rgb;
    }
    vec3 emission =
        mix(u_emission_color, v_color.rgb, u_emission_vertex) * mask * tex_color.rgb * u_emission_scale;
    float alpha = tex_color.a * v_color.a * u_opacity;
#ifdef ALPHA_CUTOUT
    if (alpha < u_alpha_cutoff) {
        discard;
    }
#endif
    // The bloom pass wants the emissive term and nothing else, before any
    // lighting, fog or tone work: a fixture blooms because it emits, never
    // because it is brightly lit.
    if (u_emission_only > 0.5) {
        gl_FragColor = vec4(emission, 1.0);
        return;
    }
    // The atlas is read only when this fragment actually takes its light from
    // it: the global switch is on while a real bake is resident, and a vertex
    // whose page byte is `LIGHTMAP_NONE` (>= 254.5) keeps the light already in
    // its colour. Inside the guard the two pages are selected branchlessly,
    // because a batch can mix pages. (A driver may still resolve both declared
    // samplers for the draw, which is why the units are bound to complete
    // textures even when no fragment ends up reading them.)
    float lightmap_on = u_lightmap_enabled * (1.0 - step(254.5, v_lightmap_page));
    vec3 light = vec3(1.0);
    if (lightmap_on > 0.5) {
        vec3 lm = mix(
            texture2D(u_lightmap0, v_lightmap_uv).rgb,
            texture2D(u_lightmap1, v_lightmap_uv).rgb,
            step(0.5, v_lightmap_page)
        );
        light = lm;
    }
    light *= u_light_scale;

    // The geometric normal, made to face the viewer: nothing in the world is
    // back-face culled, so a wall seen from behind must still shade.
    vec3 normal = normalize(v_normal);
    if (!gl_FrontFacing) {
        normal = -normal;
    }
    if (u_response_enabled > 0.5 && u_normal_enabled > 0.5) {
        vec3 tangent_space = texture2D(u_normal_map, v_uv).xyz * 2.0 - 1.0;
        tangent_space.xy *= u_normal_strength;
        vec3 tangent = normalize(v_tangent - normal * dot(normal, v_tangent));
        vec3 bitangent = cross(normal, tangent) * v_handedness;
        normal = normalize(
            tangent * tangent_space.x + bitangent * tangent_space.y + normal * tangent_space.z
        );
    }

    // View-dependent sheen: how much of the already-baked light a surface
    // catches as it turns away from the camera. Roughness decides how tightly
    // that concentrates; there is no light direction to sample, because the
    // engine has no realtime lights.
    vec3 view = normalize(u_camera_pos - v_world_pos);
    vec3 sheen = vec3(0.0);
    if (u_response_enabled > 0.5) {
        float facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
        float gloss = 1.0 - u_roughness;
        float grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
        // The tight near-normal lobe is scaled by the gloss itself: every
        // surface catches the room's light towards its silhouette, but only a
        // polished one keeps a bright glow when looked at head-on. A rough
        // surface that kept that flat glow is what made dull metal read as
        // grey plastic.
        float ahead = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
        // The engine has no light direction to place a real highlight, so both
        // lobes are scaled by the baked light and neither invents a source.
        sheen = u_specular * (grazing * 0.55 + ahead * 0.45) * light;
    }

    // Reflections. A planar surface projects the reflected frame through the
    // mirrored camera; a probe surface reads the static cubemap baked at load.
    // Both are weighted by the authored strength, the material's specular
    // colour, a Fresnel term and the gloss, so a rough or dull surface never
    // behaves like a mirror: at low shine only the grazing silhouette catches
    // the image, and what it catches is a broad average rather than a sharp
    // reflection of the room.
    vec3 reflection = vec3(0.0);
    if (u_reflect_mode > 0.5) {
        float facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
        float fresnel = mix(0.08, 1.0, pow(1.0 - facing, 5.0));
        float gloss = clamp(1.0 - u_roughness, 0.0, 1.0);
        float polish = gloss * gloss;
        // A polished surface reflects across the whole face; a dull one only
        // reaches the image near grazing angles, and then at reduced weight.
        float weight = mix(fresnel * 0.35, 1.0, polish);
        vec3 sample_color = vec3(0.0);
        if (u_reflect_mode > 1.5) {
            vec4 clip = u_planar_matrix * vec4(v_world_pos, 1.0);
            vec2 uv = clip.xy / max(clip.w, 1.0e-4) * 0.5 + 0.5;
            // A rough surface reads a small disc around the projected point; a
            // polished one reads the single texel the plane projects to.
            float blur = u_roughness * 0.035;
            if (blur > 0.002) {
                sample_color += texture2D(u_planar_map, uv + vec2(blur, blur)).rgb;
                sample_color += texture2D(u_planar_map, uv + vec2(-blur, blur)).rgb;
                sample_color += texture2D(u_planar_map, uv + vec2(blur, -blur)).rgb;
                sample_color += texture2D(u_planar_map, uv + vec2(-blur, -blur)).rgb;
                sample_color *= 0.25;
            } else {
                sample_color = texture2D(u_planar_map, uv).rgb;
            }
            // Outside the reflected frame there is no image to show.
            float inside =
                step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
            // Only a fragment on (or very near) the mirror plane reflects.
            float on_plane = 1.0 - smoothstep(
                0.0,
                0.08,
                abs(dot(u_planar_plane.xyz, v_world_pos) + u_planar_plane.w)
            );
            weight *= inside * on_plane;
        } else {
            vec3 reflected = reflect(-view, normal);
            vec3 sharp = textureCube(u_probe_map, reflected).rgb;
            // A rough surface averages a wider cone of the room than a single
            // reflected ray would: mixing the reading towards the surface's own
            // facing direction stands in for a blurred cubemap read without a
            // mip chain, a second render or a per-tap kernel.
            if (u_roughness > 0.15) {
                vec3 broad =
                    textureCube(u_probe_map, normalize(mix(reflected, normal, 0.5))).rgb;
                sharp = mix(sharp, broad, u_roughness);
            }
            sample_color = sharp;
        }
        reflection = u_reflect_strength * weight * sample_color;
    }

    vec3 lit = tex_color.rgb * v_color.rgb * light * (1.0 - u_emission_vertex);
    vec3 color = lit + sheen + reflection + emission;
    // Distance fog, applied to the finished surface colour: emission is a
    // surface property, not a hole punched through the air.
    float distance = length(u_camera_pos - v_world_pos);
    float below = max(0.0, u_fog_reference_y - v_world_pos.y);
    float density = u_fog_density * (1.0 + u_fog_height_gain * min(below, 12.0));
    float fog_amount = density * distance;
    fog_amount = 1.0 - exp(-fog_amount * fog_amount);
    color = mix(color, u_fog_color, clamp(fog_amount, 0.0, 1.0));
    gl_FragColor = vec4(color, alpha);
}
";

/// The world fragment source for one program variant.
///
/// `cutout` prepends the `ALPHA_CUTOUT` define so the alpha-tested program
/// carries the `discard` and the opaque one does not. Compiled once at startup.
#[must_use]
pub fn fragment_shader_source(cutout: bool) -> String {
    let define = if cutout {
        "#define ALPHA_CUTOUT 1\n"
    } else {
        ""
    };
    format!("{define}{FRAGMENT_SHADER_BODY}")
}

/// Fragment stage of the world pass.
///
/// Three independent terms make a pixel:
///
/// * **Lit** — `texture x vertex colour x baked light`, where the baked light is
///   either the lightmap atlas or, for a vertex that carries no lightmap
///   coordinates (`v_lightmap_page >= 254.5`), the light already folded into the
///   vertex colour by the historical bake. `u_lightmap_enabled` is the global
///   switch: with lightmaps unavailable it is zero and every vertex takes the
///   vertex-lit path, which is what makes the fallback exact rather than
///   approximate. The atlas is only sampled inside the guard that selects
///   between those two paths.
/// * **Emission** — the material's own brightness, added on top and never
///   multiplied by the light. A dark room cannot extinguish it, and it cannot
///   brighten anything else: emission is not a light source.
/// * **Sheen** — the material's lightweight surface response: a view-dependent
///   Fresnel term scaled by the baked light, optionally perturbed by a normal
///   map. See [`crate::materials::response`]. Zero for every material that
///   authors none. Its `u_specular` colour is the material's identity and its
///   `u_roughness` is `1 - shine`: the tight lobe shrinks with the gloss, so a
///   matte surface keeps the broad grazing sheen and loses the flat face-on
///   glow.
/// * **Reflection** — `u_reflect_mode` selects the static probe or the planar
///   image, weighted by `u_reflect_strength` (already the material's specular
///   colour times its authored strength), a Fresnel term and the same gloss.
///   A rough surface only catches the image near grazing angles and reads a
///   broad cone of it; a polished one reflects across the whole face.
/// * **Dynamic probe** — `u_light_scale` is `1` for static geometry. The
///   dynamic-object path sets it to the baked light sampled at the object's
///   current position, so a moving object is shaded coherently without touching
///   its vertex buffer.
///
/// `u_emission_vertex` selects which value feeds the emissive term. Ordinary
/// materials use the per-batch `u_emission_color` (their `emissive x
/// intensity`, modulated by the emissive mask when one is bound, and by the
/// surface texture so artwork shapes the glow). Fixture faces use `1.0`, which
/// makes the per-vertex colour the emission source: their glow is per instance
/// (a neutral emission strength, so the sheet keeps its own colour) while the
/// batch stays shared, and the lit term is multiplied by zero so exactly one
/// term remains.
///
/// With no emission colour, no mask, no response, `u_emission_vertex = 0` and no
/// lightmap — every material authored before either existed — the added terms
/// are zero and the output is the historical
/// `texture2D(u_texture, v_uv) * v_color` with an opaque alpha.
///
/// Fragment stage of the offscreen presentation pass follows.: one textured, unlit quad.
///
/// The scene is rendered into an offscreen colour attachment and presented to
/// the default framebuffer by this pass. It applies no grading, no scaling and
/// no sampling tricks — `texture2D` with `NEAREST` wrapping and no mip chain —
/// so what the scene pass drew is what the window shows.
pub const PRESENT_FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_scene;
varying vec2 v_uv;

void main() {
    gl_FragColor = vec4(texture2D(u_scene, v_uv).rgb, 1.0);
}
";

/// Vertex stage of the presentation pass: a screen-covering quad.
///
/// The quad's corners are the only geometry; `u_mvp` scales them from `[0, 1]`
/// to the whole drawable, and the vertex's own position is its texture
/// coordinate, so the scene texture is presented exactly once over the
/// framebuffer with no distortion and no letterbox maths.
pub const PRESENT_VERTEX_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
uniform mat4 u_mvp;
varying vec2 v_uv;

void main() {
    v_uv = a_pos.xy;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
";

/// Fragment stage of the resolve pass: the scene colour, plus bloom, exposed
/// and tone-mapped, with an optional grain-free colour grade.
///
/// This replaces the plain copy the offscreen path used to end with, so the
/// tone curve costs no extra pass over the pixels. It is the *only* place the
/// scene's exposure and tone are decided: the world shader writes linear-ish
/// surface colour and this pass turns it into a display image. The HUD is drawn
/// after it, on the default framebuffer, and never passes through here.
///
/// The shoulder is deliberately gentle: everything below `u_tone_knee` passes
/// through unchanged, so the baked lighting's own contrast — which is the art
/// direction — survives exactly, and only the genuinely over-bright pixels (an
/// emissive fixture face at intensity > 1) roll off. `u_bloom_strength` is zero
/// when no bloom target was drawn, which is what the Low profile and a frame
/// with no emissive geometry do.
pub const RESOLVE_FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_scene;
uniform sampler2D u_bloom;
uniform float u_bloom_strength;
uniform float u_exposure;
uniform float u_tone_knee;
uniform float u_grade_saturation;
uniform float u_grade_contrast;
varying vec2 v_uv;

void main() {
    vec3 color = texture2D(u_scene, v_uv).rgb;
    if (u_bloom_strength > 0.0) {
        color += texture2D(u_bloom, v_uv).rgb * u_bloom_strength;
    }
    color *= u_exposure;
    // Soft shoulder: identity at and below the knee, asymptotic to white above.
    vec3 above = max(color - u_tone_knee, vec3(0.0));
    float span = max(1.0 - u_tone_knee, 1.0e-3);
    color = min(color, vec3(u_tone_knee)) + span * (above / (above + span));
    float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
    color = mix(vec3(luma), color, u_grade_saturation);
    color = clamp((color - 0.5) * u_grade_contrast + 0.5, 0.0, 1.0);
    gl_FragColor = vec4(color, 1.0);
}
";

/// Fragment stage of a separable bloom blur: a five-tap Gaussian.
///
/// `u_texel` is one source texel along the blur axis, so the same program runs
/// horizontally and vertically. The taps are at whole texels on purpose: the
/// source is already a quarter-resolution emissive image, and this only has to
/// turn a fixture face into a soft pool of light.
pub const BLOOM_BLUR_FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_source;
uniform vec2 u_texel;
varying vec2 v_uv;

void main() {
    vec2 step0 = u_texel * 1.0;
    vec2 step1 = u_texel * 2.0;
    vec3 sum = texture2D(u_source, v_uv).rgb * 0.375;
    sum += (texture2D(u_source, v_uv + step0).rgb + texture2D(u_source, v_uv - step0).rgb) * 0.25;
    sum += (texture2D(u_source, v_uv + step1).rgb + texture2D(u_source, v_uv - step1).rgb) * 0.0625;
    gl_FragColor = vec4(sum, 1.0);
}
";

/// Fragment stage for the decal pass: the same lit, textured look as the world
/// shader, plus an alpha cut-out so a decal can have a silhouette instead of
/// being a floating rectangle.
///
/// Decals keep the world program's vertex stage, so the two programs share
/// attribute locations ([`create_program`] binds them explicitly). This is a
/// second program rather than a branch in the world shader because `discard`
/// can disable early depth testing for every draw that uses the program, and
/// the opaque world must keep it.
pub const DECAL_FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
uniform float u_alpha_cutoff;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    if (tex_color.a < u_alpha_cutoff) {
        discard;
    }
    gl_FragColor = tex_color * v_color;
}
";

/// Depth bias the decal pass applies, as `glPolygonOffset(factor, units)`.
///
/// This is the *depth-buffer* half of the decal depth solution; the geometry
/// half is [`DECAL_SURFACE_OFFSET_M`]. The bias is negative on both terms so it
/// pulls a decal towards the camera:
///
/// * `units = -4` moves a decal four depth-buffer resolution steps towards the
///   viewer. A coplanar decal needs only a couple of steps in the ideal case,
///   but the depth a rasteriser interpolates for two different tessellations of
///   the *same* plane routinely disagrees by more than that: the plane
///   coefficients are fitted from different triangles, so the error grows with
///   the depth slope and the triangle size. Four units keeps the marking in
///   front of its parent surface in the near and mid field.
/// * `factor = -1.0` adds one depth-slope of bias, which is what keeps a decal
///   winning at grazing angles and at long range, where the constant term is
///   below the buffer's resolution. A slope-scaled term is exactly what a
///   coplanar decoration needs: the interpolation disagreement is proportional
///   to the depth slope too, so the bias tracks it instead of being outrun by
///   it as the camera changes distance and angle.
///
/// Both are window-depth offsets, not a physical separation, so they cannot
/// make a decal hang in the air. [`DECAL_SURFACE_OFFSET_M`] checks the rendered
/// result's near-field ordering; this bias carries the far field and grazing
/// angles, where no sub-millimetre physical offset is resolvable.
pub const DECAL_POLYGON_OFFSET: (f32, f32) = (-1.0, -4.0);

/// Alpha below which the decal pass discards a decal texel.
pub const DECAL_ALPHA_CUTOFF: f32 = 0.5;

/// Attribute indices both scene programs bind before linking, so switching
/// between the world and decal programs never re-points vertex attributes.
pub const SCENE_ATTRIB_POS: u32 = 0;
pub const SCENE_ATTRIB_COLOR: u32 = 1;
pub const SCENE_ATTRIB_UV: u32 = 2;
/// Lightmap atlas coordinates, as two normalized unsigned shorts.
pub const SCENE_ATTRIB_LIGHTMAP_UV: u32 = 3;
/// Lightmap atlas page, as one plain unsigned byte.
pub const SCENE_ATTRIB_LIGHTMAP_PAGE: u32 = 4;
/// Geometric normal, as three normalized signed bytes.
pub const SCENE_ATTRIB_NORMAL: u32 = 5;
/// Surface tangent, as three normalized signed bytes.
pub const SCENE_ATTRIB_TANGENT: u32 = 6;
/// Bitangent sign (±1), as one normalized signed byte.
pub const SCENE_ATTRIB_HANDEDNESS: u32 = 7;

/// Number of vertex attributes the scene programs consume.
///
/// Exactly eight, the OpenGL ES 2.0 guaranteed minimum
/// (`GL_MAX_VERTEX_ATTRIBS >= 8`), which is why the frame is packed into three
/// signed bytes per vector and one for the sign rather than into floats.
pub const SCENE_ATTRIB_COUNT: usize = 8;

/// Texture unit the world pass samples a surface's own sheet from.
pub const SCENE_TEXTURE_UNIT: i32 = 0;
/// Texture unit the world pass samples a material's normal map from.
///
/// Like the mask unit it always has a texture bound (the white sheet when a
/// material authors no normal map), and the material's `u_normal_enabled`
/// uniform decides whether the fragment stage reads it at all.
pub const NORMAL_MAP_TEXTURE_UNIT: i32 = 4;

/// Texture unit the offscreen presentation pass samples the scene colour from.
pub const PRESENT_TEXTURE_UNIT: i32 = 0;

/// One texture unit expressed as the `GL_TEXTURE0 + n` constant GL wants.
///
/// The units in this module are small non-negative constants, so the conversion
/// cannot fail or wrap; the fallback keeps the expression total anyway.
#[must_use]
pub fn texture_unit(unit: i32) -> u32 {
    #[allow(clippy::cast_sign_loss)] // the units here are non-negative by definition
    let unit = unit.max(0) as u32;
    glow::TEXTURE0.saturating_add(unit)
}

/// Texture unit the world pass samples a static reflection probe cubemap from.
///
/// Always has a cube bound (a one-pixel black cube when the renderer has no
/// probe resident), and `u_reflect_mode` decides whether the fragment stage
/// reads it at all.
pub const REFLECTION_PROBE_TEXTURE_UNIT: i32 = 5;
/// Texture unit the world pass samples the planar reflection target from.
///
/// Bound exactly like the probe unit: a one-pixel black texture when no planar
/// reflection is being drawn this frame.
pub const REFLECTION_PLANAR_TEXTURE_UNIT: i32 = 6;
/// Texture unit the resolve pass samples the blurred bloom image from.
pub const BLOOM_TEXTURE_UNIT: i32 = 1;

/// Texture unit the world pass samples a material's emissive mask from.
///
/// The unit always has a bound texture (the shared white sheet when a batch has
/// no mask), so a shader that samples it anyway still reads a defined value.
pub const EMISSION_MASK_TEXTURE_UNIT: i32 = 1;
/// First texture unit of the baked lightmap atlas pages.
///
/// Two pages are bound at once ([`LIGHTMAP_PAGE_SLOTS`] = 2) and the vertex's
/// page byte selects between them, so a level that needs a second page does not
/// split its batches. Both units always have a texture bound — the white sheet
/// when no lightmap is resident — so a world draw without lightmaps is defined.
pub const LIGHTMAP_TEXTURE_UNIT: i32 = 2;

/// Number of lightmap atlas pages the world pass can sample at once.
///
/// Two units (2 and 3) are bound for every world draw and the vertex's page byte
/// selects between them. A level whose bake needs more pages than this cannot
/// render its lightmaps correctly, so the lightmap build fails over to the
/// vertex-lit path instead of dropping pages silently; see
/// `crate::lighting::lightmap::LightmapConfig::max_pages`.
pub const LIGHTMAP_PAGE_SLOTS: usize = 2;

/// Second lightmap texture unit: [`LIGHTMAP_TEXTURE_UNIT`] + 1.
pub const LIGHTMAP_TEXTURE_UNIT_1: i32 = LIGHTMAP_TEXTURE_UNIT + 1;
