//! Restrained post-processing: bloom, exposure/tone and a subtle colour grade.
//!
//! The offscreen scene target (see [`super::framebuffer`]) is what makes this
//! possible: the scene is drawn into a texture, the resolve pass turns that
//! texture into a display image, and the HUD is drawn afterwards on the default
//! framebuffer, so none of this touches the UI.
//!
//! ```text
//! scene ──▶ emissive-only pass (quarter res) ──▶ blur ×2 ──▶ bloom texture
//!      └──▶ resolve (scene + bloom, exposure, tone, grade) ──▶ default framebuffer
//!                                                              └──▶ UI/HUD
//! ```
//!
//! Three deliberate restrictions keep the low-poly aesthetic intact:
//!
//! * **Bloom comes from emission, not from brightness.** The bright pass draws
//!   the world's emissive term alone — the same expression the world shader adds
//!   for a fixture, a sign or a screen — so a brightly lit wall can never bloom
//!   however bright its baked light is. There is no luminance threshold to tune
//!   and nothing to leak.
//! * **The tone curve is a shoulder, not a look.** Everything at or below the
//!   knee passes through unchanged, so the baked lighting's own contrast is
//!   preserved; only values above it (an emissive face at intensity > 1) roll
//!   off towards white instead of clipping.
//! * **Everything is optional.** Each stage is gated by [`PostSettings`], which
//!   the quality profile fills in, and the whole module is skipped when the
//!   offscreen target is unavailable: the direct path draws exactly as it did
//!   before the offscreen target existed.

use glow::HasContext;

use super::DrawableSize;
use super::view::{
    BLOOM_BLUR_FRAGMENT_SHADER_SRC, BLOOM_SCALE_DIVISOR, BLOOM_TEXTURE_UNIT,
    PRESENT_VERTEX_SHADER_SRC, RESOLVE_FRAGMENT_SHADER_SRC, SCENE_TEXTURE_UNIT,
};
use crate::quality::QualityProfile;

/// Resolve-stage settings for one frame.
///
/// Built from the quality profile (see [`PostSettings::for_profile`]) rather
/// than from level data: post-processing is a presentation choice, not content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PostSettings {
    /// Linear exposure multiplier applied before the tone curve.
    pub(super) exposure: f32,
    /// Value at which the tone curve starts rolling off; below it the image is
    /// untouched.
    pub(super) tone_knee: f32,
    /// How much of the blurred emissive image is added back. Zero disables the
    /// bloom passes entirely for the frame.
    pub(super) bloom_strength: f32,
    /// Saturation multiplier of the colour grade (1.0 leaves colour alone).
    pub(super) grade_saturation: f32,
    /// Contrast multiplier around mid grey (1.0 leaves contrast alone).
    pub(super) grade_contrast: f32,
}

impl PostSettings {
    /// The settings one quality profile runs with.
    ///
    /// `Full` gets the complete restrained stack: a shoulder at 0.75, a bloom
    /// strength under one so a fixture glows instead of blooming across the
    /// room, and a grade that is barely a tint. `Low` keeps the tone shoulder —
    /// it is part of the resolve pass the offscreen path already pays for — and
    /// drops the two effects that need extra passes or extra per-pixel work.
    #[must_use]
    pub(super) const fn for_profile(profile: QualityProfile) -> Self {
        match profile {
            QualityProfile::Full => Self {
                exposure: 1.0,
                tone_knee: 0.75,
                bloom_strength: 0.42,
                grade_saturation: 1.03,
                grade_contrast: 1.02,
            },
            // Low presents the scene unfiltered: no bloom, no exposure, no
            // shoulder and no grade, so the resolve stage is the identity and
            // the renderer uses the plain copy quad instead of it. That keeps
            // Low exactly as cheap as the plain copy presentation while the
            // world shader keeps the fog, which is part of the image rather
            // than an extra pass.
            QualityProfile::Low => Self {
                exposure: 1.0,
                tone_knee: 1.0,
                bloom_strength: 0.0,
                grade_saturation: 1.0,
                grade_contrast: 1.0,
            },
        }
    }

    /// Whether the resolve stage would change the image at all.
    ///
    /// When it would not — no bloom, unit exposure, no shoulder, no grade — the
    /// renderer presents the scene with the plain copy quad instead, which is
    /// both cheaper and exactly what that resolve would have produced.
    #[must_use]
    #[allow(clippy::float_cmp)] // these are authored constants, not measurements
    pub(super) fn is_identity(self) -> bool {
        self.bloom_strength == 0.0
            && self.exposure == 1.0
            && self.tone_knee >= 1.0
            && self.grade_saturation == 1.0
            && self.grade_contrast == 1.0
    }

