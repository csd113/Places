//! Small in-memory rigid-body witnesses for the source-only bottle policy.
//! No candidate art, renderer, audio device, or map build is required.
use super::bottle_contacts::{Witness, commit_witnesses};
use super::{BottleKind, Breakable};
use crate::engine::{
    level::LevelDef,
    physics::{
        BodyMotion, FIXED_STEP, ImpactAudio, ImpactRelease, ItemWorld, PhysicsDef, PlayerContext,
        ShapeDef,
    },
};
use glam::Vec3;

const BOTTLE: u64 = 1;
const STRIKER: u64 = 2;
const BOUNDS: (Vec3, Vec3) = (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1));
const BEER: Breakable = Breakable::Bottle(BottleKind::Beer);

fn profile() -> PhysicsDef {
    PhysicsDef {
        motion: BodyMotion::Dynamic,
        shape: Some(ShapeDef::Sphere { radius: 0.1 }),
        pickup: true,
        restitution: 0.0,
        ..PhysicsDef::default()
    }
}

fn fracture_level() -> Result<LevelDef, String> {
    LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"bottle_fracture","name":"Bottle fracture","spawn":{"x":1,"z":1},"rooms":[{"x":-3,"z":-3,"width":6,"depth":6,"height":3,"material":"home:hardwood_oak_01"}]}"#,
    ).map_err(|error| error.to_string())
}

fn fixture(kind: Breakable, position: Vec3) -> Result<ItemWorld, String> {
    let mut world = ItemWorld::from_level(&fracture_level()?, &[]);
    world.spawn(BOTTLE, &profile(), BOUNDS, position, 0.0, 1.0)?;
    world.register_breakable(BOTTLE, kind, 1.0);
    Ok(world)
}

fn player() -> PlayerContext {
    PlayerContext {
        eye: Vec3::new(0.0, 1.5, 0.0),
        feet: Vec3::ZERO,
        yaw: 0.0,
        view: Vec3::NEG_Z,
        height: 1.8,
        half: 0.4,
        active: true,
    }
}

fn step(world: &mut ItemWorld, count: u8) {
    for _ in 0..count {
        world.step(FIXED_STEP, player(), &[]);
    }
}

fn label_release(
    world: &mut ItemWorld,
    originating: ImpactRelease,
    inherited: ImpactRelease,
    source: u64,
) -> Result<(), String> {
    let runtime = world.runtime.as_mut().ok_or("runtime")?;
    let entry = runtime.entries.get_mut(&BOTTLE).ok_or("bottle")?;
    entry.originating_release = originating;
    entry.release = inherited;
    entry.release_source = source;
    entry.release_incident = 1;
    entry.release_serial = 1;
    Ok(())
}

// Metadata only: the shared admission gate used by native authoring, the
// compiler and the loader must accept exactly 256 movable placements.
#[test]
fn movable_prop_validation_accepts_256_and_rejects_257() -> Result<(), String> {
    let mut level = fracture_level()?;
    let prototype = serde_json::from_str(
        r#"{"model":"core:beer_bottle","physics":{"motion":"dynamic","pickup":true}}"#,
    )
    .map_err(|error| error.to_string())?;
    level.props = vec![prototype; 256];
    crate::engine::loader::validate_prop_physics(&level)?;
    let extra = level.props.first().cloned().ok_or("missing test prop")?;
    level.props.push(extra);
    assert_eq!(
        crate::engine::loader::validate_prop_physics(&level),
        Err("Level contains too many movable physics props: 257 (limit: 256)".into())
    );
    Ok(())
}

fn stage_fragments(world: &mut ItemWorld, plan: super::BreakPlan) -> Result<Vec<u64>, String> {
    plan.kind
        .panels()
        .iter()
        .copied()
        .map(|panel| {
            let key = world.next_fragment_key().ok_or("missing fragment key")?;
            let _pose = world.spawn_break_fragment(key, plan, panel)?;
            Ok(key)
        })
        .collect()
}

