//! Source-preserving coplanar model chart regression controls.

// Exact source attributes and bounded numerical fixtures are test contracts.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    clippy::indexing_slicing,
    reason = "Regression fixtures compare exact source attributes and bounded coordinate arithmetic; production guards remain enforced"
)]

use super::{BatchBuilder, Vertex, lightmap_prop_triangles, prop_quad_corners, shared_prop_charts};
use crate::lighting::lightmap::{LightmapConfig, LightmapPatch, LightmapPlan, PatchKind};

const fn config() -> LightmapConfig {
    LightmapConfig::for_profile(crate::quality::QualityProfile::Full)
}

const fn rectangle() -> [[f32; 3]; 4] {
    [
        [0.0, 0.0, 0.0],
        [0.66, 0.0, 0.0],
        [0.66, 0.59, 0.0],
        [0.0, 0.59, 0.0],
    ]
}

fn triangles(corners: [[f32; 3]; 4]) -> [[Vertex; 3]; 2] {
    let [p0, p1, p2, p3] = corners;
    let mut result = [[p0, p1, p2], [p0, p2, p3]].map(|triangle| {
        triangle.map(|pos| Vertex {
            pos,
            normal: [0.0, 0.0, 1.0],
            ..Vertex::UNLIT
        })
    });
    // Duplicated source positions intentionally have unrelated albedo UVs,
    // colours and tangent handedness. A shared illumination chart must not weld
    // or replace any of them, nor treat centroid tint as a material boundary.
    for (index, vertex) in result.iter_mut().flatten().enumerate() {
        let ordinal = f32::from(u16::try_from(index).unwrap_or(0));
        vertex.uv = [ordinal.mul_add(0.2, -0.4), ordinal.mul_add(-0.3, 0.7)];
        vertex.color = [ordinal * 0.1, 0.8, 0.3, ordinal.mul_add(-0.02, 1.0)];
        vertex.tangent = if index % 2 == 0 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        vertex.handedness = if index % 2 == 0 { 1.0 } else { -1.0 };
    }
    result
}

const fn without_chart(mut vertex: Vertex) -> Vertex {
    vertex.lightmap = Vertex::UNLIT.lightmap;
    vertex.lightmap_page = Vertex::UNLIT.lightmap_page;
    vertex
}

fn assert_source_attributes(source: &[[Vertex; 3]], result: &[[Vertex; 3]]) {
    assert_eq!(
        source.len(),
        result.len(),
        "source triangles remain independent"
    );
    for (original, actual) in source.iter().flatten().zip(result.iter().flatten()) {
        assert_eq!(
            *original,
            without_chart(*actual),
            "only atlas coordinates may change"
        );
    }
}

#[test]
fn shared_prop_chart_preserves_six_corners_and_source_diagonal() -> Result<(), String> {
    let source = triangles(rectangle());
    let mut plan = LightmapPlan::new(config());
    let result = lightmap_prop_triangles(source.to_vec(), &mut plan, 8.0);
    assert!(!plan.failed(), "a valid model quad retains a valid plan");
    assert_eq!(
        plan.chart_count(),
        1,
        "two physical triangles share one domain"
    );
    assert_source_attributes(&source, &result);
    let (patch, chart) = plan.charts().first().ok_or("shared chart")?;
    assert!(
        !patch.is_triangular(),
        "new model quad has explicit quad topology"
    );
    assert_eq!(
        patch.kind,
        PatchKind::Prop,
        "surface family remains model geometry"
    );
    assert_eq!(
        patch.room, None,
        "model quads never acquire architectural room ownership"
    );
    for (source_vertex, actual) in source.iter().flatten().zip(result.iter().flatten()) {
        let (u, v) = patch.local_of(source_vertex.pos);
        assert_eq!(
            actual.lightmap,
            chart.uv_at(config().page_edge, u, v),
            "exact source corner UV"
        );
    }
    assert_eq!(
        result[0][0].lightmap, result[1][0].lightmap,
        "common source diagonal start"
    );
    assert_eq!(
        result[0][2].lightmap, result[1][1].lightmap,
        "common source diagonal end"
    );
    assert_eq!(
        plan.sample_densities(),
        &[8.0],
        "the original physical density is unchanged"
    );
    Ok(())
}

