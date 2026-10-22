//! Dynamic objects: the moving half of the scene.
//!
//! The static renderer bakes every surface, fixture and placed prop once at
//! level load into shared, pre-transformed vertex buffers, and the frame loop
//! only submits ranges and samples the baked light. **Nothing in this module
//! rebuilds, re-bakes or re-uploads static geometry.** A dynamic object is a
//! different kind of thing with a different lifetime:
//!
//! ```text
//! static                              dynamic (this module)
//! ------                              ----------------------
//! authored in the level JSON          spawned by game code or movable props
//! transformed once at level load      transformed every frame (matrix only)
//! baked into LevelMesh / prop batch   uploaded once in model space
//! baked light in vertex colour        baked light in the u_light_scale uniform
//! part of the lightmap occlusion set  invisible to the baker and to collision
//! ```
//!
//! Representation
//! --------------
//! [`DynamicScene`] owns a bounded, ordered list of [`DynamicObject`]s and one
//! [`DynamicMesh`] per distinct model. A mesh is the *same*
//! [`crate::engine::props::LoadedPropAsset`] the static prop path resolves, converted
//! once into render vertices in **model space**; its textures are the same
//! `Arc`-shared decoded images, so spawning a drum that is also placed statically
//! decodes nothing twice.
//!
//! A transform is translation + base orientation + spin about a local axis +
//! uniform scale. It is deliberately not a general matrix: a dynamic object is
//! a rigid prop-like thing, and the spin is a single axis-angle rotation. The
//! base orientation reorients the whole object, so a model built mouth-up can
//! turn about a horizontal axis; a yaw-only object (identity base, spin axis
//! Y) keeps exactly the transform it always had. The renderer composes
//! `view_projection * transform` into the existing `u_mvp` uniform per object,
//! so moving an object costs one uniform upload and never touches a vertex
//! buffer.
//!
//! Spatial lighting
//! ----------------
//! Prepared probes retain incident linear HDR. A movable object keeps its
//! combined centre sample for diagnostics and eight transformed bounds anchors
//! for residual irradiance and selected-source visibility. The shared shader
//! reconstructs residual directionality and evaluates selected practical sources
//! at the actual fragment position/normal, without including their baked direct
//! contribution twice. Missing prepared fields retain authored fallback light.
//!
//! Immutable geometry/PNG alpha supplies finite-source transmission; unchanged
//! transforms and field resources reuse their spatial payload. Bind-pose bounds
//! are a bounded interpolation approximation rather than per-vertex probe reads.
//! Lightweight projected bounds shadows ground opaque movable subjects on the
//! real floor; they are not mesh silhouette or self-shadow maps.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::mesh::LIGHTMAP_NONE;
use super::{LevelDef, Vertex};
use crate::engine::gltf::PropModel;
use crate::engine::level::{LevelSurfaces, WaterVolumes};
use crate::engine::lighting::LevelLighting;
use crate::engine::materials::{MaterialAlpha, MaterialEmission};
use crate::engine::props::{LoadedPropAsset, PropAssets};
use crate::engine::spatial::Aabb;

/// Most dynamic objects one scene holds.
///
/// Bounded headroom for 128 physics items alongside doors, floats and runtime
/// entities. Each object still costs one draw per material, so static scenery
/// remains batched. The limit is enforced on spawn; pose updates reuse geometry.
pub const MAX_DYNAMIC_OBJECTS: usize = 256;

/// Most distinct dynamic meshes one scene holds, each with a vertex buffer.
///
/// One mesh per admitted object is legal, including separate door frames and
/// leaves. Aggregate geometry retains the former sixteen-model worst case;
/// many small movable models must not exhaust an unrelated demo-era slot cap.
/// Unreferenced asset slots may be replaced; live and registered slots stay stable.
pub const MAX_DYNAMIC_MESHES: usize = MAX_DYNAMIC_OBJECTS;

const FORMER_DYNAMIC_MESH_LIMIT: usize = 16;
const MAX_DYNAMIC_VERTEX_COUNT: usize =
    FORMER_DYNAMIC_MESH_LIMIT.saturating_mul(crate::engine::level::MAX_PROP_VERTICES);
const MAX_DYNAMIC_INDEX_COUNT: usize = FORMER_DYNAMIC_MESH_LIMIT
    .saturating_mul(crate::engine::gltf::MAX_REVIEWED_PROP_TRIANGLES)
    .saturating_mul(3);

/// Checks lengths, not allocated fixture geometry. Checked sums also protect
/// code-built registrations, whose callers do not pass through the GLB reader.
fn aggregate_geometry_admitted(counts: impl Iterator<Item = (usize, usize)>) -> Result<(), String> {
    let mut vertices = 0usize;
    let mut indices = 0usize;
    for (vertex_count, index_count) in counts {
        vertices = vertices
            .checked_add(vertex_count)
            .ok_or_else(|| "aggregate dynamic mesh vertex count overflow".to_string())?;
        indices = indices
            .checked_add(index_count)
            .ok_or_else(|| "aggregate dynamic mesh index count overflow".to_string())?;
    }
    if vertices > MAX_DYNAMIC_VERTEX_COUNT {
        return Err(format!(
            "aggregate dynamic mesh vertices {vertices} exceed {MAX_DYNAMIC_VERTEX_COUNT}"
        ));
    }
    if indices > MAX_DYNAMIC_INDEX_COUNT {
        return Err(format!(
            "aggregate dynamic mesh indices {indices} exceed {MAX_DYNAMIC_INDEX_COUNT}"
        ));
    }
    Ok(())
}

/// Historical movement threshold, retained for API compatibility. Runtime
/// lighting follows exact sample positions; this value creates no dead zone.
pub const PROBE_EPSILON_M: f32 = 0.05;

/// Receiver refreshes can issue hundreds of independent visibility rays.
/// Small scenes stay on the caller; larger batches share the runtime pool.
const PARALLEL_LIGHTING_MIN_RECEIVERS: usize = 8;

/// Center samples depend on the field, authored lighting and exact position,
/// independently of movable-caster visibility. The caller's revision covers
/// resource contents, including in-place mutations and fixture switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EntityCenterLightKey {
    revision: u64,
    lighting: usize,
    field: usize,
    position: [u32; 3],
}

impl EntityCenterLightKey {
    pub(super) fn new(
        revision: u64,
        lighting: &LevelLighting,
        field: Option<&crate::engine::lighting::probes::ProbeField>,
        position: [f32; 3],
    ) -> Self {
        Self {
            revision,
            lighting: std::ptr::from_ref(lighting).addr(),
            field: field.map_or(0, |value| std::ptr::from_ref(value).addr()),
            position: position.map(f32::to_bits),
        }
    }
}

/// Exact revisioned receiver dependencies, checked before emitter enumeration
/// or worker dispatch. Bounds/center also cover animated balloon lighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EntityLightingRefreshKey {
    center: EntityCenterLightKey,
    model: [[u32; 4]; 4],
    bounds: [[u32; 3]; 2],
    scene: usize,
    dynamic: usize,
    dynamic_revision: u64,
    observed_dynamic_revision: u64,
    receiver: Option<DynamicId>,
}

impl EntityLightingRefreshKey {
    pub(super) const fn center_key(self) -> EntityCenterLightKey {
        self.center
    }

    /// A moving center diagnostic can stay inside unchanged spatial bounds.
    /// Only resources, bounds, placement and source visibility invalidate the
    /// anchor payload; the center position has its independent sample key.
    pub(super) fn spatial_changed(self, previous: Option<Self>) -> bool {
        previous.is_none_or(|cached| {
            self.center.revision != cached.center.revision
                || self.center.lighting != cached.center.lighting
                || self.center.field != cached.center.field
                || self.model != cached.model
                || self.bounds != cached.bounds
                || self.scene != cached.scene
                || self.dynamic != cached.dynamic
                || self.dynamic_revision != cached.dynamic_revision
                || self.receiver != cached.receiver
        })
    }
}

/// Prepared once before worker dispatch, consumed by the same synchronous batch.
/// The receiver publishes its dependency key in preparation; only these two
/// independent payload updates remain for its worker.
#[derive(Debug, Default)]
pub(super) struct EntityLightingRefreshPlan {
    pub(super) center: bool,
    pub(super) spatial: bool,
}

impl EntityLightingRefreshPlan {
    pub(super) fn new(
        key: Option<EntityLightingRefreshKey>,
        previous: Option<EntityLightingRefreshKey>,
        center: Option<EntityCenterLightKey>,
    ) -> Self {
        Self {
            center: key.is_none_or(|current| Some(current.center) != center),
            spatial: key.is_none_or(|current| current.spatial_changed(previous)),
        }
    }

