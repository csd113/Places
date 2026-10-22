//! Essential CPU regressions: displayed-pose/joint-slot continuity, stable
//! constrained contact, sleep/wake plus clock/handle retirement, atomic rig/budget
//! rejection, and exclusive AI/reset ownership. One installed 39-node rig and a single
//! analytic room; no lighting bake, GPU or shipped-map preparation. The longest
//! fixture advances fifteen bodies for thirty simulated seconds (under a second).
use super::{
    FIXED_STEP, ItemWorld, PlayerContext,
    ragdoll::{MAX_RAGDOLLS, RagdollInput},
};
use crate::engine::{gltf, level::LevelDef, render::CharacterAnimator};
use glam::{Mat4, Vec3};

fn model() -> Result<gltf::PropModel, String> {
    gltf::parse_glb(include_bytes!(
        "../../../assets/entities/mannequin/model/mannequin.glb"
    ))
    .map_err(|error| error.to_string())
}
fn globals(model: &gltf::PropModel, animator: &CharacterAnimator) -> Result<Vec<Mat4>, String> {
    model
        .skin
        .as_ref()
        .ok_or("missing skin")?
        .nodes
        .iter()
        .map(|node| {
            animator
                .node_global(&node.name)
                .ok_or_else(|| format!("missing node {}", node.name))
        })
        .collect()
}
fn player() -> PlayerContext {
    PlayerContext {
        eye: Vec3::new(-4.0, 1.6, -4.0),
        feet: Vec3::new(-4.0, 0.0, -4.0),
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: false,
    }
}
fn room() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"ragdoll","name":"Tiny ragdoll fixture","spawn":{"x":-4,"z":-4},"rooms":[{"x":-6,"z":-6,"width":12,"depth":12,"height":4}]}"#).map_err(|error| error.to_string())
}

#[test]
fn ragdoll_entry_preserves_displayed_pose_and_skin_joint_order() -> Result<(), String> {
    let asset = model()?;
    let mut animator = CharacterAnimator::new(&asset).ok_or("animator missing")?;
    assert!(
        animator.sample_fraction("walk", 0.37),
        "fixture clip samples"
    );
    let current = globals(&asset, &animator)?;
    let input = RagdollInput {
        skin: asset.skin.as_ref().ok_or("skin missing")?,
        globals: &current,
        previous: &current,
        placement: Mat4::from_rotation_y(0.7),
        previous_placement: Mat4::from_translation(Vec3::new(-0.0002, 0.0, 0.0))
            .mul_mat4(&Mat4::from_rotation_y(0.7)),
        delta: 0.0002,
    };
    let mut world = ItemWorld::default();
    world.start_ragdoll("walker", &input, Vec3::ZERO)?;
    let entry = world
        .ragdoll_snapshots()
        .pop()
        .ok_or("entry ragdoll missing")?;
    assert!(
        (entry.max_speed_mps - 1.0).abs() < 0.005,
        "a sub-millisecond rendered interval must still inherit actual actor velocity"
    );
    let mut output = vec![Mat4::IDENTITY; current.len()];
    let _changed = world.write_ragdoll_pose("walker", &mut output);
    for (before, after) in current.iter().zip(&output) {
        assert!(
            before.abs_diff_eq(*after, 0.00001),
            "handoff must retain every bone and finger transform"
        );
    }
    let runtime = world.runtime.as_ref().ok_or("runtime missing")?;
    assert_eq!(
        runtime.world.impulse_joints.len(),
        14,
        "fifteen segments must be connected by fourteen real joints"
    );
    for (_, joint) in runtime.world.impulse_joints() {
        assert_eq!(
            joint.data.locked_axes,
            rapier3d::prelude::JointAxesMask::LOCKED_SPHERICAL_AXES,
            "translations must be constrained"
        );
        assert_eq!(
            joint.data.limit_axes,
            rapier3d::prelude::JointAxesMask::ANG_X
                | rapier3d::prelude::JointAxesMask::ANG_Y
                | rapier3d::prelude::JointAxesMask::ANG_Z,
            "all angular axes require finite limits"
        );
    }
    Ok(())
}

