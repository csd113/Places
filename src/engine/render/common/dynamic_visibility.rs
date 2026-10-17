//! Model-local ray resources for the existing movable scene. Geometry and PNG
//! alpha are prepared once per real mesh; movement updates inverse transforms
//! and a revision, preserving visibility for stationary receivers.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::dynamic::{DynamicId, DynamicMesh, DynamicScene};
use crate::engine::lighting::transport::TransportScene;
use crate::engine::spatial::{
    Aabb, directional_light_region_overlaps, finite_light_region_overlaps,
};

/// Exact dependencies of a movable ultraviolet receiver. Source geometry is
/// retained by value because a source set can be replaced in place. Switches
/// change excitation in the environment, independently of ray visibility.
#[derive(PartialEq)]
struct BlacklightReceiverKey {
    sources: [super::light_transport::EntityDirectLight; super::blacklight::MAX_BLACKLIGHTS],
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
    value: super::blacklight::BlacklightReceiver,
}

impl Default for BlacklightReceiverCache {
    fn default() -> Self {
        Self {
            key: None,
            value: super::blacklight::BlacklightReceiver::ZERO,
        }
    }
}

impl BlacklightReceiverCache {
    pub fn sample(
        &mut self,
        visibility: Option<super::blacklight::BlacklightVisibility<'_>>,
        bounds: Aabb,
        model: Mat4,
        receiver: Option<DynamicId>,
    ) -> super::blacklight::BlacklightReceiver {
        let Some(context) = visibility else {
            self.key = None;
            self.value = super::blacklight::BlacklightReceiver::ZERO;
            return self.value;
        };
        let key = BlacklightReceiverKey {
            sources: context.sources.descriptors(None),
            scene: std::ptr::from_ref(context.scene).addr(),
            dynamic: context
                .dynamic
                .map_or(0, |value| std::ptr::from_ref(value).addr()),
            dynamic_revision: context.dynamic.map_or(0, DynamicVisibility::revision),
            model: matrix_bits(model),
            bounds: [bounds.min.map(f32::to_bits), bounds.max.map(f32::to_bits)],
            receiver,
        };
        if self.key.as_ref() != Some(&key) {
            self.value = context.receiver(bounds, model, receiver);
            self.key = Some(key);
        }
        self.value
    }
}

struct CachedMesh {
    mesh: Arc<DynamicMesh>,
    scene: Arc<TransportScene>,
}

struct Caster {
    id: DynamicId,
    mesh: usize,
    matrix: [[u32; 4]; 4],
    inverse: Mat4,
    bounds: Aabb,
    scene: Arc<TransportScene>,
}

/// Measured preparation and resident geometry counters. Triangle count is not
/// a complete allocator/RSS estimate; the cached ray resource also has BVHs.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct DynamicVisibilityStats {
    pub casters: usize,
    pub meshes: usize,
    pub triangles: usize,
    pub model_builds: usize,
    pub preparation_millis: f64,
}

/// Covers every object admitted by the ordinary dynamic scene's existing
/// limits. There is no second caster count or name-dependent classification.
#[derive(Default)]
pub struct DynamicVisibility {
    meshes: HashMap<usize, CachedMesh>,
    casters: Vec<Caster>,
    revision: u64,
    scene_revision: u64,
    model_builds: usize,
    preparation_millis: f64,
}

impl DynamicVisibility {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn stats(&self) -> DynamicVisibilityStats {
        DynamicVisibilityStats {
            casters: self.casters.len(),
            meshes: self.meshes.len(),
            triangles: self
                .meshes
                .values()
                .map(|mesh| mesh.scene.triangle_count())
                .sum(),
            model_builds: self.model_builds,
            preparation_millis: self.preparation_millis,
        }
    }