    pub(super) const fn needed(&self) -> bool {
        self.center || self.spatial
    }
}

#[derive(Clone, Copy)]
pub(super) struct EntityLightingRefreshContext<'a> {
    lighting: &'a LevelLighting,
    field: Option<&'a crate::engine::lighting::probes::ProbeField>,
    scene: Option<&'a crate::engine::lighting::transport::TransportScene>,
    visibility: Option<&'a super::dynamic_visibility::DynamicVisibility>,
    revision: Option<u64>,
}

impl<'a> EntityLightingRefreshContext<'a> {
    pub(super) const fn new(
        lighting: &'a LevelLighting,
        field: Option<&'a crate::engine::lighting::probes::ProbeField>,
        scene: Option<&'a crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&'a super::dynamic_visibility::DynamicVisibility>,
        revision: Option<u64>,
    ) -> Self {
        Self {
            lighting,
            field,
            scene,
            visibility,
            revision,
        }
    }

    pub(super) fn key(
        self,
        model: Mat4,
        bounds: Aabb,
        position: [f32; 3],
        receiver: Option<DynamicId>,
        previous: Option<EntityLightingRefreshKey>,
        rays: Option<&super::light_transport::EntityStaticRayCache>,
    ) -> Option<EntityLightingRefreshKey> {
        let observed = self
            .visibility
            .map_or(0, super::dynamic_visibility::DynamicVisibility::revision);
        let mut key = EntityLightingRefreshKey {
            center: EntityCenterLightKey::new(self.revision?, self.lighting, self.field, position),
            model: model
                .to_cols_array_2d()
                .map(|column| column.map(f32::to_bits)),
            bounds: [bounds.min.map(f32::to_bits), bounds.max.map(f32::to_bits)],
            scene: self
                .scene
                .map_or(0, |value| std::ptr::from_ref(value).addr()),
            dynamic: self
                .visibility
                .map_or(0, |value| std::ptr::from_ref(value).addr()),
            dynamic_revision: observed,
            observed_dynamic_revision: observed,
            receiver,
        };
        if let Some(cached) = previous {
            let mut unchanged = key;
            unchanged.dynamic_revision = cached.dynamic_revision;
            // A moving receiver or changed resource already needs new samples;
            // it must not first scan an obsolete dependency set to learn that.
            if !unchanged.spatial_changed(Some(cached))
                && self.visibility.is_some_and(|visibility| {
                    visibility
                        .changed_regions_since(cached.observed_dynamic_revision)
                        .is_some_and(|regions| {
                            regions.iter().all(|dirty| {
                                !self.visibility_affects(
                                    super::props::transform_bounds(&bounds, &model),
                                    *dirty,
                                ) || rays.is_some_and(|cache| !cache.visibility_affected(*dirty))
                            })
                        })
                })
            {
                key.dynamic_revision = cached.dynamic_revision;
            }
        }
        Some(key)
    }

    /// Old and new caster bounds are a conservative visibility dependency.
    /// Missed revisions use the complete scene; unchanging receivers elsewhere
    /// retain their exact eight-anchor payload without enumerating ray samples.
    fn visibility_affects(self, receivers: Aabb, dirty: Aabb) -> bool {
        use crate::engine::spatial::{
            directional_light_region_overlaps, finite_light_region_overlaps,
        };
        if dirty.is_empty() {
            return false;
        }
        if self.field.is_some_and(|field| {
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
        self.lighting
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
            || self.lighting.global_lights().iter().any(|light| {
                light.angular_radius > 0.0
                    || directional_light_region_overlaps(light.incoming, receivers, dirty)
            })
    }
}

/// Degrees per second the demonstration drum turns.
pub const DEMO_SPIN_DEGREES_PER_SECOND: f32 = 12.0;

/// Catalogue id of the static prop the washer-drum demonstration is built around.
pub const DEMO_MACHINE_ID: &str = "core:washing_machine";

/// Catalogue id of the dynamic drum the demonstration spawns.
pub const DEMO_DRUM_ID: &str = "core:washer_drum";

/// Model-space axis an object spins about when the spawner does not name one:
/// the local Y axis, which is the yaw-only case every pre-existing caller uses.
const MODEL_Y_AXIS: [f32; 3] = [0.0, 1.0, 0.0];

/// Shortest model-space spin axis accepted, in metres. A zero-length axis has
/// no rotation to apply, so it is refused like any other malformed transform
/// value rather than normalising to NaN.
const SPIN_AXIS_EPSILON: f32 = 1e-6;

/// The orientation half of an oriented spawn: the base reorientation applied
/// after the spin, plus the model-space axis the spin turns about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnOrientation {
    /// Orientation applied after the spin (translate × base × spin × scale).
    pub base_rotation: Quat,
    /// Model-space axis the spin turns about; normalised on spawn.
    pub spin_axis: [f32; 3],
}

impl SpawnOrientation {
    /// The yaw-only orientation [`DynamicScene::spawn`] uses: identity base,
    /// local Y axis.
    pub const YAW_ONLY: Self = Self {
        base_rotation: Quat::IDENTITY,
        spin_axis: MODEL_Y_AXIS,
    };
}

/// Model-space height of the washing machine's porthole centre, in metres.
///
/// Mirrors `centre_y` in `build_washing_machine`
/// (`tools/props/parts/appliances.py`): the drum is spawned on the porthole's
/// axis so it turns inside the cavity the model opens behind the front panel.
pub const WASHER_PORTHOLE_CENTRE_Y: f32 = 0.44;

/// Model-space inner radius of the washing machine's porthole opening.
///
/// Mirrors `opening_radius` in `build_washing_machine`; the drum is scaled to
/// pass through it with [`WASHER_DRUM_PORTHOLE_CLEARANCE`] to spare.
pub const WASHER_PORTHOLE_OPENING_RADIUS: f32 = 0.155;

/// Model-space depth of the machine's proud porthole bezel: the front ring
/// stands this far in front of the body's front panel. Mirrors
/// `drum_front - panel_z` in `build_washing_machine`.
pub const WASHER_PORTHOLE_BEZEL_DEPTH: f32 = 0.03;

/// Radial gap held between the drum and the porthole opening, in metres.
pub const WASHER_DRUM_PORTHOLE_CLEARANCE: f32 = 0.005;

/// How far the drum's mouth sits behind the machine's front panel, in model
/// space: the drum is inside the body, never protruding past its surface.
pub const WASHER_DRUM_MOUTH_RECESS: f32 = 0.02;

/// Stable handle to one object inside a [`DynamicScene`].
///
/// Handles are never reused: despawning increments the scene's id counter, so a
/// stale handle resolves to `None` instead of silently addressing a new object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynamicId(u32);

/// One primitive of a dynamic mesh: a texture slot, its emission, its alpha
/// contract and its slice of the model's index list.
///
/// Exactly [`crate::engine::render::PropMeshBatch`]'s submesh shape, minus the
/// pre-transformed vertices: the renderer reuses its material/emission routing
/// unchanged, so a dynamic object never grows a second material system. The
/// alpha contract is what lets a code-built object (a sauna door's glass panel)
/// draw in the sorted translucent pass: a moving transparent surface is a
/// normal material property, not a special case.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicSubmesh {
    /// Imported sidedness with conservative skin/morph bind-pose fallback.
    pub double_sided: bool,
    /// Shared scalar material response for every mesh route.
    pub response: crate::engine::materials::MaterialResponse,
    /// Index into [`DynamicMesh::textures`], or `None` for an untextured
    /// material (drawn through the shared white sheet).
    pub texture: Option<u16>,
    /// The material's own emission.
    pub emission: MaterialEmission,
    /// The material's alpha contract: opaque, cut-out or blended.
    pub alpha: MaterialAlpha,
    /// First index into [`DynamicMesh::indices`].
    pub first_index: u32,
    /// Number of indices in this submesh.
    pub index_count: u32,
}

