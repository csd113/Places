//! Runtime probe support, direct-source selection and receiver visibility.

use super::LevelLighting;
use super::probes::{MAX_RUNTIME_PROBE_LIGHTS, probe_light_enabled};
use super::transport::{TransportEmitter, TransportScene};
use crate::engine::entity::DynamicId;
use crate::engine::spatial::Aabb;

/// Practical sources re-evaluated for movable receivers. The identities are
/// chosen once per compiled field, so movement cannot reorder the live set.
const MAX_ENTITY_DIRECT_LIGHTS: usize = MAX_RUNTIME_PROBE_LIGHTS;

/// Select the strongest always-on sources without discarding weaker baked
/// sources. Switchable prepared groups reserve live slots because their direct
/// contribution is absent from the base field. Ties retain source order.
#[must_use]
pub fn select_baked_direct_lights(
    lighting: &crate::engine::lighting::LevelLighting,
    switchable: &[usize],
) -> Vec<u32> {
    let mut candidates = lighting
        .lights()
        .iter()
        .enumerate()
        .filter(|(index, light)| light.is_active() && !switchable.contains(index))
        .filter_map(|(index, light)| {
            let color = light.color();
            let strength = light.intensity() * light.height_factor * (color.r + color.g + color.b);
            Some((u32::try_from(index).ok()?, strength))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let reserved = switchable
        .iter()
        .enumerate()
        .filter(|(slot, index)| {
            lighting.lights().get(**index).is_some()
                && !switchable.iter().take(*slot).any(|other| other == *index)
        })
        .take(MAX_ENTITY_DIRECT_LIGHTS)
        .count();
    candidates.truncate(MAX_ENTITY_DIRECT_LIGHTS.saturating_sub(reserved));
    candidates.sort_unstable_by_key(|candidate| candidate.0);
    candidates
        .into_iter()
        .map(|candidate| candidate.0)
        .collect()
}

/// The runtime environment sample. Prepared values stay linear HDR until
/// the material shader reconstructs diffuse lighting at its world normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityLighting {
    pub prepared: Option<crate::engine::lighting::lightmap::LightmapTexel>,
    /// Legacy field name; values are linear HDR, never encoded display RGB.
    pub display: [f32; 3],
    pub source: EntityLightingSource,
}

/// Detectable fallback reasons; darkness in a valid probe is intentional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityLightingSource {
    Prepared,
    NoField,
    Roomless,
    Unresolved,
    InvalidPosition,
}

/// Eight support points inside the model-space bounds; shader interpolation
/// uses the same X-fastest binary corner order after the model transform.
pub const ENTITY_LIGHTING_ANCHORS: usize = 8;

// Bounds extrema can lie inside adjoining geometry at normal assembly joints.
// Keep support inside the receiver, by at most 1 cm or 2% of each local axis.
// This relocates visibility queries; it does not add illumination or skip solids.
const ENTITY_ANCHOR_MAX_INSET_M: f32 = 0.01;
const ENTITY_ANCHOR_INSET_FRACTION: f32 = 0.02;

/// One bounded source and its deterministic finite-emitter taps. Stored
/// strength has the compiler's calibration but no receiver cosine/falloff.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct EntityDirectLight {
    pub position_range: [f32; 4],
    pub color_strength: [f32; 4],
    /// Horizontal half extents, falloff selector, directional flag.
    pub extent_falloff: [f32; 4],
    /// World positions; w is each tap's normalized integration weight.
    pub taps: [[f32; 4]; 4],
}

impl EntityDirectLight {
    pub const ZERO: Self = Self {
        position_range: [0.0; 4],
        color_strength: [0.0; 4],
        extent_falloff: [0.0; 4],
        taps: [[0.0; 4]; 4],
    };

    fn from_baked(light: &crate::engine::lighting::BakedLight) -> Self {
        let emitter = TransportEmitter::from_baked(light, None);
        let [x, y, z] = emitter.position;
        let [red, green, blue] = emitter.color;
        let (half_x, half_z) = light.half_extents();
        let offsets = emitter.shape.samples(2);
        let count = u16::try_from(offsets.len()).unwrap_or(1).max(1);
        let mut taps = [[0.0; 4]; 4];
        for (tap, offset) in taps.iter_mut().zip(offsets) {
            let [dx, dy, dz] = offset;
            *tap = [x + dx, y + dy, z + dz, 1.0 / f32::from(count)];
        }
        Self {
            position_range: [x, y, z, emitter.range],
            color_strength: [
                red,
                green,
                blue,
                crate::engine::lighting::LOCAL_LIGHT_STRENGTH
                    * emitter.intensity
                    * emitter.height_factor,
            ],
            extent_falloff: [
                half_x,
                half_z,
                match emitter.falloff {
                    crate::engine::lighting::LightFalloff::Smooth => 0.0,
                    crate::engine::lighting::LightFalloff::Linear => 1.0,
                    crate::engine::lighting::LightFalloff::Constant => 2.0,
                },
                if emitter.directional { 1.0 } else { 0.0 },
            ],
            taps,
        }
    }
}

