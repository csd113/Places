//! Compatible BLEND characters share a sorted GPU stream. Each vertex retains
//! its exact instance environment and primitive emission; no artwork is baked
//! into colours and no alpha/material contract changes. Incompatible models and
//! other models retain the general triangle path. The batch uses five groups.
use super::{
    CharacterGpu, CharacterMeshGpu, CharacterScene, CharacterSpan, CharacterUploadContext,
    EmissionRecord, EnvironmentUniform, WORLD_VERTEX_STRIDE, WgpuCharacters,
};
use wgpu::util::DeviceExt as _;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlendVertex {
    actor: u32,
    emission: [f32; 3],
}

#[derive(Clone, Copy)]
struct BatchTriangle {
    indices: [u32; 3],
    distance: f32,
    ordinal: usize,
    character: usize,
}

type CopiedPose = (usize, Option<u64>, u64);

fn pose_copy_needed(
    prepared: Option<u64>,
    copied: Option<CopiedPose>,
    current: CopiedPose,
) -> bool {
    prepared.is_some() && copied != Some(current)
}

pub(in crate::engine::render::wgpu) struct BlendBatch {
    members: Vec<usize>,
    births: Vec<u64>,
    copied_poses: Vec<Option<CopiedPose>>,
    #[cfg(feature = "visual-diagnostics")]
    copied_bytes: u64,
    #[cfg(feature = "visual-diagnostics")]
    environment_uploads: u64,
    prepared_orders: Vec<Option<u64>>,
    order_initialized: bool,
    pub vertices: wgpu::Buffer,
    pub attributes: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    environments: wgpu::Buffer,
    pub binding: wgpu::BindGroup,
    triangles: Vec<BatchTriangle>,
    ordered_indices: Vec<u32>,
    uniforms: Vec<EnvironmentUniform>,
    pub visible_vertices: usize,
    pub emissive: bool,
}

/// Only emission may differ. Shared vertex identities with conflicting
/// emissions reject batching rather than silently changing either primitive.
fn vertex_emission(model: &crate::engine::gltf::PropModel) -> Option<Vec<[f32; 3]>> {
    let first = model.submeshes.first()?;
    if first.alpha.mode != crate::engine::materials::AlphaMode::Blend
        || first.response.specular.iter().any(|channel| *channel > 0.0)
        || first.response.fluorescence.strength > 0.0
        || first.emission.mask.is_some()
    {
        return None;
    }
    let mut values: Vec<Option<[f32; 3]>> = vec![None; model.vertices.len()];
    for part in &model.submeshes {
        if part.alpha != first.alpha
            || part.texture != first.texture
            || part.response != first.response
            || part.emission.mask.is_some()
        {
            return None;
        }
        let colour = EmissionRecord::material(part.emission, false).color;
        let start = usize::try_from(part.first_index).ok()?;
        let end = start.checked_add(usize::try_from(part.index_count).ok()?)?;
        for index in model.indices.get(start..end)? {
            let slot = values.get_mut(usize::from(*index))?;
            if slot.is_some_and(|previous| previous.map(f32::to_bits) != colour.map(f32::to_bits)) {
                return None;
            }
            *slot = Some(colour);
        }
    }
    Some(
        values
            .into_iter()
            .map(|value| value.unwrap_or([0.0; 3]))
            .collect(),
    )
}

pub(in crate::engine::render::wgpu) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    super::super::world::environment_storage_layout(device)
}

fn batch_binding(ctx: &CharacterUploadContext<'_>, environments: &wgpu::Buffer) -> wgpu::BindGroup {
    use super::super::texture::SamplerPolicy;
    super::super::world::environment_bind_group(
        ctx.device,
        &layout(ctx.device),
        environments,
        ctx.lightmaps,
        ctx.cache.sampler(SamplerPolicy::ClampLinear),
        ctx.probe_fallback,
        ctx.planar_fallback,
        ctx.cache.sampler(SamplerPolicy::ClampLinear),
        ctx.caustics,
        ctx.cache.sampler(SamplerPolicy::RepeatLow),
    )
}

pub(in crate::engine::render::wgpu) const fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![11 => Uint32, 12 => Float32x3];
    wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

