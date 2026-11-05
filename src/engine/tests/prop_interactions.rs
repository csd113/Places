//! The audit's two-cube push exposed control-rate dependent impulses. This
//! table adds the missing interaction coverage (including an enabled player),
//! using twelve two-second CPU trajectories and no bake, assets or graphics.

use glam::{Quat, Vec3};

use crate::engine::level::LevelDef;
use crate::engine::physics::{
    BodyMotion, BodySnapshot, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext, ShapeDef,
};

fn player(active: bool) -> PlayerContext {
    PlayerContext {
        eye: Vec3::new(-2.0, 0.15, 0.0),
        feet: Vec3::new(-2.0, 0.0, 0.0),
        height: 1.8,
        half: 0.2,
        view: Vec3::X,
        yaw: -std::f32::consts::FRAC_PI_2,
        active,
    }
}

fn world() -> Result<ItemWorld, String> {
    let level = LevelDef::from_json(include_str!(
        "../../../tests/fixtures/native/item-physics.level.json"
    ))
    .map_err(|error| error.to_string())?;
    Ok(ItemWorld::from_level(&level, &[]))
}

fn spawn(
    world: &mut ItemWorld,
    key: u64,
    mass: f32,
    radius: f32,
    position: Vec3,
) -> Result<(), String> {
    world.spawn(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            mass_kg: mass,
            shape: Some(ShapeDef::Cuboid {
                half_extents: [radius; 3],
            }),
            friction: 0.6,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            ..PhysicsDef::default()
        },
        (Vec3::splat(-radius), Vec3::splat(radius)),
        position,
        0.0,
        1.0,
    )
}

fn body(world: &ItemWorld, key: u64) -> Result<BodySnapshot, String> {
    world
        .snapshots()
        .into_iter()
        .find(|body| body.key == key)
        .ok_or_else(|| format!("missing body {key}"))
}

#[test]
fn carry_push_has_one_physical_trajectory_at_all_control_rates() -> Result<(), String> {
    for active in [false, true] {
        for rate in [30_u16, 60, 120, 144, 240, 480] {
            let mut world = world()?;
            let mut context = player(active);
            spawn(&mut world, 1, 1.0, 0.1, Vec3::new(-1.15, 0.15, 0.0))?;
            spawn(&mut world, 2, 1.0, 0.1, Vec3::new(-0.7, 0.101, 0.0))?;
            world.sync_context(context, &[]);
            assert!(
                world.pickup(1, context.eye, context.view, context.yaw),
                "fixture must pick up its front cube"
            );
            let mut peak_speed = 0.0_f32;
            let mut solved_contact = false;
            for frame in 1..=rate * 2 {
                context.eye.x = f32::from(frame) / f32::from(rate) - 2.0;
                context.feet.x = context.eye.x;
                // The game may publish interaction queries before stepping.
                world.sync_context(context, &[]);
                world.step(1.0 / f32::from(rate), context, &[]);
                peak_speed = peak_speed.max(body(&world, 2)?.linear_velocity.length());
                solved_contact |= world
                    .contact_impulses()
                    .iter()
                    .any(|contact| contact.pair == [1, 2] && contact.impulse > 0.0);
            }
            assert!(
                solved_contact,
                "{rate} Hz active={active}: push needs an actual solved pair"
            );
            assert!(
                (body(&world, 2)?.pose.position.x - 1.054).abs() < 0.003,
                "{rate} Hz active={active}: target must follow the same push ({:?})",
                body(&world, 2)?.pose.position
            );
            assert!(
                (body(&world, 1)?.pose.position.x - 0.85).abs() < 0.003 && world.held() == Some(1),
                "{rate} Hz active={active}: ownership and hand tracking must survive contact"
            );
            assert!(
                peak_speed > 0.9 && peak_speed < 1.1,
                "{rate} Hz active={active}: one metre/second carry must not multiply its impulse ({peak_speed})"
            );
            assert_eq!(
                world.simulation_state().fixed_steps,
                240,
                "every rate advances exactly two physical seconds"
            );
            assert!(
                world.take_recoveries().is_empty(),
                "ordinary push must not use escape recovery"
            );
        }
    }
    Ok(())
}