/// Residual field coefficients and important-source visibility at one anchor.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct EntityLightingAnchor {
    pub irradiance: [f32; 4],
    pub moment: [f32; 4],
    pub visibility: [[f32; 4]; MAX_ENTITY_DIRECT_LIGHTS],
    pub attenuation: [f32; 4],
}

/// A spatial lighting payload independent of pose-vertex uploads. The combined
/// centre sample remains available separately for diagnostics/legacy callers.
#[derive(Debug, PartialEq)]
pub struct EntitySpatialLighting {
    /// Actual world query sites; virtual interpolation bounds remain unchanged.
    pub support_positions: [[f32; 3]; ENTITY_LIGHTING_ANCHORS],
    pub uses_surface_support: bool,
    pub bounds_min: [f32; 4],
    pub bounds_extent: [f32; 4],
    pub anchors: [EntityLightingAnchor; ENTITY_LIGHTING_ANCHORS],
    pub direct: [EntityDirectLight; MAX_ENTITY_DIRECT_LIGHTS],
}

impl EntitySpatialLighting {
    /// The eight world support positions represented by this sampled payload.
    /// `model` must be the transform retained by the payload's cache key.
    #[must_use]
    pub const fn diagnostic_world_anchors(
        &self,
        model: glam::Mat4,
    ) -> [[f32; 3]; ENTITY_LIGHTING_ANCHORS] {
        let _ = model;
        self.support_positions
    }
}

/// Sampling positions may recover an unsupported virtual box corner from the
/// current receiver surface. Boundary assemblies retain their unmapped policy.
#[derive(Clone, Copy, Default)]
pub(in crate::engine) struct EntitySamplingOptions {
    pub allow_unmapped: bool,
    pub supports: Option<[[f32; 3]; ENTITY_LIGHTING_ANCHORS]>,
}

/// A box corner can lie inside a wall while a concave/deformed receiver fits an
/// opening. Such a virtual point is not a dark piece of the receiver. Recover
/// only static-air-invalid corners from the closest actual current surface
/// point in valid air; every probe and source still performs ordinary world and
/// movable-occluder visibility at that point. Valid-but-occluded sites are never
/// moved, and absent geometry/support retains the conservative original result.
/// The iterator is lazy: ordinary valid boxes require no extra skinning.
pub(in crate::engine) fn receiver_surface_supports(
    lighting: &LevelLighting,
    bounds: crate::engine::spatial::Aabb,
    model: glam::Mat4,
    points: impl Iterator<Item = [f32; 3]>,
) -> Option<[[f32; 3]; ENTITY_LIGHTING_ANCHORS]> {
    let positions = entity_anchor_positions(bounds, model)?;
    let missing = positions
        .world
        .map(|point| !lighting.probe_visible_from(point, point));
    if !missing.iter().any(|value| *value) {
        return None;
    }
    let mut supports = positions.world;
    let mut distances = [f32::INFINITY; ENTITY_LIGHTING_ANCHORS];
    for local in points {
        let world = model.transform_point3(glam::Vec3::from_array(local));
        if !world.is_finite() {
            continue;
        }
        let candidate_distances = positions
            .world
            .map(|corner| world.distance_squared(glam::Vec3::from_array(corner)));
        if !missing
            .iter()
            .zip(&distances)
            .zip(&candidate_distances)
            .any(|((unsupported, current), candidate)| *unsupported && candidate < current)
            || !lighting.probe_visible_from(world.to_array(), world.to_array())
        {
            continue;
        }
        for (((unsupported, distance), candidate), support) in missing
            .iter()
            .zip(&mut distances)
            .zip(candidate_distances)
            .zip(&mut supports)
        {
            if *unsupported && candidate < *distance {
                *distance = candidate;
                *support = world.to_array();
            }
        }
    }
    Some(supports)
}