impl BlendBatch {
    fn new(
        ctx: &CharacterUploadContext<'_>,
        characters: &[CharacterGpu],
        members: Vec<usize>,
        emission: &[[f32; 3]],
        indices: usize,
        remaining: u64,
    ) -> Option<Self> {
        let _first = characters.get(*members.first()?)?;
        let count = emission.len().checked_mul(members.len())?;
        let vertex_bytes = u64::try_from(count)
            .ok()?
            .checked_mul(WORLD_VERTEX_STRIDE)?;
        let attribute_bytes = u64::try_from(count).ok()?.checked_mul(16)?;
        let index_bytes = u64::try_from(indices.checked_mul(members.len())?)
            .ok()?
            .checked_mul(4)?;
        let uniform_bytes = u64::try_from(members.len())
            .ok()?
            .checked_mul(super::super::buffer_element_bytes::<EnvironmentUniform>())?;
        let total = vertex_bytes
            .checked_add(attribute_bytes)?
            .checked_add(index_bytes)?
            .checked_add(uniform_bytes)?;
        let limits = ctx.device.limits();
        if total > remaining
            || [vertex_bytes, attribute_bytes, index_bytes, uniform_bytes]
                .iter()
                .any(|size| *size > limits.max_buffer_size)
            || uniform_bytes > limits.max_storage_buffer_binding_size
        {
            return None;
        }
        let mut attributes = Vec::with_capacity(count);
        let mut uniforms = Vec::with_capacity(members.len());
        for (actor, member) in members.iter().enumerate() {
            let actor_id = u32::try_from(actor).ok()?;
            attributes.extend(emission.iter().map(|colour| BlendVertex {
                actor: actor_id,
                emission: *colour,
            }));
            uniforms.push(*characters.get(*member)?.environment.uploaded_uniform());
        }
        let environments = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("places-character-blend-environments"),
                contents: bytemuck::cast_slice(&uniforms),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let binding = batch_binding(ctx, &environments);
        let allocate = |label, size, usage| {
            ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let copied_poses = vec![None; members.len()];
        let prepared_orders = vec![None; members.len()];
        Some(Self {
            births: members
                .iter()
                .filter_map(|slot| characters.get(*slot).map(|entry| entry.birth))
                .collect(),
            members,
            copied_poses,
            #[cfg(feature = "visual-diagnostics")]
            copied_bytes: 0,
            #[cfg(feature = "visual-diagnostics")]
            environment_uploads: 0,
            prepared_orders,
            order_initialized: false,
            vertices: allocate(
                "places-character-blend-vertices",
                vertex_bytes,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ),
            attributes: ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("places-character-blend-attributes"),
                    contents: bytemuck::cast_slice(&attributes),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            // Triangle count never exceeds the admitted index topology. Allocate
            // from the model at install; no later view may grow these buffers.
            indices: allocate(
                "places-character-blend-indices",
                index_bytes,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ),
            environments,
            binding,
            triangles: Vec::new(),
            ordered_indices: Vec::new(),
            uniforms,
            visible_vertices: 0,
            emissive: emission.iter().flatten().any(|channel| *channel > 0.0),
        })
    }

    fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        characters: &[CharacterGpu],
        meshes: &[CharacterMeshGpu],
        frame: &super::super::world::WorldFrame,
        cull: bool,
    ) {
        let mut order_changed = !self.order_initialized;
        let mut uniforms_changed = false;
        for (actor, index) in self.members.iter().enumerate() {
            let entry = characters.get(*index);
            let key = entry
                .filter(|character| {
                    character.visible && (!cull || frame.visible(&character.world_bounds))
                })
                .and_then(|character| character.transparency.as_ref())
                .map(|state| state.order_revision);
            if let Some(previous) = self.prepared_orders.get_mut(actor) {
                order_changed |= *previous != key;
                *previous = key;
            }
            if let (Some(character), Some(uniform)) = (
                entry.filter(|_| key.is_some()),
                self.uniforms.get_mut(actor),
            ) {
                let current = character.environment.uploaded_uniform();
                if bytemuck::bytes_of(uniform) != bytemuck::bytes_of(current) {
                    *uniform = *current;
                    uniforms_changed = true;
                }
            }
        }
        if uniforms_changed {
            #[cfg(feature = "visual-diagnostics")]
            {
                self.environment_uploads = self.environment_uploads.saturating_add(1);
            }
            queue.write_buffer(&self.environments, 0, bytemuck::cast_slice(&self.uniforms));
        }
        if !order_changed {
            return;
        }
        self.order_initialized = true;
        self.triangles.clear();
        self.visible_vertices = 0;
        for (actor, index) in self.members.iter().enumerate() {
            let Some(character) = characters.get(*index) else {
                continue;
            };
            if !character.visible || (cull && !frame.visible(&character.world_bounds)) {
                continue;
            }
            let (Some(mesh), Some(state)) = (meshes.get(character.mesh), &character.transparency)
            else {
                continue;
            };
            let base = u32::try_from(actor)
                .unwrap_or(u32::MAX)
                .saturating_mul(mesh.vertex_count);
            self.visible_vertices = self
                .visible_vertices
                .saturating_add(usize::try_from(mesh.vertex_count).unwrap_or(usize::MAX));
            for triangle in &state.order {
                let Some(source) = mesh.blend_triangles.get(triangle.source) else {
                    continue;
                };
                self.triangles.push(BatchTriangle {
                    indices: source
                        .indices
                        .map(|vertex| base.saturating_add(u32::from(vertex))),
                    distance: triangle.distance,
                    ordinal: actor
                        .saturating_mul(mesh.blend_triangles.len())
                        .saturating_add(triangle.source),
                    character: *index,
                });
            }
        }
        self.triangles.sort_unstable_by(|left, right| {
            right
                .distance
                .total_cmp(&left.distance)
                .then_with(|| left.ordinal.cmp(&right.ordinal))
        });
        self.ordered_indices.clear();
        self.ordered_indices
            .extend(self.triangles.iter().flat_map(|triangle| triangle.indices));
        if !self.ordered_indices.is_empty() {
            queue.write_buffer(
                &self.indices,
                0,
                bytemuck::cast_slice(&self.ordered_indices),
            );
        }
    }

