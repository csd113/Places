//! Local pigment excitation. UV sources never enter visible light transport.
use super::entity_receivers::{EntityDirectLight, MovableOccluders};
use crate::engine::entity::DynamicId;
use crate::engine::level::LevelDef;
use crate::engine::lighting::transport::TransportScene;
use crate::engine::lighting::{LevelLighting, LightFalloff};
use crate::engine::spatial::{Aabb, finite_light_region_overlaps};
use glam::Mat4;

pub const MAX_BLACKLIGHTS: usize = 4;

/// Stable authored slots, including disabled sources and zero visible intensity.
#[derive(Clone, Debug, Default)]
pub struct BlacklightSet {
    sources: Vec<(usize, EntityDirectLight)>,
}

impl BlacklightSet {
    #[must_use]
    pub fn from_level(level: &LevelDef, lighting: &LevelLighting) -> Self {
        let sources = level
            .ceiling_lights
            .iter()
            .enumerate()
            .filter(|(_, fixture)| fixture.ultraviolet > 0.0)
            .filter_map(|(fixture_index, fixture)| {
                let index = lighting.fixture_light_index(fixture_index)?;
                let light = lighting.lights().get(index)?;
                let [x, y, z] = light.source.position;
                let (half_x, half_z) = light.half_extents();
                let mut source = EntityDirectLight::ZERO;
                source.position_range = [x, y, z, light.source.range];
                source.color_strength = [0.0, 0.0, 0.0, fixture.ultraviolet];
                source.extent_falloff = [
                    half_x,
                    half_z,
                    match light.source.falloff {
                        LightFalloff::Smooth => 0.0,
                        LightFalloff::Linear => 1.0,
                        LightFalloff::Constant => 2.0,
                    },
                    if light.directional { 1.0 } else { 0.0 },
                ];
                for (tap, (dx, dz)) in source.taps.iter_mut().zip([
                    (-half_x * 0.5, -half_z * 0.5),
                    (half_x * 0.5, -half_z * 0.5),
                    (-half_x * 0.5, half_z * 0.5),
                    (half_x * 0.5, half_z * 0.5),
                ]) {
                    *tap = [x + dx, y, z + dz, 0.25];
                }
                Some((index, source))
            })
            .take(MAX_BLACKLIGHTS)
            .collect();
        Self { sources }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Uses enabled state, deliberately independent of visible-light `is_active`.
    #[must_use]
    pub fn descriptors(
        &self,
        lighting: Option<&LevelLighting>,
    ) -> [EntityDirectLight; MAX_BLACKLIGHTS] {
        let mut result = [EntityDirectLight::ZERO; MAX_BLACKLIGHTS];
        for (target, (index, source)) in result.iter_mut().zip(&self.sources) {
            *target = *source;
            if !lighting
                .and_then(|state| state.lights().get(*index))
                .is_some_and(crate::engine::lighting::BakedLight::enabled)
            {
                target.color_strength = [0.0; 4];
            }
        }
        result
    }

    #[must_use]
    pub fn visibility(
        &self,
        point: [f32; 3],
        scene: &TransportScene,
        dynamic: Option<&dyn MovableOccluders>,
        receiver: Option<DynamicId>,
    ) -> [f32; MAX_BLACKLIGHTS] {
        let mut result = [0.0; MAX_BLACKLIGHTS];
        for (target, (_, source)) in result.iter_mut().zip(&self.sources) {
            *target = source
                .taps
                .iter()
                .map(|tap| {
                    let [x, y, z, weight] = *tap;
                    let to = [x, y, z];
                    let fraction = scene.transmittance(point, to);
                    fraction
                        * dynamic
                            .map_or(1.0, |geometry| geometry.transmittance(point, to, receiver))
                        * weight
                })
                .sum::<f32>()
                .clamp(0.0, 1.0);
        }
        result
    }

    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "finite glam bounds and matrix operations are floating-point geometry"
    )]
    pub fn receiver(
        &self,
        bounds: Aabb,
        model: glam::Mat4,
        scene: &TransportScene,
        dynamic: Option<&dyn MovableOccluders>,
        receiver: Option<DynamicId>,
    ) -> BlacklightReceiver {
        let minimum = glam::Vec3::from_array(bounds.min);
        let maximum = glam::Vec3::from_array(bounds.max);
        if !minimum.is_finite() || !maximum.is_finite() || !model.is_finite() || bounds.is_empty() {
            return BlacklightReceiver::ZERO;
        }
        let extent = (maximum - minimum).max(glam::Vec3::splat(1.0e-6));
        let visibility = std::array::from_fn(|corner| {
            let point = glam::Vec3::new(
                if corner & 1 == 0 {
                    minimum.x
                } else {
                    maximum.x
                },
                if corner & 2 == 0 {
                    minimum.y
                } else {
                    maximum.y
                },
                if corner & 4 == 0 {
                    minimum.z
                } else {
                    maximum.z
                },
            );
            self.visibility(
                model.transform_point3(point).to_array(),
                scene,
                dynamic,
                receiver,
            )
        });
        BlacklightReceiver {
            bounds_min: minimum.extend(1.0).to_array(),
            bounds_extent: extent.extend(0.0).to_array(),
            visibility,
        }
    }
}

