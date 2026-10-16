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
//! Nothing here is created per frame: `sync_with_blacklight` writes the small per-object
//! uniforms and
//! reuses the buffers, materials and bind groups uploaded with the level.

use std::sync::Arc;

use super::environment::EnvironmentBindings;
use super::lightmap::LightmapAtlas;
use super::material::{EmissionRecord, GpuMaterial};
use super::texture::{CacheOutcome, GpuTexture, TextureCache};
use super::world::{EnvironmentUniform, WORLD_VERTEX_STRIDE, WorldVertex};
use crate::engine::materials::{MaterialEmission, TextureOrigin};
use crate::engine::quality::{QualityLevel, TextureClass};
use crate::engine::render::common::dynamic::{DynamicId, DynamicMesh, DynamicScene};
use crate::engine::render::common::dynamic_visibility::BlacklightReceiverCache;
use crate::engine::render::common::materials::BatchPass;
use crate::engine::spatial::Aabb;

/// One distinct model uploaded in model space.
struct DynamicMeshGpu {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    vertex_count: u32,
    index_count: u32,
    /// GPU sheet per model texture.
    textures: Vec<Arc<GpuTexture>>,
    uv_masks: Vec<Option<Arc<GpuTexture>>>,
    /// One entry per drawable primitive.
    submeshes: Vec<DynamicSubmeshGpu>,
}

/// One primitive of a dynamic model.
#[derive(Clone, Copy)]
struct DynamicSubmeshGpu {
    double_sided: bool,
    /// Shared scalar material response for every mesh route.
    response: crate::engine::materials::MaterialResponse,
    /// Index into [`DynamicMeshGpu::textures`].
    texture: usize,
    /// This primitive's own emission, before any object override.
    emission: MaterialEmission,
    /// The material's alpha contract.
    alpha: crate::engine::materials::MaterialAlpha,
    /// The draw pass the alpha contract puts this primitive in.
    pass: BatchPass,
    /// Index into the mesh's texture list of the emission mask, if authored.
    mask: Option<usize>,
    first_index: u32,
    index_count: u32,
}

/// One live object's GPU state.
struct DynamicObjectGpu {
    id: DynamicId,
    visible: bool,
    culling_reflected: Option<bool>,
    /// Slot into [`WgpuDynamic::meshes`].
    mesh: usize,
    /// This object's environment binding: model matrix + probe scale.
    environment: EnvironmentBindings,
    /// World-space bounds from the last [`WgpuDynamic::sync_with_blacklight`].
    world_bounds: Aabb,
    /// Indices into [`WgpuDynamic::materials`], one per mesh submesh.
    materials: Vec<usize>,
    /// True per submesh after the object's emission override is applied.
    emissive: Vec<bool>,
    emission: Option<MaterialEmission>,
    emission_scale: f32,
    uv_reactive: bool,
    uv_receiver: BlacklightReceiverCache,
}

