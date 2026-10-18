//! Prop instancing and lighting.
//!
//! Every placed instance of one model is transformed and lit on the CPU at
//! level load time and appended to a per-model batch, so the renderer binds one
//! buffer per model and issues one draw call per *primitive* of all of its
//! instances. A prop with no usable asset falls back to a placeholder box.
//!
//! Materials
//! ---------
//! A production GLB may split one model into several primitives, each with its
//! own material, texture and emission. Instances are accumulated per
//! `(model, spatial cell)` and their index lists are grouped by primitive, so a
//! model with three primitives costs three draws per cell no matter how many
//! times it is placed — a field of vending machines still batches. A single
//! primitive model behaves exactly as it always has: one range, one draw.

use std::sync::Arc;

use super::{LevelDef, LevelLighting, LevelSurfaces, PropDef, Vertex, spatial_cell_grid};
use crate::engine::materials::{MaterialAlpha, MaterialEmission};

#[cfg(test)]
#[path = "../../tests/caustics_albedo.rs"]
mod caustics_albedo_tests;

/// One primitive's slice of a [`PropMeshBatch`]: its texture, its emission and
/// the range of the batch's index buffer it draws.
#[derive(Clone, Debug, PartialEq)]
pub struct PropSubmeshBatch {
    /// Shared scalar material response for every mesh route.
    pub response: crate::engine::materials::MaterialResponse,
    /// Index into [`PropMeshBatch::textures`], or `None` for an untextured
    /// material (the renderer draws it through the shared white sheet with the
    /// material's `baseColorFactor` already baked into the vertex colours).
    pub texture: Option<u16>,
    /// The material's visual emission. Never a light source: an emissive prop
    /// only glows, and any environmental illumination it contributes comes from
    /// the generic lights its level entry attaches to it.
    pub emission: MaterialEmission,
    /// The material's alpha contract. A glTF `MASK` primitive is
    /// [`crate::engine::materials::AlphaMode::Cutout`] and draws through the cutout
    /// pass; a `BLEND` primitive is [`crate::engine::materials::AlphaMode::Blend`] and
    /// is meaningful only on a route with a translucent pass (a character or a
    /// dynamic object); every other prop primitive is opaque.
    pub alpha: MaterialAlpha,
    /// First index into [`PropMeshBatch::indices`].
    pub first_index: u32,
    /// Number of indices in this submesh (a multiple of three).
    pub index_count: u32,
}

/// Instanced prop geometry for one distinct prop model inside one spatial cell.
///
/// Every placed instance of the same model is transformed on the CPU at level
/// load time and appended here, so the renderer binds one buffer per model and
/// draws each of its primitives once for every instance in the batch. The
/// decoded model itself is parsed once and shared through
/// [`crate::engine::props::PropAssets`], and its images are shared through [`Arc`], so a
/// model placed in twenty cells still has one decoded copy per texture.
#[derive(Clone, Debug)]
pub struct PropMeshBatch {
    /// Catalogue model path, e.g. `models/chair.glb`.
    pub model: String,
    /// Stable light controller: fixture ordinal, then fixture count + prop ordinal.
    /// Controlled instances own their luminous uniforms while images stay shared.
    pub light_control: Option<u32>,
    /// False when every instance of this model is claimed by the animated
    /// character path. The retained bind pose then supplies budgets/fallback,
    /// while immutable lighting casters follow the geometry actually drawn.
    pub casts_static_lighting: bool,
    /// Preparation-only: source albedo still needs irradiance lighting before
    /// packaging. Readers restore false because saved colours are already lit.
    pub(crate) pending_probe_lighting: bool,
    /// Every texture the model uses, indexed by [`PropSubmeshBatch::texture`]
    /// and by [`MaterialEmission::mask`]. Shared with every other batch of the
    /// same model.
    pub textures: Vec<Arc<crate::engine::loader::RawImage>>,
    /// One entry per primitive of the model, in index-buffer order. Primitives
    /// that draw nothing are absent, so a batch with an empty `indices` has no
    /// submeshes either.
    pub submeshes: Vec<PropSubmeshBatch>,
    /// Source primitive ordinals aligned with the retained, nonempty submeshes.
    /// This preparation-only identity survives skipped slivers without changing
    /// the packaged draw record. An empty vector means the decoded source
    /// ordinals are unknown and transport must use conservative batch identity.
    pub source_primitives: Vec<usize>,
    /// Pre-transformed vertices, referenced by `indices`.
    ///
    /// The historical Low path retains source indexing. Surface-lightmapped
    /// static models retain split triangle corners and source normal frames;
    /// supported coplanar pairs share only their illumination atlas domain.
    pub vertices: Vec<Vertex>,
    /// `GL_UNSIGNED_SHORT` indices into `vertices`, offset per instance and
    /// grouped so each submesh's range is contiguous.
    pub indices: Vec<u16>,
    /// World-space bounds of every instance in this batch, used for frustum
    /// culling. One batch covers one model inside one spatial cell, so a prop
    /// field spread over a level becomes several cullable ranges of the same
    /// model instead of one range spanning the whole level.
    pub bounds: crate::engine::spatial::Aabb,
}

/// Index lists accumulated per primitive while instances are appended.
///
/// Grouping the indices here (rather than keeping each instance's runs inline)
/// is what lets the finished batch draw one material across every instance in
/// one call: instance order never leaks into the draw calls.
struct BatchBuilder {
    model: String,
    light_control: Option<u32>,
    casts_static_lighting: bool,
    pending_probe_lighting: bool,
    textures: Vec<Arc<crate::engine::loader::RawImage>>,
    primitives: Vec<PrimitiveBuilder>,
    vertices: Vec<Vertex>,
    bounds: crate::engine::spatial::Aabb,
    /// Total indices accumulated, so a new instance can be rejected before it
    /// writes anything it cannot finish.
    index_count: usize,
    lighting_sources: Vec<usize>,
    sampled_light: Vec<crate::engine::lighting::LightColor>,
}

struct PrimitiveBuilder {
    /// Shared scalar material response for every mesh route.
    response: crate::engine::materials::MaterialResponse,
    texture: Option<u16>,
    emission: MaterialEmission,
    alpha: MaterialAlpha,
    indices: Vec<u16>,
}