/// One distinct model's geometry in **model space**, shared by every object
/// that uses it.
///
/// Vertices are never pre-transformed and never lit: the renderer uploads this
/// once per distinct model and every instance differs only in the matrix it
/// multiplies through `u_mvp`, so a moving object never rebuilds geometry.
#[derive(Debug)]
pub struct DynamicMesh {
    /// Catalogue model path, matching the static prop path's model key.
    pub model_path: String,
    /// Model-space vertices; the colour is albedo/tint only.
    pub vertices: Vec<Vertex>,
    /// Indices into [`Self::vertices`].
    pub indices: Vec<u16>,
    /// One entry per primitive, in index order.
    pub submeshes: Vec<DynamicSubmesh>,
    /// Decoded textures, indexed by [`DynamicSubmesh::texture`] and by
    /// [`MaterialEmission::mask`]. Shared with the model's other users.
    pub textures: Vec<Arc<crate::engine::loader::RawImage>>,
    /// Model-space bounds, for frustum culling and probe placement.
    pub bounds: Aabb,
    /// Model-space centre: the transformed probe point of every instance.
    pub centre: [f32; 3],
}

impl DynamicMesh {
    /// Builds the model-space mesh for one resolved prop asset.
    ///
    /// The model's vertices are copied verbatim, colour included; that colour is
    /// the model's albedo/tint (the toolkit's baked face shades included), *not*
    /// a baked light. The light arrives per frame through `u_light_scale`. A
    /// model with no drawable primitive yields `None`: there is nothing to draw
    /// and no material to route.
    #[must_use]
    pub fn from_asset(asset: &LoadedPropAsset) -> Option<Self> {
        let textures = asset.model.textures.clone();
        // The primitive's own alpha contract, exactly as the importer read it:
        // a code-built model and an imported GLB classify identically now that
        // `alphaMode: "BLEND"` imports as a real translucent contract.
        let alphas: Vec<MaterialAlpha> = asset
            .model
            .submeshes
            .iter()
            .map(|submesh| submesh.alpha)
            .collect();
        Self::from_model(asset.model_path.clone(), &asset.model, textures, &alphas)
    }

    /// Builds the model-space mesh for a model the engine itself authored.
    ///
    /// `textures` are the decoded images the model's submeshes index, and
    /// `alphas[i]` is submesh `i`'s alpha contract (out-of-range entries are
    /// opaque). This is the constructor for code-built geometry — sauna door
    /// leaves and frames — and shares every downstream path with a GLB prop.
    #[must_use]
    pub fn from_model(
        model_path: String,
        model: &PropModel,
        textures: Vec<Arc<crate::engine::loader::RawImage>>,
        alphas: &[MaterialAlpha],
    ) -> Option<Self> {
        if model.submeshes.is_empty() || model.indices.is_empty() {
            return None;
        }
        let vertices: Vec<Vertex> = model
            .vertices
            .iter()
            .map(|vertex| Vertex {
                pos: vertex.pos,
                normal: vertex.normal.unwrap_or([0.0; 3]),
                color: vertex.color,
                caustics_albedo: [vertex.color[0], vertex.color[1], vertex.color[2]],
                uv: vertex.uv,
                // A dynamic vertex is never lightmapped: the atlas channel is
                // unused and the probe uniform carries the light instead.
                lightmap: [0; 2],
                lightmap_page: LIGHTMAP_NONE,
                ..Vertex::UNLIT
            })
            .collect();
        let submeshes: Vec<DynamicSubmesh> = model
            .submeshes
            .iter()
            .enumerate()
            .filter(|(_, submesh)| submesh.index_count > 0)
            .map(|(index, submesh)| DynamicSubmesh {
                // This route freezes the bind/default morph pose and does not
                // retain enough rig data to establish its outward orientation.
                double_sided: submesh.double_sided
                    || model.skin.is_some()
                    || !model.morph_targets.is_empty(),
                response: submesh.response,
                texture: submesh.texture,
                emission: submesh.emission,
                alpha: alphas.get(index).copied().unwrap_or(MaterialAlpha::OPAQUE),
                first_index: submesh.first_index,
                index_count: submesh.index_count,
            })
            .collect();
        if submeshes.is_empty() {
            return None;
        }
        let bounds = match model.bounds() {
            Some((low, high)) => Aabb {
                min: low,
                max: high,
            },
            None => Aabb::EMPTY,
        };
        let centre = if bounds.min.iter().all(|value| value.is_finite()) {
            [
                f32::midpoint(bounds.min[0], bounds.max[0]),
                f32::midpoint(bounds.min[1], bounds.max[1]),
                f32::midpoint(bounds.min[2], bounds.max[2]),
            ]
        } else {
            [0.0, 0.0, 0.0]
        };
        Some(Self {
            model_path,
            vertices,
            indices: model.indices.clone(),
            submeshes,
            textures,
            bounds,
            centre,
        })
    }

    /// Number of draw calls one instance of this mesh submits.
    #[must_use]
    pub const fn draw_count(&self) -> usize {
        self.submeshes.len()
    }

    /// Distinct vertices one instance of this mesh submits, for the developer
    /// log and the frame counters.
    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}

/// Rotation matrix for one spin of `degrees` about a unit model-space `axis`.
///
/// The Y case is the dedicated yaw constructor every existing caller used, so
/// their transforms are bit-for-bit what they always were; any other axis goes
/// through the general axis-angle path.
fn spin_matrix(axis: Vec3, degrees: f32) -> Mat4 {
    let angle = degrees.to_radians();
    if axis == Vec3::Y {
        Mat4::from_rotation_y(angle)
    } else {
        Mat4::from_axis_angle(axis, angle)
    }
}

/// Normalizes an externally supplied pose without overflowing its norm.
/// Equivalent quaternion signs and signed zeroes share one transform/cache key.
fn normalized_pose_rotation(orientation: [f32; 4]) -> Option<Quat> {
    if !orientation.iter().all(|value| value.is_finite()) {
        return None;
    }
    let magnitude = orientation.into_iter().map(f32::abs).fold(0.0, f32::max);
    if magnitude == 0.0 {
        return None;
    }
    let normalized = Quat::from_array(orientation.map(|value| value / magnitude))
        .normalize()
        .to_array();
    let negate = normalized
        .iter()
        .rev()
        .find(|value| **value != 0.0)
        .is_some_and(|value| *value < 0.0);
    Some(Quat::from_array(normalized.map(|value| {
        if value == 0.0 {
            0.0
        } else if negate {
            -value
        } else {
            value
        }
    })))
}

/// One live dynamic object: a shared model-space mesh plus its own transform,
/// spin and light probe.
#[derive(Debug)]
pub struct DynamicObject {
    id: DynamicId,
    /// Retained objects can be suspended without releasing their resources.
    visible: bool,
    mesh_index: usize,
    mesh: Arc<DynamicMesh>,
    /// Shared by visibility, lighting and rendering until the next pose mutation.
    placement: std::sync::OnceLock<DynamicPlacement>,
    translation: Vec3,
    /// Orientation applied before the spin; identity for yaw-only objects.
    base_rotation: Quat,
    /// Unit model-space axis the spin turns about.
    spin_axis: Vec3,
    /// Accumulated spin angle about [`Self::spin_axis`], in degrees.
    spin_degrees: f32,
    scale: f32,
    spin_degrees_per_second: f32,
    emission: Option<MaterialEmission>,
    /// Runtime emission multiplier selected by a material-variant action.
    /// `1.0` until one does; multiplies the object override and each
    /// primitive's own material emission, so `0.0` switches the object's
    /// emission off.
    emission_scale: f32,
    albedo_tint: [f32; 3],
    light_scale: [f32; 3],
    entity_lighting: Option<super::light_transport::EntityLighting>,
    center_light_key: Option<EntityCenterLightKey>,
    lighting_refresh_key: Option<EntityLightingRefreshKey>,
    lighting_refresh_plan: EntityLightingRefreshPlan,
    spatial_lighting: Option<Box<super::light_transport::EntitySpatialLighting>>,
    spatial_key: Option<super::light_transport::EntitySpatialKey>,
    static_rays: super::light_transport::EntityStaticRayCache,
    probe_position: [f32; 3],
    probe_valid: bool,
    /// Authored water-driven motion, for a floating prop. `None` for an
    /// ordinary spinning object.
    float: Option<FloatMotion>,
}

#[derive(Debug)]
struct DynamicPlacement {
    model: Mat4,
    centre: Vec3,
    bounds: Aabb,
}

/// One floating object's authored motion, resolved from the level once.
///
/// The motion has no horizontal freedom: the object stays at its authored
/// `(x, z)` and its height is a pure function of the water surface, the
/// authored draft and the bob phase, so per-frame integration can never drift.
/// Validation proves the swept footprint stays inside the water volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatMotion {
    /// Authored world X/Z the object floats at.
    anchor: Vec3,
    /// Fallback height when no water is found under the anchor.
    dry_y: f32,
    draft: f32,
    bob_amplitude: f32,
    /// Bob angular rate, radians per second.
    bob_rate: f32,
    heel_amplitude: f32,
    /// Heel angular rate, radians per second.
    heel_rate: f32,
    /// Phase offset, radians.
    phase: f32,
}

