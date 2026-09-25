//! GPU materials: the resolved Places material as wgpu resources.
//!
//! The engine resolves a level's materials into the renderer-neutral
//! [`MaterialRenderState`] and the neutral [`ResolvedSurfaceMaterial`]; this
//! module owns everything after that: one uniform buffer and three bind groups
//! per distinct resolved material, and the normal-map texture each
//! material binds (uploaded through the [`TextureCache`] under the
//! `DataLinear` semantic).
//!
//! Scope is deliberately the ordinary world material:
//!
//! * base colour, tint/vertex colour and alpha classification stay exactly the
//!   neutral resolver's decisions (`resolve_surface_material`);
//! * the GPU record carries only what the world shader may consume: the sheen
//!   colour, the shine-derived roughness, the normal strength and gate, the
//!   opacity and cut-out threshold, and reflection-eligibility metadata;
//! * nothing else: no light counts, no shadow indices, no lightmap pages, no
//!   probe matrices, no reflection textures — those belong to the environment
//!   group.
//!
//! The cache is keyed by the resolved material identity (material index plus
//! the surface's shine override), never by draw index or surface position, so
//! every surface that resolves to the same material shares one GPU uniform and
//! one set of bind groups. Materials are created at level load, never per
//! frame.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use super::texture::{
    CacheOutcome, GpuTexture, SamplerPolicy, TextureCache, TextureFiltering, TextureSemantic,
};
use crate::materials::MaterialTable;
use crate::quality::QualityLevel;
use crate::render::common::materials::{
    BatchPass, MaterialRenderState, ResolvedSurfaceMaterial, resolve_surface_material,
};
use crate::render::common::mesh::{MaterialIndex, SurfaceKey, SurfaceKind, SurfaceShine};

/// Bit 0 of [`MaterialUniform::flags`]: the material's normal map is bound and
/// may be sampled.
pub const MATERIAL_FLAG_NORMAL_ENABLED: u32 = 1 << 0;
/// Bit 1: the surface response is enabled for this profile (the master gate the
/// reference calls `u_response_enabled`). The sheen consumes it; the normal
/// fetch is gated by both bits, exactly like the reference.
pub const MATERIAL_FLAG_RESPONSE_ENABLED: u32 = 1 << 1;
/// Bit 2: the material is eligible to participate in a reflection. The flag
/// records that eligibility; the reflection term itself is gated by
/// `reflection_mode`, which a capture or a disabled player setting zeroes.
pub const MATERIAL_FLAG_REFLECTION_ELIGIBLE: u32 = 1 << 2;

/// Every flag bit the material path defines. No other bit is written.
pub const MATERIAL_FLAG_MASK: u32 = MATERIAL_FLAG_NORMAL_ENABLED
    | MATERIAL_FLAG_RESPONSE_ENABLED
    | MATERIAL_FLAG_REFLECTION_ELIGIBLE;

/// The per-material uniform, in the exact layout the WGSL `Material` struct
/// declares.
///
/// ```text
/// offset  0  specular               vec3<f32>   12 bytes
/// offset 12  roughness              f32
/// offset 16  normal_strength        f32
/// offset 20  alpha_cutoff           f32
/// offset 24  opacity                f32
/// offset 28  flags                  u32
/// offset 32  reflection_strength    vec3<f32>   12 bytes
/// offset 44  reflection_mode        u32
/// offset 48  emission_color         vec3<f32>   12 bytes
/// offset 60  emission_mask_enabled  f32
/// offset 64  emission_vertex        f32
/// offset 68  emission_scale         f32
/// offset 72  _padding               [f32; 2]     8 bytes
/// ------------------------------------------------------ 80 bytes, align 16
/// ```
///
/// `#[repr(C, align(16))]` plus `Pod` make the byte view explicit; the unit
/// tests pin the size and every offset against this table and the WGSL source.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MaterialUniform {
    /// Sheen colour; zero for a material without one.
    pub specular: [f32; 3],
    /// Shader-facing roughness (`1 - shine`, or the authored legacy value).
    pub roughness: f32,
    /// Multiplier applied to the decoded normal's `xy`.
    pub normal_strength: f32,
    /// Alpha below which the cut-out pipeline discards a fragment.
    pub alpha_cutoff: f32,
    /// Multiplier applied to the sampled alpha.
    pub opacity: f32,
    /// [`MATERIAL_FLAG_NORMAL_ENABLED`] and friends.
    pub flags: u32,
    /// `specular × authored reflection strength`, the reference's
    /// `u_reflect_strength`.
    pub reflection_strength: [f32; 3],
    /// Authored reflection mode: 0 none, 1 probe, 2 planar.
    pub reflection_mode: u32,
    /// `emissive × intensity`, the reference's `u_emission_color`.
    pub emission_color: [f32; 3],
    /// Whether the emission mask is bound (the reference's
    /// `u_emission_mask_enabled`).
    pub emission_mask_enabled: f32,
    /// `1` when the vertex colour is the emission (fixture luminous faces).
    pub emission_vertex: f32,
    /// Animated emission multiplier; `1.0` without an animation.
    pub emission_scale: f32,
    /// Explicit padding to the WGSL struct's 16-byte size.
    pub _padding: [f32; 2],
}

/// Bytes one material uniform occupies.
pub const MATERIAL_UNIFORM_SIZE: u64 = std::mem::size_of::<MaterialUniform>() as u64;

