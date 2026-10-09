//! Building the offline transport scene from a prepared level build.
//!
//! The transport solver needs plain world-space triangles with a diffuse
//! albedo, plus the resolved light sources. This module assembles them from the
//! same data the renderer draws: the prepared [`LevelMesh`] ranges (skipping the
//! decal pass, which is a visual overlay and not an occluder), the placed prop
//! batches, and the level's material table.
//!
//! Diffuse albedo per surface is the material's authored tint multiplied by the
//! texture's sampled colour at the triangle's centre, in linear light. A prop vertex already carries its material's `baseColorFactor`
//! in its vertex colour, so a prop triangle's albedo is that colour times its
//! submesh texture. This is the receiving side of the transport equation: the
//! albedo modulates what a bounce *emits*, and it is never baked into the stored
//! light the shader multiplies.
//!
//! Emitter classification is the compiled one: each [`BakedLight`] becomes a
//! transport emitter, and a ceiling fixture the level marks `switchable` is
//! tagged with its light index so the solver can solve its contribution into
//! its own prepared layer set.

use std::collections::BTreeMap;

use crate::level::{LevelDef, WaterVolumes};
use crate::lighting::LevelLighting;
use crate::lighting::lightmap::{Chart, LightmapPatch};
use crate::lighting::transport::{
    TransportAlphaSurface, TransportEmitter, TransportScene, TransportTextureAddress,
    TransportTriangle, TransportWaterBody, receiver_targets,
};
use crate::materials::{AlphaMode, MaterialTable, RawImage};
use crate::render::{LevelMesh, MATERIAL_NONE, PropMeshBatch, SurfaceKey, SurfaceKind, Vertex};

/// World-space tolerance, in metres, for matching a triangle's corners to a
/// water volume's surface plane and footprint.
const WATER_SURFACE_EPS_M: f32 = 1.0e-3;

/// What one transport-scene build produced, for the developer report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransportSceneStats {
    /// Triangles in the scene.
    pub triangles: usize,
    /// Triangles skipped as degenerate or non-finite.
    pub skipped_triangles: usize,
    /// Resolved emitters.
    pub emitters: usize,
    /// Emitters tagged switchable.
    pub switchable_emitters: usize,
}

/// Identity follows the source material/primitive, rather than the texture
/// colour sampled at a triangle centroid. IDs are local to one prepared scene.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum SurfaceMaterial {
    Architecture(SurfaceKey),
    Prop { model: String, primitive: usize },
    UnknownProp { batch: usize, primitive: usize },
}

#[derive(Default)]
struct SurfaceAttributes {
    alpha: Vec<Option<TransportAlphaSurface>>,
    materials: Vec<u32>,
    identities: BTreeMap<SurfaceMaterial, u32>,
    unknown_batches: usize,
}

impl SurfaceAttributes {
    fn material(&mut self, source: SurfaceMaterial) -> u32 {
        let next = u32::try_from(self.identities.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        *self.identities.entry(source).or_insert(next)
    }

    fn push(&mut self, material: u32, alpha: Option<TransportAlphaSurface>) {
        self.materials.push(material);
        self.alpha.push(alpha);
    }

    const fn unknown_batch(&mut self) -> usize {
        let identity = self.unknown_batches;
        self.unknown_batches = self.unknown_batches.saturating_add(1);
        identity
    }
}

/// Builds the transport scene for one prepared build.
///
/// `charts` is the finished lightmap plan's chart list. It is needed here so
/// the authored baseline target of every receiver and probe can be resolved
/// once, from the same zone-grid lookup the vertex bake used, and stored with
/// the scene the solve later runs against.
///
/// Returns `None` when the scene exceeds the transport solver's triangle budget;
/// the caller then fails the variant over to the vertex-lit build by name,
/// exactly like a lightmap plan failure.
#[must_use]
pub fn build_transport_scene(
    level: &LevelDef,
    mesh: &LevelMesh,
    batches: &[PropMeshBatch],
    materials: &MaterialTable,
    lighting: &LevelLighting,
    charts: &[(LightmapPatch, Chart)],
) -> Option<(TransportScene, TransportSceneStats)> {
    let mut stats = TransportSceneStats::default();
    let mut triangles: Vec<TransportTriangle> = Vec::new();
    let mut surfaces = SurfaceAttributes::default();
    let mut owners = Vec::new();
    // Water bodies transmit and attenuate; the drawn surface triangles are
    // matched against the same resolved volumes the player swims in. A dry
    // level must not pay for the floor resolution the water emitter also skips.
    let water = if level.water.is_empty() {
        WaterVolumes::new()
    } else {
        WaterVolumes::from_level(level)
    };
    append_architecture_triangles(
        mesh,
        materials,
        &water,
        &mut triangles,
        &mut surfaces,
        &mut stats,
        &mut owners,
    );
    append_prop_triangles(
        batches,
        &mut triangles,
        &mut surfaces,
        &mut stats,
        &mut owners,
    );

    let mut emitters: Vec<TransportEmitter> = Vec::new();
    let switchable = switchable_lights(level, lighting);
    let selected = super::dynamic_lights::select_baked_direct_lights(lighting, &switchable);
    let mut runtime_direct = Vec::new();
    for (index, light) in lighting.lights().iter().enumerate() {
        let tag = switchable
            .iter()
            .copied()
            .find(|candidate| *candidate == index);
        // A switchable fixture is prepared even when it is authored off, so a
        // runtime toggle can turn its contribution on; a plainly inactive
        // light contributes nothing to any solve.
        if !light.is_active() && tag.is_none() {
            continue;
        }
        if let Ok(source) = u32::try_from(index)
            && selected.contains(&source)
        {
            runtime_direct.push((emitters.len(), source));
        }
        emitters.push(TransportEmitter::from_baked(light, tag));
        if tag.is_some() {
            stats.switchable_emitters = stats.switchable_emitters.saturating_add(1);
        }
    }
    stats.triangles = triangles.len();
    stats.emitters = emitters.len();
    let scene = TransportScene::new(triangles, emitters)?
        .with_runtime_direct_lights(runtime_direct)
        .with_surface_alpha(surfaces.alpha)?
        .with_surface_materials(surfaces.materials)?
        .with_water(
            water
                .volumes()
                .iter()
                .map(TransportWaterBody::from_volume)
                .collect(),
        )
        .with_receiver_target(receiver_targets(lighting, charts))
        .with_validated_probe_targets(level, lighting, charts)
        .with_sky(sky_radiance(level));
    let lit_scene = scene.with_global_lights(
        level
            .global_illuminators
            .iter()
            .filter_map(crate::lighting::directional::DirectionalLight::from_definition)
            .collect(),
    );
    if let Err(error) = crate::lighting::transport::diagnostics::dump_scene(&lit_scene, &owners) {
        crate::logging::warn(format_args!("[lighting-diagnostics] {error}"));
    }
    Some((lit_scene, stats))
}

/// The runtime environment sample. Prepared values stay linear HDR until
/// the material shader reconstructs diffuse lighting at its world normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityLighting {
    pub prepared: Option<crate::lighting::lightmap::LightmapTexel>,
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
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
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

    fn from_baked(light: &crate::lighting::BakedLight) -> Self {
        let emitter = TransportEmitter::from_baked(light, None);
        let [x, y, z] = emitter.position;
        let [red, green, blue] = emitter.color;
        let (half_x, half_z) = light.source.half_extents();
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
                crate::lighting::LOCAL_LIGHT_STRENGTH * emitter.intensity * emitter.height_factor,
            ],
            extent_falloff: [
                half_x,
                half_z,
                match emitter.falloff {
                    crate::lighting::LightFalloff::Smooth => 0.0,
                    crate::lighting::LightFalloff::Linear => 1.0,
                    crate::lighting::LightFalloff::Constant => 2.0,
                },
                if emitter.directional { 1.0 } else { 0.0 },
            ],
            taps,
        }
    }
}

/// Residual field coefficients and important-source visibility at one anchor.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EntityLightingAnchor {
    pub irradiance: [f32; 4],
    pub moment: [f32; 4],
    pub visibility: [[f32; 4]; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS],
    pub attenuation: [f32; 4],
}

impl EntityLightingAnchor {
    pub const ZERO: Self = Self {
        irradiance: [0.0; 4],
        moment: [0.0; 4],
        visibility: [[0.0; 4]; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS],
        attenuation: [1.0, 1.0, 1.0, 0.0],
    };
}