#[test]
fn ragdoll_contacts_settle_pause_step_and_remove_every_solver_handle() -> Result<(), String> {
    let asset = model()?;
    let animator = CharacterAnimator::new(&asset).ok_or("animator missing")?;
    let current = globals(&asset, &animator)?;
    let level = room()?;
    let mut world = ItemWorld::from_level(&level, &level.collision_aabbs());
    let placement = Mat4::from_translation(Vec3::new(0.0, 0.2, 0.0));
    let input = RagdollInput {
        skin: asset.skin.as_ref().ok_or("skin missing")?,
        globals: &current,
        previous: &current,
        placement,
        previous_placement: placement,
        delta: FIXED_STEP,
    };
    world.start_ragdoll("fall", &input, Vec3::new(1.5, 0.0, 0.5))?;
    for _ in 0_u16..1200 {
        world.step(FIXED_STEP, player(), &[]);
        for snapshot in world.ragdoll_snapshots() {
            assert!(
                snapshot.max_anchor_error_m < 0.035,
                "constrained segments must not detach: {}",
                snapshot.max_anchor_error_m
            );
            assert!(snapshot.max_speed_mps < 20.0, "solver must remain bounded");
        }
    }
    let snapshot = world.ragdoll_snapshots().pop().ok_or("ragdoll missing")?;
    assert_eq!(
        snapshot.sleeping, 15,
        "the articulated island must sleep naturally after contact settles: {} m/s, {} m penetration",
        snapshot.max_speed_mps, snapshot.max_penetration_m
    );
    assert!(
        snapshot.max_penetration_m < 0.02,
        "resting core penetration remains within two centimetres"
    );
    let pelvis = Vec3::from_array(*snapshot.world_positions.first().ok_or("pelvis missing")?);
    // A sleeping pose must not invalidate lighting and GPU skinning every frame.
    // Reuses this already-settled fixture, adding only two pose publications.
    let mut published = vec![Mat4::IDENTITY; current.len()];
    assert!(world.write_ragdoll_pose("fall", &mut published));
    assert!(!world.write_ragdoll_pose("fall", &mut published));
    wake_with_item(&mut world, pelvis)?;
    assert!(
        world.write_ragdoll_pose("fall", &mut published),
        "contact wake must publish the changed pose"
    );
    world.pause();
    let frozen = world
        .ragdoll_snapshots()
        .pop()
        .ok_or("ragdoll missing")?
        .world_positions;
    world.step(0.1, player(), &[]);
    assert_eq!(
        world
            .ragdoll_snapshots()
            .pop()
            .ok_or("ragdoll missing")?
            .world_positions,
        frozen,
        "pause cannot advance bones"
    );
    let previous_steps = world.simulation_state().fixed_steps;
    world.step_once(player(), &[])?;
    assert_eq!(
        world.simulation_state().fixed_steps,
        previous_steps.saturating_add(1),
        "explicit step advances one tick"
    );
    let runtime = world.runtime.as_ref().ok_or("runtime missing")?;
    let before_bodies = runtime.world.bodies.len();
    let before_colliders = runtime.world.colliders.len();
    assert!(world.remove_ragdoll("fall"), "active ragdoll removes");
    let retired_runtime = world.runtime.as_ref().ok_or("runtime missing")?;
    assert_eq!(
        retired_runtime.world.bodies.len(),
        before_bodies.saturating_sub(15),
        "all bone bodies retire"
    );
    assert_eq!(
        retired_runtime.world.colliders.len(),
        before_colliders.saturating_sub(15),
        "all bone colliders retire"
    );
    assert!(
        retired_runtime.world.impulse_joints.is_empty() && retired_runtime.ragdolls.is_empty(),
        "all joints and pose caches retire"
    );
    assert!(
        !world.remove_ragdoll("fall"),
        "repeated removal is harmless"
    );
    Ok(())
}

