//! One metadata-only ride protects the distinct custom player and NPC movers
//! and Rapier support contacts from visual-only or volume-based carry. The
//! loose rigid body, idle agent and no-input controller share one power event.

use glam::Vec3;

use crate::engine::level::LevelDef;
use crate::engine::physics::{
    ItemWorld,
    config::{BodyMotion, PhysicsDef, ShapeDef},
};
use crate::game::input::InputState;
use crate::game::{AppState, CollisionWorld, EYE_HEIGHT, Game, Settings, VERTICAL_SUBSTEP};

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{
      "schema":"places-level","format_version":1,"id":"escalator_test","name":"Escalator support",
      "spawn":{"x":1.68,"z":1.3},
      "rooms":[{"x":0,"z":0,"width":5,"depth":10,"height":6}],
      "floor_regions":[{"x":1,"z":7.6,"width":2,"depth":2.4,"offset_y":3}],
      "props":[
        {"id":"lift","model":"mall:escalator","escalator":{},"x":2,"z":1,"occludes":false,
         "components":[{"component":"animation","clip":"escalator_run","looped":true},
                       {"component":"interactable","prompt":"Toggle escalator"}],
         "bindings":[{"on":"interact","actions":[{"action":"toggle_animation","clip":"escalator_run"}]}]},
        {"id":"rider","model":"mannequin","x":2.35,"y":0.2,"z":2.0,
         "components":[{"component":"nav_agent","radius":0.18,"height":1.8,"speed_mps":1,"max_slope":1},
                       {"component":"ai","behavior":"idler"}]}],
      "volumes":[{"id":"switch","x":4,"z":1,"width":0.5,"depth":0.5,"bottom_y":0,"top_y":2,
        "bindings":[{"on":"enter_volume","actions":[
          {"action":"toggle_animation","target":"lift","clip":"escalator_run"}]}]}]
    }"#).map_err(|error| error.to_string())
}

fn game(level: &LevelDef, feet: Vec3) -> Game {
    let eye = Vec3::new(feet.x, feet.y + EYE_HEIGHT, feet.z);
    let mut game = Game::new(eye, 0.0, CollisionWorld::from_level(level));
    game.set_app_state(AppState::Playing);
    game
}

fn toggle(game: &mut Game) {
    let outside = Vec3::new(3.75, 0.0, 1.25);
    let inside = Vec3::new(4.25, 0.0, 1.25);
    game.entities_mut().update_volumes(outside, inside);
    let _toggle = game.entities_mut().dispatch_queued_events();
    game.entities_mut().update_volumes(inside, outside);
    let _leave = game.entities_mut().dispatch_queued_events();
}

fn tick(game: &mut Game, items: &mut ItemWorld) {
    tick_at(game, items, VERTICAL_SUBSTEP);
}

fn tick_at(game: &mut Game, items: &mut ItemWorld, delta: f32) {
    game.sim_delta_seconds = delta;
    game.update_player_movement_with_items(&mut InputState::default(), &Settings::default(), items);
    items.sync_escalators(
        game.entities().escalators(),
        game.entities().escalator_frame_seconds(),
    );
    items.step(delta, game.physical_player_context(), game.door_colliders());
}

fn witnesses(game: &Game, items: &ItemWorld) -> Result<[Vec3; 3], String> {
    Ok([
        game.trigger_feet(),
        game.entities()
            .ai()
            .agents()
            .first()
            .ok_or("idle rider")?
            .position,
        items.body_pose_velocity(1).ok_or("loose prop")?.0.position,
    ])
}

fn add_loose_witness(items: &mut ItemWorld) -> Result<(), String> {
    items.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            shape: Some(ShapeDef::Cuboid {
                half_extents: [0.05, 0.06, 0.0725],
            }),
            mass_kg: 0.065,
            friction: 0.6,
            restitution: 0.08,
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.05, 0.0, -0.07), Vec3::new(0.05, 0.12, 0.07)),
        Vec3::new(2.22, 0.736, 3.0),
        0.0,
        1.0,
    )
}