    fn copy_poses(&mut self, encoder: &mut wgpu::CommandEncoder, characters: &[CharacterGpu]) {
        let stride = self
            .members
            .first()
            .and_then(|index| characters.get(*index))
            .map_or(0, |entry| entry.vertex_buffer.size());
        for (slot, member) in self.members.iter().enumerate() {
            let Some(entry) = characters.get(*member) else {
                continue;
            };
            // Only the view-prepared triangle stream consumes this batch slot.
            // Keep its copied key stale while absent so the first return catches up.
            if self.prepared_orders.get(slot).is_none_or(Option::is_none) {
                continue;
            }
            let owner_index = entry.shared_pose.unwrap_or(*member);
            if let Some(owner) = characters.get(owner_index) {
                let key = (owner_index, owner.pose.revision, owner.pose.albedo_revision);
                if !pose_copy_needed(
                    self.prepared_orders.get(slot).copied().flatten(),
                    self.copied_poses.get(slot).copied().flatten(),
                    key,
                ) {
                    continue;
                }
                if let Some(copied) = self.copied_poses.get_mut(slot) {
                    *copied = Some(key);
                }
                #[cfg(feature = "visual-diagnostics")]
                {
                    self.copied_bytes = self.copied_bytes.saturating_add(stride);
                }
                encoder.copy_buffer_to_buffer(
                    &owner.vertex_buffer,
                    0,
                    &self.vertices,
                    u64::try_from(slot)
                        .unwrap_or(u64::MAX)
                        .saturating_mul(stride),
                    stride,
                );
            }
        }
    }

    fn gpu_bytes(&self) -> u64 {
        self.vertices
            .size()
            .saturating_add(self.attributes.size())
            .saturating_add(self.indices.size())
            .saturating_add(self.environments.size())
    }

    #[cfg(feature = "visual-diagnostics")]
    const fn cpu_bytes(&self) -> usize {
        self.triangles
            .capacity()
            .saturating_mul(std::mem::size_of::<BatchTriangle>())
            .saturating_add(
                self.members
                    .capacity()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
            .saturating_add(
                self.copied_poses
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Option<(usize, Option<u64>, u64)>>()),
            )
            .saturating_add(self.ordered_indices.capacity().saturating_mul(4))
            .saturating_add(
                self.uniforms
                    .capacity()
                    .saturating_mul(std::mem::size_of::<EnvironmentUniform>()),
            )
    }
}

/// Conservative prewarm admission; final GPU batch/residency admission still
/// uses the existing upload path. Unavailable shader adaptation keeps its
/// established general-triangle fallback rather than retrying a worker forever.
pub(in crate::engine::render::wgpu) fn preparation_needed(scene: &CharacterScene) -> bool {
    if !batching_available() {
        return false;
    }
    scene
        .characters()
        .iter()
        .chain(scene.runtime_characters())
        .any(|character| {
            character.texture().is_none() && vertex_emission(&character.asset().model).is_some()
        })
}

fn batching_available() -> bool {
    shader_source().is_some()
        && !(std::env::var("PLACES_BENCH").as_deref() == Ok("1")
            && std::env::var("PLACES_BENCH_CHARACTER_BATCH").as_deref() == Ok("off"))
}