    /// Call after every object transform advanced and before receiver samples.
    /// A stationary scene performs no allocation, scene construction or update.
    pub fn sync(&mut self, dynamic: &DynamicScene) -> Result<bool, String> {
        let changed = dynamic.revision() != self.scene_revision
            || dynamic
                .objects()
                .iter()
                .filter(|object| object.visible())
                .count()
                != self.casters.len()
            || dynamic
                .objects()
                .iter()
                .filter(|object| object.visible())
                .zip(&self.casters)
                .any(|(object, previous)| {
                    object.id() != previous.id
                        || mesh_identity(object.mesh()) != previous.mesh
                        || matrix_bits(object.transform()) != previous.matrix
                });
        if !changed {
            return Ok(false);
        }
        for object in dynamic.objects().iter().filter(|object| object.visible()) {
            let transform = object.transform();
            let bounds = object.world_bounds();
            if !transform.is_finite()
                || !transform.inverse().is_finite()
                || !bounds
                    .min
                    .iter()
                    .chain(&bounds.max)
                    .all(|value| value.is_finite())
            {
                return Err(format!(
                    "invalid movable visibility transform for '{}'",
                    object.mesh().model_path
                ));
            }
            let identity = mesh_identity(object.mesh());
            if self.meshes.contains_key(&identity) {
                continue;
            }
            let started = std::time::Instant::now();
            let scene = super::light_transport::dynamic_mesh_visibility_scene(object.mesh())
                .ok_or_else(|| {
                    format!(
                        "cannot prepare movable visibility for '{}'",
                        object.mesh().model_path
                    )
                })?;
            self.preparation_millis = started
                .elapsed()
                .as_secs_f64()
                .mul_add(1000.0_f64, self.preparation_millis);
            self.model_builds = self.model_builds.saturating_add(1);
            drop(self.meshes.insert(
                identity,
                CachedMesh {
                    mesh: Arc::clone(object.mesh()),
                    scene: Arc::new(scene),
                },
            ));
        }
        self.casters.clear();
        for object in dynamic.objects().iter().filter(|object| object.visible()) {
            let transform = object.transform();
            let inverse = transform.inverse();
            let bounds = object.world_bounds();
            let identity = mesh_identity(object.mesh());
            let mesh = self
                .meshes
                .get(&identity)
                .ok_or("movable visibility cache entry is missing")?;
            self.casters.push(Caster {
                id: object.id(),
                mesh: identity,
                matrix: matrix_bits(transform),
                inverse,
                bounds,
                scene: Arc::clone(&mesh.scene),
            });
        }
        self.meshes.retain(|identity, cached| {
            dynamic.objects().iter().any(|object| {
                mesh_identity(object.mesh()) == *identity
                    && Arc::ptr_eq(object.mesh(), &cached.mesh)
            })
        });
        self.revision = self.revision.wrapping_add(1);
        self.scene_revision = dynamic.revision();
        Ok(true)
    }

    /// Visibility uses the drawn mesh/UV/alpha in its current world transform.
    /// Self exclusion is an object identity, independent of shared mesh names.
    #[must_use]
    pub fn transmittance(&self, from: [f32; 3], to: [f32; 3], excluded: Option<DynamicId>) -> f32 {
        if !from.iter().chain(&to).all(|value| value.is_finite()) {
            return 0.0;
        }
        let mut throughput = 1.0_f32;
        for caster in &self.casters {
            if excluded == Some(caster.id) || !segment_bounds_overlap(from, to, caster.bounds) {
                continue;
            }
            let local_from = caster
                .inverse
                .transform_point3(Vec3::from_array(from))
                .to_array();
            let local_to = caster
                .inverse
                .transform_point3(Vec3::from_array(to))
                .to_array();
            throughput *= caster.scene.transmittance(local_from, local_to);
            if throughput <= 0.0 {
                return 0.0;
            }
        }
        throughput
    }