#[test]
fn escalator_support_rides_stops_resumes_and_dismounts() -> Result<(), String> {
    let level = level()?;
    let mut game = game(&level, Vec3::new(1.68, 0.0, 1.3));
    let mut items = ItemWorld::from_level(&level, &level.item_collision_aabbs());
    add_loose_witness(&mut items)?;
    for _ in 0_u8..90 {
        tick(&mut game, &mut items);
    }
    assert!(
        items.body_is_dormant(1),
        "stopped tread does not activate a placed prop"
    );
    let initial = witnesses(&game, &items)?;
    toggle(&mut game);
    assert!(
        game.entities()
            .escalators()
            .first()
            .is_some_and(crate::engine::escalator::Escalator::running),
        "an ordinary enter-volume action starts the belt"
    );
    for _ in 0_u8..173 {
        tick(&mut game, &mut items);
    }
    let before_stop = witnesses(&game, &items)?;
    assert!(
        !items.body_is_dormant(1),
        "real powered tread contact wakes the loose prop"
    );
    for (start, rider) in initial.into_iter().zip(before_stop) {
        assert!(
            rider.z > start.z + 0.60 && rider.y > start.y + 0.20,
            "a supported rider must make positive progress: {start:?} -> {rider:?}"
        );
    }
    toggle(&mut game);
    let stopped_pose = game
        .entities()
        .escalators()
        .first()
        .ok_or("escalator")?
        .cue();
    for _ in 0_u8..100 {
        tick(&mut game, &mut items);
    }
    let stopped = witnesses(&game, &items)?;
    assert_eq!(
        stopped_pose,
        game.entities()
            .escalators()
            .first()
            .ok_or("escalator")?
            .cue()
    );
    for (previous, rider) in before_stop.into_iter().zip(stopped) {
        assert!(
            previous.distance(rider) < 0.08,
            "stopped belt cannot creep: {previous:?} -> {rider:?}"
        );
    }
    toggle(&mut game);
    tick(&mut game, &mut items);
    for (previous, rider) in stopped.into_iter().zip(witnesses(&game, &items)?) {
        assert!(
            previous.distance(rider) < 0.02,
            "restart must not snap any rider"
        );
    }
    for _ in 0_u16..1_450 {
        tick(&mut game, &mut items);
    }
    let landed = witnesses(&game, &items)?;
    for rider in landed {
        assert!(
            rider.z >= 8.19 && (rider.y - 3.0).abs() < 0.10,
            "rider left the tread onto the true upper landing: {rider:?}"
        );
    }
    for _ in 0_u8..90 {
        tick(&mut game, &mut items);
    }
    for (previous, rider) in landed.into_iter().zip(witnesses(&game, &items)?) {
        assert!(
            previous.distance(rider) < 0.08,
            "no carry on the landing beyond the comb"
        );
    }
    Ok(())
}

#[test]
fn escalator_wakes_only_supported_dormant_and_sleeping_items() -> Result<(), String> {
    let level = level()?;
    let mut game = game(&level, Vec3::new(0.55, 0.0, 0.55));
    let mut items = ItemWorld::from_level(&level, &level.item_collision_aabbs());
    for (key, position) in [
        (1, Vec3::new(1.8, 0.002, 1.25)),
        (2, Vec3::new(2.2, 0.002, 1.25)),
        // Above the incline but inside the welded tread's broad-phase box.
        (3, Vec3::new(2.0, 2.65, 4.6)),
        (4, Vec3::new(4.15, 0.002, 7.2)),
        (5, Vec3::new(4.15, 0.002, 8.5)),
    ] {
        items.spawn_authored(
            key,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                ..PhysicsDef::default()
            },
            (Vec3::new(-0.04, 0.0, -0.04), Vec3::new(0.04, 0.08, 0.04)),
            position,
            0.0,
            1.0,
        )?;
    }
    assert!(
        items.activate(2) && items.activate(5),
        "seed the two sleepers"
    );
    for _ in 0_u16..480 {
        tick(&mut game, &mut items);
    }
    let sleeping = |world: &ItemWorld, key: u64| {
        world
            .snapshots()
            .iter()
            .any(|body| body.key == key && body.sleeping)
    };
    assert!(
        items.body_is_dormant(1),
        "a stopped tread leaves the first item dormant"
    );
    assert!(
        sleeping(&items, 2) && sleeping(&items, 5),
        "both resting bodies slept"
    );

    toggle(&mut game);
    game.sim_delta_seconds = 0.05;
    game.update_player_movement_with_items(
        &mut InputState::default(),
        &Settings::default(),
        &mut items,
    );
    assert!(
        game.entities()
            .escalators()
            .first()
            .is_some_and(crate::engine::escalator::Escalator::powered_this_frame),
        "exercise the pause with powered tread state already latched"
    );
    items.pause();
    items.sync_escalators(game.entities().escalators(), 0.05);
    items.step(0.05, game.physical_player_context(), game.door_colliders());
    assert!(
        items.body_is_dormant(1) && sleeping(&items, 2),
        "a paused world cannot wake either supported body even with latched power"
    );

    items.resume();
    for _ in 0_u8..90 {
        tick(&mut game, &mut items);
    }
    for key in [1, 2] {
        let (pose, _) = items.body_pose_velocity(key).ok_or("on-tread body")?;
        assert!(
            !items.body_is_dormant(key) && !sleeping(&items, key) && pose.position.z > 1.45,
            "powered support must wake and carry both body states: {key}: {pose:?}"
        );
    }
    assert!(
        items.body_is_dormant(3),
        "broad-phase proximity is not tread support"
    );
    assert!(
        items.body_is_dormant(4) && sleeping(&items, 5),
        "distant bodies were not disturbed"
    );
    Ok(())
}