// Four short solver runs protect real throw/drop qualification at the authored
// ceiling and both native pickup entry points without rendering or map baking.
#[test]
fn each_breakable_qualifies_and_rejects_fragment_pickup_at_authored_capacity() -> Result<(), String>
{
    let initial = Vec3::new(0.0, 1.4, -1.0);
    let p = player();
    for kind in [
        BEER,
        Breakable::House,
        Breakable::SpruceGlobe,
        Breakable::VillageGlobe,
    ] {
        let mut world = ItemWorld::from_level(&fracture_level()?, &[]);
        world.spawn_authored(BOTTLE, &profile(), BOUNDS, initial, 0.0, 1.0)?;
        world.register_breakable(BOTTLE, kind, 1.0);
        for index in 2_u16..=256 {
            let at = Vec3::new(
                f32::from(index % 16).mul_add(0.25, 4.0),
                0.0,
                f32::from(index / 16).mul_add(0.25, 4.0),
            );
            world.spawn_authored(u64::from(index), &profile(), BOUNDS, at, 0.0, 1.0)?;
        }
        step(&mut world, 1);
        assert_eq!(world.stats().active_bodies, 0);
        assert_eq!(world.stats().dormant_bodies, 256);
        let runtime = world.runtime.as_ref().ok_or("missing solver world")?;
        assert_eq!(
            runtime.wake_targets.capacity(),
            0,
            "idle props need no wake geometry"
        );
        assert_eq!(runtime.wake_sweeps.capacity(), 0);
        assert_eq!(
            world.spawn_authored(257, &profile(), BOUNDS, initial, 0.0, 1.0),
            Err("physics body limit reached (256)".into())
        );
        assert_eq!(world.nearest_pickup(p.eye, p.view), Some(BOTTLE));
        assert!(world.pickup(BOTTLE, p.eye, p.view, p.yaw));
        if kind.is_bottle() {
            assert!(world.throw(Vec3::NEG_Y));
        } else {
            assert!(world.drop());
        }
        step(&mut world, 70);
        let plans = world.drain_break_plans();
        let [plan] = plans.as_slice() else {
            return Err(format!(
                "expected one solved breakup for {} pieces",
                kind.panels().len()
            ));
        };
        assert_eq!(
            world.reserve_break_fragments(kind.panels().len())?,
            Vec::<u64>::new()
        );
        let pieces = stage_fragments(&mut world, *plan)?;
        assert_eq!(pieces.len(), kind.panels().len());
        assert!(world.commit_break(BOTTLE, pieces.clone()));
        for key in pieces {
            let pose = crate::engine::physics::ItemPose {
                position: Vec3::new(0.0, 1.5, 1.2),
                rotation: glam::Quat::IDENTITY,
            };
            assert!(world.set_fixture_pose(key, pose));
            let entry = world
                .runtime
                .as_ref()
                .and_then(|probe_world| probe_world.entries.get(&key))
                .ok_or("missing probe fragment")?;
            let eye = crate::engine::physics::world::add(
                crate::engine::physics::world::add(pose.position, entry.center),
                Vec3::Z,
            );
            assert_eq!(
                world
                    .first_unheld_body_hit(eye, Vec3::NEG_Z, 1.5)
                    .map(|(hit, _)| hit),
                Some(key)
            );
            assert_eq!(world.nearest_pickup(eye, Vec3::NEG_Z), None);
            assert!(!world.pickup(key, eye, Vec3::NEG_Z, 0.0));
            assert!(world.set_fixture_pose(key, plan.pose));
        }
        assert_eq!(world.stats().dormant_bodies, 255);
    }
    Ok(())
}

// Mixed plans verify cross-family retirement and reset without rendering
// or stepping the solver.
#[test]
fn mixed_fragment_reserve_retires_the_oldest_group_and_resets() -> Result<(), String> {
    let mut world = ItemWorld::from_level(&fracture_level()?, &[]);
    let mut sets = Vec::new();
    let settings = PhysicsDef {
        reset_group: Some("fractures".into()),
        ..profile()
    };
    let kinds = [
        BEER,
        Breakable::House,
        Breakable::SpruceGlobe,
        Breakable::VillageGlobe,
    ];
    for (index, kind) in (1_u16..).zip(kinds) {
        let key = u64::from(index);
        let position = Vec3::new(f32::from(index).mul_add(0.7, -2.0), 1.4, -1.0);
        world.spawn_authored(key, &settings, BOUNDS, position, 0.0, 1.0)?;
        world.register_breakable(key, kind, 1.0);
        let plan = super::BreakPlan {
            key,
            pose: crate::engine::physics::ItemPose {
                position,
                rotation: glam::Quat::IDENTITY,
            },
            scale: 1.0,
            kind,
        };
        let retired = world.reserve_break_fragments(kind.panels().len())?;
        if index == 4 {
            assert_eq!(
                Some(&retired),
                sets.first(),
                "a globe must reclaim the oldest bottle's shards"
            );
            assert!(retired.iter().all(|piece| !world.contains_body(*piece)));
        } else {
            assert_eq!(retired, Vec::<u64>::new());
        }
        let pieces = stage_fragments(&mut world, plan)?;
        assert!(world.commit_break(key, pieces.clone()));
        sets.push(pieces);
    }
    assert!(
        world
            .runtime
            .as_ref()
            .and_then(|r| r.entries.get(&BOTTLE))
            .is_some_and(|e| e.recovery.suspended)
    );
    assert!(
        world
            .runtime
            .as_ref()
            .and_then(|r| r.entries.get(&STRIKER))
            .is_some_and(|e| e.recovery.suspended)
    );
    let reset = world.reset_group("fractures")?;
    assert_eq!(reset.released_held, None);
    let mut expected: Vec<_> = sets.iter().skip(1).flatten().copied().collect();
    let mut retired = world.drain_fragment_retirements();
    expected.sort_unstable();
    retired.sort_unstable();
    assert_eq!(retired, expected);
    assert_eq!(
        world
            .runtime
            .as_ref()
            .ok_or("missing solver world")?
            .break_fragment_count,
        0
    );
    assert!(
        world
            .runtime
            .as_ref()
            .and_then(|r| r.entries.get(&BOTTLE))
            .is_some_and(|e| !e.recovery.suspended)
    );

    Ok(())
}

