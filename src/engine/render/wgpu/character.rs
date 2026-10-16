//! Animated characters on wgpu.
//!
//! The renderer-neutral [`CharacterScene`] owns the rigs, the placement
//! transforms and the pose evaluation; this module owns the GPU side:
//!
//! * one **shared index buffer per distinct model** (indices never change);
//! * one **mutable vertex buffer per character**, written by CPU skinning
//!   only on the frames the animator's pose revision changes;
//! * one group-3 environment per character carrying the placement model
//!   matrix and the character's current opacity; the vertices are skinned in
//!   model space, so the shader applies the placement exactly like the static
//!   prop path;
//! * one plain GPU material per mesh submesh, shared by every character of
//!   that model, and one [`BatchPass`] per submesh from its glTF material.
//!
//! Colour carries material albedo only. The per-character environment supplies
//! the same current HDR probe payload as dynamic models. A blended submesh (the sheet
//! ghost) draws in the sorted translucent pass with its opacity applied.
//! Nothing here is created per frame: `sync` reuses the buffers and materials
//! uploaded with the level and writes only the environments whose placement or
//! opacity changed.

use std::collections::HashMap;
use std::sync::Arc;

use glam::Mat4;

use super::environment::EnvironmentBindings;
use super::lightmap::LightmapAtlas;
use super::material::{EmissionRecord, GpuMaterial};
use super::texture::{CacheOutcome, GpuTexture, TextureCache};
use super::world::{EnvironmentUniform, WORLD_VERTEX_STRIDE, WorldVertex};
use crate::engine::materials::TextureOrigin;
use crate::engine::quality::{QualityLevel, TextureClass};
use crate::engine::render::common::character::{
    Character, CharacterAnimator, CharacterScene, character_vertex,
};
use crate::engine::render::common::dynamic_visibility::BlacklightReceiverCache;
use crate::engine::render::common::materials::BatchPass;
use crate::engine::spatial::Aabb;

/// One distinct model uploaded in model space.
struct CharacterMeshGpu {
    /// Triangle indices, shared by every character of this model.
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// Distinct vertices one character of this model re-poses.
    vertex_count: u32,
    /// GPU sheet per model texture.
    textures: Vec<Arc<GpuTexture>>,
    /// One entry per drawable primitive.
    submeshes: Vec<CharacterSubmeshGpu>,
    /// Material slot per submesh, shared by every character of this model.
    materials: Vec<usize>,
    /// Emission flag per submesh.
    emissive: Vec<bool>,
    /// Rest transforms and clips preserve a usable outward orientation.
    culling_safe: bool,
}

/// The pass one source submesh's alpha contract maps to for a character.
///
/// A `MASK` material draws in the cut-out pass; a `BLEND` material (a glTF
/// `alphaMode: "BLEND"`, exported for the sheet ghost) draws in the sorted
/// translucent character pass. A zero-opacity blend is invisible and draws in
/// neither. Pure, so the classification is unit-testable without a device.
#[must_use]
const fn character_submesh_pass(alpha: crate::engine::materials::MaterialAlpha) -> BatchPass {
    BatchPass::of(alpha)
}

/// Coalesces adjacent opaque/cutout ranges with identical material state once,
/// during upload. Vertex joint ownership and blend ordering are untouched.
fn coalesced_submeshes(
    sources: &[crate::engine::gltf::PropSubmesh],
) -> Vec<crate::engine::gltf::PropSubmesh> {
    let mut ranges: Vec<crate::engine::gltf::PropSubmesh> = Vec::with_capacity(sources.len());
    for source in sources.iter().filter(|source| source.index_count > 0) {
        if let Some(previous) = ranges.last_mut() {
            let same_material = previous.material == source.material
                && previous.texture == source.texture
                && previous.emission == source.emission
                && previous.alpha == source.alpha
                && previous.response == source.response
                && previous.double_sided == source.double_sided;
            let adjacent =
                previous.first_index.checked_add(previous.index_count) == Some(source.first_index);
            if same_material
                && adjacent
                && source.alpha.mode != crate::engine::materials::AlphaMode::Blend
                && let Some(count) = previous.index_count.checked_add(source.index_count)
            {
                previous.index_count = count;
                continue;
            }
        }
        ranges.push(*source);
    }
    ranges
}

/// One primitive of a character model.
#[derive(Clone, Copy)]
struct CharacterSubmeshGpu {
    /// Index into [`CharacterMeshGpu::textures`], or `None` for a primitive
    /// whose material has no texture (it draws the shared fallback sheet, like
    /// the static prop path).
    texture: Option<usize>,
    first_index: u32,
    index_count: u32,
    /// The pass the primitive's glTF material draws in.
    ///
    /// A `MASK` material selects the cut-out pass; a `BLEND` material (the
    /// sheet ghost's exported `alphaMode`) selects the sorted translucent pass.
    pass: BatchPass,
    /// The primitive explicitly asks to render both sides.
    double_sided: bool,
}

/// Which neutral-scene character a GPU entry belongs to.
///
/// A placed character is addressed by its stable index in
/// [`CharacterScene::characters`]; a runtime-spawned actor by its instance id,
/// because the runtime list is not index-stable.
enum CharacterKey {
    Placed(usize),
    Runtime(String),
}

impl CharacterKey {
    fn resolve<'a>(&self, scene: &'a CharacterScene) -> Option<&'a Character> {
        match self {
            Self::Placed(slot) => scene.characters().get(*slot),
            Self::Runtime(instance_id) => scene.runtime_character(instance_id),
        }
    }
}

/// One live character's GPU state.
struct CharacterGpu {
    /// Slot into [`WgpuCharacters::meshes`].
    mesh: usize,
    /// The neutral-scene character this entry draws. `upload` may skip a
    /// character whose mesh cannot be uploaded, so the GPU list is not always
    /// a 1:1 copy of the scene's.
    key: CharacterKey,
    /// This character's mutable, CPU-skinned vertex buffer.
    vertex_buffer: wgpu::Buffer,
    /// This character's environment binding: placement and current irradiance.
    environment: EnvironmentBindings,
    /// The animator revision the vertex buffer currently holds.
    uploaded_revision: u64,
    /// The placement matrix the environment uniform currently holds.
    uploaded_transform: Mat4,
    /// Model-space bounds of the vertices currently in the GPU buffer.
    pose_bounds: Aabb,
    /// Current pose bounds transformed by the uploaded placement.
    world_bounds: Aabb,
    /// Reused model-space CPU target, prepared before any frame uploads.
    posed_vertices: Vec<WorldVertex>,
    /// The frame's CPU-prepared environment; queue writes follow the join.
    pending_environment: EnvironmentUniform,
    uv_reactive: bool,
    uv_receiver: BlacklightReceiverCache,
}

