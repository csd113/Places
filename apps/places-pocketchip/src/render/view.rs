//! Window, drawable, viewport and shader declarations.
//!
//! The reference resolution, the drawable-size and UI-viewport maths, the field
//! of view for a non-reference aspect, and the three GLSL programs the renderer
//! compiles once at startup.

/// The resolution 2D UI geometry is authored in (`PocketCHIP` baseline).
///
/// This is a *reference space*, not a window size: the UI is scaled to the
/// drawable by [`DrawableSize::ui_viewport`], and the window the game opens at
/// is [`crate::settings::DEFAULT_WINDOW_WIDTH`] x
/// [`crate::settings::DEFAULT_WINDOW_HEIGHT`]. The Low quality profile also
/// caps its internal scene width at this reference, which is the one runtime
/// place it is used outside the UI.
pub const UI_REFERENCE_WIDTH: u32 = 480;
pub const UI_REFERENCE_HEIGHT: u32 = 272;

/// Physical size (in pixels) of the current drawable/framebuffer.
///
/// This is deliberately distinct from the window's logical size: on `HiDPI`
/// displays such as macOS Retina the drawable is larger than the window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawableSize {
    pub width: u32,
    pub height: u32,
}

impl DrawableSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// True when the surface cannot be rendered to (minimized/hidden windows).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Aspect ratio derived from the real framebuffer, safe against zero height.
    #[must_use]
    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            dimension_f32(self.width) / dimension_f32(self.height)
        }
    }

    /// Pixel size of the integer-scaled UI region that fits this drawable while
    /// preserving the 480x272 reference aspect ratio, plus its bottom-left origin.
    #[must_use]
    pub fn ui_viewport(self) -> UiViewport {
        if self.is_empty() {
            return UiViewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                scale: 1.0,
            };
        }

        let scale = (dimension_f32(self.width) / dimension_f32(UI_REFERENCE_WIDTH))
            .min(dimension_f32(self.height) / dimension_f32(UI_REFERENCE_HEIGHT))
            .max(0.0);
        let drawable_width = i32::try_from(self.width).unwrap_or(i32::MAX);
        let drawable_height = i32::try_from(self.height).unwrap_or(i32::MAX);
        let width =
            round_to_i32(dimension_f32(UI_REFERENCE_WIDTH) * scale).clamp(1, drawable_width);
        let height =
            round_to_i32(dimension_f32(UI_REFERENCE_HEIGHT) * scale).clamp(1, drawable_height);

        UiViewport {
            x: drawable_width.saturating_sub(width) / 2,
            y: drawable_height.saturating_sub(height) / 2,
            width,
            height,
            scale,
        }
    }
}

/// A drawable or reference dimension as `f32`.
///
/// Every dimension a windowing system reports fits a `u16` (65 535 px is far
/// past any real drawable), so the `u16` round-trip is exact; a value beyond
/// that bound is clamped rather than rounded, which keeps the viewport maths
/// inside a range it can represent.
pub(super) fn dimension_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

/// Rounds a viewport dimension to the nearest integer.
///
/// The value is `reference × scale`, where both dimensions come from
/// [`dimension_f32`] (at most 65 535) and the scale is their ratio, so the
/// result is a whole number in `0..=65 535`: the saturating `as` cast is exact
/// and the caller clamps it to the drawable anyway.
#[allow(clippy::cast_possible_truncation)]
const fn round_to_i32(value: f32) -> i32 {
    value.round() as i32
}

/// Placement of the 480x272 UI reference space inside the physical drawable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}

/// Aspect ratio of the authored `PocketCHIP` reference resolution (480x272).
#[must_use]
pub fn reference_aspect_ratio() -> f32 {
    dimension_f32(UI_REFERENCE_WIDTH) / dimension_f32(UI_REFERENCE_HEIGHT)
}

