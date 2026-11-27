//! CPU-only collision regressions. A few fixed steps per shot distinguish
//! intentional actor barriers from real item contacts without assets or a bake.

use glam::{Vec2, Vec3};

use crate::engine::collision::{
    DoorCollider, HorizontalWorld, PLAYER_RADIUS, WallAabb, disc_segment_blocked, sweep_player_hull,
};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::level::{LevelDef, LevelSurfaces, construction::rail_runs};
use crate::engine::package::collision::{CompiledCollision, read_collision, write_collision};
use crate::engine::physics::{
    BodyMotion, FIXED_STEP, GRAVITY, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};

const LEGACY_SEMANTICS: &str = r#"{
    "schema":"places-level","format_version":1,"id":"rail_contacts","name":"Rail contacts",
    "spawn":{"x":-1,"z":-2},
    "rooms":[{"x":-4,"z":-4,"width":12,"depth":12,"height":4}],
    "walls":[{"x":3,"z":-0.05,"width":1.2,"depth":0.1,"height":2.5}],
    "guardrails":[{"x":0,"z":0,"length":2.4,"y":0,"rise":0}]
}"#;

const PATH_SEMANTICS: &str = r#"{
    "schema":"places-level","format_version":1,"id":"rail_join","name":"Rail join",
    "spawn":{"x":-1,"z":-2},
    "rooms":[{"x":-4,"z":-4,"width":12,"depth":12,"height":4}],
    "guardrails":[{"x":0,"z":0,"length":1,"y":0,"rise":0.3,
                    "path":[[1.6970563,1.6970563],[3.3941126,1.6970563]]}]
}"#;

fn shoot(
    level: &LevelDef,
    walls: &[WallAabb],
    doors: &[DoorCollider],
    crossing: Vec3,
    direction: Vec3,
    radius: f32,
    speed: f32,
) -> Result<f32, String> {
    let feet = Vec3::new(-1.0, 0.0, -2.0);
    let player = PlayerContext {
        eye: Vec3::new(feet.x, 1.8, feet.z),
        feet,
        height: 1.8,
        half: PLAYER_RADIUS,
        view: Vec3::Z,
        yaw: 0.0,
        active: true,
    };
    let profile = PhysicsDef {
        motion: BodyMotion::Dynamic,
        shape: Some(ShapeDef::Sphere { radius }),
        linear_damping: 0.0,
        angular_damping: 0.0,
        ..PhysicsDef::default()
    };
    let mut world = ItemWorld::from_level(level, walls);
    let distance = 0.52;
    let flight = distance / speed;
    let compensated = Vec3::new(
        crossing.x,
        (0.5 * GRAVITY * flight).mul_add(flight, crossing.y),
        crossing.z,
    );
    let origin = direction.mul_add(Vec3::splat(-distance), compensated);
    world.spawn(
        1,
        &profile,
        (Vec3::splat(-radius), Vec3::splat(radius)),
        origin,
        0.0,
        1.0,
    )?;
    assert!(world.set_fixture_velocity(
        1,
        direction.mul_add(Vec3::splat(speed), Vec3::ZERO),
        Vec3::ZERO
    ));
    let mut furthest = -distance;
    for _ in 0_u8..20_u8 {
        world.step(FIXED_STEP, player, doors);
        let (pose, _) = world
            .body_pose_velocity(1)
            .ok_or("test projectile went missing")?;
        let delta = Vec3::new(
            pose.position.x - crossing.x,
            pose.position.y - crossing.y,
            pose.position.z - crossing.z,
        );
        furthest = furthest.max(delta.dot(direction));
    }
    Ok(furthest)
}

fn assert_actor_barrier(compiled: &CompiledCollision) {
    let walls = &compiled.walls;
    let from = Vec2::new(0.6, -0.6);
    let to = Vec2::new(0.6, 0.6);
    let index = CollisionIndex::build(walls);
    let controller = HorizontalWorld {
        index: &index,
        walls,
        doors: &[],
        ceiling: &compiled.ceiling,
        floor: &compiled.floor,
        debug: false,
    };
    for height in [1.2, 1.8] {
        assert!(
            sweep_player_hull(&controller, (from, to), (0.0, height, PLAYER_RADIUS), true).y < 0.0
        );
    }
    assert!(disc_segment_blocked(
        from,
        to,
        PLAYER_RADIUS,
        0.0,
        1.2,
        0.3,
        &index,
        walls
    ));
}