#[test]
fn tapered_prop_quad_retains_exact_piecewise_triangle_mapping() -> Result<(), String> {
    let corners = [
        [0.0, 0.0, 0.0],
        [0.9, 0.0, 0.0],
        [0.65, 0.55, 0.0],
        [0.1, 0.55, 0.0],
    ];
    let source = triangles(corners);
    let actual = prop_quad_corners(&source[0], &source[1]).ok_or("native tapered quad")?;
    assert_eq!(
        actual, corners,
        "common source edge is the stored p0/p2 diagonal"
    );
    let patch =
        LightmapPatch::from_quad(PatchKind::Prop, actual, None).ok_or("native quad patch")?;
    for weights in [[0.2_f32, 0.3, 0.5], [0.7, 0.2, 0.1], [0.0, 0.5, 0.5]] {
        for (triangle, local) in source.iter().zip([
            [[0.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0]],
            [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ]) {
            let point: [f32; 3] = std::array::from_fn(|axis| {
                triangle
                    .iter()
                    .zip(weights)
                    .map(|(vertex, weight)| vertex.pos[axis] * weight)
                    .sum()
            });
            let uv: [f32; 2] = std::array::from_fn(|axis| {
                local
                    .iter()
                    .zip(weights)
                    .map(|(corner, weight)| corner[axis] * weight)
                    .sum()
            });
            assert!(
                glam::Vec3::from_array(patch.point_at(uv[0], uv[1]))
                    .distance(glam::Vec3::from_array(point))
                    < 1.0e-6,
                "bake follows GPU barycentric interpolation"
            );
            let back = patch.local_of(point);
            assert!(
                (back.0 - uv[0]).abs() < 1.0e-6 && (back.1 - uv[1]).abs() < 1.0e-6,
                "inverse mapping uses the same source diagonal"
            );
        }
    }
    Ok(())
}

#[test]
fn shared_prop_quad_supports_low_angle_nonuniform_and_mirrored_transforms() -> Result<(), String> {
    let corners = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.996_194_7, 0.087_155_74, 0.0],
        [0.996_194_7, 0.087_155_74, 0.0],
    ];
    let original = triangles(corners).map(|triangle| {
        triangle.map(|vertex| crate::gltf::PropVertex {
            pos: vertex.pos,
            normal: Some(vertex.normal),
            color: vertex.color,
            uv: vertex.uv,
        })
    });
    for scale in [
        glam::Vec3::new(0.3, 0.7, 2.0),
        glam::Vec3::new(-1.25, 0.35, 1.5),
    ] {
        let transform = glam::Mat4::from_scale_rotation_translation(
            scale,
            glam::Quat::from_euler(glam::EulerRot::XYZ, 0.4, 0.7, -0.2),
            glam::Vec3::new(3.0, 1.0, 2.0),
        );
        let normal_matrix = transform.inverse().transpose();
        for reverse_winding in [false, true] {
            let mut transformed = Vec::new();
            for mut triangle in original {
                if reverse_winding {
                    triangle.swap(1, 2);
                }
                transformed.push(
                    super::model_triangle_vertices(
                        [&triangle[0], &triangle[1], &triangle[2]],
                        &transform,
                        &normal_matrix,
                    )
                    .ok_or("transformed source triangle")?,
                );
            }
            let mut plan = LightmapPlan::new(config());
            let result = lightmap_prop_triangles(transformed.clone(), &mut plan, 8.0);
            assert_eq!(
                plan.chart_count(),
                1,
                "coplanar low-angle geometry remains supported after transform/winding"
            );
            assert_source_attributes(&transformed, &result);
            let (patch, _) = plan.charts().first().ok_or("transformed shared chart")?;
            for vertex in result.iter().flatten() {
                let (u, v) = patch.local_of(vertex.pos);
                assert!(
                    glam::Vec3::from_array(patch.point_at(u, v))
                        .distance(glam::Vec3::from_array(vertex.pos))
                        < 2.0e-5,
                    "chart still covers each actual transformed corner"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn shared_prop_quad_rejects_hard_normal_fold_and_inconsistent_winding() {
    let mut hard_normals = triangles(rectangle());
    for vertex in &mut hard_normals[1] {
        vertex.normal = [0.0, 0.1, 0.994_987_4];
    }
    let mut folded = triangles(rectangle());
    folded[1][2].pos[2] = 0.000_1;
    let mut reversed = triangles(rectangle());
    reversed[1].swap(1, 2);
    let mut overlap = triangles(rectangle());
    overlap[1][2].pos = [0.5, 0.1, 0.0];
    for source in [hard_normals, folded, reversed, overlap] {
        assert!(
            prop_quad_corners(&source[0], &source[1]).is_none(),
            "unsafe shared domains are rejected"
        );
        let mut plan = LightmapPlan::new(config());
        let result = lightmap_prop_triangles(source.to_vec(), &mut plan, 8.0);
        assert!(
            !plan.failed(),
            "rejected unions keep their valid original triangle domains"
        );
        assert_eq!(
            plan.chart_count(),
            2,
            "individual triangles remain receivers"
        );
        assert_source_attributes(&source, &result);
    }
}

#[test]
fn shared_prop_quad_rejects_nonmanifold_edge_and_ill_conditioned_domain() {
    let pair = triangles(rectangle());
    let third = [
        pair[0][0],
        pair[0][2],
        Vertex {
            pos: [-0.3, 0.3, 0.0],
            ..pair[1][2]
        },
    ];
    let source = [pair[0], pair[1], third];
    assert!(
        shared_prop_charts(&source).iter().all(Option::is_none),
        "three incident faces cannot share a manifold chart"
    );
    let tiny = triangles([
        [0.0, 0.0, 0.0],
        [0.000_01, 0.0, 0.0],
        [0.000_01, 0.5, 0.0],
        [0.0, 0.5, 0.0],
    ]);
    assert!(
        prop_quad_corners(&tiny[0], &tiny[1]).is_none(),
        "native minimum-axis guard remains effective"
    );
    let mut nonfinite_normal = pair;
    nonfinite_normal[1][0].normal = [f32::NAN, 0.0, 1.0];
    assert!(
        prop_quad_corners(&nonfinite_normal[0], &nonfinite_normal[1]).is_none(),
        "nonfinite shader seams cannot pair"
    );
}

fn asset(source: &[[Vertex; 3]; 2], split_primitives: bool) -> crate::props::LoadedPropAsset {
    let primitive = crate::gltf::PropSubmesh {
        double_sided: true,
        response: crate::materials::MaterialResponse::default(),
        material: 0,
        texture: None,
        emission: crate::materials::MaterialEmission::default(),
        alpha: crate::materials::MaterialAlpha::default(),
        first_index: 0,
        index_count: if split_primitives { 3 } else { 6 },
    };
    crate::props::LoadedPropAsset {
        model_path: "chart-regression.glb".to_string(),
        model: crate::gltf::PropModel {
            vertices: source
                .iter()
                .flatten()
                .map(|vertex| crate::gltf::PropVertex {
                    pos: vertex.pos,
                    normal: Some(vertex.normal),
                    color: vertex.color,
                    uv: vertex.uv,
                })
                .collect(),
            indices: vec![0, 1, 2, 3, 4, 5],
            submeshes: if split_primitives {
                vec![
                    primitive,
                    crate::gltf::PropSubmesh {
                        double_sided: true,
                        first_index: 3,
                        ..primitive
                    },
                ]
            } else {
                vec![primitive]
            },
            triangles: 2,
            materials: 1,
            ..crate::gltf::PropModel::default()
        },
    }
}

#[test]
fn prop_batch_keeps_primitive_material_and_instance_boundaries() {
    for split_primitives in [false, true] {
        let asset = asset(&triangles(rectangle()), split_primitives);
        let mut builder = BatchBuilder::new(&asset.model_path, &asset.model, Vec::new());
        let mut plan = LightmapPlan::new(config());
        let bounds = crate::spatial::Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        };
        for transform in [
            glam::Mat4::IDENTITY,
            glam::Mat4::from_translation(glam::Vec3::X),
        ] {
            builder.push_lightmapped_instance(&transform, &asset, &mut plan, &bounds);
        }
        let batch = builder.finish();
        assert_eq!(
            plan.chart_count(),
            if split_primitives { 4 } else { 2 },
            "sharing stays inside each original material primitive and placement"
        );
        assert_eq!(
            batch.vertices.len(),
            12,
            "all original six corners survive each placement"
        );
        assert_eq!(
            batch.indices.len(),
            12,
            "source triangle/index counts remain unchanged"
        );
        assert_eq!(
            batch.source_primitives,
            if split_primitives {
                vec![0, 1]
            } else {
                vec![0]
            },
            "primitive identity stays aligned with retained draw ranges"
        );
        assert_eq!(
            batch.submeshes.len(),
            if split_primitives { 2 } else { 1 },
            "draw material boundaries remain exact"
        );
    }
}

#[test]
fn prop_batch_source_primitive_identity_survives_dropped_first_primitive() {
    let mut source = triangles(rectangle());
    for vertex in &mut source[0] {
        vertex.pos = [0.0; 3];
    }
    let asset = asset(&source, true);
    let mut builder = BatchBuilder::new(&asset.model_path, &asset.model, Vec::new());
    let mut plan = LightmapPlan::new(config());
    builder.push_lightmapped_instance(
        &glam::Mat4::IDENTITY,
        &asset,
        &mut plan,
        &crate::spatial::Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        },
    );
    let batch = builder.finish();
    assert_eq!(
        batch.source_primitives,
        vec![1],
        "retained second primitive cannot alias source slot zero"
    );
    assert_eq!(
        batch.submeshes.len(),
        1,
        "empty draw primitive remains filtered"
    );
    assert_eq!(batch.indices, vec![0, 1, 2], "draw indices remain compact");
}

#[test]
fn prop_quad_stamp_rejects_wrong_diagonal_or_duplicate_half_without_allocation() {
    let corners = rectangle();
    let source = triangles(corners);
    for mut other in [
        source[0],
        [source[1][0], source[1][2], source[1][1]],
        [source[0][1], source[0][2], source[1][2]],
    ] {
        let mut first = source[0];
        let saved = other;
        let mut plan = LightmapPlan::new(config());
        assert!(
            !plan.stamp_prop_quad(&mut first, &mut other, corners, 8.0),
            "unsupported source topology does not acquire a quad chart"
        );
        assert_eq!(
            plan.chart_count(),
            0,
            "domain validation precedes allocation"
        );
        assert_eq!(
            plan.page_count(),
            0,
            "rejected domain spends no atlas capacity"
        );
        assert_eq!(first, source[0], "first triangle remains untouched");
        assert_eq!(other, saved, "second triangle remains untouched");
        assert!(
            !plan.failed(),
            "caller can keep original independent chart domains"
        );
    }
}

#[test]
fn prop_physical_density_and_dimensions_follow_full_and_medium_policy() -> Result<(), String> {
    let medium = crate::quality::LightmapQuality::Medium
        .lightmap_config()
        .ok_or("Medium atlas configuration")?;
    for (configuration, small, large, dimensions) in [
        (config(), 32.0_f32, 2.0_f32, (23_u32, 20_u32)),
        (medium, 24.0, 1.5, (17, 16)),
    ] {
        let mut plan = LightmapPlan::new(configuration);
        assert_eq!(
            plan.prop_texels_per_metre(false, false),
            small,
            "small opaque uses twice architectural resolution"
        );
        assert_eq!(
            plan.prop_texels_per_metre(true, false),
            large,
            "large opaque retains one eighth of architecture"
        );
        assert_eq!(
            plan.prop_texels_per_metre(false, true),
            1.0,
            "small cutouts retain their established density"
        );
        assert_eq!(
            plan.prop_texels_per_metre(true, true),
            1.0,
            "large cutouts retain their established density"
        );
        let source = triangles(rectangle());
        let result = lightmap_prop_triangles(source.to_vec(), &mut plan, small);
        assert_source_attributes(&source, &result);
        let (_, chart) = plan.charts().first().ok_or("finer physical face chart")?;
        assert_eq!(
            (chart.width, chart.height),
            dimensions,
            "inclusive samples use true 0.66 by 0.59 m face dimensions"
        );
        assert_eq!(
            plan.sample_densities(),
            &[small],
            "rounded atlas size never redefines world sampling pitch"
        );
        let mut historical = LightmapPlan::new(configuration);
        let retained = lightmap_prop_triangles(source.to_vec(), &mut historical, 8.0);
        assert_source_attributes(&source, &retained);
        assert_eq!(
            historical.sample_densities(),
            &[8.0],
            "explicit historical density remains a causal diagnostic control"
        );
    }
    Ok(())
}

#[test]
fn explicit_model_density_is_bounded_and_architecture_resolution_stays_unchanged()
-> Result<(), String> {
    let prop = LightmapPatch::from_quad(PatchKind::Prop, rectangle(), None).ok_or("model quad")?;
    let architecture = LightmapPatch::from_quad(PatchKind::Wall, rectangle(), Some(0))
        .ok_or("architecture quad")?;
    let mut allocator = crate::lighting::lightmap::ChartAllocator::new(config());
    let model_chart = allocator
        .allocate_at_density(&prop, 1_000.0)
        .ok_or("bounded model allocation")?;
    assert_eq!(
        (model_chart.width, model_chart.height),
        (23, 20),
        "unbounded requests cannot exceed twice architecture density"
    );
    let architecture_chart = allocator
        .allocate_at_density(&architecture, 1_000.0)
        .ok_or("bounded architecture allocation")?;
    assert_eq!(
        (architecture_chart.width, architecture_chart.height),
        (12, 11),
        "architecture requests retain the existing resolution ceiling"
    );
    let mut plan = LightmapPlan::new(config());
    let [mut first, mut second] = triangles(rectangle());
    assert!(
        plan.stamp_prop_quad(&mut first, &mut second, rectangle(), 1_000.0),
        "bounded explicit quad allocation remains supported"
    );
    assert_eq!(
        plan.sample_densities(),
        &[32.0],
        "transport metadata records actual bounded allocation density"
    );
    assert_eq!(
        config().max_pages,
        11,
        "Full uses the measured eleven-page plan budget"
    );
    assert_eq!(config().page_edge, 1_024, "page dimensions are unchanged");
    Ok(())
}

#[test]
fn invalid_explicit_model_density_fails_closed_without_spending_capacity() {
    for density in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
        let [mut first, mut second] = triangles(rectangle());
        let saved = [first, second];
        let mut plan = LightmapPlan::new(config());
        assert!(
            !plan.stamp_prop_quad(&mut first, &mut second, rectangle(), density),
            "invalid density cannot alias a real face to a single texel"
        );
        assert_eq!(
            plan.failure(),
            Some(crate::lighting::lightmap::LightmapFailure::InvalidConfig),
            "invalid request has a precise failure category"
        );
        assert_eq!(plan.page_count(), 0, "invalid request allocates no page");
        assert_eq!(
            [first, second],
            saved,
            "failed explicit density cannot mutate source vertices"
        );
    }
}
