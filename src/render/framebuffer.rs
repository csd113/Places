//! The offscreen scene target and the presentation quad.
//!
//! The 3D scene is rendered into an offscreen framebuffer and then presented to
//! the default framebuffer by one textured quad; the UI is drawn on the default
//! framebuffer afterwards, at the drawable's own resolution, so it stays sharp
//! and unaffected by whatever the scene pass did.
//!
//! ```text
//! 3D scene ──▶ offscreen colour texture ──▶ presentation quad ──▶ UI/HUD
//!              (+ depth renderbuffer)       (default framebuffer)
//! ```
//!
//! This is *infrastructure*: the target carries exactly the drawable's aspect
//! ratio, no scaling tricks, no grading, and no other post-processing pass. It
//! exists so a later batch can insert bloom, fog, exposure or grading between
//! the scene and the presentation without touching the draw path, and it lets
//! the Low profile render the scene at a smaller resolution than the window.
//!
//! Failure is never fatal: a target that cannot be created or that comes back
//! incomplete is reported and the renderer draws straight into the default
//! framebuffer, exactly as it did before this module existed.

use glow::HasContext;

use super::DrawableSize;
use crate::quality::QualityProfile;

/// Depth buffer internal format the target prefers.
///
/// 24-bit depth matches the default framebuffer and keeps the decal pass's
/// polygon offset calibrated; the fallback below covers the contexts that only
/// offer a 16-bit depth renderbuffer.
const DEPTH_FORMAT: u32 = glow::DEPTH_COMPONENT24;
/// Depth format used when the preferred one is unavailable.
const DEPTH_FORMAT_FALLBACK: u32 = glow::DEPTH_COMPONENT16;

/// An offscreen colour+depth framebuffer the scene renders into.
pub(super) struct SceneTarget {
    framebuffer: glow::Framebuffer,
    color: glow::Texture,
    depth: glow::Renderbuffer,
    size: DrawableSize,
    /// Bits per depth sample the driver accepted (24, or 16 on the fallback
    /// path), reported once so a hardware run can see which one it got.
    depth_bits: u32,
}

impl SceneTarget {
    /// Creates a complete offscreen target of `size`.
    ///
    /// The colour attachment is an RGBA8 texture (the presentation pass samples
    /// it) and the depth attachment a renderbuffer. Both formats are core
    /// OpenGL ES 2.0, so no extension or newer context is required. A target
    /// that fails completeness — or a depth format the driver refuses — is
    /// deleted and reported, never left half-bound.
    ///
    /// # Errors
    ///
    /// Returns a message when the framebuffer, texture or renderbuffer cannot
    /// be created or the result is not complete at either depth format.
    pub(super) unsafe fn create(gl: &glow::Context, size: DrawableSize) -> Result<Self, String> {
        if size.is_empty() {
            return Err("refusing to create a zero-sized scene target".to_string());
        }
        let width = i32::try_from(size.width).unwrap_or(i32::MAX);
        let height = i32::try_from(size.height).unwrap_or(i32::MAX);
        let framebuffer = unsafe { gl.create_framebuffer()? };
        let color = match unsafe { gl.create_texture() } {
            Ok(texture) => texture,
            Err(error) => {
                unsafe { gl.delete_framebuffer(framebuffer) };
                return Err(error);
            }
        };
        let depth = match unsafe { gl.create_renderbuffer() } {
            Ok(renderbuffer) => renderbuffer,
            Err(error) => {
                unsafe {
                    gl.delete_texture(color);
                    gl.delete_framebuffer(framebuffer);
                }
                return Err(error);
            }
        };
        let mut target = Self {
            framebuffer,
            color,
            depth,
            size,
            depth_bits: 0,
        };
        match unsafe { target.configure(gl, width, height) } {
            Ok(depth_bits) => target.depth_bits = depth_bits,
            Err(error) => {
                unsafe { target.destroy(gl) };
                return Err(error);
            }
        }
        Ok(target)
    }