#[test]
fn guardrail_items_open_gap_preserves_actor_and_package_barriers() -> Result<(), String> {
    // The unchanged v1 semantics require no new fields. The packaged controller
    // walls round-trip unchanged while ItemWorld consumes the same semantics.
    let level = crate::engine::package::semantics::parse(LEGACY_SEMANTICS)?;
    let source_walls = level.collision_aabbs();
    let bytes = write_collision(&CompiledCollision::from_level(&level))?;
    assert!(bytes.starts_with(b"PCL1\x01\x00"));
    let compiled = read_collision(&bytes)?;
    let walls = &compiled.walls;
    assert_eq!(*walls, source_walls);
    assert_eq!(
        walls.len(),
        2,
        "v1 retains the ordinary wall and the full rail barrier"
    );
    assert!(walls.iter().any(|wall| wall.min_x.abs() < 1e-5
        && (wall.max_x - 2.4).abs() < 1e-5
        && (wall.min_y + 0.2).abs() < 1e-5
        && (wall.max_y - 1.0).abs() < 1e-5));
    let mut without_rails = level.clone();
    without_rails.guardrails.clear();
    assert_eq!(
        level.item_collision_aabbs_from(walls),
        without_rails.collision_aabbs()
    );

    assert_actor_barrier(&compiled);
    // The untagged wall list alone reproduces the old opaque Rapier barrier.
    assert!(
        shoot(
            &without_rails,
            walls,
            &[],
            Vec3::new(0.6, 0.64, 0.0),
            Vec3::Z,
            0.05,
            8.0
        )? < 0.03
    );

    for (label, x, y, radius, speed, passes) in [
        ("gap", 0.6, 0.64, 0.05, 8.0, true),
        ("post", 1.2, 0.64, 0.05, 8.0, false),
        ("top", 0.6, 0.98, 0.05, 8.0, false),
        ("lower rail CCD", 0.6, 0.315, 0.02, 20.0, false),
        ("oversized", 0.6, 0.64, 0.35, 8.0, false),
        ("ordinary wall", 3.6, 0.64, 0.05, 8.0, false),
    ] {
        let distance = shoot(
            &level,
            walls,
            &[],
            Vec3::new(x, y, 0.0),
            Vec3::Z,
            radius,
            speed,
        )?;
        if passes {
            assert!(
                distance > 0.3,
                "{label}: projectile did not cross, {distance}"
            );
        } else {
            assert!(distance < 0.03, "{label}: projectile crossed, {distance}");
        }
    }
    assert!(
        shoot(
            &level,
            &source_walls,
            &[],
            Vec3::new(0.6, 0.64, 0.0),
            Vec3::Z,
            0.05,
            8.0
        )? > 0.3
    );
    let door = DoorCollider::from_pose([5.0, 0.0, 0.0], [1.0, 0.0], 1.2, 0.075, 2.0);
    assert!(
        shoot(
            &level,
            walls,
            &[door],
            Vec3::new(5.6, 0.64, 0.0),
            Vec3::Z,
            0.05,
            8.0
        )? < 0.03
    );
    assert!(
        shoot(
            &level,
            walls,
            &[],
            Vec3::new(5.6, 0.64, 0.0),
            Vec3::Z,
            0.05,
            8.0
        )? > 0.3
    );
    Ok(())
}