impl BatchBuilder {
    fn new(
        model_path: &str,
        model: &crate::engine::gltf::PropModel,
        textures: Vec<Arc<crate::engine::loader::RawImage>>,
    ) -> Self {
        Self {
            model: model_path.to_string(),
            light_control: None,
            casts_static_lighting: true,
            pending_probe_lighting: false,
            textures,
            primitives: model
                .submeshes
                .iter()
                .map(|submesh| PrimitiveBuilder {
                    response: submesh.response,
                    texture: submesh.texture,
                    emission: submesh.emission,
                    alpha: submesh.alpha,
                    indices: Vec::new(),
                })
                .collect(),
            vertices: Vec::with_capacity(model.vertices.len()),
            bounds: crate::engine::spatial::Aabb::EMPTY,
            index_count: 0,
            lighting_sources: lighting_sources(&model.vertices),
            sampled_light: Vec::with_capacity(model.vertices.len()),
        }
    }

    /// Prepare independent luminous uniforms while retaining shared model images.
    fn for_prop(
        asset: &crate::engine::props::LoadedPropAsset,
        prop: &PropDef,
        light_control: Option<u32>,
        casts_static_lighting: bool,
    ) -> Self {
        let mut builder = Self::new(
            &asset.model_path,
            &asset.model,
            asset.model.textures.clone(),
        );
        builder.casts_static_lighting = casts_static_lighting;
        builder.light_control = light_control;
        for (primitive, source) in builder.primitives.iter_mut().zip(&asset.model.submeshes) {
            if let Some(emission) = prop.emission_overrides.get(&source.material) {
                primitive.emission.color = emission.color;
                primitive.emission.intensity = emission.intensity;
            }
        }
        builder
    }

    /// Append one fixed placement through its resolved lighting route.
    fn push_resolved_instance(
        &mut self,
        transform: &glam::Mat4,
        asset: &crate::engine::props::LoadedPropAsset,
        lighting: &LevelLighting,
        plan: Option<&mut crate::engine::lighting::lightmap::LightmapPlan>,
        bounds: &crate::engine::spatial::Aabb,
        options: (bool, Option<[f32; 3]>),
    ) {
        let (defer_static_lighting, tint) = options;
        let probe_lit = plan.is_some()
            && !asset.model.is_animatable()
            && dense_large_model(&asset.model, bounds);
        self.pending_probe_lighting |= probe_lit;
        self.bounds = self.bounds.union(bounds);
        let first_vertex = self.vertices.len();
        if !asset.model.is_animatable()
            && !probe_lit
            && let Some(active_plan) = plan
        {
            self.push_lightmapped_instance(transform, asset, active_plan, bounds);
        } else {
            self.push_instance(
                transform,
                asset,
                lighting,
                (defer_static_lighting || probe_lit) && !asset.model.is_animatable(),
            );
        }
        self.tint_instance(first_vertex, tint);
    }

    /// Whether one more instance of `model` fits this batch's 16-bit offsets.
    fn has_room_for(&self, model: &crate::engine::gltf::PropModel, lightmapped: bool) -> bool {
        self.vertices
            .len()
            .saturating_add(instance_vertex_count(model, lightmapped))
            <= crate::engine::spatial::MAX_INDEX_VERTICES
    }

    /// Transforms and appends one instance. The caller must have checked
    /// [`Self::has_room_for`].
    fn push_instance(
        &mut self,
        transform: &glam::Mat4,
        asset: &crate::engine::props::LoadedPropAsset,
        lighting: &LevelLighting,
        defer_lighting: bool,
    ) {
        let model = &asset.model;
        if model.vertices.iter().any(|v| v.normal.is_none()) {
            // Missing NORMAL means flat triangles, including the vertex-lit
            // fallback. Shared corners must not average across hard edges.
            let normal_matrix = transform.inverse().transpose();
            for (slot, submesh) in model.submeshes.iter().enumerate() {
                let Some(primitive) = self.primitives.get_mut(slot) else {
                    continue;
                };
                let start = usize::try_from(submesh.first_index).unwrap_or(0);
                let count = usize::try_from(submesh.index_count).unwrap_or(0);
                let Some(indices) = model.indices.get(start..start.saturating_add(count)) else {
                    continue;
                };
                for &[index_a, index_b, index_c] in indices.as_chunks::<3>().0 {
                    let (Some(a), Some(b), Some(c)) = (
                        model.vertices.get(usize::from(index_a)),
                        model.vertices.get(usize::from(index_b)),
                        model.vertices.get(usize::from(index_c)),
                    ) else {
                        continue;
                    };
                    let base = u16::try_from(self.vertices.len()).unwrap_or(0);
                    let Some(frames) =
                        model_triangle_vertices([a, b, c], transform, &normal_matrix)
                    else {
                        continue;
                    };
                    for mut vertex in frames {
                        if !defer_lighting {
                            let [position_x, position_y, position_z] = vertex.pos;
                            let light = lighting
                                .sample(position_x, position_y, position_z)
                                .plus(lighting.global_surface_light(vertex.pos, vertex.normal));
                            let [red, green, blue, alpha] = vertex.color;
                            vertex.color = [red * light.r, green * light.g, blue * light.b, alpha];
                        }
                        self.vertices.push(vertex);
                    }
                    primitive.indices.extend([
                        base,
                        base.saturating_add(1),
                        base.saturating_add(2),
                    ]);
                    self.index_count = self.index_count.saturating_add(3);
                }
            }
            return;
        }
        if defer_lighting {
            // Preserve albedo until receivers or prepared probes light it.
            // Normals remain available for probe and flashlight response.
            let normal_matrix = transform.inverse().transpose();
            self.vertices.reserve(model.vertices.len());
            self.vertices.extend(model.vertices.iter().map(|vertex| {
                Vertex {
                    pos: transform
                        .transform_point3(glam::Vec3::from_array(vertex.pos))
                        .to_array(),
                    color: vertex.color,
                    caustics_albedo: [vertex.color[0], vertex.color[1], vertex.color[2]],
                    uv: vertex.uv,
                    normal: vertex.normal.map_or([0.0; 3], |normal| {
                        normal_matrix
                            .transform_vector3(glam::Vec3::from_array(normal))
                            .normalize_or_zero()
                            .to_array()
                    }),
                    ..Vertex::UNLIT
                }
            }));
        } else {
            append_instance_vertices(
                &mut self.vertices,
                transform,
                &model.vertices,
                lighting,
                &self.lighting_sources,
                &mut self.sampled_light,
            );
        }
        let base =
            u16::try_from(self.vertices.len().saturating_sub(model.vertices.len())).unwrap_or(0);
        for (slot, submesh) in model.submeshes.iter().enumerate() {
            let Some(primitive) = self.primitives.get_mut(slot) else {
                continue;
            };
            let start = usize::try_from(submesh.first_index).unwrap_or(0);
            let count = usize::try_from(submesh.index_count).unwrap_or(0);
            let Some(range) = model.indices.get(start..start.saturating_add(count)) else {
                continue;
            };
            primitive
                .indices
                .extend(range.iter().map(|index| base.saturating_add(*index)));
            self.index_count = self.index_count.saturating_add(count);
        }
    }