#[test]
fn escalator_retains_the_local_control_proxy_across_toggles_and_reset() -> Result<(), String> {
    let level = level()?;
    let mut game = game(&level, Vec3::new(2.85, 0.0, 0.56));
    let mut items = ItemWorld::from_level(&level, &level.item_collision_aabbs());
    let control = game
        .entities()
        .interactables()
        .index_of("lift")
        .ok_or("manual control")?;
    let center = Vec3::new(2.75, 0.82, 1.29);
    let target_is_local = |state: &Game| {
        state
            .entities()
            .interactables()
            .get(control)
            .is_some_and(|item| {
                let eye = Vec3::new(2.85, EYE_HEIGHT, 0.56);
                let rider_eye = Vec3::new(2.0, EYE_HEIGHT + 0.4, 2.1);
                let look_at_tread = Vec3::new(2.0, 0.7, 2.8) - rider_eye;
                crate::engine::interact::target_distance(eye, center - eye, item, &[]).is_some()
                    && crate::engine::interact::target_distance(rider_eye, look_at_tread, item, &[])
                        .is_none()
            })
    };
    assert!(target_is_local(&game), "only the rail post can be aimed at");
    for expected in [true, false] {
        let _interaction = game.dispatch_interaction_at(Some(control));
        tick(&mut game, &mut items);
        assert!(
            target_is_local(&game),
            "a world update after pressing the control must not change its aim bounds"
        );
        assert_eq!(
            game.entities()
                .escalators()
                .first()
                .ok_or("belt")?
                .running(),
            expected
        );
    }
    game.entities_mut().reset_runtime();
    assert!(
        target_is_local(&game),
        "reset retains the same fixed aim bounds"
    );
    Ok(())
}

#[test]
fn escalator_never_drives_airborne_offside_or_dismounted_hulls() -> Result<(), String> {
    let level = level()?;
    for feet in [
        Vec3::new(2.0, 2.0, 3.0),
        Vec3::new(3.2, 0.0, 3.0),
        Vec3::new(2.0, 3.0, 8.22),
    ] {
        let mut game = game(&level, feet);
        toggle(&mut game);
        let mut items = ItemWorld::default();
        for _ in 0_u8..16 {
            tick(&mut game, &mut items);
        }
        let after = game.trigger_feet();
        assert!(
            (after.z - feet.z).abs() < 0.001 && (after.x - feet.x).abs() < 0.001,
            "only actual tread support may move a player: {feet:?} -> {after:?}"
        );
    }
    Ok(())
}

#[test]
fn escalator_rejects_a_missing_or_partial_landing() -> Result<(), String> {
    let mut level = level()?;
    level
        .floor_regions
        .first_mut()
        .ok_or("upper landing")?
        .width = 1.55;
    assert!(
        crate::engine::escalator::validate(&level).is_err(),
        "even a narrow unsupported strip along one side of the crest is invalid"
    );
    level.floor_regions.clear();
    assert!(
        crate::engine::escalator::validate(&level).is_err(),
        "a flat-room crest is invalid"
    );
    Ok(())
}

#[test]
fn escalator_landing_union_respects_authored_priority_and_gaps() -> Result<(), String> {
    let mut level = level()?;
    let base = level
        .floor_regions
        .first()
        .cloned()
        .ok_or("upper landing")?;
    let wrong = level.floor_regions.first_mut().ok_or("upper landing")?;
    wrong.offset_y = 2.9;
    let mut left = base.clone();
    left.width = 1.0;
    let mut right = base;
    right.x = 2.0;
    right.width = 1.0;
    level.floor_regions.extend([left, right]);
    crate::engine::escalator::validate(&level)?;
    level.floor_regions.last_mut().ok_or("right half")?.x = 2.02;
    assert!(
        crate::engine::escalator::validate(&level).is_err(),
        "a seam that exposes the lower-priority wrong height must fail"
    );
    Ok(())
}