impl FloatMotion {
    /// The object's translation at `seconds`: the water surface under the
    /// anchor (or the dry fallback), minus the draft, plus the bob.
    #[must_use]
    fn translation_at(&self, seconds: f32, water: &WaterVolumes) -> Vec3 {
        let bob = self.bob_amplitude * (seconds.mul_add(self.bob_rate, self.phase)).sin();
        let surface = water
            .surface_y_at(self.anchor.x, self.anchor.z)
            .unwrap_or(self.dry_y);
        Vec3::new(self.anchor.x, surface - self.draft + bob, self.anchor.z)
    }

    /// The object's heel angle at `seconds`, in degrees about its local Z.
    #[must_use]
    fn heel_at(&self, seconds: f32) -> f32 {
        self.heel_amplitude * (seconds.mul_add(self.heel_rate, self.phase)).sin()
    }
}

impl DynamicObject {
    /// This object's stable handle.
    #[must_use]
    pub const fn id(&self) -> DynamicId {
        self.id
    }

    /// Whether the retained object draws, casts shadows and receives lighting.
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// The shared model-space mesh this object draws.
    #[must_use]
    pub const fn mesh(&self) -> &Arc<DynamicMesh> {
        &self.mesh
    }

    /// This object's slot in [`DynamicScene::meshes`].
    #[must_use]
    pub const fn mesh_index(&self) -> usize {
        self.mesh_index
    }

    /// The catalogue model path this object draws.
    #[must_use]
    pub fn model_path(&self) -> &str {
        &self.mesh.model_path
    }

    /// The object's world transform: translate × base × spin × uniform scale.
    ///
    /// The spin is about [`Self::spin_axis`] in model space, then the base
    /// rotation reorients the spun object (so a mouth-up basket pitched 90
    /// degrees about X turns about the machine's front axis). A yaw-only
    /// object keeps exactly the transform it always had: an identity base, a
    /// Y axis and the dedicated `from_rotation_y` construction.
    #[must_use]
    pub fn transform(&self) -> Mat4 {
        self.placement().model
    }

    fn placement(&self) -> &DynamicPlacement {
        self.placement.get_or_init(|| {
            let spin = spin_matrix(self.spin_axis, self.spin_degrees);
            let rotation = Mat4::from_quat(self.base_rotation).mul_mat4(&spin);
            let scale = Mat4::from_scale(Vec3::splat(self.scale));
            let translation = Mat4::from_translation(self.translation);
            let model = translation.mul_mat4(&rotation).mul_mat4(&scale);
            DynamicPlacement {
                centre: model.transform_point3(Vec3::from(self.mesh.centre)),
                bounds: super::props::transform_bounds(&self.mesh.bounds, &model),
                model,
            }
        })
    }

    /// All pose writers pass here. Exact input bits keep even sub-epsilon
    /// motion correct while repeated solver/door poses retain shared results.
    fn set_placement(&mut self, translation: Vec3, rotation: Quat, spin_degrees: f32, scale: f32) {
        if self.translation.to_array().map(f32::to_bits) != translation.to_array().map(f32::to_bits)
            || self.base_rotation.to_array().map(f32::to_bits)
                != rotation.to_array().map(f32::to_bits)
            || self.spin_degrees.to_bits() != spin_degrees.to_bits()
            || self.scale.to_bits() != scale.to_bits()
        {
            let _previous = self.placement.take();
        }
        self.translation = translation;
        self.base_rotation = rotation;
        self.spin_degrees = spin_degrees;
        self.scale = scale;
    }

    /// The transform's translation.
    #[must_use]
    pub const fn translation(&self) -> [f32; 3] {
        [self.translation.x, self.translation.y, self.translation.z]
    }

    /// The accumulated spin angle, in degrees, about [`Self::spin_axis`].
    #[must_use]
    pub const fn spin_degrees(&self) -> f32 {
        self.spin_degrees
    }

    /// The unit model-space axis this object spins about.
    #[must_use]
    pub const fn spin_axis(&self) -> [f32; 3] {
        [self.spin_axis.x, self.spin_axis.y, self.spin_axis.z]
    }

    /// The orientation applied before the spin; identity for yaw-only objects.
    #[must_use]
    pub const fn base_rotation(&self) -> Quat {
        self.base_rotation
    }

    /// The transform's uniform scale.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// Yaw the object gains per second; zero means it stands still.
    #[must_use]
    pub const fn spin_degrees_per_second(&self) -> f32 {
        self.spin_degrees_per_second
    }

    /// The object-wide emission override, or `None` when each primitive draws
    /// its own material emission.
    #[must_use]
    pub const fn emission(&self) -> Option<MaterialEmission> {
        self.emission
    }

    /// The runtime emission scale; `1.0` until a material-variant action
    /// selects another value.
    ///
    /// The scale multiplies whichever emission [`Self::submesh_emission`]
    /// resolves for a primitive — the object override when one is set,
    /// otherwise the primitive's own material emission — so `0.0` switches
    /// the object's emission off and `1.0` leaves it untouched.
    #[must_use]
    pub const fn emission_scale(&self) -> f32 {
        self.emission_scale
    }

    /// Linear colour preset independent of illumination, alpha and emission.
    #[must_use]
    pub const fn albedo_tint(&self) -> [f32; 3] {
        self.albedo_tint
    }

    /// Selects the runtime emission scale, returning whether it was accepted.
    ///
    /// A finite, non-negative scale is stored; any other value is refused and
    /// the previous scale is kept, because a malformed multiplier would
    /// otherwise reach the emission uniform as a NaN or an inverted colour.
    pub fn set_emission_scale(&mut self, scale: f32) -> bool {
        if !scale.is_finite() || scale < 0.0 {
            return false;
        }
        self.emission_scale = scale;
        true
    }

    /// The emission a primitive draws with: the object override when set,
    /// otherwise the primitive's own material emission, multiplied by the
    /// runtime [`Self::emission_scale`].
    ///
    /// The default scale of `1.0` leaves the resolved emission arithmetically
    /// unchanged (a multiply by one is exact), so an object that never
    /// selected a variant draws exactly what it always did.
    #[must_use]
    pub fn emission_for(&self, primitive: MaterialEmission) -> MaterialEmission {
        let emission = self.emission.unwrap_or(primitive);
        MaterialEmission {
            intensity: emission.intensity * self.emission_scale,
            ..emission
        }
    }

    /// [`Self::emission_for`] for one of this object's primitives.
    #[must_use]
    pub fn submesh_emission(&self, submesh: &DynamicSubmesh) -> MaterialEmission {
        self.emission_for(submesh.emission)
    }

    /// The authored float motion, or `None` for an ordinary dynamic object.
    #[must_use]
    pub const fn float(&self) -> Option<&FloatMotion> {
        self.float.as_ref()
    }

    /// The cached baked-light probe.
    #[must_use]
    pub const fn light_scale(&self) -> [f32; 3] {
        self.light_scale
    }

    /// Linear probe payload and fallback reason for the material uniform.
    #[must_use]
    pub const fn entity_lighting(&self) -> Option<super::light_transport::EntityLighting> {
        self.entity_lighting
    }

    /// Spatial irradiance/visibility, refreshed only when its inputs change.
    #[must_use]
    pub fn spatial_lighting(&self) -> Option<&super::light_transport::EntitySpatialLighting> {
        self.spatial_lighting.as_deref()
    }