    /// Static models keep their source albedo and receive the same physically
    /// solved HDR atlas as architecture. Animated models retain the probe path.
    fn push_lightmapped_instance(
        &mut self,
        transform: &glam::Mat4,
        asset: &crate::engine::props::LoadedPropAsset,
        plan: &mut crate::engine::lighting::lightmap::LightmapPlan,
        bounds: &crate::engine::spatial::Aabb,
    ) {
        let source = &asset.model;
        let normal_matrix = transform.inverse().transpose();
        let large = bounds
            .max
            .iter()
            .zip(bounds.min)
            .any(|(high, low)| high - low > 3.0);
        for (slot, submesh) in source.submeshes.iter().enumerate() {
            let Some(primitive) = self.primitives.get_mut(slot) else {
                continue;
            };
            let start = usize::try_from(submesh.first_index).unwrap_or(0);
            let count = usize::try_from(submesh.index_count).unwrap_or(0);
            let Some(indices) = source.indices.get(start..start.saturating_add(count)) else {
                continue;
            };
            let density = plan.prop_texels_per_metre(
                large,
                submesh.alpha.mode == crate::engine::materials::AlphaMode::Cutout,
            );
            // The illumination domain may be shared; source corners, triangle
            // order and all material/shader attributes remain independent.
            let triangles = indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter_map(|triangle| {
                    let [Some(a), Some(b), Some(c)] =
                        triangle.map(|index| source.vertices.get(usize::from(index)))
                    else {
                        return None;
                    };
                    model_triangle_vertices([a, b, c], transform, &normal_matrix)
                })
                .collect();
            for vertices in lightmap_prop_triangles(triangles, plan, density) {
                let Ok(base) = u16::try_from(self.vertices.len()) else {
                    continue;
                };
                self.vertices.extend(vertices);
                primitive
                    .indices
                    .extend([base, base.saturating_add(1), base.saturating_add(2)]);
                self.index_count = self.index_count.saturating_add(3);
            }
        }
    }

    fn tint_instance(&mut self, first_vertex: usize, preset: Option<[f32; 3]>) {
        if let Some(tint) = preset {
            for vertex in self.vertices.get_mut(first_vertex..).into_iter().flatten() {
                for (channel, factor) in vertex.color.iter_mut().take(3).zip(tint) {
                    *channel *= factor;
                }
                for (channel, factor) in vertex.caustics_albedo.iter_mut().zip(tint) {
                    *channel *= factor;
                }
            }
        }
    }

    fn finish(self) -> PropMeshBatch {
        let mut indices: Vec<u16> = Vec::with_capacity(self.index_count);
        let mut submeshes: Vec<PropSubmeshBatch> = Vec::with_capacity(self.primitives.len());
        let mut source_primitives = Vec::with_capacity(self.primitives.len());
        for (source_primitive, primitive) in self.primitives.into_iter().enumerate() {
            if primitive.indices.is_empty() {
                continue;
            }
            let first_index = u32::try_from(indices.len()).unwrap_or(0);
            let index_count = u32::try_from(primitive.indices.len()).unwrap_or(0);
            indices.extend_from_slice(&primitive.indices);
            source_primitives.push(source_primitive);
            submeshes.push(PropSubmeshBatch {
                response: primitive.response,
                texture: primitive.texture,
                emission: primitive.emission,
                alpha: primitive.alpha,
                first_index,
                index_count,
            });
        }
        PropMeshBatch {
            model: self.model,
            light_control: self.light_control,
            casts_static_lighting: self.casts_static_lighting,
            pending_probe_lighting: self.pending_probe_lighting,
            textures: self.textures,
            submeshes,
            source_primitives,
            vertices: self.vertices,
            indices,
            bounds: self.bounds,
        }
    }
}

type PropPositionKey = [u32; 3];
type PropEdgeKey = (PropPositionKey, PropPositionKey);

#[derive(Clone, Copy)]
struct SharedPropChart {
    partner: usize,
    corners: [[f32; 3]; 4],
}

/// Exact source corners, with signed zero canonicalized for adjacency only.
fn prop_position_key(position: [f32; 3]) -> PropPositionKey {
    position.map(|value| {
        let bits = value.to_bits();
        if bits == 0x8000_0000 { 0 } else { bits }
    })
}

fn prop_triangle_edges(triangle: &[Vertex; 3]) -> [PropEdgeKey; 3] {
    let [a, b, c] = triangle.map(|vertex| prop_position_key(vertex.pos));
    [(a, b), (b, c), (c, a)]
}

// glam's bounded floating-point vector operations cannot overflow an integer
// or panic; the existing patch guard rejects nonfinite/degenerate geometry.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded float normal frame math; native patch guards reject unusable domains"
)]
fn prop_triangle_normal(triangle: &[Vertex; 3]) -> glam::Vec3 {
    let [a, b, c] = triangle.map(|vertex| glam::Vec3::from_array(vertex.pos));
    (b - a).cross(c - a).normalize_or_zero()
}