/// Model-space receiver support and X-fastest anchor visibility.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlacklightReceiver {
    pub bounds_min: [f32; 4],
    pub bounds_extent: [f32; 4],
    pub visibility: [[f32; MAX_BLACKLIGHTS]; 8],
}
impl BlacklightReceiver {
    pub const ZERO: Self = Self {
        bounds_min: [0.0; 4],
        bounds_extent: [0.0; 4],
        visibility: [[0.0; MAX_BLACKLIGHTS]; 8],
    };
}

/// Shared immutable visibility inputs for static upload and moving receivers.
#[derive(Clone, Copy)]
pub struct BlacklightVisibility<'a> {
    pub sources: &'a BlacklightSet,
    pub scene: &'a TransportScene,
    pub dynamic: Option<&'a dyn MovableOccluders>,
}
impl BlacklightVisibility<'_> {
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "finite glam position and normal operations are floating-point geometry"
    )]
    pub fn vertex(
        self,
        world_position: [f32; 3],
        surface_normal: [f32; 3],
    ) -> [f32; MAX_BLACKLIGHTS] {
        let position = glam::Vec3::from_array(world_position);
        let normal = glam::Vec3::from_array(surface_normal).normalize_or_zero();
        self.sources.visibility(
            (position + normal * 0.001).to_array(),
            self.scene,
            None,
            None,
        )
    }
    #[must_use]
    pub fn receiver(
        self,
        bounds: Aabb,
        model: glam::Mat4,
        receiver: Option<DynamicId>,
    ) -> BlacklightReceiver {
        self.sources
            .receiver(bounds, model, self.scene, self.dynamic, receiver)
    }
}

/// The existing lighting-grid density and cap also bound UV sampling.
pub fn receiver_grid_cuts(minimum: f32, maximum: f32) -> Vec<f32> {
    let cell = crate::engine::lighting::LIGHT_GRID_CELL_M;
    let cap = crate::engine::lighting::MAX_LIGHT_GRID_CELLS;
    let span = maximum - minimum;
    let cap_f = f32::from(u16::try_from(cap).unwrap_or(u16::MAX));
    if span > cell * cap_f {
        // Large receivers retain the existing cap rather than increasing any
        // geometry budget. Ordinary prepared charts are smaller than this.
        return crate::engine::spatial::split_span(minimum, maximum, span / cap_f)
            .into_iter()
            .skip(1)
            .map(|(start, _)| start)
            .collect();
    }
    let first = (minimum / cell).floor().mul_add(cell, cell);
    (0..cap)
        .map(|index| f32::from(u16::try_from(index).unwrap_or(u16::MAX)).mul_add(cell, first))
        .take_while(|plane| *plane < maximum)
        .filter(|plane| *plane > minimum)
        .collect()
}

