//! Conservative runtime factoring of the existing, validated prop records.
//!
//! Admission verifies every shape attribute and ordered index against an authored
//! placement of the dependency-verified source. No tolerance or model-name-only
//! cache key can admit changed geometry. Lighting is never used as a shape key.

use std::collections::HashMap;
use std::sync::Arc;

use super::props::{PropMeshBatch, prop_instance_matrix};
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::props::{LoadedPropAsset, PropAssets};
use crate::engine::spatial::Aabb;

#[derive(Clone, Debug)]
pub struct CanonicalPlacement {
    pub model: glam::Mat4,
    pub normal: glam::Mat4,
    pub tint: [f32; 3],
    pub bounds: Aabb,
}

impl CanonicalPlacement {
    pub fn vertex(&self, source: &crate::engine::gltf::PropVertex) -> super::Vertex {
        let normal = source.normal.map_or([0.0; 3], |value| {
            self.normal
                .transform_vector3(glam::Vec3::from_array(value))
                .normalize_or_zero()
                .to_array()
        });
        super::Vertex {
            pos: self
                .model
                .transform_point3(glam::Vec3::from_array(source.pos))
                .to_array(),
            normal,
            uv: source.uv,
            caustics_albedo: [
                source.color[0] * self.tint[0],
                source.color[1] * self.tint[1],
                source.color[2] * self.tint[2],
            ],
            ..super::Vertex::UNLIT
        }
    }
}

/// Existing package records remain authoritative. Unsupported topology, unknown
/// source inputs and independent poses retain the established palette path.
/// # Errors
/// Propagates resource cancellation/pressure; failure never installs half a batch.
pub fn share_canonical_props(
    batches: &mut [PropMeshBatch],
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut PropAssets,
    lighting: &crate::engine::lighting::LevelLighting,
) -> Result<(), String> {
    if std::env::var("PLACES_BENCH_CANONICAL_PROPS").as_deref() == Ok("0") {
        return Ok(());
    }
    let surfaces = LevelSurfaces::new(level);
    let mut placements: HashMap<String, Vec<CanonicalPlacement>> = HashMap::new();
    for prop in &level.props {
        if prop.shadow_only || prop.float.is_some() || prop.is_physics_movable() {
            continue;
        }
        let Some(path) = catalog.get(&prop.model).model else {
            continue;
        };
        let Ok(asset) = assets.resolve(&path) else {
            continue;
        };
        let model = prop_instance_matrix(prop, surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0));
        if !model.is_finite() || model.determinant().abs() <= 1.0e-12 {
            continue;
        }
        let Some((min, max)) = asset.model.bounds() else {
            continue;
        };
        placements
            .entry(path)
            .or_default()
            .push(CanonicalPlacement {
                model,
                normal: model.inverse().transpose(),
                tint: catalog.tint(&prop.model).unwrap_or([1.0; 3]),
                bounds: guarded_bounds(&Aabb { min, max }, &model),
            });
    }
    add_fixture_placements(&mut placements, level, catalog, assets, lighting);
    for batch in batches.iter_mut() {
        crate::engine::resources::checkpoint()?;
        if batch.vertices.shared().is_some()
            || !batch.casts_static_lighting
            || batch.pending_probe_lighting
            || batch
                .submeshes
                .iter()
                .any(|s| s.alpha.mode == crate::engine::materials::AlphaMode::Blend)
        {
            continue;
        }
        // Material variants use the same shape, but their neutral albedo must
        // pass the same exact check. Unknown variant factors stay independent.
        let path = batch.model.split('#').next().unwrap_or(&batch.model);
        let Some(candidates) = placements.get(path) else {
            continue;
        };
        let Ok(asset) = assets.resolve(path) else {
            continue;
        };
        let Some(matched) = match_batch(batch, &asset, candidates) else {
            continue;
        };
        batch.vertices.share(asset, matched)?;
    }
    trace_shared_storage(batches);
    Ok(())
}

fn add_fixture_placements(
    placements: &mut HashMap<String, Vec<CanonicalPlacement>>,
    level: &LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut PropAssets,
    lighting: &crate::engine::lighting::LevelLighting,
) {
    for fixture in &level.ceiling_lights {
        let Some(id) = crate::engine::lighting::fixture_model_id(&fixture.fixture) else {
            continue;
        };
        let Some(path) = catalog.get(id).model else {
            continue;
        };
        let Ok(asset) = assets.resolve(&path) else {
            continue;
        };
        let Some((min, max)) = asset.model.bounds() else {
            continue;
        };
        let rotation = if fixture.mount == crate::engine::level::LightMount::Wall {
            fixture.rotation_degrees
        } else if crate::engine::lighting::fixture_is_turned(fixture.rotation_degrees) {
            -90.0
        } else {
            0.0
        };
        let model = glam::Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(rotation.to_radians()),
            glam::Vec3::new(
                fixture.x,
                lighting.fixture_y_for(fixture)
                    + crate::engine::lighting::fixture_profile(&fixture.fixture).model_offset_y,
                fixture.z,
            ),
        );
        placements
            .entry(path)
            .or_default()
            .push(CanonicalPlacement {
                model,
                normal: model.inverse().transpose(),
                tint: [1.0; 3],
                bounds: guarded_bounds(&Aabb { min, max }, &model),
            });
    }
}

