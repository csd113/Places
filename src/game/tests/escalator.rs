//! One metadata-only ride protects the distinct custom player and NPC movers
//! and Rapier support contacts from visual-only or volume-based carry. The
//! loose rigid body, idle agent and no-input controller share one power event.

use glam::Vec3;

use crate::engine::entity::PoseCue;
use crate::engine::escalator::Direction;
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

// Fit uses the exact collision solids, not a visual approximation of an
// opening. This tiny scene needs neither a rendered window nor a bake.
#[test]
fn escalator_fit_refuses_solid_glazing_and_void_boxes_but_allows_passable_ones()
-> Result<(), String> {
    use crate::engine::escalator::Fit;

    let mut level = level()?;
    level.props.clear();
    let lower = Vec3::new(2.0, 0.0, 1.0);
    let upper = Vec3::new(2.0, 3.0, 8.2);
    let glass = serde_json::from_str(r#"{"x":0,"z":3,"width":5,"depth":0.2,"height":6,"openings":[{"offset":1,"width":2,"height":5,"glass":"core:glass_window_dirty_01"}]}"#).map_err(|error| error.to_string())?;
    level.walls.push(glass);
    assert!(
        Fit::connect(&level, lower, upper).is_ok(),
        "a passable pane is still an opening"
    );
    level
        .walls
        .first_mut()
        .and_then(|wall| wall.openings.first_mut())
        .ok_or("pane")?
        .solid = true;
    assert!(
        Fit::connect(&level, lower, upper).is_err(),
        "solid glazing closes that opening"
    );
    level.walls.clear();
    let shell = serde_json::from_str(
        r#"{"min":[1,0,3],"max":[3,3,3.2],"material":"core:concrete","solid":false}"#,
    )
    .map_err(|error| error.to_string())?;
    level.void_walls.push(shell);
    assert!(
        Fit::connect(&level, lower, upper).is_ok(),
        "a sight-only void box is not a physical blocker"
    );
    level.void_walls.first_mut().ok_or("box")?.solid = true;
    assert!(
        Fit::connect(&level, lower, upper).is_err(),
        "solid void geometry blocks the actual belt"
    );
    Ok(())
}

fn atrium() -> Result<LevelDef, String> {
    // Two ordinary Room forms can author this geometry: the lower room has an
    // explicitly open roof and its nominal height ends below the raised room.
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"atrium_test","name":"Atrium","spawn":{"x":1,"z":2},"rooms":[
      {"x":0,"z":0,"width":8,"depth":12,"height":2.7,"ceiling":{"kind":"open"}},
      {"x":1,"z":7.6,"width":6,"depth":3.5,"floor_y":3,"height":3}],
      "props":[
        {"id":"up","model":"mall:escalator","x":2,"z":1,"occludes":false,"escalator":{"initial_running":true,"placement":"connected"},"components":[{"component":"animation","clip":"escalator_run","looped":true}]},
        {"id":"down","model":"mall:escalator","x":5,"z":1,"occludes":false,"escalator":{"initial_running":true,"placement":"connected","direction":"down"},"components":[{"component":"animation","clip":"escalator_run","looped":true}]}]}"#).map_err(|error| error.to_string())
}