/// The resolved material identity: what a GPU material is deduplicated by.
///
/// The surface kind decides which neutral resolver rules apply (a fixture
/// luminous face and a wall can share a numeric material slot and must never
/// collapse), the material index determines the albedo, response and alpha
/// contracts, and the shine override changes the roughness and therefore the
/// rendering state. All three together are the final identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialKey {
    /// The surface family the material is used by.
    pub kind: SurfaceKind,
    /// Index into the neutral material table (or the fixture-sheet list for a
    /// `Light` batch).
    pub material: MaterialIndex,
    /// The surface's authored shine override, if any.
    pub shine: Option<SurfaceShine>,
}

impl MaterialKey {
    /// The key of one static draw's surface.
    #[must_use]
    pub const fn new(
        kind: SurfaceKind,
        material: MaterialIndex,
        shine: Option<SurfaceShine>,
    ) -> Self {
        Self {
            kind,
            material,
            shine,
        }
    }

    /// The neutral surface key this material identity belongs to.
    #[must_use]
    pub const fn surface_key(self) -> SurfaceKey {
        SurfaceKey::with_shine(self.kind, self.material, self.shine)
    }
}

impl MaterialUniform {
    /// The same record with the reference's emission fields filled in.
    ///
    /// `emission` is the neutral material's emission for a floor/ceiling/wall
    /// batch, the per-vertex flag for a fixture face, or nothing at all; the
    /// mask gate is set by the caller that actually bound a mask texture.
    #[must_use]
    pub fn from_state_with_emission(
        state: &ResolvedSurfaceMaterial,
        normal_enabled: bool,
        emission: EmissionRecord,
    ) -> Self {
        let mut flags = 0u32;
        if normal_enabled && state.response_enabled {
            flags |= MATERIAL_FLAG_NORMAL_ENABLED;
        }
        if state.response_enabled {
            flags |= MATERIAL_FLAG_RESPONSE_ENABLED;
        }
        if state.reflection_eligible() {
            flags |= MATERIAL_FLAG_REFLECTION_ELIGIBLE;
        }
        // No other bit is ever written; the mask keeps a future bit from
        // leaking into the shader before it has a consumer.
        let flags = flags & MATERIAL_FLAG_MASK;
        Self {
            specular: state.specular,
            roughness: state.roughness,
            normal_strength: state.normal_strength,
            alpha_cutoff: state.alpha.cutoff,
            opacity: state.alpha.opacity,
            flags,
            reflection_strength: state.reflection_strength(),
            reflection_mode: reflection_mode_code(state),
            emission_color: emission.color,
            emission_mask_enabled: if emission.mask { 1.0 } else { 0.0 },
            emission_vertex: if emission.vertex { 1.0 } else { 0.0 },
            emission_scale: emission.scale,
            _padding: [0.0; 2],
        }
    }

    /// True when the normal fetch gate is set.
    #[must_use]
    pub const fn normal_enabled(&self) -> bool {
        self.flags & MATERIAL_FLAG_NORMAL_ENABLED != 0
    }

    /// True when the surface response is enabled for this profile.
    #[must_use]
    pub const fn response_enabled(&self) -> bool {
        self.flags & MATERIAL_FLAG_RESPONSE_ENABLED != 0
    }

    /// True when the material is reflection-eligible metadata.
    #[must_use]
    pub const fn reflection_eligible(&self) -> bool {
        self.flags & MATERIAL_FLAG_REFLECTION_ELIGIBLE != 0
    }
}

/// Numeric reflection-mode code the uniform carries.
const fn reflection_mode_code(state: &ResolvedSurfaceMaterial) -> u32 {
    match state.reflection.mode {
        crate::materials::ReflectionMode::None => 0,
        crate::materials::ReflectionMode::Probe => 1,
        crate::materials::ReflectionMode::Planar => 2,
    }
}

/// The reference's emission fields for one GPU material.
///
/// A floor/ceiling/wall material carries its `emissive × intensity` colour and,
/// when it authored a mask, the mask gate; a fixture's luminous face carries
/// the per-vertex flag instead (its glow lives in the vertex colour), and an
/// animated material carries the animation's multiplier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmissionRecord {
    /// The reference's `u_emission_color`: `emissive × intensity`.
    pub color: [f32; 3],
    /// Whether an emission mask texture is bound.
    pub mask: bool,
    /// Whether the vertex colour is the emission (fixture faces).
    pub vertex: bool,
    /// Animated emission multiplier; `1.0` without an animation.
    pub scale: f32,
}

impl EmissionRecord {
    /// No emission: every material authored before emission existed.
    pub const NONE: Self = Self {
        color: [0.0; 3],
        mask: false,
        vertex: false,
        scale: 1.0,
    };

    /// A material's uniform emission, with an optional bound mask.
    #[must_use]
    pub const fn material(emission: crate::materials::MaterialEmission, mask: bool) -> Self {
        Self {
            color: emission.effective_color(),
            mask,
            vertex: false,
            scale: 1.0,
        }
    }

    /// Per-vertex emission: a fixture's luminous face.
    #[must_use]
    pub const fn vertex() -> Self {
        Self {
            color: [0.0; 3],
            mask: false,
            vertex: true,
            scale: 1.0,
        }
    }

    /// The same record with an animation multiplier.
    #[must_use]
    pub const fn with_scale(mut self, scale: f32) -> Self {
        self.scale = if scale.is_finite() { scale } else { 1.0 };
        self
    }

    /// True when the surface emits anything at all; the bloom pass draws
    /// exactly the batches this answers `true` for.
    #[must_use]
    pub fn is_emissive(&self) -> bool {
        self.vertex || (self.color[0] > 0.0 || self.color[1] > 0.0 || self.color[2] > 0.0)
    }
}