/// Recover only a native-supported four-corner union. The common source edge
/// is always p0/p2, so a tapered quad follows exactly the original two triangles.
fn prop_quad_corners(first: &[Vertex; 3], second: &[Vertex; 3]) -> Option<[[f32; 3]; 4]> {
    use std::collections::BTreeMap;
    // Do not recover a previously rejected sliver merely because its neighbor
    // has a larger frame. Unsupported pairs keep the original triangle path.
    for triangle in [first, second] {
        let [a, b, c] = triangle.map(|vertex| vertex.pos);
        let _triangle_domain = crate::engine::lighting::lightmap::LightmapPatch::from_quad(
            crate::engine::lighting::lightmap::PatchKind::Prop,
            [a, b, c, c],
            None,
        )?;
    }
    let common: Vec<_> = first
        .iter()
        .filter(|vertex| {
            second
                .iter()
                .any(|other| prop_position_key(vertex.pos) == prop_position_key(other.pos))
        })
        .collect();
    let [start, opposite] = common.as_slice() else {
        return None;
    };
    let positions: BTreeMap<_, _> = first
        .iter()
        .chain(second)
        .map(|vertex| (prop_position_key(vertex.pos), vertex.pos))
        .collect();
    if positions.len() != 4
        || prop_triangle_normal(first).dot(prop_triangle_normal(second)) < 0.999_999
    {
        return None;
    }
    for vertex in &common {
        let other = second
            .iter()
            .find(|other| prop_position_key(vertex.pos) == prop_position_key(other.pos))?;
        let alignment =
            glam::Vec3::from_array(vertex.normal).dot(glam::Vec3::from_array(other.normal));
        if !alignment.is_finite() || alignment < 8.0_f32.mul_add(-f32::EPSILON, 1.0) {
            return None;
        }
    }
    let common_keys = [
        prop_position_key(start.pos),
        prop_position_key(opposite.pos),
    ];
    let mut boundary = BTreeMap::new();
    for (from, to) in prop_triangle_edges(first)
        .into_iter()
        .chain(prop_triangle_edges(second))
    {
        if common_keys.contains(&from) && common_keys.contains(&to) {
            continue;
        }
        // Consistent winding gives exactly one outgoing boundary edge per
        // corner. Overlaps, reversed faces and bowties cannot form this loop.
        if boundary.insert(from, to).is_some() {
            return None;
        }
    }
    let mut key = prop_position_key(start.pos);
    let mut corners = [[0.0; 3]; 4];
    for corner in &mut corners {
        *corner = *positions.get(&key)?;
        key = *boundary.get(&key)?;
    }
    let [p0, _, p2, _] = corners;
    if key != prop_position_key(p0) || prop_position_key(p2) != prop_position_key(opposite.pos) {
        return None;
    }
    let patch = crate::engine::lighting::lightmap::LightmapPatch::from_quad(
        crate::engine::lighting::lightmap::PatchKind::Prop,
        corners,
        None,
    )?;
    (!patch.is_triangular()).then_some(corners)
}

/// Pair manifold shared edges only within this placed instance and primitive.
/// Sorting by source triangle order keeps allocation and ambiguous pairing
/// deterministic without changing the source mesh's emission order.
fn shared_prop_charts(triangles: &[[Vertex; 3]]) -> Vec<Option<SharedPropChart>> {
    use std::collections::BTreeMap;
    let mut edges: BTreeMap<PropEdgeKey, Vec<usize>> = BTreeMap::new();
    for (index, triangle) in triangles.iter().enumerate() {
        for (a, b) in prop_triangle_edges(triangle) {
            edges.entry((a.min(b), a.max(b))).or_default().push(index);
        }
    }
    let mut candidates = Vec::new();
    for neighbors in edges.values() {
        let &[first, second] = neighbors.as_slice() else {
            continue;
        };
        let (Some(left), Some(right)) = (triangles.get(first), triangles.get(second)) else {
            continue;
        };
        if let Some(corners) = prop_quad_corners(left, right) {
            candidates.push((first, second, corners));
        }
    }
    candidates.sort_by_key(|(first, second, _)| (*first, *second));
    let mut pairs = vec![None; triangles.len()];
    for (first, second, corners) in candidates {
        if pairs.get(first).is_some_and(Option::is_none)
            && pairs.get(second).is_some_and(Option::is_none)
        {
            if let Some(slot) = pairs.get_mut(first) {
                *slot = Some(SharedPropChart {
                    partner: second,
                    corners,
                });
            }
            if let Some(slot) = pairs.get_mut(second) {
                *slot = Some(SharedPropChart {
                    partner: first,
                    corners,
                });
            }
        }
    }
    pairs
}

fn stamp_shared_prop_chart(
    triangles: &mut [[Vertex; 3]],
    first: usize,
    pair: SharedPropChart,
    plan: &mut crate::engine::lighting::lightmap::LightmapPlan,
    density: f32,
) -> bool {
    if first >= pair.partner {
        return false;
    }
    let Some((earlier, later)) = triangles.split_at_mut_checked(pair.partner) else {
        return false;
    };
    let (Some(left), Some(right)) = (earlier.get_mut(first), later.first_mut()) else {
        return false;
    };
    plan.stamp_prop_quad(left, right, pair.corners, density)
}

fn lightmap_prop_triangles(
    mut triangles: Vec<[Vertex; 3]>,
    plan: &mut crate::engine::lighting::lightmap::LightmapPlan,
    density: f32,
) -> Vec<[Vertex; 3]> {
    let pairs = shared_prop_charts(&triangles);
    let mut stamped = vec![false; triangles.len()];
    let mut retained = Vec::with_capacity(triangles.len());
    for index in 0..triangles.len() {
        let already_stamped = stamped.get(index).copied().unwrap_or(false);
        let shared = !already_stamped
            && pairs.get(index).copied().flatten().is_some_and(|pair| {
                let accepted = stamp_shared_prop_chart(&mut triangles, index, pair, plan, density);
                if accepted && let Some(slot) = stamped.get_mut(pair.partner) {
                    *slot = true;
                }
                accepted
            });
        let Some(triangle) = triangles.get_mut(index) else {
            continue;
        };
        if already_stamped || shared || plan.stamp_prop_triangle(triangle, density) || plan.failed()
        {
            retained.push(*triangle);
        }
    }
    retained
}

/// Resolves every placed prop into either a batched real mesh or a fallback box,
/// sharing one decoded model (and its textures) per distinct model path.
///
/// Baked lighting is sampled per transformed vertex in world space, so a prop
/// standing on a crate or lying on a bed is lit at its real height and still
/// contributes to the same shared per-model batch (one draw call per primitive
/// per model and cell).
pub fn resolve_prop_instances<'a>(
    level: &'a LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
    materials: &crate::engine::materials::MaterialTable,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    resolve_prop_instances_inner(
        level,
        catalog,
        assets,
        lighting,
        surfaces,
        materials,
        PropLightingMode::Vertex,
    )
}