// Adjacent lanes share a real upper room without replacing the ground below.
// Ride the actual compiled collision and cross underneath with the controller.
#[test]
fn escalator_open_atrium_keeps_both_lanes_and_the_lower_passage() -> Result<(), String> {
    use crate::engine::escalator::Fit;
    use crate::engine::package::collision::{CompiledCollision, read_collision, write_collision};
    use crate::game::input::{Control, ScriptedHold};

    let mut level = atrium()?;
    crate::engine::loader::validate_level(&level)?;
    let compiled = write_collision(&CompiledCollision::from_level(&level))?;
    let spawn = |feet: Vec3| -> Result<Game, String> {
        let world = CollisionWorld::from_compiled(&level, read_collision(&compiled)?, None);
        let mut game = Game::new(Vec3::new(feet.x, feet.y + EYE_HEIGHT, feet.z), 0.0, world);
        game.set_app_state(AppState::Playing);
        Ok(game)
    };
    for (z, reference) in [(7.1_f32, "under incline"), (8.5_f32, "under deck")] {
        let mut game = spawn(Vec3::new(0.9, 0.0, z))?;
        let mut input = InputState::default();
        input.apply_move_script(
            &[ScriptedHold {
                control: Control::StrafeRight,
                first_seconds: 0.0,
                last_seconds: 2.0,
            }],
            1.0,
        );
        for _ in 0_u8..125 {
            game.sim_delta_seconds = VERTICAL_SUBSTEP;
            let _origin = game.update_playing_frame(&input, &Settings::default(), (0.0, 0.0));
            if game.trigger_feet().x >= 6.2 {
                break;
            }
        }
        let feet = game.trigger_feet();
        assert!(
            feet.x > 5.8 && feet.y.abs() < 0.01,
            "lower route {reference} stays walkable: {feet:?}"
        );
    }
    for (start, landing, comb, sign) in [
        (Vec3::new(2.0, 0.0, 1.3), 3.0_f32, 8.19_f32, 1.0_f32),
        (Vec3::new(5.0, 3.0, 8.1), 0.0, 1.01, -1.0),
    ] {
        let mut game = spawn(start)?;
        let mut items = ItemWorld::from_level(&level, &[]);
        for _ in 0_u16..1_550 {
            tick(&mut game, &mut items);
        }
        let feet = game.trigger_feet();
        assert!(
            (feet.z - comb) * sign >= 0.0 && (feet.y - landing).abs() < 0.02,
            "carried to the correct deck: {feet:?}"
        );
    }
    verify_atrium_floor_render(&level);
    level.props.clear();
    let lower = Vec3::new(2.0, 0.0, 1.0);
    let upper = Vec3::new(2.0, 3.0, 8.2);
    assert!(
        Fit::connect(&level, lower, upper).is_ok(),
        "editor fits the actual elevated carrier"
    );
    level.rooms.first_mut().ok_or("lower room")?.ceiling =
        crate::engine::level::CeilingProfileDef::Flat;
    assert!(
        Fit::connect(&level, lower, upper).is_err(),
        "a real low roof blocks the ascent"
    );
    Ok(())
}

fn verify_atrium_floor_render(level: &LevelDef) {
    use crate::engine::render::{SurfaceKind, build_level_geometry_with_catalog};
    let mesh =
        build_level_geometry_with_catalog(level, &crate::engine::loader::PropCatalog::builtin());
    let floor_at = |point: Vec3| {
        mesh.ranges
            .iter()
            .filter(|range| range.key.kind == SurfaceKind::Floor)
            .any(|range| {
                range.indices.as_chunks::<3>().0.iter().any(|indices| {
                    let mut bounds = crate::engine::spatial::Aabb::EMPTY;
                    for &index in indices {
                        let Some(vertex) = range.vertices.get(usize::from(index)) else {
                            return false;
                        };
                        bounds.expand(vertex.pos);
                    }
                    let [x, y, z] = point.to_array();
                    let [x0, y0, z0] = bounds.min;
                    let [x1, y1, z1] = bounds.max;
                    (x0..=x1).contains(&x)
                        && (z0..=z1).contains(&z)
                        && (y - y0).abs() < 0.001
                        && (y - y1).abs() < 0.001
                })
            })
    };
    for z in [7.1, 7.9, 8.6] {
        assert!(
            floor_at(Vec3::new(2.0, 0.0, z)),
            "lower ground must still be rendered under z={z}"
        );
    }
    assert!(
        !floor_at(Vec3::new(2.0, 3.0, 7.9)),
        "metal comb owns the replaced upper surface only"
    );
    assert!(
        floor_at(Vec3::new(2.0, 3.0, 8.6)),
        "the rest of the upper walkway stays rendered"
    );
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

fn add_loose_witness(items: &mut ItemWorld, position: Vec3) -> Result<(), String> {
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
        position,
        0.0,
        1.0,
    )
}

#[test]
fn escalator_support_rides_stops_resumes_and_dismounts() -> Result<(), String> {
    ride(
        &level()?,
        Vec3::new(1.68, 0.0, 1.3),
        Vec3::new(2.22, 0.736, 3.0),
        Direction::Up,
    )
}

// Reuse the headless ride to protect reverse-only failures: an upper boarding
// seam, actual descent to the lower dismount and a wrapping published pose.
// The same three riders share a tiny metadata-only room and physics clock.
#[test]
fn escalator_down_descends_dismounts_and_wraps_its_published_pose() -> Result<(), String> {
    let mut level = level()?;
    level
        .props
        .first_mut()
        .and_then(|prop| prop.escalator.as_mut())
        .ok_or("escalator definition")?
        .direction = Direction::Down;
    let rider = level.props.get_mut(1).ok_or("NPC definition")?;
    rider.y = 0.0;
    rider.z = 8.1;
    ride(
        &level,
        Vec3::new(1.68, 3.0, 8.1),
        Vec3::new(2.22, 3.002, 8.0),
        Direction::Down,
    )
}

