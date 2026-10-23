//! One sphere exercises dormant contact wakeup and fixed-body separation.
//! A wall-free metadata world and a handful of fixed steps cost milliseconds;
//! this specifically protects the all-dormant early-return activation gap.

use crate::engine::level::LevelDef;
use crate::engine::physics::{
    BodyMotion, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};
use glam::Vec3;

#[test]
fn player_contact_activates_dormant_movable_bodies_but_preserves_fixed_bodies() -> Result<(), String>
{
    for (motion, top) in [
        (BodyMotion::Dynamic, false),
        (BodyMotion::Fixed, false),
        (BodyMotion::Dynamic, true),
        (BodyMotion::Fixed, true),
    ] {
        let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"contact","name":"Contact","spawn":{"x":0,"z":0}}"#).map_err(|error| error.to_string())?;
        let mut world = ItemWorld::from_level(&level, &[]);
        world.spawn_authored(
            1,
            &PhysicsDef {
                motion,
                shape: Some(ShapeDef::Sphere { radius: 0.2 }),
                ..PhysicsDef::default()
            },
            (Vec3::splat(-0.2), Vec3::splat(0.2)),
            Vec3::new(0.0, 0.9, -0.7),
            0.0,
            1.0,
        )?;
        let player = PlayerContext {
            half: crate::engine::collision::PLAYER_RADIUS,
            eye: Vec3::new(0.0, 1.6, 0.0),
            feet: Vec3::ZERO,
            height: 1.8,
            view: Vec3::NEG_Z,
            yaw: 0.0,
            active: true,
        };
        world.sync_context(player, &[]);
        let desired = PlayerContext {
            eye: Vec3::new(0.0, 1.6, -0.4),
            feet: Vec3::new(0.0, 0.0, -0.4),
            ..player
        };
        let eye = if top {
            // Final landing contact at the sphere's top, with no penetration.
            Vec3::new(0.0, 2.7, -0.7)
        } else {
            world.constrain_player_eye(desired)
        };
        let contact = PlayerContext {
            eye,
            feet: Vec3::new(eye.x, eye.y - 1.6, eye.z),
            ..player
        };
        world.step(FIXED_STEP, contact, &[]);
        if motion == BodyMotion::Dynamic {
            assert!(
                !world.body_is_dormant(1),
                "a controller sweep contact must wake the first movable body"
            );
        } else {
            assert_eq!(
                world.body_motion(1),
                Some(BodyMotion::Fixed),
                "static definitions must stay fixed"
            );
        }
    }
    Ok(())
}

// Two one-sphere fixtures protect side/top contact after genuine solver sleep,
// distinct from authored dormancy. At most 600 tiny fixed steps per fixture;
// no assets, graphics or baking, and no timing threshold assertions.
#[test]
fn walking_and_stepping_contact_reach_a_genuinely_sleeping_movable_prop() -> Result<(), String> {
    for top in [false, true] {
        let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"sleep_contact","name":"Sleep contact","spawn":{"x":0,"z":0},"rooms":[{"x":-3,"z":-3,"width":6,"depth":6,"height":4}]}"#).map_err(|error| error.to_string())?;
        let mut world = ItemWorld::from_level(&level, &[]);
        world.spawn(
            1,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                shape: Some(ShapeDef::Sphere { radius: 0.2 }),
                ..PhysicsDef::default()
            },
            (Vec3::splat(-0.2), Vec3::splat(0.2)),
            Vec3::new(0.0, 0.3, 0.0),
            0.0,
            1.0,
        )?;
        let away = PlayerContext {
            half: crate::engine::collision::PLAYER_RADIUS,
            eye: Vec3::new(2.0, 1.6, 2.0),
            feet: Vec3::new(2.0, 0.0, 2.0),
            height: 1.8,
            view: Vec3::NEG_Z,
            yaw: 0.0,
            active: true,
        };
        for _ in 0_u16..600 {
            world.step(FIXED_STEP, away, &[]);
        }
        let resting = world
            .snapshots()
            .into_iter()
            .next()
            .ok_or("sleeping prop disappeared")?;
        assert!(
            resting.sleeping && !resting.dormant,
            "fixture must reach genuine solver sleep before player contact"
        );
        let feet = if top {
            Vec3::new(0.0, resting.pose.position.y + 0.2, 0.0)
        } else {
            Vec3::new(0.0, 0.0, 0.5)
        };
        let contact = PlayerContext {
            feet,
            eye: feet + Vec3::new(0.0, 1.6, 0.0),
            ..away
        };
        world.step(FIXED_STEP, contact, &[]);
        let contacted = world
            .snapshots()
            .into_iter()
            .next()
            .ok_or("contacted prop disappeared")?;
        assert!(
            !contacted.sleeping,
            "walking into or stepping atop a sleeping movable prop must wake its physics"
        );
    }
    Ok(())
}

// Value: the old physics proxy rejected short bodies and retained human width.
// One dormant cube and two contexts protect tiny-form clearance and a wide-form
// refusal independently of static-world movement; two syncs, no assets or bake.
#[test]
fn prop_body_dimensions_control_live_item_clearance_and_sweeps() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"form_proxy","name":"Form proxy","spawn":{"x":0,"z":0}}"#).map_err(|error|error.to_string())?;
    let mut world = ItemWorld::from_level(&level, &[]);
    world.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            shape: Some(ShapeDef::Cuboid {
                half_extents: [0.1, 0.5, 0.5],
            }),
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.1, 0.0, -0.5), Vec3::new(0.1, 1.0, 0.5)),
        Vec3::new(0.3, 0.0, 0.0),
        0.0,
        1.0,
    )?;
    let tiny = PlayerContext {
        eye: Vec3::new(0.0, 0.08, 0.0),
        feet: Vec3::ZERO,
        height: 0.09,
        half: 0.07,
        view: Vec3::X,
        yaw: 0.0,
        active: true,
    };
    assert!(world.player_pose_clear(tiny));
    let human = PlayerContext {
        eye: Vec3::new(0.0, 1.6, 0.0),
        height: 1.8,
        half: 0.4,
        ..tiny
    };
    assert!(!world.player_pose_clear(human));
    world.sync_context(tiny, &[]);
    let desired = PlayerContext {
        feet: Vec3::new(1.0, 0.0, 0.0),
        eye: Vec3::new(1.0, 0.08, 0.0),
        ..tiny
    };
    let resolved = world.constrain_player_eye(desired);
    assert!(
        resolved.x > 0.0 && resolved.x < 0.14,
        "tiny body must sweep to the real cube face"
    );
    Ok(())
}