/// Maps the configured (baseline) vertical field of view onto a drawable with
/// the given aspect ratio.
///
/// * Wider than the `PocketCHIP` baseline: the vertical FOV is unchanged, so the
///   horizontal view expands naturally ("Hor+").
/// * Narrower/taller than the baseline: the horizontal FOV is preserved instead
///   so the level is not cropped left/right; only the vertical FOV grows.
///
/// At the baseline aspect this is the identity, so `PocketCHIP` is unchanged.
#[must_use]
pub fn vertical_fov_for_aspect(configured_vertical_fov_degrees: f32, aspect: f32) -> f32 {
    // Guards against a near-singular projection on very tall/portrait windows.
    const MAX_VERTICAL_FOV_DEGREES: f32 = 150.0;

    let reference = reference_aspect_ratio();
    if !aspect.is_finite() || aspect <= 0.0 || aspect >= reference {
        return configured_vertical_fov_degrees;
    }

    let half_vertical_tan = (configured_vertical_fov_degrees.to_radians() * 0.5).tan();
    let half_horizontal_tan = half_vertical_tan * reference;
    let adjusted = 2.0 * (half_horizontal_tan / aspect).atan();
    adjusted
        .to_degrees()
        .clamp(configured_vertical_fov_degrees, MAX_VERTICAL_FOV_DEGREES)
}

pub(super) const VERTEX_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
attribute vec4 a_color;
attribute vec2 a_uv;
attribute vec2 a_lightmap_uv;
attribute float a_lightmap_page;
// The surface frame is only declared, interpolated and transformed by the
// material tier that shades with it. A tier that draws no normal map, no sheen
// and no reflection has no use for three extra varyings per vertex, and on a
// tiler the varyings are paid for on every vertex of every draw whether or not
// the value reaches a fragment.
#ifndef LIMINAL_MATERIAL_SIMPLE
attribute vec3 a_normal;
attribute vec3 a_tangent;
attribute float a_handedness;
#endif
uniform mat4 u_mvp;
// Model transform of the object being drawn, identity for the static world.
// Normals and the world position the surface-response term needs are taken
// through it, so the dynamic-object path can shade a rigidly transformed mesh
// with the same program.
uniform mat4 u_model;
// Reciprocal of the number of atlas pages stacked into `u_lightmap`: page 0
// occupies the upper half of a two-page texture, so a vertex's page byte
// becomes a half-texture offset here instead of a second fragment fetch.
uniform float u_lightmap_page_scale;
// Atmosphere. The reduced tier evaluates the rational approximation of the
// fog curve once per vertex and interpolates one float to the fragment stage;
// the full tier keeps its world position and evaluates the exact
// exponential-squared curve per fragment, because its sheen needs that
// position anyway.
uniform vec3 u_camera_pos;
uniform float u_fog_density_sq;
uniform float u_fog_reference_y;
uniform float u_fog_height_gain;
varying vec4 v_color;
varying vec2 v_uv;
varying vec2 v_lightmap_uv;
varying float v_lightmap_page;
#ifndef LIMINAL_MATERIAL_SIMPLE
varying vec3 v_world_pos;
#else
varying float v_fog_amount;
#endif
#ifndef LIMINAL_MATERIAL_SIMPLE
varying vec3 v_normal;
varying vec3 v_tangent;
varying float v_handedness;
#endif

