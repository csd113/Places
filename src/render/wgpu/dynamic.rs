//! Dynamic objects: the washer-drum demonstration on wgpu.
//!
//! The renderer-neutral [`DynamicScene`] owns the model-space meshes, their
//! transforms and their baked-light probes; this module owns the GPU side:
//!
//! * one vertex/index buffer pair per distinct model (model space, uploaded
//!   once; the colour is albedo/tint only, never a baked light);
//! * one clamped GPU sheet per model texture, cached like a prop's;
//! * one plain-opaque GPU material per `(object, submesh)` because an object can
//!   override its emission while sharing the mesh with another object;
//! * one group-3 environment per object, carrying the object's model matrix and
//!   its per-frame linear irradiance and directional moment, so a moving object is
//!   lit coherently without touching its vertex buffer.
//!
//! Nothing here is created per frame: `sync` writes the small per-object
//! uniforms and
//! reuses the buffers, materials and bind groups uploaded with the level.

use std::sync::Arc;

use super::environment::EnvironmentBindings;
use super::lightmap::LightmapAtlas;
use super::material::{EmissionRecord, GpuMaterial};
use super::texture::{CacheOutcome, GpuTexture, TextureCache};
use super::world::{EnvironmentUniform, WORLD_VERTEX_STRIDE, WorldVertex};
use crate::materials::{MaterialEmission, TextureOrigin};
use crate::quality::{QualityLevel, TextureClass};
use crate::render::common::dynamic::{DynamicMesh, DynamicScene};
use crate::render::common::materials::BatchPass;
use crate::spatial::Aabb;

/// One distinct model uploaded in model space.
struct DynamicMeshGpu {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    vertex_count: u32,
    index_count: u32,
    /// GPU sheet per model texture.
    textures: Vec<Arc<GpuTexture>>,
    /// One entry per drawable primitive.
    submeshes: Vec<DynamicSubmeshGpu>,
}

/// One primitive of a dynamic model.
#[derive(Clone, Copy)]
struct DynamicSubmeshGpu {
    /// Shared scalar material response for every mesh route.
    response: crate::materials::MaterialResponse,
    /// Index into [`DynamicMeshGpu::textures`].
    texture: usize,
    /// This primitive's own emission, before any object override.
    emission: MaterialEmission,
    /// The material's alpha contract.
    alpha: crate::materials::MaterialAlpha,
    /// The draw pass the alpha contract puts this primitive in.
    pass: BatchPass,
    /// Index into the mesh's texture list of the emission mask, if authored.
    mask: Option<usize>,
    first_index: u32,
    index_count: u32,
}

/// One live object's GPU state.
struct DynamicObjectGpu {
    /// Slot into [`WgpuDynamic::meshes`].
    mesh: usize,
    /// This object's environment binding: model matrix + probe scale.
    environment: EnvironmentBindings,
    /// World-space bounds from the last [`WgpuDynamic::sync`].
    world_bounds: Aabb,
    /// Indices into [`WgpuDynamic::materials`], one per mesh submesh.
    materials: Vec<usize>,
    /// True per submesh after the object's emission override is applied.
    emissive: Vec<bool>,
}

/// What one dynamic upload or frame did, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicGpuStats {
    /// Distinct models uploaded.
    pub meshes: usize,
    /// Live objects.
    pub objects: usize,
    /// Draw calls one frame submits (one per object per primitive).
    pub draws: usize,
    /// Distinct vertices the scene draws, summed over objects.
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

/// Every dynamic object the level draws.
#[derive(Default)]
pub struct WgpuDynamic {
    meshes: Vec<DynamicMeshGpu>,
    objects: Vec<DynamicObjectGpu>,
    materials: Vec<GpuMaterial>,
    stats: DynamicGpuStats,
}

/// The renderer state a dynamic upload needs: the shared texture cache, the
/// material and environment layouts, the level's lightmap atlas, the level's
/// environment template (lightmap selection and fog) and the reflection views
/// the environment binds.
pub struct DynamicUploadContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub cache: &'a mut TextureCache,
    pub material_layout: &'a wgpu::BindGroupLayout,
    pub environment_layout: &'a wgpu::BindGroupLayout,
    pub lightmaps: &'a LightmapAtlas,
    /// The level environment every object's uniform starts from: unit light
    /// scale, the resident lightmap selection and the fog constants.
    pub environment: EnvironmentUniform,
    /// The level's probe cubemaps, in bake order.
    pub probes: &'a [&'a wgpu::TextureView],
    pub planar: &'a wgpu::TextureView,
    pub probe_fallback: &'a wgpu::TextureView,
    pub planar_fallback: &'a wgpu::TextureView,
    pub level: QualityLevel,
}