/// Preserve historical model/batch budgets and fallback discovery without
/// evaluating lighting that a successful static lightmap plan replaces.
/// Animated models still receive their normal vertex lighting.
pub(super) fn resolve_prop_fallbacks<'a>(
    level: &'a LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
    materials: &crate::engine::materials::MaterialTable,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    resolve_prop_instances_inner(
        level,
        catalog,
        assets,
        lighting,
        surfaces,
        materials,
        PropLightingMode::Deferred,
    )
}

/// Resolves static surface receivers after architecture has reserved its atlas
/// space. Moving/animated models continue to use their independent probe path.
pub fn resolve_prop_instances_lightmapped<'a>(
    level: &'a LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
    materials: &crate::engine::materials::MaterialTable,
    plan: &mut crate::engine::lighting::lightmap::LightmapPlan,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    resolve_prop_instances_inner(
        level,
        catalog,
        assets,
        lighting,
        surfaces,
        materials,
        PropLightingMode::Lightmap(plan),
    )
}

enum PropLightingMode<'a> {
    Vertex,
    Deferred,
    Lightmap(&'a mut crate::engine::lighting::lightmap::LightmapPlan),
}

impl<'a> PropLightingMode<'a> {
    const fn into_options(
        self,
    ) -> (
        Option<&'a mut crate::engine::lighting::lightmap::LightmapPlan>,
        bool,
    ) {
        match self {
            Self::Vertex => (None, false),
            Self::Deferred => (None, true),
            Self::Lightmap(plan) => (Some(plan), false),
        }
    }
}

use crate::engine::spatial::CellKey;

type PropVariantKey = (String, std::collections::BTreeMap<u16, String>);
type PropVariantCache =
    std::collections::HashMap<PropVariantKey, Arc<crate::engine::props::LoadedPropAsset>>;

// one bounded deterministic model/cell batching pass
fn resolve_prop_instances_inner<'a>(
    level: &'a LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
    materials: &crate::engine::materials::MaterialTable,
    mode: PropLightingMode<'_>,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    use std::collections::{HashMap, HashSet};

    let (mut plan, defer_static_lighting) = mode.into_options();
    let grid = spatial_cell_grid(level);
    let claimed = super::character::claimed_character_models(level, catalog, assets);
    let mut builders: Vec<BatchBuilder> = Vec::new();
    // Variants share decoded input images; each material combination is built
    // once during preparation and receives its own compatible draw ranges.
    let mut variants = PropVariantCache::new();
    let mut index_by_batch: HashMap<(PropVariantKey, CellKey, Option<u32>), usize> = HashMap::new();
    let mut models_seen: HashSet<String> = HashSet::new();
    let mut fallbacks: Vec<&'a PropDef> = Vec::new();
    let mut busy_vertices = 0usize;

    for (prop_index, prop) in level.props.iter().enumerate() {
        // Movable props use the dynamic path at their live pose. Neither a
        // static mesh nor a placeholder may remain at the authored position:
        // both would draw twice and leave immutable lighting casters behind.
        if prop.shadow_only || prop.float.is_some() || prop.is_physics_movable() {
            continue;
        }
        let Some(model_path) = catalog.get(&prop.model).model else {
            fallbacks.push(prop);
            continue;
        };
        if busy_vertices >= crate::engine::level::MAX_LEVEL_PROP_VERTICES {
            fallbacks.push(prop);
            continue;
        }
        let variant_key = (model_path.clone(), prop.material_overrides.clone());
        let asset = match resolve_placed_asset(prop, &variant_key, assets, materials, &mut variants)
        {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&model_path, &error);
                fallbacks.push(prop);
                continue;
            }
        };
        if !models_seen.contains(&model_path)
            && models_seen.len() >= crate::engine::level::MAX_LEVEL_PROP_MODELS
        {
            fallbacks.push(prop);
            continue;
        }

        // A prop's authored `y` is an offset above the local walkable floor, so
        // a chair in an elevated room or a recessed region lands on the surface
        // it was placed against.
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        let model = prop_instance_matrix(prop, base_y);
        // Cull by the instance's real world-space extent, not by the cell it
        // happens to be centred in: a chair on a cell boundary must not be
        // culled while a sliver of it is still on screen.
        let instance_bounds = prop_instance_bounds(&asset.model, &model, prop, base_y);
        let lightmapped = plan.is_some()
            && !asset.model.is_animatable()
            && !dense_large_model(&asset.model, &instance_bounds);
        let cell = grid.cell_of(instance_bounds.centre());

        // One batch holds every instance of a model inside one spatial cell, but
        // never more than a 16-bit index can address: `PropMeshBatch::indices`
        // are `GL_UNSIGNED_SHORT` offsets into the batch's own vertex list, so a
        // cell holding hundreds of instances has to become several batches.
        let light_control = prop.light_control_index(level.ceiling_lights.len(), prop_index);
        let key = (variant_key, cell, light_control);
        let batch_index = match index_by_batch.get(&key).copied() {
            Some(index)
                if builders
                    .get(index)
                    .is_some_and(|b| b.has_room_for(&asset.model, lightmapped)) =>
            {
                index
            }
            _ => {
                let _new_entry = models_seen.insert(model_path.clone());
                let builder = BatchBuilder::for_prop(
                    &asset,
                    prop,
                    light_control,
                    !claimed.contains(&model_path),
                );
                if !builder.has_room_for(&asset.model, lightmapped) {
                    fallbacks.push(prop);
                    continue;
                }
                builders.push(builder);
                let index = builders.len().saturating_sub(1);
                let _previous_value = index_by_batch.insert(key.clone(), index);
                index
            }
        };
        let Some(builder) = builders.get_mut(batch_index) else {
            fallbacks.push(prop);
            continue;
        };
        builder.push_resolved_instance(
            &model,
            &asset,
            lighting,
            plan.as_deref_mut(),
            &instance_bounds,
            (defer_static_lighting, catalog.tint(&prop.model)),
        );
        busy_vertices =
            busy_vertices.saturating_add(instance_vertex_count(&asset.model, lightmapped));
    }

    let mut batches: Vec<_> = builders.into_iter().map(BatchBuilder::finish).collect();
    batches.extend(resolve_fixture_model_instances(
        level,
        catalog,
        assets,
        lighting,
        plan,
        defer_static_lighting,
        (busy_vertices, models_seen),
    ));
    (batches, fallbacks)
}