    /// Captures live pose and actual sampled lighting for explicit diagnostics.
    /// This allocates JSON only when called; it performs no sampling or upload.
    #[must_use]
    pub fn diagnostic_lighting_snapshot(&self) -> serde_json::Value {
        let model = self.transform();
        let normal_model = super::normal::inverse_transpose(model).unwrap_or(Mat4::IDENTITY);
        let [normal_x, normal_y, normal_z, _translation] = normal_model.to_cols_array_2d();
        let bounds = self.world_bounds();
        let sampled_model = self.spatial_key.map_or(
            model,
            super::light_transport::EntitySpatialKey::diagnostic_model,
        );
        let vertex_normals: Vec<_> = self
            .mesh
            .vertices
            .iter()
            .take(8)
            .map(|vertex| {
                serde_json::json!({
                    "local": vertex.normal,
                    "world": normal_model.transform_vector3(Vec3::from_array(vertex.normal))
                        .normalize_or_zero().to_array(),
                })
            })
            .collect();
        serde_json::json!({
            "model_path": self.model_path(),
            "live": {
                "visible": self.visible,
                "position": self.translation(),
                "orientation": model.to_scale_rotation_translation().1.to_array(),
                "scale": self.scale,
                "model": model.to_cols_array_2d(),
                "normal_model": [normal_x, normal_y, normal_z],
                "world_bounds": {"min": bounds.min, "max": bounds.max},
                "centre": self.centre().to_array(),
                "vertex_normals": vertex_normals,
            },
            "sample": {
                "probe_position": self.probe_position,
                "probe_valid": self.probe_valid,
                "source": self.entity_lighting.map(|sample| format!("{:?}", sample.source)),
                "hdr": self.entity_lighting.map(|sample| sample.display),
                "prepared": self.entity_lighting.and_then(|sample| sample.prepared).map(|texel| {
                    serde_json::json!({"irradiance": texel.irradiance, "direction": texel.direction})
                }),
                "spatial": self.spatial_lighting().map(|spatial| serde_json::json!({
                    "bounds_min": spatial.bounds_min,
                    "bounds_extent": spatial.bounds_extent,
                    "anchors": spatial.anchors,
                    "direct": spatial.direct,
                    "world_anchors": spatial.diagnostic_world_anchors(sampled_model),
                })),
                "cache": self.spatial_key.map(super::light_transport::EntitySpatialKey::diagnostic_snapshot),
                "ray_cache": self.static_rays.diagnostic_snapshot(),
                "spatial_payload_identity": self.spatial_lighting().map(|payload| std::ptr::from_ref(payload).addr()),
            },
        })
    }

    /// True when [`Self::light_scale`] has been sampled from the bake.
    #[must_use]
    pub const fn probe_valid(&self) -> bool {
        self.probe_valid
    }

    /// World-space centre of the object's bounds: its probe point.
    #[must_use]
    pub fn centre(&self) -> Vec3 {
        self.placement().centre
    }

    /// World-space AABB of the object's mesh, for frustum culling and tests.
    #[must_use]
    pub fn world_bounds(&self) -> Aabb {
        self.placement().bounds
    }

    /// Random access to one object's own mesh-space vertices, for tests.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertex_count()
    }

    /// Only this receiver's output changes; the current visibility snapshot is
    /// complete before the batch starts. The result counts changed GPU probes.
    fn refresh_lighting(
        &mut self,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        spatial: &super::light_transport::EntitySpatialLightingContext<'_>,
        lighting_revision: Option<u64>,
    ) -> bool {
        let plan = std::mem::take(&mut self.lighting_refresh_plan);
        if !plan.needed() {
            return false;
        }
        #[cfg(feature = "visual-diagnostics")]
        let refresh_started = std::time::Instant::now();
        let probe = self.centre().to_array();
        let mut changed = false;
        if plan.center {
            let sample = super::light_transport::entity_lighting(lighting, irradiance, probe);
            changed = self.entity_lighting != Some(sample)
                || self.probe_position.map(f32::to_bits) != probe.map(f32::to_bits);
            self.light_scale = sample.display;
            self.entity_lighting = Some(sample);
            self.center_light_key = self.lighting_refresh_key.map(|key| key.center);
            self.probe_position = probe;
            self.probe_valid = true;
        }
        if plan.spatial {
            let transform = self.transform();
            let spatial_key = spatial.key(transform, Some(self.id));
            if self.spatial_key != spatial_key || self.lighting_refresh_key.is_some() {
                self.spatial_lighting = spatial.sample(
                    self.mesh.bounds,
                    transform,
                    Some(self.id),
                    spatial_key,
                    lighting_revision.map(|revision| (&mut self.static_rays, revision)),
                );
                self.spatial_key = spatial_key;
            }
        }
        #[cfg(feature = "visual-diagnostics")]
        self.static_rays.record_refresh(refresh_started);
        changed
    }
}

/// What one [`DynamicScene::update`] pass did, for the developer log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicUpdate {
    /// Objects whose spin advanced their transform this pass.
    pub moved: usize,
    /// Objects whose baked-light probe was re-sampled this pass.
    pub probes_refreshed: usize,
}

/// Bounded, deterministic collection of dynamic objects for one level.
///
/// Order is spawn order and stays stable: despawning preserves the relative
/// order of the survivors, so the draw list is reproducible for a given
/// sequence of spawns. Nothing here allocates while updating.
#[derive(Debug, Default)]
pub struct DynamicScene {
    objects: Vec<DynamicObject>,
    meshes: Vec<Arc<DynamicMesh>>,
    mesh_index_by_path: HashMap<String, usize>,
    /// Explicit registration returns a stable index that a caller may retain.
    /// Asset-cache eviction must never recycle one of these reserved slots.
    registered_meshes: Vec<usize>,
    /// Bumped on every structural change (spawn, despawn, clear, mesh
    /// registration). The renderer compares it to decide when to re-upload.
    revision: u64,
    next_id: u32,
    /// The water volumes floating props ride, resolved once when they spawn.
    /// Empty for a scene with no floats.
    water: WaterVolumes,
}