/// A spatial lighting payload independent of pose-vertex uploads. The combined
/// centre sample remains available separately for diagnostics/legacy callers.
#[derive(Debug, PartialEq)]
pub struct EntitySpatialLighting {
    pub bounds_min: [f32; 4],
    pub bounds_extent: [f32; 4],
    pub anchors: [EntityLightingAnchor; ENTITY_LIGHTING_ANCHORS],
    pub direct: [EntityDirectLight; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS],
}

struct EntityAnchorPositions {
    world: [[f32; 3]; ENTITY_LIGHTING_ANCHORS],
    centre: [f32; 3],
    minimum: glam::Vec3,
    extent: glam::Vec3,
}

fn entity_anchor_positions(
    bounds: crate::spatial::Aabb,
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
    receiver: Option<super::dynamic::DynamicId>,
}

/// Optional current movable geometry and the rigid receiver's self identity.
#[derive(Clone, Copy, Default)]
pub struct EntityVisibility<'a> {
    pub dynamic: Option<&'a super::dynamic_visibility::DynamicVisibility>,
    pub receiver: Option<super::dynamic::DynamicId>,
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
    field: &crate::lighting::probes::ProbeField,
    scene: Option<&TransportScene>,
) -> [Option<usize>; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS] {
    let mut ids = [None; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS];
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
            .take(crate::package::MAX_SWITCHABLE_LIGHTS)
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

#[cfg(test)]
#[must_use]
pub fn entity_spatial_key(
    lighting: &LevelLighting,
    field: Option<&crate::lighting::probes::ProbeField>,
    model: glam::Mat4,
    scene: Option<&TransportScene>,
) -> Option<EntitySpatialKey> {
    entity_spatial_key_with_visibility(lighting, field, model, scene, EntityVisibility::default())
}

#[must_use]
pub fn entity_spatial_key_with_visibility(
    lighting: &LevelLighting,
    field: Option<&crate::lighting::probes::ProbeField>,
    model: glam::Mat4,
    scene: Option<&TransportScene>,
    visibility: EntityVisibility<'_>,
) -> Option<EntitySpatialKey> {
    let prepared = field?;
    let _selected_direct = prepared.local_direct.as_ref()?;
    let mut enabled = 0_u64;
    for (slot, index) in runtime_entity_light_ids(prepared, scene).iter().enumerate() {
        let active = index
            .and_then(|id| lighting.lights().get(id))
            .is_some_and(crate::lighting::BakedLight::is_active);
        if active {
            enabled |= 1_u64.checked_shl(u32::try_from(slot).ok()?).unwrap_or(0);
        }
    }
    Some(EntitySpatialKey {
        field: std::ptr::from_ref(prepared).addr(),
        scene: scene.map_or(0, |value| std::ptr::from_ref(value).addr()),
        model: model
            .to_cols_array_2d()
            .map(|column| column.map(f32::to_bits)),
        enabled,
        dynamic: visibility
            .dynamic
            .map_or(0, |value| std::ptr::from_ref(value).addr()),
        dynamic_revision: visibility
            .dynamic
            .map_or(0, super::dynamic_visibility::DynamicVisibility::revision),
        receiver: visibility.receiver,
    })
}

/// Resolve spatial coefficients at transformed interior support points.
/// Missing support uses a visibly connected centre field; the fallback does
/// not inject an ambient floor into valid dark samples. Without a prepared
/// field the legacy authored environment remains the existing per-object path.
#[cfg(test)]
#[must_use]
pub fn entity_spatial_lighting(
    lighting: &LevelLighting,
    field: Option<&crate::lighting::probes::ProbeField>,
    bounds: crate::spatial::Aabb,
    model: glam::Mat4,
    scene: Option<&TransportScene>,
) -> Option<Box<EntitySpatialLighting>> {
    entity_spatial_lighting_with_visibility(
        lighting,
        field,
        bounds,
        model,
        scene,
        EntityVisibility::default(),
    )
}

#[must_use]
pub fn entity_spatial_lighting_with_visibility(
    lighting: &LevelLighting,
    field: Option<&crate::lighting::probes::ProbeField>,
    bounds: crate::spatial::Aabb,
    model: glam::Mat4,
    scene: Option<&TransportScene>,
    visibility: EntityVisibility<'_>,
) -> Option<Box<EntitySpatialLighting>> {
    let prepared = field?;
    let _selected_direct = prepared.local_direct.as_ref()?;
    let positions = entity_anchor_positions(bounds, model)?;
    let sample = |point| {
        prepared.sample_nonlocal_filtered_with_rooms(point, None, |probe, label| {
            lighting.labelled_probe_visible_from(point, probe, label)
                && visibility.transmittance(point, probe) > 0.0
        })
    };
    let samples = positions.world.map(|world| (world, sample(world)));
    let centre_sample = sample(positions.centre);
    if centre_sample.is_none() && samples.iter().all(|(_, value)| value.is_none()) {
        let has_static_support = |point| {
            prepared
                .sample_nonlocal_filtered_with_rooms(point, None, |probe, label| {
                    lighting.labelled_probe_visible_from(point, probe, label)
                })
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
        scene.map_or_else(
            || lighting.runtime_probe_segment_visible(from, to),
            |transport| transport.transmittance(from, to) > 0.0,
        ) && visibility.transmittance(from, to) > 0.0
    };
    let mut direct = [EntityDirectLight::ZERO; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS];
    let source_ids = runtime_entity_light_ids(prepared, scene);
    for (slot, index) in direct.iter_mut().zip(source_ids) {
        let Some(light) = index.and_then(|id| lighting.lights().get(id)) else {
            continue;
        };
        if light.is_active() {
            *slot = EntityDirectLight::from_baked(light);
        }
    }
    let anchors = samples.map(|(world, exact_sample)| {
        let texel = exact_sample
            .or_else(|| centre_sample.filter(|_| connected(world, positions.centre)))
            .or_else(|| {
                samples
                    .iter()
                    .find_map(|(position, value)| value.filter(|_| connected(world, *position)))
            })
            .unwrap_or(crate::lighting::lightmap::LightmapTexel {
                irradiance: [0.0; 3],
                direction: [0.0; 3],
                axis: [0.5; 2],
            });
        let [red, green, blue] = texel.irradiance;
        let [x, y, z] = texel.direction;
        let throughput =
            spatial_anchor_visibility(lighting, &source_ids, scene, &direct, world, visibility);
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
    let [x, y, z] = positions.minimum.to_array();
    let [dx, dy, dz] = positions.extent.to_array();
    Some(Box::new(EntitySpatialLighting {
        bounds_min: [x, y, z, 1.0],
        bounds_extent: [dx, dy, dz, 0.0],
        anchors,
        direct,
    }))
}

fn spatial_anchor_visibility(
    lighting: &LevelLighting,
    source_ids: &[Option<usize>; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS],
    scene: Option<&TransportScene>,
    sources: &[EntityDirectLight; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS],
    world: [f32; 3],
    visibility: EntityVisibility<'_>,
) -> [[f32; 4]; super::dynamic_lights::MAX_ENTITY_DIRECT_LIGHTS] {
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
                        f32::from(u8::from(lighting.runtime_light_visible(
                            index,
                            [x, y, z],
                            world,
                        )))
                    },
                    |transport| transport.transmittance(world, [x, y, z]),
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
    irradiance: Option<&crate::lighting::probes::ProbeField>,
    position: [f32; 3],
) -> EntityLighting {
    let finite = position.iter().all(|v| v.is_finite());
    let room = finite
        .then(|| lighting.room_index_at_height(position[0], position[1], position[2]))
        .flatten()
        .filter(|r| lighting.probe_region_at(*r, position).is_some());
    let prepared = room.and_then(|_| {
        irradiance.and_then(|field| {
            field.sample_filtered_with_rooms(position, None, |probe, label| {
                lighting.labelled_probe_visible_from(position, probe, label)
            })
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
                    crate::lighting::AMBIENT_LEVEL
                }
            })
        },
        |t| t.irradiance,
    );
    EntityLighting {
        prepared,
        display,
        source,
    }
}

/// Isotropic linear light used by compatibility tests.
#[cfg(test)]
#[must_use]
pub fn moving_object_light(
    lighting: &LevelLighting,
    irradiance: Option<&crate::lighting::probes::ProbeField>,
    position: [f32; 3],
) -> [f32; 3] {
    entity_lighting(lighting, irradiance, position).display
}

/// The radiance an escaping bounce ray sees, from the level's optional sky.
///
/// Zero without a sky (or with `ambient` 0.0), which preserves the historical
/// "an interior has no sky" solve exactly. The authored scalar is scaled by
/// [`crate::lighting::SKY_AMBIENT_COLOR`].
fn sky_radiance(level: &LevelDef) -> [f32; 3] {
    let Some(sky) = level.sky.as_ref() else {
        return [0.0; 3];
    };
    let ambient = if sky.ambient.is_finite() {
        sky.ambient.clamp(0.0, crate::level::MAX_SKY_AMBIENT)
    } else {
        0.0
    };
    sky.ambient_color
        .unwrap_or(crate::lighting::SKY_AMBIENT_COLOR)
        .map(|channel| channel * ambient)
}

/// Appends every architecture range's triangles to the transport scene.
///
/// Decals and fixture geometry are skipped; walls, floors, ceilings,
/// architecture and prop fallbacks occlude. Water transmits into its attenuating
/// body; other surfaces retain the renderer's alpha coverage at each ray hit.
fn append_architecture_triangles(
    mesh: &LevelMesh,
    materials: &MaterialTable,
    water: &WaterVolumes,
    triangles: &mut Vec<TransportTriangle>,
    surfaces: &mut SurfaceAttributes,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::lighting::transport::diagnostics::CasterRange>,
) {
    for range in &mesh.ranges {
        // Decals are a visual overlay, and fixture geometry is the analytic
        // emitters' own housing: keeping it would let a fixture shadow itself
        // (its authored luminous face sits between the room and the emitter
        // point), which the historical occluder classification never did.
        // Walls, floors, ceilings, architecture and prop fallbacks occlude.
        if matches!(range.key.kind, SurfaceKind::Decal | SurfaceKind::Light) {
            continue;
        }
        let first = triangles.len();
        let material = material_albedo(materials, range.key.material);
        let material_id = material_id(materials, range.key.material);
        let identity = surfaces.material(SurfaceMaterial::Architecture(range.key));
        for chunk in range.indices.as_chunks::<3>().0 {
            let (Some(a), Some(b), Some(c)) = (chunk.first(), chunk.get(1), chunk.get(2)) else {
                continue;
            };
            let (Some(va), Some(vb), Some(vc)) = (
                range.vertices.get(usize::from(*a)),
                range.vertices.get(usize::from(*b)),
                range.vertices.get(usize::from(*c)),
            ) else {
                continue;
            };
            let albedo = material;
            match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                Some(triangle) => {
                    let corners = [va.pos, vb.pos, vc.pos];
                    let water_surface = is_water_surface(corners, material_id, water);
                    triangles.push(triangle.with_transmissive(water_surface));
                    surfaces.push(
                        identity,
                        if water_surface {
                            None
                        } else {
                            materials.entry(range.key.material).and_then(|entry| {
                                alpha_surface(
                                    entry.alpha,
                                    entry.image.clone(),
                                    [va, vb, vc],
                                    TransportTextureAddress::Repeat,
                                )
                            })
                        },
                    );
                }
                None => stats.skipped_triangles = stats.skipped_triangles.saturating_add(1),
            }
        }
        if crate::lighting::transport::diagnostics::enabled() {
            owners.push(crate::lighting::transport::diagnostics::CasterRange {
                first,
                end: triangles.len(),
                owner: format!(
                    "architecture:{:?}:{}",
                    range.key.kind,
                    material_id.unwrap_or("untextured")
                ),
            });
        }
    }
}

/// Prop sheets clamp their UVs; alpha coverage and material factors match the
/// draw path without turning the whole card into either a blocker or a hole.
fn append_prop_triangles(
    batches: &[PropMeshBatch],
    triangles: &mut Vec<TransportTriangle>,
    surfaces: &mut SurfaceAttributes,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::lighting::transport::diagnostics::CasterRange>,
) {
    for batch in batches {
        if !batch.casts_static_lighting {
            continue;
        }
        let first = triangles.len();
        let unknown_batch = surfaces.unknown_batch();
        for (primitive, submesh) in batch.submeshes.iter().enumerate() {
            // Finished batches omit empty primitives. Only the retained
            // original source slot can be shared safely across those batches.
            // Decoded packages have no offline sidecar: keep their materials
            // scoped to this batch rather than guessing from draw ordinal.
            let source_primitive = if batch.source_primitives.len() == batch.submeshes.len() {
                batch.source_primitives.get(primitive).copied()
            } else {
                None
            };
            let material = source_primitive.map_or(
                SurfaceMaterial::UnknownProp {
                    batch: unknown_batch,
                    primitive,
                },
                |source_slot| SurfaceMaterial::Prop {
                    model: batch.model.clone(),
                    primitive: source_slot,
                },
            );
            let identity = surfaces.material(material);
            let start = usize::try_from(submesh.first_index).unwrap_or(usize::MAX);
            let count = usize::try_from(submesh.index_count).unwrap_or(usize::MAX);
            let end = start.saturating_add(count);
            let image = submesh
                .texture
                .and_then(|index| batch.textures.get(usize::from(index)))
                .map(AsRef::as_ref);
            let mut index = start;
            while index < end {
                let Some(chunk) = batch.indices.get(index..index.saturating_add(3)) else {
                    break;
                };
                index = index.saturating_add(3);
                let (Some(a), Some(b), Some(c)) = (chunk.first(), chunk.get(1), chunk.get(2))
                else {
                    continue;
                };
                let (Some(va), Some(vb), Some(vc)) = (
                    batch.vertices.get(usize::from(*a)),
                    batch.vertices.get(usize::from(*b)),
                    batch.vertices.get(usize::from(*c)),
                ) else {
                    continue;
                };
                let albedo = triangle_albedo([1.0; 3], va, vb, vc, image);
                match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                    Some(triangle) => {
                        triangles
                            .push(triangle.with_shading_normals([va.normal, vb.normal, vc.normal]));
                        surfaces.push(
                            identity,
                            alpha_surface(
                                submesh.alpha,
                                submesh
                                    .texture
                                    .and_then(|texture| batch.textures.get(usize::from(texture)))
                                    .cloned(),
                                [va, vb, vc],
                                TransportTextureAddress::Clamp,
                            ),
                        );
                    }
                    None => stats.skipped_triangles = stats.skipped_triangles.saturating_add(1),
                }
            }
        }
        if crate::lighting::transport::diagnostics::enabled() {
            owners.push(crate::lighting::transport::diagnostics::CasterRange {
                first,
                end: triangles.len(),
                owner: format!("model:{}", batch.model),
            });
        }
    }
}