// Construct one partial transaction that cannot be cleared by retiring the
// only committed group. Failed admission leaves old debris and the new original.
#[test]
fn reserve_exhaustion_keeps_old_debris_and_new_original() -> Result<(), String> {
    let at = Vec3::new(-1.0, 1.4, -1.0);
    let mut world = fixture(BEER, at)?;
    let bottle = super::BreakPlan {
        key: BOTTLE,
        pose: crate::engine::physics::ItemPose {
            position: at,
            rotation: glam::Quat::IDENTITY,
        },
        scale: 1.0,
        kind: BEER,
    };
    let house = super::BreakPlan {
        key: STRIKER,
        pose: crate::engine::physics::ItemPose {
            position: Vec3::new(1.0, 1.4, -1.0),
            rotation: glam::Quat::IDENTITY,
        },
        scale: 1.0,
        kind: Breakable::House,
    };
    world.spawn_authored(STRIKER, &profile(), BOUNDS, house.pose.position, 0.0, 1.0)?;
    world.register_breakable(STRIKER, house.kind, house.scale);
    let committed = stage_fragments(&mut world, bottle)?;
    assert!(world.commit_break(bottle.key, committed.clone()));
    let sample = *house.kind.panels().first().ok_or("no house panel")?;
    for _ in committed.len()..crate::engine::physics::MAX_BREAK_FRAGMENTS {
        let key = world.next_fragment_key().ok_or("missing fragment key")?;
        let _pose = world.spawn_break_fragment(key, house, sample)?;
    }
    assert_eq!(
        world.reserve_break_fragments(house.kind.panels().len()),
        Err("not enough committed fracture debris to retire".into())
    );
    assert!(
        committed.iter().all(|key| world.contains_body(*key)),
        "failed admission must not retire a partial breakup"
    );
    let overflow = world.next_fragment_key().ok_or("missing fragment key")?;
    assert!(world.spawn_break_fragment(overflow, house, sample).is_err());
    let runtime = world.runtime.as_ref().ok_or("missing solver world")?;
    assert_eq!(
        runtime.break_fragment_count,
        crate::engine::physics::MAX_BREAK_FRAGMENTS
    );
    assert!(
        runtime
            .entries
            .get(&house.key)
            .is_some_and(|entry| !entry.recovery.suspended)
    );
    assert!(
        runtime
            .breakables
            .get(&house.key)
            .is_some_and(|record| record.pieces.is_empty())
    );
    Ok(())
}

