//! Small solver fixtures: no textures, renderer, map compiler or shipped-map bake.
#![allow(
    clippy::expect_used,
    reason = "Invalid deterministic fixture setup is a test failure"
)]

use glam::{Quat, Vec3};

use super::{
    BodyMotion, CompoundPart, ItemWorld, MAX_BODIES, PhysicsDef, PlayerContext, PrimitiveShapeDef,
    ShapeDef,
};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::level::LevelDef;
use crate::engine::materials::{MaterialTable, PackMaterials};

fn level(extra: &str) -> LevelDef {
    serde_json::from_str(&format!(r#"{{"format_version":3,"id":"physics_unit_fixture","name":"physics unit fixture",
        "spawn":{{"x":0,"z":0}},"rooms":[{{"x":-3,"z":-3,"width":6,"depth":6,"height":4}}],
        "defaults":{{"floor":"core:carpet","ceiling":"core:ceiling_tile","wall":"core:wallpaper"}}{extra}}}"#))
        .expect("tiny level fixture parses")
}

fn player(eye: Vec3) -> PlayerContext {
    PlayerContext {
        eye,
        feet: Vec3::new(eye.x, eye.y - 1.6, eye.z),
        height: 1.8,
        view: -Vec3::Z,
        yaw: 0.0,
        active: true,
    }
}

fn dynamic(shape: Option<ShapeDef>) -> PhysicsDef {
    PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        shape,
        ..PhysicsDef::default()
    }
}

fn spawn_cube(world: &mut ItemWorld, key: u64, position: Vec3) {
    world
        .spawn(
            key,
            &dynamic(None),
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            position,
            0.0,
            1.0,
        )
        .expect("valid small cube spawns");
}

fn advance(world: &mut ItemWorld, context: PlayerContext, frames: u32) {
    for _ in 0..frames {
        world.step(1.0 / 60.0, context, &[]);
    }
}

fn tip_spawned_body(world: &mut ItemWorld, key: u64) {
    let snapshot = world
        .snapshots()
        .into_iter()
        .find(|body| body.key == key)
        .expect("spawned item");
    let tipped = Quat::from_rotation_z(1.0);
    assert!(
        world.set_fixture_pose(
            key,
            super::ItemPose {
                position: snapshot.pose.position,
                rotation: tipped
            }
        ),
        "explicit developer fixture pose tips a dynamic, unheld body"
    );
}

fn assert_centered_upright_hand(world: &ItemWorld, context: PlayerContext) {
    let expected = super::collision::add(
        context.eye,
        super::collision::scaled(context.view, super::HOLD_DISTANCE),
    );
    let center = world.body_center(1).expect("centered held body");
    assert!(
        center.distance(expected) < 0.001,
        "clear-space physical center follows the view without a side/down offset: {center:?}"
    );
    let held = world.snapshots().into_iter().next().expect("held rotation");
    assert!(
        held.pose
            .rotation
            .dot(Quat::from_rotation_y(-context.yaw))
            .abs()
            > 0.999,
        "the authored facing follows yaw and stays upright independently of pitch"
    );
}

fn assert_exact_body_target(world: &mut ItemWorld, eye: Vec3) {
    let (key, distance) = world
        .first_body_hit(eye, -Vec3::Z, 4.0)
        .expect("item-only ray sees the body behind the wall");
    assert_eq!(key, 1, "world geometry is resolved by the shared target");
    assert!(
        (distance - 1.5).abs() < 0.001,
        "actual front-face ray distance"
    );
    assert!(
        world.first_body_hit(eye, -Vec3::Z, 1.0).is_none(),
        "the item-only query obeys its explicit reach"
    );
    world
        .spawn(
            3,
            &PhysicsDef::default(),
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            Vec3::new(0.0, 1.6, 0.0),
            0.0,
            1.0,
        )
        .expect("fixed target blocker");
    assert_eq!(
        world.first_body_hit(eye, -Vec3::Z, 4.0).map(|hit| hit.0),
        Some(3),
        "a nearer unpickable fixed item remains the shared front target"
    );
    assert!(
        world
            .first_body_hit(Vec3::new(0.101, eye.y, eye.z), -Vec3::Z, 4.0)
            .is_none(),
        "an adjacent off-axis body is never selected by a wider cone"
    );
    assert!(
        world.first_body_hit(eye, -Vec3::Z, 4.001).is_none(),
        "the shared query cannot expand beyond its four-metre bound"
    );
    assert!(world.remove(3), "target blocker cleanup");
}

fn assert_half_turn_routes_around_player() {
    let wall = WallAabb::with_y(-2.0, 0.0, 1.5, 4.0, 4.0, 0.1);
    let mut world = ItemWorld::from_level(&level(""), &[wall]);
    let eye = Vec3::new(0.0, 1.6, 0.5);
    spawn_cube(&mut world, 1, Vec3::new(0.0, 1.6, -0.2));
    world.sync_context(player(eye), &[]);
    assert!(world.pickup(1, eye, -Vec3::Z, 0.0), "turn fixture pickup");
    advance(&mut world, player(eye), 15);
    let turned = PlayerContext {
        view: Vec3::Z,
        yaw: std::f32::consts::PI,
        ..player(eye)
    };
    let mut previous = world.body_center(1).expect("held center before turn");
    // A sharp view turn must route the continuous hand path around the actual
    // capsule, instead of freezing behind it or teleporting through its head.
    for _ in 0_u32..90_u32 {
        world.step(1.0 / 60.0, turned, &[]);
        let center = world.body_center(1).expect("held center during turn");
        assert!(
            center.distance(previous) <= 4.0 / 60.0 + 1.0e-4,
            "routing preserves the existing bounded hand speed"
        );
        let runtime = world.runtime.as_ref().expect("turn runtime");
        let entry = runtime.entries.get(&1).expect("turn item");
        let body = runtime.world.bodies.get(entry.body).expect("turn body");
        assert!(
            runtime
                .world
                .intersect_shape(
                    *body.position(),
                    entry.shape.as_ref(),
                    super::collision::query_filter().exclude_collider(entry.collider),
                )
                .next()
                .is_none(),
            "the routed pose stays clear of the enabled player and world"
        );
        assert!(
            center.distance(eye) > 0.1 + entry.radius,
            "the hand route stays outside the camera near plane"
        );
        previous = center;
    }
    assert!(
        previous.z - eye.z > 0.8,
        "a half-turn brings the item back in front of the camera: {previous:?}"
    );
    assert_centered_upright_hand(&world, turned);
    let quarter_turn = PlayerContext {
        view: Vec3::X,
        yaw: std::f32::consts::FRAC_PI_2,
        ..player(eye)
    };
    advance(&mut world, quarter_turn, 30);
    assert_centered_upright_hand(&world, quarter_turn);
    let pitched = PlayerContext {
        view: Vec3::new(0.8, 0.6, 0.0),
        ..quarter_turn
    };
    advance(&mut world, pitched, 30);
    assert_centered_upright_hand(&world, pitched);
    let against_wall = PlayerContext {
        eye: Vec3::new(0.0, 1.6, 1.2),
        feet: Vec3::new(0.0, 0.0, 1.2),
        ..turned
    };
    // This stage combines a player teleport with a yaw/pitch change. Allow the
    // same bounded catch-up interval as the half-turn before testing its goal.
    advance(&mut world, against_wall, 90);
    let shortened = world.body_center(1).expect("shortened held center");
    assert!(
        shortened.z - against_wall.eye.z > 0.1,
        "a wall-clipped target stays in front instead of circling the capsule: {shortened:?}"
    );
    assert!(world.drop(), "the routed hand pose remains safe to release");
}