/// The reflection mode one material runs with this frame.
///
/// The reference's `reflection_uniforms`, as a pure rule:
///
/// * zero while a capture runs (a mirror must not sample the image being
///   written) or when reflections are disabled;
/// * a probe material is mode 1 only while a probe is resident;
/// * a planar material is mode 2 only while its own plane is the one this
///   frame reflects;
/// * every other combination is mode 0.
#[must_use]
pub fn reflection_mode_for(
    authored: u32,
    material_plane: Option<usize>,
    active_plane: Option<usize>,
    probes_resident: bool,
    enabled: bool,
    capturing: bool,
) -> u32 {
    if capturing || !enabled {
        return 0;
    }
    match authored {
        1 if probes_resident => 1,
        2 if active_plane.is_some() && active_plane == material_plane => 2,
        _ => 0,
    }
}

/// The bind group layout of group 2: one material uniform, its normal-map
/// texture and the sampler the normal map is read with, and its emission mask
/// with its sampler.
///
/// Created once per renderer and shared by every world pipeline rebuild and
/// every cached material bind group, so a changed surface format never
/// invalidates a material binding.
#[must_use]
pub fn material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let sampler = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-material-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(MATERIAL_UNIFORM_SIZE),
                },
                count: None,
            },
            texture(1),
            sampler(2),
            texture(3),
            sampler(4),
        ],
    })
}

/// One resolved material, resident on the GPU.
///
/// The normal and emission textures are kept alive alongside their bind groups
/// so a material outlives a texture-cache release safely (the same lifetime rule
/// the per-draw textures follow).
pub struct GpuMaterial {
    /// Kept for ownership; the bind groups reference it, and the reflection /
    /// animation writes target it.
    uniform_buffer: wgpu::Buffer,
    /// Uniform + normal texture + sampler + emission mask + sampler, with the
    /// Low world filtering preset.
    low_bind_group: wgpu::BindGroup,
    /// The same with the Medium world filtering preset.
    medium_bind_group: wgpu::BindGroup,
    /// The same with the High world filtering preset.
    high_bind_group: wgpu::BindGroup,
    /// The normal-map texture, or the shared white fallback.
    _normal: Arc<GpuTexture>,
    /// The emission-mask texture, or the shared white fallback.
    _emission: Arc<GpuTexture>,
    /// True when the material emits anything; the emissive pass draws exactly
    /// these.
    emissive: bool,
    /// The emission scale last written into the uniform.
    applied_scale: f32,
    /// The authored reflection mode (0 none, 1 probe, 2 planar) after the
    /// eligibility gate; zero for a material that authors none.
    authored_reflection_mode: u32,
    /// The mirror plane this material reflects on, for the planar gate.
    reflection_plane: Option<usize>,
    /// The reflection mode last written into the uniform, so a frame writes
    /// only what changed.
    applied_reflection_mode: u32,
}

impl GpuMaterial {
    /// The bind group for one filtering level.
    ///
    /// A material without a normal map or emission mask binds the clamped
    /// nearest policy in all three; a material with a real map follows the
    /// player's world preset.
    #[must_use]
    pub const fn bind_group(&self, filtering: TextureFiltering) -> &wgpu::BindGroup {
        match filtering {
            TextureFiltering::Low => &self.low_bind_group,
            TextureFiltering::Medium => &self.medium_bind_group,
            TextureFiltering::High => &self.high_bind_group,
        }
    }
}

/// What one level's material resolution did, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldMaterialStats {
    /// Distinct resolved material states the draw set uses.
    pub materials: usize,
    /// Materials whose surface response is enabled on this profile.
    pub response_materials: usize,
    /// Materials eligible to participate in a reflection.
    pub reflection_eligible: usize,
    /// Materials whose normal map is bound and enabled.
    pub normal_maps: usize,
    /// Normal-map GPU uploads this level load performed.
    pub normal_uploads: usize,
    /// Normal-map lookups the renderer's texture cache already held.
    pub normal_cache_hits: usize,
    /// Materials that emit anything (the emissive pass draws exactly these).
    pub emission_materials: usize,
    /// Materials whose emission mask is bound.
    pub emission_masks: usize,
    /// Emission-mask GPU uploads this level load performed.
    pub emission_uploads: usize,
    /// Emission-mask lookups the renderer's texture cache already held.
    pub emission_cache_hits: usize,
    /// Draws in the opaque pass.
    pub opaque_draws: usize,
    /// Draws in the cut-out pass.
    pub cutout_draws: usize,
    /// Draws in the translucent pass.
    pub translucent_draws: usize,
}

/// The emission record one material identity resolves to.
///
/// The reference's rule, exactly: a fixture's luminous face (`Light` with a
/// sheet) uses its vertex colour; a floor/ceiling/wall material uses its
/// `emissive × intensity` with the authored mask; every other family (fixture
/// housings, placeholder boxes, decals) does not emit.
fn resolver_emission(
    kind: SurfaceKind,
    material: MaterialIndex,
    inputs: &WorldMaterialInputs<'_>,
) -> EmissionRecord {
    match crate::render::common::materials::emission_routing(
        kind,
        material != crate::render::common::mesh::MATERIAL_NONE,
    ) {
        crate::render::common::materials::EmissionRouting::Vertex => EmissionRecord::vertex(),
        crate::render::common::materials::EmissionRouting::Material => {
            let emission = inputs
                .materials
                .emissions
                .get(usize::from(material))
                .copied()
                .unwrap_or_default();
            if emission.is_emissive() {
                let mask = emission
                    .mask
                    .is_some_and(|index| inputs.table.textures().get(usize::from(index)).is_some());
                EmissionRecord::material(emission, mask)
            } else {
                EmissionRecord::NONE
            }
        }
        crate::render::common::materials::EmissionRouting::None => EmissionRecord::NONE,
    }
}