/// Reviewed models beyond the ordinary art budget cannot afford one
/// padded receiver per tiny face. Large dense scenery retains indexed geometry
/// and source albedo for transport, then receives the prepared irradiance field.
fn dense_large_model(
    model: &crate::engine::gltf::PropModel,
    bounds: &crate::engine::spatial::Aabb,
) -> bool {
    model.triangles > crate::engine::level::PROP_TRIANGLE_BUDGET
        && bounds
            .max
            .iter()
            .zip(bounds.min)
            .any(|(high, low)| high - low > 3.0)
}

/// Light the deferred, uncharted static scenery after transport has consumed
/// its original albedo. Valid dark probes stay dark; missing fields use the
/// established environment lighting, never an unlit/full-bright colour.
pub fn relight_probe_props(
    batches: &mut [PropMeshBatch],
    lighting: &LevelLighting,
    field: Option<&crate::engine::lighting::probes::ProbeField>,
) {
    for batch in batches
        .iter_mut()
        .filter(|batch| batch.pending_probe_lighting)
    {
        for vertex in batch
            .vertices
            .iter_mut()
            .filter(|vertex| vertex.lightmap_page == super::LIGHTMAP_NONE)
        {
            let position = glam::Vec3::from_array(vertex.pos);
            let normal = glam::Vec3::from_array(vertex.normal).normalize_or_zero();
            let sample = normal.mul_add(glam::Vec3::splat(0.025), position);
            let room = lighting.room_index_at_height(sample.x, sample.y, sample.z);
            let display = field
                .and_then(|prepared| prepared.sample(sample.to_array(), room))
                .map_or_else(
                    || {
                        let light = lighting
                            .sample(position.x, position.y, position.z)
                            .plus(lighting.global_surface_light(vertex.pos, normal.to_array()));
                        [light.r, light.g, light.b]
                    },
                    |texel| texel.light_at(normal.to_array()),
                );
            for (channel, light) in vertex.color.iter_mut().take(3).zip(display) {
                *channel *= light;
            }
        }
        batch.pending_probe_lighting = false;
    }
}

fn prop_instance_bounds(
    model: &crate::engine::gltf::PropModel,
    transform: &glam::Mat4,
    prop: &PropDef,
    base_y: f32,
) -> crate::engine::spatial::Aabb {
    model.bounds().map_or_else(
        || crate::engine::spatial::Aabb::from_point([prop.x, base_y + prop.y, prop.z]),
        |(min, max)| transform_bounds(&crate::engine::spatial::Aabb { min, max }, transform),
    )
}

/// Resolve immutable material inputs and validate fitted emission slot references.
fn resolve_placed_asset(
    prop: &PropDef,
    key: &PropVariantKey,
    assets: &mut crate::engine::props::PropAssets,
    materials: &crate::engine::materials::MaterialTable,
    variants: &mut PropVariantCache,
) -> Result<Arc<crate::engine::props::LoadedPropAsset>, String> {
    let source = assets.resolve(&key.0)?;
    let asset = resolve_material_variant(source, key, materials, variants)?;
    if prop.emission_overrides.keys().any(|slot| {
        !asset
            .model
            .submeshes
            .iter()
            .any(|part| part.material == *slot)
    }) {
        return Err("emission override names an absent material slot".into());
    }
    Ok(asset)
}

fn resolve_material_variant(
    source: Arc<crate::engine::props::LoadedPropAsset>,
    key: &PropVariantKey,
    materials: &crate::engine::materials::MaterialTable,
    variants: &mut PropVariantCache,
) -> Result<Arc<crate::engine::props::LoadedPropAsset>, String> {
    if key.1.is_empty() {
        return Ok(source);
    }
    if let Some(variant) = variants.get(key) {
        return Ok(Arc::clone(variant));
    }
    if variants.len() >= crate::engine::level::MAX_LEVEL_PROP_MODELS {
        return Err("static prop material variant budget exceeded".into());
    }
    let variant = Arc::new(override_materials(&source, &key.1, materials)?);
    let _previous = variants.insert(key.clone(), Arc::clone(&variant));
    Ok(variant)
}

/// Resolves one immutable material variant. Vertex/index arrays are cloned
/// once per distinct override set; decoded PNGs remain shared through Arc.
fn override_materials(
    source: &crate::engine::props::LoadedPropAsset,
    overrides: &std::collections::BTreeMap<u16, String>,
    materials: &crate::engine::materials::MaterialTable,
) -> Result<crate::engine::props::LoadedPropAsset, String> {
    if source.model.is_animatable() {
        return Err("material overrides require a static model".into());
    }
    let mut model = source.model.clone();
    for (&slot, id) in overrides {
        let material = materials
            .entry_of(id)
            .ok_or_else(|| format!("unknown material override `{id}`"))?;
        if let Some(error) = &material.error {
            return Err(format!("material override `{id}`: {error}"));
        }
        if !model
            .submeshes
            .iter()
            .any(|submesh| submesh.material == slot)
        {
            return Err(format!(
                "material override slot {slot} is absent from {}",
                source.model_path
            ));
        }
        if model.submeshes.iter().any(|submesh| {
            submesh.material == slot
                && submesh.alpha.mode != crate::engine::materials::AlphaMode::Opaque
        }) {
            return Err("material overrides require opaque source slots".into());
        }
        let (texture, response, emission) =
            append_material_images(&mut model.textures, material, materials)?;
        for submesh in &mut model.submeshes {
            if submesh.material != slot {
                continue;
            }
            submesh.texture = Some(texture);
            submesh.response = response;
            submesh.emission = emission;
            submesh.alpha = material.alpha;
            let start = usize::try_from(submesh.first_index).unwrap_or(usize::MAX);
            let end = start.saturating_add(usize::try_from(submesh.index_count).unwrap_or(0));
            for index in model.indices.get(start..end).into_iter().flatten() {
                if let Some(vertex) = model.vertices.get_mut(usize::from(*index)) {
                    let [red, green, blue] = material.tint;
                    vertex.color = [red, green, blue, 1.0];
                }
            }
        }
    }
    Ok(crate::engine::props::LoadedPropAsset {
        model_path: crate::engine::props::model_variant_identity(&source.model_path, overrides),
        model,
    })
}

