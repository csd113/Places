//! The OpenGL reflection resources: probe cubemaps and the planar target.
//!
//! The routing and mirror maths are renderer-neutral and live in
//! `crate::render::common::reflections`; this module owns the GL objects and
//! their framebuffers.

use glow::HasContext;

use super::postprocess::ColorTarget;
use crate::quality::QualityProfile;
use crate::render::common::reflections::planar_target_size;
use crate::render::common::view::{
    DrawableSize, MAX_REFLECTION_PROBES, PROBE_FACE_TEXELS_FULL, PROBE_FACE_TEXELS_LOW,
};

/// A cubemap the renderer bakes once per level load.
pub struct ProbeTarget {
    framebuffer: glow::Framebuffer,
    depth: glow::Renderbuffer,
    cube: glow::Texture,
    face_texels: u32,
    position: [f32; 3],
}

/// The six cubemap faces, in the order the renderer bakes them.
///
/// The look directions and up vectors follow the standard OpenGL cubemap face
/// layout, which is what makes a `textureCube` lookup of a reflected vector
/// fetch the part of the room that vector points at. `up` is deliberately not
/// `+Y` for the four vertical faces: each face's image must be oriented the way
/// the cube lookup expects, not the way a normal camera would shoot it.
pub const CUBE_FACES: [(u32, [f32; 3], [f32; 3]); 6] = [
    // (face, look direction, up)
    (
        glow::TEXTURE_CUBE_MAP_POSITIVE_X,
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_NEGATIVE_X,
        [-1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_POSITIVE_Y,
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_NEGATIVE_Y,
        [0.0, -1.0, 0.0],
        [0.0, 0.0, -1.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_POSITIVE_Z,
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
    ),
    (
        glow::TEXTURE_CUBE_MAP_NEGATIVE_Z,
        [0.0, 0.0, -1.0],
        [0.0, -1.0, 0.0],
    ),
];

impl ProbeTarget {
    /// Creates a probe cubemap and its framebuffer at the profile's face size.
    ///
    /// # Errors
    ///
    /// Returns a message when the texture, framebuffer or depth renderbuffer
    /// cannot be created or the framebuffer is not complete.
    pub unsafe fn create(
        gl: &glow::Context,
        profile: QualityProfile,
        position: [f32; 3],
    ) -> Result<Self, String> {
        let face_texels = match profile {
            QualityProfile::Full => PROBE_FACE_TEXELS_FULL,
            QualityProfile::Low => PROBE_FACE_TEXELS_LOW,
        };
        let side = i32::try_from(face_texels).unwrap_or(i32::MAX);
        let cube = unsafe { gl.create_texture()? };
        let framebuffer = match unsafe { gl.create_framebuffer() } {
            Ok(framebuffer) => framebuffer,
            Err(error) => {
                unsafe { gl.delete_texture(cube) };
                return Err(error);
            }
        };
        let depth = match unsafe { gl.create_renderbuffer() } {
            Ok(depth) => depth,
            Err(error) => {
                unsafe {
                    gl.delete_framebuffer(framebuffer);
                    gl.delete_texture(cube);
                }
                return Err(error);
            }
        };
        let target = Self {
            framebuffer,
            depth,
            cube,
            face_texels,
            position,
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(cube));
            for (face, ..) in CUBE_FACES {
                gl.tex_image_2d(
                    face,
                    0,
                    glow::RGBA8.cast_signed(),
                    side,
                    side,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_CUBE_MAP,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR.cast_signed(),
            );
            gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT16, side, side);
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            // Attach one face and check completeness once: every face has the
            // same format and size, so one test covers all six.
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_CUBE_MAP_POSITIVE_X,
                Some(cube),
                0,
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if !complete {
                target.destroy(gl);
                return Err("reflection probe framebuffer is not complete".to_string());
            }
        }
        Ok(target)
    }

    /// Binds one face as the draw target.
    pub unsafe fn bind_face(&self, gl: &glow::Context, face: u32) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                face,
                Some(self.cube),
                0,
            );
        }
    }

    /// The cubemap texture for the world pass to sample.
    pub const fn cube(&self) -> glow::Texture {
        self.cube
    }

    /// World position the probe was baked from.
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Side of one face, in texels.
    pub const fn face_texels(&self) -> u32 {
        self.face_texels
    }

    /// Deletes every GL object this probe owns.
    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_renderbuffer(self.depth);
            gl.delete_texture(self.cube);
        }
    }
}

/// The planar reflection target: a half-resolution colour+depth view of the
/// level seen through the active mirror plane.
pub struct PlanarTarget {
    target: ColorTarget,
    framebuffer: glow::Framebuffer,
    depth: glow::Renderbuffer,
    size: DrawableSize,
}