    /// Whether this frame draws the extra bloom passes.
    #[must_use]
    #[allow(clippy::float_cmp)] // an exact zero is how "no bloom" is spelled
    pub(super) fn blooms(self) -> bool {
        self.bloom_strength > 0.0
    }
}

/// A colour-only offscreen target: a framebuffer with one RGBA8 attachment and
/// no depth, used for the emissive image the bloom blurs and the planar
/// reflection image.
pub(super) struct ColorTarget {
    framebuffer: glow::Framebuffer,
    color: glow::Texture,
    size: DrawableSize,
}

impl ColorTarget {
    /// Creates a complete colour-only target of `size`.
    ///
    /// # Errors
    ///
    /// Returns a message when the framebuffer or texture cannot be created, or
    /// the result is not framebuffer-complete.
    pub(super) unsafe fn create(
        gl: &glow::Context,
        size: DrawableSize,
        linear: bool,
    ) -> Result<Self, String> {
        if size.is_empty() {
            return Err("refusing to create a zero-sized colour target".to_string());
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
        let target = Self {
            framebuffer,
            color,
            size,
        };
        let filter = if linear { glow::LINEAR } else { glow::NEAREST };
        let complete = unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(color));
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
                filter.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                filter.cast_signed(),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(color),
                0,
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            complete
        };
        if !complete {
            unsafe { target.destroy(gl) };
            return Err(format!(
                "no complete colour target at {}x{}",
                size.width, size.height
            ));
        }
        Ok(target)
    }

    /// Binds this target as the draw target.
    pub(super) unsafe fn bind(&self, gl: &glow::Context) {
        unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer)) };
    }

    /// The colour attachment, for whatever pass samples it.
    pub(super) const fn color(&self) -> glow::Texture {
        self.color
    }

    /// Size this target was created at.
    pub(super) const fn size(&self) -> DrawableSize {
        self.size
    }

    /// Deletes every GL object this target owns.
    pub(super) unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_texture(self.color);
        }
    }
}

/// The three targets the bloom path needs: the emissive image, and the two
/// quarter-resolution buffers the separable blur ping-pongs between.
///
/// The emissive image is drawn at the **scene target's own size**, because the
/// emissive pass shares the scene's depth buffer: the depth test compares
/// window coordinates, so a smaller viewport would read the wrong corner of it
/// and an emitter hidden behind a wall would still land in the bloom. Only the
/// handful of emissive batches are drawn into it, so the extra pixels are the
/// few the emitters actually cover.
struct BloomTargets {
    emissive: ColorTarget,
    blur_a: ColorTarget,
    blur_b: ColorTarget,
}

/// The post-processing programs, their quad, and the targets of the current
/// size — everything except the scene target itself.
pub(super) struct PostProcess {
    resolve_program: glow::Program,
    resolve_mvp: Option<glow::UniformLocation>,
    resolve_scene: Option<glow::UniformLocation>,
    resolve_bloom: Option<glow::UniformLocation>,
    resolve_bloom_strength: Option<glow::UniformLocation>,
    resolve_exposure: Option<glow::UniformLocation>,
    resolve_tone_knee: Option<glow::UniformLocation>,
    resolve_grade_saturation: Option<glow::UniformLocation>,
    resolve_grade_contrast: Option<glow::UniformLocation>,
    blur_program: glow::Program,
    blur_mvp: Option<glow::UniformLocation>,
    blur_source: Option<glow::UniformLocation>,
    blur_texel: Option<glow::UniformLocation>,
    quad_vbo: glow::Buffer,
    a_pos_loc: u32,
    settings: PostSettings,
    targets: Option<BloomTargets>,
}

