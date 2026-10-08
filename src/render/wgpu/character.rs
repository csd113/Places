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
use crate::materials::TextureOrigin;
use crate::quality::{QualityLevel, TextureClass};
use crate::render::common::character::{Character, CharacterScene, character_vertex};
use crate::render::common::materials::BatchPass;
use crate::spatial::Aabb;

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
}

/// The pass one source submesh's alpha contract maps to for a character.
///
/// A `MASK` (cut-out) material keeps the character path's historical opaque
/// treatment; a `BLEND` material (a glTF `alphaMode: "BLEND"`, exported for the
/// sheet ghost) draws in the sorted translucent character pass. A zero-opacity
/// blend is invisible and draws in neither. Pure, so the classification is
/// unit-testable without a device.
#[must_use]
const fn character_submesh_pass(alpha: crate::materials::MaterialAlpha) -> BatchPass {
    BatchPass::of(alpha)
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
    /// A `MASK` (cut-out) material keeps the character path's historical
    /// opaque treatment; a `BLEND` material (the sheet ghost's exported
    /// `alphaMode`) selects the sorted translucent character pass.
    pass: BatchPass,
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
    /// World-space culling bounds from the last applied transform.
    world_bounds: Aabb,
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
    /// Reused CPU skinning target; capacity covers the largest character.
    scratch: Vec<WorldVertex>,
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
    pub lightmaps: &'a LightmapAtlas,
    /// The level environment every character's uniform starts from: unit light
    /// scale, the resident lightmap selection and the fog constants.
    pub environment: EnvironmentUniform,
    pub probes: &'a [&'a wgpu::TextureView],
    pub planar: &'a wgpu::TextureView,
    pub probe_fallback: &'a wgpu::TextureView,
    pub planar_fallback: &'a wgpu::TextureView,
    pub level: QualityLevel,
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
                &ctx.environment
                    .with_model(character.transform())
                    .with_entity_lighting(Some(character.entity_lighting()))
                    .with_opacity(character.opacity()),
            );
            Self::fill_vertices(character, &mut value.scratch);
            ctx.queue
                .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&value.scratch));
            value.characters.push(CharacterGpu {
                mesh: mesh_index,
                key,
                vertex_buffer,
                environment,
                uploaded_revision: character.animator().revision(),
                uploaded_transform: character.transform(),
                world_bounds: character.world_bounds(),
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
        asset: &crate::props::LoadedPropAsset,
    ) -> Option<CharacterMeshGpu> {
        let model = &asset.model;
        if model.indices.is_empty() || model.submeshes.is_empty() {
            return None;
        }
        let mut textures: Vec<Arc<GpuTexture>> = Vec::with_capacity(model.textures.len());
        for (index, image) in model.textures.iter().enumerate() {
            let logical = super::texture::image_content_key(
                &format!("character:{}:{index}", asset.model_path),
                image,
            );
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
        let mut submeshes: Vec<CharacterSubmeshGpu> = Vec::with_capacity(model.submeshes.len());
        let mut materials: Vec<usize> = Vec::with_capacity(model.submeshes.len());
        let mut emissive: Vec<bool> = Vec::with_capacity(model.submeshes.len());
        for source in model
            .submeshes
            .iter()
            .filter(|submesh| submesh.index_count > 0)
        {
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
            value.materials.push(GpuMaterial::model_with_alpha(
                ctx.device,
                ctx.queue,
                ctx.material_layout,
                ctx.cache,
                &mask_texture,
                super::material::ModelMaterial {
                    emission: record,
                    alpha: source.alpha,
                    response: source.response,
                    response_enabled: ctx.level.draws_surface_response(),
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
        })
    }

    /// Fills the reusable scratch buffer with one character's posed vertices.
    ///
    /// Skins in model space with the animator's current deltas and attaches
    /// material albedo; no allocation once the buffer has been sized.
    fn fill_vertices(character: &Character, out: &mut Vec<WorldVertex>) {
        let model = &character.asset().model;
        out.clear();
        for (index, vertex) in model.vertices.iter().enumerate() {
            let joints = model.joints.get(index).copied().unwrap_or([0; 4]);
            let weights = model.weights.get(index).copied().unwrap_or([0.0; 4]);
            let position = character
                .animator()
                .skin_position(joints, weights, vertex.pos);
            let albedo = character.albedo().get(index).copied().unwrap_or([1.0; 4]);
            let mut posed = character_vertex(albedo, vertex.uv, position);
            posed.normal = vertex.normal.map_or([0.0; 3], |normal| {
                character.animator().skin_normal(joints, weights, normal)
            });
            out.push(WorldVertex::from(posed));
        }
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
    pub fn sync(
        &mut self,
        queue: &wgpu::Queue,
        scene: &CharacterScene,
        environment: &EnvironmentUniform,
    ) -> usize {
        let mut uploaded = 0usize;
        for gpu in &mut self.characters {
            let character = match &gpu.key {
                CharacterKey::Placed(slot) => scene.characters().get(*slot),
                CharacterKey::Runtime(instance_id) => scene.runtime_character(instance_id),
            };
            let Some(scene_character) = character else {
                continue;
            };
            // A fade is a per-frame environment write, so the comparison also
            // covers opacity: a still, settled pose writes nothing, while a
            // fading ghost rewrites only its own uniform.
            let transform_changed = scene_character.transform() != gpu.uploaded_transform;

            let _update_stats = gpu.environment.update(
                queue,
                &environment
                    .with_model(scene_character.transform())
                    .with_entity_lighting(Some(scene_character.entity_lighting()))
                    .with_opacity(scene_character.opacity()),
            );
            gpu.uploaded_transform = scene_character.transform();
            if transform_changed {
                gpu.world_bounds = scene_character.world_bounds();
            }
            if scene_character.animator().revision() == gpu.uploaded_revision {
                continue;
            }
            Self::fill_vertices(scene_character, &mut self.scratch);
            queue.write_buffer(&gpu.vertex_buffer, 0, bytemuck::cast_slice(&self.scratch));
            gpu.uploaded_revision = scene_character.animator().revision();
            uploaded = uploaded.saturating_add(1);
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
    use crate::materials::{AlphaMode, MaterialAlpha};

    #[test]
    fn an_empty_scene_reports_nothing() {
        let characters = WgpuCharacters::default();
        assert_eq!(characters.character_count(), 0);
        assert_eq!(characters.stats().draws, 0);
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