/// The same triangle/material/coverage preparation, in a movable mesh's local
/// space. The resulting ray resource has no light sources or bake work.
#[must_use]
pub(super) fn dynamic_mesh_visibility_scene(
    mesh: &super::dynamic::DynamicMesh,
) -> Option<TransportScene> {
    let batch = PropMeshBatch {
        model: mesh.model_path.clone(),
        casts_static_lighting: true,
        // DynamicMesh filters empty source primitives too. Its isolated ray
        // scene can preserve material continuity within each draw submesh
        // without pretending the compressed ordinal is an original source ID.
        source_primitives: Vec::new(),
        textures: mesh.textures.clone(),
        submeshes: mesh
            .submeshes
            .iter()
            .map(|submesh| crate::render::PropSubmeshBatch {
                response: submesh.response,
                texture: submesh.texture,
                emission: submesh.emission,
                alpha: submesh.alpha,
                first_index: submesh.first_index,
                index_count: submesh.index_count,
            })
            .collect(),
        vertices: mesh.vertices.clone(),
        indices: mesh.indices.clone(),
        bounds: mesh.bounds,
    };
    let mut triangles = Vec::new();
    let mut surfaces = SurfaceAttributes::default();
    append_prop_triangles(
        &[batch],
        &mut triangles,
        &mut surfaces,
        &mut TransportSceneStats::default(),
        &mut Vec::new(),
    );
    TransportScene::new(triangles, Vec::new())?
        .with_surface_alpha(surfaces.alpha)?
        .with_surface_materials(surfaces.materials)
}

/// The level material id one surface key resolves to, or `None` when the key
/// binds no level material (a fixture housing or a prop fallback).
fn material_id(materials: &MaterialTable, material: u32) -> Option<&str> {
    if material == MATERIAL_NONE {
        return None;
    }
    materials.entry(material).map(|entry| entry.id.as_str())
}