/// What one character upload or frame did, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterGpuStats {
    /// Distinct models uploaded.
    pub meshes: usize,
    /// Live characters.
    pub characters: usize,
    /// Draw calls one frame submits (one per character per primitive).
    pub draws: usize,
    /// Distinct vertices the scene re-poses, summed over characters.
    pub vertices: usize,
    /// Vertex bytes resident.
    pub vertex_bytes: usize,
    /// Index bytes resident.
    pub index_bytes: usize,
    /// Model sheets uploaded this level load.
    pub texture_uploads: usize,
    /// Model sheets the cache already held.
    pub texture_cache_hits: usize,
}

/// Every character the level draws.
#[derive(Default)]
pub struct WgpuCharacters {
    meshes: Vec<CharacterMeshGpu>,
    characters: Vec<CharacterGpu>,
    materials: Vec<GpuMaterial>,
    /// Shared sheet bound by a submesh whose material declares no texture, so
    /// an untextured primitive still draws instead of disappearing.
    fallback: Option<Arc<GpuTexture>>,
    stats: CharacterGpuStats,
    /// Inline frames retain the original shared, cache-hot skin/upload target.
    scratch: Vec<WorldVertex>,
}

/// Below this total changed vertex count, a frame avoids worker dispatch.
/// Work is independent per character; a one-character scene also stays serial.
const PARALLEL_SKINNING_MIN_VERTICES: usize = 8_192;

const fn skinning_min_characters(changed_vertices: usize) -> usize {
    if changed_vertices >= PARALLEL_SKINNING_MIN_VERTICES {
        2
    } else {
        usize::MAX
    }
}

/// The renderer state a character upload needs: the shared texture cache, the
/// material and environment layouts, the level's lightmap atlas, the level's
/// environment template (lightmap selection and fog) and the reflection views
/// the environment binds.
pub struct CharacterUploadContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub cache: &'a mut TextureCache,
    pub material_layout: &'a wgpu::BindGroupLayout,
    pub environment_layout: &'a wgpu::BindGroupLayout,
    pub caustics: &'a super::caustics::Caustics,
    pub lightmaps: &'a LightmapAtlas,
    /// The level environment every character's uniform starts from: unit light
    /// scale, the resident lightmap selection and the fog constants.
    pub environment: EnvironmentUniform,
    pub probes: &'a [&'a wgpu::TextureView],
    pub planar: &'a wgpu::TextureView,
    pub probe_fallback: &'a wgpu::TextureView,
    pub planar_fallback: &'a wgpu::TextureView,
    pub level: QualityLevel,
    pub lighting_level: QualityLevel,
}

/// Bounds follow the submitted pose, with only a numeric guard for CPU/GPU
/// matrix rounding. The absolute term is 0.1 mm; the relative term covers a
/// few float operations when placements use larger world coordinates.
fn posed_world_bounds(pose: &Aabb, transform: &Mat4) -> Aabb {
    let mut bounds = crate::engine::render::common::props::transform_bounds(pose, transform);
    if !bounds.is_empty() {
        for (min, max) in bounds.min.iter_mut().zip(bounds.max.iter_mut()) {
            let margin = min.abs().max(max.abs()).mul_add(8.0 * f32::EPSILON, 1.0e-4);
            *min -= margin;
            *max += margin;
        }
    }
    bounds
}

const CULLING_DETERMINANT_MIN: f32 = 1.0e-8;

fn positive_orientation(matrix: &Mat4) -> bool {
    let determinant = matrix.determinant();
    matrix.is_finite() && determinant.is_finite() && determinant > CULLING_DETERMINANT_MIN
}

/// Imported reflected/singular bind hierarchies, scaling clips and morphs have
/// no stable global winding contract, so their opaque primitives remain two-sided.
/// Ordinary skeletal rotation deformation retains the authored material side.
fn character_mesh_culling_safe(model: &crate::engine::gltf::PropModel) -> bool {
    model.morph_targets.is_empty()
        && node_hierarchy_culling_safe(&model.nodes)
        && model.skin.as_ref().is_none_or(|skin| {
            node_hierarchy_culling_safe(&skin.nodes)
                && skin.inverse_bind.iter().all(positive_orientation)
        })
        && model.animations.iter().all(|clip| {
            clip.channels
                .iter()
                .all(|channel| channel.path != crate::engine::gltf::AnimationPath::Scale)
        })
}

fn node_hierarchy_culling_safe(nodes: &[crate::engine::gltf::PropNode]) -> bool {
    nodes.iter().all(|node| {
        let mut global = node.local_transform();
        if !positive_orientation(&global) {
            return false;
        }
        let mut ancestor = node.parent;
        // Parsing rejects cycles; the cap also keeps malformed test/future
        // callers conservative without allocating another hierarchy cache.
        for _ in 0..nodes.len() {
            let Some(parent) = ancestor else {
                return positive_orientation(&global);
            };
            let Some(parent_node) = nodes.get(usize::from(parent)) else {
                return false;
            };
            global = parent_node.local_transform().mul_mat4(&global);
            ancestor = parent_node.parent;
        }
        false
    })
}

impl WgpuCharacters {
    /// The exact uniform last sent for a placed or runtime entity id.
    pub fn diagnostic_uniform(
        &self,
        id: &str,
        placed_slot: Option<usize>,
    ) -> Option<&EnvironmentUniform> {
        self.characters
            .iter()
            .find(|c| match &c.key {
                CharacterKey::Placed(slot) => Some(*slot) == placed_slot,
                CharacterKey::Runtime(key) => key == id,
            })
            .map(|c| c.environment.uploaded_uniform())
    }