impl WgpuCharacters {
    pub(super) fn upload_blend_batches(
        &mut self,
        ctx: &CharacterUploadContext<'_>,
        scene: &CharacterScene,
    ) {
        if !batching_available() {
            return;
        }
        // Additional batching residency is capped across the complete scene.
        // Admission over this budget retains correct general triangle draws.
        let mut remaining = 64_u64 * 1024 * 1024;
        let mut previous = std::mem::take(&mut self.blend_batches);
        for mesh in 0..self.meshes.len() {
            let members: Vec<_> = self
                .characters
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.mesh == mesh && entry.texture.is_none())
                .map(|(index, _)| index)
                .collect();
            let Some(character) = members
                .first()
                .and_then(|index| self.characters.get(*index))
                .and_then(|entry| entry.key.resolve(scene))
            else {
                continue;
            };
            let Some(emission) = vertex_emission(&character.asset().model) else {
                continue;
            };
            let reusable = previous.iter().position(|batch| {
                batch.births.len() == members.len()
                    && batch.births.iter().zip(&members).all(|(birth, member)| {
                        self.characters
                            .get(*member)
                            .is_some_and(|entry| entry.birth == *birth)
                    })
            });
            let prepared = if let Some(index) = reusable {
                let mut retained = previous.remove(index);
                retained.members = members;
                // Owner slots may have changed even though these batch members survived.
                retained.copied_poses.fill(None);
                retained.order_initialized = false;
                Some(retained)
            } else {
                BlendBatch::new(
                    ctx,
                    &self.characters,
                    members,
                    &emission,
                    character.asset().model.indices.len(),
                    remaining,
                )
            };
            if let Some(mut batch) = prepared {
                let triangles = batch.members.len().saturating_mul(
                    self.meshes
                        .get(mesh)
                        .map_or(0, |model| model.blend_triangles.len()),
                );
                batch.triangles.clear();
                batch.triangles.reserve(triangles);
                batch.ordered_indices.clear();
                batch.ordered_indices.reserve(triangles.saturating_mul(3));
                remaining = remaining.saturating_sub(batch.gpu_bytes());
                let slot = self.blend_batches.len();
                for member in &batch.members {
                    if let Some(entry) = self.characters.get_mut(*member) {
                        entry.blend_batch = Some(slot);
                    }
                }
                self.blend_batches.push(batch);
            }
        }
    }

    pub(super) fn prepare_blend_batches(
        &mut self,
        queue: &wgpu::Queue,
        frame: &super::super::world::WorldFrame,
        cull: bool,
    ) {
        for batch in &mut self.blend_batches {
            batch.prepare(queue, &self.characters, &self.meshes, frame, cull);
        }
    }

    pub fn encode_blend_poses(&mut self, encoder: &mut wgpu::CommandEncoder) {
        for batch in &mut self.blend_batches {
            batch.copy_poses(encoder, &self.characters);
        }
    }

    pub const fn uses_blend_batches(&self) -> bool {
        !self.blend_batches.is_empty()
    }

    pub(in crate::engine::render::wgpu) fn blend_batch(&self, slot: usize) -> Option<&BlendBatch> {
        self.blend_batches.get(slot)
    }

    pub(in crate::engine::render::wgpu) fn batch_spans(
        &self,
    ) -> impl Iterator<Item = (usize, CharacterSpan, f32)> + '_ {
        self.blend_batches
            .iter()
            .enumerate()
            .flat_map(|(batch, entries)| {
                entries
                    .triangles
                    .iter()
                    .enumerate()
                    .map(move |(triangle, entry)| {
                        (
                            entry.character,
                            CharacterSpan {
                                batch: Some(batch),
                                submesh: 0,
                                first_index: u32::try_from(triangle)
                                    .unwrap_or(u32::MAX)
                                    .saturating_mul(3),
                                index_count: 3,
                            },
                            entry.distance,
                        )
                    })
            })
    }

    #[cfg(feature = "visual-diagnostics")]
    pub fn receiver_preparation_diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "blend_copied_bytes": self.blend_batches.iter().fold(0u64, |sum, batch| sum.saturating_add(batch.copied_bytes)),
            "blend_environment_uploads": self.blend_batches.iter().fold(0u64, |sum, batch| sum.saturating_add(batch.environment_uploads)),
            "uv_receiver_samples": self.characters.iter().fold(0usize, |sum, actor| sum.saturating_add(actor.uv_receiver.samples())),
        })
    }

    #[cfg(feature = "visual-diagnostics")]
    pub fn blend_batch_cpu_bytes(&self) -> usize {
        self.blend_batches
            .iter()
            .map(BlendBatch::cpu_bytes)
            .fold(0, usize::saturating_add)
    }

    #[cfg(feature = "visual-diagnostics")]
    pub fn blend_batch_gpu_bytes(&self) -> usize {
        self.blend_batches
            .iter()
            .map(|batch| {
                batch
                    .vertices
                    .size()
                    .saturating_add(batch.attributes.size())
                    .saturating_add(batch.indices.size())
                    .saturating_add(batch.environments.size())
            })
            .filter_map(|bytes| usize::try_from(bytes).ok())
            .fold(0, usize::saturating_add)
    }
}