// Protects recess geometry and capped catch-up together: two seconds in one
// frame must not consume two seconds, and a body must land below the base floor.
// One cube, one region, at most 252 tiny fixed steps; no asset loading.
#[test]
fn catch_up_is_bounded_and_recess_is_a_real_landing_surface() {
    let level = level(r#", "floor_regions":[{"x":-1,"z":-1,"width":2,"depth":2,"offset_y":-0.8}]"#);
    let mut world = ItemWorld::from_level(&level, &[]);
    spawn_cube(&mut world, 1, Vec3::new(0.0, 1.0, 0.0));
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    world.step(2.0, context, &[]);
    assert_eq!(
        world.stats().substeps,
        12,
        "long frames have a fixed work cap"
    );
    assert!(
        (world.stats().clamped_seconds - 1.9).abs() < 1.0e-5,
        "excess wall time is reported"
    );
    advance(&mut world, context, 120);
    let body = world.snapshots().into_iter().next().expect("cube snapshot");
    assert!(
        (body.pose.position.y + 0.7).abs() < 0.025,
        "body rests on the recessed floor: {:?}",
        body.pose
    );
    assert!(
        body.linear_velocity.length() < 0.03,
        "settled body loses contact motion"
    );
}

// Protects physically occluded pickup, carry sweeps and zero teleport velocity
// on release, plus the distinct capsule-trapping bug after a sharp half-turn.
// Two separate one-box fixtures for fewer than 650 tiny fixed steps.
#[test]
fn pickup_is_occluded_and_carry_stops_before_a_wall_without_launching_on_drop() {
    let level = level("");
    let wall = WallAabb::with_y(-0.5, 0.0, -0.8, 1.0, 4.0, 0.1);
    let mut world = ItemWorld::from_level(&level, &[wall]);
    let eye = Vec3::new(0.0, 1.6, 0.5);
    let context = player(eye);
    spawn_cube(&mut world, 1, Vec3::new(0.0, 1.6, -1.1));
    assert_eq!(
        world.nearest_pickup(eye, -Vec3::Z),
        None,
        "the wall occludes a body behind it"
    );
    assert_exact_body_target(&mut world, eye);
    assert!(world.remove(1), "occluded setup removes cleanly");
    spawn_cube(&mut world, 2, Vec3::new(0.0, 1.6, -0.2));
    tip_spawned_body(&mut world, 2);
    world.step(1.0 / 60.0, context, &[]);
    assert!(
        world.pickup(2, eye, -Vec3::Z, 0.0),
        "unoccluded body enters carry state"
    );
    assert!(
        !world.set_fixture_pose(
            2,
            super::ItemPose {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY
            }
        ),
        "developer fixture pose cannot replace a held item's transform"
    );
    let through_wall = player(Vec3::new(0.0, 1.6, -0.1));
    let constrained_eye = world.constrain_player_eye(through_wall);
    assert!(
        constrained_eye.z > 0.19,
        "ordinary capsule movement stops before entering the held box"
    );
    assert!(
        (constrained_eye.y - through_wall.eye.y).abs() < 1.0e-6,
        "the carry constraint preserves vertical controller output"
    );
    // Intentional developer teleports bypass the planar movement constraint.
    // The checked recovery must then clear both the player and the nearby wall.
    advance(&mut world, through_wall, 40);
    let held = world
        .snapshots()
        .into_iter()
        .next()
        .expect("held cube snapshot");
    assert!(
        held.pose.position.z > -0.6,
        "held cuboid stays in front of the wall"
    );
    assert!(
        held.pose.rotation.mul_vec3(Vec3::Y).dot(Vec3::Y) > 0.99,
        "a tipped pickup smoothly returns to its authored upright orientation"
    );
    let covering_door = DoorCollider::from_pose(
        [
            held.pose.position.x - 0.2,
            held.pose.position.y - 0.2,
            held.pose.position.z,
        ],
        [1.0, 0.0],
        0.4,
        0.1,
        0.4,
    );
    world.sync_context(through_wall, &[covering_door]);
    assert!(
        !world.drop(),
        "a newly overlapping live door prevents release"
    );
    assert_eq!(world.held(), Some(2), "a blocked drop keeps ownership");
    assert!(
        !world.throw(-Vec3::Z),
        "an obstructed throw keeps the same held state"
    );
    world.sync_context(through_wall, &[]);
    assert!(world.drop(), "safe checked pose releases");
    let dropped = world
        .snapshots()
        .into_iter()
        .next()
        .expect("dropped cube snapshot");
    assert!(
        dropped.linear_velocity.length() < 1.0e-5,
        "hand motion contributes no launch velocity"
    );
    assert!(
        dropped.angular_velocity.length() < 1.0e-5,
        "view turns contribute no synthetic spin"
    );
    assert_eq!(world.held(), None, "release clears the held state");
    assert_half_turn_routes_around_player();
    assert_carried_body_pushes_active_item();
}

// Two primitives and twelve fixed steps protect real controlled-body contacts;
// carry sweeps must not turn already active items into immovable obstacles.
fn assert_carried_body_pushes_active_item() {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let sphere = dynamic(Some(ShapeDef::Sphere { radius: 0.108 }));
    world
        .spawn(
            1,
            &sphere,
            (
                Vec3::new(-0.108, 0.0, -0.108),
                Vec3::new(0.108, 0.216, 0.108),
            ),
            Vec3::new(0.0, 1.492, 0.15),
            0.0,
            1.0,
        )
        .expect("carried sphere");
    let box_profile = dynamic(Some(ShapeDef::Cuboid {
        half_extents: [0.08; 3],
    }));
    world
        .spawn(
            2,
            &box_profile,
            (Vec3::splat(-0.08), Vec3::splat(0.08)),
            Vec3::new(0.0, 1.6, -0.07),
            0.0,
            1.0,
        )
        .expect("active carry target");
    let mut context = player(Vec3::new(0.0, 1.6, 1.0));
    world.sync_context(context, &[]);
    assert!(
        world.pickup(1, context.eye, context.view, context.yaw),
        "ordinary ray pickup"
    );
    let mut contact_seen = false;
    for _ in 0..12_u32 {
        context.eye.z -= 0.015;
        context.feet.z -= 0.015;
        world.step(super::FIXED_STEP, context, &[]);
        contact_seen |= world.contact_pairs().contains(&[1, 2]);
    }
    assert_eq!(world.held(), Some(1), "contact retains held ownership");
    assert!(
        contact_seen && snapshot(&world, 2).pose.position.z < -0.09,
        "Rapier contacts from the held collider push the existing active body"
    );
}

// Protects model-origin/collider-center mapping and capacity rejection using
// one dynamic and fixed filler bodies; no solver steps. This also protects reset
// at the allocation cap without constructing another large fixture.
#[test]
fn model_origin_survives_scaled_off_center_bounds_and_capacity_is_atomic() {
    let mut world = ItemWorld::default();
    let origin = Vec3::new(2.0, 3.0, 4.0);
    world
        .spawn(
            7,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                reset_group: Some("capacity".into()),
                ..PhysicsDef::default()
            },
            (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.4, 0.1)),
            origin,
            0.7,
            2.0,
        )
        .expect("off-center scaled model spawns");
    let body = world
        .snapshots()
        .into_iter()
        .next()
        .expect("off-center body snapshot");
    assert!(
        body.pose.position.distance(origin) < 1.0e-6,
        "render origin is preserved"
    );
    assert!(
        body.pose.rotation.dot(Quat::from_rotation_y(0.7)).abs() > 0.9999,
        "yaw orientation is preserved"
    );
    // Floor-origin models rotate about their physical center, including
    // interpolation between solver frames; their origin cannot be lerped alone.
    let local_center = Vec3::new(0.0, 0.4, 0.0);
    let fixed_center = Vec3::new(2.0, 3.4, 4.0);
    let tipped_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let offset = tipped_rotation.mul_vec3(local_center);
    let tipped = super::ItemPose {
        position: Vec3::new(
            fixed_center.x - offset.x,
            fixed_center.y - offset.y,
            fixed_center.z - offset.z,
        ),
        rotation: tipped_rotation,
    };
    let interpolated = body.pose.interpolate(tipped, 0.5, local_center);
    let interpolated_offset = interpolated.rotation.mul_vec3(local_center);
    let interpolated_center = Vec3::new(
        interpolated.position.x + interpolated_offset.x,
        interpolated.position.y + interpolated_offset.y,
        interpolated.position.z + interpolated_offset.z,
    );
    assert!(
        interpolated_center.distance(fixed_center) < 1.0e-6,
        "interpolation preserves the physical pivot"
    );
    for key in 100_u64..227_u64 {
        world
            .spawn(
                key,
                &PhysicsDef::default(),
                (Vec3::splat(-0.1), Vec3::splat(0.1)),
                Vec3::ZERO,
                0.0,
                1.0,
            )
            .expect("capacity fixture spawns");
    }
    assert_eq!(
        world.snapshots().len(),
        MAX_BODIES,
        "capacity fixture fills the exact bound"
    );
    assert_eq!(
        world
            .reset_group("capacity")
            .expect("reset at body cap")
            .reset_count,
        1,
        "reset reuses existing bodies even at the allocation cap"
    );
    assert_eq!(
        world.snapshots().len(),
        MAX_BODIES,
        "reset consumes no slot"
    );
    assert!(
        world
            .spawn(
                999,
                &PhysicsDef::default(),
                (Vec3::splat(-0.1), Vec3::splat(0.1)),
                Vec3::ZERO,
                0.0,
                1.0
            )
            .is_err(),
        "the next body is rejected"
    );
    assert_eq!(
        world.snapshots().len(),
        MAX_BODIES,
        "rejection leaves the world unchanged"
    );
}