struct EntityAnchorPositions {
    world: [[f32; 3]; ENTITY_LIGHTING_ANCHORS],
    centre: [f32; 3],
    minimum: glam::Vec3,
    extent: glam::Vec3,
}

fn entity_anchor_positions(
    bounds: crate::engine::spatial::Aabb,
    model: glam::Mat4,
) -> Option<EntityAnchorPositions> {
    let mut minimum = glam::Vec3::from_array(bounds.min);
    let mut maximum = glam::Vec3::from_array(bounds.max);
    if !minimum.is_finite() || !maximum.is_finite() || !model.is_finite() {
        return None;
    }
    let original_extent = glam::Vec3::new(
        maximum.x - minimum.x,
        maximum.y - minimum.y,
        maximum.z - minimum.z,
    );
    if !original_extent.is_finite() || original_extent.min_element() < 0.0 {
        return None;
    }
    let [inset_x, inset_y, inset_z] = original_extent
        .to_array()
        .map(|axis| (axis * ENTITY_ANCHOR_INSET_FRACTION).min(ENTITY_ANCHOR_MAX_INSET_M));
    minimum = glam::Vec3::new(
        minimum.x + inset_x,
        minimum.y + inset_y,
        minimum.z + inset_z,
    );
    maximum = glam::Vec3::new(
        maximum.x - inset_x,
        maximum.y - inset_y,
        maximum.z - inset_z,
    );
    let extent = glam::Vec3::new(
        maximum.x - minimum.x,
        maximum.y - minimum.y,
        maximum.z - minimum.z,
    )
    .max(glam::Vec3::splat(1.0e-6));
    let centre = model
        .transform_point3(minimum.lerp(maximum, 0.5))
        .to_array();
    let world: [[f32; 3]; ENTITY_LIGHTING_ANCHORS] = std::array::from_fn(|corner| {
        let local = glam::Vec3::new(
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
        model.transform_point3(local).to_array()
    });
    if !centre
        .iter()
        .chain(world.iter().flatten())
        .all(|value| value.is_finite())
    {
        return None;
    }
    Some(EntityAnchorPositions {
        world,
        centre,
        minimum,
        extent,
    })
}

/// Input identity of the immutable field, model transform and active sources.
/// A stationary entity does not repeat probe or visibility work each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntitySpatialKey {
    field: usize,
    scene: usize,
    model: [[u32; 4]; 4],
    enabled: u64,
    dynamic: usize,
    dynamic_revision: u64,
    receiver: Option<DynamicId>,
}

impl EntitySpatialKey {
    /// The actual model transform used when the cached anchors were sampled.
    #[must_use]
    pub fn diagnostic_model(self) -> glam::Mat4 {
        glam::Mat4::from_cols_array_2d(&self.model.map(|column| column.map(f32::from_bits)))
    }

    /// Read-only cache identities for explicit native acceptance traces.
    #[must_use]
    pub fn diagnostic_snapshot(self) -> serde_json::Value {
        serde_json::json!({
            "field_identity": self.field,
            "transport_identity": self.scene,
            "sampled_model": self.diagnostic_model().to_cols_array_2d(),
            "enabled_mask": self.enabled,
            "visibility_identity": self.dynamic,
            "visibility_revision": self.dynamic_revision,
            "receiver": self.receiver.map(|id| format!("{id:?}")),
        })
    }
}

// Each receiver admits at most 2,048 exact ray answers. On overflow it computes
// normally; content, probe candidates and shadow taps are never reduced.
#[derive(Debug, Default)]
pub struct EntityStaticRayCache {
    scope: Option<(Option<EntitySpatialKey>, [[u32; 3]; 2], u64)>,
    rays: std::collections::HashMap<StaticRayKey, f32>,
    complete: bool,
    static_queries: u64,
    refresh_count: u64,
    last_refresh_ms: f64,
}

impl EntityStaticRayCache {
    fn prepare(
        &mut self,
        mut key: Option<EntitySpatialKey>,
        bounds: crate::engine::spatial::Aabb,
        revision: u64,
    ) {
        if let Some(identity) = &mut key {
            identity.dynamic = 0;
            identity.dynamic_revision = 0;
            identity.receiver = None;
            // Switch state changes irradiance, not immutable ray visibility.
            identity.enabled = 0;
        }
        let scope = (
            key,
            [bounds.min.map(f32::to_bits), bounds.max.map(f32::to_bits)],
            revision,
        );
        if self.scope.as_ref() != Some(&scope) {
            self.rays.clear();
            self.complete = true;
            self.scope = Some(scope);
        }
    }