// A near-zero positive scale has long gaps between solver ticks. One cube and
// one real second protect responsive swept control plus clean tick/release
// history, independently of the existing explicit pause/zero-scale tests.
#[test]
fn slow_world_carry_stays_responsive_between_physical_ticks() -> Result<(), String> {
    let mut world = ItemWorld::default();
    let mut context = PlayerContext {
        eye: Vec3::new(-2.0, 2.0, 0.0),
        ..player(true)
    };
    spawn(&mut world, 1, 1.0, 0.1, Vec3::new(-1.15, 2.0, 0.0))?;
    world.sync_context(context, &[]);
    assert!(
        world.pickup(1, context.eye, context.view, context.yaw),
        "slow fixture must start in the hand"
    );
    world.set_time_scale(0.01)?;
    for frame in 1_u16..=240 {
        context.eye.x = f32::from(frame) / 240.0 - 2.0;
        context.feet.x = context.eye.x;
        world.sync_context(context, &[]);
        world.step(1.0 / 240.0, context, &[]);
        let carried_x = body(&world, 1)?.pose.position.x;
        assert!(
            (carried_x - context.eye.x - 0.85).abs() < 0.003,
            "frame {frame}: slow carrying must respond on control frames, including the solver tick (body {carried_x}, eye {})",
            context.eye.x
        );
    }
    assert_eq!(
        world.simulation_state().fixed_steps,
        1,
        "slow scale must preserve the scaled world clock"
    );
    assert!(
        world.drop(),
        "the swept preview must remain a valid release pose"
    );
    assert!(
        (body(&world, 1)?.linear_velocity.x - 1.0).abs() < 0.001,
        "release inherits real controller speed, without the slow solver's synthetic velocity"
    );
    Ok(())
}

// Two single-tick fixtures protect the new angular jam guard: a free prop
// must still receive a rotational push; the same prop against a wall must
// retain core clearance. Existing scenery-only carry sweeps cannot cover it.
#[test]
fn angular_carry_pushes_free_props_but_stops_at_a_supported_jam() -> Result<(), String> {
    for supported in [false, true] {
        let mut world = world()?;
        let mut context = player(false);
        context.eye = Vec3::new(-0.85, 0.2, 0.0);
        context.feet = Vec3::new(-0.85, 0.0, 0.0);
        world.spawn(
            1,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                pickup: true,
                mass_kg: 1.0,
                shape: Some(ShapeDef::Cuboid {
                    half_extents: [0.3, 0.05, 0.05],
                }),
                restitution: 0.0,
                ..PhysicsDef::default()
            },
            (Vec3::new(-0.3, -0.05, -0.05), Vec3::new(0.3, 0.05, 0.05)),
            Vec3::new(0.0, 0.2, 0.0),
            0.0,
            1.0,
        )?;
        spawn(&mut world, 2, 1.0, 0.05, Vec3::new(0.25, 0.2, 0.105))?;
        if supported {
            world.install_fixed_shape(
                &PhysicsDef {
                    shape: Some(ShapeDef::Cuboid {
                        half_extents: [0.5, 0.5, 0.05],
                    }),
                    ..PhysicsDef::default()
                },
                (Vec3::new(-0.5, -0.5, -0.05), Vec3::new(0.5, 0.5, 0.05)),
                Vec3::new(0.25, 0.5, 0.205),
                0.0,
                1.0,
            )?;
        }
        world.sync_context(context, &[]);
        assert!(
            world.pickup(1, context.eye, context.view, context.yaw),
            "angular fixture must pick up its long prop"
        );
        context.yaw += 0.5;
        world.step(FIXED_STEP, context, &[]);
        let rotation = body(&world, 1)?.pose.rotation.to_scaled_axis().length();
        if supported {
            assert!(
                rotation < 0.05,
                "supported rotation must stop at its contact skins ({rotation})"
            );
            assert!(
                world
                    .body_penetration_depth(2)
                    .is_some_and(|depth| depth <= 0.006),
                "angular jam must retain core clearance"
            );
        } else {
            assert!(
                rotation > 0.05,
                "free dynamic contact must retain the ordinary angular hand target ({rotation})"
            );
            assert!(
                world
                    .contact_impulses()
                    .iter()
                    .any(|contact| contact.pair == [1, 2] && contact.impulse > 0.0),
                "free angular push must solve its exact prop pair"
            );
        }
        assert_eq!(
            world.held(),
            Some(1),
            "angular contact must retain ownership"
        );
    }
    Ok(())
}

