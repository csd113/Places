//! Small metadata and geometry checks; no renderer or map bake.
#![allow(
    clippy::expect_used,
    reason = "Invalid deterministic fixture setup is a test failure"
)]

use glam::{Quat, Vec3};
use rapier3d::parry::query::Ray;
use rapier3d::prelude::{
    ColliderBuilder, PhysicsWorld, Pose, RigidBodyBuilder, RigidBodyType, SharedShape, Vector,
};

use super::collision::{add, mass_properties, model_shape, point, pose, vector};
use super::config::MAX_HULL_POINTS;
use super::{PhysicsDef, ShapeDef};

fn shipped_profile(id: &str) -> PhysicsDef {
    let profiles: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/gameplay.json"))
            .expect("small shipped physics metadata parses");
    let profile = profiles
        .get("assets")
        .and_then(|assets| assets.get(id))
        .and_then(|asset| asset.get("physics"))
        .cloned()
        .expect("shipped collision profile exists");
    serde_json::from_value(profile).expect("shipped collision profile parses")
}

fn hull_plane_excess(shape: &SharedShape, model_frame: &Pose, vertex: Vector) -> f32 {
    // PointQuery uses approximate GJK and can project an exact input vertex
    // outside a small hull. Inspect the built hull's real planes instead.
    let compound = shape.as_compound().expect("fitted compound hulls");
    compound
        .shapes()
        .iter()
        .map(|(child_pose, child)| {
            let local =
                child_pose.inverse_transform_point(model_frame.inverse_transform_point(vertex));
            let hull = child.as_convex_polyhedron().expect("convex hull primitive");
            hull.faces()
                .iter()
                .map(|face| {
                    let first =
                        usize::try_from(face.first_vertex_or_edge).expect("face index fits");
                    let point_index = hull
                        .vertices_adj_to_face()
                        .get(first)
                        .expect("face vertex index");
                    let index = usize::try_from(*point_index).expect("hull point index fits");
                    let anchor = hull.points().get(index).expect("hull plane anchor");
                    let relative =
                        Vector::new(local.x - anchor.x, local.y - anchor.y, local.z - anchor.z);
                    face.normal.dot(relative)
                })
                .fold(f32::NEG_INFINITY, f32::max)
        })
        .fold(f32::INFINITY, f32::min)
}

fn assert_millimetre_hull_mass_frame() {
    let points = [-0.001, 0.001]
        .into_iter()
        .flat_map(|x| {
            [-0.002, 0.002]
                .into_iter()
                .flat_map(move |y| [-0.003, 0.003].into_iter().map(move |z| [x, y, z]))
        })
        .collect();
    let definition = PhysicsDef {
        shape: Some(ShapeDef::ConvexHull { points }),
        ..PhysicsDef::default()
    };
    let (shape, _, _) = model_shape(
        &definition,
        (
            Vec3::new(-0.001, -0.002, -0.003),
            Vec3::new(0.001, 0.002, 0.003),
        ),
        1.0,
    )
    .expect("small asymmetric hull");
    let properties = mass_properties(&shape, 0.001).expect("small hull inertia normalizes");
    let matrix = properties.reconstruct_inertia_matrix();
    let actual = [matrix.x_axis.x, matrix.y_axis.y, matrix.z_axis.z];
    let expected = [
        0.001 * 0.000_013 / 3.0,
        0.001 * 0.000_010 / 3.0,
        0.001 * 0.000_005 / 3.0,
    ];
    for (component, reference) in actual.into_iter().zip(expected) {
        assert!(
            (component - reference).abs() < 2.0e-12,
            "millimetre hull inertia must retain its correct local axis: actual={actual:?}"
        );
    }
    assert!(
        mass_properties(&shape, f32::NAN).is_err(),
        "invalid masses cannot reach the solver"
    );
}