impl DynamicScene {
    /// An empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live objects.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.objects.len()
    }

    /// True when no object is live.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The live objects, in stable spawn order.
    #[must_use]
    pub fn objects(&self) -> &[DynamicObject] {
        &self.objects
    }

    /// The distinct meshes this scene draws, in first-spawn order.
    #[must_use]
    pub fn meshes(&self) -> &[Arc<DynamicMesh>] {
        &self.meshes
    }

    /// Number of distinct meshes; the renderer keeps one GPU buffer per mesh.
    #[must_use]
    pub const fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Draw calls the whole scene submits: one per object per primitive.
    #[must_use]
    pub fn draw_count(&self) -> usize {
        self.objects
            .iter()
            .filter(|object| object.visible)
            .map(|object| object.mesh.draw_count())
            .sum()
    }

    /// Distinct vertices the whole scene draws, summed over live objects.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.objects
            .iter()
            .filter(|object| object.visible)
            .map(|object| object.mesh.vertex_count())
            .sum()
    }

    /// Structural revision, bumped whenever the mesh list or object list
    /// changes. The renderer re-uploads only when this differs from the
    /// revision it last uploaded.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// One object by handle.
    #[must_use]
    pub fn get(&self, id: DynamicId) -> Option<&DynamicObject> {
        self.objects.iter().find(|object| object.id == id)
    }

    /// Empties the scene. Meshes stay registered: a level's dynamic set is
    /// usually re-spawned from the same models.
    pub fn clear(&mut self) {
        if self.objects.is_empty() {
            return;
        }
        self.objects.clear();
        self.bump();
    }

    /// Drops every registered mesh too. Called when a level is replaced.
    pub fn clear_all(&mut self) {
        self.objects.clear();
        self.meshes.clear();
        self.mesh_index_by_path.clear();
        self.registered_meshes.clear();
        self.water = WaterVolumes::new();
        self.bump();
    }

    /// Number of live floating objects.
    #[must_use]
    pub fn float_count(&self) -> usize {
        self.objects
            .iter()
            .filter(|object| object.float.is_some())
            .count()
    }

    /// Removes every floating object, keeping ordinary dynamic objects.
    ///
    /// Spawning floats is idempotent: a re-run clears the previous set first,
    /// so a level reload or a demonstration respawn cannot duplicate them.
    pub fn clear_floats(&mut self) {
        let before = self.objects.len();
        self.objects.retain(|object| object.float.is_none());
        self.water = WaterVolumes::new();
        if self.objects.len() != before {
            self.bump();
        }
    }

    /// Spawns one floating object per placed prop that authors `float`.
    ///
    /// The model resolves through the shared [`PropAssets`] cache, so a duck
    /// model is decoded once for every placement. A prop whose model cannot
    /// resolve is reported once and skipped, exactly like the static path's
    /// fallback; validation makes that unreachable for a shipped level.
    /// Returns the number of floats spawned.
    pub fn spawn_floating_props(
        &mut self,
        level: &LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
        assets: &mut PropAssets,
    ) -> usize {
        let water = WaterVolumes::from_level(level);
        let surfaces = LevelSurfaces::new(level);
        let mut spawned = 0usize;
        for (index, prop) in level.props.iter().enumerate() {
            let Some(float) = prop.float.as_ref() else {
                continue;
            };
            if !prop.x.is_finite() || !prop.y.is_finite() || !prop.z.is_finite() {
                continue;
            }
            let Some(model_path) = catalog
                .get(&prop.model)
                .model
                .filter(|path| !path.is_empty())
            else {
                continue;
            };
            let asset = match assets.resolve(&model_path) {
                Ok(asset) => asset,
                Err(error) => {
                    assets.report_failure(&model_path, &error);
                    continue;
                }
            };
            let dry_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            let phase = float.phase.map_or_else(
                || {
                    // Deterministic per-placement phase: consecutive floats
                    // land on different points of the cycle with no random
                    // source and no cross-instance state.
                    let seed = u16::try_from(index).unwrap_or(u16::MAX);
                    (f32::from(seed) * 0.618_034_f32).fract() * std::f32::consts::TAU
                },
                |authored| authored * std::f32::consts::TAU,
            );
            let motion = FloatMotion {
                anchor: Vec3::new(prop.x, 0.0, prop.z),
                dry_y,
                draft: float.draft,
                bob_amplitude: float.bob,
                bob_rate: std::f32::consts::TAU / float.bob_seconds,
                heel_amplitude: float.heel_degrees,
                heel_rate: std::f32::consts::TAU / float.heel_seconds,
                phase,
            };
            if self
                .spawn_floating(&asset, motion, &water, prop.rotation_degrees, prop.scale)
                .is_some()
            {
                spawned = spawned.saturating_add(1);
            }
        }
        if spawned > 0 {
            self.water = water;
        }
        spawned
    }

    /// Advances every floating object to `seconds` on its authored cycle.
    ///
    /// `seconds` is an absolute animation clock, not a per-frame delta: the
    /// pose is a pure function of time, so the motion cannot drift, cannot
    /// accumulate error and is identical at any frame rate. Returns how many
    /// objects moved.
    pub fn update_floats(&mut self, seconds: f32) -> usize {
        if self.water.is_empty() {
            return 0;
        }
        let animation_seconds = if seconds.is_finite() { seconds } else { 0.0 };
        let water = &self.water;
        let mut moved = 0usize;
        for object in &mut self.objects {
            let Some(float) = object.float else {
                continue;
            };
            let translation = float.translation_at(animation_seconds, water);
            let heel = float.heel_at(animation_seconds);
            let changed = (object.translation.x - translation.x).abs() > f32::EPSILON
                || (object.translation.y - translation.y).abs() > f32::EPSILON
                || (object.translation.z - translation.z).abs() > f32::EPSILON
                || (object.spin_degrees - heel).abs() > f32::EPSILON;
            object.set_placement(translation, object.base_rotation, heel, object.scale);
            if changed {
                moved = moved.saturating_add(1);
            }
        }
        moved
    }

    const fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Returns the shared mesh for `asset`, building it on first use.
    ///
    /// Registration is keyed by the model path, so two objects using the same
    /// model share one mesh — and one GPU buffer — even when they were resolved
    /// separately.
    fn mesh_for(&mut self, asset: &Arc<LoadedPropAsset>) -> Option<usize> {
        if let Some(index) = self.mesh_index_by_path.get(&asset.model_path).copied() {
            return Some(index);
        }
        let mesh = Arc::new(DynamicMesh::from_asset(asset)?);
        self.insert_mesh(mesh, false)
    }

    /// Reuses only an unreferenced asset-cache slot before growing the table.
    /// Object mesh indices and explicit registration handles remain stable.
    fn insert_mesh(&mut self, mesh: Arc<DynamicMesh>, registered: bool) -> Option<usize> {
        let reusable = self.meshes.iter().enumerate().find_map(|(slot, _)| {
            (!self.registered_meshes.contains(&slot)
                && !self.objects.iter().any(|object| object.mesh_index == slot))
            .then_some(slot)
        });
        let index = reusable
            .or_else(|| (self.meshes.len() < MAX_DYNAMIC_MESHES).then_some(self.meshes.len()))?;
        let counts = self
            .meshes
            .iter()
            .enumerate()
            .filter_map(|(slot, current)| {
                (slot != index).then_some((current.vertices.len(), current.indices.len()))
            })
            .chain(std::iter::once((mesh.vertices.len(), mesh.indices.len())));
        if let Err(reason) = aggregate_geometry_admitted(counts) {
            crate::engine::logging::warn_once(
                format!("dynamic-mesh-budget:{}", mesh.model_path),
                format!("[dynamic] cannot register `{}`: {reason}", mesh.model_path),
            );
            return None;
        }
        let path = mesh.model_path.clone();
        if let Some(previous) = self.meshes.get_mut(index) {
            let _old_index = self.mesh_index_by_path.remove(&previous.model_path);
            *previous = mesh;
        } else {
            self.meshes.push(mesh);
        }
        let _previous_value = self.mesh_index_by_path.insert(path, index);
        if registered {
            self.registered_meshes.push(index);
        }
        self.bump();
        Some(index)
    }

    /// Registers a code-built model under `key`, returning its mesh index.
    ///
    /// Registration is idempotent: an already-registered key returns the
    /// existing mesh, so respawning a level's doors after a quality rebuild
    /// shares the geometry instead of duplicating it. `alphas[i]` is submesh
    /// `i`'s alpha contract.
    pub fn register_model(
        &mut self,
        key: &str,
        model: &PropModel,
        textures: Vec<Arc<crate::engine::loader::RawImage>>,
        alphas: &[MaterialAlpha],
    ) -> Option<usize> {
        if let Some(index) = self.mesh_index_by_path.get(key).copied() {
            if !self.registered_meshes.contains(&index) {
                self.registered_meshes.push(index);
            }
            return Some(index);
        }
        let mesh = Arc::new(DynamicMesh::from_model(
            key.to_string(),
            model,
            textures,
            alphas,
        )?);
        self.insert_mesh(mesh, true)
    }

    /// Spawns one object that draws an already-registered mesh.
    ///
    /// The same transform contract as [`Self::spawn_oriented`], but keyed by a
    /// mesh index from [`Self::register_model`] instead of a resolved asset.
    #[must_use]
    pub fn spawn_registered(
        &mut self,
        mesh_index: usize,
        translation: [f32; 3],
        orientation: SpawnOrientation,
        spin_degrees: f32,
        scale: f32,
    ) -> Option<DynamicId> {
        if self.objects.len() >= MAX_DYNAMIC_OBJECTS
            || !translation.iter().all(|value| value.is_finite())
            || !orientation.base_rotation.is_finite()
            || !orientation.spin_axis.iter().all(|value| value.is_finite())
            || !spin_degrees.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
        {
            return None;
        }
        let axis = Vec3::from(orientation.spin_axis);
        if axis.length() < SPIN_AXIS_EPSILON {
            return None;
        }
        let mesh = self.meshes.get(mesh_index)?.clone();
        let id = DynamicId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.objects.push(DynamicObject {
            id,
            visible: true,
            mesh_index,
            mesh,
            placement: std::sync::OnceLock::new(),
            translation: Vec3::from(translation),
            base_rotation: orientation.base_rotation,
            spin_axis: axis.normalize(),
            spin_degrees,
            scale,
            spin_degrees_per_second: 0.0,
            emission: None,
            emission_scale: 1.0,
            albedo_tint: [1.0; 3],
            light_scale: [crate::engine::lighting::AMBIENT_LEVEL; 3],
            entity_lighting: None,
            center_light_key: None,
            lighting_refresh_key: None,
            lighting_refresh_plan: EntityLightingRefreshPlan::default(),
            spatial_lighting: None,
            spatial_key: None,
            static_rays: super::light_transport::EntityStaticRayCache::default(),
            probe_position: [f32::NAN; 3],
            probe_valid: false,
            float: None,
        });
        self.bump();
        Some(id)
    }

    /// Spawns one yaw-only dynamic object and returns its handle.
    ///
    /// The terse yaw-only spawn: identity base orientation, spin about the
    /// model's Y axis. See [`Self::spawn_oriented`] for the general case.
    #[must_use]
    pub fn spawn(
        &mut self,
        asset: &Arc<LoadedPropAsset>,
        translation: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
        spin_degrees_per_second: f32,
    ) -> Option<DynamicId> {
        self.spawn_oriented(
            asset,
            translation,
            SpawnOrientation::YAW_ONLY,
            yaw_degrees,
            scale,
            spin_degrees_per_second,
        )
    }

    /// Spawns one dynamic object with an explicit [`SpawnOrientation`], and
    /// returns its handle.
    ///
    /// The orientation's `base_rotation` is applied after the spin (translate
    /// × base × spin × scale), so a model built mouth-up can be pitched onto a
    /// horizontal axis; its `spin_axis` is the model-space axis the object
    /// turns about and is normalised on spawn. `spin_degrees` is the initial
    /// angle about that axis.
    ///
    /// Returns `None` when the scene is full, when the model has nothing
    /// drawable, when the mesh budget is exhausted, or when any transform
    /// value is not finite — a malformed spawn degrades to "nothing spawned"
    /// instead of poisoning the draw list with NaN.
    #[must_use]
    pub fn spawn_oriented(
        &mut self,
        asset: &Arc<LoadedPropAsset>,
        translation: [f32; 3],
        orientation: SpawnOrientation,
        spin_degrees: f32,
        scale: f32,
        spin_degrees_per_second: f32,
    ) -> Option<DynamicId> {
        let SpawnOrientation {
            base_rotation,
            spin_axis,
        } = orientation;
        if self.objects.len() >= MAX_DYNAMIC_OBJECTS
            || !translation.iter().all(|value| value.is_finite())
            || !base_rotation.is_finite()
            || !spin_axis.iter().all(|value| value.is_finite())
            || !spin_degrees.is_finite()
            || !scale.is_finite()
            || !spin_degrees_per_second.is_finite()
            || scale <= 0.0
        {
            return None;
        }
        let axis = Vec3::from(spin_axis);
        if axis.length() < SPIN_AXIS_EPSILON {
            return None;
        }
        let mesh_index = self.mesh_for(asset)?;
        let mesh = self.meshes.get(mesh_index)?.clone();
        let id = DynamicId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.objects.push(DynamicObject {
            id,
            visible: true,
            mesh_index,
            mesh,
            placement: std::sync::OnceLock::new(),
            translation: Vec3::from(translation),
            base_rotation,
            spin_axis: axis.normalize(),
            spin_degrees,
            scale,
            spin_degrees_per_second,
            emission: None,
            emission_scale: 1.0,
            albedo_tint: [1.0; 3],
            light_scale: [crate::engine::lighting::AMBIENT_LEVEL; 3],
            entity_lighting: None,
            center_light_key: None,
            lighting_refresh_key: None,
            lighting_refresh_plan: EntityLightingRefreshPlan::default(),
            spatial_lighting: None,
            spatial_key: None,
            static_rays: super::light_transport::EntityStaticRayCache::default(),
            probe_position: [f32::NAN; 3],
            probe_valid: false,
            float: None,
        });
        self.bump();
        Some(id)
    }

    /// Spawns one floating prop at its water surface.
    ///
    /// The object is a normal dynamic object (shared mesh, transform-only
    /// updates) with a [`FloatMotion`]: `base_rotation` aims the model by its
    /// authored yaw, the spin axis is the model's local Z (its forward axis),
    /// and the spin angle is the authored heel. Returns the handle, or `None`
    /// when the scene is full or the model has nothing drawable.
    #[must_use]
    pub fn spawn_floating(
        &mut self,
        asset: &Arc<LoadedPropAsset>,
        motion: FloatMotion,
        water: &WaterVolumes,
        yaw_degrees: f32,
        scale: f32,
    ) -> Option<DynamicId> {
        let translation = motion.translation_at(0.0, water);
        let id = self.spawn_oriented(
            asset,
            [translation.x, translation.y, translation.z],
            SpawnOrientation {
                base_rotation: Quat::from_rotation_y(yaw_degrees.to_radians()),
                spin_axis: [0.0, 0.0, 1.0],
            },
            motion.heel_at(0.0),
            scale,
            0.0,
        )?;
        if let Some(object) = self.objects.iter_mut().find(|object| object.id == id) {
            object.float = Some(motion);
        }
        Some(id)
    }

    /// Removes one object. Returns whether the handle was live; the surviving
    /// objects keep their relative order.
    pub fn despawn(&mut self, id: DynamicId) -> bool {
        let Some(index) = self.objects.iter().position(|object| object.id == id) else {
            return false;
        };
        drop(self.objects.remove(index));
        self.bump();
        true
    }

    /// Sets an object's transform (translation, spin about its own axis,
    /// uniform scale). The base orientation and spin axis are not changed here.
    ///
    /// Returns `false` for an unknown handle or a non-finite/negative-scale
    /// value. The probe is not re-sampled here: [`Self::update`] owns that, so
    /// a frame that sets ten transforms costs at most ten probes, not twenty
    /// samples.
    pub fn set_transform(
        &mut self,
        id: DynamicId,
        translation: [f32; 3],
        spin_degrees: f32,
        scale: f32,
    ) -> bool {
        if !translation.iter().all(|value| value.is_finite())
            || !spin_degrees.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
        {
            return false;
        }
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.set_placement(
            Vec3::from(translation),
            object.base_rotation,
            spin_degrees,
            scale,
        );
        true
    }

    /// Sets an externally simulated rigid pose, preserving scale and materials.
    ///
    /// `orientation` is a quaternion in `[x, y, z, w]` order. Finite nonzero
    /// quaternions are normalized; invalid positions, zero/non-finite rotations
    /// and unknown handles leave the object unchanged. The absolute pose stops
    /// the object's procedural spin and floating motion. Bounds, visibility
    /// and lighting use this same transform during the next update, without
    /// rebuilding the mesh or changing the scene's geometry revision.
    pub fn set_pose(
        &mut self,
        id: DynamicId,
        translation: [f32; 3],
        orientation: [f32; 4],
    ) -> bool {
        if !translation.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(rotation) = normalized_pose_rotation(orientation) else {
            return false;
        };
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.set_placement(Vec3::from(translation), rotation, 0.0, object.scale);
        object.spin_degrees_per_second = 0.0;
        object.float = None;
        true
    }

    /// Suspends or restores an object without changing its pose or resources.
    ///
    /// Hidden objects keep their handles and count against the scene budget.
    /// Visibility and lighting are refreshed on the next update; changing this
    /// flag never bumps the geometry revision or reloads the model.
    pub fn set_visible(&mut self, id: DynamicId, visible: bool) -> bool {
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.visible = visible;
        true
    }

    /// Sets or clears an object's emission override. The override replaces
    /// every primitive's own material emission; `None` restores the model's.
    pub fn set_emission(&mut self, id: DynamicId, emission: Option<MaterialEmission>) -> bool {
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.emission = emission.map(MaterialEmission::sanitized);
        true
    }

    /// Selects an object's runtime emission scale, returning whether the
    /// object was live and the scale accepted.
    ///
    /// The scale multiplies the emission every primitive of that object
    /// resolves (see [`DynamicObject::submesh_emission`]); the default is
    /// `1.0`. A non-finite or negative scale is refused, exactly like a
    /// malformed transform.
    pub fn set_emission_scale(&mut self, id: DynamicId, scale: f32) -> bool {
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.set_emission_scale(scale)
    }

    /// Sets a bounded albedo preset while retaining the shared model buffers.
    pub fn set_albedo_tint(&mut self, id: DynamicId, tint: [f32; 3]) -> bool {
        if !tint
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return false;
        }
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.albedo_tint = tint;
        true
    }

    /// Advances spinning objects and refreshes environmental lighting.
    /// Samples allocate nothing; unchanged uniforms require no GPU write.
    pub fn update(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
    ) -> DynamicUpdate {
        self.update_with_field(delta_seconds, lighting, None)
    }

    /// [`Self::update`] preferring the prepared HDR irradiance field.
    /// Valid darkness is preserved. Missing, invalid or roomless samples use
    /// the existing authored environment response with a diagnostic reason.
    pub fn update_with_field(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
    ) -> DynamicUpdate {
        self.update_with_field_and_scene(delta_seconds, lighting, irradiance, None)
    }

    /// Refresh using the prepared static triangle/alpha visibility resource.
    /// Construction and baking stay outside the per-frame update.
    pub fn update_with_field_and_scene(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
    ) -> DynamicUpdate {
        let moved = self.advance_spins(delta_seconds);
        let mut update = self.refresh_entity_lighting(lighting, irradiance, scene, None, None);
        update.moved = moved;
        update
    }

    /// Advance every rigid transform before refreshing the movable visibility
    /// resource and any receiver lighting. Provider errors stay explicit.
    ///
    /// # Errors
    /// Returns an error when a movable mesh's ray resource or transform cannot
    /// be prepared; no lighting refresh uses an incomplete visibility snapshot.
    pub fn update_with_visibility(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&mut super::dynamic_visibility::DynamicVisibility>,
    ) -> Result<DynamicUpdate, String> {
        self.update_with_visibility_revision(
            delta_seconds,
            lighting,
            irradiance,
            scene,
            visibility,
            None,
        )
    }

    /// Uses a caller-owned lighting revision to reuse unchanged center probes.
    /// Increment it whenever lighting or field contents change, including
    /// fixture switches and in-place edits. Movable-caster changes refresh the
    /// spatial payload independently and do not invalidate the center probe.
    ///
    /// # Errors
    /// Returns the same movable-visibility preparation errors as
    /// [`Self::update_with_visibility`].
    pub fn update_with_visibility_and_lighting_revision(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&mut super::dynamic_visibility::DynamicVisibility>,
        lighting_revision: u64,
    ) -> Result<DynamicUpdate, String> {
        self.update_with_visibility_revision(
            delta_seconds,
            lighting,
            irradiance,
            scene,
            visibility,
            Some(lighting_revision),
        )
    }

    fn update_with_visibility_revision(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&mut super::dynamic_visibility::DynamicVisibility>,
        lighting_revision: Option<u64>,
    ) -> Result<DynamicUpdate, String> {
        let moved = self.advance_spins(delta_seconds);
        let current = if let Some(provider) = visibility {
            let _visibility_changed = provider.sync(self)?;
            Some(&*provider)
        } else {
            None
        };
        let mut update =
            self.refresh_entity_lighting(lighting, irradiance, scene, current, lighting_revision);
        update.moved = moved;
        Ok(update)
    }

    fn advance_spins(&mut self, delta_seconds: f32) -> usize {
        let step = if delta_seconds.is_finite() {
            delta_seconds.max(0.0)
        } else {
            0.0
        };
        let mut moved = 0_usize;
        for object in &mut self.objects {
            if object.spin_degrees_per_second != 0.0 && step > 0.0 {
                let advanced = step * object.spin_degrees_per_second;
                let spin_degrees = (object.spin_degrees + advanced).rem_euclid(360.0);
                object.set_placement(
                    object.translation,
                    object.base_rotation,
                    spin_degrees,
                    object.scale,
                );
                moved = moved.saturating_add(1);
            }
        }
        moved
    }

    /// Fixture switches change energy, never immutable visibility geometry.
    pub fn retain_static_visibility_on_source_change(&mut self, previous: u64, revision: u64) {
        for object in &mut self.objects {
            object
                .static_rays
                .advance_source_revision(previous, revision);
        }
    }

    fn refresh_entity_lighting(
        &mut self,
        lighting: Option<&LevelLighting>,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&super::dynamic_visibility::DynamicVisibility>,
        lighting_revision: Option<u64>,
    ) -> DynamicUpdate {
        let Some(level_lighting) = lighting else {
            return DynamicUpdate::default();
        };
        let context = EntityLightingRefreshContext::new(
            level_lighting,
            irradiance,
            scene,
            visibility,
            lighting_revision,
        );
        let mut dirty_count = 0_usize;
        for object in &mut self.objects {
            object.lighting_refresh_plan = EntityLightingRefreshPlan::default();
            if !object.visible {
                continue;
            }
            let probe = object.centre().to_array();
            if !probe.iter().all(|value| value.is_finite()) {
                continue;
            }
            let key = context.key(
                object.transform(),
                object.mesh.bounds,
                probe,
                Some(object.id),
                object.lighting_refresh_key,
                Some(&object.static_rays),
            );
            object.lighting_refresh_plan = EntityLightingRefreshPlan::new(
                key,
                object.lighting_refresh_key,
                object.center_light_key,
            );
            // Consume observed visibility revisions even when all payloads stay
            // unchanged, so a later frame does not unnecessarily miss history.
            object.lighting_refresh_key = key;
            if object.lighting_refresh_plan.needed() {
                dirty_count = dirty_count.saturating_add(1);
            }
        }
        if dirty_count == 0 {
            return DynamicUpdate::default();
        }
        let min_receivers = if dirty_count >= PARALLEL_LIGHTING_MIN_RECEIVERS {
            PARALLEL_LIGHTING_MIN_RECEIVERS
        } else {
            usize::MAX
        };
        let spatial = super::light_transport::EntitySpatialLightingContext::new(
            level_lighting,
            irradiance,
            scene,
            visibility,
        );
        let refreshed = std::sync::atomic::AtomicUsize::new(0);
        crate::engine::runtime_work::for_each_mut(&mut self.objects, min_receivers, |object| {
            if object.refresh_lighting(level_lighting, irradiance, &spatial, lighting_revision) {
                let _previous_count = refreshed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
        DynamicUpdate {
            probes_refreshed: refreshed.into_inner(),
            ..DynamicUpdate::default()
        }
    }

    /// Registers the dynamic drum inside every placed washing machine.
    ///
    /// The washer-drum demonstration, kept deliberately small: the machine
    /// itself stays an ordinary static level prop (baked, occluding,
    /// collidable) and only the drum is dynamic. Placement is derived, not
    /// authored: the drum's axis is the machine's front axis through the
    /// porthole centre, its mouth is recessed behind the front panel so it
    /// never protrudes, and its scale keeps the basket just inside the
    /// opening. Moving or turning the machine moves the drum with no
    /// level-schema change. Returns how many objects were
    /// spawned.
    pub fn spawn_washer_drum_demo(
        &mut self,
        level: &LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
        assets: &mut PropAssets,
    ) -> usize {
        let machine_path = catalog
            .get(DEMO_MACHINE_ID)
            .model
            .filter(|path| !path.is_empty());
        let drum_path = catalog
            .get(DEMO_DRUM_ID)
            .model
            .filter(|path| !path.is_empty());
        let (Some(resolved_machine_path), Some(resolved_drum_path)) = (machine_path, drum_path)
        else {
            return 0;
        };
        let drum_asset = match assets.resolve(&resolved_drum_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&resolved_drum_path, &error);
                return 0;
            }
        };
        let drum_radius = catalog.get(DEMO_DRUM_ID).size[0].max(0.0) * 0.5;
        let drum_length = catalog.get(DEMO_DRUM_ID).size[1].max(0.0);
        let machine_depth = catalog.get(DEMO_MACHINE_ID).size[2].max(0.0);
        // The drum is fitted to the porthole opening; a placed machine's own
        // scale multiplies both below, so a scaled machine keeps a
        // proportionally scaled drum.
        let unit_drum_scale = if drum_radius > 0.0 {
            (WASHER_PORTHOLE_OPENING_RADIUS - WASHER_DRUM_PORTHOLE_CLEARANCE).max(0.0) / drum_radius
        } else {
            0.0
        };
        if unit_drum_scale <= 0.0 {
            return 0;
        }
        let surfaces = LevelSurfaces::new(level);
        let mut spawned = 0usize;
        for prop in &level.props {
            if prop.model != DEMO_MACHINE_ID {
                continue;
            }
            // Only a placement that really resolves to the machine model gets a
            // drum: a level whose catalog maps the id elsewhere must not spawn
            // one against an unrelated model.
            match catalog.get(&prop.model).model {
                Some(path) if path == resolved_machine_path => {}
                _ => continue,
            }
            if !prop.x.is_finite()
                || !prop.y.is_finite()
                || !prop.z.is_finite()
                || !prop.rotation_degrees.is_finite()
                || !prop.pitch_degrees.is_finite()
                || !prop.roll_degrees.is_finite()
                || !prop.scale.is_finite()
                || prop.scale <= 0.0
            {
                continue;
            }
            let machine_scale = prop.scale;
            let drum_scale = unit_drum_scale * machine_scale;
            let panel_from_centre = machine_depth.mul_add(0.5, -WASHER_PORTHOLE_BEZEL_DEPTH);
            let local_z = drum_length.mul_add(
                -unit_drum_scale,
                panel_from_centre - WASHER_DRUM_MOUTH_RECESS,
            );
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let position = prop.placement_matrix(base_y).transform_point3(Vec3::new(
                0.0,
                WASHER_PORTHOLE_CENTRE_Y,
                local_z,
            ));
            let orientation = SpawnOrientation {
                base_rotation: prop
                    .orientation()
                    .mul_quat(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                spin_axis: MODEL_Y_AXIS,
            };
            let id = self.spawn_oriented(
                &drum_asset,
                position.to_array(),
                orientation,
                0.0,
                drum_scale,
                DEMO_SPIN_DEGREES_PER_SECOND,
            );
            if id.is_some() {
                spawned = spawned.saturating_add(1);
            }
        }
        spawned
    }
}

#[cfg(test)]
#[path = "../../tests/dynamic_mesh_admission.rs"]
mod tests;