    /// Transmission toward a source at infinity through the current drawn meshes.
    /// `incoming` points from the receiver toward the source. The finite endpoint
    /// lies beyond every caster's bounds, including the local ray endpoint's
    /// numerical allowance, so no arbitrary world-distance cap loses shadows.
    #[must_use]
    // Endpoint components are checked against finite f32 bounds before conversion.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "Finite f64 endpoint components are checked within f32 bounds before their stored-coordinate conversion."
    )]
    pub fn directional_transmittance(
        &self,
        origin: [f32; 3],
        incoming: [f32; 3],
        excluded: Option<DynamicId>,
    ) -> f32 {
        if !origin
            .iter()
            .chain(&incoming)
            .all(|value| value.is_finite())
        {
            return 0.0;
        }
        let origin_wide = Vec3::from_array(origin).as_dvec3();
        let direction_wide = Vec3::from_array(incoming).as_dvec3();
        let length = direction_wide.length();
        if length <= 0.0_f64 {
            return 0.0;
        }
        let unit = direction_wide.normalize();
        let mut distance = 0.0_f64;
        let mut allowance_scale = origin_wide.abs().max_element().max(1.0);
        for caster in self
            .casters
            .iter()
            .filter(|caster| excluded != Some(caster.id))
        {
            // The support corner gives the exact far projection of this AABB.
            let projection = caster
                .bounds
                .min
                .into_iter()
                .zip(caster.bounds.max)
                .zip(origin_wide.to_array())
                .zip(unit.to_array())
                .map(|(((minimum, maximum), start), direction)| {
                    let bound = if direction < 0.0_f64 {
                        minimum
                    } else {
                        maximum
                    };
                    (f64::from(bound) - start) * direction
                })
                .sum::<f64>();
            distance = distance.max(projection);
            allowance_scale = caster
                .bounds
                .min
                .iter()
                .chain(&caster.bounds.max)
                .fold(allowance_scale, |scale, value| {
                    scale.max(f64::from(value.abs()))
                });
            // The shared segment query shortens its far endpoint in model-local
            // units. Convert that tolerance to world-ray units before extending.
            let inverse = caster.inverse.as_dmat4();
            let local_origin = inverse.transform_point3(origin_wide);
            let local_rate = inverse.transform_vector3(unit).length();
            if !local_rate.is_finite() || local_rate <= 0.0_f64 {
                return 0.0;
            }
            allowance_scale =
                allowance_scale.max(local_origin.abs().max_element().max(1.0) / local_rate);
        }
        if distance <= 0.0_f64 {
            return 1.0;
        }
        let allowance = 8.0_f64
            * f64::from(crate::engine::lighting::transport::SURFACE_OFFSET_M)
            * (allowance_scale + distance);
        let endpoint = unit.mul_add(glam::DVec3::splat(distance + allowance), origin_wide);
        if !endpoint.is_finite() || endpoint.abs().max_element() > f64::from(f32::MAX) {
            return 0.0;
        }
        self.transmittance(
            origin,
            endpoint.to_array().map(|value| value as f32),
            excluded,
        )
    }

    /// Conservative full-cell visibility through current movable caster bounds.
    /// The same identity exclusion as mesh transmittance removes the receiver.
    #[must_use]
    pub fn local_region_visible(
        &self,
        source: Aabb,
        receivers: Aabb,
        excluded: Option<DynamicId>,
    ) -> bool {
        finite_region_bounds(source)
            && finite_region_bounds(receivers)
            && self
                .casters
                .iter()
                .filter(|caster| excluded != Some(caster.id))
                .all(|caster| !finite_light_region_overlaps(source, receivers, caster.bounds))
    }

    /// Conservative full-cell visibility toward a source at infinity.
    #[must_use]
    pub fn directional_region_visible(
        &self,
        receivers: Aabb,
        incoming: [f32; 3],
        excluded: Option<DynamicId>,
    ) -> bool {
        finite_region_bounds(receivers)
            && incoming.iter().all(|value| value.is_finite())
            && incoming.iter().any(|value| *value != 0.0)
            && self
                .casters
                .iter()
                .filter(|caster| excluded != Some(caster.id))
                .all(|caster| {
                    !directional_light_region_overlaps(incoming, receivers, caster.bounds)
                })
    }
}

fn finite_region_bounds(bounds: Aabb) -> bool {
    !bounds.is_empty()
        && bounds
            .min
            .iter()
            .chain(&bounds.max)
            .all(|value| value.is_finite())
}

fn mesh_identity(mesh: &Arc<DynamicMesh>) -> usize {
    std::ptr::from_ref(mesh.as_ref()).addr()
}

fn matrix_bits(matrix: Mat4) -> [[u32; 4]; 4] {
    matrix
        .to_cols_array_2d()
        .map(|column| column.map(f32::to_bits))
}

fn segment_bounds_overlap(from: [f32; 3], to: [f32; 3], bounds: Aabb) -> bool {
    from.into_iter()
        .zip(to)
        .zip(bounds.min.into_iter().zip(bounds.max))
        .all(|((a, b), (minimum, maximum))| a.min(b) <= maximum && a.max(b) >= minimum)
}