impl PlanarTarget {
    /// Creates a half-resolution colour+depth target for `scene_size`.
    ///
    /// # Errors
    ///
    /// Returns a message when any GL object cannot be created or the framebuffer
    /// is not complete.
    pub unsafe fn create(gl: &glow::Context, scene_size: DrawableSize) -> Result<Self, String> {
        let size = planar_target_size(scene_size);
        if size.is_empty() {
            return Err("refusing to create a zero-sized reflection target".to_string());
        }
        let target = unsafe { ColorTarget::create(gl, size, true)? };
        let framebuffer = match unsafe { gl.create_framebuffer() } {
            Ok(framebuffer) => framebuffer,
            Err(error) => {
                unsafe { target.destroy(gl) };
                return Err(error);
            }
        };
        let depth = match unsafe { gl.create_renderbuffer() } {
            Ok(depth) => depth,
            Err(error) => {
                unsafe {
                    gl.delete_framebuffer(framebuffer);
                    target.destroy(gl);
                }
                return Err(error);
            }
        };
        let planar = Self {
            target,
            framebuffer,
            depth,
            size,
        };
        let width = i32::try_from(size.width).unwrap_or(i32::MAX);
        let height = i32::try_from(size.height).unwrap_or(i32::MAX);
        let complete = unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(planar.color()),
                0,
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT16, width, height);
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            complete
        };
        if !complete {
            unsafe { planar.destroy(gl) };
            return Err("planar reflection framebuffer is not complete".to_string());
        }
        Ok(planar)
    }

    /// Binds this target as the draw target at its own viewport.
    pub unsafe fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.viewport(
                0,
                0,
                i32::try_from(self.size.width).unwrap_or(i32::MAX),
                i32::try_from(self.size.height).unwrap_or(i32::MAX),
            );
        }
    }

    /// The colour attachment for the world pass to sample.
    pub const fn color(&self) -> glow::Texture {
        self.target.color()
    }

    /// Deletes every GL object this target owns.
    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            self.target.destroy(gl);
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_renderbuffer(self.depth);
        }
    }
}

/// The baked reflection resources of the current level: up to
/// [`MAX_REFLECTION_PROBES`] probe cubemaps and the lazily created planar
/// target. CPU positions are kept alongside the probes so frame preparation can
/// choose the nearest one without touching a GL handle.
#[derive(Default)]
pub struct ReflectionTargets {
    probes: Vec<ProbeTarget>,
    positions: Vec<[f32; 3]>,
    planar: Option<PlanarTarget>,
}

impl ReflectionTargets {
    /// True when no probe has been baked.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.probes.is_empty()
    }

    /// How many probes are resident.
    #[must_use]
    pub const fn probe_count(&self) -> usize {
        self.probes.len()
    }

    /// Baked probe positions, in bake order.
    #[must_use]
    pub fn probe_positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    /// The cube of one baked probe.
    #[must_use]
    pub fn cube(&self, index: usize) -> Option<glow::Texture> {
        self.probes.get(index).map(ProbeTarget::cube)
    }

    /// Face size of the first baked probe, for the load-time log.
    #[must_use]
    pub fn first_face_texels(&self) -> u32 {
        self.probes.first().map_or(0, ProbeTarget::face_texels)
    }

    /// Deletes every baked probe: the geometry it was baked for no longer
    /// exists.
    pub unsafe fn clear_probes(&mut self, gl: &glow::Context) {
        for probe in self.probes.drain(..) {
            unsafe { probe.destroy(gl) };
        }
        self.positions.clear();
    }

    /// Adds one baked probe.
    pub fn push_probe(&mut self, probe: ProbeTarget) {
        if self.probes.len() < MAX_REFLECTION_PROBES {
            self.positions.push(probe.position());
            self.probes.push(probe);
        }
    }

    /// Ensures the planar target matches `scene_size`, creating it on demand.
    ///
    /// Returns whether the planar pass can run. A context that refuses the
    /// target disables the pass for the session rather than retrying every
    /// frame; the caller applies that policy through
    /// [`crate::render::common::reflections::Reflections::disable_planar`].
    pub unsafe fn ensure_planar(&mut self, gl: &glow::Context, scene_size: DrawableSize) -> bool {
        let wanted = planar_target_size(scene_size);
        if self
            .planar
            .as_ref()
            .is_some_and(|planar| planar.size == wanted)
        {
            return true;
        }
        if let Some(planar) = self.planar.take() {
            unsafe { planar.destroy(gl) };
        }
        match unsafe { PlanarTarget::create(gl, scene_size) } {
            Ok(planar) => {
                self.planar = Some(planar);
                true
            }
            Err(_error) => false,
        }
    }

    /// The planar target, if one is resident.
    #[must_use]
    pub const fn planar(&self) -> Option<&PlanarTarget> {
        self.planar.as_ref()
    }
}