void main() {
    v_color = a_color;
    v_uv = a_uv;
    // The atlas pages are stacked vertically in one texture, so the page byte
    // becomes a texture offset here: page 0 is the upper half, page 1 the
    // lower. `LIGHTMAP_NONE` clamps into the last page and the fragment guard
    // keeps it from ever sampling there.
    float page = min(a_lightmap_page, 1.0);
    v_lightmap_uv = vec2(a_lightmap_uv.x, (a_lightmap_uv.y + page) * u_lightmap_page_scale);
    v_lightmap_page = a_lightmap_page;
    vec3 world_pos = (u_model * vec4(a_pos, 1.0)).xyz;
#ifndef LIMINAL_MATERIAL_SIMPLE
    v_world_pos = world_pos;
#else
    // With one evaluation per vertex instead of one per fragment, the divide
    // the rational form needs is affordable, and it tracks the full tier's
    // exponential to within about 1 % of blend at 40 m (about 5 % at 70 m).
    vec3 fog_delta = u_camera_pos - world_pos;
    float distance_sq = dot(fog_delta, fog_delta);
    float below = clamp(u_fog_reference_y - world_pos.y, 0.0, 12.0);
    float height = 1.0 + u_fog_height_gain * below;
    float x_sq = u_fog_density_sq * height * height * distance_sq;
    v_fog_amount = x_sq / (1.0 + x_sq);
#endif
#ifndef LIMINAL_MATERIAL_SIMPLE
    // The frame goes through the model's rotation. A mat4 multiply (rather than
    // a mat3 constructor) is what GLSL 110 and GLSL ES 100 both accept; the
    // model transform is a rigid motion, so the vectors stay unit length.
    v_normal = (u_model * vec4(a_normal, 0.0)).xyz;
    v_tangent = (u_model * vec4(a_tangent, 0.0)).xyz;
    v_handedness = a_handedness;
#endif
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
// Both atlas pages, stacked vertically. The vertex stage has already folded
// the page byte into the `v` coordinate, so one fetch selects the page.
uniform sampler2D u_lightmap;
uniform vec3 u_emission_color;
// Master gate for the whole emissive term. A uniform, so it is coherent across
// every fragment of a draw: content that emits nothing never executes the block
// or the mask fetch, and the value is exact because black emission under a
// white mask multiplies to zero anyway.
uniform float u_emission_enabled;
uniform float u_emission_mask_enabled;
uniform float u_emission_vertex;
// Animated emission: a level can make a sign breathe or a tube stutter. The
// multiplier is 1.0 for every surface that declares no animation, and it scales
// the emissive term alone — never the baked light, the albedo or the alpha.
uniform float u_emission_scale;
uniform float u_lightmap_enabled;
// Baked-light gain for this draw: the frame's light multiplier already scaled
// by `1 - u_emission_vertex` on the CPU, so a luminous fixture face shades its
// glow alone and every other surface is unaffected. Folding both factors into
// one uploaded vector removes two multiplies from every fragment.
uniform vec3 u_light_gain;
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
// Atmosphere. Squared-exponential distance fog with a mild height term, so
// depth reads inside the building without the interior turning milky. The
// density reaches the shader already squared (`density * density`): both curves
// need the square of the exponent and nothing else, so the square root the old
// `length()` took was undone immediately. The HUD sets the squared density to
// zero.
uniform vec3 u_fog_color;
uniform float u_fog_density_sq;
uniform float u_fog_reference_y;
uniform float u_fog_height_gain;
varying vec4 v_color;
varying vec2 v_uv;
varying vec2 v_lightmap_uv;
varying float v_lightmap_page;
#ifndef LIMINAL_MATERIAL_SIMPLE
varying vec3 v_world_pos;
#else
varying float v_fog_amount;
#endif
#ifndef LIMINAL_MATERIAL_SIMPLE
varying vec3 v_normal;
varying vec3 v_tangent;
varying float v_handedness;
#endif

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    vec3 emission = vec3(0.0);
    if (u_emission_enabled > 0.5) {
        vec3 mask = vec3(1.0);
        if (u_emission_mask_enabled > 0.5) {
            mask = texture2D(u_emission_mask, v_uv).rgb;
        }
        emission =
            mix(u_emission_color, v_color.rgb, u_emission_vertex) * mask * tex_color.rgb * u_emission_scale;
    }
    float alpha = tex_color.a * v_color.a * u_opacity;
#ifdef ALPHA_CUTOUT
    if (alpha < u_alpha_cutoff) {
        discard;
    }
#endif
    // The atlas is read only when this fragment actually takes its light from
    // it: the global switch is on while a real bake is resident, and a vertex
    // whose page byte is `LIGHTMAP_NONE` (>= 254.5) keeps the light already in
    // its colour. The vertex stage has already selected the page inside the
    // stacked texture, so this is a single fetch.
    float lightmap_on = u_lightmap_enabled * (1.0 - step(254.5, v_lightmap_page));
    vec3 light = vec3(1.0);
    if (lightmap_on > 0.5) {
        light = texture2D(u_lightmap, v_lightmap_uv).rgb;
    }
    light *= u_light_gain;

#ifndef LIMINAL_MATERIAL_SIMPLE
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
#endif

    vec3 lit = tex_color.rgb * v_color.rgb * light;
#ifndef LIMINAL_MATERIAL_SIMPLE
    vec3 color = lit + sheen + emission;
#else
    vec3 color = lit + emission;
#endif
    // Distance fog, applied to the finished surface colour: emission is a
    // surface property, not a hole punched through the air.
#ifdef LIMINAL_MATERIAL_SIMPLE
    // The reduced tier interpolates the fog amount the vertex stage computed;
    // the full tier evaluates the exact curve per fragment.
    color = mix(color, u_fog_color, v_fog_amount);
#else
    vec3 fog_delta = u_camera_pos - v_world_pos;
    // The squared distance is all either curve needs: the old `length()` took
    // a square root only to square it again.
    float distance_sq = dot(fog_delta, fog_delta);
    float below = clamp(u_fog_reference_y - v_world_pos.y, 0.0, 12.0);
    float height = 1.0 + u_fog_height_gain * below;
    float x_sq = u_fog_density_sq * height * height * distance_sq;
    float fog_amount = 1.0 - exp(-x_sq);
    color = mix(color, u_fog_color, fog_amount);
#endif
    gl_FragColor = vec4(color, alpha);
}
";