    /// Changed receiver support geometry invalidates its recorded ray coverage.
    pub(in crate::engine) fn invalidate_receiver_shape(&mut self) {
        self.scope = None;
        self.rays.clear();
        self.complete = false;
    }

    /// Called only for source-energy changes while field and geometry remain
    /// installed. Ordinary resource revisions still invalidate the entire scope.
    pub(in crate::engine) const fn advance_source_revision(
        &mut self,
        previous: u64,
        revision: u64,
    ) {
        if let Some((_, _, observed)) = self.scope.as_mut()
            && *observed == previous
        {
            *observed = revision;
        }
    }
    #[cfg(feature = "visual-diagnostics")]
    pub(in crate::engine) fn record_refresh(&mut self, started: std::time::Instant) {
        self.refresh_count = self.refresh_count.saturating_add(1);
        self.last_refresh_ms = started.elapsed().as_secs_f64() * 1000.0_f64;
    }

    pub(in crate::engine) fn diagnostic_snapshot(&self) -> serde_json::Value {
        serde_json::json!({"entries":self.rays.len(),"complete":self.complete,"static_queries":self.static_queries,
            "refreshes":self.refresh_count,"last_refresh_ms":self.last_refresh_ms})
    }

    /// Every possible dynamic query follows a recorded immutable ray first.
    /// Off-path old/new caster bounds preserve the complete receiver result.
    /// Over-cap scopes never justify reuse; missed revisions are caller-owned.
    pub(in crate::engine) fn visibility_affected(
        &self,
        dirty: crate::engine::spatial::Aabb,
    ) -> bool {
        if !self.complete {
            return true;
        }
        self.rays.iter().any(|(ray, value)| {
            if *value <= 0.0 {
                return false;
            }
            let from = ray.from.map(f32::from_bits);
            let to = ray.to.map(f32::from_bits);
            let scale = from
                .into_iter()
                .chain(to)
                .chain(dirty.min)
                .chain(dirty.max)
                .map(|v| f64::from(v.abs()))
                .fold(1.0_f64, f64::max);
            let padding = scale * f64::from(f32::EPSILON) * 8.0_f64;
            !value.is_finite()
                || from.into_iter().zip(to).zip(dirty.min).zip(dirty.max).all(
                    |(((a, b), low), high)| {
                        !a.is_finite()
                            || !b.is_finite()
                            || !low.is_finite()
                            || !high.is_finite()
                            || (f64::from(a.min(b)) <= f64::from(high) + padding
                                && f64::from(a.max(b)) >= f64::from(low) - padding)
                    },
                )
        })
    }
}

fn static_probe_visible(
    cache: &std::cell::RefCell<Option<&mut EntityStaticRayCache>>,
    lighting: &LevelLighting,
    point: [f32; 3],
    probe: [f32; 3],
    label: i32,
) -> bool {
    static_ray(cache, 0, i64::from(label), point, probe, || {
        f32::from(u8::from(
            lighting.labelled_probe_visible_from(point, probe, label),
        ))
    }) > 0.0
}

fn static_segment_visible(
    cache: &std::cell::RefCell<Option<&mut EntityStaticRayCache>>,
    lighting: &LevelLighting,
    scene: Option<&TransportScene>,
    from: [f32; 3],
    to: [f32; 3],
) -> bool {
    scene.map_or_else(
        || {
            static_ray(cache, 1, 0, from, to, || {
                f32::from(u8::from(lighting.runtime_probe_segment_visible(from, to)))
            }) > 0.0
        },
        |transport| static_ray(cache, 2, 0, from, to, || transport.transmittance(from, to)) > 0.0,
    )
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StaticRayKey {
    kind: u8,
    tag: i64,
    from: [u32; 3],
    to: [u32; 3],
}

fn static_ray(
    cache: &std::cell::RefCell<Option<&mut EntityStaticRayCache>>,
    kind: u8,
    tag: i64,
    from: [f32; 3],
    to: [f32; 3],
    compute: impl FnOnce() -> f32,
) -> f32 {
    let mut borrowed = cache.borrow_mut();
    let Some(storage) = borrowed.as_deref_mut() else {
        return compute();
    };
    let key = StaticRayKey {
        kind,
        tag,
        from: from.map(f32::to_bits),
        to: to.map(f32::to_bits),
    };
    if let Some(value) = storage.rays.get(&key) {
        return *value;
    }
    let value = compute();
    storage.static_queries = storage.static_queries.saturating_add(1);
    if storage.rays.len() < 2048 {
        let _previous = storage.rays.insert(key, value);
    } else {
        storage.complete = false;
    }
    value
}

/// A read-only snapshot of current movable geometry. Geometry gathering and
/// revision publication stay with the caller.
pub trait MovableOccluders: Sync {
    fn revision(&self) -> u64;
    fn transmittance(&self, from: [f32; 3], to: [f32; 3], receiver: Option<DynamicId>) -> f32;
}

/// Optional current movable geometry and the rigid receiver's self identity.
#[derive(Clone, Copy, Default)]
pub struct EntityVisibility<'a> {
    pub dynamic: Option<&'a dyn MovableOccluders>,
    pub receiver: Option<DynamicId>,
}