    /// Bits per depth sample this target was created with.
    pub(super) const fn depth_bits(&self) -> u32 {
        self.depth_bits
    }

    /// Allocates the attachments and checks completeness, retrying the depth
    /// format once when the preferred one is refused.
    unsafe fn configure(&self, gl: &glow::Context, width: i32, height: i32) -> Result<u32, String> {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8.cast_signed(),
                width,
                height,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            // A framebuffer texture is sampled 1:1 at presentation: clamped and
            // unfiltered, so no texel is invented at the edges of the drawable.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST.cast_signed(),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.color),
                0,
            );
            for (format, bits) in [(DEPTH_FORMAT, 24u32), (DEPTH_FORMAT_FALLBACK, 16u32)] {
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(self.depth));
                gl.renderbuffer_storage(glow::RENDERBUFFER, format, width, height);
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::RENDERBUFFER,
                    Some(self.depth),
                );
                let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                if status == glow::FRAMEBUFFER_COMPLETE {
                    gl.bind_renderbuffer(glow::RENDERBUFFER, None);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    return Ok(bits);
                }
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::RENDERBUFFER,
                    None,
                );
            }
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Err(format!(
                "no complete framebuffer at {}x{} (RGBA8 colour, 24- or 16-bit depth)",
                self.size.width, self.size.height
            ))
        }
    }

    /// Binds this target as the draw target.
    pub(super) unsafe fn bind(&self, gl: &glow::Context) {
        unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer)) };
    }

    /// The colour attachment, for the presentation pass.
    pub(super) const fn color(&self) -> glow::Texture {
        self.color
    }

    /// The depth attachment, which a later pass can share.
    ///
    /// The bloom pass draws the world's emissive term at a quarter resolution
    /// and needs the scene's own depth to reject the emitters hidden behind a
    /// wall; a renderbuffer may be attached to several framebuffers as long as
    /// only one is bound at a time, which is the case here.
    pub(super) const fn depth_buffer(&self) -> glow::Renderbuffer {
        self.depth
    }

    /// Deletes every GL object this target owns.
    pub(super) unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_texture(self.color);
            gl.delete_renderbuffer(self.depth);
        }
    }
}

/// The size the offscreen scene target should render at for one profile.
///
/// * [`QualityProfile::Full`] renders at the drawable's own resolution: the
///   intended presentation, with no rescaling at all.
/// * [`QualityProfile::Low`] renders no wider than the `PocketCHIP` reference
///   width, which is a large saving on a desktop window and exactly the
///   drawable's size on the reference device itself.
///
/// Both scale by a single factor, so the target's aspect ratio is the drawable's
/// and the presentation cannot distort the image. A drawable that is already
/// small is never *upscaled*: the factor is capped at one.
#[must_use]
pub(super) fn scene_target_size(profile: QualityProfile, drawable: DrawableSize) -> DrawableSize {
    if drawable.is_empty() {
        return drawable;
    }
    let factor = match profile {
        QualityProfile::Full => 1.0,
        QualityProfile::Low => {
            let reference = f64::from(super::WINDOW_WIDTH);
            let width = f64::from(drawable.width);
            (reference / width).min(1.0)
        }
    };
    if factor >= 1.0 {
        return drawable;
    }
    let width = scale_dimension(drawable.width, factor);
    let height = scale_dimension(drawable.height, factor);
    DrawableSize::new(width.max(1), height.max(1))
}

/// Scales one drawable dimension, rounding to nearest and never to zero.
fn scale_dimension(value: u32, factor: f64) -> u32 {
    let scaled = (f64::from(value) * factor).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `value` is a `u32` and `factor` is in `(0, 1]`, so the product is in
    // `[0, u32::MAX]` and non-negative; the clamp only guards the fractional
    // rounding.
    let clamped = scaled.clamp(1.0, f64::from(u32::MAX)) as u32;
    clamped
}