/// Every material the level's draw set resolves to.
///
/// Created once per level upload. `per_draw` is parallel to the world draw set
/// and gives each draw its material entry; two draws that resolve to the same
/// identity share the entry.
pub struct WorldMaterials {
    entries: Vec<GpuMaterial>,
    per_draw: Vec<usize>,
    /// Per entry: the emission animation that material authored, if any.
    animations: Vec<Option<crate::render::common::animation::EmissionAnimation>>,
    stats: WorldMaterialStats,
}

/// The distinct material identities one draw set uses, in first-use order,
/// together with each draw's slot into that list.
///
/// Two draws with the same `(kind, material index, shine override)` share one
/// slot — the same GPU uniform and the same pair of bind groups. Two draws that
/// differ in any field never collapse, so a per-surface shine override always
/// gets its own material and a fixture face can never collide with a wall that
/// happens to carry the same numeric slot.
///
/// GPU-free on purpose: the dedupe rule is unit-testable without a device, and
/// `WorldMaterials::resolve` uses exactly this assignment.
#[must_use]
pub fn material_identities(
    draws: &[super::world::WorldDraw],
) -> (Vec<(MaterialKey, SurfaceKind)>, Vec<usize>) {
    let mut seen: HashMap<MaterialKey, usize> = HashMap::new();
    let mut entries: Vec<(MaterialKey, SurfaceKind)> = Vec::new();
    let mut per_draw: Vec<usize> = Vec::with_capacity(draws.len());
    for draw in draws {
        let key = MaterialKey::new(draw.kind, draw.material, draw.shine);
        let slot = seen.get(&key).copied().unwrap_or_else(|| {
            entries.push((key, draw.kind));
            let slot = entries.len().saturating_sub(1);
            seen.insert(key, slot);
            slot
        });
        per_draw.push(slot);
    }
    (entries, per_draw)
}

/// The renderer-neutral inputs one level's material resolution needs.
#[derive(Clone, Copy)]
pub struct WorldMaterialInputs<'a> {
    /// The uploaded world draw set.
    pub draws: &'a [super::world::WorldDraw],
    /// The engine's per-material render state.
    pub materials: &'a MaterialRenderState,
    /// The level's resolved material table.
    pub table: &'a MaterialTable,
    /// The active quality level (the response gate and the texture fit).
    pub level: QualityLevel,
    /// Per-material emission animations, indexed by material index.
    pub animations: &'a [Option<crate::render::common::animation::EmissionAnimation>],
    /// The level's reflection routing, for the planar/probe gates.
    pub routing: &'a crate::render::common::reflections::ReflectionRouting,
}

