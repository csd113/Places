//! Cheap native-seam regression using one primitive body and logical materials.

use glam::Vec3;

use super::{BodyMotion, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext, ShapeDef};
use crate::level::LevelDef;
use crate::materials::{MaterialTable, PackMaterials};

// Protects the actual wood→carpet rebound seen in the native fixture: material
// boundaries must change traction without adding a wall or launching the item.
// One 24 cm cuboid, 390 fixed steps, three metadata-only regions; no renderer,
// asset decoding, baked map, random seed or expanded physics framework.
fn seam_world() -> Result<ItemWorld, String> {
    let level: LevelDef = serde_json::from_str(
        r#"{
        "format_version":3,"id":"material_seam","name":"material seam",
        "spawn":{"x":0,"z":1},
        "defaults":{"floor":"pack:wood","ceiling":"core:ceiling_tile","wall":"core:wallpaper"},
        "rooms":[{"x":-1,"z":0,"width":2,"depth":8,"height":4,"ceiling":{"kind":"open"}}],
        "floor_patches":[
            {"x":-1,"z":0,"width":2,"depth":2.5,"material":"pack:carpet"},
            {"x":-1,"z":5.5,"width":2,"depth":2.5,"material":"pack:ice"}]
        }"#,
    )
    .map_err(|error| error.to_string())?;
    let pack = PackMaterials::new(
        "seam_coefficients",
        Some(
            r#"{
            "pack:wood":{"texture":"unused.png","ground_friction":0.6},
            "pack:carpet":{"texture":"unused.png","ground_friction":1.0},
            "pack:ice":{"texture":"unused.png","ground_friction":0.025}}
            "#,
        ),
        std::collections::HashMap::new(),
    );
    let materials =
        MaterialTable::logical(&level, &crate::assets::AssetCatalog::builtin(), Some(&pack));
    let mut world = ItemWorld::from_level_with_materials(&level, &[], &materials);
    let definition = PhysicsDef {
        motion: BodyMotion::Dynamic,
        shape: Some(ShapeDef::Cuboid {
            half_extents: [0.3, 0.3, 0.3],
        }),
        mass_kg: 0.3,
        friction: 0.6,
        restitution: 0.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
        ..PhysicsDef::default()
    };
    world.spawn(
        1,
        &definition,
        (Vec3::splat(-0.3), Vec3::splat(0.3)),
        Vec3::new(0.0, 0.1205, 6.0),
        0.0,
        0.4,
    )?;
    Ok(world)
}

#[test]
fn coplanar_material_seams_slide_continuously_and_change_traction() -> Result<(), String> {
    let mut world = seam_world()?;
    let player = PlayerContext {
        eye: Vec3::new(3.0, 1.6, 3.0),
        feet: Vec3::new(3.0, 0.0, 3.0),
        height: 1.8,
        view: -Vec3::Z,
        yaw: 0.0,
        active: false,
    };
    for _ in 0..30_u32 {
        world.step(FIXED_STEP, player, &[]);
    }
    let mut previous = world.snapshots().into_iter().next().ok_or("missing cube")?;
    let resting_height = previous.pose.position.y;
    assert!(world.set_fixture_velocity(1, Vec3::new(0.0, 0.0, -6.0), Vec3::ZERO));
    previous.linear_velocity.z = -6.0;
    let mut wood_deceleration = (0.0_f32, 0.0_f32);
    let mut carpet_deceleration = (0.0_f32, 0.0_f32);
    for _ in 0..360_u32 {
        world.step(FIXED_STEP, player, &[]);
        let body = world.snapshots().into_iter().next().ok_or("missing cube")?;
        assert!(
            body.linear_velocity.z <= 0.02
                && body.pose.position.z <= previous.pose.position.z + 0.0002,
            "a material seam must not rebound the sliding cube: {body:?}"
        );
        assert!(
            body.pose.position.y <= resting_height + 0.003
                && body.pose.position.y >= 0.118
                && body.pose.rotation.mul_vec3(Vec3::Y).dot(Vec3::Y) > 0.999,
            "coplanar seams must not launch, penetrate or tip the cube: {body:?}"
        );
        if body.linear_velocity.z < -0.5 {
            let deceleration = (body.linear_velocity.z - previous.linear_velocity.z) / FIXED_STEP;
            let sample = if (2.64..=5.36).contains(&body.pose.position.z) {
                Some(&mut wood_deceleration)
            } else if body.pose.position.z < 2.36 {
                Some(&mut carpet_deceleration)
            } else {
                None
            };
            if let Some((sum, count)) = sample {
                *sum += deceleration;
                *count += 1.0;
            }
        }
        previous = body;
    }
    assert!(
        wood_deceleration.1 > 10.0 && carpet_deceleration.1 > 10.0,
        "the cube must actually traverse both fully supported material regions"
    );
    let wood = wood_deceleration.0 / wood_deceleration.1;
    let carpet = carpet_deceleration.0 / carpet_deceleration.1;
    assert!(
        wood > 2.0 && carpet > wood * 1.3,
        "actual floor contacts must increase traction on carpet: wood={wood}, carpet={carpet}"
    );
    assert!(
        previous.pose.position.z < 2.0
            && previous.linear_velocity.length() < 0.02
            && previous.sleeping,
        "the body crosses the former ghost edge and settles naturally on carpet: {previous:?}"
    );
    Ok(())
}
