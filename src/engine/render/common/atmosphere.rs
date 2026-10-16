//! Lightweight atmospheric distance fog.
//!
//! Fog is the one part of the post-processing brief that lives in the world
//! fragment shader rather than in a separate pass: it has to be applied per
//! fragment anyway, it costs no extra draw and no extra target, and keeping it
//! in the world shader means the historical direct (no-offscreen) path still
//! gets it. Bloom, exposure and grading live in [`super::postprocess`] instead,
//! because they need the finished image.
//!
//! The model is exponential-squared distance fog with a mild height term:
//!
//! ```text
//! density(x) = density * (1 + height_gain * max(0, reference_y - y))
//! amount     = 1 - exp(-(density(x) * distance)^2)
//! ```
//!
//! Both constants are deliberately small. The shipped interiors are at most
//! about 30 m across, so the fog only becomes visible where the building is
//! genuinely deep — a long corridor, the far end of the pool hall, the unmade
//! world through the last doorway — and never turns a room smoky. The height
//! term gives the air a touch more body near the floor without a volumetric
//! pass: it is a gradient on a scalar, not a light shaft.
//!
//! On top of the global atmosphere a level may author regional fog volumes
//! ([`FogRegion`]): boxes that thicken the air *inside* them, per fragment.
//! The region evaluation is the shader's `fogged` loop mirrored here so tests
//! can pin the exact numbers a level produces.

use crate::engine::level::{DEFAULT_FOG_FALLOFF_M, FogRegionDef, LevelDef, MAX_FOG_REGIONS};
use crate::engine::quality::QualityLevel;

/// How the air is tinted and how thick it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogState {
    /// Colour the fog mixes towards.
    pub color: [f32; 3],
    /// Extinction per metre at the reference height.
    pub density: f32,
    /// World `y` above which the height term stops thinning the fog.
    pub reference_y: f32,
    /// How much denser the fog gets per metre below the reference height.
    pub height_gain: f32,
}

impl FogState {
    /// The shipped atmosphere: neutral-cool, thin, and a little heavier low
    /// down.
    ///
    /// `density = 0.0095` gives about 4 % at 20 m, 15 % at 40 m and 63 % at the
    /// 100 m far plane before the height term, which is depth without haze. The
    /// colour is the concrete-and-glass grey the interiors already use, so the
    /// fog reads as the building rather than as weather.
    pub const SHIPPED: Self = Self {
        color: [0.60, 0.63, 0.68],
        density: 0.0095,
        reference_y: 2.0,
        height_gain: 0.045,
    };
}

/// One resolved regional fog volume: world-space values only, no JSON shape.
///
/// The level loader validates the authored `min < max`, density, colour and
/// falloff bounds; this type resolves the optional fields once at level
/// install, so the per-fragment path never sees an `Option`. Resolution never
/// invents a value the author did not write: an omitted colour is the global
/// atmosphere's colour, an omitted ground layer is the box floor and an
/// omitted top is the box ceiling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogRegion {
    /// Minimum world corner of the box.
    pub min: [f32; 3],
    /// Maximum world corner of the box.
    pub max: [f32; 3],
    /// Extinction per metre at full contribution.
    pub density: f32,
    /// Colour the region mixes towards when it wins the overlap.
    pub color: [f32; 3],
    /// Horizontal soft edge inside the box, in metres (`0.0` = hard edge).
    pub falloff_m: f32,
    /// World Y the full-density ground layer starts at.
    pub ground_y: f32,
    /// World Y the density has faded to zero at.
    pub top_y: f32,
}

impl FogRegion {
    /// Resolves one authored region against the global atmosphere.
    ///
    /// A non-finite optional value is ignored in favour of its default; the
    /// loader rejects one before this point, so this is the renderer's
    /// defensive path, not the authoring contract.
    #[must_use]
    pub fn resolve(def: &FogRegionDef, global_color: [f32; 3]) -> Self {
        let min = [
            def.min[0].min(def.max[0]),
            def.min[1].min(def.max[1]),
            def.min[2].min(def.max[2]),
        ];
        let max = [
            def.min[0].max(def.max[0]),
            def.min[1].max(def.max[1]),
            def.min[2].max(def.max[2]),
        ];
        let finite = |value: Option<f32>| value.filter(|candidate| candidate.is_finite());
        Self {
            min,
            max,
            density: if def.density.is_finite() {
                def.density.max(0.0)
            } else {
                0.0
            },
            color: def.color.unwrap_or(global_color),
            falloff_m: finite(def.falloff_m).map_or(DEFAULT_FOG_FALLOFF_M, |value| value.max(0.0)),
            ground_y: finite(def.ground_y).unwrap_or(min[1]),
            top_y: finite(def.top_y).unwrap_or(max[1]),
        }
    }
}

/// The resolved fog of one level: the global atmosphere plus its regions.
///
/// A level without regions resolves to [`FogState::SHIPPED`] and an empty
/// list, which is exactly the historical renderer state.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelFog {
    /// The global distance fog every level keeps.
    pub global: FogState,
    /// The level's regional volumes, in authoring order.
    pub regions: Vec<FogRegion>,
}

impl LevelFog {
    /// The fog of a level that authors no region: the global atmosphere.
    #[must_use]
    pub const fn global_only() -> Self {
        Self {
            global: FogState::SHIPPED,
            regions: Vec::new(),
        }
    }

    /// Resolves every authored region of one level against the global colour.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let global = level
            .environment
            .map_or(FogState::SHIPPED, |environment| FogState {
                color: environment.fog.color,
                density: environment.fog.density,
                reference_y: environment.fog.reference_y,
                height_gain: environment.fog.height_gain,
            });
        Self {
            global,
            regions: level
                .fog_regions
                .iter()
                .map(|def| FogRegion::resolve(def, global.color))
                .collect(),
        }
    }

    /// How many of the authored regions a quality preset uploads.
    ///
    /// Low uploads the first two regions, Medium the first eight and High all
    /// [`MAX_FOG_REGIONS`]; the count is a uniform word, so switching presets
    /// recovers the fuller set instantly with no rebuild.
    #[must_use]
    pub fn uploaded_region_count(&self, quality: QualityLevel) -> usize {
        self.regions.len().min(fog_region_cap(quality))
    }
}

/// The largest number of fog regions a quality preset uploads.
///
/// The authored cap is [`MAX_FOG_REGIONS`]; Low and Medium deliberately upload
/// only a prefix so a cheap preset never pays for the full array, and the
/// count travels as a uniform word, so a preset change is recovered from the
/// already-resident level.
#[must_use]
pub const fn fog_region_cap(quality: QualityLevel) -> usize {
    match quality {
        QualityLevel::Low => 2,
        QualityLevel::Medium => 8,
        QualityLevel::High => MAX_FOG_REGIONS,
    }
}