#[cfg(test)]
#[path = "../../tests/character_blend.rs"]
mod tests;

/// Only the dedicated BLEND module uses a per-invocation environment. Ordinary
/// world/opaque/cutout shader source and its uniform layout remain unchanged.
pub(in crate::engine::render::wgpu) fn shader_source() -> Option<String> {
    // Fail closed to the general triangle path if a shared shader refactor
    // changes a required splice point. Never publish a partly adapted module.
    #[cfg(feature = "visual-diagnostics")]
    let shared =
        super::super::diagnostics::world_shader_source(super::super::world::WORLD_SHADER_SRC);
    #[cfg(not(feature = "visual-diagnostics"))]
    let shared: std::borrow::Cow<'static, str> = super::super::world::WORLD_SHADER_SRC.into();
    let ordinary = super::super::world::without_posed_shading(shared);
    let compact = without_opaque_accessibility(ordinary.as_ref())?;
    let base = compact.as_str();
    for marker in [
        "var<uniform> environment: Environment;",
        "    @location(12) @interpolate(flat) water_node_hint: u32,",
        "fn vs_main(vertex: WorldVertex, @builtin(vertex_index) vertex_index: u32) -> VsOut { return world_vertex(vertex, vertex_index); }",
        "mix(material.emission_color, in.color.rgb, material.emission_vertex)",
        "fn fs_main(",
        "fn fs_main_raw(",
        "fn fs_emission_blend(",
    ] {
        if !base.contains(marker) {
            return None;
        }
    }
    let mut source = base.replace(
        "var<uniform> environment: Environment;",
        "var<storage, read> blend_environments: array<Environment>;\nvar<private> blend_actor: u32;",
    );
    // Read only the needed fields directly from storage; never copy the large
    // shelter/spatial arrays into private fragment memory.
    source = source.replace("environment.", "blend_environments[blend_actor].");
    source = source.replace("    @location(12) @interpolate(flat) water_node_hint: u32,", "    @location(12) @interpolate(flat) water_node_hint: u32,\n    @location(13) @interpolate(flat) blend_actor: u32,\n    @location(14) blend_emission: vec3<f32>,");
    source = source.replace("fn vs_main(vertex: WorldVertex, @builtin(vertex_index) vertex_index: u32) -> VsOut { return world_vertex(vertex, vertex_index); }", "fn vs_main(vertex: WorldVertex, @builtin(vertex_index) vertex_index: u32, @location(11) actor: u32, @location(12) emission: vec3<f32>) -> VsOut { blend_actor = actor; var out = world_vertex(vertex, vertex_index); out.blend_actor = actor; out.blend_emission = emission; return out; }");
    source = source.replace(
        "mix(material.emission_color, in.color.rgb, material.emission_vertex)",
        "mix(in.blend_emission, in.color.rgb, material.emission_vertex)",
    );
    for name in [
        "fs_main",
        "fs_main_raw",
        "fs_cutout",
        "fs_cutout_raw",
        "fs_emission",
        "fs_emission_cutout",
        "fs_emission_blend",
        "fs_shadow",
        "fs_shadow_cutout",
        "fs_shadow_blend",
    ] {
        if let Some(start) = source.find(&format!("fn {name}("))
            && let Some(body) = source
                .get(start..)
                .and_then(|tail| tail.find('{'))
                .and_then(|offset| start.checked_add(offset))
                .and_then(|offset| offset.checked_add(1))
        {
            source.insert_str(body, "\n    blend_actor = in.blend_actor;");
        }
    }
    Some(source)
}

fn without_opaque_accessibility(source: &str) -> Option<String> {
    let start = source.find("    out.self_ambient = vec4<f32>(0.0);")?;
    let end = source
        .get(start..)?
        .find("    out.uv = vertex.uv;")?
        .checked_add(start)?;
    let mut output = source.to_owned();
    output.replace_range(start..end, "");
    // Git's Windows checkout may use CRLF. Remove the field itself, not its
    // newline, or an unused opaque varying exceeds DX12's fragment input limit.
    output = output.replace("    @location(15) self_ambient: vec4<f32>,", "");
    output = output.replace("in.self_ambient", "vec4<f32>(0.0)");
    Some(output)
}