impl WorldMaterials {
    /// Resolves every world draw to its material, creating the GPU records the
    /// cache does not hold yet.
    ///
    /// `inputs.level` supplies the response gate; the neutral resolver
    /// applies it to the normal map and sheen (and therefore to the reflection
    /// strength), exactly as the OpenGL draw path does. Emissions resolve the
    /// same way they do there: a floor/ceiling/wall material carries
    /// `emissive × intensity` plus its optional mask, a fixture luminous face
    /// carries the per-vertex flag, and an animated material carries its
    /// multiplier at `seconds = 0` (rewritten per frame by
    /// [`Self::update_animations`]).
    #[must_use]
    // One cohesive resolution pass: the per-material loop threads the same
    // cache, stats and inputs, and the emission/reflection gates read one entry.
    #[allow(clippy::too_many_lines)]
    pub fn resolve(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        layout: &wgpu::BindGroupLayout,
        inputs: WorldMaterialInputs<'_>,
    ) -> Self {
        let draws = inputs.draws;
        let level = inputs.level;
        let mut stats = WorldMaterialStats::default();
        for draw in draws {
            match draw.pass {
                BatchPass::Opaque => stats.opaque_draws = stats.opaque_draws.saturating_add(1),
                BatchPass::Cutout => stats.cutout_draws = stats.cutout_draws.saturating_add(1),
                BatchPass::Translucent => {
                    stats.translucent_draws = stats.translucent_draws.saturating_add(1);
                }
            }
        }
        let (keys, per_draw) = material_identities(draws);
        let response_allowed = level.draws_surface_response();
        let mut entries: Vec<GpuMaterial> = Vec::with_capacity(keys.len());
        let mut animations: Vec<Option<crate::render::common::animation::EmissionAnimation>> =
            Vec::with_capacity(keys.len());
        for (key, kind) in &keys {
            let resolved = resolve_surface_material(
                key.surface_key(),
                inputs.materials,
                inputs.table,
                response_allowed,
            );
            let normal = resolved
                .normal
                .and_then(|index| inputs.table.textures().get(usize::from(index)));
            let emission_record = resolver_emission(*kind, key.material, &inputs);
            let mask_texture = if emission_record.mask {
                inputs
                    .materials
                    .emissions
                    .get(usize::from(key.material))
                    .and_then(|emission| emission.mask)
                    .and_then(|index| inputs.table.textures().get(usize::from(index)))
            } else {
                None
            };
            let animation = if matches!(
                kind,
                SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
            ) {
                inputs
                    .animations
                    .get(usize::from(key.material))
                    .copied()
                    .flatten()
            } else {
                None
            };
            let emission_record = animation.map_or(emission_record, |animation| {
                emission_record.with_scale(animation.factor(0.0))
            });
            let uniform = MaterialUniform::from_state_with_emission(
                &resolved,
                normal.is_some(),
                emission_record,
            );
            if uniform.response_enabled() {
                stats.response_materials = stats.response_materials.saturating_add(1);
            }
            if uniform.reflection_eligible() {
                stats.reflection_eligible = stats.reflection_eligible.saturating_add(1);
            }
            if emission_record.is_emissive() {
                stats.emission_materials = stats.emission_materials.saturating_add(1);
            }
            let normal_texture = if let Some(resolved_texture) = normal {
                let (outcome, texture) = cache.get_or_upload(
                    device,
                    queue,
                    resolved_texture,
                    TextureSemantic::DataLinear,
                    level,
                );
                match outcome {
                    CacheOutcome::Uploaded => {
                        stats.normal_uploads = stats.normal_uploads.saturating_add(1);
                    }
                    CacheOutcome::Reused => {
                        stats.normal_cache_hits = stats.normal_cache_hits.saturating_add(1);
                    }
                }
                if uniform.normal_enabled() {
                    stats.normal_maps = stats.normal_maps.saturating_add(1);
                }
                texture
            } else {
                cache.fallback()
            };
            let emission_texture = if let Some(resolved_texture) = mask_texture {
                let (outcome, texture) = cache.get_or_upload(
                    device,
                    queue,
                    resolved_texture,
                    TextureSemantic::DataLinear,
                    level,
                );
                match outcome {
                    CacheOutcome::Uploaded => {
                        stats.emission_uploads = stats.emission_uploads.saturating_add(1);
                    }
                    CacheOutcome::Reused => {
                        stats.emission_cache_hits = stats.emission_cache_hits.saturating_add(1);
                    }
                }
                stats.emission_masks = stats.emission_masks.saturating_add(1);
                texture
            } else {
                cache.fallback()
            };
            entries.push(GpuMaterial::new(
                device,
                queue,
                layout,
                cache,
                &normal_texture,
                &emission_texture,
                uniform,
            ));
            if let Some(entry) = entries.last_mut() {
                // The authored mode is only live when the material is eligible
                // (an active authored reflection with a non-zero weighted
                // strength); the reference's `reflection_uniforms` returns zero
                // otherwise, and a capture zeroes it for every material.
                if !resolved.reflection_eligible() {
                    entry.authored_reflection_mode = 0;
                    entry.applied_reflection_mode = 0;
                    entry.write_reflection_mode(queue, 0);
                }
                entry.reflection_plane = inputs.routing.plane_of(usize::from(key.material));
            }
            animations.push(animation);
        }
        stats.materials = entries.len();
        Self {
            entries,
            per_draw,
            animations,
            stats,
        }
    }

    /// Applies one frame's reflection modes to every material.
    ///
    /// The reference computes `u_reflect_mode` per draw from the frame's active
    /// plane, the capture flag and whether a probe is resident; here it lives in
    /// the material's own uniform, so a frame writes only the entries whose mode
    /// changed (usually none across a still frame, and at most one capture
    /// round-trip when a plane is active).
    ///
    /// * a capture zeroes every mode (a mirror must not sample the image being
    ///   written, and a probe bake must not sample an incomplete cube);
    /// * with reflections disabled every mode stays zero for the session;
    /// * a probe material is mode 1 while a probe is resident, else 0;
    /// * a planar material is mode 2 only while its own plane is the one this
    ///   frame reflects.
    pub fn update_reflection_modes(
        &mut self,
        queue: &wgpu::Queue,
        active_plane: Option<usize>,
        probes_resident: bool,
        enabled: bool,
        capturing: bool,
    ) {
        for entry in &mut self.entries {
            let desired = reflection_mode_for(
                entry.authored_reflection_mode,
                entry.reflection_plane,
                active_plane,
                probes_resident,
                enabled,
                capturing,
            );
            if desired != entry.applied_reflection_mode {
                entry.write_reflection_mode(queue, desired);
                entry.applied_reflection_mode = desired;
            }
        }
    }

    /// Rewrites the emission-scale uniform of every animated material.
    ///
    /// The reference uploads `u_emission_scale` per draw per frame; here the
    /// value lives in the material's own uniform, so a frame writes only the
    /// entries whose factor actually changed (usually none or one). Never a
    /// per-frame allocation: the material records are level resources.
    pub fn update_animations(&mut self, queue: &wgpu::Queue, seconds: f32) {
        for (slot, animation) in self.animations.iter().enumerate() {
            let Some(animation) = animation else {
                continue;
            };
            let factor = animation.factor(seconds);
            let Some(entry) = self.entries.get_mut(slot) else {
                continue;
            };
            if (entry.applied_scale - factor).abs() <= f32::EPSILON {
                continue;
            }
            entry.applied_scale = factor;
            entry.write_emission_scale(queue, factor);
        }
    }

    /// The material entry index one draw uses.
    #[must_use]
    pub fn slot_for_draw(&self, draw_index: usize) -> Option<usize> {
        self.per_draw.get(draw_index).copied()
    }

    /// The GPU material of one entry index.
    #[must_use]
    pub fn entry(&self, slot: usize) -> Option<&GpuMaterial> {
        self.entries.get(slot)
    }

