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
    rounding_scale: f64,
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
    changed_from: u64,
    changed_bounds: Aabb,
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

    /// Bounds of both the old and new geometry for the latest revision only.
    /// A consumer that missed revisions must refresh from the complete scene.
    #[must_use]
    pub const fn changed_bounds_since(&self, revision: u64) -> Option<Aabb> {
        if revision == self.revision {
            Some(Aabb::EMPTY)
        } else if revision == self.changed_from {
            Some(self.changed_bounds)
        } else {
            None
        }
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
        let changed_bounds = self.changed_caster_bounds(dynamic);
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
                rounding_scale: placement_rounding_scale(transform, object.mesh().bounds),
                scene: Arc::clone(&mesh.scene),
            });
        }
        self.meshes.retain(|identity, cached| {
            dynamic.objects().iter().any(|object| {
                mesh_identity(object.mesh()) == *identity
                    && Arc::ptr_eq(object.mesh(), &cached.mesh)
            })
        });
        self.changed_from = self.revision;
        self.changed_bounds = changed_bounds;
        self.revision = self.revision.wrapping_add(1);
        self.scene_revision = dynamic.revision();
        Ok(true)
    }

    fn changed_caster_bounds(&self, dynamic: &DynamicScene) -> Aabb {
        let objects = || dynamic.objects().iter().filter(|object| object.visible());
        let same_layout = objects().count() == self.casters.len()
            && objects()
                .zip(&self.casters)
                .all(|(object, previous)| object.id() == previous.id);
        let mut changed_bounds = Aabb::EMPTY;
        if same_layout {
            for (object, previous) in objects().zip(&self.casters) {
                if mesh_identity(object.mesh()) != previous.mesh
                    || matrix_bits(object.transform()) != previous.matrix
                {
                    changed_bounds = changed_bounds
                        .union(&previous.bounds)
                        .union(&object.world_bounds());
                }
            }
        } else {
            // Admission/removal/reordering is uncommon. Cover every former and
            // current caster rather than retaining an incomplete positional diff.
            for previous in &self.casters {
                changed_bounds = changed_bounds.union(&previous.bounds);
            }
            for object in objects() {
                changed_bounds = changed_bounds.union(&object.world_bounds());
            }
        }
        changed_bounds
    }

    /// Visibility uses the drawn mesh/UV/alpha in its current world transform.
    /// Self exclusion is an object identity, independent of shared mesh names.
    #[must_use]
    pub fn transmittance(&self, from: [f32; 3], to: [f32; 3], excluded: Option<DynamicId>) -> f32 {
        if !from.iter().chain(&to).all(|value| value.is_finite()) {
            return 0.0;
        }
        if self.casters.is_empty() {
            return 1.0;
        }
        // Small sets already reject cheaply in each model's BVH. Amortize the
        // additional world-space slabs only across a large caster set.
        if self.casters.len() < 64 {
            self.transmittance_candidates::<false>(from, to, excluded)
        } else {
            self.transmittance_candidates::<true>(from, to, excluded)
        }
    }

    fn transmittance_candidates<const REFINE: bool>(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        excluded: Option<DynamicId>,
    ) -> f32 {
        let mut throughput = 1.0_f32;
        let segment = REFINE.then(|| VisibilitySegment::new(from, to));
        for caster in &self.casters {
            if excluded == Some(caster.id)
                || !segment_bounds_overlap(from, to, caster.bounds)
                || segment
                    .as_ref()
                    .is_some_and(|query| !query.overlaps(caster.bounds, caster.rounding_scale))
            {
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
        if !finite_region_bounds(source) || !finite_region_bounds(receivers) {
            return false;
        }
        let envelope = source.union(&receivers);
        self.casters
            .iter()
            .filter(|caster| excluded != Some(caster.id))
            .all(|caster| {
                !light_region_bounds_overlap(envelope, caster.bounds)
                    || !finite_light_region_overlaps(source, receivers, caster.bounds)
            })
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

/// The finite source/receiver hull lies inside this enclosing box. Reject only
/// clear misses before constructing the full f64 separating-axis certificate.
/// Caster bounds were validated by sync; source and receiver bounds by caller.
fn light_region_bounds_overlap(envelope: Aabb, blocker: Aabb) -> bool {
    let scale = envelope
        .min
        .into_iter()
        .chain(envelope.max)
        .chain(blocker.min)
        .chain(blocker.max)
        .fold(1.0_f64, |scale, value| scale.max(f64::from(value.abs())));
    // Wider than the full certificate's f64 projection allowance, including
    // very small boxes at large coordinates. Boundary cases keep the full test.
    let padding = scale * f64::from(f32::EPSILON) * 4.0_f64;
    envelope
        .min
        .into_iter()
        .zip(envelope.max)
        .zip(blocker.min.into_iter().zip(blocker.max))
        .all(|((low, high), (minimum, maximum))| {
            f64::from(low) <= f64::from(maximum) + padding
                && f64::from(minimum) <= f64::from(high) + padding
        })
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

/// Bound the magnitude of placement arithmetic before cancellation. A large
/// imported pivot translated back to the origin still needs its f32 rounding
/// allowance, even when the final world bounds and ray endpoints are small.
fn placement_rounding_scale(transform: Mat4, local: Aabb) -> f64 {
    let [x, y, z] = std::array::from_fn(|axis| {
        f64::from(local.min.get(axis).copied().unwrap_or(0.0_f32).abs()).max(f64::from(
            local.max.get(axis).copied().unwrap_or(0.0_f32).abs(),
        ))
    });
    let mut rows = [0.0_f64; 3];
    for (column, extent) in transform
        .to_cols_array_2d()
        .into_iter()
        .zip([x, y, z, 1.0_f64])
    {
        for (sum, coefficient) in rows.iter_mut().zip(column) {
            *sum = f64::from(coefficient.abs()).mul_add(extent, *sum);
        }
    }
    rows.into_iter().fold(1.0_f64, f64::max)
}

/// Reject clear diagonal misses before transforming a ray into a mesh BVH.
/// Double precision and outward padding retain boundary/near-parallel cases;
/// uncertain overlaps still use the unchanged triangle/alpha query.
struct VisibilitySegment {
    origin: [f64; 3],
    inverse: [f64; 3],
    parallel: [bool; 3],
    scale: f64,
}

impl VisibilitySegment {
    fn new(from: [f32; 3], to: [f32; 3]) -> Self {
        let origin = from.map(f64::from);
        let delta: [f64; 3] = std::array::from_fn(|axis| {
            f64::from(to.get(axis).copied().unwrap_or(0.0_f32))
                - origin.get(axis).copied().unwrap_or(0.0_f64)
        });
        Self {
            origin,
            inverse: delta.map(|value| {
                if value == 0.0_f64 {
                    0.0_f64
                } else {
                    value.recip()
                }
            }),
            parallel: delta.map(|value| value == 0.0_f64),
            scale: from
                .into_iter()
                .chain(to)
                .fold(1.0_f64, |scale, value| scale.max(f64::from(value.abs()))),
        }
    }

    fn overlaps(&self, bounds: Aabb, placement_scale: f64) -> bool {
        // Covers f32 transform/triangle rounding as well as the slab division.
        // It only admits more candidates, never changes a mesh's ray answer.
        let scale = bounds
            .min
            .into_iter()
            .chain(bounds.max)
            .fold(self.scale.max(placement_scale), |scale, value| {
                scale.max(f64::from(value.abs()))
            });
        let padding = scale * f64::from(f32::EPSILON) * 16.0_f64;
        let mut entry = 0.0_f64;
        let mut exit = 1.0_f64;
        for (((origin, inverse), parallel), (minimum, maximum)) in self
            .origin
            .into_iter()
            .zip(self.inverse)
            .zip(self.parallel)
            .zip(bounds.min.into_iter().zip(bounds.max))
        {
            let low = f64::from(minimum) - padding;
            let high = f64::from(maximum) + padding;
            if parallel {
                if origin < low || origin > high {
                    return false;
                }
            } else {
                let a = (low - origin) * inverse;
                let b = (high - origin) * inverse;
                entry = entry.max(a.min(b));
                exit = exit.min(a.max(b));
                if entry > exit {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
#[path = "../../tests/dynamic_ray_bounds.rs"]
mod bounds_tests;