/// Exact dependencies of a movable ultraviolet receiver. Source geometry is
/// retained by value because a source set can be replaced in place. Switches
/// change excitation in the environment, independently of ray visibility.
#[derive(PartialEq)]
struct BlacklightReceiverKey {
    sources: [EntityDirectLight; MAX_BLACKLIGHTS],
    scene: usize,
    dynamic: usize,
    dynamic_revision: u64,
    model: [[u32; 4]; 4],
    bounds: [[u32; 3]; 2],
    receiver: Option<DynamicId>,
}

/// Reuses the eight anchor visibility samples while the receiver, sources and
/// movable occluders are unchanged. This avoids tracing rays merely to discover
/// that the GPU environment is identical to the preceding frame.
pub struct BlacklightReceiverCache {
    key: Option<BlacklightReceiverKey>,
    value: BlacklightReceiver,
    #[cfg(any(test, feature = "visual-diagnostics"))]
    samples: usize,
}

impl Default for BlacklightReceiverCache {
    fn default() -> Self {
        Self {
            key: None,
            value: BlacklightReceiver::ZERO,
            #[cfg(any(test, feature = "visual-diagnostics"))]
            samples: 0,
        }
    }
}

impl BlacklightReceiverCache {
    #[cfg(any(test, feature = "visual-diagnostics"))]
    pub const fn samples(&self) -> usize {
        self.samples
    }

    pub fn sample(
        &mut self,
        visibility: Option<BlacklightVisibility<'_>>,
        bounds: Aabb,
        model: Mat4,
        receiver: Option<DynamicId>,
    ) -> BlacklightReceiver {
        let Some(context) = visibility else {
            self.key = None;
            self.value = BlacklightReceiver::ZERO;
            return self.value;
        };
        let key = BlacklightReceiverKey {
            sources: context.sources.descriptors(None),
            scene: std::ptr::from_ref(context.scene).addr(),
            dynamic: context
                .dynamic
                .map_or(0, |value| std::ptr::from_ref(value).addr()),
            dynamic_revision: context.dynamic.map_or(0, MovableOccluders::revision),
            model: model
                .to_cols_array_2d()
                .map(|column| column.map(f32::to_bits)),
            bounds: [bounds.min.map(f32::to_bits), bounds.max.map(f32::to_bits)],
            receiver,
        };
        if let Some(previous) = self.key.as_mut()
            && previous.dynamic == key.dynamic
            && context.dynamic.is_some_and(|dynamic| {
                dynamic
                    .changed_regions_since(previous.dynamic_revision)
                    .is_some_and(|regions| {
                        regions.iter().all(|dirty| {
                            !blacklight_visibility_affected(&key.sources, bounds, model, *dirty)
                        })
                    })
            })
        {
            previous.dynamic_revision = key.dynamic_revision;
        }
        if self.key.as_ref() != Some(&key) {
            #[cfg(any(test, feature = "visual-diagnostics"))]
            {
                self.samples = self.samples.saturating_add(1);
            }
            self.value = context.receiver(bounds, model, receiver);
            self.key = Some(key);
        }
        self.value
    }
}

fn blacklight_visibility_affected(
    sources: &[EntityDirectLight; MAX_BLACKLIGHTS],
    bounds: Aabb,
    model: Mat4,
    dirty: Aabb,
) -> bool {
    let receiver = crate::engine::spatial::transform_bounds(&bounds, &model);
    sources.iter().any(|source| {
        let mut emitter = Aabb::EMPTY;
        for [x, y, z, weight] in source.taps {
            if weight > 0.0 {
                emitter.expand([x, y, z]);
            }
        }
        !emitter.is_empty() && finite_light_region_overlaps(emitter, receiver, dirty)
    })
}