fn alpha_surface(
    alpha: crate::materials::MaterialAlpha,
    image: Option<std::sync::Arc<RawImage>>,
    vertices: [&Vertex; 3],
    address: TransportTextureAddress,
) -> Option<TransportAlphaSurface> {
    (alpha.mode != AlphaMode::Opaque).then(|| TransportAlphaSurface {
        alpha,
        image,
        uv: vertices.map(|vertex| vertex.uv),
        vertex_alpha: vertices.map(|vertex| vertex.color[3]),
        address,
    })
}

/// True when a triangle is one water volume's drawn surface: all three corners
/// lie on one resolved footprint at its `surface_y` (so the triangle is
/// horizontal) and the range resolves to that volume's material id.
///
/// The surface is drawn only for [`WaterVolumes`] entries, so matching the
/// authored geometry back to the same resolution is what keeps the transmitted
/// surface and the attenuating body one thing.
fn is_water_surface(
    corners: [[f32; 3]; 3],
    material_id: Option<&str>,
    water: &WaterVolumes,
) -> bool {
    let Some(surface_material) = material_id else {
        return false;
    };
    water.volumes().iter().any(|volume| {
        volume.material_id() == surface_material
            && corners.iter().all(|corner| {
                (corner[1] - volume.surface_y).abs() <= WATER_SURFACE_EPS_M
                    && corner[0] >= volume.x0 - WATER_SURFACE_EPS_M
                    && corner[0] <= volume.x1 + WATER_SURFACE_EPS_M
                    && corner[2] >= volume.z0 - WATER_SURFACE_EPS_M
                    && corner[2] <= volume.z1 + WATER_SURFACE_EPS_M
            })
    })
}

/// The light indices of every switchable ceiling fixture, in fixture order.
///
/// The compiled lighting record already knows the fixture-to-light mapping, so
/// this is the same classification the runtime switch uses: a fixture whose
/// `switchable` flag is set owns one light, and that light's contribution must
/// be prepared as its own selectable layer.
#[must_use]
pub fn switchable_lights(level: &crate::level::LevelDef, lighting: &LevelLighting) -> Vec<usize> {
    let mut out = Vec::new();
    for (fixture_index, fixture) in level.ceiling_lights.iter().enumerate() {
        if !fixture.switchable {
            continue;
        }
        if let Some(light) = lighting.fixture_light_index(fixture_index)
            && !out.contains(&light)
        {
            out.push(light);
        }
    }
    out
}

/// The albedo of one triangle: the material tint (or the prop vertex's own
/// base colour) multiplied by the submesh texture's colour at the triangle
/// centre.
fn triangle_albedo(
    material_albedo: [f32; 3],
    a: &Vertex,
    b: &Vertex,
    c: &Vertex,
    texture: Option<&RawImage>,
) -> [f32; 3] {
    let mut base = [
        material_albedo[0] * ((a.color[0] + b.color[0] + c.color[0]) / 3.0),
        material_albedo[1] * ((a.color[1] + b.color[1] + c.color[1]) / 3.0),
        material_albedo[2] * ((a.color[2] + b.color[2] + c.color[2]) / 3.0),
    ];
    if let Some(image) = texture {
        let centroid = [
            (a.uv[0] + b.uv[0] + c.uv[0]) / 3.0,
            (a.uv[1] + b.uv[1] + c.uv[1]) / 3.0,
        ];
        let sample = sample_texture(image, centroid);
        base = [
            base[0] * sample[0],
            base[1] * sample[1],
            base[2] * sample[2],
        ];
    }
    [
        base[0].clamp(0.0, 1.0),
        base[1].clamp(0.0, 1.0),
        base[2].clamp(0.0, 1.0),
    ]
}

/// The averaged albedo of one level material, or a neutral white for a slot
/// with no level material.
fn material_albedo(materials: &MaterialTable, material: u32) -> [f32; 3] {
    if material == MATERIAL_NONE {
        return [0.8; 3];
    }
    let Some(entry) = materials.entry(material) else {
        return [0.8; 3];
    };
    let mut tint = entry.tint;
    if let Some(image) = entry.image.as_deref() {
        let mean = mean_texture_color(image);
        tint = [tint[0] * mean[0], tint[1] * mean[1], tint[2] * mean[2]];
    }
    [
        tint[0].clamp(0.0, 1.0),
        tint[1].clamp(0.0, 1.0),
        tint[2].clamp(0.0, 1.0),
    ]
}

/// Mean colour of one texture, sampled on a fixed stride so a large image is
/// bounded work.
#[must_use]
pub fn mean_texture_color(image: &RawImage) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let texels = (usize::try_from(image.width).unwrap_or(0))
        .saturating_mul(usize::try_from(image.height).unwrap_or(0));
    if texels == 0 {
        return [1.0; 3];
    }
    let stride = (texels / 1024).max(1);
    let mut sum = [0.0_f32; 3];
    let mut count = 0_usize;
    let mut index = 0usize;
    while index < texels {
        let offset = index.saturating_mul(4);
        if let Some([r, g, b, a]) = image
            .rgba
            .get(offset..offset.saturating_add(4))
            .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
            && a > 0
        {
            sum[0] += crate::materials::color::decode_byte(r);
            sum[1] += crate::materials::color::decode_byte(g);
            sum[2] += crate::materials::color::decode_byte(b);
            count = count.saturating_add(1);
        }
        index = index.saturating_add(stride);
    }
    if count == 0 {
        return [1.0; 3];
    }
    // The stride caps the sample count near 1024, so the divisor is bounded.
    let divisor = f32::from(u16::try_from(count).unwrap_or(u16::MAX)).max(1.0);
    [sum[0] / divisor, sum[1] / divisor, sum[2] / divisor]
}