    /// True when the material in `slot` emits anything.
    #[must_use]
    pub fn is_emissive(&self, slot: usize) -> bool {
        self.entries.get(slot).is_some_and(GpuMaterial::is_emissive)
    }

    /// The mirror plane a material's planar reflection belongs to, if any.
    #[must_use]
    pub fn material_plane(&self, slot: usize) -> Option<usize> {
        self.entries
            .get(slot)
            .and_then(|entry| entry.reflection_plane)
    }

    /// The resolution counters.
    #[must_use]
    pub const fn stats(&self) -> WorldMaterialStats {
        self.stats
    }
}

impl GpuMaterial {
    /// Creates one material's uniform buffer and its three bind groups.
    ///
    /// A material without a normal map or emission mask binds the shared white
    /// fallback with the clamped nearest sampler and the fetch gate off,
    /// exactly like the OpenGL reference; a real map follows the player's
    /// filtering setting (its own wrap/mip policy + user filter) at all three
    /// world presets.
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        cache: &TextureCache,
        normal: &Arc<GpuTexture>,
        emission: &Arc<GpuTexture>,
        uniform: MaterialUniform,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-material"),
            size: MATERIAL_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // The uniform is static level data: one write at creation, never a
        // per-frame update (an animated emission scale is rewritten per frame
        // through `write_emission_scale`).
        queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniform));
        // The mask gate the uniform already carries decides the sampler policy;
        // no separate flag can disagree with it. A map-less material binds the
        // shared fallback with the clamped nearest policy in all three slots; a
        // real normal map or emission mask follows the player's world presets.
        let emission_bound = uniform.emission_mask_enabled > 0.5;
        let maps_resident = !normal.is_fallback() || emission_bound;
        let policy = |filtering: TextureFiltering| {
            if maps_resident {
                filtering.sampler_policy()
            } else {
                SamplerPolicy::ClampNearest
            }
        };
        let normal_view = normal.view();
        let emission_view = emission.view();
        let bind_group_for = |filtering: TextureFiltering| {
            create_material_bind_group(
                device,
                layout,
                &buffer,
                normal_view,
                emission_view,
                cache,
                policy(filtering),
            )
        };
        let low_bind_group = bind_group_for(TextureFiltering::Low);
        let medium_bind_group = bind_group_for(TextureFiltering::Medium);
        let high_bind_group = bind_group_for(TextureFiltering::High);
        Self {
            uniform_buffer: buffer,
            low_bind_group,
            medium_bind_group,
            high_bind_group,
            _normal: Arc::clone(normal),
            _emission: Arc::clone(emission),
            emissive: uniform.emission_vertex > 0.5
                || uniform.emission_color.map(f32::to_bits) != [0_u32; 3],
            applied_scale: uniform.emission_scale,
            authored_reflection_mode: uniform.reflection_mode,
            reflection_plane: None,
            applied_reflection_mode: uniform.reflection_mode,
        }
    }

    /// True when the material emits anything at all.
    #[must_use]
    pub const fn is_emissive(&self) -> bool {
        self.emissive
    }

    /// Writes a new animated emission scale into the material's uniform.
    ///
    /// Called at most once per frame per animated material; only the four bytes
    /// at the field's offset move, and a scale that did not change is skipped by
    /// the caller.
    pub fn write_emission_scale(&self, queue: &wgpu::Queue, scale: f32) {
        let offset = std::mem::offset_of!(MaterialUniform, emission_scale) as u64;
        queue.write_buffer(&self.uniform_buffer, offset, &scale.to_le_bytes());
    }

    /// Writes a new frame reflection mode into the material's uniform.
    pub fn write_reflection_mode(&self, queue: &wgpu::Queue, mode: u32) {
        let offset = std::mem::offset_of!(MaterialUniform, reflection_mode) as u64;
        queue.write_buffer(&self.uniform_buffer, offset, &mode.to_le_bytes());
    }
}

impl GpuMaterial {
    /// A plain-opaque material with only an emission record: the prop path.
    ///
    /// The reference draws every prop primitive with `SurfaceState::plain`
    /// plus its glTF emission (fixture faces use the per-vertex flag, which
    /// props never do). No normal map, no response, no reflection, no alpha
    /// contract beyond the always-opaque default.
    #[must_use]
    pub fn plain_emissive(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        cache: &TextureCache,
        emission_texture: &Arc<GpuTexture>,
        record: EmissionRecord,
    ) -> Self {
        let uniform = MaterialUniform::from_state_with_emission(
            &ResolvedSurfaceMaterial::plain(),
            false,
            record,
        );
        let fallback = cache.fallback();
        Self::new(
            device,
            queue,
            layout,
            cache,
            &fallback,
            emission_texture,
            uniform,
        )
    }
}