#[test]
fn bottle_solver_routes_only_own_strong_throw_to_one_fracture() -> Result<(), String> {
    let initial = Vec3::new(0.0, 1.4, -1.0);
    let p = player();
    for release in [
        ImpactRelease::Natural,
        ImpactRelease::Dropped,
        ImpactRelease::Thrown,
    ] {
        let mut world = fixture(BEER, initial)?;
        if release != ImpactRelease::Natural {
            assert!(world.pickup(BOTTLE, p.eye, p.view, p.yaw));
            assert!(if release == ImpactRelease::Thrown {
                world.throw(Vec3::NEG_Y)
            } else {
                world.drop()
            });
        }
        // Match the CCD speed of the vertical throw without granting throw
        // provenance to the drop. Both must retain the real solved contact.
        if release == ImpactRelease::Dropped {
            assert!(world.set_fixture_velocity(BOTTLE, Vec3::new(0.0, -8.0, 0.0), Vec3::ZERO));
        }
        step(&mut world, 70);
        let plans = world.drain_break_plans();
        assert!(
            world.drain_break_plans().is_empty(),
            "cannot drain the same breakup twice"
        );
        let thrown = release == ImpactRelease::Thrown;
        assert_eq!(plans.len(), usize::from(thrown), "{release:?}");
        if thrown {
            // Commit with a real test solver body, not a stand-in shipped asset.
            let part = world.next_fragment_key().ok_or("fragment key")?;
            world.spawn(part, &profile(), BOUNDS, initial, 0.0, 1.0)?;
            assert!(world.commit_break(BOTTLE, vec![part]));
            assert!(
                !world.commit_break(BOTTLE, vec![part]),
                "no second committed shatter"
            );
            let ids = [(BOTTLE, "bottle".into(), "core:beer_bottle".into())];
            let persisted = world.checkpoint(&ids)?;
            let mut restored = fixture(BEER, initial)?;
            restored.restore_checkpoint(&ids, &persisted)?;
            assert_eq!(restored.snapshots().len(), 1, "shards are session-only");
            assert!(
                restored.drain_break_plans().is_empty(),
                "resume cannot replay a shatter"
            );
        }
        let mut found = false;
        world.drain_impact_audio(|impact, route| {
            if impact.key == BOTTLE && impact.other_key.is_none() {
                found = true;
                assert_eq!(
                    route,
                    if thrown {
                        ImpactAudio::ReplacedByShatter
                    } else {
                        ImpactAudio::BottleGlass
                    }
                );
                assert!(impact.speed >= 3.6, "even the hard drop must stay intact");
            }
        });
        assert!(found, "expected a real floor impact for {release:?}");
    }
    Ok(())
}

#[test]
fn bottle_cannot_borrow_collision_speed_or_release_from_another_body() -> Result<(), String> {
    for (originating, source) in [
        (ImpactRelease::Natural, STRIKER),
        (ImpactRelease::Thrown, BOTTLE),
    ] {
        let mut world = fixture(BEER, Vec3::new(0.0, 0.7, -1.0))?;
        label_release(&mut world, originating, ImpactRelease::Thrown, source)?;
        world.spawn(
            STRIKER,
            &profile(),
            BOUNDS,
            Vec3::new(-0.5, 0.7, -1.0),
            0.0,
            1.0,
        )?;
        assert!(world.set_fixture_velocity(STRIKER, Vec3::new(8.0, 0.0, 0.0), Vec3::ZERO));
        step(&mut world, 10);
        assert!(world.drain_break_plans().is_empty());
        let mut found = false;
        world.drain_impact_audio(|impact, route| {
            if impact.key == STRIKER && impact.other_key == Some(BOTTLE) {
                found = true;
                assert!(
                    impact.speed >= 3.6,
                    "the other body supplied the strong relative speed"
                );
                assert_eq!(
                    route,
                    ImpactAudio::Surface,
                    "non-bottle ownership keeps generic audio"
                );
            }
        });
        assert!(
            found,
            "the fixture must exercise an actual collateral contact"
        );
    }
    Ok(())
}

fn contact(world: &mut ItemWorld, speed: f32, impulse: f32) -> Result<bool, String> {
    let runtime = world.runtime.as_mut().ok_or("runtime")?;
    let entry = runtime.entries.get(&BOTTLE).ok_or("body")?;
    let item = runtime
        .breakables
        .get_mut(&BOTTLE)
        .ok_or("registered fracture")?;
    Ok(item.witness_contact(BOTTLE, entry, false, speed, impulse))
}

#[test]
fn bottle_weak_first_contact_stays_consumed_after_restore_and_christmas_policy_is_unchanged()
-> Result<(), String> {
    let position = Vec3::new(0.0, 1.4, -1.0);
    let ids = [(BOTTLE, "bottle".into(), "core:beer_bottle".into())];
    for (speed, impulse, consumed) in [(1.0, 0.2, true), (8.0, 0.012, true), (8.0, 0.0, false)] {
        let mut original = fixture(BEER, position)?;
        label_release(
            &mut original,
            ImpactRelease::Thrown,
            ImpactRelease::Thrown,
            BOTTLE,
        )?;
        assert!(!contact(&mut original, speed, impulse)?);
        let saved = original.checkpoint(&ids)?;
        let mut restored = fixture(BEER, position)?;
        restored.restore_checkpoint(&ids, &saved)?;
        assert_eq!(contact(&mut restored, 8.0, 0.2)?, !consumed);
        assert!(
            !contact(&mut restored, 8.0, 0.2)?,
            "first real contact spends the release"
        );
    }
    for kind in [
        Breakable::House,
        Breakable::SpruceGlobe,
        Breakable::VillageGlobe,
    ] {
        let mut world = fixture(kind, position)?;
        label_release(
            &mut world,
            ImpactRelease::Thrown,
            ImpactRelease::Thrown,
            BOTTLE,
        )?;
        assert!(!contact(&mut world, 8.0, 0.2)?);
        label_release(
            &mut world,
            ImpactRelease::Dropped,
            ImpactRelease::Dropped,
            BOTTLE,
        )?;
        assert!(
            contact(&mut world, 8.0, 0.2)?,
            "the original drop rule remains in force"
        );
    }
    Ok(())
}

