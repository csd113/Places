//! Selective reflections: which surfaces reflect, and how.
//!
//! A reflection is the one thing in this renderer that samples something the
//! bake did not produce. It is therefore opt-in *per material* and deliberately
//! limited: three modes, one strength, no screen-space ray marching.
//!
//! ```text
//! mode    source                                        cost
//! none    nothing (the default; no mode authored)           -
//! probe   one static cubemap baked at level load,        one texture read
//!         read by the reflection vector                  per fragment
//! planar  a second view of the level through the         one scene pass
//!         marked plane, drawn once per frame             per active plane
//! ```
//!
//! Two properties keep this honest next to the rest of the material model:
//!
//! * **It rides on the existing response numbers.** The reflected colour is
//!   weighted by the material's own specular colour, its roughness and the view
//!   angle, so a surface that authors no sheen never reflects and a rough one
//!   suppresses what it does catch. There is no separate "reflectivity" to keep
//!   in sync with the sheen.
//! * **It is approximate and says so.** A probe is a 64-texel-per-face cubemap
//!   baked once; a planar reflection is half resolution. Both are images of the
//!   room, not of the light transport, and neither is expected to survive close
//!   inspection. They exist to make a wet floor and a mirror read as wet and
//!   mirrored.
//!
//! Marking is done in the catalog: `"reflection": {"mode": "planar",
//! "strength": 0.55}`. Nothing in a level file changes, and a material used on
//! two different planes (a wall panel reused on the floor) is resolved per
//! batch at build time by the renderer, which derives the plane from the
//! geometry actually emitted.

/// Largest accepted reflection strength.
pub const MAX_REFLECTION_STRENGTH: f32 = 1.0;

/// Reflection strength a material keeps when it authors a mode but no strength.
///
/// Restrained on purpose: a marked surface should read as *reflective*, not as
/// a mirror. Content that wants a true mirror writes a higher strength.
pub const DEFAULT_REFLECTION_STRENGTH: f32 = 0.45;

/// Where a material's reflection image comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReflectionMode {
    /// No reflection at all: the default for a material that authors no
    /// reflection mode.
    #[default]
    None,
    /// A static cubemap baked once per level load and sampled by the reflected
    /// view vector. Cheap enough to keep on every quality level.
    Probe,
    /// A real second view of the level, mirrored through the surface's own
    /// plane. Drawn once per active plane per frame and only where a material
    /// explicitly asks for it.
    Planar,
}

impl ReflectionMode {
    /// Every mode, in report order.
    pub const ALL: [Self; 3] = [Self::None, Self::Probe, Self::Planar];

    /// Stable lowercase name, as written in the catalog and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Probe => "probe",
            Self::Planar => "planar",
        }
    }

    /// Parses a mode name, case-insensitively. Unknown names are `None`, which
    /// the caller reports as a catalog error rather than silently ignoring.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(trimmed))
    }

    /// True when this mode samples anything.
    #[must_use]
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::None)
    }

    /// True when this mode needs the planar reflection pass.
    #[must_use]
    pub const fn is_planar(self) -> bool {
        matches!(self, Self::Planar)
    }
}

/// The reflection half of a material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialReflection {
    pub mode: ReflectionMode,
    /// `0.0..=1.0`, folded into the reflected colour on top of the material's
    /// specular weight.
    pub strength: f32,
}

impl MaterialReflection {
    /// No reflection: a material that authors no reflection mode.
    pub const NONE: Self = Self {
        mode: ReflectionMode::None,
        strength: 0.0,
    };

    /// A reflection of the given mode and strength.
    #[must_use]
    pub const fn new(mode: ReflectionMode, strength: f32) -> Self {
        Self { mode, strength }
    }

    /// True when this reflection can change a pixel.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.mode.is_active() && self.strength > 0.0
    }

    /// True when the reflection needs the per-frame planar pass.
    #[must_use]
    pub const fn is_planar(&self) -> bool {
        self.is_active() && self.mode.is_planar()
    }

    /// Clamps every value into its documented range.
    #[must_use]
    pub const fn sanitized(self) -> Self {
        let strength = if self.strength.is_finite() {
            self.strength.clamp(0.0, MAX_REFLECTION_STRENGTH)
        } else {
            0.0
        };
        Self {
            mode: self.mode,
            strength,
        }
    }
}

impl Default for MaterialReflection {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // the clamping contract is exact

    use super::*;

    #[test]
    fn the_default_is_no_reflection() {
        let reflection = MaterialReflection::default();
        assert_eq!(reflection, MaterialReflection::NONE);
        assert!(!reflection.is_active());
        assert!(!reflection.is_planar());
    }

    #[test]
    fn mode_names_round_trip_case_insensitively() {
        for mode in ReflectionMode::ALL {
            assert_eq!(ReflectionMode::parse(mode.name()), Some(mode));
            assert_eq!(
                ReflectionMode::parse(&mode.name().to_uppercase()),
                Some(mode)
            );
        }
        assert_eq!(ReflectionMode::parse("mirror"), None);
        assert_eq!(ReflectionMode::parse(""), None);
    }

    #[test]
    fn a_zero_strength_material_never_reflects() {
        let reflection = MaterialReflection::new(ReflectionMode::Planar, 0.0);
        assert!(!reflection.is_active());
        assert!(
            !reflection.is_planar(),
            "a zero strength must not cost a pass"
        );
    }

    #[test]
    fn sanitize_clamps_and_drops_non_finite_strengths() {
        assert_eq!(
            MaterialReflection::new(ReflectionMode::Probe, 4.0)
                .sanitized()
                .strength,
            MAX_REFLECTION_STRENGTH
        );
        assert_eq!(
            MaterialReflection::new(ReflectionMode::Probe, -1.0)
                .sanitized()
                .strength,
            0.0
        );
        assert_eq!(
            MaterialReflection::new(ReflectionMode::Probe, f32::NAN)
                .sanitized()
                .strength,
            0.0
        );
    }

    #[test]
    fn only_planar_needs_the_reflection_pass() {
        assert!(MaterialReflection::new(ReflectionMode::Planar, 0.5).is_planar());
        assert!(!MaterialReflection::new(ReflectionMode::Probe, 0.5).is_planar());
    }
}
