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
//! authored in the level JSON          spawned by game code, never in the level
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
//! [`crate::props::LoadedPropAsset`] the static prop path resolves, converted
//! once into render vertices in **model space**; its textures are the same
//! `Rc`-shared decoded images, so spawning a drum that is also placed statically
//! decodes nothing twice.
//!
//! A transform is translation + yaw about Y + uniform scale. It is deliberately
//! not a general matrix: a dynamic object is a rigid prop-like thing, and
//! keeping the rotation to one axis keeps the demo, the probe maths and the
//! tests exact. The renderer composes `view_projection * transform` into the
//! existing `u_mvp` uniform per object, so moving an object costs one uniform
//! upload and never touches a vertex buffer.
//!
//! Lighting (probe-based)
//! ----------------------
//! Baked static light cannot follow a moving object, so each object carries a
//! **probe**: the baked [`LevelLighting::sample`] value at its world-space
//! centre, refreshed only when the object has moved by more than
//! [`PROBE_EPSILON_M`]. The renderer feeds the probe through the world
//! program's `u_light_scale` uniform, and the object's per-vertex colour carries
//! albedo/tint only — exactly like a lightmapped static vertex. The object then
//! reads coherently as it crosses a pool of light and a dark corner.
//!
//! Documented limits of that model:
//!
//! * **One probe per object.** The whole object is lit uniformly; a long object
//!   lying across a light/dark boundary cannot shade across its length.
//! * **No self-occlusion and no shadows.** The probe is the room's baked light
//!   at the object's centre, so the object does not darken the floor beneath it,
//!   does not shadow itself and casts no shadow of any kind.
//! * **Static light only.** The probe reads the level's bake; light emitted by
//!   other dynamic objects, or by anything that moves, never contributes.
//! * **The probe is a snapshot.** Because it is refreshed on movement, a light
//!   change (there is none at runtime today) would not be picked up until the
//!   object moved.

use std::collections::HashMap;
use std::rc::Rc;

use glam::{Mat4, Vec3};

use super::mesh::LIGHTMAP_NONE;
use super::{LevelDef, Vertex};
use crate::level::LevelSurfaces;
use crate::lighting::LevelLighting;
use crate::materials::MaterialEmission;
use crate::props::{LoadedPropAsset, PropAssets};
use crate::spatial::Aabb;

/// Most dynamic objects one scene holds.
///
/// Deliberately small: the draw path is one draw call per object per material,
/// so a level with hundreds of dynamic objects would cost more than the static
/// batches it is meant to sit beside. The limit is enforced on spawn, and the
/// scene never allocates on the update path.
pub const MAX_DYNAMIC_OBJECTS: usize = 64;

/// Most distinct dynamic meshes one scene holds, each getting its own vertex
/// buffer. Matches the static prop path's practical model budget.
pub const MAX_DYNAMIC_MESHES: usize = 16;

/// Smallest world-space movement, in metres, that refreshes an object's light
/// probe. A rotation about the object's own centre moves no point of the probe,
/// so a spinning object re-probes exactly never.
pub const PROBE_EPSILON_M: f32 = 0.05;

/// Degrees per second the demonstration drum turns.
pub const DEMO_SPIN_DEGREES_PER_SECOND: f32 = 12.0;

/// Catalogue id of the static prop the washer-drum demonstration is built around.
pub const DEMO_MACHINE_ID: &str = "core:washing_machine";

/// Catalogue id of the dynamic drum the demonstration spawns.
pub const DEMO_DRUM_ID: &str = "core:washer_drum";

/// Stable handle to one object inside a [`DynamicScene`].
///
/// Handles are never reused: despawning increments the scene's id counter, so a
/// stale handle resolves to `None` instead of silently addressing a new object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynamicId(u32);