/// The world fragment source for one program variant.
///
/// `cutout` prepends the `ALPHA_CUTOUT` define so the alpha-tested program
/// carries the `discard` and the opaque one does not. Compiled once at startup.
#[must_use]
pub fn fragment_shader_source(cutout: bool, simple: bool) -> String {
    let mut defines = String::new();
    if simple {
        defines.push_str("#define LIMINAL_MATERIAL_SIMPLE 1\n");
    }
    if cutout {
        defines.push_str("#define ALPHA_CUTOUT 1\n");
    }
    format!("{defines}{FRAGMENT_SHADER_BODY}")
}

/// The world vertex source for one material tier.
///
/// `simple` selects the tier that declares no surface frame, evaluates its fog
/// per vertex and interpolates one float to the fragment stage (see
/// [`fragment_shader_source`]).
#[must_use]
pub fn vertex_shader_source(simple: bool) -> String {
    if simple {
        format!("#define LIMINAL_MATERIAL_SIMPLE 1\n{VERTEX_SHADER_SRC}")
    } else {
        VERTEX_SHADER_SRC.to_string()
    }
}

/// Fragment stage for the decal pass: the same lit, textured look as the world
/// shader, plus an alpha cut-out so a decal can have a silhouette instead of
/// being a floating rectangle.
///
/// Decals keep the world program's vertex stage, so the two programs share
/// attribute locations ([`create_program`] binds them explicitly). This is a
/// second program rather than a branch in the world shader because `discard`
/// can disable early depth testing for every draw that uses the program, and
/// the opaque world must keep it.
pub(super) const DECAL_FRAGMENT_SHADER_SRC: &str = r"
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

/// Physical distance a decal is displaced along its surface normal, in metres.
///
/// This is the *geometry* half of the decal depth solution: it turns the
/// exactly-coplanar tie between a decal and the surface it is printed on into a
/// real, rasteriser-independent depth ordering, so the base texture can never
/// win a pixel. 0.2 mm is chosen to be invisible from every practical distance
/// (sub-pixel parallax even at the near plane) while still exceeding the
/// depth-buffer resolution over the whole interior range: at 10 m a 24-bit
/// buffer resolves about 60 µm, so this is several steps of separation there.
///
/// The offset is always along the *surface* normal reported by
/// [`crate::level::DecalSurface::normal`], so it lifts floor and ceiling decals
/// vertically and wall decals out of the wall, never into their surface. It is
/// applied in one place, `render::add_decal_quad`, so every decal a level
/// authors — current or future — inherits it without any level-side epsilon.
pub const DECAL_SURFACE_OFFSET_M: f32 = 2.0e-4;