// Airborne items do not receive belt traction. This short, deterministic
// trial adds coverage distinct from the resting/dormant rider above.
#[test]
fn escalator_carries_a_dropped_prop_only_after_contact_in_both_directions() -> Result<(), String> {
    for (direction, sign, start, comb) in [
        (Direction::Up, 1.0_f32, 7.8_f32, 8.2_f32),
        (Direction::Down, -1.0_f32, 1.4_f32, 1.0_f32),
    ] {
        let mut level = level()?;
        let definition = level
            .props
            .first_mut()
            .and_then(|prop| prop.escalator.as_mut())
            .ok_or("belt")?;
        definition.direction = direction;
        definition.initial_running = true;
        let mut game = game(&level, Vec3::new(4.5, 0.0, 5.5));
        let mut items = ItemWorld::from_level(&level, &level.item_collision_aabbs());
        let floor_y = if sign > 0.0 { 3.0 } else { 0.0 };
        items.spawn(
            1,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                ..PhysicsDef::default()
            },
            (Vec3::splat(-0.045), Vec3::splat(0.045)),
            Vec3::new(2.03, floor_y + 0.60, start),
            0.0,
            1.0,
        )?;
        for _ in 0_u8..12 {
            tick(&mut game, &mut items);
        }
        let air = items
            .body_pose_velocity(1)
            .ok_or("airborne prop")?
            .0
            .position;
        assert!(
            (air.z - start).abs() < 0.001 && air.y > floor_y + 0.2,
            "no belt traction before {direction:?} contact: {air:?}"
        );
        for _ in 0_u16..420 {
            tick(&mut game, &mut items);
        }
        let landed = items.body_pose_velocity(1).ok_or("landed prop")?.0.position;
        assert!(
            (landed.z - comb) * sign >= -0.01 && (landed.y - floor_y).abs() < 0.1,
            "contact carries the prop to the {direction:?} exit: {landed:?}"
        );
    }
    Ok(())
}

// Each free belt has its own landing, not an invisible extension of the room
// floor. Verify two scales in prepared collision and the real installed rig.
#[test]
fn escalator_free_rig_and_compiled_belt_share_both_integral_landings() -> Result<(), String> {
    use crate::engine::escalator::{Dimensions, Placement};
    use crate::engine::package::collision::{CompiledCollision, read_collision, write_collision};
    use crate::engine::props::PropAssets;
    let mut assets =
        PropAssets::with_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"));
    for (rise, direction) in [(1.35_f32, Direction::Up), (3.75_f32, Direction::Down)] {
        let mut level = level()?;
        level.floor_regions.clear();
        let room = level.rooms.first_mut().ok_or("room")?;
        room.height = 8.0;
        room.depth = 14.0;
        let prop = level.props.first_mut().ok_or("belt")?;
        prop.y = 0.35;
        let definition = prop.escalator.as_mut().ok_or("esc")?;
        definition.placement = Placement::Free;
        definition.rise = rise;
        definition.direction = direction;
        definition.initial_running = true;
        let rig = assets.resolve_placement("environments/mall/props/escalator.glb", prop)?;
        let dimensions = Dimensions::new(rise)?;
        let (_, [_, max_y, max_z]) = rig.model.bounds().ok_or("fitted geometry")?;
        assert!(
            (max_z - dimensions.length).abs() < 0.001 && (max_y - rise - 0.997).abs() < 0.001,
            "the shell's crest and rail must land at the derived run and rise"
        );
        let statics = read_collision(&write_collision(&CompiledCollision::from_level(&level))?)?;
        let world = CollisionWorld::from_compiled(&level, statics, None);
        let z = 1.0 + dimensions.length - 0.15;
        let underside = world
            .floor
            .height_below(2.0, z, 0.1)
            .ok_or("floor still exists below rig")?;
        assert!(underside.abs() < 0.001, "no architectural floor hole");
        let upper = world
            .floor
            .height_below(2.0, z, 8.0)
            .ok_or("integral plate")?;
        assert!(
            (upper - rise - 0.35).abs() < 0.001,
            "upper plate is physical without a building floor"
        );
        let mut game = Game::new(Vec3::new(2.0, upper + EYE_HEIGHT, z), 0.0, world);
        game.set_app_state(AppState::Playing);
        let mut items = ItemWorld::from_level(&level, &[]);
        for _ in 0_u8..72 {
            tick(&mut game, &mut items);
        }
        let result = game.trigger_feet();
        if direction == Direction::Down {
            assert!(
                result.z < z - 0.3,
                "reverse movement reaches the incline: {result:?}"
            );
            for _ in 0_u8..75 {
                tick(&mut game, &mut items);
            }
            assert!(
                game.trigger_feet().y < upper - 0.05,
                "the fitted crest supports actual descent"
            );
        } else {
            assert!(
                result.z > z + 0.14 && (result.y - upper).abs() < 0.02,
                "a stopped rider stays on the free end plate: {result:?}"
            );
        }
    }
    Ok(())
}

