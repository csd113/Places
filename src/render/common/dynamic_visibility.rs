//! Model-local ray resources for the existing movable scene. Geometry and PNG
//! alpha are prepared once per real mesh; movement updates inverse transforms
//! and a revision, preserving visibility for stationary receivers.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::dynamic::{DynamicId, DynamicMesh, DynamicScene};
use crate::lighting::transport::TransportScene;
use crate::spatial::Aabb;

struct CachedMesh {
    mesh: Arc<DynamicMesh>,
    scene: Arc<TransportScene>,
}

struct Caster {
    id: DynamicId,
    mesh: usize,
    matrix: [[u32; 4]; 4],
    inverse: Mat4,
    bounds: Aabb,
    scene: Arc<TransportScene>,
}

/// Measured preparation and resident geometry counters. Triangle count is not
/// a complete allocator/RSS estimate; the cached ray resource also has BVHs.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct DynamicVisibilityStats {
    pub casters: usize,
    pub meshes: usize,
    pub triangles: usize,
    pub model_builds: usize,
    pub preparation_millis: f64,
}

/// Covers every object admitted by the ordinary dynamic scene's existing
/// limits. There is no second caster count or name-dependent classification.
#[derive(Default)]
pub struct DynamicVisibility {
    meshes: HashMap<usize, CachedMesh>,
    casters: Vec<Caster>,
    revision: u64,
    model_builds: usize,
    preparation_millis: f64,
}

impl DynamicVisibility {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn stats(&self) -> DynamicVisibilityStats {
        DynamicVisibilityStats {
            casters: self.casters.len(),
            meshes: self.meshes.len(),
            triangles: self
                .meshes
                .values()
                .map(|mesh| mesh.scene.triangle_count())
                .sum(),
            model_builds: self.model_builds,
            preparation_millis: self.preparation_millis,
        }
    }

    /// Call after every object transform advanced and before receiver samples.
    /// A stationary scene performs no allocation, scene construction or update.
    pub fn sync(&mut self, dynamic: &DynamicScene) -> Result<bool, String> {
        let changed = dynamic.objects().len() != self.casters.len()
            || dynamic
                .objects()
                .iter()
                .zip(&self.casters)
                .any(|(object, previous)| {
                    object.id() != previous.id
                        || mesh_identity(object.mesh()) != previous.mesh
                        || matrix_bits(object.transform()) != previous.matrix
                });
        if !changed {
            return Ok(false);
        }
        for object in dynamic.objects() {
            let transform = object.transform();
            let bounds = object.world_bounds();
            if !transform.is_finite()
                || !transform.inverse().is_finite()
                || !bounds
                    .min
                    .iter()
                    .chain(&bounds.max)
                    .all(|value| value.is_finite())
            {
                return Err(format!(
                    "invalid movable visibility transform for '{}'",
                    object.mesh().model_path
                ));
            }
            let identity = mesh_identity(object.mesh());
            if self.meshes.contains_key(&identity) {
                continue;
            }
            let started = std::time::Instant::now();
            let scene = super::light_transport::dynamic_mesh_visibility_scene(object.mesh())
                .ok_or_else(|| {
                    format!(
                        "cannot prepare movable visibility for '{}'",
                        object.mesh().model_path
                    )
                })?;
            self.preparation_millis = started
                .elapsed()
                .as_secs_f64()
                .mul_add(1000.0_f64, self.preparation_millis);
            self.model_builds = self.model_builds.saturating_add(1);
            drop(self.meshes.insert(
                identity,
                CachedMesh {
                    mesh: Arc::clone(object.mesh()),
                    scene: Arc::new(scene),
                },
            ));
        }
        self.casters.clear();
        for object in dynamic.objects() {
            let transform = object.transform();
            let inverse = transform.inverse();
            let bounds = object.world_bounds();
            let identity = mesh_identity(object.mesh());
            let mesh = self
                .meshes
                .get(&identity)
                .ok_or("movable visibility cache entry is missing")?;
            self.casters.push(Caster {
                id: object.id(),
                mesh: identity,
                matrix: matrix_bits(transform),
                inverse,
                bounds,
                scene: Arc::clone(&mesh.scene),
            });
        }
        self.meshes.retain(|identity, cached| {
            dynamic.objects().iter().any(|object| {
                mesh_identity(object.mesh()) == *identity
                    && Arc::ptr_eq(object.mesh(), &cached.mesh)
            })
        });
        self.revision = self.revision.wrapping_add(1);
        Ok(true)
    }