impl EntityVisibility<'_> {
    fn transmittance(self, from: [f32; 3], to: [f32; 3]) -> f32 {
        self.dynamic
            .map_or(1.0, |scene| scene.transmittance(from, to, self.receiver))
    }
}

/// Combined-field subtraction concerns only selected always-on IDs. A
/// switchable emitter already lives outside the base solve and joins the
/// reserved live slots directly, retaining its identity while disabled.
fn runtime_entity_light_ids(
    field: &crate::engine::lighting::probes::ProbeField,
    scene: Option<&TransportScene>,
) -> [Option<usize>; MAX_ENTITY_DIRECT_LIGHTS] {
    let mut ids = [None; MAX_ENTITY_DIRECT_LIGHTS];
    let mut count = 0_usize;
    for index in field
        .runtime_direct_lights()
        .iter()
        .filter_map(|id| usize::try_from(*id).ok())
    {
        if let Some(slot) = ids.get_mut(count) {
            *slot = Some(index);
            count = count.saturating_add(1);
        }
    }
    if let Some(transport) = scene {
        for index in transport
            .emitters()
            .iter()
            .filter_map(|source| source.switchable)
            .take(MAX_ENTITY_DIRECT_LIGHTS)
        {
            if ids.contains(&Some(index)) {
                continue;
            }
            if let Some(slot) = ids.get_mut(count) {
                *slot = Some(index);
                count = count.saturating_add(1);
            }
        }
    }
    ids
}

/// One synchronous receiver batch borrows a complete lighting/visibility
/// snapshot. Source selection and direct payloads initialize only when a dirty
/// receiver needs spatial samples, then all workers borrow the same result.
/// Joining the batch ends this lifetime; no cross-frame or cross-map cache.
pub(in crate::engine) struct EntitySpatialLightingContext<'a> {
    lighting: &'a LevelLighting,
    field: Option<&'a crate::engine::lighting::probes::ProbeField>,
    scene: Option<&'a TransportScene>,
    dynamic: Option<&'a dyn MovableOccluders>,
    sources: std::sync::OnceLock<Option<EntitySpatialSources>>,
}

struct EntitySpatialSources {
    ids: [Option<usize>; MAX_ENTITY_DIRECT_LIGHTS],
    direct: [EntityDirectLight; MAX_ENTITY_DIRECT_LIGHTS],
    enabled: u64,
}

impl<'a> EntitySpatialLightingContext<'a> {
    pub(in crate::engine) const fn new(
        lighting: &'a LevelLighting,
        field: Option<&'a crate::engine::lighting::probes::ProbeField>,
        scene: Option<&'a TransportScene>,
        dynamic: Option<&'a dyn MovableOccluders>,
    ) -> Self {
        Self {
            lighting,
            field,
            scene,
            dynamic,
            sources: std::sync::OnceLock::new(),
        }
    }

    fn sources(&self) -> Option<&EntitySpatialSources> {
        self.sources
            .get_or_init(|| {
                let lighting = self.lighting;
                let prepared = self.field?;
                let _selected_direct = prepared.local_direct.as_ref()?;
                let ids = runtime_entity_light_ids(prepared, self.scene);
                let mut enabled = 0_u64;
                for (slot, index) in ids.iter().enumerate() {
                    let active = index
                        .and_then(|id| lighting.lights().get(id))
                        .is_some_and(crate::engine::lighting::BakedLight::is_active);
                    if active {
                        enabled |= 1_u64.checked_shl(u32::try_from(slot).ok()?).unwrap_or(0);
                    }
                }
                for (group, plane) in prepared.switchable_indirect.iter().enumerate() {
                    if probe_light_enabled(lighting, plane.light_index) {
                        let bit = group.saturating_add(MAX_ENTITY_DIRECT_LIGHTS);
                        enabled |= 1_u64.checked_shl(u32::try_from(bit).ok()?).unwrap_or(0);
                    }
                }
                let mut direct = [EntityDirectLight::ZERO; MAX_ENTITY_DIRECT_LIGHTS];
                for (slot, index) in direct.iter_mut().zip(ids) {
                    if let Some(light) = index.and_then(|id| lighting.lights().get(id))
                        && light.is_active()
                    {
                        *slot = EntityDirectLight::from_baked(light);
                    }
                }
                Some(EntitySpatialSources {
                    ids,
                    direct,
                    enabled,
                })
            })
            .as_ref()
    }