#[test]
fn escalator_support_loses_to_a_higher_solid_even_within_numeric_tolerance() -> Result<(), String> {
    let level = level()?;
    let mut world = CollisionWorld::from_level(&level);
    world.walls.push(crate::engine::collision::WallAabb::with_y(
        1.6, -0.1, 0.85, 0.8, 0.100_05, 0.48,
    ));
    let mut game = Game::new(Vec3::new(2.0, EYE_HEIGHT + 0.000_05, 1.1), 0.0, world);
    game.set_app_state(AppState::Playing);
    toggle(&mut game);
    let mut physics = ItemWorld::default();
    for _ in 0_u8..10 {
        tick(&mut game, &mut physics);
    }
    let support = game
        .walking_support_at(2.0, 1.1, game.feet_y())
        .ok_or("solid top")?;
    assert!(
        game.grounded && support.escalator.is_none() && support.surface_y > 0.0,
        "normal winning contact must be the higher solid"
    );
    assert!(
        (game.player_position.z - 1.1).abs() < 1.0e-5,
        "the solid is not a tread"
    );
    Ok(())
}

#[test]
fn escalator_respects_item_pause_in_menu_and_noclip_and_never_overrides_solver_pause()
-> Result<(), String> {
    let level = level()?;
    let mut game = game(&level, Vec3::new(1.68, 0.0, 1.3));
    let mut items = ItemWorld::from_level(&level, &[]);
    items.spawn(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            ..PhysicsDef::default()
        },
        (Vec3::splat(-0.04), Vec3::splat(0.04)),
        Vec3::new(2.35, 0.84, 3.2),
        0.0,
        1.0,
    )?;
    toggle(&mut game);
    items.pause();
    game.sim_delta_seconds = 0.05;
    game.update_menu_world(&items);
    assert!(
        game.entities().escalator_frame_seconds().abs() < f32::EPSILON,
        "menu respects solver pause"
    );
    let _noclip = game.set_noclip(true)?;
    tick_at(&mut game, &mut items, 0.05);
    assert!(
        game.entities().escalator_frame_seconds().abs() < f32::EPSILON,
        "noclip respects solver pause"
    );
    items.sync_escalators(game.entities().escalators(), 0.1);
    items.step(0.1, game.physical_player_context(), game.door_colliders());
    assert_eq!(
        items.simulation_state().fixed_steps,
        0,
        "even a stale positive belt budget cannot bypass explicit solver pause"
    );
    Ok(())
}

#[test]
fn escalator_uses_one_capped_frame_budget_and_defers_power_changes() -> Result<(), String> {
    let level = level()?;
    let mut game = game(&level, Vec3::new(1.68, 0.0, 1.3));
    let mut items = ItemWorld::from_level(&level, &[]);
    items.spawn(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            ..PhysicsDef::default()
        },
        (Vec3::splat(-0.04), Vec3::splat(0.04)),
        Vec3::new(2.35, 0.84, 3.2),
        0.0,
        1.0,
    )?;
    tick(&mut game, &mut items);
    let [start, actor, _] = witnesses(&game, &items)?;
    let before_steps = items.simulation_state().fixed_steps;
    items.set_time_scale(4.0)?;
    game.set_world_time_scale(4.0)?;
    toggle(&mut game);
    tick_at(&mut game, &mut items, 1.0 / 30.0);
    let [player_after, actor_after, _] = witnesses(&game, &items)?;
    let belt = game.entities().escalators().first().ok_or("belt")?;
    let crate::engine::entity::PoseCue::Sample { progress, name: _ } = belt.cue() else {
        return Err("belt lacks exact visual pose".into());
    };
    assert!((progress - 0.2).abs() < 0.001, "0.1 s of visual motion");
    assert!(
        (player_after.z - start.z - 0.06).abs() < 0.001,
        "0.1 s of player carry"
    );
    assert!(
        (actor_after.z - actor.z - 0.06).abs() < 0.001,
        "0.1 s of idle NPC carry"
    );
    assert_eq!(
        items
            .simulation_state()
            .fixed_steps
            .saturating_sub(before_steps),
        12,
        "0.1 s of rigid-body steps"
    );
    toggle(&mut game);
    let latched = game.entities().escalators().first().ok_or("belt")?;
    assert!(
        !latched.running() && latched.powered_this_frame(),
        "late trigger cannot update only Rapier"
    );
    tick_at(&mut game, &mut items, 1.0 / 30.0);
    let next_frame = game.entities().escalators().first().ok_or("belt")?;
    assert!(
        !next_frame.powered_this_frame(),
        "new power commits at the following frame boundary"
    );
    Ok(())
}