// A small ordinary item impact must wake a sleeping island: this protects
// interactive physics beyond merely accepting an initially active ragdoll.
fn wake_with_item(world: &mut ItemWorld, pelvis: Vec3) -> Result<(), String> {
    world.spawn(
        999,
        &super::PhysicsDef {
            motion: super::BodyMotion::Dynamic,
            mass_kg: 2.0,
            ..super::PhysicsDef::default()
        },
        (Vec3::splat(-0.1), Vec3::splat(0.1)),
        super::collision::add(pelvis, Vec3::new(0.0, 0.7, 0.0)),
        0.0,
        1.0,
    )?;
    assert!(world.set_fixture_velocity(999, Vec3::new(0.0, -2.0, 0.0), Vec3::ZERO));
    let mut woke = false;
    for _ in 0_u8..90 {
        world.step(FIXED_STEP, player(), &[]);
        woke |= world
            .ragdoll_snapshots()
            .iter()
            .any(|state| state.sleeping < 15);
    }
    assert!(woke, "ordinary item contact must wake the sleeping island");
    Ok(())
}

#[test]
fn ragdoll_unsupported_rigs_and_shared_budget_fail_without_partial_ownership() -> Result<(), String>
{
    let asset = model()?;
    let animator = CharacterAnimator::new(&asset).ok_or("animator missing")?;
    let current = globals(&asset, &animator)?;
    let skin = asset.skin.as_ref().ok_or("skin missing")?;
    let mut unsupported = skin.clone();
    unsupported.joints.clear();
    let mut input = RagdollInput {
        skin: &unsupported,
        globals: &current,
        previous: &current,
        placement: Mat4::IDENTITY,
        previous_placement: Mat4::IDENTITY,
        delta: FIXED_STEP,
    };
    let mut world = ItemWorld::default();
    assert!(
        world.start_ragdoll("bad", &input, Vec3::ZERO).is_err() && world.runtime.is_none(),
        "invalid mapping must allocate nothing"
    );
    input.skin = skin;
    input.placement = Mat4::from_scale(Vec3::splat(0.25));
    input.previous_placement = input.placement;
    for index in 0..MAX_RAGDOLLS {
        world.start_ragdoll(&format!("actor{index}"), &input, Vec3::ZERO)?;
    }
    world.alias_ragdoll_contacts("actor0", 999);
    let runtime = world.runtime.as_ref().ok_or("runtime missing")?;
    let total_mass: f32 = runtime
        .world
        .bodies
        .iter()
        .filter(|(_, body)| body.is_dynamic())
        .map(|(_, body)| body.mass())
        .sum();
    assert!(
        (total_mass - 10.625).abs() < 0.001,
        "ten quarter-scale skeletons must retain density, measured {total_mass} kg"
    );
    assert_eq!(
        runtime.contact_aliases.len(),
        15,
        "all converted bone contacts retain their source pin identity"
    );
    let counts = (
        runtime.world.bodies.len(),
        runtime.world.colliders.len(),
        runtime.world.impulse_joints.len(),
    );
    assert!(
        world.start_ragdoll("overflow", &input, Vec3::ZERO).is_err(),
        "eleventh ragdoll must be refused"
    );
    assert!(
        world.start_ragdoll("actor0", &input, Vec3::ZERO).is_err(),
        "duplicate activation must be refused"
    );
    let refused_runtime = world.runtime.as_ref().ok_or("runtime missing")?;
    assert_eq!(
        (
            refused_runtime.world.bodies.len(),
            refused_runtime.world.colliders.len(),
            refused_runtime.world.impulse_joints.len()
        ),
        counts,
        "refusals cannot leave partial bodies/joints"
    );
    fill_item_budget(&mut world)?;
    assert!(world.remove_ragdoll("actor0"));
    assert!(
        world
            .runtime
            .as_ref()
            .ok_or("runtime missing")?
            .contact_aliases
            .is_empty(),
        "reset must retire every pin contact alias"
    );
    Ok(())
}