impl WgpuDynamic {
    /// Uploads every mesh and object of one scene.
    ///
    /// Called after the level's static resources exist, once per dynamic
    /// spawn. An empty scene produces an empty value.
    #[must_use]
    // One cohesive level upload: the per-model loop and the per-object loop
    // share the same counters and context.
    #[expect(
        clippy::too_many_lines,
        reason = "One cohesive level upload: the per-model loop and the per-object loop share the same counters and context."
    )]
    pub fn upload(ctx: &mut DynamicUploadContext<'_>, scene: &DynamicScene) -> Self {
        let mut value = Self::default();
        if scene.is_empty() {
            return value;
        }
        for mesh in scene.meshes() {
            let mut textures: Vec<Arc<GpuTexture>> = Vec::with_capacity(mesh.textures.len());
            for (index, image) in mesh.textures.iter().enumerate() {
                let logical = format!("dynamic:{}:{}", mesh.model_path, index);
                let (outcome, texture) = ctx.cache.get_or_upload_fitted(
                    ctx.device,
                    ctx.queue,
                    &logical,
                    image.as_ref(),
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
            // Untextured primitives still draw with the asset-backed white
            // fallback; they must not borrow another primitive's texture.
            let white = textures.len();
            textures.push(ctx.cache.fallback());
            let submeshes: Vec<DynamicSubmeshGpu> = mesh
                .submeshes
                .iter()
                .map(|submesh| DynamicSubmeshGpu {
                    response: submesh.response,
                    texture: submesh.texture.map_or(white, usize::from).min(white),
                    emission: submesh.emission,
                    alpha: submesh.alpha,
                    pass: BatchPass::of(submesh.alpha),
                    mask: submesh
                        .emission
                        .mask
                        .map(|index| usize::try_from(index).unwrap_or(usize::MAX))
                        .filter(|index| *index < white),
                    first_index: submesh.first_index,
                    index_count: submesh.index_count,
                })
                .collect();
            value
                .meshes
                .push(Self::upload_mesh(ctx, mesh, textures, submeshes));
        }
        for object in scene.objects() {
            let Some(mesh_index) =
                Some(object.mesh_index()).filter(|index| *index < value.meshes.len())
            else {
                continue;
            };
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
                    .with_model(object.transform())
                    .with_entity_lighting(object.entity_lighting()),
            );
            let Some(mesh) = value.meshes.get(mesh_index) else {
                continue;
            };
            let mut materials = Vec::with_capacity(mesh.submeshes.len());
            let mut emissive = Vec::with_capacity(mesh.submeshes.len());
            for submesh in &mesh.submeshes {
                let emission = object.emission_for(submesh.emission);
                let mask_texture = submesh
                    .mask
                    .and_then(|index| mesh.textures.get(index).cloned());
                let record = EmissionRecord::material(emission, mask_texture.is_some());
                let mask = mask_texture.unwrap_or_else(|| ctx.cache.fallback());
                value.materials.push(GpuMaterial::model_with_alpha(
                    ctx.device,
                    ctx.queue,
                    ctx.material_layout,
                    ctx.cache,
                    &mask,
                    super::material::ModelMaterial {
                        emission: record,
                        alpha: submesh.alpha,
                        response: submesh.response,
                        response_enabled: ctx.level.draws_surface_response(),
                    },
                ));
                materials.push(value.materials.len().saturating_sub(1));
                emissive.push(record.is_emissive());
            }
            value.objects.push(DynamicObjectGpu {
                mesh: mesh_index,
                environment,
                world_bounds: object.world_bounds(),
                materials,
                emissive,
            });
        }
        value.finish_stats(scene);
        value
    }

    /// Uploads one model's buffers.
    fn upload_mesh(
        ctx: &DynamicUploadContext<'_>,
        mesh: &DynamicMesh,
        textures: Vec<Arc<GpuTexture>>,
        submeshes: Vec<DynamicSubmeshGpu>,
    ) -> DynamicMeshGpu {
        let vertices: Vec<WorldVertex> = mesh
            .vertices
            .iter()
            .map(|vertex| WorldVertex::from(*vertex))
            .collect();
        let vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-dynamic-vertices"),
            size: u64::try_from(vertices.len())
                .unwrap_or(u64::MAX)
                .saturating_mul(WORLD_VERTEX_STRIDE)
                .max(4),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            ctx.queue
                .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        let index_buffer = super::world::upload_index_buffer(
            ctx.device,
            &mesh.indices,
            "places-wgpu-dynamic-indices",
        );
        DynamicMeshGpu {
            vertex_buffer,
            index_buffer,
            vertex_count: u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX),
            index_count: u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX),
            textures,
            submeshes,
        }
    }

    /// Fills the per-frame counters.
    fn finish_stats(&mut self, scene: &DynamicScene) {
        self.stats.meshes = self.meshes.len();
        self.stats.objects = self.objects.len();
        self.stats.draws = scene.draw_count();
        self.stats.vertices = scene.vertex_count();
        let vertex_stride = usize::try_from(WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX);
        self.stats.vertex_bytes = self
            .meshes
            .iter()
            .map(|mesh| {
                usize::try_from(mesh.vertex_count)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(vertex_stride)
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

    /// Writes every object's transform and baked-light probe into its
    /// environment uniform, and refreshes the world bounds the cull reads.
    ///
    /// `environment` is the level's current template (the lightmap selection,
    /// the fog constants and no active mirror); each object installs its model
    /// matrix and probe scale on top, so a light switch reaches the objects'
    /// uniforms the next time they sync. Called once per frame; a still object
    /// writes nothing (the uniform compares equal).
    pub fn sync(
        &mut self,
        queue: &wgpu::Queue,
        scene: &DynamicScene,
        environment: &EnvironmentUniform,
    ) {
        for (slot, object) in self.objects.iter_mut().enumerate() {
            let Some(live) = scene.objects().get(slot) else {
                continue;
            };
            let uniform = environment
                .with_model(live.transform())
                .with_entity_lighting(live.entity_lighting());
            let _update_stats = object.environment.update(queue, &uniform);
            object.world_bounds = live.world_bounds();
        }
    }

    /// The exact uniform last sent for the scene slot.
    pub fn diagnostic_uniform(&self, slot: usize) -> Option<&EnvironmentUniform> {
        self.objects
            .get(slot)
            .map(|o| o.environment.uploaded_uniform())
    }

    /// The upload and frame counters.
    #[must_use]
    pub const fn stats(&self) -> DynamicGpuStats {
        self.stats
    }

    /// One object's environment binding.
    #[must_use]
    pub fn environment(&self, object: usize) -> Option<&wgpu::BindGroup> {
        self.objects
            .get(object)
            .map(|gpu_object| gpu_object.environment.bind_group())
    }

    /// One object's world bounds.
    #[must_use]
    pub fn world_bounds(&self, object: usize) -> Option<Aabb> {
        self.objects
            .get(object)
            .map(|gpu_object| gpu_object.world_bounds)
    }

    /// One object's geometry: vertex buffer, index buffer, index range of one
    /// submesh.
    #[must_use]
    pub fn geometry(
        &self,
        object: usize,
        submesh: usize,
    ) -> Option<(&wgpu::Buffer, &wgpu::Buffer, u32, u32)> {
        let gpu_object = self.objects.get(object)?;
        let mesh = self.meshes.get(gpu_object.mesh)?;
        let mesh_part = mesh.submeshes.get(submesh)?;
        Some((
            &mesh.vertex_buffer,
            &mesh.index_buffer,
            mesh_part.first_index,
            mesh_part.index_count,
        ))
    }

    /// The number of submeshes one object draws.
    #[must_use]
    pub fn submesh_count(&self, object: usize) -> usize {
        self.objects
            .get(object)
            .and_then(|gpu_object| self.meshes.get(gpu_object.mesh))
            .map_or(0, |mesh| mesh.submeshes.len())
    }

    /// The number of live objects.
    #[must_use]
    pub const fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// The draw pass one object's submesh belongs to.
    #[must_use]
    pub fn submesh_pass(&self, object: usize, submesh: usize) -> Option<BatchPass> {
        let gpu_object = self.objects.get(object)?;
        let mesh = self.meshes.get(gpu_object.mesh)?;
        mesh.submeshes.get(submesh).map(|mesh_part| mesh_part.pass)
    }

    /// True when the object's submesh emits.
    #[must_use]
    pub fn submesh_emissive(&self, object: usize, submesh: usize) -> bool {
        self.objects
            .get(object)
            .and_then(|gpu_object| gpu_object.emissive.get(submesh))
            .copied()
            .unwrap_or(false)
    }

    /// The material slot of one object's submesh.
    #[must_use]
    pub fn material_slot(&self, object: usize, submesh: usize) -> Option<usize> {
        self.objects.get(object)?.materials.get(submesh).copied()
    }

    /// The GPU material of one slot.
    #[must_use]
    pub fn material(&self, slot: usize) -> Option<&GpuMaterial> {
        self.materials.get(slot)
    }

    /// The texture of one object's submesh.
    #[must_use]
    pub fn submesh_texture(&self, object: usize, submesh: usize) -> Option<&GpuTexture> {
        let gpu_object = self.objects.get(object)?;
        let mesh = self.meshes.get(gpu_object.mesh)?;
        let mesh_part = mesh.submeshes.get(submesh)?;
        mesh.textures.get(mesh_part.texture).map(Arc::as_ref)
    }

    /// The distinct vertices one object indexes.
    #[must_use]
    pub fn object_vertex_count(&self, object: usize) -> usize {
        let Some(gpu_object) = self.objects.get(object) else {
            return 0;
        };
        self.meshes.get(gpu_object.mesh).map_or(0, |mesh| {
            usize::try_from(mesh.vertex_count).unwrap_or(usize::MAX)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::float_cmp,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    #[test]
    fn an_empty_scene_reports_nothing() {
        let dynamic = WgpuDynamic::default();
        assert_eq!(dynamic.object_count(), 0);
        assert_eq!(dynamic.stats().draws, 0);
    }
}