// Protects paused repeated /give insertion and pending player collision before
// lazy initialization. Three attempts, two tiny boxes; no solver steps.
#[test]
fn clear_spawn_rejects_player_and_coincident_items_without_consuming_capacity() {
    let mut world = ItemWorld::default();
    let context = player(Vec3::new(0.0, 1.6, 0.0));
    world.sync_context(context, &[]);
    let definition = dynamic(None);
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    assert!(
        world
            .spawn_if_clear(1, &definition, bounds, Vec3::new(0.0, 1.0, 0.0), 0.0, 1.0)
            .is_err(),
        "first spawn includes the pending player capsule"
    );
    assert!(
        world.snapshots().is_empty(),
        "a player-blocked spawn consumes no slot"
    );
    let position = Vec3::new(0.0, 1.2, -1.0);
    world
        .spawn_if_clear(2, &definition, bounds, position, 0.0, 1.0)
        .expect("clear given item spawns");
    assert!(
        world
            .spawn_if_clear(3, &definition, bounds, position, 0.0, 1.0)
            .is_err(),
        "paused repeated give cannot stack coincident bodies"
    );
    assert_eq!(
        world.snapshots().len(),
        1,
        "overlap rejection preserves the existing body only"
    );
    assert_fixed_shapes_are_queryable_before_first_step();
}

// Guards lazy and late query registration, plus non-JSON profile safety.
// Two boxes per world, four insertions; no solver steps or external assets.
fn assert_fixed_shapes_are_queryable_before_first_step() {
    let mut world = ItemWorld::default();
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    let fixed = PhysicsDef {
        shape: Some(ShapeDef::Cuboid {
            half_extents: [0.1; 3],
        }),
        ..PhysicsDef::default()
    };
    let blocker = Vec3::new(0.0, 1.6, -0.2);
    for invalid in [
        PhysicsDef {
            friction: f32::NAN,
            ..fixed.clone()
        },
        PhysicsDef {
            restitution: f32::NAN,
            ..fixed.clone()
        },
    ] {
        assert!(
            world
                .install_fixed_shape(&invalid, bounds, blocker, 0.0, 1.0)
                .is_err()
        );
    }
    world
        .install_fixed_shape(&fixed, bounds, blocker, 0.0, 1.0)
        .expect("cached fixed blocker");
    world
        .spawn_authored(
            1,
            &dynamic(None),
            bounds,
            Vec3::new(0.0, 1.6, -0.8),
            0.0,
            1.0,
        )
        .expect("dormant target initializes queries");
    assert_eq!(
        world.nearest_pickup(Vec3::new(0.0, 1.6, 0.5), -Vec3::Z),
        None,
        "an explicit fixed shape occludes pickup before any solver step"
    );
    let later = Vec3::new(1.0, 1.0, 0.0);
    world
        .install_fixed_shape(&fixed, bounds, later, 0.0, 1.0)
        .expect("late fixed blocker");
    let count = collider_count(&world);
    assert!(
        world
            .spawn_if_clear(2, &dynamic(None), bounds, later, 0.0, 1.0)
            .is_err(),
        "late fixed geometry blocks insertion without a solver step"
    );
    assert_eq!(world.snapshots().len(), 1);
    assert_eq!(collider_count(&world), count);
}

// Protects mass-scaled impulses and CCD at an actual throw speed. Two 3 cm
// cubes, one 2 cm wall and at most 304 fixed steps; no assets or map baking.
#[test]
fn throws_use_actual_mass_and_ccd_keeps_a_small_fast_body_in_front_of_a_thin_wall() {
    let level = level("");
    let wall = WallAabb::with_y(-1.0, 0.0, -1.98, 2.0, 4.0, 0.02);
    let context = player(Vec3::new(0.0, 1.6, 1.0));
    let mut light = ItemWorld::from_level(&level, &[wall]);
    let mut heavy = ItemWorld::from_level(&level, &[wall]);
    let bounds = (Vec3::splat(-0.015), Vec3::splat(0.015));
    for (world, mass_kg) in [(&mut light, 1.0_f32), (&mut heavy, 5.0_f32)] {
        let definition = PhysicsDef {
            mass_kg,
            ..dynamic(None)
        };
        world
            .spawn(1, &definition, bounds, Vec3::new(0.0, 1.6, 0.3), 0.0, 1.0)
            .expect("tiny throw fixture spawns");
        world.step(1.0 / 60.0, context, &[]);
        assert!(
            world.pickup(1, context.eye, context.view, context.yaw),
            "throw fixture is picked up"
        );
        advance(world, context, 30);
    }
    assert!(
        !light.throw(Vec3::ZERO),
        "invalid aim cannot release an item"
    );
    assert_eq!(light.held(), Some(1), "invalid aim keeps held ownership");
    assert!(light.throw(-Vec3::Z), "clear light item throws");
    assert!(heavy.throw(-Vec3::Z), "clear heavy item throws");
    let light_release = light
        .snapshots()
        .into_iter()
        .next()
        .expect("light thrown body");
    let heavy_release = heavy
        .snapshots()
        .into_iter()
        .next()
        .expect("heavy thrown body");
    assert!(
        (light_release.linear_velocity.z + 8.0).abs() < 0.02,
        "one kilogram receives the controlled 8 m/s impulse"
    );
    assert!(
        (heavy_release.linear_velocity.z + 8.0 / 5.0_f32.sqrt()).abs() < 0.02,
        "five kilograms receives the reduced speed from actual mass"
    );
    advance(&mut light, context, 90);
    let settled = light
        .snapshots()
        .into_iter()
        .next()
        .expect("post-contact body");
    assert!(
        settled.pose.position.z > -1.95,
        "CCD prevents the fast small body crossing a thinner wall: {:?}",
        settled.pose
    );
    assert!(
        settled.linear_velocity.is_finite() && settled.angular_velocity.is_finite(),
        "throw contacts retain finite motion"
    );
    assert!(
        light
            .take_impacts()
            .iter()
            .any(|impact| { (impact.position.z + 1.96).abs() < 0.006 && impact.position.y > 0.4 }),
        "the impact position is the actual wall contact, rather than the body origin"
    );
}

fn reset_fixture_pose(world: &mut ItemWorld, key: u64, position: Vec3) {
    assert!(
        world.set_fixture_pose(
            key,
            super::ItemPose {
                position,
                rotation: Quat::IDENTITY,
            }
        ),
        "controlled reset fixture scattering"
    );
}