/// One primitive of a dynamic mesh: a texture slot, its emission, and its slice
/// of the model's index list.
///
/// Exactly [`crate::render::PropMeshBatch`]'s submesh shape, minus the
/// pre-transformed vertices: the renderer reuses its material/emission routing
/// unchanged, so a dynamic object never grows a second material system.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicSubmesh {
    /// Index into [`DynamicMesh::textures`], or `None` for an untextured
    /// material (drawn through the shared white sheet).
    pub texture: Option<u16>,
    /// The material's own emission.
    pub emission: MaterialEmission,
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
    pub textures: Vec<Rc<crate::loader::RawImage>>,
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
        if asset.model.submeshes.is_empty() || asset.model.indices.is_empty() {
            return None;
        }
        let vertices: Vec<Vertex> = asset
            .model
            .vertices
            .iter()
            .map(|vertex| Vertex {
                pos: vertex.pos,
                color: vertex.color,
                uv: vertex.uv,
                // A dynamic vertex is never lightmapped: the atlas channel is
                // unused and the probe uniform carries the light instead.
                lightmap: [0; 2],
                lightmap_page: LIGHTMAP_NONE,
                ..Vertex::UNLIT
            })
            .collect();
        let submeshes: Vec<DynamicSubmesh> = asset
            .model
            .submeshes
            .iter()
            .filter(|submesh| submesh.index_count > 0)
            .map(|submesh| DynamicSubmesh {
                texture: submesh.texture,
                emission: submesh.emission,
                first_index: submesh.first_index,
                index_count: submesh.index_count,
            })
            .collect();
        if submeshes.is_empty() {
            return None;
        }
        let bounds = match asset.model.bounds() {
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
            model_path: asset.model_path.clone(),
            vertices,
            indices: asset.model.indices.clone(),
            submeshes,
            textures: asset.model.textures.iter().cloned().map(Rc::new).collect(),
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

/// One live dynamic object: a shared model-space mesh plus its own transform,
/// spin and light probe.
#[derive(Debug)]
pub struct DynamicObject {
    id: DynamicId,
    mesh_index: usize,
    mesh: Rc<DynamicMesh>,
    translation: Vec3,
    yaw_degrees: f32,
    scale: f32,
    spin_degrees_per_second: f32,
    emission: Option<MaterialEmission>,
    light_scale: [f32; 3],
    probe_position: [f32; 3],
    probe_valid: bool,
}

impl DynamicObject {
    /// This object's stable handle.
    #[must_use]
    pub const fn id(&self) -> DynamicId {
        self.id
    }

    /// The shared model-space mesh this object draws.
    #[must_use]
    pub const fn mesh(&self) -> &Rc<DynamicMesh> {
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

    /// The object's world transform: translate × yaw × uniform scale.
    #[must_use]
    pub fn transform(&self) -> Mat4 {
        let rotation = Mat4::from_rotation_y(self.yaw_degrees.to_radians());
        let scale = Mat4::from_scale(Vec3::splat(self.scale));
        let translation = Mat4::from_translation(self.translation);
        // `glam` matrix multiplication is per-element `f32` arithmetic with no
        // overflow or panic path; clippy cannot see that through the operator
        // impl (the same note `prop_instance_matrix` carries).
        #[allow(clippy::arithmetic_side_effects)]
        let transform = translation * rotation * scale;
        transform
    }

    /// The transform's translation.
    #[must_use]
    pub const fn translation(&self) -> [f32; 3] {
        [self.translation.x, self.translation.y, self.translation.z]
    }

    /// The transform's yaw, in degrees.
    #[must_use]
    pub const fn yaw_degrees(&self) -> f32 {
        self.yaw_degrees
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

    /// The emission one primitive draws with: the object override when set,
    /// otherwise the primitive's own material emission.
    #[must_use]
    pub fn submesh_emission(&self, submesh: &DynamicSubmesh) -> MaterialEmission {
        self.emission.unwrap_or(submesh.emission)
    }

    /// The cached baked-light probe.
    #[must_use]
    pub const fn light_scale(&self) -> [f32; 3] {
        self.light_scale
    }

    /// True when [`Self::light_scale`] has been sampled from the bake.
    #[must_use]
    pub const fn probe_valid(&self) -> bool {
        self.probe_valid
    }

    /// World-space centre of the object's bounds: its probe point.
    #[must_use]
    pub fn centre(&self) -> Vec3 {
        self.transform()
            .transform_point3(Vec3::from(self.mesh.centre))
    }

    /// World-space AABB of the object's mesh, for frustum culling and tests.
    #[must_use]
    pub fn world_bounds(&self) -> Aabb {
        super::props::transform_bounds(&self.mesh.bounds, &self.transform())
    }

    /// Random access to one object's own mesh-space vertices, for tests.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertex_count()
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
    meshes: Vec<Rc<DynamicMesh>>,
    mesh_index_by_path: HashMap<String, usize>,
    /// Bumped on every structural change (spawn, despawn, clear, mesh
    /// registration). The renderer compares it to decide when to re-upload.
    revision: u64,
    next_id: u32,
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
    pub fn meshes(&self) -> &[Rc<DynamicMesh>] {
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
            .map(|object| object.mesh.draw_count())
            .sum()
    }

    /// Distinct vertices the whole scene draws, summed over live objects.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.objects
            .iter()
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
        self.bump();
    }

    const fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Returns the shared mesh for `asset`, building it on first use.
    ///
    /// Registration is keyed by the model path, so two objects using the same
    /// model share one mesh — and one GPU buffer — even when they were resolved
    /// separately.
    fn mesh_for(&mut self, asset: &Rc<LoadedPropAsset>) -> Option<usize> {
        if let Some(index) = self.mesh_index_by_path.get(&asset.model_path).copied() {
            return Some(index);
        }
        if self.meshes.len() >= MAX_DYNAMIC_MESHES {
            return None;
        }
        let mesh = Rc::new(DynamicMesh::from_asset(asset)?);
        self.mesh_index_by_path
            .insert(mesh.model_path.clone(), self.meshes.len());
        self.meshes.push(mesh);
        self.bump();
        self.meshes.len().checked_sub(1)
    }

    /// Spawns one dynamic object and returns its handle.
    ///
    /// Returns `None` when the scene is full, when the model has nothing
    /// drawable, when the mesh budget is exhausted, or when any transform value
    /// is not finite — a malformed spawn degrades to "nothing spawned" instead
    /// of poisoning the draw list with NaN.
    #[must_use]
    pub fn spawn(
        &mut self,
        asset: &Rc<LoadedPropAsset>,
        translation: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
        spin_degrees_per_second: f32,
    ) -> Option<DynamicId> {
        if self.objects.len() >= MAX_DYNAMIC_OBJECTS
            || !translation.iter().all(|value| value.is_finite())
            || !yaw_degrees.is_finite()
            || !scale.is_finite()
            || !spin_degrees_per_second.is_finite()
            || scale <= 0.0
        {
            return None;
        }
        let mesh_index = self.mesh_for(asset)?;
        let mesh = self.meshes.get(mesh_index)?.clone();
        let id = DynamicId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.objects.push(DynamicObject {
            id,
            mesh_index,
            mesh,
            translation: Vec3::from(translation),
            yaw_degrees,
            scale,
            spin_degrees_per_second,
            emission: None,
            light_scale: [1.0; 3],
            probe_position: [f32::NAN; 3],
            probe_valid: false,
        });
        self.bump();
        Some(id)
    }

    /// Removes one object. Returns whether the handle was live; the surviving
    /// objects keep their relative order.
    pub fn despawn(&mut self, id: DynamicId) -> bool {
        let Some(index) = self.objects.iter().position(|object| object.id == id) else {
            return false;
        };
        self.objects.remove(index);
        self.bump();
        true
    }

    /// Sets an object's transform (translation, yaw about Y, uniform scale).
    ///
    /// Returns `false` for an unknown handle or a non-finite/negative-scale
    /// value. The probe is not re-sampled here: [`Self::update`] owns that, so
    /// a frame that sets ten transforms costs at most ten probes, not twenty
    /// samples.
    pub fn set_transform(
        &mut self,
        id: DynamicId,
        translation: [f32; 3],
        yaw_degrees: f32,
        scale: f32,
    ) -> bool {
        if !translation.iter().all(|value| value.is_finite())
            || !yaw_degrees.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
        {
            return false;
        }
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.translation = Vec3::from(translation);
        object.yaw_degrees = yaw_degrees;
        object.scale = scale;
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

    /// Advances every spinning object and refreshes the light probes of the
    /// objects that moved appreciably.
    ///
    /// This is the whole per-frame update: no allocation, no geometry work and
    /// no GPU traffic — only `f32` arithmetic and, when an object crossed
    /// [`PROBE_EPSILON_M`], one [`LevelLighting::sample`] call.
    pub fn update(
        &mut self,
        delta_seconds: f32,
        lighting: Option<&LevelLighting>,
    ) -> DynamicUpdate {
        let mut update = DynamicUpdate::default();
        let step = if delta_seconds.is_finite() {
            delta_seconds.max(0.0)
        } else {
            0.0
        };
        for object in &mut self.objects {
            if object.spin_degrees_per_second != 0.0 && step > 0.0 {
                let advanced = step * object.spin_degrees_per_second;
                object.yaw_degrees = (object.yaw_degrees + advanced).rem_euclid(360.0);
                update.moved = update.moved.saturating_add(1);
            }
            let Some(lighting) = lighting else {
                continue;
            };
            let centre = object.centre();
            let probe = [centre.x, centre.y, centre.z];
            if !probe.iter().all(|value| value.is_finite()) {
                continue;
            }
            let moved = if object.probe_valid {
                let dx = probe[0] - object.probe_position[0];
                let dy = probe[1] - object.probe_position[1];
                let dz = probe[2] - object.probe_position[2];
                dx.mul_add(dx, dy.mul_add(dy, dz * dz)) > PROBE_EPSILON_M * PROBE_EPSILON_M
            } else {
                true
            };
            if !moved {
                continue;
            }
            let light = lighting.sample(probe[0], probe[1], probe[2]);
            object.light_scale = [light.r, light.g, light.b];
            object.probe_position = probe;
            object.probe_valid = true;
            update.probes_refreshed = update.probes_refreshed.saturating_add(1);
        }
        update
    }

    /// Registers the dynamic drum in front of every placed washing machine.
    ///
    /// The washer-drum demonstration, kept deliberately small: the machine itself
    /// stays an ordinary static level prop (baked, occluding, collidable) and
    /// only the loose drum is dynamic. Placement is derived, not authored — the
    /// drum stands on the floor in front of the machine's door, so moving the
    /// machine moves the drum with no level-schema change and no editor work.
    /// Returns how many objects were spawned.
    pub fn spawn_washer_drum_demo(
        &mut self,
        level: &LevelDef,
        catalog: &crate::loader::PropCatalog,
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
        let (Some(machine_path), Some(drum_path)) = (machine_path, drum_path) else {
            return 0;
        };
        let drum_asset = match assets.resolve(&drum_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&drum_path, &error);
                return 0;
            }
        };
        let drum_radius = catalog.get(DEMO_DRUM_ID).size[0].max(0.0) * 0.5;
        let machine_depth = catalog.get(DEMO_MACHINE_ID).size[2].max(0.0);
        let gap = 0.02;
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
                Some(path) if path == machine_path => {}
                _ => continue,
            }
            if !prop.x.is_finite() || !prop.y.is_finite() || !prop.z.is_finite() {
                continue;
            }
            let yaw = prop.rotation_degrees.to_radians();
            let (sin, cos) = yaw.sin_cos();
            let reach = machine_depth * 0.5 + drum_radius + gap;
            // A prop at rotation 0 faces +Z, so forward is (sin, 0, cos).
            let x = sin.mul_add(reach, prop.x);
            let z = cos.mul_add(reach, prop.z);
            let base_y = surfaces.floor_y_at(x, z).unwrap_or(0.0);
            let id = self.spawn(
                &drum_asset,
                [x, base_y + prop.y, z],
                prop.rotation_degrees,
                1.0,
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
mod tests {
    // Test code: unwrap/expect, indexing, loose casts and permissive arithmetic
    // are idiomatic in tests; the production lints stay enforced everywhere
    // else in the crate.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::too_many_lines,
        clippy::unwrap_used
    )]

    use super::*;
    use crate::gltf::{PropModel, PropSubmesh, PropVertex};

    fn level_from(json: &str) -> LevelDef {
        LevelDef::from_json(json).expect("test level parses")
    }

    /// A one-room level with the washing machine the demonstration needs.
    fn demo_level() -> LevelDef {
        level_from(
            r#"{
                "format_version": 1,
                "id": "dynamic_test",
                "name": "Dynamic Test",
                "spawn": { "x": 2.0, "z": 5.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 2.7 } ],
                "walls": [ { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.3 } ],
                "ceiling_lights": [ { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 } ],
                "props": [
                    { "model": "core:washing_machine", "x": 3.0, "z": 0.45,
                      "rotation_degrees": 0.0, "size": [0.6, 0.85, 0.6], "solid": true }
                ]
            }"#,
        )
    }

    fn washers() -> (crate::loader::PropCatalog, PropAssets) {
        (
            crate::loader::PropCatalog::load_default(),
            PropAssets::load_default(),
        )
    }

    fn synthetic_model(emission: MaterialEmission) -> PropModel {
        PropModel {
            vertices: vec![
                PropVertex {
                    pos: [0.0, 0.0, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                },
                PropVertex {
                    pos: [1.0, 0.0, 0.0],
                    color: [1.0, 0.5, 0.25, 1.0],
                    uv: [1.0, 0.0],
                },
                PropVertex {
                    pos: [0.0, 1.0, 0.0],
                    color: [0.5, 1.0, 0.5, 1.0],
                    uv: [0.0, 1.0],
                },
            ],
            indices: vec![0, 1, 2],
            textures: Vec::new(),
            submeshes: vec![PropSubmesh {
                material: 0,
                texture: None,
                emission,
                first_index: 0,
                index_count: 3,
            }],
            triangles: 1,
            materials: 1,
        }
    }

    fn synthetic_asset(emission: MaterialEmission) -> Rc<LoadedPropAsset> {
        Rc::new(LoadedPropAsset {
            model_path: "core/test_triangle.glb".to_string(),
            model: synthetic_model(emission),
        })
    }

    // ------------------------------------------------ representation boundary

    #[test]
    fn a_dynamic_mesh_keeps_model_space_vertices_and_albedo_colours() {
        let (catalog, mut assets) = washers();
        let path = catalog.get(DEMO_DRUM_ID).model.expect("drum model path");
        let asset = assets.resolve(&path).expect("drum loads");
        let mesh = DynamicMesh::from_asset(&asset).expect("drum has drawable geometry");
        assert_eq!(mesh.vertices.len(), asset.model.vertices.len());
        assert_eq!(mesh.indices, asset.model.indices);
        assert_eq!(mesh.model_path, path);
        for (vertex, source) in mesh.vertices.iter().zip(&asset.model.vertices) {
            // Model space, verbatim: no transform and no baked light were
            // folded in, which is what lets one buffer serve every instance.
            assert_eq!(vertex.pos, source.pos);
            assert_eq!(vertex.color, source.color);
            assert!(!vertex.is_lightmapped());
            assert_eq!(vertex.lightmap_page, LIGHTMAP_NONE);
        }
    }

    #[test]
    fn spawning_two_objects_of_one_model_shares_one_mesh() {
        let (catalog, mut assets) = washers();
        let path = catalog.get(DEMO_DRUM_ID).model.expect("drum model path");
        let asset = assets.resolve(&path).expect("drum loads");
        let mut scene = DynamicScene::new();
        let first = scene
            .spawn(&asset, [0.0, 0.0, 0.0], 0.0, 1.0, 0.0)
            .expect("first spawn");
        let second = scene
            .spawn(&asset, [2.0, 0.0, 0.0], 0.0, 1.0, 0.0)
            .expect("second spawn");
        assert_eq!(scene.len(), 2);
        assert_eq!(scene.mesh_count(), 1);
        assert!(Rc::ptr_eq(
            scene.get(first).unwrap().mesh(),
            scene.get(second).unwrap().mesh()
        ));
        // The shared mesh is the same decode the static prop path uses: the
        // asset cache hands back one `Rc` for one model path.
        let again = assets.resolve(&path).expect("cached resolve");
        assert!(Rc::ptr_eq(&asset, &again));
        assert_eq!(scene.vertex_count(), 2 * asset.model.vertices.len());
    }

    #[test]
    fn a_level_that_places_a_model_statically_does_not_re_decode_it_for_dynamics() {
        let level = demo_level();
        let (catalog, mut assets) = washers();
        let machine = catalog.get(DEMO_MACHINE_ID).model.expect("machine model");
        let first = assets.resolve(&machine).expect("static resolve");
        let second = assets.resolve(&machine).expect("dynamic resolve");
        assert!(
            Rc::ptr_eq(&first, &second),
            "the prop asset cache must hand out one decode per model path"
        );
        // ... and the demo spawns the drum, not the machine.
        let mut scene = DynamicScene::new();
        let spawned = scene.spawn_washer_drum_demo(&level, &catalog, &mut assets);
        assert_eq!(spawned, 1);
        assert_eq!(scene.len(), 1);
        let drum = catalog.get(DEMO_DRUM_ID).model.expect("drum model");
        assert_eq!(scene.objects()[0].model_path(), drum);
        assert_ne!(scene.objects()[0].model_path(), machine);
    }

    // ---------------------------------------------------------------- transforms

    #[test]
    fn a_transform_is_translation_yaw_then_uniform_scale() {
        let asset = synthetic_asset(MaterialEmission::NONE);
        let mut scene = DynamicScene::new();
        let id = scene
            .spawn(&asset, [1.0, 2.0, 3.0], 90.0, 2.0, 0.0)
            .expect("spawn");
        let transform = scene.get(id).unwrap().transform();
        // The model's +Z front faces +X after a 90 degree yaw, exactly like a
        // placed prop, and the scale doubles it.
        let front = transform.transform_point3(Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(front, Vec3::new(3.0, 2.0, 3.0));
        let up = transform.transform_point3(Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(up, Vec3::new(1.0, 4.0, 3.0));
        assert_eq!(scene.get(id).unwrap().translation(), [1.0, 2.0, 3.0]);
        assert_eq!(scene.get(id).unwrap().yaw_degrees(), 90.0);
        assert_eq!(scene.get(id).unwrap().scale(), 2.0);
    }

    #[test]
    fn set_transform_updates_only_the_named_object() {
        let asset = synthetic_asset(MaterialEmission::NONE);
        let mut scene = DynamicScene::new();
        let a = scene.spawn(&asset, [0.0, 0.0, 0.0], 0.0, 1.0, 0.0).unwrap();
        let b = scene.spawn(&asset, [1.0, 0.0, 0.0], 0.0, 1.0, 0.0).unwrap();
        assert!(scene.set_transform(a, [4.0, 5.0, 6.0], 180.0, 3.0));
        assert_eq!(scene.get(a).unwrap().translation(), [4.0, 5.0, 6.0]);
        assert_eq!(scene.get(a).unwrap().yaw_degrees(), 180.0);
        assert_eq!(scene.get(b).unwrap().translation(), [1.0, 0.0, 0.0]);
        assert!(scene.set_transform(b, [1.0, 0.0, 0.0], 0.0, 0.5));
        assert_eq!(scene.get(b).unwrap().scale(), 0.5);
        // Malformed input is refused, not applied.
        assert!(!scene.set_transform(a, [f32::NAN, 0.0, 0.0], 0.0, 1.0));
        assert!(!scene.set_transform(a, [0.0, 0.0, 0.0], 0.0, 0.0));
        assert!(!scene.set_transform(a, [0.0, 0.0, 0.0], f32::INFINITY, 1.0));
        assert_eq!(scene.get(a).unwrap().translation(), [4.0, 5.0, 6.0]);
    }

    #[test]
    fn spinning_advances_the_yaw_and_wraps_deterministically() {
        let asset = synthetic_asset(MaterialEmission::NONE);
        let mut scene = DynamicScene::new();
        let id = scene
            .spawn(&asset, [0.0, 0.0, 0.0], 350.0, 1.0, 12.0)
            .unwrap();
        let update = scene.update(1.0, None);
        assert_eq!(update.moved, 1);
        assert_eq!(scene.get(id).unwrap().yaw_degrees(), 2.0);
        let update = scene.update(0.5, None);
        assert_eq!(update.moved, 1);
        assert_eq!(scene.get(id).unwrap().yaw_degrees(), 8.0);
        // A non-finite delta is inert.
        let update = scene.update(f32::NAN, None);
        assert_eq!(update.moved, 0);
        assert_eq!(scene.get(id).unwrap().yaw_degrees(), 8.0);
    }

    // ------------------------------------------------------------------ probes

    #[test]
    fn a_light_probe_refreshes_on_movement_and_not_on_spin() {
        let level = demo_level();
        let lighting = LevelLighting::bake(&level);
        let (catalog, mut assets) = washers();
        let path = catalog.get(DEMO_DRUM_ID).model.expect("drum model");
        let asset = assets.resolve(&path).expect("drum loads");
        let mut scene = DynamicScene::new();
        let id = scene
            .spawn(&asset, [2.0, 1.5, 2.0], 0.0, 1.0, 30.0)
            .unwrap();
        assert!(!scene.get(id).unwrap().probe_valid());
        let update = scene.update(0.016, Some(&lighting));
        assert_eq!(update.probes_refreshed, 1);
        assert!(scene.get(id).unwrap().probe_valid());
        let first_probe = scene.get(id).unwrap().light_scale();
        assert!(first_probe.iter().all(|value| *value > 0.0));
        // The drum spins every frame, but its centre never moves: no re-probe.
        for _ in 0..10 {
            let update = scene.update(0.016, Some(&lighting));
            assert_eq!(update.moved, 1);
            assert_eq!(update.probes_refreshed, 0);
        }
        assert_eq!(scene.get(id).unwrap().light_scale(), first_probe);
        // A move smaller than the epsilon is also ignored.
        scene.set_transform(id, [2.0, 1.5, PROBE_EPSILON_M.mul_add(0.25, 2.0)], 0.0, 1.0);
        assert_eq!(scene.update(0.016, Some(&lighting)).probes_refreshed, 0);
        // A real move re-samples the probe at the new position.
        scene.set_transform(id, [4.5, 1.5, 4.5], 0.0, 1.0);
        let update = scene.update(0.016, Some(&lighting));
        assert_eq!(update.probes_refreshed, 1);
        assert_eq!(scene.get(id).unwrap().light_scale().len(), 3);
        // Without lighting the probe keeps its last value and never samples.
        let before = scene.get(id).unwrap().light_scale();
        scene.set_transform(id, [0.5, 1.5, 0.5], 0.0, 1.0);
        assert_eq!(scene.update(0.016, None).probes_refreshed, 0);
        assert_eq!(scene.get(id).unwrap().light_scale(), before);
    }

    #[test]
    fn a_probe_is_baked_light_and_never_allows_black_or_overbright() {
        let level = demo_level();
        let lighting = LevelLighting::bake(&level);
        let (catalog, mut assets) = washers();
        let path = catalog.get(DEMO_DRUM_ID).model.expect("drum model");
        let asset = assets.resolve(&path).expect("drum loads");
        let mut scene = DynamicScene::new();
        // Inside the room, under the panel and far outside every room.
        let ids: Vec<DynamicId> = [[3.0, 1.2, 3.0], [0.2, 0.2, 0.2], [900.0, 900.0, 900.0]]
            .into_iter()
            .map(|position| scene.spawn(&asset, position, 0.0, 1.0, 0.0).expect("spawn"))
            .collect();
        scene.update(0.016, Some(&lighting));
        for id in ids {
            let probe = scene.get(id).unwrap().light_scale();
            for channel in probe {
                assert!(channel >= crate::lighting::AMBIENT_LEVEL - 1e-4);
                assert!(channel <= crate::lighting::MAX_BRIGHTNESS + 1e-4);
            }
        }
    }

    // --------------------------------------------------------------- lifecycle

    #[test]
    fn spawn_is_bounded_deterministic_and_ordered() {
        let asset = synthetic_asset(MaterialEmission::NONE);
        let mut scene = DynamicScene::new();
        let mut ids = Vec::new();
        for index in 0..MAX_DYNAMIC_OBJECTS {
            let position = [index as f32, 0.0, 0.0];
            ids.push(
                scene
                    .spawn(&asset, position, 0.0, 1.0, 0.0)
                    .unwrap_or_else(|| panic!("spawn {index}")),
            );
        }
        assert_eq!(scene.len(), MAX_DYNAMIC_OBJECTS);
        assert!(
            scene
                .spawn(&asset, [0.0, 0.0, 0.0], 0.0, 1.0, 0.0)
                .is_none(),
            "the object budget must be enforced"
        );
        // Ids are unique and the iteration order is spawn order.
        let order: Vec<DynamicId> = scene.objects().iter().map(DynamicObject::id).collect();
        assert_eq!(order, ids);
        // Despawning preserves the relative order of the survivors.
        assert!(scene.despawn(ids[1]));
        assert!(!scene.despawn(ids[1]));
        let after: Vec<DynamicId> = scene.objects().iter().map(DynamicObject::id).collect();
        let expected: Vec<DynamicId> = ids.iter().copied().filter(|id| *id != ids[1]).collect();
        assert_eq!(after, expected);
        assert_eq!(scene.len(), MAX_DYNAMIC_OBJECTS - 1);
        // A despawning scene never reuses a stale handle.
        let fresh = scene.spawn(&asset, [0.0, 0.0, 0.0], 0.0, 1.0, 0.0).unwrap();
        assert!(!ids.contains(&fresh));
        scene.clear();
        assert!(scene.is_empty());
        assert!(scene.get(fresh).is_none());
        assert!(scene.revision() > 0);
    }

    #[test]
    fn distinct_mesh_budget_is_enforced_and_meshes_stay_registered() {
        let mut scene = DynamicScene::new();
        for index in 0..=MAX_DYNAMIC_MESHES {
            let asset = Rc::new(LoadedPropAsset {
                model_path: format!("core/test_{index}.glb"),
                model: synthetic_model(MaterialEmission::NONE),
            });
            let spawned = scene.spawn(&asset, [0.0, 0.0, 0.0], 0.0, 1.0, 0.0);
            if index < MAX_DYNAMIC_MESHES {
                assert!(spawned.is_some(), "mesh {index} must fit the budget");
            } else {
                assert!(spawned.is_none(), "mesh budget must be enforced");
            }
        }
        assert_eq!(scene.mesh_count(), MAX_DYNAMIC_MESHES);
        scene.clear();
        assert!(scene.is_empty());
        assert_eq!(scene.mesh_count(), MAX_DYNAMIC_MESHES);
        scene.clear_all();
        assert_eq!(scene.mesh_count(), 0);
    }

    #[test]
    fn a_model_with_nothing_drawable_never_spawns() {
        let asset = Rc::new(LoadedPropAsset {
            model_path: "core/empty.glb".to_string(),
            model: PropModel {
                vertices: Vec::new(),
                indices: Vec::new(),
                textures: Vec::new(),
                submeshes: Vec::new(),
                triangles: 0,
                materials: 0,
            },
        });
        let mut scene = DynamicScene::new();
        assert!(
            scene
                .spawn(&asset, [0.0, 0.0, 0.0], 0.0, 1.0, 0.0)
                .is_none()
        );
        assert!(scene.is_empty());
        assert_eq!(scene.mesh_count(), 0);
    }

    // ------------------------------------------------------- emission routing

    #[test]
    fn an_emissive_dynamic_object_routes_emission_independently_of_light() {
        let asset = synthetic_asset(MaterialEmission::new([1.0, 0.25, 0.0], 4.0));
        let mut scene = DynamicScene::new();
        let id = scene.spawn(&asset, [3.0, 1.0, 3.0], 0.0, 1.0, 0.0).unwrap();
        let submesh = scene.get(id).unwrap().mesh().submeshes[0].clone();
        let object = scene.get(id).unwrap();
        // Without an override the primitive keeps its own material emission...
        assert_eq!(object.submesh_emission(&submesh), submesh.emission);
        assert!(object.submesh_emission(&submesh).is_emissive());
        assert_eq!(
            object.submesh_emission(&submesh).effective_color(),
            [4.0, 1.0, 0.0]
        );
        // ... and an override replaces it, sanitised.
        assert!(scene.set_emission(id, Some(MaterialEmission::new([1.0, 1.0, 1.0], 99.0))));
        let object = scene.get(id).unwrap();
        let overridden = object.submesh_emission(&submesh);
        assert_eq!(
            overridden.intensity,
            crate::materials::MAX_EMISSION_INTENSITY
        );
        // Emission is not a light: the probe is the baked room light and the
        // albedo is untouched, which is what the shader adds the emission on
        // top of.
        let albedo = object.mesh().vertices[0].color;
        assert_eq!(albedo, [1.0, 1.0, 1.0, 1.0]);
        assert!(scene.set_emission(id, None));
        assert_eq!(scene.get(id).unwrap().emission(), None);
    }

    // --------------------------------------------------- demonstration plumbing

    #[test]
    fn the_demo_places_the_drum_in_front_of_the_machines_door() {
        let level = demo_level();
        let (catalog, mut assets) = washers();
        let mut scene = DynamicScene::new();
        assert_eq!(
            scene.spawn_washer_drum_demo(&level, &catalog, &mut assets),
            1
        );
        let drum = scene.objects()[0].transform();
        // The machine is at (3, 0.45) facing +Z, is 0.6 m deep and the drum is
        // 0.42 m across, so the drum centre lands at 0.45 + 0.3 + 0.21 + 0.02.
        let position = drum.transform_point3(Vec3::ZERO);
        assert!((position.x - 3.0).abs() < 1e-6);
        assert!((position.z - 0.98).abs() < 1e-6);
        assert!((position.y - 0.0).abs() < 1e-6);
        assert_eq!(
            scene.objects()[0].spin_degrees_per_second(),
            DEMO_SPIN_DEGREES_PER_SECOND
        );
    }

    #[test]
    fn the_demo_follows_a_rotated_machine() {
        let level = level_from(
            r#"{
                "format_version": 1,
                "id": "dynamic_test_rotated",
                "name": "Rotated",
                "spawn": { "x": 2.0, "z": 2.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 2.7 } ],
                "walls": [],
                "ceiling_lights": [],
                "props": [
                    { "model": "core:washing_machine", "x": 3.0, "z": 3.0,
                      "rotation_degrees": 90.0, "size": [0.6, 0.85, 0.6], "solid": true }
                ]
            }"#,
        );
        let (catalog, mut assets) = washers();
        let mut scene = DynamicScene::new();
        assert_eq!(
            scene.spawn_washer_drum_demo(&level, &catalog, &mut assets),
            1
        );
        // At 90 degrees the machine faces +X: the drum sits east of it.
        let position = scene.objects()[0].transform().transform_point3(Vec3::ZERO);
        assert!((position.x - 3.53).abs() < 1e-5);
        assert!((position.z - 3.0).abs() < 1e-5);
    }

    #[test]
    fn a_level_with_no_machine_spawns_nothing() {
        let level = level_from(
            r#"{
                "format_version": 1,
                "id": "dynamic_test_empty",
                "name": "Empty",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [],
                "walls": [],
                "ceiling_lights": [],
                "props": []
            }"#,
        );
        let (catalog, mut assets) = washers();
        let mut scene = DynamicScene::new();
        assert_eq!(
            scene.spawn_washer_drum_demo(&level, &catalog, &mut assets),
            0
        );
        assert!(scene.is_empty());
    }

    #[test]
    fn the_demo_scene_draws_one_call_per_primitive_and_never_touches_static_geometry() {
        let level = demo_level();
        let (catalog, mut assets) = washers();
        let materials = crate::render::logical_materials(&level);
        let (mesh, _props, lighting, _timings) =
            crate::render::build_level_geometry_timed(&level, &catalog, &mut assets, &materials);
        let static_vertices = mesh.vertex_count;
        let static_indices = mesh.index_count;
        let static_fallback = mesh.index_count_for(crate::render::SurfaceKind::PropFallback);
        let blockers = lighting.summary().blockers;
        let baking: Vec<crate::render::Vertex> = mesh.all_vertices();

        let mut scene = DynamicScene::new();
        assert_eq!(
            scene.spawn_washer_drum_demo(&level, &catalog, &mut assets),
            1
        );
        let id = scene.objects()[0].id();
        for step in 0..120 {
            scene.update(1.0 / 60.0, Some(&lighting));
            assert!(scene.set_transform(id, [step as f32 * 0.01, 0.0, 0.0], 0.0, 1.0));
        }
        // Moving the drum rebuilt nothing: the static mesh is bit-for-bit what
        // the bake produced, the drum is not in the occlusion set, and no
        // placeholder box was emitted for it.
        assert_eq!(mesh.vertex_count, static_vertices);
        assert_eq!(mesh.index_count, static_indices);
        assert_eq!(
            mesh.index_count_for(crate::render::SurfaceKind::PropFallback),
            static_fallback
        );
        assert_eq!(mesh.all_vertices(), baking);
        assert_eq!(lighting.summary().blockers, blockers);
        assert_eq!(level.props.len(), 1);
        assert_eq!(scene.draw_count(), 1);
    }
}