fn trace_shared_storage(batches: &[PropMeshBatch]) {
    let mut models = std::collections::HashSet::new();
    let mut vertices = 0_usize;
    let mut instances = 0_usize;
    let mut bytes = 0_usize;
    for batch in batches {
        bytes = bytes.saturating_add(batch.vertices.retained_bytes());
        if let Some(shared) = batch.vertices.shared() {
            let _new = models.insert(Arc::as_ptr(&shared.asset));
            instances = instances.saturating_add(shared.placements.len());
            vertices = vertices.saturating_add(batch.vertices.len());
        }
    }
    crate::engine::logging::info(format_args!(
        "[canonical-props] meshes={} instances={instances} placed_vertices={vertices} retained_vertex_bytes={bytes}",
        models.len()
    ));
}

fn guarded_bounds(local: &Aabb, model: &glam::Mat4) -> Aabb {
    let mut bounds = super::props::transform_bounds(local, model);
    for (min, max) in bounds.min.iter_mut().zip(bounds.max.iter_mut()) {
        let margin = min.abs().max(max.abs()).mul_add(8.0 * f32::EPSILON, 1.0e-4);
        *min -= margin;
        *max += margin;
    }
    bounds
}

fn same_shape(a: super::Vertex, b: super::Vertex) -> bool {
    a.pos.map(f32::to_bits) == b.pos.map(f32::to_bits)
        && a.normal.map(f32::to_bits) == b.normal.map(f32::to_bits)
        && a.uv.map(f32::to_bits) == b.uv.map(f32::to_bits)
        && a.tangent.map(f32::to_bits) == b.tangent.map(f32::to_bits)
        && a.handedness.to_bits() == b.handedness.to_bits()
        && a.caustics_albedo.map(f32::to_bits) == b.caustics_albedo.map(f32::to_bits)
        && a.lightmap == b.lightmap
        && a.lightmap_page == b.lightmap_page
}

fn match_batch(
    batch: &PropMeshBatch,
    asset: &Arc<LoadedPropAsset>,
    candidates: &[CanonicalPlacement],
) -> Option<Vec<CanonicalPlacement>> {
    let source = &asset.model;
    let count = source.vertices.len();
    if count == 0
        || source.is_animatable()
        || source.vertices.iter().any(|v| v.normal.is_none())
        || !batch.vertices.len().is_multiple_of(count)
        || batch.submeshes.len() != source.submeshes.len()
    {
        return None;
    }
    let instances = batch.vertices.len().checked_div(count)?;
    if instances == 0 || !topology_matches(batch, source, instances) {
        return None;
    }
    let mut result = Vec::with_capacity(instances);
    for instance in 0..instances {
        let offset = instance.checked_mul(count)?;
        let first = batch.vertices.get(offset)?;
        let matched = candidates.iter().find(|candidate| {
            source
                .vertices
                .first()
                .is_some_and(|v| same_shape(first, candidate.vertex(v)))
                && source.vertices.iter().enumerate().all(|(index, vertex)| {
                    batch
                        .vertices
                        .get(offset.saturating_add(index))
                        .is_some_and(|actual| same_shape(actual, candidate.vertex(vertex)))
                })
        })?;
        result.push(matched.clone());
    }
    Some(result)
}

fn topology_matches(
    batch: &PropMeshBatch,
    source: &crate::engine::gltf::PropModel,
    instances: usize,
) -> bool {
    if source.indices.len().checked_mul(instances) != Some(batch.indices.len()) {
        return false;
    }
    batch
        .submeshes
        .iter()
        .zip(&source.submeshes)
        .all(|(actual, original)| {
            let Ok(start) = usize::try_from(actual.first_index) else {
                return false;
            };
            let Ok(source_start) = usize::try_from(original.first_index) else {
                return false;
            };
            let Ok(count) = usize::try_from(original.index_count) else {
                return false;
            };
            let Some(expected_count) = count.checked_mul(instances) else {
                return false;
            };
            if usize::try_from(actual.index_count).ok() != Some(expected_count) {
                return false;
            }
            let Some(indices) = source
                .indices
                .get(source_start..source_start.saturating_add(count))
            else {
                return false;
            };
            (0..instances).all(|instance| {
                let offset = start.saturating_add(instance.saturating_mul(count));
                let Some(run) = batch.indices.get(offset..offset.saturating_add(count)) else {
                    return false;
                };
                let base = instance.saturating_mul(source.vertices.len());
                run.iter().zip(indices).all(|(actual_index, source_index)| {
                    usize::from(*actual_index) == base.saturating_add(usize::from(*source_index))
                })
            })
        })
}

#[cfg(test)]
#[path = "../../tests/canonical_props.rs"]
mod tests;