fn add_reset_fixture_forces(world: &mut ItemWorld) {
    let runtime = world.runtime.as_mut().expect("reset runtime");
    let handle = runtime.entries.get(&2).expect("second member").body;
    let body = runtime
        .world
        .bodies
        .get_mut(handle)
        .expect("second solver body");
    body.add_force(super::collision::vector(Vec3::Y), true);
    body.add_torque(super::collision::vector(Vec3::X), true);
}

fn assert_reset_preserved(world: &ItemWorld, before: &[super::BodySnapshot]) {
    for (old, current) in before.iter().zip(world.snapshots()) {
        assert_eq!(
            old.pose, current.pose,
            "blocked reset changes no member pose"
        );
        assert_eq!(
            old.linear_velocity, current.linear_velocity,
            "blocked reset changes no velocity"
        );
        assert_eq!(
            old.angular_velocity, current.angular_velocity,
            "blocked reset changes no spin"
        );
    }
}

fn assert_restored_group(
    world: &ItemWorld,
    foreign: &super::BodySnapshot,
    first_origin: Vec3,
    second_origin: Vec3,
) {
    for snapshot in world.snapshots() {
        if snapshot.key == 9 {
            assert_eq!(snapshot.pose, foreign.pose, "unrelated pose stays exact");
            assert_eq!(
                snapshot.linear_velocity, foreign.linear_velocity,
                "unrelated velocity stays exact"
            );
            assert_eq!(
                snapshot.angular_velocity, foreign.angular_velocity,
                "unrelated spin stays exact"
            );
        } else {
            let expected = if snapshot.key == 1 {
                first_origin
            } else {
                second_origin
            };
            assert_eq!(snapshot.pose.position, expected, "authored origin restored");
            assert_eq!(
                snapshot.linear_velocity,
                Vec3::ZERO,
                "member linear velocity clears"
            );
            assert_eq!(
                snapshot.angular_velocity,
                Vec3::ZERO,
                "member angular velocity clears"
            );
        }
    }
    let runtime = world.runtime.as_ref().expect("reset runtime");
    for entry in runtime
        .entries
        .values()
        .filter(|entry| entry.reset_group.as_deref() == Some("rack_a"))
    {
        let body = runtime
            .world
            .bodies
            .get(entry.body)
            .expect("reset solver body");
        assert!(
            body.is_dynamic(),
            "held member's physical motion is restored"
        );
        assert_eq!(
            body.user_force(),
            rapier3d::prelude::Vector::ZERO,
            "reset clears accumulated force"
        );
        assert_eq!(
            body.user_torque(),
            rapier3d::prelude::Vector::ZERO,
            "reset clears accumulated torque"
        );
        assert_eq!(entry.previous, entry.current, "reset snaps interpolation");
    }
}

fn collider_count(world: &ItemWorld) -> usize {
    world
        .runtime
        .as_ref()
        .expect("fixture runtime")
        .world
        .colliders
        .len()
}

fn snapshot(world: &ItemWorld, key: u64) -> super::BodySnapshot {
    world
        .snapshots()
        .into_iter()
        .find(|body| body.key == key)
        .expect("fixture body snapshot")
}

fn assert_authored_dormant(world: &ItemWorld, originals: &[super::BodySnapshot]) {
    for original in originals {
        let current = snapshot(world, original.key);
        assert_eq!(
            current.pose, original.pose,
            "dormancy retains the exact saved pose"
        );
        assert_eq!(
            current.linear_velocity,
            Vec3::ZERO,
            "dormancy has no translation"
        );
        assert_eq!(current.angular_velocity, Vec3::ZERO, "dormancy has no spin");
        assert_eq!(
            current.dormant,
            current.motion.is_movable(),
            "fixed remains distinct"
        );
        if current.motion.is_movable() {
            assert!(
                !current.sleeping,
                "authored dormancy is distinct from ordinary sleep"
            );
        }
        let mut published = None;
        world.for_each_pose(|key, pose| {
            if key == original.key {
                published = Some(pose);
            }
        });
        assert_eq!(
            published,
            Some(original.pose),
            "rendering retains the same exact authored pose"
        );
    }
}

fn assert_dormant_pickup_reset_and_heavy_throw() {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let definition = PhysicsDef {
        mass_kg: 5.0,
        reset_group: Some("heavy".into()),
        ..dynamic(Some(ShapeDef::Sphere { radius: 0.1 }))
    };
    let context = player(Vec3::new(0.0, 1.6, 0.5));
    world
        .spawn_authored(
            1,
            &definition,
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            Vec3::new(0.0, 1.6, -0.4),
            0.0,
            1.0,
        )
        .expect("dormant heavy sphere");
    world.sync_context(context, &[]);
    assert_eq!(
        world.nearest_pickup(context.eye, context.view),
        Some(1),
        "dormant collider remains queryable"
    );
    assert!(
        world.pickup(1, context.eye, context.view, 0.0),
        "pickup activates dormancy"
    );
    assert!(!snapshot(&world, 1).dormant, "held item stays activated");
    let report = world.reset_group("heavy").expect("held authored reset");
    assert_eq!(
        report.released_held,
        Some(1),
        "reset clears member ownership"
    );
    assert_eq!(
        world.held(),
        None,
        "held member returns to its authored state"
    );
    assert!(
        snapshot(&world, 1).dormant,
        "authored reset restores dormancy"
    );
    assert!(
        world.pickup(1, context.eye, context.view, 0.0),
        "reset item remains reusable"
    );
    assert!(world.throw(context.view), "clear heavy item can be thrown");
    let thrown = snapshot(&world, 1);
    assert!(!thrown.dormant, "throw preserves the activated state");
    assert!(
        (thrown.linear_velocity.length() - 8.0 / 5.0_f32.sqrt()).abs() < 0.001,
        "fixed-to-dynamic-to-held transitions preserve the real five-kilogram mass"
    );
}

fn assert_ordinary_sleep_is_not_dormancy(world: &mut ItemWorld, context: PlayerContext) {
    assert!(world.activate(1), "ordinary dynamic activation");
    world.step(super::FIXED_STEP, context, &[]);
    let runtime = world.runtime.as_mut().expect("sleep fixture runtime");
    let handle = runtime.entries.get(&1).expect("activated entry").body;
    runtime
        .world
        .bodies
        .get_mut(handle)
        .expect("activated body")
        .sleep();
    world.step(super::FIXED_STEP, context, &[]);
    let asleep = snapshot(world, 1);
    assert!(
        asleep.sleeping && !asleep.dormant,
        "solver sleeping never restores authored dormancy"
    );
}

fn assert_background_solver_preserves_authored(
    world: &mut ItemWorld,
    context: PlayerContext,
    originals: &[super::BodySnapshot],
) {
    spawn_cube(world, 9, Vec3::new(2.5, 2.0, -2.5));
    for _ in 0_u32..4_u32 {
        world.step(super::FIXED_STEP, context, &[]);
    }
    assert_authored_dormant(world, originals);
    assert!(
        snapshot(world, 9).linear_velocity.y < -0.1,
        "an unrelated active body runs the solver while imperfect authored placements stay exact"
    );
}