    pub(in crate::engine) fn key(
        &self,
        model: glam::Mat4,
        receiver: Option<DynamicId>,
    ) -> Option<EntitySpatialKey> {
        Some(EntitySpatialKey {
            field: self
                .field
                .map_or(0, |field| std::ptr::from_ref(field).addr()),
            scene: self
                .scene
                .map_or(0, |scene| std::ptr::from_ref(scene).addr()),
            model: model
                .to_cols_array_2d()
                .map(|column| column.map(f32::to_bits)),
            enabled: self.sources()?.enabled,
            dynamic: self
                .dynamic
                .map_or(0, |dynamic| std::ptr::from_ref(dynamic).cast::<()>().addr()),
            dynamic_revision: self.dynamic.map_or(0, MovableOccluders::revision),
            receiver,
        })
    }
}

fn sample_receiver_irradiance(
    prepared: &crate::engine::lighting::probes::ProbeField,
    lighting: &LevelLighting,
    visibility: EntityVisibility<'_>,
    cache: &std::cell::RefCell<Option<&mut EntityStaticRayCache>>,
    point: [f32; 3],
) -> Option<crate::engine::lighting::lightmap::LightmapTexel> {
    let visible = lighting.labelled_probe_visibility(point);
    prepared.sample_nonlocal_with_switchable_indirect(
        point,
        None,
        |index| probe_light_enabled(lighting, index),
        |probe, label| {
            static_ray(cache, 0, i64::from(label), point, probe, || {
                f32::from(u8::from(visible(probe, label)))
            }) > 0.0
                && visibility.transmittance(point, probe) > 0.0
        },
    )
}

/// Reuses immutable ray answers while still evaluating every current movable caster.
#[cfg(any(test, feature = "visual-diagnostics"))]
#[must_use]
pub fn entity_spatial_lighting_cached(
    lighting: &LevelLighting,
    field: Option<&crate::engine::lighting::probes::ProbeField>,
    bounds: crate::engine::spatial::Aabb,
    model: glam::Mat4,
    scene: Option<&TransportScene>,
    visibility: EntityVisibility<'_>,
    storage: Option<(&mut EntityStaticRayCache, u64)>,
) -> Option<Box<EntitySpatialLighting>> {
    let context = EntitySpatialLightingContext::new(lighting, field, scene, visibility.dynamic);
    let key = context.key(model, visibility.receiver);
    context.sample(
        bounds,
        model,
        visibility.receiver,
        key,
        storage,
        EntitySamplingOptions::default(),
    )
}