#[test]
fn ragdoll_retires_ai_and_reset_restores_authored_controller() -> Result<(), String> {
    let mut level = room()?;
    level.props.push(serde_json::from_value(serde_json::json!({"id":"actor","model":"mannequin","x":1_i32,"z":2_i32,"solid":false,
        "components":[{"component":"interactable","prompt":"Ragdoll"},{"component":"nav_agent","radius":0.2_f32,"speed_mps":1_i32,"height":1.8_f32},{"component":"ai","behavior":"wanderer"}]})).map_err(|error| error.to_string())?);
    let mut world = crate::engine::entities::EntityWorld::from_level(&level);
    assert!(
        world.ai().agent("actor").is_some(),
        "authored controller exists"
    );
    let aim_index = world
        .interactables()
        .index_of("actor")
        .ok_or("aim target missing")?;
    let original_size = world
        .interactables()
        .get(aim_index)
        .ok_or("aim target missing")?
        .size;
    world.set_ragdoll_owned("actor", true);
    world.publish_ragdoll(
        "actor",
        Vec3::new(3.0, 0.2, 2.0),
        crate::engine::spatial::Aabb {
            min: [2.0, 0.0, 1.0],
            max: [4.0, 0.4, 3.0],
        },
    );

    assert!(
        world.ai().agent("actor").is_none(),
        "no invisible AI controller may remain active"
    );
    world.set_ragdoll_owned("actor", false);
    let reset_index = world
        .interactables()
        .index_of("actor")
        .ok_or("reset aim target missing")?;
    assert_eq!(
        world
            .interactables()
            .get(reset_index)
            .ok_or("reset aim target missing")?
            .size,
        original_size,
        "authored upright aim size must replace the physical envelope after reset"
    );
    let actor = world
        .ai()
        .agent("actor")
        .ok_or("authored controller not restored")?;
    assert!(
        actor
            .position
            .abs_diff_eq(Vec3::new(1.0, 0.0, 2.0), 0.00001),
        "reset must restore authored location"
    );
    Ok(())
}