fn assert_authored_group_activation_reset(
    world: &mut ItemWorld,
    context: PlayerContext,
    originals: &[super::BodySnapshot],
) {
    let original_dynamic = snapshot(world, 1);
    let original_kinematic = snapshot(world, 2);
    let original_fixed = snapshot(world, 3);
    let report = world
        .activate_group("stage")
        .expect("explicit gameplay activation");
    assert_eq!(report.activated_count, 3, "named movable group activates");
    assert_eq!(
        snapshot(world, 1).pose,
        original_dynamic.pose,
        "activation supplies no synthetic movement"
    );
    world.step(super::FIXED_STEP, context, &[]);
    assert!(
        snapshot(world, 1).pose.position.y < original_dynamic.pose.position.y,
        "activated dynamic starts natural gravity"
    );
    assert_eq!(
        snapshot(world, 2).pose.position,
        original_kinematic.pose.position,
        "kinematic waits for prescribed motion"
    );
    let _reset = world
        .reset_group("stage")
        .expect("authored reset accepts its original floor penetration");
    world.step(super::FIXED_STEP, context, &[]);
    assert_authored_dormant(world, originals);
    assert!(
        world.set_kinematic_pose(
            2,
            super::ItemPose {
                position: Vec3::new(1.0, 0.3, 0.0),
                rotation: Quat::IDENTITY
            }
        ),
        "prescribed motion activates authored kinematics"
    );
    world.step(super::FIXED_STEP, context, &[]);
    assert!(
        !snapshot(world, 2).dormant,
        "kinematic activation remains active"
    );
    assert!(
        (snapshot(world, 2).pose.position.y - 0.3).abs() < 0.001,
        "prescribed pose is reached"
    );
    assert_eq!(
        snapshot(world, 3).pose,
        original_fixed.pose,
        "fixed placement is untouched"
    );
}

// Protects engine-wide exact initial placement (including floor penetration),
// explicit/kinematic activation, reset and real heavy pickup mass. Six tiny
// bodies across two worlds; 30 idle frames do no solver work, then nine fixed
// steps also prove unrelated active simulation cannot dislodge dormancy.
#[test]
fn authored_dormancy_retains_imperfect_poses_until_activation_and_reset() {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let definition = PhysicsDef {
        reset_group: Some("stage".into()),
        ..dynamic(None)
    };
    let kinematic = PhysicsDef {
        motion: BodyMotion::Kinematic,
        pickup: false,
        ..definition.clone()
    };
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    for (key, profile, position) in [
        (1_u64, &definition, Vec3::new(0.0, 1.6, -0.4)),
        (2, &kinematic, Vec3::new(1.0, 0.035, 0.0)),
        (3, &PhysicsDef::default(), Vec3::new(-1.0, 0.035, 0.0)),
        (4, &definition, Vec3::new(-1.5, 0.035, 1.0)),
    ] {
        world
            .spawn_authored(key, profile, bounds, position, 0.37, 1.0)
            .expect("authored placement");
    }
    let originals = world.snapshots();
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    advance(&mut world, context, 30);
    assert_eq!(
        world.stats().substeps,
        0,
        "fully dormant scenes avoid solver work"
    );
    assert_eq!(
        world.stats().dormant_bodies,
        3,
        "fixed props never count as dormant"
    );
    assert_authored_dormant(&world, &originals);
    assert_background_solver_preserves_authored(&mut world, context, &originals);
    assert!(
        !world.activate(3),
        "permanent fixed definitions cannot activate"
    );
    assert_authored_group_activation_reset(&mut world, context, &originals);
    assert_ordinary_sleep_is_not_dormancy(&mut world, context);
    assert_dormant_pickup_reset_and_heavy_throw();
}

fn assert_simultaneous_dormant_contacts(context: PlayerContext) {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let sphere = dynamic(Some(ShapeDef::Sphere { radius: 0.1 }));
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    for (key, horizontal) in [(1_u64, -0.14_f32), (2_u64, 0.14_f32)] {
        world
            .spawn_authored(
                key,
                &sphere,
                bounds,
                Vec3::new(horizontal, 1.2, 0.0),
                0.0,
                1.0,
            )
            .expect("simultaneous target");
    }
    let wide = dynamic(Some(ShapeDef::Cuboid {
        half_extents: [0.15, 0.05, 0.05],
    }));
    world
        .spawn(
            9,
            &wide,
            (Vec3::new(-0.15, -0.05, -0.05), Vec3::new(0.15, 0.05, 0.05)),
            Vec3::new(0.0, 1.2, 0.155),
            0.0,
            1.0,
        )
        .expect("wide active source");
    assert!(
        world.set_fixture_velocity(9, Vec3::new(0.0, 0.0, -8.0), Vec3::ZERO),
        "incoming simultaneous motion"
    );
    world.step(super::FIXED_STEP, context, &[]);
    assert!(
        !snapshot(&world, 1).dormant && !snapshot(&world, 2).dormant,
        "all simultaneous contacts activate before solving, rather than only the nearest"
    );
}

fn assert_offset_wall_blocks_whole_incoming_shape(context: PlayerContext) {
    let wall = WallAabb::with_y(0.105, 0.0, -2.0, 0.1, 2.0, 2.01);
    let mut world = ItemWorld::from_level(&level(""), &[wall]);
    let small = dynamic(Some(ShapeDef::Sphere { radius: 0.03 }));
    world
        .spawn_authored(
            1,
            &small,
            (Vec3::splat(-0.03), Vec3::splat(0.03)),
            Vec3::new(0.15, 1.0, 0.09),
            0.0,
            1.0,
        )
        .expect("off-axis dormant receiver");
    let incoming = dynamic(Some(ShapeDef::Sphere { radius: 0.1 }));
    world
        .spawn(
            9,
            &incoming,
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            Vec3::Y,
            0.0,
            1.0,
        )
        .expect("wide incoming sphere");
    assert!(
        world.set_fixture_velocity(9, Vec3::new(8.0, 0.0, 0.0), Vec3::ZERO),
        "incoming wall approach"
    );
    for _ in 0_u32..4_u32 {
        world.step(super::FIXED_STEP, context, &[]);
    }
    assert!(
        snapshot(&world, 1).dormant,
        "the leading surface hits a wall before the offset receiver even though its contact-witness ray misses the wall"
    );
    assert!(
        world.body_center(9).expect("blocked sphere center").x < 0.011,
        "CCD also confirms the whole-body obstruction"
    );
}

// Protects same-step real collision response through an initially touching
// dormant chain, plus all simultaneous targets. Six primitive bodies across
// two worlds, nine fixed steps; no map baking or asset decoding.
#[test]
fn active_impacts_activate_all_dormant_contacts_before_the_first_solve() {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let definition = dynamic(Some(ShapeDef::Sphere { radius: 0.1 }));
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    for (key, depth) in [(1_u64, 0.0_f32), (2_u64, -0.2_f32)] {
        world
            .spawn_authored(
                key,
                &definition,
                bounds,
                Vec3::new(0.0, 1.2, depth),
                0.0,
                1.0,
            )
            .expect("touching dormant chain");
    }
    world
        .spawn(9, &definition, bounds, Vec3::new(0.0, 1.2, 0.225), 0.0, 1.0)
        .expect("explicit active source");
    assert!(
        world.set_fixture_velocity(9, Vec3::new(0.0, 0.0, -8.0), Vec3::ZERO),
        "incoming collision"
    );
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    world.step(super::FIXED_STEP, context, &[]);
    assert!(
        !snapshot(&world, 1).dormant && !snapshot(&world, 2).dormant,
        "touching chain becomes dynamic before the first incoming solve: {:?}",
        world.snapshots()
    );
    let mut saw_front_contact = world.contact_pairs().contains(&[1, 9]);
    let mut saw_chain_contact = world.contact_pairs().contains(&[1, 2]);
    for _ in 0_u32..7_u32 {
        world.step(super::FIXED_STEP, context, &[]);
        saw_front_contact |= world.contact_pairs().contains(&[1, 9]);
        saw_chain_contact |= world.contact_pairs().contains(&[1, 2]);
    }
    assert!(
        saw_front_contact && saw_chain_contact,
        "activation is followed by actual solver contacts along the chain"
    );
    assert!(
        snapshot(&world, 2).linear_velocity.z < -0.01,
        "the rear body receives a real impulse without an immovable first collision"
    );
    assert!(
        snapshot(&world, 9).linear_velocity.z < 0.0,
        "incoming body does not rebound from a dormant fixed barrier"
    );
    assert_simultaneous_dormant_contacts(context);
    assert_offset_wall_blocks_whole_incoming_shape(context);
}