// A compiled-only wall can legitimately outlive its source scenery. Exercise
// the decoded PCL1 wall list with one tiny launch on either side of this split.
#[test]
fn guardrail_items_keep_compiled_only_blockers() -> Result<(), String> {
    let level = crate::engine::package::semantics::parse(LEGACY_SEMANTICS)?;
    let mut compiled = CompiledCollision::from_level(&level);
    compiled
        .walls
        .push(WallAabb::with_y(6.0, 0.0, -0.05, 1.2, 2.5, 0.1));
    let walls = read_collision(&write_collision(&compiled)?)?.walls;
    let gap = shoot(
        &level,
        &walls,
        &[],
        Vec3::new(0.6, 0.64, 0.0),
        Vec3::Z,
        0.05,
        8.0,
    )?;
    assert!(
        gap > 0.3,
        "the supplied synthetic curtain should be removed"
    );
    let extra = shoot(
        &level,
        &walls,
        &[],
        Vec3::new(6.6, 0.64, 0.0),
        Vec3::Z,
        0.05,
        8.0,
    )?;
    assert!(
        extra < 0.03,
        "the compiled-only nonrail wall vanished: {extra}"
    );
    let mut world = ItemWorld::from_level(&level, &walls);
    let profile = PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        shape: Some(ShapeDef::Sphere { radius: 0.05 }),
        ..PhysicsDef::default()
    };
    for (key, x, visible) in [(1_u64, 0.6, Some(1_u64)), (2_u64, 6.6, None)] {
        world.spawn(
            key,
            &profile,
            (Vec3::splat(-0.05), Vec3::splat(0.05)),
            Vec3::new(x, 0.64, 0.4),
            0.0,
            1.0,
        )?;
        assert_eq!(
            world.nearest_pickup(Vec3::new(x, 0.64, -0.4), Vec3::Z),
            visible
        );
    }
    Ok(())
}

// Besides the dynamic ray case, protect the identity of coincident and
// already-filtered records when a fixed prop also replaces a legacy box.
#[test]
fn guardrail_items_filter_only_known_box_occurrences() -> Result<(), String> {
    let mut level = crate::engine::package::semantics::parse(LEGACY_SEMANTICS)?;
    let ordinary = level.item_collision_aabbs_from(&level.collision_aabbs());
    let curtain = level
        .collision_aabbs()
        .into_iter()
        .find(|wall| !ordinary.contains(wall))
        .ok_or("missing synthetic curtain")?;
    level.props.push(
        serde_json::from_str(
            r#"{"model":"core:crate","x":5,"z":2,"solid":true,"size":[1.0,1.0,1.0],
            "physics":{"motion":"fixed","shape":{"kind":"sphere","radius":0.2}}}"#,
        )
        .map_err(|error| error.to_string())?,
    );
    let mut supplied = level.collision_aabbs();
    let legacy = WallAabb {
        step_up: 0.1,
        ..curtain
    };
    supplied.push(curtain);
    supplied.push(legacy);
    let mut expected = ordinary.clone();
    expected.extend([curtain, legacy]);
    assert_eq!(level.item_collision_aabbs_from(&supplied), expected);
    assert_eq!(level.item_collision_aabbs_from(&ordinary), ordinary);
    Ok(())
}

#[test]
fn guardrail_items_follow_rotated_slope_and_mitered_join() -> Result<(), String> {
    let level = crate::engine::package::semantics::parse(PATH_SEMANTICS)?;
    let walls = level.collision_aabbs();
    let surfaces = LevelSurfaces::new(&level);
    let rail = level.guardrails.first().ok_or("missing path rail")?;
    let runs = rail_runs(rail, &level);
    let run = runs.first().ok_or("missing first run")?;
    let (across_x, across_z) = run.across();
    let toward_rail = -Vec3::new(across_x, 0.0, across_z);
    let crossing = |along, height| {
        let t = along / run.length;
        let (x, z) = run.point_at(t, 0.0);
        Vec3::new(x, run.base_y_at(&surfaces, t) + height, z)
    };
    let gap = shoot(
        &level,
        &walls,
        &[],
        crossing(0.6, 0.64),
        toward_rail,
        0.05,
        8.0,
    )?;
    assert!(gap > 0.3, "rotated and sloped bay blocked at {gap}");
    let post = shoot(
        &level,
        &walls,
        &[],
        crossing(1.2, 0.64),
        toward_rail,
        0.05,
        8.0,
    )?;
    assert!(post < 0.03, "rotated post leaked at {post}");

    let (x, z) = run.point_at(1.0, 0.0);
    let next = runs.get(1).ok_or("missing second run")?;
    let (nx, nz) = next.across();
    let inward = -Vec3::new(across_x + nx, 0.0, across_z + nz).normalize();
    let joint = Vec3::new(x, run.base_y_at(&surfaces, 1.0) + 0.98, z);
    let top = shoot(&level, &walls, &[], joint, inward, 0.05, 8.0)?;
    assert!(top < 0.03, "mitered top joint leaked at {top}");
    Ok(())
}