    /// Visibility uses the drawn mesh/UV/alpha in its current world transform.
    /// Self exclusion is an object identity, independent of shared mesh names.
    #[must_use]
    pub fn transmittance(&self, from: [f32; 3], to: [f32; 3], excluded: Option<DynamicId>) -> f32 {
        if !from.iter().chain(&to).all(|value| value.is_finite()) {
            return 0.0;
        }
        let mut throughput = 1.0_f32;
        for caster in &self.casters {
            if excluded == Some(caster.id) || !segment_bounds_overlap(from, to, caster.bounds) {
                continue;
            }
            let local_from = caster
                .inverse
                .transform_point3(Vec3::from_array(from))
                .to_array();
            let local_to = caster
                .inverse
                .transform_point3(Vec3::from_array(to))
                .to_array();
            throughput *= caster.scene.transmittance(local_from, local_to);
            if throughput <= 0.0 {
                return 0.0;
            }
        }
        throughput
    }
}

fn mesh_identity(mesh: &Arc<DynamicMesh>) -> usize {
    std::ptr::from_ref(mesh.as_ref()).addr()
}

fn matrix_bits(matrix: Mat4) -> [[u32; 4]; 4] {
    matrix
        .to_cols_array_2d()
        .map(|column| column.map(f32::to_bits))
}