/// A straight-leg stand entry falling from one table edge used to chatter
/// indefinitely with hard joint corrections. This single box/15-body fixture
/// adds distinct contact-stability coverage beyond the flat-floor fall; ten
/// simulated seconds cost well under one second and require no map preparation.
#[test]
fn ragdoll_stand_pose_table_contact_dissipates_constraint_chatter() -> Result<(), String> {
    let asset = model()?;
    let mut animator = CharacterAnimator::new(&asset).ok_or("animator missing")?;
    let _changed = animator.sample_fraction("pose_stand", 0.0);
    let current = globals(&asset, &animator)?;
    let mut level = room()?;
    level.props.push(serde_json::from_str(r#"{"id":"table","model":"home:coffee_table","x":4,"z":1,"size":[1.4,0.46,0.8],"solid":true}"#).map_err(|error| error.to_string())?);
    let mut world = ItemWorld::from_level(&level, &level.collision_aabbs());
    let placement = Mat4::from_translation(Vec3::new(4.0, 0.46, 1.0));
    let input = RagdollInput {
        skin: asset.skin.as_ref().ok_or("skin missing")?,
        globals: &current,
        previous: &current,
        placement,
        previous_placement: placement,
        delta: FIXED_STEP,
    };
    world.start_ragdoll("table", &input, Vec3::new(0.8, 0.0, -1.8))?;
    for _ in 0_u16..1200 {
        world.step(FIXED_STEP, player(), &[]);
    }
    let snapshot = world
        .ragdoll_snapshots()
        .pop()
        .ok_or("missing table ragdoll")?;
    assert_eq!(
        snapshot.sleeping, 15,
        "table contact must dissipate corrections and sleep the complete articulated island"
    );
    assert!(
        snapshot.max_anchor_error_m < 0.006,
        "damped joint compliance must stay within six millimetres"
    );
    assert!(
        snapshot.max_penetration_m < 0.01,
        "table contact must retain real collision separation"
    );
    Ok(())
}

/// Raised arms landing beside a wall formerly kept a small hand rotating and
/// prevented island sleep. This analytic room/15-body regression adds distinct
/// angular-contact coverage, costs under one second, and never bakes a map.
#[test]
fn ragdoll_raised_arms_wall_contact_reaches_sleep() -> Result<(), String> {
    let asset = model()?;
    let mut animator = CharacterAnimator::new(&asset).ok_or("animator missing")?;
    assert!(animator.sample_fraction("pose_arms_up", 0.0));
    let current = globals(&asset, &animator)?;
    let mut level = room()?;
    level.walls.push(
        serde_json::from_str(r#"{"x":-6,"z":-6.2,"width":12,"depth":0.2}"#)
            .map_err(|error| error.to_string())?,
    );
    let mut world = ItemWorld::from_level(&level, &level.collision_aabbs());
    let placement = Mat4::from_translation(Vec3::new(0.0, 0.0, -4.9));
    let input = RagdollInput {
        skin: asset.skin.as_ref().ok_or("skin missing")?,
        globals: &current,
        previous: &current,
        placement,
        previous_placement: placement,
        delta: FIXED_STEP,
    };
    world.start_ragdoll("wall", &input, Vec3::new(0.0, 0.0, -3.0))?;
    for _ in 0_u16..3600 {
        world.step(FIXED_STEP, player(), &[]);
    }
    let snapshot = world
        .ragdoll_snapshots()
        .pop()
        .ok_or("missing wall ragdoll")?;
    assert_eq!(
        snapshot.sleeping, 15,
        "raised-arm wall contact must sleep: {} rad/s, {} m/s, {} quiet seconds",
        snapshot.max_angular_speed_rps, snapshot.max_speed_mps, snapshot.min_quiet_seconds
    );
    assert!(snapshot.max_anchor_error_m < 0.006);
    assert!(snapshot.max_penetration_m < 0.01);
    Ok(())
}

fn fill_item_budget(world: &mut ItemWorld) -> Result<(), String> {
    for key in 0_u64..128 {
        world.spawn(
            key,
            &super::PhysicsDef {
                motion: super::BodyMotion::Dynamic,
                ..super::PhysicsDef::default()
            },
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            Vec3::ZERO,
            0.0,
            1.0,
        )?;
    }
    assert!(
        world
            .spawn(
                129,
                &super::PhysicsDef {
                    motion: super::BodyMotion::Dynamic,
                    ..super::PhysicsDef::default()
                },
                (Vec3::splat(-0.1), Vec3::splat(0.1)),
                Vec3::ZERO,
                0.0,
                1.0
            )
            .is_err(),
        "ordinary item allocation remains bounded while ten articulated rigs have their own bounded reservation"
    );
    Ok(())
}

// Protects the native ten-miniature pile that retained contact chatter after
// twenty-two seconds. One analytic floor and the existing rig, no map/GPU/bake.
// 150 bodies for thirty simulated seconds is the narrow supported rack workload.
#[test]
fn miniature_rack_contacts_settle_without_forcing_sleep() -> Result<(), String> {
    let asset = model()?;
    let animator = CharacterAnimator::new(&asset).ok_or("animator missing")?;
    let current = globals(&asset, &animator)?;
    let level = room()?;
    let mut world = ItemWorld::from_level(&level, &level.collision_aabbs());
    for row in 0_u8..4 {
        for column in 0_u8..=row {
            let position = Vec3::new(
                f32::from(row).mul_add(-0.5, f32::from(column)) * 0.21,
                0.0,
                -f32::from(row) * 0.19,
            );
            let placement = Mat4::from_scale_rotation_translation(
                Vec3::splat(0.25),
                glam::Quat::IDENTITY,
                position,
            );
            let input = RagdollInput {
                skin: asset.skin.as_ref().ok_or("skin missing")?,
                globals: &current,
                previous: &current,
                placement,
                previous_placement: placement,
                delta: FIXED_STEP,
            };
            world.start_ragdoll(
                &format!("mini-{row}-{column}"),
                &input,
                Vec3::new(0.0, 0.0, -0.5),
            )?;
        }
    }
    for _ in 0_u16..3600 {
        world.step(FIXED_STEP, player(), &[]);
    }
    let snapshots = world.ragdoll_snapshots();
    assert_eq!(snapshots.len(), 10);
    for state in snapshots {
        assert_eq!(
            state.sleeping, 15,
            "{} must settle naturally: {} m/s / {} rad/s",
            state.id, state.max_speed_mps, state.max_angular_speed_rps
        );
        assert!(
            state.max_anchor_error_m < 0.004,
            "miniature joints cannot detach"
        );
        assert!(
            state.max_penetration_m < 0.004,
            "miniature contacts retain separation"
        );
    }
    Ok(())
}