impl EntitySpatialLightingContext<'_> {
    pub(in crate::engine) fn sample(
        &self,
        bounds: crate::engine::spatial::Aabb,
        model: glam::Mat4,
        receiver: Option<DynamicId>,
        context_key: Option<EntitySpatialKey>,
        storage: Option<(&mut EntityStaticRayCache, u64)>,
        options: EntitySamplingOptions,
    ) -> Option<Box<EntitySpatialLighting>> {
        let lighting = self.lighting;
        let field = self.field;
        let scene = self.scene;
        let visibility = EntityVisibility {
            dynamic: self.dynamic,
            receiver,
        };
        let active_cache = storage.map(|(entry, revision)| {
            entry.prepare(context_key, bounds, revision);
            entry
        });
        let cache = std::cell::RefCell::new(active_cache);
        let prepared = field?;
        let _selected_direct = prepared.local_direct.as_ref()?;
        let mut positions = entity_anchor_positions(bounds, model)?;
        if let Some(supports) = options.supports {
            positions.world = supports;
        }
        let sample =
            |point| sample_receiver_irradiance(prepared, lighting, visibility, &cache, point);
        let samples = positions.world.map(|world| (world, sample(world)));
        let centre_sample = sample(positions.centre);
        // Declared boundary assemblies may occupy wall/shaft stock without a
        // mapped air cell. Their unsupported bounce is zero, while the same
        // actual-geometry visibility still evaluates prepared direct sources.
        // They must not recover the roomless 0.1 ambient fallback.
        if !options.allow_unmapped
            && centre_sample.is_none()
            && samples.iter().all(|(_, value)| value.is_none())
        {
            let has_static_support = |point| {
                prepared
                    .sample_nonlocal_with_switchable_indirect(
                        point,
                        None,
                        |index| probe_light_enabled(lighting, index),
                        |probe, label| static_probe_visible(&cache, lighting, point, probe, label),
                    )
                    .is_some()
            };
            if visibility.dynamic.is_none()
                || !(has_static_support(positions.centre)
                    || positions.world.into_iter().any(has_static_support))
            {
                return None;
            }
            // A current opaque caster blocking all otherwise-valid support must
            // not recover the stale authored ambient/direct fallback.
        }
        let connected = |from, to| {
            static_segment_visible(&cache, lighting, scene, from, to)
                && visibility.transmittance(from, to) > 0.0
        };
        let sources = self.sources()?;
        let mut direct = sources.direct;
        let source_ids = &sources.ids;
        let anchors = samples.map(|(world, exact_sample)| {
            let texel = exact_sample
                .or_else(|| centre_sample.filter(|_| connected(world, positions.centre)))
                .or_else(|| {
                    samples
                        .iter()
                        .find_map(|(position, value)| value.filter(|_| connected(world, *position)))
                })
                .unwrap_or(crate::engine::lighting::lightmap::LightmapTexel {
                    irradiance: [0.0; 3],
                    direction: [0.0; 3],
                    axis: [0.5; 2],
                });
            let [red, green, blue] = texel.irradiance;
            let [x, y, z] = texel.direction;
            let throughput = spatial_anchor_visibility(
                lighting, source_ids, scene, &direct, world, visibility, &cache,
            );
            let [ar, ag, ab] = scene.map_or([1.0; 3], |transport| transport.attenuation_at(world));
            EntityLightingAnchor {
                irradiance: [
                    red,
                    green,
                    blue,
                    if exact_sample.is_some() { 1.0 } else { 0.0 },
                ],
                moment: [x, y, z, 0.0],
                visibility: throughput,
                attenuation: [ar, ag, ab, 0.0],
            }
        });
        disable_fully_occluded_entity_lights(&mut direct, &anchors);
        let [x, y, z] = positions.minimum.to_array();
        let [dx, dy, dz] = positions.extent.to_array();
        Some(Box::new(EntitySpatialLighting {
            support_positions: positions.world,
            uses_surface_support: options.supports.is_some(),
            bounds_min: [x, y, z, 1.0],
            bounds_extent: [dx, dy, dz, 0.0],
            anchors,
            direct,
        }))
    }
}

fn disable_fully_occluded_entity_lights(
    sources: &mut [EntityDirectLight; MAX_ENTITY_DIRECT_LIGHTS],
    anchors: &[EntityLightingAnchor; ENTITY_LIGHTING_ANCHORS],
) {
    for (slot, source) in sources.iter_mut().enumerate() {
        // Clamped trilinear weights are nonnegative. A source whose every tap
        // is zero at every support point is therefore zero for all fragments.
        // Keep its descriptor/taps intact and clear only the strength, which
        // the shader checks before falloff and visibility interpolation.
        if anchors.iter().all(|anchor| {
            anchor
                .visibility
                .get(slot)
                .is_some_and(|taps| taps.iter().all(|value| *value == 0.0))
        }) {
            let [red, green, blue, _strength] = source.color_strength;
            source.color_strength = [red, green, blue, 0.0];
        }
    }
}

fn spatial_anchor_visibility(
    lighting: &LevelLighting,
    source_ids: &[Option<usize>; MAX_ENTITY_DIRECT_LIGHTS],
    scene: Option<&TransportScene>,
    sources: &[EntityDirectLight; MAX_ENTITY_DIRECT_LIGHTS],
    world: [f32; 3],
    visibility: EntityVisibility<'_>,
    cache: &std::cell::RefCell<Option<&mut EntityStaticRayCache>>,
) -> [[f32; 4]; MAX_ENTITY_DIRECT_LIGHTS] {
    std::array::from_fn(|slot| {
        let Some(index) = source_ids.get(slot).copied().flatten() else {
            return [0.0; 4];
        };
        let Some(source) = sources.get(slot) else {
            return [0.0; 4];
        };
        source.taps.map(|[x, y, z, weight]| {
            if weight > 0.0 {
                scene.map_or_else(
                    || {
                        static_ray(
                            cache,
                            3,
                            i64::try_from(index).unwrap_or(i64::MAX),
                            [x, y, z],
                            world,
                            || {
                                f32::from(u8::from(lighting.runtime_light_visible(
                                    index,
                                    [x, y, z],
                                    world,
                                )))
                            },
                        )
                    },
                    |transport| {
                        static_ray(cache, 2, 0, world, [x, y, z], || {
                            transport.transmittance(world, [x, y, z])
                        })
                    },
                ) * visibility.transmittance(world, [x, y, z])
            } else {
                0.0
            }
        })
    })
}