fn table_cup_fixture(
    offset: Vec3,
    mixed_support: bool,
) -> Result<(ItemWorld, PlayerContext), String> {
    let mut world = world()?;
    // The native fixture already contributes its solid table to static collision.
    let context = PlayerContext {
        eye: Vec3::new(0.0, 1.6, 0.2),
        feet: Vec3::new(0.0, 0.0, 0.2),
        view: Vec3::new(
            0.0,
            -50.0_f32.to_radians().sin(),
            -50.0_f32.to_radians().cos(),
        ),
        yaw: 0.0,
        half: 0.4,
        ..player(true)
    };
    let target = context.view.mul_add(Vec3::splat(0.85), context.eye);
    world.spawn(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            mass_kg: 5.0,
            shape: Some(ShapeDef::Sphere { radius: 0.108 }),
            friction: 0.45,
            restitution: 0.2,
            linear_damping: 0.03,
            angular_damping: 0.08,
            ..PhysicsDef::default()
        },
        (
            Vec3::new(-0.108, 0.0, -0.108),
            Vec3::new(0.108, 0.216, 0.108),
        ),
        target.with_y(target.y - 0.108),
        0.0,
        1.0,
    )?;
    world.sync_context(context, &[]);
    assert!(world.pickup(1, context.eye, context.view, context.yaw));
    world.spawn_oriented(
        2,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            mass_kg: 0.025,
            shape: Some(ShapeDef::Cylinder {
                half_height: 0.061,
                radius: 0.0425,
            }),
            friction: 0.55,
            restitution: 0.18,
            linear_damping: 0.02,
            angular_damping: 0.12,
            ..PhysicsDef::default()
        },
        (
            Vec3::new(-0.0425, 0.0, -0.0425),
            Vec3::new(0.0425, 0.122, 0.0425),
        ),
        // Captured native pre-contact asymmetry prevents an exactly centred
        // fixture from hiding the ill-conditioned sphere/cap/table wedge.
        offset.mul_add(Vec3::ONE, Vec3::new(-2.919_954e-6, 0.756, -0.500_002)),
        Quat::from_xyzw(-5.878_109_4e-8, 7.964_637_3e-10, -2.598_360_1e-8, 1.0),
        1.0,
    )?;
    if mixed_support {
        // A separate real heavy/light stack must not destabilize the hand
        // when it enables the strongest contact convergence profile.
        spawn(&mut world, 3, 1.0, 0.1, Vec3::new(3.0, 0.102, 1.0))?;
        spawn(&mut world, 4, 1_000.0, 0.2, Vec3::new(3.0, 0.404, 1.0))?;
    }
    for _ in 0_u8..60 {
        world.step(FIXED_STEP, context, &[]);
    }
    Ok((world, context))
}

// Native acceptance exposed a stiff sphere/cap/table wedge launching the cup.
// Six one-second traces with two/four bodies protect energy and clearance,
// including checkpoint and stopped-world jam persistence.
// Three millimetre-separated starts expose sweep-normal instability. Repeating
// with one distant heavy/light support catches global-profile interactions.
// A geometric jam may stop at carry clearance before generating an impulse;
// the free-push and angular cases above separately require solved contact.
#[test]
fn gentle_held_sphere_push_does_not_launch_a_light_table_cup() -> Result<(), String> {
    for mixed_support in [false, true] {
        for offset in [Vec3::ZERO, Vec3::X * 0.001, Vec3::X * -0.001] {
            let (mut world, mut context) = table_cup_fixture(offset, mixed_support)?;
            let mut peak = 0.0_f32;
            let initial_cup = body(&world, 2)?.pose.position;
            for frame in 1_u8..=60 {
                // One metre/second for five centimetres, then the player stops at
                // the table. The sphere presses the cup toward its support.
                context.eye.z = 0.2 - (f32::from(frame) / 60.0).min(0.05);
                context.feet.z = context.eye.z;
                if frame == 6 {
                    let mut selected = vec![
                        (1, "held".to_owned(), "sphere".to_owned()),
                        (2, "cup".to_owned(), "cylinder".to_owned()),
                    ];
                    if mixed_support {
                        selected.extend([
                            (3, "support".to_owned(), "cube".to_owned()),
                            (4, "load".to_owned(), "cube".to_owned()),
                        ]);
                    }
                    let saved = world.checkpoint(&selected)?;
                    let (mut restored, _) = table_cup_fixture(offset, mixed_support)?;
                    restored.restore_checkpoint(&selected, &saved)?;
                    world = restored;
                }
                // Clock changes must not discard an opposed-contact decision and
                // release accumulated hand lag into this already tipped cup.
                if frame == 8 {
                    world.pause();
                } else if frame == 9 {
                    world.resume();
                } else if frame == 12 {
                    world.set_time_scale(0.0)?;
                } else if frame == 13 {
                    world.set_time_scale(1.0)?;
                }
                world.sync_context(context, &[]);
                world.step(1.0 / 60.0, context, &[]);
                peak = peak.max(body(&world, 2)?.linear_velocity.length());
                assert!(
                    body(&world, 2)?.pose.position.distance(initial_cup) <= 0.25,
                    "frame {frame}: a five centimetre hand push must not expel the cup through positional correction ({:?})",
                    body(&world, 2)?.pose.position
                );
                assert!(world.body_bounds(2).ok_or("missing cup")?.0.y >= -0.006);
                assert!(
                    world
                        .body_penetration_depth(2)
                        .is_some_and(|depth| depth <= 0.006),
                    "frame {frame}: the wedged cup must retain core clearance ({:?}); sphere {:?}, cup {:?}",
                    world.body_penetration_depth(2),
                    body(&world, 1)?.pose,
                    body(&world, 2)?.pose
                );
                assert!(world.take_recoveries().is_empty());
                assert_eq!(world.held(), Some(1));
            }
            assert!(
                body(&world, 1)?.pose.position.z
                    > context.view.z.mul_add(0.85, context.eye.z) + 0.005,
                "the supported wedge must actually stop the hand before its unobstructed target"
            );
            assert!(
                peak <= 2.0,
                "a one metre/second hand must not launch the cup ({peak})"
            );
        }
    }
    Ok(())
}