fn missed_bottle_pair(
    release: ImpactRelease,
    [first_speed, second_speed]: [f32; 2],
) -> Result<ItemWorld, String> {
    let initial = Vec3::new(0.0, 1.4, -1.0);
    let mut world = fixture(BEER, initial)?;
    world.spawn(
        STRIKER,
        &profile(),
        BOUNDS,
        Vec3::new(-1.0, 1.4, -1.0),
        0.0,
        1.0,
    )?;
    world.register_breakable(STRIKER, Breakable::Bottle(BottleKind::Wine), 1.0);
    label_release(&mut world, release, release, BOTTLE)?;
    let runtime = world.runtime.as_mut().ok_or("runtime")?;
    let first = runtime.entries.get(&BOTTLE).ok_or("first bottle")?.collider;
    let second = runtime
        .entries
        .get(&STRIKER)
        .ok_or("second bottle")?
        .collider;
    assert!(
        !runtime.world.contact_pairs().any(|pair| {
            [pair.collider1, pair.collider2] == [first, second]
                || [pair.collider1, pair.collider2] == [second, first]
        }),
        "the pair must be absent from the final graph"
    );
    // Inject only the solved-substep records. The bodies, throw provenance,
    // fracture commit and queued audio are real. Reverse collider order for
    // the two witnesses to test that contact identity is independent of order.
    let mut contacts = [
        Witness {
            key: STRIKER,
            pair: [second, first],
            position: initial,
            speed: second_speed,
            impulse: 0.2,
            fracture: false,
        },
        Witness {
            key: BOTTLE,
            pair: [first, second],
            position: initial,
            speed: first_speed,
            impulse: 0.2,
            fracture: false,
        },
    ];
    commit_witnesses(&mut contacts, &mut runtime.breakables, &runtime.entries);
    runtime.collect_impacts(&world.surface_materials, &contacts);
    assert!(runtime.impacts.is_empty(), "no ordinary graph event");
    runtime.collect_missed_bottle_impacts(&world.surface_materials, &mut contacts);
    assert_eq!(runtime.impacts.len(), 1, "one event for the same CCD pair");
    assert!(
        runtime
            .entries
            .get(&BOTTLE)
            .is_some_and(|entry| entry.impact_cooldown > 0.0)
    );
    assert!(
        runtime
            .entries
            .get(&STRIKER)
            .is_some_and(|entry| entry.impact_cooldown > 0.0)
    );
    runtime.collect_missed_bottle_impacts(&world.surface_materials, &mut contacts);
    assert_eq!(runtime.impacts.len(), 1, "the cooldown covers both sides");
    Ok(world)
}

#[test]
fn bottle_ccd_pair_a_single_shatter_replaces_the_whole_impact() -> Result<(), String> {
    let mut world = missed_bottle_pair(ImpactRelease::Thrown, [4.2, 6.0])?;
    let plans = world.drain_break_plans();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans.first().map(|plan| plan.key), Some(BOTTLE));
    let piece = world.next_fragment_key().ok_or("fragment key")?;
    world.spawn(
        piece,
        &profile(),
        BOUNDS,
        Vec3::new(0.0, 1.4, -1.0),
        0.0,
        1.0,
    )?;
    assert!(world.commit_break(BOTTLE, vec![piece]));
    let mut audio = Vec::new();
    world.drain_impact_audio(|event, route| audio.push((event.key, event.other_key, route)));
    assert_eq!(
        audio,
        [(STRIKER, Some(BOTTLE), ImpactAudio::ReplacedByShatter)]
    );
    Ok(())
}

#[test]
fn bottle_ccd_pair_quiet_intact_contact_keeps_one_glass_cue() -> Result<(), String> {
    let mut world = missed_bottle_pair(ImpactRelease::Dropped, [0.8, 0.6])?;
    assert!(world.drain_break_plans().is_empty());
    let mut audio = Vec::new();
    world.drain_impact_audio(|event, route| audio.push((event.key, event.release, route)));
    assert_eq!(
        audio,
        [(BOTTLE, ImpactRelease::Dropped, ImpactAudio::BottleGlass)]
    );
    Ok(())
}