/// Sample at the transformed model-bounds centre, in metres, Y up.
/// A failed lookup uses the existing linear authored environment model. No brightness floor is applied to valid baked energy.
#[must_use]
pub fn entity_lighting(
    lighting: &LevelLighting,
    irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
    position: [f32; 3],
) -> EntityLighting {
    let finite = position.iter().all(|v| v.is_finite());
    let room = finite
        .then(|| lighting.room_index_at_height(position[0], position[1], position[2]))
        .flatten()
        .filter(|r| lighting.probe_region_at(*r, position).is_some());
    let prepared = room.and_then(|_| {
        irradiance.and_then(|field| {
            let visible = lighting.labelled_probe_visibility(position);
            field.sample_with_switchable_indirect(
                position,
                None,
                |index| probe_light_enabled(lighting, index),
                &visible,
            )
        })
    });
    let source = if !finite {
        EntityLightingSource::InvalidPosition
    } else if irradiance.is_none() {
        EntityLightingSource::NoField
    } else if room.is_none() {
        EntityLightingSource::Roomless
    } else if prepared.is_none() {
        EntityLightingSource::Unresolved
    } else {
        EntityLightingSource::Prepared
    };
    let display = prepared.map_or_else(
        || {
            // Invalid transforms never enter the spatial lookup. The environment
            // at the map origin supplies a deterministic diagnostic fallback.
            let p = if finite { position } else { [0.0; 3] };
            let light = lighting.sample(p[0], p[1], p[2]);
            [light.r, light.g, light.b].map(|v| {
                if v.is_finite() {
                    v.max(0.0)
                } else {
                    crate::engine::lighting::AMBIENT_LEVEL
                }
            })
        },
        |t| t.irradiance,
    );
    let resolved = prepared.or_else(|| {
        finite
            .then(|| lighting.fallback_entity_texel(position, display))
            .flatten()
    });
    EntityLighting {
        prepared: resolved,
        display: resolved.map_or(display, |texel| texel.irradiance),
        source,
    }
}

/// Old and new caster bounds are a conservative visibility dependency.
/// Missed revisions use the complete scene; unchanging receivers elsewhere
/// retain their exact eight-anchor payload without enumerating ray samples.
pub(in crate::engine) fn visibility_affects(
    lighting: &LevelLighting,
    irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
    receivers: Aabb,
    dirty: Aabb,
) -> bool {
    use crate::engine::spatial::{directional_light_region_overlaps, finite_light_region_overlaps};
    if dirty.is_empty() {
        return false;
    }
    if irradiance.is_some_and(|field| {
        // Probe candidates lie within two lattice cells of each anchor.
        // This also encloses anchor-to-anchor fallback connections.
        let reach = field.cell_m.mul_add(2.0, 0.000_1);
        !reach.is_finite()
            || receivers
                .min
                .into_iter()
                .zip(receivers.max)
                .zip(dirty.min)
                .zip(dirty.max)
                .all(|(((low, high), dirty_low), dirty_high)| {
                    dirty_high >= low - reach && dirty_low <= high + reach
                })
    }) {
        return true;
    }
    lighting
        .lights()
        .iter()
        .filter(|light| light.is_active())
        .any(|light| {
            let source = Aabb {
                min: [
                    light.x() - light.half_w(),
                    light.y(),
                    light.z() - light.half_d(),
                ],
                max: [
                    light.x() + light.half_w(),
                    light.y(),
                    light.z() + light.half_d(),
                ],
            };
            finite_light_region_overlaps(source, receivers, dirty)
        })
        || lighting.global_lights().iter().any(|light| {
            light.angular_radius > 0.0
                || directional_light_region_overlaps(light.incoming, receivers, dirty)
        })
}

#[cfg(test)]
#[path = "../tests/static_ray_cache.rs"]
mod static_ray_cache_tests;