// The audit's sleeping overlaps are distinct from floor retention. Six
// two-body mass cases protect dynamic separation, exact rotated floor bounds,
// balanced support, natural sleep and unchanged inertia, including both
// convergence thresholds. Twelve five-second CPU runs share the tiny existing
// fixture; no map bake or graphics work.
#[test]
fn heavy_light_contacts_retain_floor_and_separate_before_sleeping() -> Result<(), String> {
    for (light, heavy, radius) in [
        (0.025, 5.0, 0.05),
        (1.0, 1_000.0, 0.1),
        (0.025, 0.7975, 0.05),
        (0.025, 1.0, 0.05),
        (0.025, 6.3975, 0.05),
        (0.025, 6.4, 0.05),
    ] {
        for dropped in [false, true] {
            let mut world = world()?;
            spawn(
                &mut world,
                1,
                light,
                radius,
                Vec3::new(0.0, radius + 0.002, 1.0),
            )?;
            let upper_y = if dropped {
                1.5
            } else {
                radius.mul_add(2.0, 0.204)
            };
            spawn(&mut world, 2, heavy, 0.2, Vec3::new(0.0, upper_y, 1.0))?;
            let original_lower = body(&world, 1)?;
            let original_upper = body(&world, 2)?;
            let mut maximum_penetration = 0.0_f32;
            for _ in 0_u16..600 {
                world.step(FIXED_STEP, player(false), &[]);
                for key in [1, 2] {
                    let (minimum, _) = world.body_bounds(key).ok_or("missing contact bounds")?;
                    assert!(
                        minimum.y >= -0.006,
                        "{light}/{heavy}kg dropped={dropped}: exact shape must retain its floor ({minimum:?})"
                    );
                    maximum_penetration = maximum_penetration.max(
                        world
                            .body_penetration_depth(key)
                            .ok_or("missing contact shape")?,
                    );
                }
                assert!(
                    world.take_recoveries().is_empty(),
                    "ordinary heavy/light contact must not require escape recovery"
                );
            }
            // A 1.3m drop arrives below 5.1m/s. One 120Hz interval plus the
            // two skins and solver slop bounds transient overlap by 50mm.
            assert!(
                maximum_penetration <= 0.05,
                "{light}/{heavy}kg dropped={dropped}: transient overlap {maximum_penetration} exceeds one interval of incoming travel"
            );
            for (key, original) in [(1, original_lower), (2, original_upper)] {
                let resting = body(&world, key)?;
                assert!(
                    resting.sleeping,
                    "{light}/{heavy}kg dropped={dropped}: contact must settle naturally"
                );
                assert!(
                    world
                        .body_penetration_depth(key)
                        .is_some_and(|depth| depth <= 0.006),
                    "{light}/{heavy}kg: sleeping shapes must remain separated"
                );
                assert_eq!(
                    resting.solver_mass_kg, original.solver_mass_kg,
                    "convergence must preserve authored mass"
                );
                assert_eq!(
                    resting.principal_inertia, original.principal_inertia,
                    "convergence must preserve authored inertia"
                );
                if !dropped {
                    assert!(
                        resting.pose.position.x.abs() < 0.01
                            && (resting.pose.position.z - 1.0).abs() < 0.01,
                        "{light}/{heavy}kg: a balanced touching stack must retain its support, without sideways ejection ({:?})",
                        resting.pose.position
                    );
                }
            }
            if !dropped {
                let lower_center = body(&world, 1)?.pose.position;
                let upper_center = body(&world, 2)?.pose.position;
                assert!(
                    (upper_center.y - lower_center.y - radius - 0.2).abs() <= 0.006,
                    "{light}/{heavy}kg: the original balanced dynamic support must survive settling ({lower_center:?}, {upper_center:?})"
                );
            }
        }
    }
    Ok(())
}