/// Package loading restores only shared image references; prepared vertices,
/// material ranges and normal frames are already stored in the prop record.
///
/// # Errors
/// Returns an error for an unresolved or non-opaque material override.
pub fn override_material_textures(
    source: &crate::engine::props::LoadedPropAsset,
    overrides: &std::collections::BTreeMap<u16, String>,
    materials: &crate::engine::materials::MaterialTable,
) -> Result<Vec<Arc<crate::engine::loader::RawImage>>, String> {
    let mut textures = source.model.textures.clone();
    for id in overrides.values() {
        let material = materials
            .entry_of(id)
            .ok_or_else(|| format!("unknown material override `{id}`"))?;
        let _binding = append_material_images(&mut textures, material, materials)?;
    }
    Ok(textures)
}

fn append_material_images(
    textures: &mut Vec<Arc<crate::engine::loader::RawImage>>,
    material: &crate::engine::materials::ResolvedMaterial,
    materials: &crate::engine::materials::MaterialTable,
) -> Result<
    (
        u16,
        crate::engine::materials::MaterialResponse,
        MaterialEmission,
    ),
    String,
> {
    if material.alpha.mode != crate::engine::materials::AlphaMode::Opaque {
        return Err(format!(
            "material override `{}` must be opaque",
            material.id
        ));
    }
    if let Some(error) = &material.error {
        return Err(format!("material override `{}`: {error}", material.id));
    }
    let image = material
        .image
        .as_ref()
        .ok_or_else(|| format!("material override `{}` has no decoded PNG", material.id))?;
    let texture = intern_override_image(textures, image)?;
    let mut response = material.response;
    response.normal = remap_override_image(response.normal, materials, textures)?;
    response.fluorescence.mask =
        remap_override_image(response.fluorescence.mask, materials, textures)?;
    let mut emission = material.emission;
    emission.mask = remap_override_image(emission.mask, materials, textures)?;
    Ok((texture, response, emission))
}

fn intern_override_image(
    textures: &mut Vec<Arc<crate::engine::loader::RawImage>>,
    image: &Arc<crate::engine::loader::RawImage>,
) -> Result<u16, String> {
    if let Some(index) = textures
        .iter()
        .position(|existing| Arc::ptr_eq(existing, image))
    {
        return u16::try_from(index).map_err(|error| error.to_string());
    }
    let index = u16::try_from(textures.len()).map_err(|error| error.to_string())?;
    textures.push(Arc::clone(image));
    Ok(index)
}

fn remap_override_image(
    source_index: Option<u32>,
    materials: &crate::engine::materials::MaterialTable,
    textures: &mut Vec<Arc<crate::engine::loader::RawImage>>,
) -> Result<Option<u32>, String> {
    source_index
        .map(|index| {
            let image = materials
                .textures()
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .ok_or_else(|| "material override references a missing texture".to_string())?;
            intern_override_image(textures, &image.image).map(u32::from)
        })
        .transpose()
}

/// Flat triangles and atlas receivers need independent corners at hard edges.
fn instance_vertex_count(model: &crate::engine::gltf::PropModel, lightmapped: bool) -> usize {
    if lightmapped || model.vertices.iter().any(|vertex| vertex.normal.is_none()) {
        model.indices.len()
    } else {
        model.vertices.len()
    }
}

/// Derives a stable UV tangent frame for each actual triangle while retaining
/// valid smooth normals. Lighting coordinates never replace the source UVs.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded float frame math; degenerate triangles are rejected"
)] // bounded float frame math; degenerate triangles are rejected
fn model_triangle_vertices(
    source: [&crate::engine::gltf::PropVertex; 3],
    transform: &glam::Mat4,
    normal_matrix: &glam::Mat4,
) -> Option<[Vertex; 3]> {
    let [a, b, c] =
        source.map(|vertex| transform.transform_point3(glam::Vec3::from_array(vertex.pos)));
    let edge_u = b - a;
    let edge_v = c - a;
    let face = edge_u.cross(edge_v).normalize_or_zero();
    // Zero-area source triangles draw no pixels and cannot be receivers.
    if face == glam::Vec3::ZERO {
        return None;
    }
    let [uv_a, uv_b, uv_c] = source.map(|vertex| glam::Vec2::from_array(vertex.uv));
    let du = uv_b - uv_a;
    let dv = uv_c - uv_a;
    let determinant = du.x.mul_add(dv.y, -(du.y * dv.x));
    let (tangent, bitangent) = if determinant.abs() > 1.0e-8 {
        (
            (edge_u * dv.y - edge_v * du.y) / determinant,
            (edge_v * du.x - edge_u * dv.x) / determinant,
        )
    } else {
        (edge_u, edge_v)
    };
    Some(source.map(|vertex| {
        let normal = vertex.normal.map_or(face, |normal| {
            normal_matrix
                .transform_vector3(glam::Vec3::from_array(normal))
                .normalize_or_zero()
        });
        let projected = (tangent - normal * tangent.dot(normal)).normalize_or_zero();
        let frame_u = if projected == glam::Vec3::ZERO {
            normal.any_orthonormal_vector()
        } else {
            projected
        };
        Vertex {
            pos: transform
                .transform_point3(glam::Vec3::from_array(vertex.pos))
                .to_array(),
            color: vertex.color,
            caustics_albedo: [vertex.color[0], vertex.color[1], vertex.color[2]],
            uv: vertex.uv,
            normal: normal.to_array(),
            tangent: frame_u.to_array(),
            handedness: if normal.cross(frame_u).dot(bitangent) < 0.0 {
                -1.0
            } else {
                1.0
            },
            ..Vertex::UNLIT
        }
    }))
}

/// UV seams and hard edges duplicate positions. Share only their light sample;
/// colours, UVs, vertex order and per-placement transforms remain independent.
fn lighting_sources(vertices: &[crate::engine::gltf::PropVertex]) -> Vec<usize> {
    let mut first = std::collections::HashMap::with_capacity(vertices.len());
    vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| *first.entry(vertex.pos.map(f32::to_bits)).or_insert(index))
        .collect()
}