impl PostProcess {
    /// Creates the post-processing programs and quad.
    ///
    /// # Errors
    ///
    /// Returns a message when either program does not link or a buffer or
    /// attribute lookup fails. A failure here is not fatal: the caller keeps
    /// the plain presentation pass instead.
    pub(super) unsafe fn create(
        gl: &glow::Context,
        settings: PostSettings,
    ) -> Result<Self, String> {
        unsafe {
            // Both post programs share the scene vertex stage, so `a_pos` is
            // already bound to attribute index 0 by `create_program`.
            let resolve_program = super::renderer::create_program(
                gl,
                PRESENT_VERTEX_SHADER_SRC,
                RESOLVE_FRAGMENT_SHADER_SRC,
            )?;
            let blur_program = match super::renderer::create_program(
                gl,
                PRESENT_VERTEX_SHADER_SRC,
                BLOOM_BLUR_FRAGMENT_SHADER_SRC,
            ) {
                Ok(program) => program,
                Err(error) => {
                    gl.delete_program(resolve_program);
                    return Err(error);
                }
            };
            let quad_vbo = match gl.create_buffer() {
                Ok(buffer) => buffer,
                Err(error) => {
                    gl.delete_program(resolve_program);
                    gl.delete_program(blur_program);
                    return Err(error);
                }
            };
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_vbo));
            let quad_bytes = std::slice::from_raw_parts(
                super::framebuffer::PRESENT_QUAD.as_ptr().cast::<u8>(),
                std::mem::size_of_val(&super::framebuffer::PRESENT_QUAD),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, quad_bytes, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            Ok(Self {
                resolve_mvp: gl.get_uniform_location(resolve_program, "u_mvp"),
                resolve_scene: gl.get_uniform_location(resolve_program, "u_scene"),
                resolve_bloom: gl.get_uniform_location(resolve_program, "u_bloom"),
                resolve_bloom_strength: gl
                    .get_uniform_location(resolve_program, "u_bloom_strength"),
                resolve_exposure: gl.get_uniform_location(resolve_program, "u_exposure"),
                resolve_tone_knee: gl.get_uniform_location(resolve_program, "u_tone_knee"),
                resolve_grade_saturation: gl
                    .get_uniform_location(resolve_program, "u_grade_saturation"),
                resolve_grade_contrast: gl
                    .get_uniform_location(resolve_program, "u_grade_contrast"),
                blur_mvp: gl.get_uniform_location(blur_program, "u_mvp"),
                blur_source: gl.get_uniform_location(blur_program, "u_source"),
                blur_texel: gl.get_uniform_location(blur_program, "u_texel"),
                resolve_program,
                blur_program,
                quad_vbo,
                a_pos_loc: 0,
                settings,
                targets: None,
            })
        }
    }

    /// Current resolve settings.
    pub(super) const fn settings(&self) -> PostSettings {
        self.settings
    }

    /// Applies a new profile's settings, dropping the bloom targets when bloom
    /// is no longer wanted so the memory is not held for nothing.
    pub(super) unsafe fn set_settings(&mut self, gl: &glow::Context, settings: PostSettings) {
        self.settings = settings;
        if !settings.blooms() {
            unsafe { self.release_targets(gl) };
        }
    }

    /// Whether a bloom target is resident for this frame.
    pub(super) fn blooms(&self) -> bool {
        self.settings.blooms() && self.targets.is_some()
    }

    /// Creates, resizes or drops the bloom targets to match `scene_size`.
    ///
    /// Returns whether bloom can run this frame. A target that cannot be
    /// allocated leaves `blooms()` false and the resolve pass simply samples the
    /// scene texture, so a driver that refuses the extra framebuffers loses the
    /// effect and nothing else.
    pub(super) unsafe fn ensure_targets(
        &mut self,
        gl: &glow::Context,
        scene_size: DrawableSize,
        scene_depth: Option<glow::Renderbuffer>,
    ) -> bool {
        if !self.settings.blooms() {
            unsafe { self.release_targets(gl) };
            return false;
        }
        let wanted = bloom_target_size(scene_size);
        if wanted.is_empty() {
            unsafe { self.release_targets(gl) };
            return false;
        }
        if self
            .targets
            .as_ref()
            .is_some_and(|targets| targets.emissive.size() == wanted)
        {
            return true;
        }
        unsafe { self.release_targets(gl) };
        let created = unsafe {
            let Ok(emissive) = ColorTarget::create(gl, scene_size, false) else {
                return false;
            };
            let Ok(blur_a) = ColorTarget::create(gl, wanted, true) else {
                emissive.destroy(gl);
                return false;
            };
            let Ok(blur_b) = ColorTarget::create(gl, wanted, true) else {
                emissive.destroy(gl);
                blur_a.destroy(gl);
                return false;
            };
            BloomTargets {
                emissive,
                blur_a,
                blur_b,
            }
        };
        // The emissive pass draws the same emitters the main pass did, from the
        // same camera, so it tests against the same depth buffer: without it an
        // emitter behind a wall still lands in the bloom image and its glow
        // bleeds through the wall. Sharing a renderbuffer between framebuffers
        // is legal because only one of them is ever bound at a time.
        if let Some(depth) = scene_depth {
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(created.emissive.framebuffer));
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::RENDERBUFFER,
                    Some(depth),
                );
                let complete =
                    gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::RENDERBUFFER,
                    None,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                if complete {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(created.emissive.framebuffer));
                    gl.framebuffer_renderbuffer(
                        glow::FRAMEBUFFER,
                        glow::DEPTH_ATTACHMENT,
                        glow::RENDERBUFFER,
                        Some(depth),
                    );
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                }
            }
        }
        self.targets = Some(created);
        true
    }

    /// Deletes the bloom targets, if any.
    pub(super) unsafe fn release_targets(&mut self, gl: &glow::Context) {
        if let Some(targets) = self.targets.take() {
            unsafe {
                targets.emissive.destroy(gl);
                targets.blur_a.destroy(gl);
                targets.blur_b.destroy(gl);
            }
        }
    }

    /// Binds the emissive target and clears its colour, so the caller can draw
    /// the world's emissive term into it with the ordinary scene programs.
    ///
    /// The **depth buffer is not cleared**: it holds the scene the main pass just
    /// drew, which is what keeps an emitter hidden behind a wall out of the
    /// bloom image.
    ///
    /// Returns the target's size, or `None` when bloom is not resident.
    pub(super) unsafe fn begin_emissive(&self, gl: &glow::Context) -> Option<DrawableSize> {
        let targets = self.targets.as_ref()?;
        let size = targets.emissive.size();
        unsafe {
            targets.emissive.bind(gl);
            gl.viewport(
                0,
                0,
                i32::try_from(size.width).unwrap_or(i32::MAX),
                i32::try_from(size.height).unwrap_or(i32::MAX),
            );
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.clear_color(0.08, 0.08, 0.09, 1.0);
            gl.enable(glow::DEPTH_TEST);
            // The emitters must not write depth into the scene's buffer: it is
            // about to be read by the resolve pass and the next frame's clear.
            gl.depth_mask(false);
        }
        Some(size)
    }

    /// Restores the depth-write state [`Self::begin_emissive`] turned off.
    pub(super) unsafe fn end_emissive(gl: &glow::Context) {
        unsafe { gl.depth_mask(true) };
    }

    /// Runs the two blur passes over the emissive image and returns the texture
    /// to add back in the resolve stage.
    ///
    /// The first pass reads the full-resolution emissive image and writes the
    /// quarter-resolution buffer, so the downsample and the horizontal blur are
    /// one pass; the second does the vertical blur at that size.
    pub(super) unsafe fn finish_bloom(&self, gl: &glow::Context) -> Option<glow::Texture> {
        let targets = self.targets.as_ref()?;
        unsafe {
            self.run_blur(gl, &targets.emissive, &targets.blur_a, 1.0, 0.0);
            self.run_blur(gl, &targets.blur_a, &targets.blur_b, 0.0, 1.0);
        }
        Some(targets.blur_b.color())
    }

    /// One separable blur pass, from `source` into `target`.
    ///
    /// `x` and `y` select the axis; the step is one texel of the *source*.
    unsafe fn run_blur(
        &self,
        gl: &glow::Context,
        source: &ColorTarget,
        target: &ColorTarget,
        x: f32,
        y: f32,
    ) {
        let size = target.size();
        let source_size = source.size();
        unsafe {
            target.bind(gl);
            gl.viewport(
                0,
                0,
                i32::try_from(size.width).unwrap_or(i32::MAX),
                i32::try_from(size.height).unwrap_or(i32::MAX),
            );
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.blur_program));
            let columns = super::framebuffer::present_matrix().to_cols_array();
            if let Some(ref loc) = self.blur_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &columns);
            }
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(source.color()));
            if let Some(ref loc) = self.blur_source {
                gl.uniform_1_i32(Some(loc), 0);
            }
            if let Some(ref loc) = self.blur_texel {
                // One texel of the source, so a full-resolution emissive image
                // feeding a quarter-resolution buffer downsamples by four.
                let width = f32::max(super::view::dimension_f32(source_size.width), 1.0);
                let height = f32::max(super::view::dimension_f32(source_size.height), 1.0);
                gl.uniform_2_f32(Some(loc), x / width, y / height);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.enable_vertex_attrib_array(self.a_pos_loc);
            gl.vertex_attrib_pointer_f32(self.a_pos_loc, 3, glow::FLOAT, false, 12, 0);
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
            gl.disable_vertex_attrib_array(self.a_pos_loc);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    /// Resolves the scene into the default framebuffer: bloom add, exposure,
    /// tone shoulder and colour grade.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn resolve(
        &self,
        gl: &glow::Context,
        scene: glow::Texture,
        bloom: glow::Texture,
        bloom_strength: f32,
        drawable: DrawableSize,
    ) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(
                0,
                0,
                i32::try_from(drawable.width).unwrap_or(i32::MAX),
                i32::try_from(drawable.height).unwrap_or(i32::MAX),
            );
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.resolve_program));
            let columns = super::framebuffer::present_matrix().to_cols_array();
            if let Some(ref loc) = self.resolve_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &columns);
            }
            gl.active_texture(super::view::texture_unit(SCENE_TEXTURE_UNIT));
            gl.bind_texture(glow::TEXTURE_2D, Some(scene));
            if let Some(ref loc) = self.resolve_scene {
                gl.uniform_1_i32(Some(loc), SCENE_TEXTURE_UNIT);
            }
            gl.active_texture(super::view::texture_unit(BLOOM_TEXTURE_UNIT));
            gl.bind_texture(glow::TEXTURE_2D, Some(bloom));
            if let Some(ref loc) = self.resolve_bloom {
                gl.uniform_1_i32(Some(loc), BLOOM_TEXTURE_UNIT);
            }
            if let Some(ref loc) = self.resolve_bloom_strength {
                gl.uniform_1_f32(Some(loc), bloom_strength);
            }
            if let Some(ref loc) = self.resolve_exposure {
                gl.uniform_1_f32(Some(loc), self.settings.exposure);
            }
            if let Some(ref loc) = self.resolve_tone_knee {
                gl.uniform_1_f32(Some(loc), self.settings.tone_knee);
            }
            if let Some(ref loc) = self.resolve_grade_saturation {
                gl.uniform_1_f32(Some(loc), self.settings.grade_saturation);
            }
            if let Some(ref loc) = self.resolve_grade_contrast {
                gl.uniform_1_f32(Some(loc), self.settings.grade_contrast);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.quad_vbo));
            gl.enable_vertex_attrib_array(self.a_pos_loc);
            gl.vertex_attrib_pointer_f32(self.a_pos_loc, 3, glow::FLOAT, false, 12, 0);
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
            gl.disable_vertex_attrib_array(self.a_pos_loc);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.use_program(None);
            gl.enable(glow::DEPTH_TEST);
        }
    }
}