// Protects atomic refusal, held ownership, scoped motion/force clearing and
// repeated reuse and controlled-body velocity clearing. Four tiny bodies,
// one solver step, no asset loads.
#[test]
fn reset_group_is_atomic_scoped_and_restores_a_held_member_without_spawning() {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let definition = PhysicsDef {
        reset_group: Some("rack_a".into()),
        ..dynamic(None)
    };
    let bounds = (Vec3::splat(-0.1), Vec3::splat(0.1));
    let first_origin = Vec3::new(0.0, 1.6, -0.4);
    let second_origin = Vec3::new(1.0, 0.1, 0.0);
    for (key, origin) in [(1_u64, first_origin), (2_u64, second_origin)] {
        world
            .spawn(key, &definition, bounds, origin, 0.0, 1.0)
            .expect("group member");
    }
    spawn_cube(&mut world, 9, Vec3::new(2.0, 0.1, 0.0));
    let context = player(Vec3::new(0.0, 1.6, 0.5));
    world.sync_context(context, &[]);
    assert!(
        world.pickup(1, context.eye, context.view, 0.0),
        "held reset member"
    );
    reset_fixture_pose(&mut world, 2, first_origin);
    assert!(
        world.set_fixture_velocity(2, Vec3::Z, Vec3::X),
        "group scattering motion"
    );
    add_reset_fixture_forces(&mut world);
    reset_fixture_pose(&mut world, 9, second_origin);
    let before = world.snapshots();
    assert!(
        world.reset_group("rack_a").is_err(),
        "foreign body blocks an authored destination"
    );
    assert_eq!(world.held(), Some(1), "blocked reset keeps held ownership");
    assert_reset_preserved(&world, &before);
    reset_fixture_pose(&mut world, 9, Vec3::new(2.0, 0.1, 0.0));
    assert!(
        world.set_fixture_velocity(9, Vec3::X, Vec3::Y),
        "unrelated motion"
    );
    let foreign = world
        .snapshots()
        .into_iter()
        .find(|body| body.key == 9)
        .expect("foreign snapshot");
    world.sync_context(player(first_origin), &[]);
    assert!(
        world.reset_group("rack_a").is_err(),
        "player blocks reset destination"
    );
    assert_eq!(
        world.held(),
        Some(1),
        "player-blocked reset keeps ownership"
    );
    world.sync_context(context, &[]);
    let resource_count = collider_count(&world);
    let report = world
        .reset_group("rack_a")
        .expect("clear authored destinations");
    assert_eq!(report.reset_count, 2, "only named group resets");
    assert_eq!(
        report.released_held,
        Some(1),
        "held group member returns safely to simulation"
    );
    assert_eq!(
        world.held(),
        None,
        "reset clears only a member's held state"
    );
    assert_restored_group(&world, &foreign, first_origin, second_origin);
    for _ in 0_u32..3_u32 {
        let _report = world
            .reset_group("rack_a")
            .expect("repeat reset reuses bodies");
    }
    assert_eq!(
        collider_count(&world),
        resource_count,
        "reset grows no resources"
    );
    assert!(
        world.reset_group("missing").is_err(),
        "unknown group is explicit"
    );
    assert!(
        world.reset_group("../rack_a").is_err(),
        "group names reject path syntax"
    );
    assert_controlled_reset_clears_velocity();
}

