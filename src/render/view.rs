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
            self.width as f32 / self.height as f32
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

        let scale = (self.width as f32 / UI_REFERENCE_WIDTH as f32)
            .min(self.height as f32 / UI_REFERENCE_HEIGHT as f32)
            .max(0.0);
        let drawable_width = i32::try_from(self.width).unwrap_or(i32::MAX);
        let drawable_height = i32::try_from(self.height).unwrap_or(i32::MAX);
        let width = ((UI_REFERENCE_WIDTH as f32 * scale).round() as i32).clamp(1, drawable_width);
        let height =
            ((UI_REFERENCE_HEIGHT as f32 * scale).round() as i32).clamp(1, drawable_height);

        UiViewport {
            x: (drawable_width - width) / 2,
            y: (drawable_height - height) / 2,
            width,
            height,
            scale,
        }
    }
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
    UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32
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

pub(super) const FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    gl_FragColor = tex_color * v_color;
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
/// `units = -2` pulls a decal two depth-buffer resolution steps towards the
/// camera, which is enough to win against the surface it is printed on even
/// when the two quad tessellations disagree by a few ULPs, and is far too
/// small to be visible as physical separation: at a one-metre view distance it
/// is well under a micrometre. The `factor` is zero because a constant bias is
/// exactly what a coplanar decoration needs; a slope-dependent bias would push
/// decals further out at grazing angles for no benefit.
pub const DECAL_POLYGON_OFFSET: (f32, f32) = (0.0, -2.0);

/// Alpha below which the decal pass discards a decal texel.
pub const DECAL_ALPHA_CUTOFF: f32 = 0.5;

/// Attribute indices both scene programs bind before linking, so switching
/// between the world and decal programs never re-points vertex attributes.
pub(super) const SCENE_ATTRIB_POS: u32 = 0;
pub(super) const SCENE_ATTRIB_COLOR: u32 = 1;
pub(super) const SCENE_ATTRIB_UV: u32 = 2;