/// Alpha below which the decal pass discards a decal texel.
pub const DECAL_ALPHA_CUTOFF: f32 = 0.5;

/// Attribute indices both scene programs bind before linking, so switching
/// between the world and decal programs never re-points vertex attributes.
pub(super) const SCENE_ATTRIB_POS: u32 = 0;
pub(super) const SCENE_ATTRIB_COLOR: u32 = 1;
pub(super) const SCENE_ATTRIB_UV: u32 = 2;
/// Lightmap atlas coordinates, as two normalized unsigned shorts.
pub(super) const SCENE_ATTRIB_LIGHTMAP_UV: u32 = 3;
/// Lightmap atlas page, as one plain unsigned byte.
pub(super) const SCENE_ATTRIB_LIGHTMAP_PAGE: u32 = 4;
/// Geometric normal, as three normalized signed bytes.
pub(super) const SCENE_ATTRIB_NORMAL: u32 = 5;
/// Surface tangent, as three normalized signed bytes.
pub(super) const SCENE_ATTRIB_TANGENT: u32 = 6;
/// Bitangent sign (±1), as one normalized signed byte.
pub(super) const SCENE_ATTRIB_HANDEDNESS: u32 = 7;

/// Number of vertex attributes the scene programs consume.
///
/// Exactly eight, the OpenGL ES 2.0 guaranteed minimum
/// (`GL_MAX_VERTEX_ATTRIBS >= 8`), which is why the frame is packed into three
/// signed bytes per vector and one for the sign rather than into floats.
pub(super) const SCENE_ATTRIB_COUNT: usize = 8;

/// Texture unit the world pass samples a surface's own sheet from.
pub(super) const SCENE_TEXTURE_UNIT: i32 = 0;
/// Texture unit the world pass samples a material's normal map from.
///
/// Like the mask unit it always has a texture bound (the white sheet when a
/// material authors no normal map), and the material's `u_normal_enabled`
/// uniform decides whether the fragment stage reads it at all.
pub(super) const NORMAL_MAP_TEXTURE_UNIT: i32 = 4;

/// One texture unit expressed as the `GL_TEXTURE0 + n` constant GL wants.
///
/// The units in this module are small non-negative constants, so the conversion
/// cannot fail or wrap; the fallback keeps the expression total anyway.
#[must_use]
pub(super) fn texture_unit(unit: i32) -> u32 {
    #[allow(clippy::cast_sign_loss)] // the units here are non-negative by definition
    let unit = unit.max(0) as u32;
    glow::TEXTURE0.saturating_add(unit)
}

/// Texture unit the world pass samples a material's emissive mask from.
///
/// The unit always has a bound texture (the shared white sheet when a batch has
/// no mask), so a shader that samples it anyway still reads a defined value.
pub(super) const EMISSION_MASK_TEXTURE_UNIT: i32 = 1;
/// Texture unit the world pass samples the baked lightmap atlas from.
///
/// The atlas pages are stacked vertically inside this one texture
/// ([`LIGHTMAP_PAGE_SLOTS`] pages tall), so the unit always holds either the
/// whole bake or the white sheet and the vertex stage's page-offset UV picks
/// the page. One sampler, one fetch per lightmapped fragment.
pub(super) const LIGHTMAP_TEXTURE_UNIT: i32 = 2;

/// Number of lightmap atlas pages the stacked world texture can hold.
///
/// The bake plans against the same bound ([`LIGHTMAP_PAGE_SLOTS`] must equal
/// the lighting side's `LIGHTMAP_ATLAS_MAX_PAGES`): a level whose bake needs
/// more pages than this cannot render its lightmaps correctly, so the lightmap
/// build fails over to the vertex-lit path instead of dropping pages silently.
/// See `crate::lighting::lightmap::LightmapConfig::max_pages`.
pub(super) const LIGHTMAP_PAGE_SLOTS: usize = 2;