/// Nearest-texel sample of a GLB sheet, clamped like the runtime sampler.
#[must_use]
pub fn sample_texture(image: &RawImage, uv: [f32; 2]) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let width = usize::try_from(image.width).unwrap_or(1).max(1);
    let height = usize::try_from(image.height).unwrap_or(1).max(1);
    let u_coordinate = if uv[0].is_finite() {
        uv[0].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v_coordinate = if uv[1].is_finite() {
        uv[1].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let width_f =
        f32::from(u16::try_from(width.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    let height_f =
        f32::from(u16::try_from(height.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Finite wrapped UVs become floored texel indices bounded by u16 texture dimensions and clamped to the last texel."
    )]
    // Both coordinates are finite and in `0..width` after the modulo and the
    // multiply, so the truncating cast is exact for the range it receives.
    let x_texel = ((u_coordinate * width_f).floor() as u32)
        .min(u32::try_from(width.saturating_sub(1)).unwrap_or(0));
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Finite wrapped UVs become floored texel indices bounded by u16 texture dimensions and clamped to the last texel."
    )]
    let y_texel = ((v_coordinate * height_f).floor() as u32)
        .min(u32::try_from(height.saturating_sub(1)).unwrap_or(0));
    let offset = usize::try_from(y_texel)
        .unwrap_or(0)
        .saturating_mul(width)
        .saturating_add(usize::try_from(x_texel).unwrap_or(0))
        .saturating_mul(4);
    match image
        .rgba
        .get(offset..offset.saturating_add(4))
        .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
    {
        Some([r, g, b, _]) => [
            crate::materials::color::decode_byte(r),
            crate::materials::color::decode_byte(g),
            crate::materials::color::decode_byte(b),
        ],
        _ => [1.0; 3],
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect and permissive float comparison are idiomatic
    // here; the production lints stay enforced above.
    #![allow(
        clippy::expect_used,
        clippy::float_cmp,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    #[test]
    fn authored_sky_colour_enters_incident_energy_once_and_ignores_presentation() {
        let mut level = LevelDef::from_json(include_str!(
            "../../../tests/fixtures/levels/art_style_hero.json"
        ))
        .expect("hero");
        if let Some(sky) = level.sky.as_mut() {
            sky.ambient_color = Some([0.2, 0.4, 0.8]);
            sky.ambient = 0.25;
        }
        assert_eq!(sky_radiance(&level), [0.05, 0.1, 0.2]);
        level.environment = Some(crate::environment::EnvironmentDef {
            presentation: crate::environment::PresentationDef {
                exposure: 4.0,
                ..Default::default()
            },
            ..Default::default()
        });
        if let Some(sky) = level.sky.as_mut() {
            sky.brightness = 4.0;
        }
        assert_eq!(
            sky_radiance(&level),
            [0.05, 0.1, 0.2],
            "exposure/background never scale stored incident energy"
        );
        if let Some(sky) = level.sky.as_mut() {
            sky.ambient = 0.0;
        }
        assert_eq!(sky_radiance(&level), [0.0; 3]);
    }

    fn spatial_fixture() -> (LevelLighting, crate::lighting::probes::ProbeField) {
        let level = LevelDef::from_json(
            r#"{
            "format_version":3,"id":"spatial_entity","name":"Spatial entity",
            "spawn":{"x":0,"z":0},
            "rooms":[{"x":-4,"z":-4,"width":8,"depth":8,"height":3}],
            "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","x":0,"z":0,"align":"none"}]
        }"#,
        )
        .expect("spatial fixture");
        let lighting = LevelLighting::bake(&level);
        let field = crate::lighting::probes::ProbeField {
            min: [-2.0, 0.0, -2.0],
            cell_m: 4.0,
            dims: [1; 3],
            probes: vec![crate::lighting::probes::ProbeSample {
                irradiance: [0.7; 3],
                direction: [0.0, 0.3, 0.0],
                axis: [0.5; 2],
                room: 0,
            }],
            local_direct: Some(crate::lighting::probes::ProbeDirectField {
                light_indices: vec![0],
                probes: vec![crate::lighting::lightmap::LightmapTexel {
                    irradiance: [0.2; 3],
                    direction: [0.0, 0.15, 0.0],
                    axis: [0.5; 2],
                }],
            }),
        };
        (lighting, field)
    }

    fn framed_receiver() -> (
        super::super::dynamic::DynamicScene,
        super::super::dynamic::DynamicId,
        LevelLighting,
        crate::lighting::probes::ProbeField,
    ) {
        let level = LevelDef::from_json(
            r#"{"format_version":3,"id":"framed_receiver","name":"Framed receiver",
            "spawn":{"x":0,"z":1},
            "rooms":[{"x":-3,"z":-3,"width":6,"depth":6,"height":3}],
            "doors":[{"id":"assembly","x":0,"z":0,"width":1.48,"height":2.3}],
            "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","x":0.74,"z":1.5,"align":"none"}]}"#,
        )
        .expect("assembly fixture");
        let catalog = crate::assets::AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("asset root");
        let materials = crate::materials::resolve_materials(
            &level,
            &catalog,
            None,
            Some(&root),
            &mut crate::materials::TextureCache::new(),
        );
        let models = super::super::doors::build_door_models(
            level.doors.first().expect("assembly"),
            &materials,
            crate::level::DoorFrame {
                depth: 0.3,
                center: 0.0,
            },
        )
        .expect("actual frame and receiver geometry");
        let mut dynamic = super::super::dynamic::DynamicScene::new();
        let frame_mesh = dynamic
            .register_model(
                "frame",
                &models.frame,
                models.textures.clone(),
                &models.frame_alphas,
            )
            .expect("frame mesh");
        let leaf_mesh = dynamic
            .register_model(
                "receiver",
                &models.leaf,
                models.textures,
                &models.leaf_alphas,
            )
            .expect("receiver mesh");
        let _frame = dynamic
            .spawn_registered(
                frame_mesh,
                [0.0; 3],
                super::super::dynamic::SpawnOrientation::YAW_ONLY,
                0.0,
                1.0,
            )
            .expect("frame");
        let leaf = dynamic
            .spawn_registered(
                leaf_mesh,
                [0.0; 3],
                super::super::dynamic::SpawnOrientation::YAW_ONLY,
                0.0,
                1.0,
            )
            .expect("receiver");
        (
            dynamic,
            leaf,
            LevelLighting::bake(&level),
            framed_receiver_field(),
        )
    }

    fn framed_receiver_field() -> crate::lighting::probes::ProbeField {
        crate::lighting::probes::ProbeField {
            min: [-0.01, 0.85, -1.5],
            cell_m: 1.5,
            dims: [1, 1, 2],
            probes: vec![
                crate::lighting::probes::ProbeSample {
                    irradiance: [0.6; 3],
                    direction: [0.0; 3],
                    axis: [0.5; 2],
                    room: 0,
                };
                2
            ],
            local_direct: Some(crate::lighting::probes::ProbeDirectField {
                light_indices: vec![0],
                probes: vec![
                    crate::lighting::lightmap::LightmapTexel {
                        irradiance: [0.1; 3],
                        direction: [0.0; 3],
                        axis: [0.5; 2],
                    };
                    2
                ],
            }),
        }
    }

    #[test]
    fn enclosed_bounds_edges_do_not_blacken_a_supported_receiver() {
        let (dynamic, leaf, lighting, field) = framed_receiver();
        let mut visibility = super::super::dynamic_visibility::DynamicVisibility::new();
        assert!(visibility.sync(&dynamic).expect("current geometry"));
        let receiver = dynamic.get(leaf).expect("receiver");
        let bounds = receiver.mesh().bounds;
        let source = lighting
            .lights()
            .first()
            .expect("practical source")
            .source
            .position;
        let extrema = [bounds.min[0], bounds.max[0]].into_iter().flat_map(|x| {
            [bounds.min[1], bounds.max[1]]
                .into_iter()
                .flat_map(move |y| [bounds.min[2], bounds.max[2]].map(|z| [x, y, z]))
        });
        assert!(
            extrema
                .into_iter()
                .all(|corner| visibility.transmittance(corner, source, Some(leaf)) == 0.0),
            "outer corners start inside the physically surrounding frame"
        );
        let spatial = entity_spatial_lighting_with_visibility(
            &lighting,
            Some(&field),
            bounds,
            receiver.transform(),
            None,
            EntityVisibility {
                dynamic: Some(&visibility),
                receiver: Some(leaf),
            },
        )
        .expect("interior support lattice");
        assert!(
            spatial.anchors.iter().all(|anchor| anchor
                .irradiance
                .iter()
                .take(3)
                .all(|energy| (*energy - 0.5).abs() < 1.0e-6)),
            "valid residual energy survives without an ambient addition"
        );
        assert!(
            spatial.anchors.iter().any(|anchor| anchor
                .visibility
                .first()
                .expect("selected source")
                .iter()
                .all(|tap| *tap == 1.0)),
            "the actual source has clear interior support"
        );
        assert!(
            spatial.anchors.iter().any(|anchor| anchor
                .visibility
                .first()
                .expect("selected source")
                .iter()
                .all(|tap| *tap == 0.0)),
            "the real header still shades upper source taps"
        );
        assert_eq!(
            visibility.transmittance([-0.1, 1.0, 0.0], [0.1, 1.0, 0.0], Some(leaf)),
            0.0,
            "neighboring frame geometry still blocks real crossing rays"
        );
    }

    #[test]
    fn interior_anchor_lattice_handles_thin_axes_and_nonuniform_transforms() {
        let bounds = crate::spatial::Aabb {
            min: [0.0; 3],
            max: [1.48, 2.3, 1.0e-8],
        };
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(0.75, 3.0, 2.0),
            glam::Quat::from_rotation_y(0.7),
            glam::Vec3::new(8.0, 0.0, 4.0),
        );
        let lattice = entity_anchor_positions(bounds, model).expect("finite thin lattice");
        assert!(lattice.minimum.x > 0.0 && lattice.minimum.y > 0.0);
        assert!(lattice.minimum.z > 0.0 && lattice.minimum.z < 0.5e-8);
        assert!(lattice.extent.is_finite() && lattice.extent.min_element() > 0.0);
        assert!(
            lattice
                .world
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        let first = model.transform_point3(lattice.minimum).to_array();
        assert_eq!(
            lattice.world.first(),
            Some(&first),
            "GPU interpolation min is the same sampled point"
        );
        assert!(
            entity_anchor_positions(bounds, glam::Mat4::from_scale(glam::Vec3::splat(f32::MAX)))
                .is_none(),
            "overflowing transformed support never reaches GPU payloads"
        );
    }

    #[test]
    fn spatial_samples_remove_only_selected_coefficients_and_follow_transforms() {
        let (mut lighting, field) = spatial_fixture();
        let scene = TransportScene::new(Vec::new(), Vec::new()).expect("empty visibility scene");
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(2.0, 0.75, 0.5),
            glam::Quat::from_rotation_y(0.7),
            glam::Vec3::new(0.1, 0.0, 0.1),
        );
        let spatial = entity_spatial_lighting(
            &lighting,
            Some(&field),
            crate::spatial::Aabb {
                min: [-0.5, 0.1, -0.5],
                max: [0.5, 1.1, 0.5],
            },
            model,
            Some(&scene),
        )
        .expect("transformed bounds resolve");
        for anchor in &spatial.anchors {
            for channel in anchor.irradiance.into_iter().take(3) {
                assert!(
                    (channel - 0.5).abs() < 1.0e-6,
                    "only selected direct is removed"
                );
            }
            assert_eq!(anchor.moment, [0.0, 0.15, 0.0, 0.0]);
            assert_eq!(
                anchor.visibility.first().expect("visibility row").first(),
                Some(&1.0)
            );
        }
        let source = spatial.direct.first().expect("direct source");
        assert!(
            source
                .color_strength
                .last()
                .is_some_and(|strength| *strength > 0.0),
            "compiler calibration remains active"
        );
        assert_eq!(
            source
                .taps
                .iter()
                .map(|tap| tap.last().copied().unwrap_or(0.0))
                .sum::<f32>(),
            1.0
        );
        let key = entity_spatial_key(&lighting, Some(&field), model, Some(&scene));
        assert_eq!(
            key,
            entity_spatial_key(&lighting, Some(&field), model, Some(&scene))
        );
        assert!(lighting.set_light_enabled(0, false), "source toggles");
        assert_ne!(
            key,
            entity_spatial_key(&lighting, Some(&field), model, Some(&scene))
        );
    }

    #[test]
    fn entity_visibility_keeps_blend_throughput_and_water_depth() {
        let (lighting, field) = spatial_fixture();
        let triangles = vec![
            TransportTriangle::new(
                [-4.0, 1.5, -4.0],
                [4.0, 1.5, -4.0],
                [4.0, 1.5, 4.0],
                [0.5; 3],
            )
            .expect("pane first"),
            TransportTriangle::new(
                [-4.0, 1.5, -4.0],
                [4.0, 1.5, 4.0],
                [-4.0, 1.5, 4.0],
                [0.5; 3],
            )
            .expect("pane second"),
        ];
        let alpha = TransportAlphaSurface {
            alpha: crate::materials::MaterialAlpha {
                mode: AlphaMode::Blend,
                opacity: 0.5,
                ..crate::materials::MaterialAlpha::OPAQUE
            },
            image: None,
            uv: [[0.0; 2]; 3],
            vertex_alpha: [1.0; 3],
            address: TransportTextureAddress::Clamp,
        };
        let scene = TransportScene::new(triangles, Vec::new())
            .expect("pane scene")
            .with_surface_alpha(vec![Some(alpha.clone()), Some(alpha)])
            .expect("alpha aligned")
            .with_water(vec![TransportWaterBody {
                x0: -4.0,
                x1: 4.0,
                z0: -4.0,
                z1: 4.0,
                surface_y: 1.0,
                bottom_y: 0.0,
                extinction: [1.0, 0.5, 0.25],
            }]);
        assert!(
            (scene.transmittance([0.0, 0.5, 0.0], [0.0, 2.5, 0.0]) - 0.5).abs() < 1.0e-6,
            "pane fixture retains authored blend coverage"
        );
        let spatial = entity_spatial_lighting(
            &lighting,
            Some(&field),
            crate::spatial::Aabb {
                min: [-0.2, 0.2, -0.2],
                max: [0.2, 0.8, 0.2],
            },
            glam::Mat4::IDENTITY,
            Some(&scene),
        )
        .expect("under pane");
        for anchor in &spatial.anchors {
            let visibility = anchor
                .visibility
                .first()
                .expect("row")
                .first()
                .copied()
                .expect("source");
            assert!(
                (visibility - 0.5).abs() < 1.0e-6,
                "straight blend crossing remains partial"
            );
            let [red, green, blue, _reserved] = anchor.attenuation;
            assert!(
                red < green && green < blue && blue < 1.0,
                "receiver-depth colour extinction remains independent of alpha"
            );
        }
    }

    #[test]
    fn valid_dark_spatial_samples_stay_dark_without_a_brightness_floor() {
        let (lighting, mut field) = spatial_fixture();
        field.local_direct = Some(crate::lighting::probes::ProbeDirectField {
            light_indices: Vec::new(),
            probes: vec![crate::lighting::lightmap::LightmapTexel::ZERO],
        });
        for probe in &mut field.probes {
            probe.irradiance = [0.0; 3];
            probe.direction = [0.0; 3];
        }
        let centre = entity_lighting(&lighting, Some(&field), [0.0, 1.0, 0.0]);
        assert_eq!(centre.source, EntityLightingSource::Prepared);
        assert_eq!(centre.display, [0.0; 3]);
        let spatial = entity_spatial_lighting(
            &lighting,
            Some(&field),
            crate::spatial::Aabb {
                min: [-0.2, 0.8, -0.2],
                max: [0.2, 1.2, 0.2],
            },
            glam::Mat4::IDENTITY,
            None,
        )
        .expect("dark valid field");
        for anchor in &spatial.anchors {
            assert_eq!(anchor.irradiance, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(anchor.moment, [0.0; 4]);
        }
        assert!(
            spatial
                .direct
                .iter()
                .all(|source| *source == EntityDirectLight::ZERO),
            "an empty selected set adds no direct"
        );
    }

    #[test]
    fn rotated_nonuniform_bounds_sample_the_world_field_in_the_right_axes() {
        let (lighting, mut field) = spatial_fixture();
        field.min = [-2.0, 0.0, -2.0];
        field.cell_m = 2.0;
        field.dims = [2, 1, 2];
        field.probes = [0.1, 0.9, 0.1, 0.9]
            .map(|energy| crate::lighting::probes::ProbeSample {
                irradiance: [energy; 3],
                direction: [0.0; 3],
                axis: [0.5; 2],
                room: 0,
            })
            .to_vec();
        field.local_direct = Some(crate::lighting::probes::ProbeDirectField {
            light_indices: Vec::new(),
            probes: vec![crate::lighting::lightmap::LightmapTexel::ZERO; field.probes.len()],
        });
        let model = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(2.0, 0.75, 0.5),
            glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            glam::Vec3::ZERO,
        );
        let spatial = entity_spatial_lighting(
            &lighting,
            Some(&field),
            crate::spatial::Aabb {
                min: [-0.5, 0.8, -0.5],
                max: [0.5, 1.2, 0.5],
            },
            model,
            None,
        )
        .expect("rotated bounds");
        let energy = |corner: usize| {
            spatial
                .anchors
                .get(corner)
                .expect("corner")
                .irradiance
                .first()
                .copied()
                .expect("red")
        };
        assert!(
            (energy(0) - energy(1)).abs() < 1.0e-6,
            "local X rotates to symmetric world Z"
        );
        assert!(
            energy(4) > energy(0),
            "local Z rotates toward the brighter world X samples"
        );
    }

    #[test]
    fn legacy_combined_probe_fields_keep_the_existing_entity_path() {
        let (lighting, mut field) = spatial_fixture();
        field.local_direct = None;
        assert!(entity_spatial_key(&lighting, Some(&field), glam::Mat4::IDENTITY, None).is_none());
        assert!(
            entity_spatial_lighting(
                &lighting,
                Some(&field),
                crate::spatial::Aabb {
                    min: [-0.2, 0.8, -0.2],
                    max: [0.2, 1.2, 0.2],
                },
                glam::Mat4::IDENTITY,
                None,
            )
            .is_none()
        );
        let centre = entity_lighting(&lighting, Some(&field), [0.0, 1.0, 0.0]);
        assert_eq!(centre.source, EntityLightingSource::Prepared);
        assert_eq!(centre.prepared.expect("combined").irradiance, [0.7; 3]);
    }

    #[test]
    fn switchable_direct_joins_the_reserved_slot_without_subtracting_base_energy() {
        let (mut lighting, mut field) = spatial_fixture();
        field.local_direct = Some(crate::lighting::probes::ProbeDirectField {
            light_indices: Vec::new(),
            probes: vec![crate::lighting::lightmap::LightmapTexel::ZERO],
        });
        let encoded = field.write().expect("empty selected-source field");
        field = crate::lighting::probes::ProbeField::read(&encoded)
            .expect("serialized switch-only runtime field");
        let direct = field.local_direct.as_ref().expect("v3 presence marker");
        assert_eq!(direct.light_indices, [0_u32; 0]);
        assert_eq!(direct.probes.len(), field.probes.len());
        assert!(
            direct
                .probes
                .iter()
                .all(|sample| *sample == crate::lighting::lightmap::LightmapTexel::ZERO)
        );
        let emitter =
            TransportEmitter::from_baked(lighting.lights().first().expect("source"), Some(0));
        let scene = TransportScene::new(Vec::new(), vec![emitter]).expect("switchable scene");
        let bounds = crate::spatial::Aabb {
            min: [-0.2, 0.8, -0.2],
            max: [0.2, 1.2, 0.2],
        };
        assert_eq!(
            runtime_entity_light_ids(&field, Some(&scene)),
            [Some(0), None, None, None, None, None, None, None]
        );
        let on_key =
            entity_spatial_key(&lighting, Some(&field), glam::Mat4::IDENTITY, Some(&scene));
        let on = entity_spatial_lighting(
            &lighting,
            Some(&field),
            bounds,
            glam::Mat4::IDENTITY,
            Some(&scene),
        )
        .expect("on");
        assert!(on.direct.first().expect("live source").color_strength[3] > 0.0);
        assert_eq!(
            on.anchors.first().expect("base").irradiance,
            [0.7, 0.7, 0.7, 1.0]
        );
        assert!(lighting.set_light_enabled(0, false));
        let off_key =
            entity_spatial_key(&lighting, Some(&field), glam::Mat4::IDENTITY, Some(&scene));
        assert_ne!(
            on_key, off_key,
            "a switch invalidates a stationary entity payload"
        );
        let off = entity_spatial_lighting(
            &lighting,
            Some(&field),
            bounds,
            glam::Mat4::IDENTITY,
            Some(&scene),
        )
        .expect("off");
        assert_eq!(
            off.direct.first().expect("reserved source"),
            &EntityDirectLight::ZERO
        );
        assert_eq!(
            off.anchors,
            on.anchors.map(|mut anchor| {
                anchor.visibility = [[0.0; 4]; 8];
                anchor
            })
        );
        assert!(lighting.set_light_enabled(0, true));
        assert_eq!(
            entity_spatial_key(&lighting, Some(&field), glam::Mat4::IDENTITY, Some(&scene)),
            on_key,
            "serialized-field source identity restores the original cache key"
        );
        let restored = entity_spatial_lighting(
            &lighting,
            Some(&field),
            bounds,
            glam::Mat4::IDENTITY,
            Some(&scene),
        )
        .expect("restored");
        assert_eq!(
            *restored, *on,
            "restoration needs neither reload nor rebake"
        );
    }

    #[test]
    fn retained_source_materials_do_not_alias_after_an_empty_first_primitive() {
        // Both batches draw ordinal zero. The second has lost original source
        // primitive zero, while its retained sidecar still identifies slot one.
        let batch = |source_primitives: Vec<usize>, colour: [f32; 4]| PropMeshBatch {
            model: "source-material-control".to_owned(),
            casts_static_lighting: true,
            source_primitives,
            textures: Vec::new(),
            submeshes: vec![crate::render::PropSubmeshBatch {
                response: crate::materials::MaterialResponse::NONE,
                texture: None,
                emission: crate::materials::MaterialEmission::NONE,
                alpha: crate::materials::MaterialAlpha::OPAQUE,
                first_index: 0,
                index_count: 3,
            }],
            vertices: vec![
                Vertex::new([0.0, 0.0, 0.0], colour, [0.0, 0.0]),
                Vertex::new([1.0, 0.0, 0.0], colour, [1.0, 0.0]),
                Vertex::new([1.0, 1.0, 0.0], colour, [1.0, 1.0]),
            ],
            indices: vec![0, 1, 2],
            bounds: crate::spatial::Aabb::EMPTY,
        };
        let mut triangles = Vec::new();
        let mut surfaces = SurfaceAttributes::default();
        append_prop_triangles(
            &[
                batch(vec![0], [0.6; 4]),
                batch(vec![1], [0.9; 4]),
                batch(vec![1], [0.7; 4]),
                batch(Vec::new(), [0.6; 4]),
                batch(Vec::new(), [0.6; 4]),
                batch(vec![0, 1], [0.6; 4]),
            ],
            &mut triangles,
            &mut surfaces,
            &mut TransportSceneStats::default(),
            &mut Vec::new(),
        );
        let first_source = *surfaces.materials.first().expect("original primitive zero");
        let retained_source = *surfaces
            .materials
            .get(1)
            .expect("retained original primitive one");
        let continuation = *surfaces
            .materials
            .get(2)
            .expect("same original source in another batch");
        let first_unknown = *surfaces.materials.get(3).expect("first decoded batch");
        let second_unknown = *surfaces.materials.get(4).expect("second decoded batch");
        let malformed = *surfaces
            .materials
            .get(5)
            .expect("misaligned sidecar is conservative");
        assert_ne!(
            first_source, retained_source,
            "filtered draw ordinal zero cannot merge two original source materials"
        );
        assert_eq!(
            retained_source, continuation,
            "the same original material continues across prepared batches and sampled colour variation"
        );
        assert_ne!(
            first_unknown, second_unknown,
            "unknown decoded batches cannot guess shared material identity"
        );
        assert_ne!(
            malformed, first_source,
            "a partial or misaligned sidecar cannot guess an original source identity"
        );
        append_prop_triangles(
            &[batch(Vec::new(), [0.6; 4])],
            &mut triangles,
            &mut surfaces,
            &mut TransportSceneStats::default(),
            &mut Vec::new(),
        );
        let later_unknown = *surfaces
            .materials
            .last()
            .expect("later appended unknown batch");
        assert_ne!(
            later_unknown, first_unknown,
            "unknown batch scope must remain unique across separate append calls"
        );
        assert_ne!(
            later_unknown, second_unknown,
            "a later unknown batch must not alias either preceding unknown batch"
        );
        assert_eq!(
            triangles.len(),
            surfaces.materials.len(),
            "source identities stay aligned with accepted triangles"
        );
        assert_eq!(
            triangles.len(),
            surfaces.alpha.len(),
            "alpha and material identities share accepted triangle order"
        );
    }

    #[test]
    fn alpha_prop_cards_preserve_covered_pixels_and_opaque_fallbacks() {
        for mode in [AlphaMode::Opaque, AlphaMode::Cutout, AlphaMode::Blend] {
            let batch = PropMeshBatch {
                model: "alpha-contract".to_owned(),
                casts_static_lighting: true,
                source_primitives: vec![0],
                textures: Vec::new(),
                submeshes: vec![crate::render::PropSubmeshBatch {
                    response: crate::materials::MaterialResponse::NONE,
                    texture: None,
                    emission: crate::materials::MaterialEmission::NONE,
                    alpha: crate::materials::MaterialAlpha {
                        mode,
                        ..crate::materials::MaterialAlpha::OPAQUE
                    },
                    first_index: 0,
                    index_count: 6,
                }],
                vertices: vec![
                    Vertex::new([-1.0, 0.0, 0.0], [1.0; 4], [0.0, 0.0]),
                    Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [1.0, 0.0]),
                    Vertex::new([1.0, 2.0, 0.0], [1.0; 4], [1.0, 1.0]),
                    Vertex::new([-1.0, 2.0, 0.0], [1.0; 4], [0.0, 1.0]),
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                bounds: crate::spatial::Aabb::EMPTY,
            };
            let mut triangles = Vec::new();
            let mut surfaces = SurfaceAttributes::default();
            let mut animated = batch.clone();
            animated.casts_static_lighting = false;
            append_prop_triangles(
                &[animated],
                &mut triangles,
                &mut surfaces,
                &mut TransportSceneStats::default(),
                &mut Vec::new(),
            );
            assert_eq!(triangles, []);
            assert!(
                surfaces.alpha.is_empty(),
                "animated geometry has no static alpha surfaces"
            );
            assert!(
                surfaces.materials.is_empty(),
                "animated geometry has no static material identities"
            );
            append_prop_triangles(
                &[batch],
                &mut triangles,
                &mut surfaces,
                &mut TransportSceneStats::default(),
                &mut Vec::new(),
            );
            assert_eq!(triangles.len(), 2);
            let scene = TransportScene::new(triangles, Vec::new())
                .expect("scene")
                .with_surface_alpha(surfaces.alpha)
                .expect("aligned alpha")
                .with_surface_materials(surfaces.materials)
                .expect("aligned material identities");
            assert_eq!(
                scene.occluded([0.0, 1.0, -1.0], [0.0, 1.0, 1.0]),
                mode != AlphaMode::Blend
            );
            assert_eq!(
                scene.probe_is_clear([0.0, 1.0, 0.0]),
                mode == AlphaMode::Blend
            );
        }
    }

    #[test]
    fn architecture_bounce_albedo_excludes_vertex_tint_and_lighting() {
        let level = pane_level("core:wallpaper_yellow_01");
        let materials = crate::render::logical_materials(&level);
        let material = materials
            .index_of("core:wallpaper_yellow_01")
            .expect("material");
        let mut mesh = pane_mesh(material);
        let expected = material_albedo(&materials, material);
        for vertex in &mut mesh.ranges.first_mut().expect("one pane").vertices {
            vertex.color = [0.01, 0.3, 4.0, 1.0];
        }
        let mut triangles = Vec::new();
        append_architecture_triangles(
            &mesh,
            &materials,
            &WaterVolumes::new(),
            &mut triangles,
            &mut SurfaceAttributes::default(),
            &mut TransportSceneStats::default(),
            &mut Vec::new(),
        );
        assert_eq!(triangles.len(), 2);
        assert!(triangles.iter().all(|triangle| triangle.albedo == expected));
    }

    #[test]
    fn a_solid_texture_reads_its_colour() {
        let image = RawImage::new(2, 2, [0, 128, 255, 255].repeat(4));
        let mean = mean_texture_color(&image);
        assert!((mean[0] - 0.0).abs() < 1e-6);
        assert!((mean[1] - crate::materials::color::decode_byte(128)).abs() < 1e-6);
        assert!((mean[2] - 1.0).abs() < 1e-6);
        let sample = sample_texture(&image, [0.75, 0.25]);
        for (actual, expected) in sample.iter().zip(mean.iter()) {
            assert!((actual - expected).abs() < 1.0e-6, "{sample:?} vs {mean:?}");
        }
    }

    #[test]
    fn a_prop_sample_clamps_to_the_runtime_sheet_edges() {
        let image = RawImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        );
        let sample = sample_texture(&image, [1.25, -0.25]);
        assert_eq!(sample, [0.0, 1.0, 0.0]);
    }

    use crate::level::LevelDef;
    use crate::render::{LevelMeshBatches, LevelMeshRange, SurfaceKey};
    use crate::spatial::Aabb;

    /// A one-room level whose floor references `material`, so the logical
    /// material table resolves it.
    fn pane_level(material: &str) -> LevelDef {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "transport_pane",
                "name": "Transport Pane",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
                            "material": "{material}" }}]
            }}"#
        ))
        .expect("pane level parses")
    }

    /// A hand-built vertical wall pane at `z = 0`, one quad, one material.
    fn pane_mesh(material: u32) -> LevelMesh {
        let vertices = vec![
            Vertex::new([-1.0, 0.0, 0.0], [1.0; 4], [0.0, 0.0]),
            Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [1.0, 0.0]),
            Vertex::new([1.0, 2.0, 0.0], [1.0; 4], [1.0, 1.0]),
            Vertex::new([-1.0, 2.0, 0.0], [1.0; 4], [0.0, 1.0]),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        LevelMesh {
            ranges: vec![LevelMeshRange {
                key: SurfaceKey::new(SurfaceKind::Wall, material),
                vertices,
                indices,
                bounds: Aabb::EMPTY,
            }],
            batches: LevelMeshBatches::default(),
            vertex_count: 4,
            index_count: 6,
        }
    }

    /// The grille's authored central slat blocks, while clear BLEND glazing
    /// retains a straight transmitting path and an opaque wall blocks.
    #[test]
    fn architecture_panes_keep_glass_transmission_and_grille_slats() {
        let from = [0.0, 1.0, -1.0];
        let to = [0.0, 1.0, 1.0];
        for (material, transmits) in [
            ("core:glass_window_clear_01", true),
            ("core:grille_vent_01", false),
            ("core:painting_dull_01", false),
        ] {
            let level = pane_level(material);
            let materials = crate::render::logical_materials(&level);
            let material_index = materials.index_of(material).expect("material resolves");
            let mesh = pane_mesh(material_index);
            let lighting = LevelLighting::bake(&level);
            let (scene, _) = build_transport_scene(&level, &mesh, &[], &materials, &lighting, &[])
                .expect("scene builds");
            assert_eq!(
                scene.occluded(from, to),
                !transmits,
                "{material}: the pane must {} the ray",
                if transmits { "transmit" } else { "block" }
            );
        }
    }

    /// A void wall's drawn faces are real transport geometry: the prepared
    /// solve blocks on them exactly as it blocks on a solid prop's drawn
    /// triangles, independent of the fast-path `occludes` flag. Transparency
    /// is the only transmission, exactly like every other drawn surface.
    #[test]
    fn void_wall_faces_block_the_prepared_transport_solve() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 3,
                "id": "transport_void",
                "name": "Transport Void",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": -4.0, "z": -4.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
                "void_walls": [
                    { "id": "slab", "min": [-0.15, 0.0, -2.0], "max": [0.15, 3.0, 2.0],
                      "material": "core:wallpaper_yellow_01", "occludes": false }
                ]
            }"#,
        )
        .expect("transport void level parses");
        let materials = crate::render::logical_materials(&level);
        let mesh = crate::render::build_level_geometry(&level);
        let lighting = LevelLighting::bake(&level);
        let (scene, _) = build_transport_scene(&level, &mesh, &[], &materials, &lighting, &[])
            .expect("scene builds");
        assert!(
            scene.occluded([-2.0, 1.0, 0.0], [2.0, 1.0, 0.0]),
            "the drawn faces block the solve even with occludes: false"
        );
        assert!(
            !scene.occluded([-2.0, 4.0, 0.0], [2.0, 4.0, 0.0]),
            "above the box the ray escapes"
        );
    }

    /// The authored water volume reaches the solver as a transmissive surface
    /// and an attenuating body: a ray crosses the surface, a point below it is
    /// attenuated by its submerged depth, and a point above is not.
    #[test]
    fn a_water_volume_transmits_and_attenuates_by_submerged_depth() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 3,
                "id": "transport_water",
                "name": "Transport Water",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 } ],
                "water": [
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0,
                      "surface_y": 1.0, "bottom_y": -1.0 }
                ]
            }"#,
        )
        .expect("water level parses");
        let materials = crate::render::logical_materials(&level);
        let water_index = materials
            .index_of(crate::level::DEFAULT_WATER_MATERIAL)
            .expect("the water material resolves");
        // The same quad the water emitter draws: `p0, p1, p2` then `p0, p2, p3`.
        let vertices = vec![
            Vertex::new([0.0, 1.0, 4.0], [1.0; 4], [0.0, 0.0]),
            Vertex::new([4.0, 1.0, 4.0], [1.0; 4], [1.0, 0.0]),
            Vertex::new([4.0, 1.0, 0.0], [1.0; 4], [1.0, 1.0]),
            Vertex::new([0.0, 1.0, 0.0], [1.0; 4], [0.0, 1.0]),
        ];
        let mesh = LevelMesh {
            ranges: vec![LevelMeshRange {
                key: SurfaceKey::new(SurfaceKind::Floor, water_index),
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                bounds: Aabb::EMPTY,
            }],
            batches: LevelMeshBatches::default(),
            vertex_count: 4,
            index_count: 6,
        };
        let lighting = LevelLighting::bake(&level);
        let (scene, _) = build_transport_scene(&level, &mesh, &[], &materials, &lighting, &[])
            .expect("scene builds");
        assert!(
            !scene.occluded([2.0, 0.0, 2.0], [2.0, 2.0, 2.0]),
            "the water surface must transmit"
        );
        let below = scene.attenuation_at([2.0, 0.5, 2.0]);
        assert!(
            below.iter().all(|value| *value < 1.0),
            "a submerged point must be attenuated: {below:?}"
        );
        assert!(
            below[0] < below[1] && below[1] < below[2],
            "red is absorbed most: {below:?}"
        );
        assert_eq!(
            scene.attenuation_at([2.0, 1.5, 2.0]),
            [1.0; 3],
            "above the surface there is no attenuation"
        );
    }
}
