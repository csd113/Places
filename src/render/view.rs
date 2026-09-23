//! Window, drawable, viewport and shader declarations.
//!
//! The reference resolution, the drawable-size and UI-viewport maths, the field
//! of view for a non-reference aspect, and the three GLSL programs the renderer
//! compiles once at startup.

/// `PocketCHIP` reference resolution.
///
/// The game logic and UI layout are authored against this 480x272 space; it is
/// also the default window size. It is *not* an assumption about the actual
/// drawable/framebuffer size at runtime.
pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

/// Reference space that 2D UI geometry is authored in (`PocketCHIP` baseline).
pub const UI_REFERENCE_WIDTH: u32 = WINDOW_WIDTH;
pub const UI_REFERENCE_HEIGHT: u32 = WINDOW_HEIGHT;

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
uniform mat4 u_mvp;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    v_color = a_color;
    v_uv = a_uv;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
";

/// Fragment stage of the world pass.
///
/// Two independent terms make a pixel:
///
/// * **Lit** — `texture x vertex colour`, exactly the historical behaviour. The
///   vertex colour already carries the baked environment light, the material
///   tint and the per-face shading.
/// * **Emission** — the material's own brightness, added on top and never
///   multiplied by the light. A dark room cannot extinguish it, and it cannot
///   brighten anything else: emission is not a light source.
///
/// `u_emission_vertex` selects which value feeds the emissive term. Ordinary
/// materials use the per-batch `u_emission_color` (their `emissive x
/// intensity`, modulated by the emissive mask when one is bound, and by the
/// surface texture so artwork shapes the glow). Fixture faces use `1.0`, which
/// makes the per-vertex colour the emission source: their glow is per instance
/// (colour x intensity response) while the batch stays shared, and the lit term
/// is multiplied by zero so exactly one term remains.
///
/// With no emission colour, no mask and `u_emission_vertex = 0` — every
/// material authored before emission existed — the added term is zero and the
/// output is bit-for-bit the old `texture2D(u_texture, v_uv) * v_color`.
pub(super) const FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
uniform sampler2D u_emission_mask;
uniform vec3 u_emission_color;
uniform float u_emission_mask_enabled;
uniform float u_emission_vertex;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    vec3 mask = vec3(1.0);
    if (u_emission_mask_enabled > 0.5) {
        mask = texture2D(u_emission_mask, v_uv).rgb;
    }
    vec3 emission = mix(u_emission_color, v_color.rgb, u_emission_vertex) * mask * tex_color.rgb;
    vec3 lit = tex_color.rgb * v_color.rgb * (1.0 - u_emission_vertex);
    gl_FragColor = vec4(lit + emission, tex_color.a * v_color.a);
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

/// Texture unit the world pass samples a surface's own sheet from.
pub(super) const SCENE_TEXTURE_UNIT: i32 = 0;
/// Texture unit the world pass samples a material's emissive mask from.
///
/// The unit always has a bound texture (the shared white sheet when a batch has
/// no mask), so a shader that samples it anyway still reads a defined value.
pub(super) const EMISSION_MASK_TEXTURE_UNIT: i32 = 1;