/// Size of the emissive and blur targets for a scene target of `scene_size`.
#[must_use]
pub(super) fn bloom_target_size(scene_size: DrawableSize) -> DrawableSize {
    if scene_size.is_empty() {
        return scene_size;
    }
    DrawableSize::new(
        (scene_size.width / BLOOM_SCALE_DIVISOR).max(1),
        (scene_size.height / BLOOM_SCALE_DIVISOR).max(1),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // the profile contract is exact

    use super::*;

    #[test]
    fn bloom_targets_are_a_quarter_of_the_scene() {
        let size = bloom_target_size(DrawableSize::new(960, 544));
        assert_eq!(size, DrawableSize::new(240, 136));
    }

    #[test]
    fn a_tiny_scene_still_gets_a_one_texel_target() {
        assert_eq!(
            bloom_target_size(DrawableSize::new(2, 3)),
            DrawableSize::new(1, 1)
        );
        assert_eq!(
            bloom_target_size(DrawableSize::new(0, 0)),
            DrawableSize::new(0, 0)
        );
    }

    #[test]
    fn full_blooms_and_low_does_not() {
        let full = PostSettings::for_profile(QualityProfile::Full);
        let low = PostSettings::for_profile(QualityProfile::Low);
        assert!(full.blooms(), "Full runs the bloom passes");
        assert!(!low.blooms(), "Low must not pay for bloom");
        assert_eq!(low.grade_saturation, 1.0, "Low leaves colour alone");
        assert_eq!(low.grade_contrast, 1.0);
        assert!(
            low.is_identity(),
            "Low must be able to skip the resolve stage entirely"
        );
        assert!(!full.is_identity(), "Full always resolves");
    }

    #[test]
    fn the_tone_knee_leaves_the_common_range_alone() {
        // The shoulder is only applied above the knee; the resolve shader's
        // arithmetic is pinned here so a change to the constant is deliberate.
        let settings = PostSettings::for_profile(QualityProfile::Full);
        assert!((0.5..1.0).contains(&settings.tone_knee));
        assert!(settings.bloom_strength < 1.0, "bloom must stay restrained");
    }
}