/// What one dynamic upload or frame did, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
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
    pub caustics: &'a super::caustics::Caustics,
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
    pub lighting_level: QualityLevel,
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
                if super::material::ultraviolet_only_texture(
                    index,
                    mesh.submeshes.iter().map(|part| {
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
            let uv_masks = mesh
                .textures
                .iter()
                .enumerate()
                .map(|(index, image)| {
                    let slot = u32::try_from(index).unwrap_or(u32::MAX);
                    mesh.submeshes
                        .iter()
                        .any(|submesh| submesh.response.fluorescence.mask == Some(slot))
                        .then(|| {
                            super::material::upload_uv_mask(
                                ctx.device,
                                ctx.queue,
                                ctx.cache,
                                &format!("dynamic:{}:uv", mesh.model_path),
                                image,
                                ctx.level,
                            )
                        })
                })
                .collect();
            // Untextured primitives still draw with the asset-backed white
            // fallback; they must not borrow another primitive's texture.
            let white = textures.len();
            textures.push(ctx.cache.fallback());
            let submeshes: Vec<DynamicSubmeshGpu> = mesh
                .submeshes
                .iter()
                .map(|submesh| DynamicSubmeshGpu {
                    double_sided: submesh.double_sided,
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
                .push(Self::upload_mesh(ctx, mesh, textures, uv_masks, submeshes));
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
                ctx.caustics,
                &ctx.environment
                    .with_model(object.transform())
                    .with_entity_lighting(object.entity_lighting())
                    .with_spatial_lighting(object.spatial_lighting()),
            );
            let Some(mesh) = value.meshes.get(mesh_index) else {
                continue;
            };
            let mut materials = Vec::with_capacity(mesh.submeshes.len());
            let mut emissive = Vec::with_capacity(mesh.submeshes.len());
            for submesh in &mesh.submeshes {
                let emission = object.emission().unwrap_or(submesh.emission);
                let mask_texture = submesh
                    .mask
                    .and_then(|index| mesh.textures.get(index).cloned());
                let record = EmissionRecord::material(emission, mask_texture.is_some())
                    .with_scale(object.emission_scale());
                let mask = mask_texture.unwrap_or_else(|| ctx.cache.fallback());
                let uv_mask = submesh
                    .response
                    .fluorescence
                    .mask
                    .and_then(|index| {
                        mesh.uv_masks
                            .get(usize::try_from(index).unwrap_or(usize::MAX))
                    })
                    .and_then(Clone::clone)
                    .unwrap_or_else(|| ctx.cache.fallback());
                value.materials.push(GpuMaterial::model_with_fluorescence(
                    ctx.device,
                    ctx.queue,
                    ctx.material_layout,
                    ctx.cache,
                    &mask,
                    &uv_mask,
                    super::material::ModelMaterial {
                        albedo_tint: object.albedo_tint(),
                        emission: record,
                        alpha: submesh.alpha,
                        response: submesh.response,
                        response_enabled: ctx.lighting_level.draws_surface_response(),
                    },
                ));
                materials.push(value.materials.len().saturating_sub(1));
                emissive.push(object.emission_for(submesh.emission).is_emissive());
            }
            value.objects.push(DynamicObjectGpu {
                id: object.id(),
                visible: object.visible(),
                culling_reflected: super::world::culling_reflected(object.transform()),
                mesh: mesh_index,
                environment,
                world_bounds: object.world_bounds(),
                materials,
                emissive,
                emission: object.emission(),
                emission_scale: object.emission_scale(),
                uv_reactive: mesh
                    .submeshes
                    .iter()
                    .any(|submesh| submesh.response.fluorescence.strength > 0.0),
                uv_receiver: BlacklightReceiverCache::default(),
            });
        }
        value.finish_stats(scene);
        value
    }

    /// Updates a material variant without recreating model buffers, textures or
    /// bindings. Stable identity prevents a removed scene slot addressing a new
    /// object. The neutral scene remains the source for future level uploads.
    pub fn set_object_emission_scale(
        &mut self,
        queue: &wgpu::Queue,
        id: DynamicId,
        scale: f32,
    ) -> bool {
        if !scale.is_finite() || scale < 0.0 {
            return false;
        }
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        if object.emission_scale.to_bits() == scale.to_bits() {
            return true;
        }
        let Some(mesh) = self.meshes.get(object.mesh) else {
            return false;
        };
        for ((slot, emissive), submesh) in object
            .materials
            .iter()
            .zip(&mut object.emissive)
            .zip(&mesh.submeshes)
        {
            let Some(material) = self.materials.get(*slot) else {
                continue;
            };
            material.write_emission_scale(queue, scale);
            let emission = object.emission.unwrap_or(submesh.emission);
            *emissive = MaterialEmission {
                intensity: emission.intensity * scale,
                ..emission
            }
            .is_emissive();
        }
        object.emission_scale = scale;
        true
    }

    /// Uploads one model's buffers.
    fn upload_mesh(
        ctx: &DynamicUploadContext<'_>,
        mesh: &DynamicMesh,
        textures: Vec<Arc<GpuTexture>>,
        uv_masks: Vec<Option<Arc<GpuTexture>>>,
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
            uv_masks,
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
    pub fn sync_with_blacklight(
        &mut self,
        queue: &wgpu::Queue,
        scene: &DynamicScene,
        environment: &EnvironmentUniform,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) {
        for (slot, object) in self.objects.iter_mut().enumerate() {
            let Some(live) = scene.objects().get(slot) else {
                continue;
            };
            object.visible = live.visible();
            if !object.visible {
                continue;
            }
            let transform = live.transform();
            let mut uniform = environment
                .with_model(transform)
                .with_entity_lighting(live.entity_lighting())
                .with_spatial_lighting(live.spatial_lighting());
            uniform.uv_receiver = object.uv_receiver.sample(
                visibility.filter(|_| object.uv_reactive),
                live.mesh().bounds,
                transform,
                Some(live.id()),
            );
            let _update_stats = object.environment.update(queue, &uniform);
            object.world_bounds = crate::engine::render::common::props::transform_bounds(
                &live.mesh().bounds,
                &transform,
            );
            object.culling_reflected = super::world::culling_reflected(transform);
        }
        self.stats.draws = scene.draw_count();
        self.stats.vertices = scene.vertex_count();
    }

    /// Changes draw visibility without releasing any object or mesh resources.
    /// The next ordinary sync refreshes restored lighting and frame counters.
    pub fn set_object_visible(&mut self, slot: usize, visible: bool) -> bool {
        let Some(object) = self.objects.get_mut(slot) else {
            return false;
        };
        object.visible = visible;
        true
    }

    /// The exact uniform last sent for the scene slot.
    #[must_use]
    pub fn diagnostic_uniform(&self, slot: usize) -> Option<&EnvironmentUniform> {
        self.objects
            .get(slot)
            .map(|object| object.environment.uploaded_uniform())
    }

    /// Lighting/model values in the exact CPU mirror of the last GPU upload.
    /// No command submission or GPU readback occurs on this diagnostic path.
    #[must_use]
    pub fn diagnostic_lighting_snapshot(&self, slot: usize) -> Option<serde_json::Value> {
        let uniform = self.diagnostic_uniform(slot)?;
        Some(serde_json::json!({
            "visible": self.objects.get(slot)?.visible,
            "model": uniform.model,
            "normal_model": uniform.normal_model,
            "entity_irradiance": uniform.entity_irradiance,
            "entity_moment": uniform.entity_moment,
            "light_scale": uniform.light_scale,
            "entity_bounds_min": uniform.entity_bounds_min,
            "entity_bounds_extent": uniform.entity_bounds_extent,
            "entity_anchors": uniform.entity_anchors,
            "entity_direct": uniform.entity_direct,
            "lightmap_switchable": uniform.lightmap_switchable,
            "lightmap_enabled": uniform.lightmap_enabled,
        }))
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
            .filter(|gpu_object| gpu_object.visible)
            .map(|gpu_object| gpu_object.environment.bind_group())
    }

    /// One object's world bounds.
    #[must_use]
    pub fn world_bounds(&self, object: usize) -> Option<Aabb> {
        self.objects
            .get(object)
            .filter(|gpu_object| gpu_object.visible)
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
            .filter(|gpu_object| gpu_object.visible)
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

    /// Unknown submesh metadata preserves two-sided rendering.
    pub fn submesh_double_sided(&self, object: usize, submesh: usize) -> bool {
        self.objects
            .get(object)
            .and_then(|entry| self.meshes.get(entry.mesh))
            .and_then(|mesh| mesh.submeshes.get(submesh))
            .is_none_or(|part| part.double_sided)
    }

    pub fn culling_reflected(&self, object: usize) -> Option<bool> {
        self.objects
            .get(object)
            .and_then(|entry| entry.culling_reflected)
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