// Covers bounded deserialization and finite/noncoplanar geometry validation.
// Four tetrahedron vertices plus rejected inputs avoid any solver or map setup.
#[test]
fn convex_hull_inputs_are_bounded_scaled_and_three_dimensional() {
    let points = vec![
        [-0.01, -0.01, -0.01],
        [0.01, -0.01, -0.01],
        [-0.01, 0.01, -0.01],
        [-0.01, -0.01, 0.01],
    ];
    let valid = ShapeDef::ConvexHull {
        points: points.clone(),
    };
    assert!(
        valid.validate_scale(0.5).is_ok(),
        "a scaled solid tetrahedron is accepted"
    );
    assert!(
        valid.validate_scale(0.001).is_err(),
        "scaled thickness stays bounded"
    );
    let round_trip: ShapeDef =
        serde_json::from_str(&serde_json::to_string(&valid).expect("hull serializes"))
            .expect("hull parses");
    assert_eq!(round_trip, valid, "convex hull vertices round trip exactly");
    for count in [3, MAX_HULL_POINTS + 1] {
        let invalid = ShapeDef::ConvexHull {
            points: vec![*points.first().expect("tetrahedron vertex"); count],
        };
        assert!(
            invalid.validate_scale(1.0).is_err(),
            "direct APIs enforce hull point counts"
        );
        assert!(
            serde_json::from_str::<ShapeDef>(
                &serde_json::to_string(&invalid).expect("invalid count serializes")
            )
            .is_err(),
            "serde caps allocation before accepting invalid point counts"
        );
    }
    let planar = ShapeDef::ConvexHull {
        points: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 2.0],
        ],
    };
    assert!(
        planar.validate_scale(1.0).is_err(),
        "a plane is not a solid hull"
    );
    for coordinate in [f32::NAN, f32::INFINITY, 101.0] {
        let mut malformed = points.clone();
        malformed.first_mut().expect("tetrahedron vertex")[0] = coordinate;
        assert!(
            ShapeDef::ConvexHull { points: malformed }
                .validate_scale(1.0)
                .is_err(),
            "non-finite or out-of-bounds hull coordinates are rejected"
        );
    }
    assert!(
        serde_json::from_str::<ShapeDef>(
            r#"{"kind":"convex_hull","points":[[0,0,0],[1,0,0],[0,1,0],[0,0,1]],"extra":0}"#
        )
        .is_err(),
        "hull metadata rejects unknown fields"
    );
    assert_millimetre_hull_mass_frame();
}