fn segment_bounds_overlap(from: [f32; 3], to: [f32; 3], bounds: Aabb) -> bool {
    from.into_iter()
        .zip(to)
        .zip(bounds.min.into_iter().zip(bounds.max))
        .all(|((a, b), (minimum, maximum))| a.min(b) <= maximum && a.max(b) >= minimum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::{PropModel, PropSubmesh, PropVertex};
    use crate::materials::{AlphaMode, MaterialAlpha};

    #[test]
    #[ignore = "explicit preserved hero package and asset root required; isolated CPU profiling"]
    fn profile_hero_moving_caster_refresh() -> Result<(), String> {
        let package = std::env::var("PLACES_PROFILE_PACKAGE").map_err(|error| error.to_string())?;
        let root = std::env::var("PLACES_PROFILE_ASSETS").map_err(|error| error.to_string())?;
        let output = std::env::var("PLACES_PROFILE_OUT").map_err(|error| error.to_string())?;
        let path = std::path::Path::new(&package);
        let opened = crate::package::world::open(path)?;
        let mut assets = crate::props::PropAssets::with_root(root);
        let variant = crate::package::world::load_variant(
            path,
            &opened.manifest,
            crate::quality::LightmapQuality::Full,
            &mut assets,
        )?;
        let materials = crate::render::logical_materials(&opened.level);
        let (transport, _) = super::super::light_transport::build_transport_scene(
            &opened.level,
            &variant.mesh,
            &variant.props,
            &materials,
            &variant.lighting,
            &[],
        )
        .ok_or("hero transport scene")?;
        let asset = assets.resolve("environment/home/props/models/dining_chair.glb")?;
        let mut dynamic = DynamicScene::new();
        let mut ids = Vec::new();
        for row in 0_u16..4 {
            for column in 0_u16..8 {
                ids.push(
                    dynamic
                        .spawn(
                            &asset,
                            [
                                f32::from(column).mul_add(0.5, 1.7),
                                0.0,
                                f32::from(row).mul_add(0.6, 1.1),
                            ],
                            0.0,
                            1.0,
                            0.0,
                        )
                        .ok_or("hero chair spawn")?,
                );
            }
        }
        let moving = *ids.first().ok_or("moving caster")?;
        let mut visibility = DynamicVisibility::new();
        let mut cases = Vec::new();
        for moving_case in [false, true] {
            let mut samples = Vec::new();
            for frame in 0_u16..240 {
                if moving_case {
                    let offset = if frame % 2 == 0 { 0.0125 } else { 0.0 };
                    assert!(dynamic.set_transform(moving, [1.7, 0.0, 1.1 + offset], 0.0, 1.0));
                }
                let started = std::time::Instant::now();
                let update = dynamic.update_with_visibility(
                    0.0,
                    Some(&variant.lighting),
                    variant.irradiance.as_deref(),
                    Some(&transport),
                    Some(&mut visibility),
                )?;
                let _timed_update = std::hint::black_box(update);
                if frame >= 40 {
                    samples.push(started.elapsed().as_secs_f64() * 1_000.0_f64);
                }
            }
            samples.sort_by(f64::total_cmp);
            cases.push(serde_json::json!({
                "moving": moving_case,
                "samples": samples,
                "visibility": visibility.stats(),
            }));
        }
        let report = serde_json::json!({
            "scope": "CPU DynamicScene::update_with_visibility only; no GPU, command dispatch, logging or frame pacing",
            "package": package,
            "cases": cases,
        });
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }

    fn panel(alpha: MaterialAlpha) -> Arc<crate::props::LoadedPropAsset> {
        Arc::new(crate::props::LoadedPropAsset {
            model_path: "movable-panel".to_string(),
            model: PropModel {
                vertices: [
                    [0.0, 0.0, -1.0],
                    [0.0, 2.0, -1.0],
                    [0.0, 2.0, 1.0],
                    [0.0, 0.0, 1.0],
                ]
                .map(|pos| PropVertex {
                    pos,
                    normal: Some([1.0, 0.0, 0.0]),
                    color: [1.0; 4],
                    uv: [0.5; 2],
                })
                .to_vec(),
                indices: vec![0, 1, 2, 0, 2, 3],
                submeshes: vec![PropSubmesh {
                    alpha,
                    response: crate::materials::MaterialResponse::NONE,
                    material: 0,
                    texture: None,
                    emission: crate::materials::MaterialEmission::NONE,
                    first_index: 0,
                    index_count: 6,
                }],
                triangles: 2,
                materials: 1,
                ..PropModel::default()
            },
        })
    }

    #[test]
    fn motion_changes_visibility_without_rebuilding_meshes_or_self_blocking() -> Result<(), String>
    {
        let mut dynamic = DynamicScene::new();
        let asset = panel(MaterialAlpha::OPAQUE);
        let id = dynamic
            .spawn(&asset, [0.0; 3], 0.0, 1.0, 90.0)
            .ok_or("panel spawn")?;
        let mut visibility = DynamicVisibility::new();
        assert!(visibility.sync(&dynamic)?);
        let before = visibility.revision();
        let from = [-1.0, 1.0, 0.5];
        let to = [1.0, 1.0, 0.5];
        assert_eq!(
            visibility.transmittance(from, to, None).to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            visibility.transmittance(from, to, Some(id)).to_bits(),
            1.0_f32.to_bits()
        );
        assert!(
            !visibility.sync(&dynamic)?,
            "stationary state stays resident"
        );
        assert_eq!(visibility.revision(), before);
        let update =
            dynamic.update_with_visibility(1.0, None, None, None, Some(&mut visibility))?;
        assert_eq!(update.moved, 1);
        assert_ne!(visibility.revision(), before);
        assert_eq!(
            visibility.transmittance(from, to, None).to_bits(),
            1.0_f32.to_bits(),
            "same frame sees the rotated panel"
        );
        assert_eq!(
            visibility.stats().model_builds,
            1,
            "motion reuses the mesh BVH"
        );
        Ok(())
    }

    #[test]
    fn alpha_geometry_and_shared_mesh_instances_preserve_throughput() -> Result<(), String> {
        for (alpha, expected) in [
            (
                MaterialAlpha {
                    mode: AlphaMode::Blend,
                    opacity: 0.5,
                    ..MaterialAlpha::OPAQUE
                },
                0.25_f32,
            ),
            (
                MaterialAlpha {
                    mode: AlphaMode::Cutout,
                    opacity: 0.0,
                    ..MaterialAlpha::OPAQUE
                },
                1.0_f32,
            ),
        ] {
            let asset = panel(alpha);
            let mut dynamic = DynamicScene::new();
            let _first = dynamic
                .spawn(&asset, [0.0; 3], 0.0, 1.0, 0.0)
                .ok_or("first panel")?;
            let _second = dynamic
                .spawn(&asset, [0.5, 0.0, 0.0], 0.0, 1.0, 0.0)
                .ok_or("second panel")?;
            let mut visibility = DynamicVisibility::new();
            let _changed = visibility.sync(&dynamic)?;
            assert_eq!(visibility.stats().casters, 2);
            assert_eq!(visibility.stats().meshes, 1);
            assert_eq!(visibility.stats().model_builds, 1);
            assert!(
                (visibility.transmittance([-1.0, 1.0, 0.5], [1.0, 1.0, 0.5], None) - expected)
                    .abs()
                    < 1.0e-6
            );
        }
        Ok(())
    }

    #[test]
    fn neighbor_motion_invalidates_stationary_receiver_support_without_ambient_rescue()
    -> Result<(), String> {
        let level = crate::level::LevelDef::from_json(r#"{"format_version":3,"id":"movable_support","name":"Movable support","spawn":{"x":0,"z":0},"rooms":[{"x":-3,"z":-3,"width":6,"depth":6,"height":4}]}"#).map_err(|error| error.to_string())?;
        let lighting = crate::lighting::LevelLighting::bake(&level);
        let field = crate::lighting::probes::ProbeField {
            local_direct: Some(crate::lighting::probes::ProbeDirectField {
                light_indices: Vec::new(),
                probes: vec![crate::lighting::lightmap::LightmapTexel::ZERO],
            }),
            min: [0.5, 0.5, -0.5],
            cell_m: 1.0,
            dims: [1; 3],
            probes: vec![crate::lighting::probes::ProbeSample {
                irradiance: [0.6; 3],
                direction: [0.0; 3],
                axis: [0.5; 2],
                room: 0,
            }],
        };
        let mut dynamic = DynamicScene::new();
        let id = dynamic
            .spawn(&panel(MaterialAlpha::OPAQUE), [0.0; 3], 0.0, 1.0, 0.0)
            .ok_or("panel")?;
        let mut visibility = DynamicVisibility::new();
        let _changed = visibility.sync(&dynamic)?;
        let bounds = Aabb {
            min: [-0.1; 3],
            max: [0.1; 3],
        };
        let model = Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.5));
        let closed_context = super::super::light_transport::EntityVisibility {
            dynamic: Some(&visibility),
            receiver: None,
        };
        let closed_key = super::super::light_transport::entity_spatial_key_with_visibility(
            &lighting,
            Some(&field),
            model,
            None,
            closed_context,
        );
        let closed = super::super::light_transport::entity_spatial_lighting_with_visibility(
            &lighting,
            Some(&field),
            bounds,
            model,
            None,
            closed_context,
        )
        .ok_or("closed support")?;
        assert!(
            closed.anchors.iter().all(|anchor| anchor
                .irradiance
                .iter()
                .take(3)
                .all(|value| value.to_bits() == 0.0_f32.to_bits())),
            "current opaque visibility cannot recover stale fullbright lighting"
        );
        assert!(dynamic.set_transform(id, [2.5, 0.0, 0.0], 0.0, 1.0));
        let _moved = visibility.sync(&dynamic)?;
        let open_context = super::super::light_transport::EntityVisibility {
            dynamic: Some(&visibility),
            receiver: None,
        };
        let open_key = super::super::light_transport::entity_spatial_key_with_visibility(
            &lighting,
            Some(&field),
            model,
            None,
            open_context,
        );
        assert_ne!(closed_key, open_key);
        let open = super::super::light_transport::entity_spatial_lighting_with_visibility(
            &lighting,
            Some(&field),
            bounds,
            model,
            None,
            open_context,
        )
        .ok_or("open support")?;
        assert!(open.anchors.iter().all(|anchor| {
            anchor
                .irradiance
                .iter()
                .take(3)
                .all(|value| (*value - 0.6).abs() < 1.0e-6)
        }));
        assert_eq!(visibility.stats().model_builds, 1);
        Ok(())
    }
}