fn published_phase(game: &Game) -> Result<f32, String> {
    let Some(PoseCue::Sample { progress, name }) = game
        .entities()
        .entity_frames()
        .iter()
        .find(|frame| frame.instance_id == "lift")
        .and_then(|frame| frame.cue.as_ref())
    else {
        return Err("belt lacks exact visual pose".into());
    };
    assert_eq!(name, crate::engine::escalator::CLIP);
    Ok(*progress)
}

fn ride(level: &LevelDef, feet: Vec3, prop: Vec3, direction: Direction) -> Result<(), String> {
    let (sign, landing_y, comb_z) = match direction {
        Direction::Up => (1.0_f32, 3.0, 8.2),
        Direction::Down => (-1.0_f32, 0.0, 1.0),
    };
    let mut game = game(level, feet);
    let mut items = ItemWorld::from_level(level, &level.item_collision_aabbs());
    add_loose_witness(&mut items, prop)?;
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
    tick(&mut game, &mut items);
    let phase_delta = sign * VERTICAL_SUBSTEP * 2.0;
    let phase_after_tick = phase_delta.rem_euclid(1.0);
    assert!(
        (published_phase(&game)? - phase_after_tick).abs() < 0.001,
        "published phase advances in the belt direction and wraps at zero"
    );
    for _ in 0_u8..172 {
        tick(&mut game, &mut items);
    }
    let before_stop = witnesses(&game, &items)?;
    assert!(
        !items.body_is_dormant(1),
        "real powered tread contact wakes the loose prop"
    );
    for (start, rider) in initial.into_iter().zip(before_stop) {
        assert!(
            (rider.z - start.z) * sign > 0.60 && (rider.y - start.y) * sign > 0.12,
            "a supported rider must travel {direction:?} on the incline: {start:?} -> {rider:?}"
        );
    }
    toggle(&mut game);
    let stopped_phase = published_phase(&game)?;
    for _ in 0_u8..100 {
        tick(&mut game, &mut items);
    }
    let stopped = witnesses(&game, &items)?;
    assert!(
        (published_phase(&game)? - stopped_phase).abs() < f32::EPSILON,
        "stopping holds the last visible pose"
    );
    for (previous, rider) in before_stop.into_iter().zip(stopped) {
        assert!(
            previous.distance(rider) < 0.08,
            "stopped belt cannot creep: {previous:?} -> {rider:?}"
        );
    }
    toggle(&mut game);
    tick(&mut game, &mut items);
    assert!(
        (published_phase(&game)? - (stopped_phase + phase_delta).rem_euclid(1.0)).abs() < 0.001,
        "restart resumes the held pose in the authored direction"
    );
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
            (rider.z - comb_z) * sign >= -0.01 && (rider.y - landing_y).abs() < 0.10,
            "rider left the tread onto the true {direction:?} landing: {rider:?}"
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
    verify_phase_after_reset(&mut game, &mut items, phase_after_tick)
}

fn verify_phase_after_reset(
    game: &mut Game,
    items: &mut ItemWorld,
    next_phase: f32,
) -> Result<(), String> {
    game.entities_mut().reset_runtime();
    assert!(
        published_phase(game)?.abs() < f32::EPSILON,
        "reset returns to the neutral pose"
    );
    toggle(game);
    tick(game, items);
    assert!(
        (published_phase(game)? - next_phase).abs() < 0.001,
        "reset preserves the authored direction"
    );
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
