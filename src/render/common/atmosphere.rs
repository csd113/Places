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

use crate::level::{DEFAULT_FOG_FALLOFF_M, FogRegionDef, LevelDef, MAX_FOG_REGIONS};
use crate::quality::QualityLevel;

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

    /// The fraction of fog at `distance` metres and `height` metres.
    ///
    /// Mirrors the shader's arithmetic exactly, so a test can pin the numbers a
    /// level's depth actually produces.
    #[must_use]
    #[cfg(test)]
    // Float-only arithmetic on finite inputs: no overflow, no panic, and the
    // result is clamped.
    pub fn amount(self, distance: f32, height: f32) -> f32 {
        let below = (self.reference_y - height).clamp(0.0, 12.0);
        let density = self.height_gain.mul_add(below, 1.0) * self.density;
        let scaled = density * distance;
        (1.0 - (-scaled * scaled).exp()).clamp(0.0, 1.0)
    }
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

    /// This region's effective contribution at a world position:
    /// `density * horizontal_edge * vertical_layer`.
    ///
    /// Mirrors the shader's arithmetic exactly, so a test can pin the numbers
    /// a fragment actually produces. Test-only: the renderer evaluates the
    /// same expression in the fragment shader, never on the CPU.
    #[must_use]
    #[cfg(test)]
    // Float-only arithmetic on finite inputs: no overflow, no panic, and the
    // result is clamped.
    pub fn contribution(&self, position: [f32; 3]) -> f32 {
        self.density * self.horizontal_factor(position) * self.vertical_factor(position[1])
    }

    /// Horizontal edge factor: zero outside the box footprint, one at least
    /// `falloff_m` inside it, a linear ramp in between. A falloff of zero is a
    /// hard edge.
    ///
    /// Test-only, exactly like [`Self::contribution`].
    #[must_use]
    #[cfg(test)]
    // Float-only arithmetic on finite inputs: no overflow and no panic.
    pub fn horizontal_factor(&self, position: [f32; 3]) -> f32 {
        let horizontal = (position[0] - self.min[0])
            .min(self.max[0] - position[0])
            .min(position[2] - self.min[2])
            .min(self.max[2] - position[2]);
        if horizontal <= 0.0 {
            return 0.0;
        }
        if self.falloff_m <= 0.0 {
            return 1.0;
        }
        (horizontal / self.falloff_m).clamp(0.0, 1.0)
    }

    /// Vertical layer factor: one at and below `ground_y`, a linear fade to
    /// zero at `top_y`, zero above it. A degenerate layer (`top_y` at or below
    /// `ground_y`) is a half-space: full below its base, nothing above.
    ///
    /// Test-only, exactly like [`Self::contribution`].
    #[must_use]
    #[cfg(test)]
    // Float-only arithmetic on finite inputs: no overflow and no panic.
    pub fn vertical_factor(&self, y: f32) -> f32 {
        if self.top_y <= self.ground_y {
            return if y <= self.ground_y { 1.0 } else { 0.0 };
        }
        if y <= self.ground_y {
            return 1.0;
        }
        if y < self.top_y {
            return ((self.top_y - y) / (self.top_y - self.ground_y)).clamp(0.0, 1.0);
        }
        0.0
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
        let global = FogState::SHIPPED;
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

    /// The mixed fog at one fragment, from the camera's own position.
    ///
    /// Returns `(amount, colour)`: the fraction of the surface colour the
    /// shader mixes towards `colour`. The regional term is a pure function of
    /// the *fragment's* world position; the camera only sets the distance, so
    /// this mirrors the shader's per-fragment evaluation and not a
    /// camera-membership test. Test-only: the renderer evaluates the same
    /// expression in the fragment shader.
    #[must_use]
    #[cfg(test)]
    // Float-only arithmetic on finite inputs: no overflow, no panic, and the
    // result is clamped.
    pub fn amount(&self, camera_position: [f32; 3], world_position: [f32; 3]) -> (f32, [f32; 3]) {
        let dx = camera_position[0] - world_position[0];
        let dy = camera_position[1] - world_position[1];
        let dz = camera_position[2] - world_position[2];
        let distance = dz.mul_add(dz, dx.mul_add(dx, dy * dy)).sqrt();
        let below = (self.global.reference_y - world_position[1]).clamp(0.0, 12.0);
        let mut color = self.global.color;
        let mut layer_density = 0.0f32;
        for region in &self.regions {
            let contribution = region.contribution(world_position);
            // Strictly greater keeps the lowest authoring index on a tie.
            if contribution > layer_density {
                layer_density = contribution;
                color = region.color;
            }
        }
        // Mirrors the shader's `fog_density * (1.0 + gain * below)` shape.
        let global_density = self.global.density * self.global.height_gain.mul_add(below, 1.0);
        let density = global_density + layer_density;
        let scaled = density * distance;
        let amount = (1.0 - (-scaled * scaled).exp()).clamp(0.0, 1.0);
        (amount, color)
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

#[cfg(test)]
mod tests {
    // The helpers are mirrored by exact values, the mirrored formulas are float
    // arithmetic on finite inputs, and test fixtures index their own arrays.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::float_cmp,
        clippy::indexing_slicing,
        reason = "The helpers are mirrored by exact values, the mirrored formulas are float arithmetic on finite inputs, and test fixtures index their own arrays."
    )]

    use super::*;
    use crate::test_support::assert_exact;

    fn close(left: f32, right: f32) -> bool {
        (left - right).abs() < 1.0e-6
    }

    #[test]
    fn the_shipped_fog_is_invisible_up_close_and_subtle_far_away() {
        let fog = FogState::SHIPPED;
        assert!(
            fog.amount(5.0, 1.0) < 0.01,
            "a room's near wall must not haze"
        );
        assert!(
            (0.05..0.20).contains(&fog.amount(40.0, 1.0)),
            "40 m must read as depth, not as weather: {}",
            fog.amount(40.0, 1.0)
        );
        assert!(
            fog.amount(100.0, 1.0) < 0.65,
            "the far plane must stay readable: {}",
            fog.amount(100.0, 1.0)
        );
    }

    #[test]
    fn the_height_term_only_ever_thickens_the_air() {
        let fog = FogState::SHIPPED;
        let high = fog.amount(30.0, 4.0);
        let low = fog.amount(30.0, -1.5);
        assert!(low > high, "air near the floor must be at least as thick");
        assert!(close(high, fog.amount(30.0, fog.reference_y)));
        assert!(
            close(high, fog.amount(30.0, fog.reference_y + 10.0)),
            "above the reference height the term must be flat"
        );
    }

    #[test]
    fn a_zero_density_fog_is_exactly_off() {
        let fog = FogState {
            color: [0.0; 3],
            density: 0.0,
            reference_y: 0.0,
            height_gain: 0.0,
        };
        assert!(close(fog.amount(1000.0, -20.0), 0.0));
    }

    #[test]
    fn the_amount_is_bounded_for_extreme_inputs() {
        let fog = FogState::SHIPPED;
        assert!(close(fog.amount(f32::INFINITY, 0.0), 1.0));
        assert!(fog.amount(1.0e6, -1.0e6) <= 1.0);
        assert!(fog.amount(1.0e6, -1.0e6) >= 0.0);
    }

    // ------------------------------------------------------------ fog regions

    /// A region box `0..10` in X/Z and `0..4` in Y with explicit layers.
    fn region(
        density: f32,
        color: [f32; 3],
        falloff_m: f32,
        ground_y: f32,
        top_y: f32,
    ) -> FogRegion {
        FogRegion {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 4.0, 10.0],
            density,
            color,
            falloff_m,
            ground_y,
            top_y,
        }
    }

    #[test]
    fn a_region_contributes_only_inside_its_own_bounds() {
        // The camera sits inside the region; the *fragment* is 2 m outside it,
        // so the regional term must be exactly zero and the global atmosphere
        // alone applies. This is the per-fragment contract.
        let level_fog = LevelFog {
            global: FogState::SHIPPED,
            regions: vec![region(0.2, [1.0, 0.0, 0.0], 2.0, 0.0, 2.0)],
        };
        let globally = LevelFog::global_only();
        let camera = [5.0, 1.0, 5.0];
        let outside = [12.0, 1.0, 5.0];
        assert_exact(level_fog.regions[0].contribution(outside), 0.0);
        assert_eq!(
            level_fog.amount(camera, outside),
            globally.amount(camera, outside)
        );
        assert_eq!(level_fog.amount(camera, outside).1, FogState::SHIPPED.color);

        // The camera is outside the region and the fragment inside it: the
        // region still applies, because membership is the fragment's.
        let inside = [5.0, 0.5, 5.0];
        let camera_outside = [40.0, 5.0, 40.0];
        let (amount, color) = level_fog.amount(camera_outside, inside);
        let (global_amount, _) = globally.amount(camera_outside, inside);
        assert!(amount > global_amount, "the region thickens the fragment");
        assert_eq!(color, [1.0, 0.0, 0.0], "the winner's colour is mixed to");
    }

    #[test]
    fn the_horizontal_edge_ramps_inside_the_box_and_zero_is_hard() {
        let soft = region(0.2, [1.0; 3], 2.0, 0.0, 4.0);
        // 1 m inside a 2 m falloff is exactly half; the core is full.
        assert!(close(soft.horizontal_factor([1.0, 1.0, 5.0]), 0.5));
        assert!(close(soft.horizontal_factor([5.0, 1.0, 5.0]), 1.0));
        assert!(close(soft.horizontal_factor([-0.001, 1.0, 5.0]), 0.0));
        let hard = region(0.2, [1.0; 3], 0.0, 0.0, 4.0);
        assert!(close(hard.horizontal_factor([0.001, 1.0, 5.0]), 1.0));
        assert!(close(hard.horizontal_factor([-0.001, 1.0, 5.0]), 0.0));
    }

    #[test]
    fn the_ground_layer_fades_to_zero_at_the_top() {
        let layer = region(0.2, [1.0; 3], 2.0, 0.0, 2.0);
        assert!(close(layer.vertical_factor(-1.0), 1.0));
        assert!(close(layer.vertical_factor(0.0), 1.0));
        assert!(close(layer.vertical_factor(1.0), 0.5));
        assert!(close(layer.vertical_factor(2.0), 0.0));
        assert!(close(layer.vertical_factor(3.0), 0.0));
        // A degenerate layer is a half-space: full below its base, nothing
        // above it.
        let half_space = region(0.2, [1.0; 3], 2.0, 1.0, 1.0);
        assert!(close(half_space.vertical_factor(0.5), 1.0));
        assert!(close(half_space.vertical_factor(1.5), 0.0));
        // The layer contribution is exactly density * edge * vertical.
        assert!(close(layer.contribution([5.0, 1.0, 5.0]), 0.1));
    }

    #[test]
    fn regions_never_sum_and_ties_keep_the_lowest_authoring_index() {
        let strong = region(0.2, [1.0, 0.0, 0.0], 2.0, 0.0, 1.0);
        let weak = region(0.1, [0.0, 1.0, 0.0], 2.0, 0.0, 1.0);
        let level_fog = LevelFog {
            global: FogState {
                color: [0.5; 3],
                density: 0.0,
                reference_y: 0.0,
                height_gain: 0.0,
            },
            regions: vec![strong, weak],
        };
        // Full edge, y = 0 is inside both ground layers; the strong region's
        // contribution wins and the densities do not sum.
        let (amount, color) = level_fog.amount([0.0; 3], [5.0, 0.0, 5.0]);
        assert_eq!(color, [1.0, 0.0, 0.0]);
        // The fragment sits 5 m out on X and Z: the distance is the 3D one.
        let distance = 50.0f32.sqrt();
        let expected = 1.0 - (-(0.2f32 * distance).powi(2)).exp();
        assert!(
            close(amount, expected),
            "amount {amount} expected {expected}"
        );

        // A tie keeps the first authored region.
        let tied = LevelFog {
            global: level_fog.global,
            regions: vec![strong, region(0.2, [0.0, 1.0, 0.0], 2.0, 0.0, 1.0)],
        };
        assert_eq!(tied.amount([0.0; 3], [5.0, 0.0, 5.0]).1, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_zero_global_density_with_regions_still_mixes_the_layer() {
        let level_fog = LevelFog {
            global: FogState {
                color: [0.0; 3],
                density: 0.0,
                reference_y: 0.0,
                height_gain: 0.0,
            },
            regions: vec![region(0.05, [0.6, 0.63, 0.68], 2.0, 0.0, 4.0)],
        };
        let (amount, _) = level_fog.amount([0.0; 3], [5.0, 1.0, 5.0]);
        assert!(amount > 0.0, "the region is the only density left");
        // With no region and zero density the fog is exactly off.
        let off = LevelFog::global_only();
        let off_layer = LevelFog {
            global: FogState {
                density: 0.0,
                ..off.global
            },
            ..off
        };
        assert_exact(off_layer.amount([0.0; 3], [1000.0, -20.0, 0.0]).0, 0.0);
    }

    #[test]
    fn resolution_fills_omitted_fields_from_the_global_atmosphere() {
        let def = FogRegionDef {
            id: "yard_mist".into(),
            min: [0.0, -2.0, 0.0],
            max: [10.0, 3.0, 8.0],
            density: 0.05,
            color: None,
            falloff_m: None,
            ground_y: None,
            top_y: None,
        };
        let resolved = FogRegion::resolve(&def, FogState::SHIPPED.color);
        assert_eq!(resolved.color, FogState::SHIPPED.color);
        assert!(close(resolved.falloff_m, DEFAULT_FOG_FALLOFF_M));
        assert!(close(resolved.ground_y, -2.0), "ground defaults to min.y");
        assert!(close(resolved.top_y, 3.0), "top defaults to max.y");

        let explicit = FogRegionDef {
            color: Some([0.2, 0.3, 0.4]),
            falloff_m: Some(0.0),
            ground_y: Some(-1.0),
            top_y: Some(1.0),
            ..def
        };
        let explicit_region = FogRegion::resolve(&explicit, FogState::SHIPPED.color);
        assert_eq!(explicit_region.color, [0.2, 0.3, 0.4]);
        assert_exact(explicit_region.falloff_m, 0.0);
        assert_exact(explicit_region.ground_y, -1.0);
        assert_exact(explicit_region.top_y, 1.0);
    }

    #[test]
    fn the_quality_presets_upload_two_eight_and_sixteen_regions() {
        assert_eq!(fog_region_cap(QualityLevel::Low), 2);
        assert_eq!(fog_region_cap(QualityLevel::Medium), 8);
        assert_eq!(fog_region_cap(QualityLevel::High), MAX_FOG_REGIONS);
    }

    #[test]
    fn a_preset_change_only_changes_the_uploaded_count() {
        let regions: Vec<FogRegion> = (0..MAX_FOG_REGIONS)
            .map(|index| {
                let density_index = f32::from(u8::try_from(index).unwrap_or(u8::MAX));
                region(0.01 * density_index, [0.5; 3], 2.0, 0.0, 1.0)
            })
            .collect();
        let level_fog = LevelFog {
            global: FogState::SHIPPED,
            regions: regions.clone(),
        };
        assert_eq!(level_fog.uploaded_region_count(QualityLevel::Low), 2);
        assert_eq!(level_fog.uploaded_region_count(QualityLevel::Medium), 8);
        assert_eq!(level_fog.uploaded_region_count(QualityLevel::High), 16);
        // The recovered set is the same authored list: the count is the only
        // value a preset switch moves.
        assert_eq!(level_fog.regions, regions);
        let short = LevelFog {
            global: FogState::SHIPPED,
            regions: regions.iter().take(3).copied().collect(),
        };
        assert_eq!(short.uploaded_region_count(QualityLevel::Medium), 3);
        assert_eq!(short.uploaded_region_count(QualityLevel::High), 3);
    }
}