/// Creates one group-2 bind group: the material uniform, a normal-map view, an
/// emission-mask view and one sampler policy for both.
fn create_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    normal_view: &wgpu::TextureView,
    emission_view: &wgpu::TextureView,
    cache: &TextureCache,
    sampler: SamplerPolicy,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-material"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(normal_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(cache.sampler(sampler)),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(emission_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(cache.sampler(sampler)),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/indexing/float comparisons are idiomatic here.
    #![allow(clippy::float_cmp, clippy::indexing_slicing, clippy::unwrap_used)]

    use super::*;

    // ------------------------------------------------------------- layout

    #[test]
    fn the_material_uniform_layout_matches_the_declared_offsets() {
        assert_eq!(std::mem::size_of::<MaterialUniform>(), 80);
        assert_eq!(MATERIAL_UNIFORM_SIZE, 80);
        // The host struct aligns to 16 exactly like WGSL's struct; the explicit
        // offsets above and the uniform's `min_binding_size` pin the rest.
        assert_eq!(std::mem::align_of::<MaterialUniform>(), 16);
        assert_eq!(std::mem::offset_of!(MaterialUniform, specular), 0);
        assert_eq!(std::mem::offset_of!(MaterialUniform, roughness), 12);
        assert_eq!(std::mem::offset_of!(MaterialUniform, normal_strength), 16);
        assert_eq!(std::mem::offset_of!(MaterialUniform, alpha_cutoff), 20);
        assert_eq!(std::mem::offset_of!(MaterialUniform, opacity), 24);
        assert_eq!(std::mem::offset_of!(MaterialUniform, flags), 28);
        assert_eq!(
            std::mem::offset_of!(MaterialUniform, reflection_strength),
            32
        );
        assert_eq!(std::mem::offset_of!(MaterialUniform, reflection_mode), 44);
        assert_eq!(std::mem::offset_of!(MaterialUniform, emission_color), 48);
        assert_eq!(
            std::mem::offset_of!(MaterialUniform, emission_mask_enabled),
            60
        );
        assert_eq!(std::mem::offset_of!(MaterialUniform, emission_vertex), 64);
        assert_eq!(std::mem::offset_of!(MaterialUniform, emission_scale), 68);
        assert_eq!(std::mem::offset_of!(MaterialUniform, _padding), 72);
    }

    #[test]
    fn the_flag_bits_are_the_documented_ones() {
        assert_eq!(MATERIAL_FLAG_NORMAL_ENABLED, 1);
        assert_eq!(MATERIAL_FLAG_RESPONSE_ENABLED, 2);
        assert_eq!(MATERIAL_FLAG_REFLECTION_ELIGIBLE, 4);
        assert_eq!(MATERIAL_FLAG_MASK, 7);
    }

    #[test]
    fn a_plain_state_packs_to_a_gated_opaque_record() {
        let uniform = MaterialUniform::from_state_with_emission(
            &ResolvedSurfaceMaterial::plain(),
            false,
            EmissionRecord::NONE,
        );
        assert_eq!(uniform.specular, [0.0; 3]);
        assert_eq!(uniform.roughness, crate::materials::DEFAULT_ROUGHNESS);
        assert!(!uniform.normal_enabled());
        assert!(!uniform.response_enabled());
        assert!(!uniform.reflection_eligible());
        assert_eq!(uniform.alpha_cutoff, crate::materials::DEFAULT_ALPHA_CUTOFF);
        assert_eq!(uniform.opacity, 1.0);
        assert_eq!(uniform.reflection_strength, [0.0; 3]);
        assert_eq!(uniform.reflection_mode, 0);
        assert_eq!(uniform.flags, 0);
    }

    #[test]
    fn a_normal_mapped_reflective_state_sets_its_bits() {
        let state = ResolvedSurfaceMaterial {
            texture: Some(0),
            normal: Some(1),
            normal_strength: 0.35,
            specular: [0.5, 0.5, 0.5],
            roughness: 0.7,
            alpha: crate::materials::MaterialAlpha::OPAQUE,
            reflection: crate::materials::MaterialReflection::new(
                crate::materials::ReflectionMode::Probe,
                0.45,
            ),
            response_enabled: true,
        };
        let uniform = MaterialUniform::from_state_with_emission(&state, true, EmissionRecord::NONE);
        assert!(uniform.normal_enabled());
        assert!(uniform.response_enabled());
        assert!(uniform.reflection_eligible());
        assert_eq!(uniform.flags, MATERIAL_FLAG_MASK);
        assert_eq!(uniform.normal_strength, 0.35);
        assert_eq!(uniform.reflection_strength, [0.225, 0.225, 0.225]);
        assert_eq!(uniform.reflection_mode, 1);
    }

    #[test]
    fn the_profile_gate_clears_the_response_and_reflection_eligibility() {
        // The neutral resolver has already zeroed the specular for a gated
        // profile; this pins what the GPU record sees for that state.
        let state = ResolvedSurfaceMaterial {
            texture: Some(0),
            normal: None,
            normal_strength: 1.0,
            specular: [0.0; 3],
            roughness: 0.5,
            alpha: crate::materials::MaterialAlpha::OPAQUE,
            reflection: crate::materials::MaterialReflection::new(
                crate::materials::ReflectionMode::Planar,
                0.9,
            ),
            response_enabled: false,
        };
        let uniform =
            MaterialUniform::from_state_with_emission(&state, false, EmissionRecord::NONE);
        assert!(!uniform.normal_enabled());
        assert!(!uniform.response_enabled());
        assert!(!uniform.reflection_eligible());
        assert_eq!(uniform.reflection_strength, [0.0; 3]);
        // The authored mode still reaches the metadata; only the eligibility
        // flag is gated by the zeroed weight.
        assert_eq!(uniform.reflection_mode, 2);
    }

    #[test]
    fn a_stale_normal_index_falls_back_with_the_gate_off() {
        let state = ResolvedSurfaceMaterial {
            texture: Some(0),
            normal: Some(99),
            normal_strength: 1.0,
            specular: [1.0; 3],
            roughness: 0.4,
            alpha: crate::materials::MaterialAlpha::OPAQUE,
            reflection: crate::materials::MaterialReflection::NONE,
            response_enabled: true,
        };
        let uniform =
            MaterialUniform::from_state_with_emission(&state, false, EmissionRecord::NONE);
        assert!(
            !uniform.normal_enabled(),
            "an unresolvable map must not be read"
        );
        assert!(uniform.response_enabled(), "the sheen is still authored");
    }

    #[test]
    fn the_key_is_material_and_shine_only() {
        let base = MaterialKey::new(SurfaceKind::Wall, 7, None);
        assert_eq!(base, MaterialKey::new(SurfaceKind::Wall, 7, None));
        assert_ne!(
            base,
            MaterialKey::new(SurfaceKind::Wall, 7, Some(SurfaceShine::from_unit(0.5)))
        );
        assert_ne!(base, MaterialKey::new(SurfaceKind::Wall, 8, None));
        // Two different overrides never collapse, even when both are authored.
        assert_ne!(
            MaterialKey::new(SurfaceKind::Wall, 7, Some(SurfaceShine::from_unit(0.5))),
            MaterialKey::new(SurfaceKind::Wall, 7, Some(SurfaceShine::from_unit(0.6)))
        );
    }

    /// One synthetic draw with a material key and a pass.
    fn draw(
        material: MaterialIndex,
        shine: Option<SurfaceShine>,
        pass: BatchPass,
    ) -> super::super::world::WorldDraw {
        super::super::world::WorldDraw {
            chunk: 0,
            index_start: 0,
            index_count: 6,
            vertex_count: 6,
            bounds: crate::spatial::Aabb {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
            },
            kind: SurfaceKind::Wall,
            material,
            shine,
            pass,
        }
    }

    #[test]
    fn equal_identities_dedupe_and_overrides_do_not() {
        let shine = SurfaceShine::from_unit(0.05);
        let draws = vec![
            draw(0, None, BatchPass::Opaque),
            draw(0, None, BatchPass::Cutout),
            draw(0, Some(shine), BatchPass::Translucent),
            draw(1, None, BatchPass::Opaque),
            draw(0, Some(shine), BatchPass::Translucent),
        ];
        let (keys, per_draw) = material_identities(&draws);
        assert_eq!(keys.len(), 3, "one entry per distinct identity");
        assert_eq!(keys[0].0, MaterialKey::new(SurfaceKind::Wall, 0, None));
        assert_eq!(
            keys[1].0,
            MaterialKey::new(SurfaceKind::Wall, 0, Some(shine))
        );
        assert_eq!(keys[2].0, MaterialKey::new(SurfaceKind::Wall, 1, None));
        assert_eq!(per_draw, vec![0, 0, 1, 2, 1]);
        assert_eq!(per_draw.len(), draws.len());
    }

    #[test]
    fn a_numeric_shine_override_that_matches_no_other_still_splits() {
        // 0.50 and 0.51 quantise to different percents and must not dedupe.
        let draws = vec![
            draw(3, Some(SurfaceShine::from_unit(0.50)), BatchPass::Opaque),
            draw(3, Some(SurfaceShine::from_unit(0.51)), BatchPass::Opaque),
            draw(3, Some(SurfaceShine::from_unit(0.50)), BatchPass::Opaque),
        ];
        let (keys, per_draw) = material_identities(&draws);
        assert_eq!(keys.len(), 2);
        assert_eq!(per_draw, vec![0, 1, 0]);
    }

    // ------------------------------------------------- reflections

    #[test]
    fn the_reflection_mode_rule_matches_the_reference_gates() {
        // A capture zeroes every mode, whatever the material authors.
        assert_eq!(
            reflection_mode_for(2, Some(0), Some(0), true, true, true),
            0
        );
        assert_eq!(reflection_mode_for(1, None, None, true, true, true), 0);
        // Disabled reflections keep every mode zero.
        assert_eq!(
            reflection_mode_for(2, Some(0), Some(0), true, false, false),
            0
        );
        assert_eq!(reflection_mode_for(1, None, None, true, false, false), 0);
        // A probe material reflects while a probe is resident.
        assert_eq!(reflection_mode_for(1, None, None, true, true, false), 1);
        assert_eq!(reflection_mode_for(1, None, None, false, true, false), 0);
        // A planar material reflects only on its own active plane.
        assert_eq!(
            reflection_mode_for(2, Some(1), Some(1), true, true, false),
            2
        );
        assert_eq!(
            reflection_mode_for(2, Some(1), Some(0), true, true, false),
            0
        );
        assert_eq!(reflection_mode_for(2, Some(1), None, true, true, false), 0);
        // A material with no authored mode never reflects.
        assert_eq!(reflection_mode_for(0, None, Some(0), true, true, false), 0);
        // An unknown code degrades to none.
        assert_eq!(reflection_mode_for(9, None, None, true, true, false), 0);
    }

    #[test]
    fn a_gated_material_never_sets_a_reflection_mode() {
        // The neutral resolver's eligibility gate zeroes the authored mode for
        // a material whose weighted strength is zero (Low, or no strength).
        let state = ResolvedSurfaceMaterial {
            texture: Some(0),
            normal: None,
            normal_strength: 1.0,
            specular: [0.0; 3],
            roughness: 0.5,
            alpha: crate::materials::MaterialAlpha::OPAQUE,
            reflection: crate::materials::MaterialReflection::new(
                crate::materials::ReflectionMode::Planar,
                0.9,
            ),
            response_enabled: false,
        };
        assert!(!state.reflection_eligible());
        // The authored code is still metadata, but `resolve` clears it before
        // the frame rule can ever see it; assert the rule's own gate too.
        assert_eq!(
            reflection_mode_for(0, Some(0), Some(0), true, true, false),
            0
        );
    }
}
