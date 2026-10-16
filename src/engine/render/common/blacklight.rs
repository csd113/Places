//! Local pigment excitation. UV sources never enter visible light transport.
use super::light_transport::EntityDirectLight;
use crate::engine::level::LevelDef;
use crate::engine::lighting::transport::TransportScene;
use crate::engine::lighting::{LevelLighting, LightFalloff};
use crate::engine::spatial::Aabb;
use bytemuck::{Pod, Zeroable};

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
                let (half_x, half_z) = light.source.half_extents();
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
        dynamic: Option<&super::dynamic_visibility::DynamicVisibility>,
        receiver: Option<super::dynamic::DynamicId>,
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
        dynamic: Option<&super::dynamic_visibility::DynamicVisibility>,
        receiver: Option<super::dynamic::DynamicId>,
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

/// Model-space receiver anchors, carried only by GPU environment records.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
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
    pub dynamic: Option<&'a super::dynamic_visibility::DynamicVisibility>,
}
impl BlacklightVisibility<'_> {
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "finite glam position and normal operations are floating-point geometry"
    )]
    pub fn vertex(self, vertex: &super::mesh::Vertex) -> [f32; MAX_BLACKLIGHTS] {
        let position = glam::Vec3::from_array(vertex.pos);
        let normal = glam::Vec3::from_array(vertex.normal).normalize_or_zero();
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
        receiver: Option<super::dynamic::DynamicId>,
    ) -> BlacklightReceiver {
        self.sources
            .receiver(bounds, model, self.scene, self.dynamic, receiver)
    }
}