fn assert_controlled_reset_clears_velocity() {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let definition = PhysicsDef {
        motion: BodyMotion::Kinematic,
        pickup: false,
        reset_group: Some("controlled".into()),
        ..dynamic(None)
    };
    world
        .spawn(
            1,
            &definition,
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            Vec3::new(0.0, 1.0, 0.0),
            0.0,
            1.0,
        )
        .expect("controlled runtime body");
    assert!(world.set_kinematic_pose(
        1,
        super::ItemPose {
            position: Vec3::new(0.1, 1.1, 0.0),
            rotation: Quat::from_rotation_y(0.2),
        }
    ));
    world.step(super::FIXED_STEP, player(Vec3::new(2.0, 1.6, 2.0)), &[]);
    let moving = snapshot(&world, 1);
    assert!(moving.linear_velocity.length() > 1.0 && moving.angular_velocity.length() > 1.0);
    let count = collider_count(&world);
    let report = world
        .reset_group("controlled")
        .expect("clear controlled origin");
    let restored = snapshot(&world, 1);
    assert_eq!(report.reset_count, 1);
    assert_eq!(restored.linear_velocity, Vec3::ZERO);
    assert_eq!(restored.angular_velocity, Vec3::ZERO);
    assert_eq!(restored.pose.position, Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(collider_count(&world), count);
}

fn pin_profile() -> PhysicsDef {
    let mut parts: Vec<CompoundPart> = [
        (0.078_f32, 0.055_f32),
        (0.128, 0.060),
        (0.170, 0.047),
        (0.327, 0.041),
        (0.340, 0.041),
    ]
    .into_iter()
    .map(|(height, radius)| CompoundPart {
        offset: [0.0, height, 0.0],
        shape: PrimitiveShapeDef::Sphere { radius },
    })
    .collect();
    parts.extend([
        CompoundPart {
            offset: [0.0, 0.0125, 0.0],
            shape: PrimitiveShapeDef::Cylinder {
                half_height: 0.0125,
                radius: 0.035,
            },
        },
        CompoundPart {
            offset: [0.0, 0.250, 0.0],
            shape: PrimitiveShapeDef::Cylinder {
                half_height: 0.045,
                radius: 0.024,
            },
        },
    ]);
    PhysicsDef {
        shape: Some(ShapeDef::Compound { parts }),
        mass_kg: 1.5,
        friction: 0.6,
        restitution: 0.1,
        linear_damping: 0.05,
        angular_damping: 0.08,
        reset_group: Some("pins".into()),
        ..dynamic(None)
    }
}

fn assert_compound_metadata_bounds(
    world: &mut ItemWorld,
    definition: &PhysicsDef,
    pin_bounds: (Vec3, Vec3),
) {
    let round_trip: PhysicsDef =
        serde_json::from_str(&serde_json::to_string(definition).expect("compound serializes"))
            .expect("compound parses");
    assert_eq!(
        round_trip, *definition,
        "compound/group metadata round trips"
    );
    let mut too_many = definition.clone();
    if let Some(ShapeDef::Compound { parts }) = too_many.shape.as_mut() {
        let part = parts.first().expect("pin primitive").clone();
        parts.extend([part.clone(), part]);
    }
    assert!(
        too_many.validate().is_err(),
        "direct API caps compound primitives"
    );
    assert!(
        serde_json::from_str::<PhysicsDef>(
            &serde_json::to_string(&too_many).expect("over-budget metadata")
        )
        .is_err(),
        "serde caps primitive allocation"
    );
    let invalid = PhysicsDef {
        shape: Some(ShapeDef::Compound {
            parts: vec![CompoundPart {
                offset: [101.0, 0.0, 0.0],
                shape: PrimitiveShapeDef::Sphere { radius: 0.1 },
            }],
        }),
        ..dynamic(None)
    };
    assert!(invalid.validate().is_err(), "compound offsets stay bounded");
    assert!(
        world
            .spawn(1, &too_many, pin_bounds, Vec3::ZERO, 0.0, 1.0)
            .is_err(),
        "invalid compound grows no runtime"
    );
    assert!(
        world.snapshots().is_empty(),
        "invalid shape consumes no slot"
    );
}

fn assert_compound_center_interpolation(world: &mut ItemWorld, context: PlayerContext) {
    let (authored, center_offset, fixed_center) = {
        let runtime = world
            .runtime
            .as_ref()
            .expect("compound interpolation runtime");
        let entry = runtime.entries.get(&1).expect("compound entry");
        let body = runtime.world.bodies.get(entry.body).expect("compound body");
        assert!(
            super::collision::point(body.local_center_of_mass()).length() > 0.015,
            "the regression uses a real compound mass center away from its bounds center"
        );
        (
            entry.authored_pose,
            entry.interpolation_center,
            super::collision::point(body.center_of_mass()),
        )
    };
    let tipped_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let tipped = super::ItemPose {
        position: super::collision::subtract(fixed_center, tipped_rotation.mul_vec3(center_offset)),
        rotation: tipped_rotation,
    };
    let entry = world
        .runtime
        .as_mut()
        .expect("runtime")
        .entries
        .get_mut(&1)
        .expect("entry");
    entry.previous = authored;
    entry.current = tipped;
    world.step(super::FIXED_STEP * 0.5, context, &[]);
    let mut published = None;
    world.for_each_pose(|key, pose| {
        if key == 1 {
            published = Some(pose);
        }
    });
    let interpolated = published.expect("render interpolation pose");
    assert_eq!(
        snapshot(world, 1).render_pose,
        interpolated,
        "diagnostics publish the renderer's exact interpolated pose"
    );
    let rendered_center = super::collision::add(
        interpolated.position,
        interpolated.rotation.mul_vec3(center_offset),
    );
    assert!(
        rendered_center.distance(fixed_center) < 1.0e-6,
        "rotation interpolation keeps the actual compound COM fixed"
    );
    let restored_entry = world
        .runtime
        .as_mut()
        .expect("runtime")
        .entries
        .get_mut(&1)
        .expect("entry");
    restored_entry.previous = authored;
    restored_entry.current = authored;
}

fn spawn_slow_rolling_ball(world: &mut ItemWorld) {
    let _settling_impacts = world.take_impacts();
    let ball = PhysicsDef {
        mass_kg: 5.0,
        restitution: 0.2,
        friction: 0.45,
        linear_damping: 0.03,
        angular_damping: 0.08,
        ..dynamic(Some(ShapeDef::Sphere { radius: 0.108 }))
    };
    world
        .spawn(
            2,
            &ball,
            (
                Vec3::new(-0.108, 0.0, -0.108),
                Vec3::new(0.108, 0.216, 0.108),
            ),
            Vec3::new(0.0, 0.0, 0.1814),
            0.0,
            1.0,
        )
        .expect("rolling ball");
}

fn cylinder_bottom(pose: super::ItemPose, half_height: f32, radius: f32) -> f32 {
    let axis = pose.rotation.mul_vec3(Vec3::Y);
    let center = super::collision::add(pose.position, super::collision::scaled(axis, half_height));
    center.y - half_height.mul_add(axis.y.abs(), radius * axis.x.hypot(axis.z))
}

// Protects floor contact when a falling heavy ball wakes and strikes a very
// light resting cylinder, plus a late rolling load. Two two-body fixtures,
// 60 settling plus 128 impact steps;
// matching physical dimensions/masses requires no models or native harness.
#[test]
fn heavy_falling_impact_cannot_push_a_sleeping_light_cylinder_through_the_floor() {
    let minimum = heavy_impact_fixture().min(heavy_loaded_cylinder_fixture());
    assert!(
        minimum >= -0.006,
        "floor support after heavy impact: {minimum}"
    );
}

fn heavy_impact_fixture() -> f32 {
    let fixture = level("");
    let pack = PackMaterials::new(
        "light_cup_contact",
        Some(r#"{"core:carpet":{"texture":"unused.png","ground_friction":0.6}}"#),
        std::collections::HashMap::new(),
    );
    let materials = MaterialTable::logical(
        &fixture,
        &crate::engine::assets::AssetCatalog::builtin(),
        Some(&pack),
    );
    let mut world = ItemWorld::from_level_with_materials(&fixture, &[], &materials);
    let light_profile = PhysicsDef {
        mass_kg: 0.025,
        friction: 0.55,
        restitution: 0.18,
        linear_damping: 0.02,
        angular_damping: 0.12,
        ..dynamic(Some(ShapeDef::Cylinder {
            half_height: 0.061,
            radius: 0.0425,
        }))
    };
    world
        .spawn(
            1,
            &light_profile,
            (
                Vec3::new(-0.0425, 0.0, -0.0425),
                Vec3::new(0.0425, 0.122, 0.0425),
            ),
            Vec3::ZERO,
            0.0,
            1.0,
        )
        .expect("light floor-contact cylinder");
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    advance(&mut world, context, 30);
    let runtime = world.runtime.as_mut().expect("contact fixture runtime");
    let handle = runtime.entries.get(&1).expect("resting cylinder").body;
    runtime
        .world
        .bodies
        .get_mut(handle)
        .expect("resting body")
        .sleep();
    let ball = PhysicsDef {
        mass_kg: 5.0,
        friction: 0.45,
        restitution: 0.2,
        linear_damping: 0.03,
        angular_damping: 0.08,
        ..dynamic(Some(ShapeDef::Sphere { radius: 0.108 }))
    };
    world
        .spawn(
            2,
            &ball,
            (
                Vec3::new(-0.108, 0.0, -0.108),
                Vec3::new(0.108, 0.216, 0.108),
            ),
            Vec3::new(-0.1253, 0.0949, 0.0),
            -std::f32::consts::FRAC_PI_2,
            1.0,
        )
        .expect("incoming heavy ball");
    assert!(
        world.set_fixture_velocity(2, Vec3::new(3.325, -4.783, 0.0), Vec3::ZERO),
        "falling heavy impact"
    );
    let mut contact_seen = false;
    let mut minimum = 0.0_f32;
    for _ in 0_u32..8_u32 {
        world.step(super::FIXED_STEP, context, &[]);
        let cylinder = snapshot(&world, 1);
        let bottom = cylinder_bottom(cylinder.pose, 0.061, 0.0425);
        minimum = minimum.min(bottom);
        contact_seen |= world.contact_pairs().contains(&[1, 2]);
    }
    assert!(
        contact_seen,
        "the heavy/light contact occurs in the regression"
    );
    assert!(
        world.take_impacts().iter().any(|impact| {
            [Some(impact.key), impact.other_key].contains(&Some(1))
                && [Some(impact.key), impact.other_key].contains(&Some(2))
        }),
        "a new item-pair impact is emitted even while the cup already touches its floor"
    );
    minimum
}

// Reproduces the native late impact with a five-kilogram ball rolling over
// a tipped 25-gram cup. The exact finite pre-contact poses/motion are retained
// from the failure; two primitives and 120 steps protect coupled floor contacts.
fn heavy_loaded_cylinder_fixture() -> f32 {
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let cylinder = PhysicsDef {
        mass_kg: 0.025,
        friction: 0.55,
        restitution: 0.18,
        linear_damping: 0.02,
        angular_damping: 0.12,
        ..dynamic(Some(ShapeDef::Cylinder {
            half_height: 0.061,
            radius: 0.0425,
        }))
    };
    let ball = PhysicsDef {
        mass_kg: 5.0,
        friction: 0.45,
        restitution: 0.2,
        linear_damping: 0.03,
        angular_damping: 0.08,
        ..dynamic(Some(ShapeDef::Sphere { radius: 0.108 }))
    };
    world
        .spawn(
            1,
            &cylinder,
            (
                Vec3::new(-0.0425, 0.0, -0.0425),
                Vec3::new(0.0425, 0.122, 0.0425),
            ),
            Vec3::ZERO,
            0.0,
            1.0,
        )
        .expect("loaded cylinder");
    world
        .spawn(
            2,
            &ball,
            (
                Vec3::new(-0.108, 0.0, -0.108),
                Vec3::new(0.108, 0.216, 0.108),
            ),
            Vec3::new(-0.2, 0.3, 0.0),
            0.0,
            1.0,
        )
        .expect("loading ball");
    for (key, position, rotation, linear, angular) in [
        (
            1,
            Vec3::new(0.0, 0.040_130_313, 0.0),
            Quat::from_array([-0.081_790_22, -0.087_495_41, 0.698_931_16, -0.705_088_9]),
            Vec3::new(-0.126_761_2, -0.077_661_91, -0.230_938_26),
            Vec3::new(-4.897_148_6, -1.219_964_1, 3.005_335_3),
        ),
        (
            2,
            Vec3::new(-0.054_340_363, 0.216_104_73, 0.066_187_86),
            Quat::from_array([-0.703_342_9, 0.065_317_59, -0.705_204_7, -0.061_062_276]),
            Vec3::new(1.808_937, 0.110_011_44, 0.109_087_92),
            Vec3::new(1.072_277_3, 0.257_637_38, -17.55999),
        ),
    ] {
        assert!(world.set_fixture_pose(key, super::ItemPose { position, rotation }));
        assert!(world.set_fixture_velocity(key, linear, angular));
    }
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    let mut minimum = 0.0_f32;
    let mut contact_seen = false;
    for _ in 0..120_u32 {
        world.step(super::FIXED_STEP, context, &[]);
        minimum = minimum.min(cylinder_bottom(snapshot(&world, 1).pose, 0.061, 0.0425));
        contact_seen |= world.contact_pairs().contains(&[1, 2]);
    }
    assert!(
        contact_seen,
        "the rolling ball actually loads the tipped cup"
    );
    minimum
}

// Protects primitive bounds, actual bottom-heavy inertia/toppling and slow
// spinning speculative-contact activation. One pin and one ball, 241 fixed
// steps; the floor-origin shape reproduces the native miss without map assets.
#[test]
fn bounded_compound_pin_derives_its_mass_distribution_and_topples_from_ball_contact() {
    let definition = pin_profile();
    let mut world = ItemWorld::from_level(&level(""), &[]);
    let pin_bounds = (Vec3::new(-0.06, 0.0, -0.06), Vec3::new(0.06, 0.38, 0.06));
    assert_compound_metadata_bounds(&mut world, &definition, pin_bounds);
    world
        .spawn_authored(1, &definition, pin_bounds, Vec3::ZERO, 0.0, 1.0)
        .expect("compound pin");
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    assert!(
        world.activate(1),
        "interpolation fixture activates the compound"
    );
    assert_compound_center_interpolation(&mut world, context);
    let _reset = world
        .reset_group("pins")
        .expect("pin returns dormant before collision");
    spawn_slow_rolling_ball(&mut world);
    advance(&mut world, context, 30);
    let runtime = world.runtime.as_ref().expect("compound runtime");
    let entry = runtime.entries.get(&1).expect("pin entry");
    let body = runtime.world.bodies.get(entry.body).expect("pin body");
    assert!(
        body.local_center_of_mass().y < -0.015,
        "compound's physical mass is lower than its bounds center"
    );
    assert!(
        (body.mass() - 1.5).abs() < 0.001,
        "authored total mass is retained"
    );
    assert!(
        world.set_fixture_velocity(2, Vec3::new(0.0, -0.032, -1.54), Vec3::new(-14.3, 0.0, 0.0)),
        "slow spun floor-contact impact reproduces the native speculative-contact gap"
    );
    world.step(super::FIXED_STEP, context, &[]);
    assert!(
        !snapshot(&world, 1).dormant,
        "a slow rolling ball activates a compound before a speculative fixed-body response"
    );
    let mut saw_item_contact = false;
    for _ in 0_u32..90_u32 {
        world.step(1.0 / 60.0, context, &[]);
        saw_item_contact |= world.contact_pairs().contains(&[1, 2]);
    }
    assert!(
        saw_item_contact,
        "the frame diagnostic preserves actual ball/pin solver contacts"
    );
    let pin = world
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.key == 1)
        .expect("contact pin snapshot");
    assert!(
        !pin.dormant,
        "real ball impact activates the authored compound pin"
    );
    assert!(
        pin.pose.rotation.mul_vec3(Vec3::Y).dot(Vec3::Y).abs() < 0.65,
        "rolling ball topples a shaped pin through contact: {:?}",
        pin.pose
    );
    assert!(
        pin.linear_velocity.is_finite() && pin.angular_velocity.is_finite(),
        "compound contact stays finite"
    );
}

// Protects contact-material traction and Multiply friction with real contact
// motion, plus the shallow recess side missed by player-sized rim boxes.
// Three boxes, 190 fixed steps, metadata-only materials; no image decoding.
#[test]
fn material_friction_changes_sliding_and_shallow_recess_sides_are_closed() {
    let mut floor_level =
        level(r#", "floor_patches":[{"x":-3,"z":-3,"width":3,"depth":6,"material":"pack:ice"}]"#);
    floor_level.defaults.floor = "pack:carpet".into();
    let pack = PackMaterials::new(
        "physics_coefficients",
        Some(
            r#"{
        "pack:ice":{"texture":"unused.png","ground_surface":"ice"},
        "pack:carpet":{"texture":"unused.png","ground_friction":1.0}}
        "#,
        ),
        std::collections::HashMap::new(),
    );
    let table = MaterialTable::logical(
        &floor_level,
        &crate::engine::assets::AssetCatalog::builtin(),
        Some(&pack),
    );
    let mut world = ItemWorld::from_level_with_materials(&floor_level, &[], &table);
    let definition = PhysicsDef {
        friction: 0.55,
        linear_damping: 0.0,
        ..dynamic(None)
    };
    for (key, x) in [(1_u64, -1.5_f32), (2_u64, 1.5_f32)] {
        world
            .spawn(
                key,
                &definition,
                (Vec3::splat(-0.1), Vec3::splat(0.1)),
                Vec3::new(x, 0.105, -2.0),
                0.0,
                1.0,
            )
            .expect("sliding cube spawns");
    }
    let context = player(Vec3::new(2.0, 1.6, 2.0));
    advance(&mut world, context, 30);
    for key in [1_u64, 2_u64] {
        assert!(
            world.set_fixture_velocity(key, Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO),
            "controlled comparison velocity is accepted"
        );
    }
    assert!(
        !world.set_fixture_velocity(1, Vec3::new(21.0, 0.0, 0.0), Vec3::ZERO),
        "developer velocities stay bounded"
    );
    advance(&mut world, context, 30);
    let snapshots = world.snapshots();
    let ice = snapshots
        .iter()
        .find(|body| body.key == 1)
        .expect("ice cube snapshot");
    let carpet = snapshots
        .iter()
        .find(|body| body.key == 2)
        .expect("carpet cube snapshot");
    assert!(
        ice.linear_velocity.z > 2.7,
        "ice retains motion with multiplied friction: {}",
        ice.linear_velocity.z
    );
    assert!(
        carpet.linear_velocity.z < 1.0,
        "carpet slows the identical item: {}",
        carpet.linear_velocity.z
    );
    assert!(
        (ice.mass_kg - 1.0).abs() < 1.0e-5 && (ice.friction - 0.55).abs() < 1.0e-5,
        "snapshots expose actual collider coefficients"
    );

    let pit_level =
        level(r#", "floor_regions":[{"x":-1,"z":-1,"width":2,"depth":2,"offset_y":-0.25}]"#);
    let mut pit = ItemWorld::from_level(&pit_level, &[]);
    pit.spawn(
        3,
        &dynamic(None),
        (Vec3::splat(-0.015), Vec3::splat(0.015)),
        Vec3::new(0.0, -0.23, 0.0),
        0.0,
        1.0,
    )
    .expect("shallow-pit cube spawns");
    advance(&mut pit, context, 10);
    assert!(
        pit.set_fixture_velocity(3, Vec3::new(4.0, 0.0, 0.0), Vec3::ZERO),
        "pit contact comparison launches"
    );
    advance(&mut pit, context, 25);
    let blocked = pit
        .snapshots()
        .into_iter()
        .next()
        .expect("pit cube snapshot");
    assert!(
        blocked.pose.position.x < 1.02 && blocked.pose.position.y < -0.2,
        "a small body cannot pass beneath a shallow recess side: {:?}",
        blocked.pose
    );
}