// Protects the shipped pin's real silhouette and mass distribution, including
// its shoulder and narrow red sleeves. Five small hulls and two unsimulated
// bodies uniquely cover the previous underfit and overlapping-volume errors.
// The one 984-vertex GLB and its 128-square embedded sheet are a small fixture.
#[test]
fn pin_hulls_cover_the_mesh_and_keep_authored_mass_through_activation() {
    let definition = shipped_profile("arcade:bowling_pin");
    let model = crate::gltf::parse_glb(include_bytes!(
        "../../assets/environments/arcade/props/bowling_pin.glb"
    ))
    .expect("existing pin geometry parses");
    let (minimum, maximum) = model.bounds().expect("pin geometry bounds");
    let (shape, centre, _) = model_shape(
        &definition,
        (Vec3::from_array(minimum), Vec3::from_array(maximum)),
        1.0,
    )
    .expect("pin hulls build");
    let model_frame = pose(centre, Quat::IDENTITY);
    for vertex in &model.vertices {
        let excess = hull_plane_excess(&shape, &model_frame, Vector::from_array(vertex.pos));
        assert!(
            excess < 2.0e-6,
            "real GLB pin vertex {:?} exceeds its collider's plane by {excess} m",
            vertex.pos
        );
    }
    let normalized_properties = mass_properties(&shape, definition.mass_kg)
        .expect("pin mass properties normalize without changing its authored mass");
    let mut world = PhysicsWorld::default();
    let (body_handle, _) = world.insert(
        RigidBodyBuilder::dynamic(),
        ColliderBuilder::new(shape).mass_properties(normalized_properties),
    );
    let body = world.bodies.get_mut(body_handle).expect("pin body");
    body.set_body_type(RigidBodyType::Fixed, true);
    body.set_body_type(RigidBodyType::Dynamic, true);
    let properties = body.mass_properties();
    assert!(
        (body.mass() - 1.5).abs() < 1.0e-5,
        "solver mass equals authored mass"
    );
    let origin_com = add(centre, point(body.local_center_of_mass()));
    assert!(
        (origin_com.y - 0.1522).abs() < 0.001,
        "mesh-fitted mass centre stays in the belly"
    );
    let inertia = properties.local_mprops.principal_inertia();
    let inertia_per_kg = Vector::new(
        inertia.x / body.mass(),
        inertia.y / body.mass(),
        inertia.z / body.mass(),
    );
    assert!(
        (inertia_per_kg.min_element() - 0.00117).abs() < 0.00006,
        "yaw inertia per kg must match the faceted pin volume; actual={inertia_per_kg:?}, mass={}, COM={origin_com:?}, principal frame={:?}",
        body.mass(),
        properties.local_mprops.principal_inertia_local_frame
    );
    assert!(
        (inertia_per_kg.max_element() - 0.01004).abs() < 0.00015,
        "pitch and roll inertia must match the pin silhouette; actual={inertia_per_kg:?}, mass={}, COM={origin_com:?}",
        body.mass()
    );
    let (ball_handle, _) = world.insert(
        RigidBodyBuilder::dynamic(),
        ColliderBuilder::ball(0.108).mass(5.0),
    );
    assert!(
        (world.bodies.get(ball_handle).expect("ball body").mass() / 1.5 - 10.0 / 3.0).abs()
            < 1.0e-5,
        "the solver receives distinct ball and pin masses"
    );
}

// Protects the missing six-centimetre gutter support without map-specific
// runtime geometry. Eight bounded metadata hulls and 13 rays inspect its bowl.
// Endpoints on the actual facet polyline bound vertical support excess to
// 4.5023 mm; the centre bottom is exact and no visible stock is undercovered.
#[test]
fn fixed_gutter_hulls_follow_the_existing_open_bowl() {
    let definition = shipped_profile("arcade:gutter_segment");
    let (shape, centre, _) = model_shape(
        &definition,
        (Vec3::new(-0.135, 0.0, -1.5), Vec3::new(0.135, 0.135, 1.5)),
        1.0,
    )
    .expect("gutter hulls build");
    let model_frame = pose(centre, Quat::IDENTITY);
    for facet in 0_u16..=12 {
        let angle = std::f32::consts::PI * f32::from(facet) / 12.0;
        let x = 0.12 * angle.cos();
        let mesh_y = (-0.12_f32).mul_add(angle.sin(), 0.135);
        let ray = Ray::new(Vector::new(x, 0.3, 0.0), Vector::new(0.0, -1.0, 0.0));
        let time = shape
            .cast_ray(&model_frame, &ray, 1.0, true)
            .expect("curved bowl support");
        let supported_y = 0.3 - time;
        assert!(
            supported_y + 2.0e-6 >= mesh_y,
            "plastic support cannot fall below its visible bowl"
        );
        assert!(
            supported_y - mesh_y < 0.005,
            "gutter facet {facet} at x {x}: collision Y {supported_y}, mesh Y {mesh_y}"
        );
    }
    let centre_ray = Ray::new(Vector::new(0.0, 0.3, 0.0), Vector::new(0.0, -1.0, 0.0));
    let time = shape
        .cast_ray(&model_frame, &centre_ray, 1.0, true)
        .expect("gutter bottom");
    assert!(
        (0.3 - time - 0.015).abs() < 2.0e-6,
        "the open bottom matches the mesh, not the floor beneath it"
    );
    let gap_ray = Ray::new(vector(Vec3::new(0.0, 0.14, 0.0)), vector(Vec3::Z));
    assert!(
        shape.cast_ray(&model_frame, &gap_ray, 1.0, true).is_none(),
        "a fixed compound leaves the channel open"
    );
}