/// Transforms and lights one instance's model vertices.
///
/// Every model vertex is transformed, and equal positions share lighting per
/// placement, and the environment is baked into the instance's colour: the same
/// model in a dark corner and under a fixture still shares one batch, but is no
/// longer uniformly lit.
fn append_instance_vertices(
    batch: &mut Vec<Vertex>,
    model: &glam::Mat4,
    source: &[crate::engine::gltf::PropVertex],
    lighting: &LevelLighting,
    representatives: &[usize],
    samples: &mut Vec<crate::engine::lighting::LightColor>,
) {
    samples.clear();
    batch.reserve(source.len());
    let normal_matrix = model.inverse().transpose();
    for (index, vertex) in source.iter().enumerate() {
        let position =
            model.transform_point3(glam::Vec3::new(vertex.pos[0], vertex.pos[1], vertex.pos[2]));
        let light = representatives
            .get(index)
            .and_then(|first| samples.get(*first))
            .copied()
            .unwrap_or_else(|| lighting.sample(position.x, position.y, position.z));
        samples.push(light);
        let normal = normal_matrix
            .transform_vector3(glam::Vec3::from_array(
                vertex.normal.unwrap_or([0.0, 1.0, 0.0]),
            ))
            .normalize_or_zero();
        let surface_light =
            light.plus(lighting.global_surface_light(position.to_array(), normal.to_array()));
        batch.push(Vertex {
            pos: [position.x, position.y, position.z],
            caustics_albedo: [vertex.color[0], vertex.color[1], vertex.color[2]],
            color: [
                vertex.color[0] * surface_light.r,
                vertex.color[1] * surface_light.g,
                vertex.color[2] * surface_light.b,
                vertex.color[3],
            ],
            uv: vertex.uv,
            normal: if vertex.normal.is_some() {
                normal.to_array()
            } else {
                [0.0; 3]
            },
            ..Vertex::UNLIT
        });
    }
}

/// World-space bounds of a local-space box placed by `transform`.
///
/// Only the eight corners are transformed: the result is the AABB of the
/// rotated box, which is conservative (never smaller than the real geometry),
/// which is exactly what a culling test needs.
pub fn transform_bounds(
    local: &crate::engine::spatial::Aabb,
    transform: &glam::Mat4,
) -> crate::engine::spatial::Aabb {
    let mut bounds = crate::engine::spatial::Aabb::EMPTY;
    for x in [local.min[0], local.max[0]] {
        for y in [local.min[1], local.max[1]] {
            for z in [local.min[2], local.max[2]] {
                let point = transform.transform_point3(glam::Vec3::new(x, y, z));
                bounds.expand([point.x, point.y, point.z]);
            }
        }
    }
    bounds
}

/// Instance transform for a placed prop: translate, yaw/pitch/roll and scale.
///
/// `base_y` is the world Y of the walkable floor at the prop's `(x, z)`; the
/// authored `prop.y` is an offset above it. This is exactly the transform the
/// placeholder boxes use (see [`add_prop_box`]), so a prop keeps its position,
/// orientation and vertical offset when its real model replaces the box. Model
/// space is metres with the origin at the floor-contact centre (see
/// `docs/assets/README.md`).
#[must_use]
pub fn prop_instance_matrix(prop: &PropDef, base_y: f32) -> glam::Mat4 {
    prop.placement_matrix(base_y)
}

/// Keeps completed fixture geometry in the shared model/material renderer.
/// Each instance retains its fixture ordinal so runtime emission follows the
/// same switch and animation as its prepared illumination contribution.
fn resolve_fixture_model_instances(
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    lighting: &LevelLighting,
    mut plan: Option<&mut crate::engine::lighting::lightmap::LightmapPlan>,
    defer_lighting: bool,
    budget: (usize, std::collections::HashSet<String>),
) -> Vec<PropMeshBatch> {
    let mut batches = Vec::new();
    let (mut busy_vertices, mut models_seen) = budget;
    for (index, fixture) in level.ceiling_lights.iter().enumerate() {
        let Some(model_id) = crate::engine::lighting::fixture_model_id(&fixture.fixture) else {
            continue;
        };
        let Some(model_path) = catalog.get(model_id).model else {
            continue;
        };
        if !models_seen.contains(&model_path)
            && models_seen.len() >= crate::engine::level::MAX_LEVEL_PROP_MODELS
        {
            report_fixture_budget(&model_path);
            continue;
        }
        let asset = match assets.resolve(&model_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&model_path, &error);
                continue;
            }
        };
        let lightmapped = plan.is_some();
        let Some(next_vertices) = busy_vertices
            .checked_add(instance_vertex_count(&asset.model, lightmapped))
            .filter(|count| *count <= crate::engine::level::MAX_LEVEL_PROP_VERTICES)
        else {
            report_fixture_budget(&model_path);
            continue;
        };
        let mut builder =
            BatchBuilder::new(&model_path, &asset.model, asset.model.textures.clone());
        if !builder.has_room_for(&asset.model, lightmapped) {
            continue;
        }
        // Ceiling families keep the established quarter-turn interpretation;
        // wall fixtures use their exact authored yaw, with local +Z in front.
        let rotation = if fixture.mount == crate::engine::level::LightMount::Wall {
            fixture.rotation_degrees
        } else if crate::engine::lighting::fixture_is_turned(fixture.rotation_degrees) {
            -90.0
        } else {
            0.0
        };
        let transform = glam::Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(rotation.to_radians()),
            glam::Vec3::new(fixture.x, lighting.fixture_y_for(fixture), fixture.z),
        );
        let Some((min, max)) = asset.model.bounds() else {
            continue;
        };
        let bounds = transform_bounds(&crate::engine::spatial::Aabb { min, max }, &transform);
        builder.bounds = bounds;
        if let Some(active_plan) = plan.as_deref_mut() {
            builder.push_lightmapped_instance(&transform, &asset, active_plan, &bounds);
        } else {
            builder.push_instance(&transform, &asset, lighting, defer_lighting);
        }
        let color = fixture.emitted_color();
        for (primitive, source) in builder.primitives.iter_mut().zip(&asset.model.submeshes) {
            primitive.emission = if source.material == 2 {
                MaterialEmission::new([color.r, color.g, color.b], fixture.emission_intensity())
            } else {
                MaterialEmission::NONE
            };
        }
        let mut batch = builder.finish();
        batch.light_control = u32::try_from(index).ok();
        batches.push(batch);
        busy_vertices = next_vertices;
        let _retained_model = models_seen.insert(model_path);
    }
    batches
}

fn report_fixture_budget(model_path: &str) {
    crate::engine::logging::warn_once(
        format!("fixture-model-budget:{model_path}"),
        format!(
            "[fixtures] model {model_path} exceeds the remaining level prop budget; drawing its procedural fallback"
        ),
    );
}