    /// Uploads every character of one scene, building the shared meshes,
    /// per-character vertex buffers and environments.
    ///
    /// Placed characters and runtime-spawned actors upload through the same
    /// path; only the key each entry resolves through differs. Called after the
    /// level's static resources and probes exist, once per level install, per
    /// graphics change and whenever a runtime spawn or despawn changes the
    /// scene. An empty scene produces an empty value.
    #[must_use]
    pub fn upload(ctx: &mut CharacterUploadContext<'_>, scene: &CharacterScene) -> Self {
        let mut value = Self::default();
        if scene.is_empty() {
            return value;
        }
        value.fallback = Some(ctx.cache.fallback());
        let widest = scene
            .characters()
            .iter()
            .chain(scene.runtime_characters().iter())
            .map(|character| character.asset().model.vertices.len())
            .max()
            .unwrap_or(0);
        value.scratch.reserve(widest);
        let mut mesh_index_by_path: HashMap<String, usize> = HashMap::new();
        let entries = scene
            .characters()
            .iter()
            .enumerate()
            .map(|(slot, character)| (CharacterKey::Placed(slot), character))
            .chain(scene.runtime_characters().iter().filter_map(|character| {
                character
                    .instance_id()
                    .map(|id| (CharacterKey::Runtime(id.to_string()), character))
            }));
        for (key, character) in entries {
            let model_path = character.asset().model_path.clone();
            let mesh_index = if let Some(index) = mesh_index_by_path.get(&model_path).copied() {
                index
            } else {
                let Some(mesh) = Self::upload_mesh(ctx, &mut value, character.asset()) else {
                    continue;
                };
                value.meshes.push(mesh);
                let index = value.meshes.len().saturating_sub(1);
                let _previous_value = mesh_index_by_path.insert(model_path, index);
                index
            };
            let Some(mesh) = value.meshes.get(mesh_index) else {
                continue;
            };
            let vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("places-wgpu-character-vertices"),
                size: u64::from(mesh.vertex_count)
                    .saturating_mul(WORLD_VERTEX_STRIDE)
                    .max(4),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let pending_environment = ctx
                .environment
                .with_model(character.transform())
                .with_entity_lighting(Some(character.entity_lighting()))
                .with_spatial_lighting(character.spatial_lighting())
                .with_opacity(character.opacity());
            let environment = EnvironmentBindings::new(
                ctx.device,
                ctx.queue,
                ctx.environment_layout,
                ctx.cache,
                ctx.lightmaps,
                ctx.probes,
                ctx.planar,
                ctx.probe_fallback,
                ctx.planar_fallback,
                ctx.caustics,
                &pending_environment,
            );
            let mut posed_vertices = Vec::with_capacity(character.asset().model.vertices.len());
            let pose_bounds = Self::fill_vertices(character, &mut posed_vertices);
            ctx.queue
                .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&posed_vertices));
            value.characters.push(CharacterGpu {
                mesh: mesh_index,
                key,
                vertex_buffer,
                environment,
                uploaded_revision: character.animator().revision(),
                uploaded_transform: character.transform(),
                pose_bounds,
                world_bounds: posed_world_bounds(&pose_bounds, &character.transform()),
                posed_vertices,
                pending_environment,
                uv_reactive: character
                    .asset()
                    .model
                    .submeshes
                    .iter()
                    .any(|submesh| submesh.response.fluorescence.strength > 0.0),
                uv_receiver: BlacklightReceiverCache::default(),
            });
        }
        value.finish_stats();
        value
    }

    /// Uploads one model's shared index buffer, textures, materials and
    /// submeshes.
    ///
    /// There is no vertex buffer here: every character owns its own mutable
    /// vertex buffer because every character's pose differs.
    fn upload_mesh(
        ctx: &mut CharacterUploadContext<'_>,
        value: &mut Self,
        asset: &crate::engine::props::LoadedPropAsset,
    ) -> Option<CharacterMeshGpu> {
        let model = &asset.model;
        if model.indices.is_empty() || model.submeshes.is_empty() {
            return None;
        }
        let textures = Self::upload_display_textures(ctx, value, asset);
        let mut submeshes: Vec<CharacterSubmeshGpu> = Vec::with_capacity(model.submeshes.len());
        let mut materials: Vec<usize> = Vec::with_capacity(model.submeshes.len());
        let mut emissive: Vec<bool> = Vec::with_capacity(model.submeshes.len());
        for source in coalesced_submeshes(&model.submeshes) {
            let texture = source
                .texture
                .map(usize::from)
                .filter(|index| *index < textures.len());
            let mask = source
                .emission
                .mask
                .map(|index| usize::try_from(index).unwrap_or(usize::MAX))
                .filter(|index| *index < textures.len());
            let record = EmissionRecord::material(source.emission, mask.is_some());
            let mask_texture = mask
                .and_then(|index| textures.get(index).cloned())
                .unwrap_or_else(|| ctx.cache.fallback());
            let uv_mask = Self::upload_response_mask(ctx, asset, source.response.fluorescence);
            value.materials.push(GpuMaterial::model_with_fluorescence(
                ctx.device,
                ctx.queue,
                ctx.material_layout,
                ctx.cache,
                &mask_texture,
                &uv_mask,
                super::material::ModelMaterial {
                    albedo_tint: [1.0; 3],
                    emission: record,
                    alpha: source.alpha,
                    response: source.response,
                    response_enabled: ctx.lighting_level.draws_surface_response(),
                },
            ));
            materials.push(value.materials.len().saturating_sub(1));
            emissive.push(record.is_emissive());
            let pass = character_submesh_pass(source.alpha);
            submeshes.push(CharacterSubmeshGpu {
                texture,
                first_index: source.first_index,
                index_count: source.index_count,
                pass,
                double_sided: source.double_sided,
            });
        }
        if submeshes.is_empty() {
            return None;
        }
        let index_buffer = super::world::upload_index_buffer(
            ctx.device,
            &model.indices,
            "places-wgpu-character-indices",
        );
        Some(CharacterMeshGpu {
            index_buffer,
            index_count: u32::try_from(model.indices.len()).unwrap_or(u32::MAX),
            vertex_count: u32::try_from(model.vertices.len()).unwrap_or(u32::MAX),
            textures,
            submeshes,
            materials,
            emissive,
            culling_safe: character_mesh_culling_safe(model),
        })
    }

    /// Uploads display sheets while keeping numeric-only texture slots stable.
    fn upload_display_textures(
        ctx: &mut CharacterUploadContext<'_>,
        value: &mut Self,
        asset: &crate::engine::props::LoadedPropAsset,
    ) -> Vec<Arc<GpuTexture>> {
        let model = &asset.model;
        let mut textures: Vec<Arc<GpuTexture>> = Vec::with_capacity(model.textures.len());
        for (index, image) in model.textures.iter().enumerate() {
            if super::material::ultraviolet_only_texture(
                index,
                model.submeshes.iter().map(|part| {
                    (
                        part.texture,
                        part.emission.mask,
                        part.response.fluorescence.mask,
                    )
                }),
            ) {
                textures.push(ctx.cache.fallback());
                continue;
            }
            let logical = super::texture::model_image_key(image);
            let (outcome, texture) = ctx.cache.get_or_upload_fitted(
                ctx.device,
                ctx.queue,
                &logical,
                image,
                TextureClass::Prop,
                TextureOrigin::Catalog,
                ctx.level,
            );
            match outcome {
                CacheOutcome::Uploaded => {
                    value.stats.texture_uploads = value.stats.texture_uploads.saturating_add(1);
                }
                CacheOutcome::Reused => {
                    value.stats.texture_cache_hits =
                        value.stats.texture_cache_hits.saturating_add(1);
                }
            }
            textures.push(texture);
        }
        textures
    }

    /// Resolves one fitted numeric response mask independently of display colour.
    fn upload_response_mask(
        ctx: &mut CharacterUploadContext<'_>,
        asset: &crate::engine::props::LoadedPropAsset,
        fluorescence: crate::engine::materials::MaterialFluorescence,
    ) -> Arc<GpuTexture> {
        let Some(image) = fluorescence.mask.and_then(|index| {
            asset
                .model
                .textures
                .get(usize::try_from(index).unwrap_or(usize::MAX))
        }) else {
            return ctx.cache.fallback();
        };
        super::material::upload_uv_mask(
            ctx.device,
            ctx.queue,
            ctx.cache,
            &format!("character:{}:uv", asset.model_path),
            image,
            ctx.level,
        )
    }

    /// Fills one character's reusable CPU target with its posed vertices.
    ///
    /// Skins in model space with the animator's current deltas and attaches
    /// material albedo while measuring exactly the positions uploaded; no
    /// allocation once the buffer has been sized.
    fn fill_vertices(character: &Character, out: &mut Vec<WorldVertex>) -> Aabb {
        Self::fill_posed_vertices(
            &character.asset().model,
            character.animator(),
            character.albedo(),
            out,
        )
    }

    fn fill_posed_vertices(
        model: &crate::engine::gltf::PropModel,
        animator: &CharacterAnimator,
        albedo: &[[f32; 4]],
        out: &mut Vec<WorldVertex>,
    ) -> Aabb {
        out.clear();
        let mut bounds = Aabb::EMPTY;
        for (index, vertex) in model.vertices.iter().enumerate() {
            let joints = model.joints.get(index).copied().unwrap_or([0; 4]);
            let weights = model.weights.get(index).copied().unwrap_or([0.0; 4]);
            let position = animator.skin_position(joints, weights, vertex.pos);
            bounds.expand(position);
            let colour = albedo.get(index).copied().unwrap_or([1.0; 4]);
            let mut posed = character_vertex(colour, vertex.uv, position);
            posed.normal = vertex.normal.map_or([0.0; 3], |normal| {
                animator.skin_normal(joints, weights, normal)
            });
            out.push(WorldVertex::from(posed));
        }
        bounds
    }

    /// Fills the per-frame counters.
    fn finish_stats(&mut self) {
        self.stats.meshes = self.meshes.len();
        self.stats.characters = self.characters.len();
        self.stats.draws = self
            .characters
            .iter()
            .map(|character| {
                self.meshes
                    .get(character.mesh)
                    .map_or(0, |mesh| mesh.submeshes.len())
            })
            .sum();
        self.stats.vertices = self
            .characters
            .iter()
            .map(|character| {
                self.meshes.get(character.mesh).map_or(0, |mesh| {
                    usize::try_from(mesh.vertex_count).unwrap_or(usize::MAX)
                })
            })
            .sum();
        let vertex_stride = usize::try_from(WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX);
        self.stats.vertex_bytes = self
            .characters
            .iter()
            .map(|character| {
                self.meshes.get(character.mesh).map_or(0, |mesh| {
                    usize::try_from(mesh.vertex_count)
                        .unwrap_or(usize::MAX)
                        .saturating_mul(vertex_stride)
                })
            })
            .sum();
        self.stats.index_bytes = self
            .meshes
            .iter()
            .map(|mesh| {
                usize::try_from(mesh.index_count)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(std::mem::size_of::<u16>())
            })
            .sum();
    }

    /// Re-skins and re-uploads every character whose pose revision changed,
    /// and rewrites the environment matrix of every character whose route
    /// moved it.
    ///
    /// Placed characters and runtime-spawned actors sync through the same
    /// path, each entry resolved through its key. `environment` is the level's
    /// current template (the lightmap selection, the fog constants and no
    /// active mirror); a moved character installs its placement matrix on top.
    /// Called once per frame; a still character with a settled pose writes
    /// nothing.
    pub fn sync_with_blacklight(
        &mut self,
        queue: &wgpu::Queue,
        scene: &CharacterScene,
        environment: &EnvironmentUniform,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) -> usize {
        // Static lighting's short GPU workload benefits from interleaved pose
        // uploads: batching preparation raised measured Retina frame-time tails.
        // Use the installed environment, so live lighting changes switch safely.
        if environment.lightmap_enabled <= 0.5 {
            return self.sync_inline(queue, scene, environment, visibility);
        }
        let changed_vertices = self
            .characters
            .iter()
            .filter_map(|gpu| {
                let character = gpu.key.resolve(scene)?;
                (character.animator().revision() != gpu.uploaded_revision)
                    .then_some(character.asset().model.vertices.len())
            })
            .fold(0_usize, usize::saturating_add);
        let min_characters = skinning_min_characters(changed_vertices);
        if self.characters.len() < min_characters
            || crate::engine::runtime_work::global().workers() == 1
        {
            return self.sync_inline(queue, scene, environment, visibility);
        }
        crate::engine::runtime_work::for_each_mut(&mut self.characters, min_characters, |gpu| {
            let Some(character) = gpu.key.resolve(scene) else {
                return;
            };
            let pose_changed = character.animator().revision() != gpu.uploaded_revision;
            if pose_changed {
                gpu.pose_bounds = Self::fill_vertices(character, &mut gpu.posed_vertices);
            }
            gpu.pending_environment = environment
                .with_model(character.transform())
                .with_entity_lighting(Some(character.entity_lighting()))
                .with_spatial_lighting(character.spatial_lighting())
                .with_opacity(character.opacity());
            gpu.pending_environment.uv_receiver = gpu.uv_receiver.sample(
                visibility.filter(|_| gpu.uv_reactive),
                gpu.pose_bounds,
                character.transform(),
                None,
            );
            if character.transform() != gpu.uploaded_transform || pose_changed {
                gpu.world_bounds = posed_world_bounds(&gpu.pose_bounds, &character.transform());
            }
        });

        // The bounded CPU work has joined: all wgpu queue writes remain on
        // the caller thread and each uploaded revision names the finished pose.
        let mut uploaded = 0usize;
        for gpu in &mut self.characters {
            let Some(scene_character) = gpu.key.resolve(scene) else {
                continue;
            };
            // A fade is a per-frame environment write, so the comparison also
            // covers opacity: a still, settled pose writes nothing, while a
            // fading ghost rewrites only its own uniform.
            let pose_changed = scene_character.animator().revision() != gpu.uploaded_revision;
            if pose_changed {
                queue.write_buffer(
                    &gpu.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&gpu.posed_vertices),
                );
                gpu.uploaded_revision = scene_character.animator().revision();
                uploaded = uploaded.saturating_add(1);
            }

            let _update_stats = gpu.environment.update(queue, &gpu.pending_environment);
            gpu.uploaded_transform = scene_character.transform();
        }
        uploaded
    }

    /// Inline preparation uploads each pose immediately, retaining the original
    /// shared scratch and avoiding staged uniform copies in the serial path.
    fn sync_inline(
        &mut self,
        queue: &wgpu::Queue,
        scene: &CharacterScene,
        environment: &EnvironmentUniform,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) -> usize {
        let mut uploaded = 0_usize;
        for gpu in &mut self.characters {
            let Some(character) = gpu.key.resolve(scene) else {
                continue;
            };
            let transform_changed = character.transform() != gpu.uploaded_transform;
            let pose_changed = character.animator().revision() != gpu.uploaded_revision;
            if pose_changed {
                gpu.pose_bounds = Self::fill_vertices(character, &mut self.scratch);
                queue.write_buffer(&gpu.vertex_buffer, 0, bytemuck::cast_slice(&self.scratch));
                gpu.uploaded_revision = character.animator().revision();
                uploaded = uploaded.saturating_add(1);
            }
            let mut uniform = environment
                .with_model(character.transform())
                .with_entity_lighting(Some(character.entity_lighting()))
                .with_spatial_lighting(character.spatial_lighting())
                .with_opacity(character.opacity());
            uniform.uv_receiver = gpu.uv_receiver.sample(
                visibility.filter(|_| gpu.uv_reactive),
                gpu.pose_bounds,
                character.transform(),
                None,
            );
            let _update_stats = gpu.environment.update(queue, &uniform);
            gpu.uploaded_transform = character.transform();
            if transform_changed || pose_changed {
                gpu.world_bounds = posed_world_bounds(&gpu.pose_bounds, &gpu.uploaded_transform);
            }
        }
        uploaded
    }

    /// The upload and frame counters.
    #[must_use]
    pub const fn stats(&self) -> CharacterGpuStats {
        self.stats
    }

    /// The number of live characters.
    #[must_use]
    pub const fn character_count(&self) -> usize {
        self.characters.len()
    }

    /// One character's environment binding.
    #[must_use]
    pub fn environment(&self, character: usize) -> Option<&wgpu::BindGroup> {
        self.characters
            .get(character)
            .map(|gpu_character| gpu_character.environment.bind_group())
    }

    /// One character's world bounds.
    #[must_use]
    pub fn world_bounds(&self, character: usize) -> Option<Aabb> {
        self.characters
            .get(character)
            .map(|entry| entry.world_bounds)
    }

    /// One character's geometry: its own vertex buffer, the shared index
    /// buffer and one submesh's index range.
    #[must_use]
    pub fn geometry(
        &self,
        character: usize,
        submesh: usize,
    ) -> Option<(&wgpu::Buffer, &wgpu::Buffer, u32, u32)> {
        let entry = self.characters.get(character)?;
        let mesh = self.meshes.get(entry.mesh)?;
        let mesh_part = mesh.submeshes.get(submesh)?;
        Some((
            &entry.vertex_buffer,
            &mesh.index_buffer,
            mesh_part.first_index,
            mesh_part.index_count,
        ))
    }

    /// The number of submeshes one character draws.
    #[must_use]
    pub fn submesh_count(&self, character: usize) -> usize {
        self.characters
            .get(character)
            .and_then(|entry| self.meshes.get(entry.mesh))
            .map_or(0, |mesh| mesh.submeshes.len())
    }

    /// The pass one character's submesh draws in.
    #[must_use]
    pub fn submesh_pass(&self, character: usize, submesh: usize) -> Option<BatchPass> {
        let entry = self.characters.get(character)?;
        let mesh = self.meshes.get(entry.mesh)?;
        mesh.submeshes.get(submesh).map(|mesh_part| mesh_part.pass)
    }

    /// The material's two-sided contract; missing data stays conservative.
    #[must_use]
    pub fn submesh_double_sided(&self, character: usize, submesh: usize) -> bool {
        self.characters
            .get(character)
            .and_then(|entry| self.meshes.get(entry.mesh))
            .and_then(|mesh| mesh.submeshes.get(submesh))
            .is_none_or(|mesh_part| mesh_part.double_sided)
    }

    /// Selects placement winding only for an eligible bind hierarchy and
    /// clips. Singular/non-finite placements remain two-sided as well.
    #[must_use]
    pub fn culling_reflected(&self, character: usize) -> Option<bool> {
        let entry = self.characters.get(character)?;
        let mesh = self.meshes.get(entry.mesh)?;
        if !mesh.culling_safe {
            return None;
        }
        super::world::culling_reflected(entry.uploaded_transform)
    }

    /// True when one character has at least one submesh in `pass`.
    ///
    /// The translucent pass sorts only the characters that can actually draw
    /// there, so a fully opaque model never enters the ordering.
    #[must_use]
    pub fn has_submesh_pass(&self, character: usize, pass: BatchPass) -> bool {
        let Some(entry) = self.characters.get(character) else {
            return false;
        };
        let Some(mesh) = self.meshes.get(entry.mesh) else {
            return false;
        };
        mesh.submeshes.iter().any(|submesh| submesh.pass == pass)
    }

    /// True when the character's submesh emits.
    #[must_use]
    pub fn submesh_emissive(&self, character: usize, submesh: usize) -> bool {
        self.characters
            .get(character)
            .and_then(|entry| self.meshes.get(entry.mesh))
            .and_then(|mesh| mesh.emissive.get(submesh))
            .copied()
            .unwrap_or(false)
    }

    /// The material slot of one character's submesh.
    #[must_use]
    pub fn material_slot(&self, character: usize, submesh: usize) -> Option<usize> {
        let entry = self.characters.get(character)?;
        let mesh = self.meshes.get(entry.mesh)?;
        mesh.materials.get(submesh).copied()
    }

    /// The GPU material of one slot.
    #[must_use]
    pub fn material(&self, slot: usize) -> Option<&GpuMaterial> {
        self.materials.get(slot)
    }

    /// The texture of one character's submesh, or the shared fallback sheet
    /// for a primitive whose material declares none.
    #[must_use]
    pub fn submesh_texture(&self, character: usize, submesh: usize) -> Option<&GpuTexture> {
        let entry = self.characters.get(character)?;
        let mesh = self.meshes.get(entry.mesh)?;
        let mesh_part = mesh.submeshes.get(submesh)?;
        mesh_part
            .texture
            .and_then(|index| mesh.textures.get(index).map(Arc::as_ref))
            .or(self.fallback.as_deref())
    }

    /// The distinct vertices one character re-poses.
    #[must_use]
    pub fn character_vertex_count(&self, character: usize) -> usize {
        self.characters
            .get(character)
            .and_then(|entry| self.meshes.get(entry.mesh))
            .map_or(0, |mesh| {
                usize::try_from(mesh.vertex_count).unwrap_or(usize::MAX)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::entity::PoseCue;
    use crate::engine::materials::{AlphaMode, MaterialAlpha};

    fn tiny_scaled_rig() -> crate::engine::gltf::PropModel {
        use crate::engine::gltf::{
            AnimationInterpolation, AnimationPath, PropAnimation, PropAnimationChannel, PropNode,
            PropSkin, PropVertex,
        };

        let nodes: Vec<PropNode> = ["root", "scaled"]
            .into_iter()
            .map(|name| PropNode {
                name: name.to_string(),
                parent: None,
                children: Vec::new(),
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            })
            .collect();
        let rotation = glam::Quat::from_rotation_y(0.7).to_array();
        let mut rotations = vec![0.0, 0.0, 0.0, 1.0];
        rotations.extend(rotation);
        let channel = |node, path, values_per_key, values| PropAnimationChannel {
            node,
            path,
            interpolation: AnimationInterpolation::Linear,
            times: vec![0.0, 1.0],
            values,
            values_per_key,
        };
        let vertex = |pos, normal, uv| PropVertex {
            pos,
            normal,
            color: [1.0; 4],
            uv,
        };
        crate::engine::gltf::PropModel {
            vertices: vec![
                vertex(
                    [1.0, 1.0, 0.0],
                    Some([
                        std::f32::consts::FRAC_1_SQRT_2,
                        std::f32::consts::FRAC_1_SQRT_2,
                        0.0,
                    ]),
                    [0.25, 0.5],
                ),
                vertex([0.0, 0.0, 1.0], Some([0.0, 1.0, 0.0]), [0.75, 0.25]),
                vertex([4.0, -3.0, 2.0], None, [0.5, 0.75]),
                vertex([0.0, 1.0, -1.0], Some([0.0, 0.0, 1.0]), [1.0, 0.0]),
            ],
            indices: vec![0, 1, 2],
            textures: Vec::new(),
            submeshes: Vec::new(),
            triangles: 1,
            materials: 0,
            skin: Some(PropSkin {
                joints: vec![0, 1],
                inverse_bind: vec![Mat4::IDENTITY; 2],
                nodes: nodes.clone(),
                root: Some(0),
                mesh_node: None,
            }),
            joints: vec![[0; 4], [1, 0, 0, 0], [0; 4], [0, 1, 99, 0]],
            weights: vec![
                [1.0, 0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.0; 4],
                [0.5, 0.5, 0.25, -1.0],
            ],
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
            mesh_morph_ranges: Vec::new(),
            animations: vec![PropAnimation {
                name: "scaled_pose".to_string(),
                duration: 1.0,
                channels: vec![
                    channel(
                        0,
                        AnimationPath::Scale,
                        3,
                        vec![1.0, 1.0, 1.0, 2.0, 0.5, 1.5],
                    ),
                    channel(0, AnimationPath::Rotation, 4, rotations),
                    channel(
                        0,
                        AnimationPath::Translation,
                        3,
                        vec![0.0, 0.0, 0.0, 3.0, -2.0, 1.0],
                    ),
                    channel(
                        1,
                        AnimationPath::Scale,
                        3,
                        vec![1.0, 1.0, 1.0, 0.0, 1.0, 2.0],
                    ),
                ],
                looped: false,
                reference_speed_mps: None,
                kind: None,
            }],
            nodes,
        }
    }

    /// Two four-vertex poses exercise joined character preparation, including
    /// scaled/singular normals and unweighted/missing-joint fallback. It reuses
    /// one two-worker pool and fixed storage; no GPU, assets or map bake.
    #[test]
    fn parallel_skinning_matches_serial_vertex_bytes_and_reuses_storage() -> Result<(), String> {
        let model = tiny_scaled_rig();
        let rest = CharacterAnimator::new(&model).ok_or("tiny rest rig is missing")?;
        let mut scaled = CharacterAnimator::new(&model).ok_or("tiny scaled rig is missing")?;
        play_pose(&mut scaled, "scaled_pose");
        let albedo = [[0.8, 0.4, 0.2, 0.6], [0.3, 0.5, 0.7, 1.0]];
        let make_targets = || {
            [&rest, &scaled].map(|animator| {
                (
                    animator,
                    Vec::with_capacity(model.vertices.len()),
                    Aabb::EMPTY,
                )
            })
        };
        let mut serial = make_targets();
        let mut parallel = make_targets();
        let executor = crate::engine::runtime_work::RuntimeWork::new(2)?;
        let operation = |target: &mut (&CharacterAnimator, Vec<WorldVertex>, Aabb)| {
            target.2 =
                WgpuCharacters::fill_posed_vertices(&model, target.0, &albedo, &mut target.1);
        };
        for _ in 0_u8..3 {
            serial.iter_mut().for_each(operation);
            executor.for_each_mut(&mut parallel, 2, operation);
            for (reference, actual) in serial.iter().zip(&parallel) {
                assert_eq!(
                    bytemuck::cast_slice::<WorldVertex, u8>(&actual.1),
                    bytemuck::cast_slice::<WorldVertex, u8>(&reference.1),
                    "joined workers preserve every uploaded vertex field and order"
                );
                assert_eq!(
                    actual.2, reference.2,
                    "pose bounds match the submitted bytes"
                );
                assert_eq!(
                    actual.1.capacity(),
                    model.vertices.len(),
                    "repeated preparation retains the fixed CPU allocation"
                );
            }
        }
        let posed = parallel.get(1).ok_or("scaled output is missing")?;
        let rotated = posed.1.first().ok_or("scaled vertex is missing")?;
        let delta = scaled.joint_delta(0).ok_or("scaled delta is missing")?;
        assert!(
            glam::Vec3::from_array(rotated.normal)
                .dot(delta.transform_vector3(glam::Vec3::new(1.0, -1.0, 0.0)))
                .abs()
                < 1.0e-5,
            "scaled normals retain inverse-transpose orthogonality"
        );
        assert_eq!(
            posed.1.get(1).map(|vertex| vertex.normal),
            Some([0.0; 3]),
            "a singular posed joint retains the zero-normal guard"
        );
        assert_eq!(
            posed.1.get(2).map(|vertex| (vertex.position, vertex.color)),
            Some(([4.0, -3.0, 2.0], [1.0; 4])),
            "unweighted vertices and absent albedo keep their existing fallbacks"
        );
        Ok(())
    }

    /// Protects the shared balloon rig's 13-to-2 draw reduction and material,
    /// transparency and index boundaries. Thirteen source ranges, no GPU or bake.
    #[test]
    fn adjacent_character_ranges_merge_without_crossing_material_or_blend_boundaries() {
        let body = crate::engine::gltf::PropSubmesh {
            double_sided: false,
            response: crate::engine::materials::MaterialResponse::NONE,
            material: 0,
            texture: Some(0),
            emission: crate::engine::materials::MaterialEmission::NONE,
            alpha: MaterialAlpha::OPAQUE,
            first_index: 0,
            index_count: 30,
        };
        let mut sources = vec![body];
        for segment in 0_u32..12 {
            sources.push(crate::engine::gltf::PropSubmesh {
                material: 1,
                first_index: 30_u32.saturating_add(segment.saturating_mul(6)),
                index_count: 6,
                ..body
            });
        }
        let merged = coalesced_submeshes(&sources);
        assert_eq!(
            merged.len(),
            2,
            "one latex and one shared-string draw replace thirteen node draws"
        );
        assert_eq!(
            merged.first().copied(),
            Some(body),
            "body material and range survive exactly"
        );
        assert_eq!(
            merged
                .get(1)
                .map(|part| (part.first_index, part.index_count)),
            Some((30, 72)),
            "all twelve strings remain in the same index buffer"
        );
        let mut different = sources.clone();
        if let Some(part) = different.get_mut(5) {
            part.material = 2;
        }
        assert_eq!(
            coalesced_submeshes(&different).len(),
            4,
            "distinct material cannot merge into its neighbours"
        );
        for part in different.iter_mut().skip(1) {
            part.alpha = MaterialAlpha::blend(0.5);
        }
        assert_eq!(
            coalesced_submeshes(&different).len(),
            13,
            "blend ranges preserve their original order and granularity"
        );
        let mut discontinuous = sources;
        if let Some(part) = discontinuous.get_mut(5) {
            part.first_index = part.first_index.saturating_add(1);
        }
        assert_eq!(
            coalesced_submeshes(&discontinuous).len(),
            4,
            "a discontinuous range cannot swallow another range's indices"
        );
    }

    fn mannequin_model() -> Result<crate::engine::gltf::PropModel, String> {
        crate::engine::gltf::parse_glb(include_bytes!(
            "../../../../assets/entities/mannequin/model/mannequin.glb"
        ))
        .map_err(|error| error.to_string())
    }

    fn contains_position(bounds: &Aabb, position: [f32; 3]) -> bool {
        position
            .iter()
            .zip(bounds.min.iter().zip(bounds.max.iter()))
            .all(|(value, (min, max))| value >= min && value <= max)
    }

    fn play_pose(animator: &mut CharacterAnimator, name: &str) {
        assert!(
            animator.update_cued(
                10.0,
                &PoseCue::Clip {
                    name: name.to_string(),
                    once: true,
                    paused: false,
                },
            ),
            "the shipped pose must update the animator"
        );
    }

    #[test]
    fn uploaded_pose_bounds_keep_extended_arms_visible_at_the_near_plane() -> Result<(), String> {
        let model = mannequin_model()?;
        let mut animator = CharacterAnimator::new(&model).ok_or("mannequin rig is missing")?;
        let mut vertices = Vec::with_capacity(model.vertices.len());
        let vertex_capacity = vertices.capacity();
        let standing = WgpuCharacters::fill_posed_vertices(&model, &animator, &[], &mut vertices);
        play_pose(&mut animator, "pose_arms_forward");
        let forward = WgpuCharacters::fill_posed_vertices(&model, &animator, &[], &mut vertices);
        assert!(
            forward.max.get(2).is_some_and(|z| *z > 0.6),
            "the shipped forward pose extends well past the bind box: {forward:?}"
        );
        assert!(
            vertices
                .iter()
                .all(|vertex| contains_position(&forward, vertex.position)),
            "every submitted position must fit the current pose bounds"
        );

        // These are the old 15%-plus-5-cm bind-pose limits for this shipped
        // model. The camera looks +Z: arms beyond its near plane are visible
        // although the old padded bind box is entirely behind the camera.
        let old_bounds = Aabb {
            min: [-0.2915, -0.179, -0.23078],
            max: [0.2915, 1.899, 0.23078],
        };
        let projection = glam::camera::rh::proj::directx::perspective(
            60.0_f32.to_radians(),
            1280.0 / 720.0,
            crate::engine::render::common::SCENE_NEAR_M,
            20.0,
        );
        let view = glam::camera::rh::view::look_at_mat4(
            glam::Vec3::new(0.0, 1.4, 0.4),
            glam::Vec3::new(0.0, 1.4, 1.4),
            glam::Vec3::Y,
        );
        let frustum = crate::engine::spatial::Frustum::from_view_projection(
            &projection.mul_mat4(&view),
            crate::engine::spatial::DepthRange::ZeroToOne,
        );
        assert!(
            !frustum.intersects_aabb(&old_bounds),
            "the old padded bind box reproduces the erroneous rejection"
        );
        assert!(
            vertices
                .iter()
                .any(|vertex| { frustum.intersects_aabb(&Aabb::from_point(vertex.position)) }),
            "the actual extended pose has submitted positions inside the view"
        );
        assert!(
            frustum.intersects_aabb(&posed_world_bounds(&forward, &Mat4::IDENTITY)),
            "the submitted pose must survive the matching CPU frustum test"
        );

        play_pose(&mut animator, "pose_arms_up");
        let raised = WgpuCharacters::fill_posed_vertices(&model, &animator, &[], &mut vertices);
        assert!(
            raised
                .max
                .get(1)
                .zip(standing.max.get(1))
                .is_some_and(|(y, rest)| *y > *rest + 0.1),
            "the raised hands extend above the current standing pose"
        );
        play_pose(&mut animator, "pose_stand");
        let restored = WgpuCharacters::fill_posed_vertices(&model, &animator, &[], &mut vertices);
        assert!(
            glam::Vec3::from_array(restored.min).distance(glam::Vec3::from_array(standing.min))
                < 1.0e-5
                && glam::Vec3::from_array(restored.max)
                    .distance(glam::Vec3::from_array(standing.max))
                    < 1.0e-5,
            "a later pose must replace stale bounds"
        );
        assert_eq!(
            vertices.capacity(),
            vertex_capacity,
            "skinning and bounds measurement reuse the pre-sized vertex storage"
        );
        Ok(())
    }

    #[test]
    fn cached_pose_bounds_follow_placement_without_skinning_again() -> Result<(), String> {
        let model = mannequin_model()?;
        let mut animator = CharacterAnimator::new(&model).ok_or("mannequin rig is missing")?;
        play_pose(&mut animator, "pose_arms_forward");
        let mut vertices = Vec::with_capacity(model.vertices.len());
        let pose = WgpuCharacters::fill_posed_vertices(&model, &animator, &[], &mut vertices);
        let initial = posed_world_bounds(&pose, &Mat4::IDENTITY);
        let placement = Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(2.5),
            glam::Quat::from_rotation_y(0.8),
            glam::Vec3::new(7.0, -0.5, -3.0),
        );
        let moved = posed_world_bounds(&pose, &placement);
        assert_ne!(
            initial, moved,
            "placement changes must replace world bounds"
        );
        for vertex in &vertices {
            let position = placement
                .transform_point3(glam::Vec3::from_array(vertex.position))
                .to_array();
            assert!(
                contains_position(&moved, position),
                "rotated/scaled submitted positions must remain inside the moved bounds"
            );
        }
        assert_eq!(
            posed_world_bounds(&pose, &Mat4::IDENTITY),
            initial,
            "moving a held pose must not retain the previous placement"
        );
        assert!(
            posed_world_bounds(&Aabb::EMPTY, &placement).is_empty(),
            "an empty uploaded pose has no draw bounds"
        );
        Ok(())
    }

    #[test]
    fn unsupported_rig_orientation_stays_two_sided() -> Result<(), String> {
        let mut model = mannequin_model()?;
        // Blender exports constant scale channels; the real asset correctly
        // stays conservative. Remove only those tracks for this rotation-only
        // control before adding reflected/singular/scaling counterexamples.
        assert!(!character_mesh_culling_safe(&model));
        for clip in &mut model.animations {
            clip.channels
                .retain(|channel| channel.path != crate::engine::gltf::AnimationPath::Scale);
        }
        assert!(
            character_mesh_culling_safe(&model),
            "the shipped mannequin has an orientation-preserving bind hierarchy"
        );
        let mut reflected = model.clone();
        reflected
            .skin
            .as_mut()
            .and_then(|skin| skin.inverse_bind.first_mut())
            .ok_or("mannequin inverse bind is missing")?
            .clone_from(&Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)));
        assert!(
            !character_mesh_culling_safe(&reflected),
            "a reflected inverse bind cannot use the single-sided pipeline"
        );
        let mut singular = model.clone();
        singular
            .nodes
            .first_mut()
            .ok_or("mannequin node is missing")?
            .scale = [0.0, 1.0, 1.0];
        assert!(
            !character_mesh_culling_safe(&singular),
            "a singular retained node must remain two-sided"
        );
        let mut scaling = model.clone();
        scaling
            .animations
            .first_mut()
            .and_then(|clip| clip.channels.first_mut())
            .ok_or("mannequin animation channel is missing")?
            .path = crate::engine::gltf::AnimationPath::Scale;
        assert!(
            !character_mesh_culling_safe(&scaling),
            "scale clips can cross a singular or reflected pose"
        );
        let mut morphed = model;
        morphed
            .morph_targets
            .push(crate::engine::gltf::PropMorphTarget {
                position: Vec::new(),
                normal: Vec::new(),
                tangent: Vec::new(),
            });
        assert!(
            !character_mesh_culling_safe(&morphed),
            "arbitrary morphs have no proven winding contract"
        );
        Ok(())
    }

    #[test]
    fn an_empty_scene_reports_nothing() {
        let characters = WgpuCharacters::default();
        assert_eq!(characters.character_count(), 0);
        assert_eq!(characters.stats().draws, 0);
        assert!(
            characters.submesh_double_sided(0, 0),
            "unknown materials remain two-sided"
        );
        assert_eq!(
            characters.culling_reflected(0),
            None,
            "unknown character orientation stays conservative"
        );
    }

    #[test]
    fn a_blended_character_material_selects_the_translucent_pass() {
        // The ghost's exported `alphaMode: BLEND` is what puts it in the
        // sorted translucent pass; cut-out keeps today's opaque treatment.
        assert_eq!(
            character_submesh_pass(MaterialAlpha::OPAQUE),
            BatchPass::Opaque
        );
        assert_eq!(
            character_submesh_pass(MaterialAlpha::blend(1.0)),
            BatchPass::Translucent
        );
        // A zero-opacity blend is invisible: it draws in neither pass.
        assert_eq!(
            character_submesh_pass(MaterialAlpha::blend(0.0)),
            BatchPass::Opaque
        );
        assert_eq!(
            character_submesh_pass(MaterialAlpha {
                mode: AlphaMode::Cutout,
                ..MaterialAlpha::OPAQUE
            }),
            BatchPass::Cutout
        );
    }
}