/// The unit quad the presentation pass draws, as `(x, y, z)` positions in
/// clip space whose `xy` are also the texture coordinates.
///
/// Two triangles, four distinct vertices, one small static buffer: enough to
/// cover the whole drawable, and the vertex's own UV is its position because
/// clip space and texture space coincide here.
pub(super) const PRESENT_QUAD: [f32; 18] = [
    0.0, 0.0, 0.0, // bottom-left -> uv (0, 0)
    1.0, 0.0, 0.0, // bottom-right
    1.0, 1.0, 0.0, // top-right
    0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0,
];

/// Clip-space matrix that maps [`PRESENT_QUAD`] onto the whole drawable.
///
/// `x` maps `[0, 1]` to `[-1, 1]`, `y` maps `[0, 1]` to `[-1, 1]` (the quad's
/// own `y` already grows upwards, matching a framebuffer texture's bottom-up
/// rows, so the scene is not mirrored), and `z` is unused.
#[must_use]
pub(super) const fn present_matrix() -> glam::Mat4 {
    glam::Mat4::from_cols_array_2d(&[
        [2.0, 0.0, 0.0, 0.0],
        [0.0, 2.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, -1.0, 0.0, 1.0],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_renders_at_the_drawable_resolution() {
        let drawable = DrawableSize::new(1920, 1080);
        assert_eq!(
            scene_target_size(QualityProfile::Full, drawable),
            drawable,
            "Full must never rescale the scene"
        );
    }

    #[test]
    fn low_caps_the_scene_width_at_the_reference_and_keeps_the_aspect() {
        let drawable = DrawableSize::new(1920, 1080);
        let low = scene_target_size(QualityProfile::Low, drawable);
        assert_eq!(low.width, 480);
        assert_eq!(low.height, 270);
        let drawable_aspect = f64::from(drawable.width) / f64::from(drawable.height);
        let low_aspect = f64::from(low.width) / f64::from(low.height);
        assert!(
            (drawable_aspect - low_aspect).abs() < 1.0e-3,
            "the target must keep the drawable's aspect ratio"
        );
    }

    #[test]
    fn low_is_the_identity_on_the_reference_device() {
        let drawable = DrawableSize::new(480, 272);
        assert_eq!(scene_target_size(QualityProfile::Low, drawable), drawable);
    }

    #[test]
    fn a_smaller_than_reference_drawable_is_never_upscaled() {
        let drawable = DrawableSize::new(320, 180);
        let low = scene_target_size(QualityProfile::Low, drawable);
        assert_eq!(low, drawable);
    }

    #[test]
    fn an_empty_drawable_stays_empty() {
        let empty = DrawableSize::new(0, 0);
        assert_eq!(scene_target_size(QualityProfile::Full, empty), empty);
        assert_eq!(scene_target_size(QualityProfile::Low, empty), empty);
    }

    #[test]
    fn tiny_drawables_still_produce_a_drawable_target() {
        let drawable = DrawableSize::new(3, 2);
        let low = scene_target_size(QualityProfile::Low, drawable);
        assert!(low.width >= 1 && low.height >= 1);
    }

    #[test]
    fn the_present_quad_covers_clip_space_exactly() {
        let matrix = present_matrix();
        for corner in [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            let clip = matrix * glam::Vec4::new(corner[0], corner[1], 0.0, 1.0);
            let x = clip.x / clip.w;
            let y = clip.y / clip.w;
            assert!((-1.0..=1.0).contains(&x), "x {x} outside clip space");
            assert!((-1.0..=1.0).contains(&y), "y {y} outside clip space");
        }
        // The quad's own UV corner (0, 0) must land on the bottom-left corner of
        // the drawable, and (1, 1) on the top-right: a framebuffer texture is
        // bottom-up, so presenting it unflipped is what keeps the image upright.
        let bottom_left = matrix * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((bottom_left.x + 1.0).abs() < 1.0e-6 && (bottom_left.y + 1.0).abs() < 1.0e-6);
        let top_right = matrix * glam::Vec4::new(1.0, 1.0, 0.0, 1.0);
        assert!((top_right.x - 1.0).abs() < 1.0e-6 && (top_right.y - 1.0).abs() < 1.0e-6);
    }
}
