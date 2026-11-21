//! Essential input/lifecycle and numerical regressions. Tiny CPU rooms only;
//! these cover tool ownership that neither health nor carrying tests exercise.
use super::ray_entry;
use crate::{
    engine::{level::LevelDef, spatial::Aabb, tools::ToolSettings},
    game::{CollisionWorld, Game},
};
use glam::Vec3;

fn game_with_tools(enabled: bool) -> Result<Game, String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"tools","name":"Tools","spawn":{"x":2,"z":2},"rooms":[{"width":8,"depth":8}]}"#)
        .map_err(|error| error.to_string())?;
    level.tools.crowbar = enabled;
    Ok(Game::new(
        crate::game::spawn_position(&level),
        0.0,
        CollisionWorld::from_level(&level),
    ))
}

// Native regression: a readback/frame stall previously caused immediate contact
// by using raw elapsed time. One tiny CPU world proves the adapter clock is the
// same bounded simulation delta as movement/health, at negligible resource cost.
#[test]
fn tool_frame_uses_the_shared_clamped_world_clock_after_a_stall() -> Result<(), String> {
    let mut game = game_with_tools(true)?;
    game.set_benchmark_simulation_delta(1.0);
    assert!(game.start_tool_swing());
    assert!(
        !game.advance_tool_frame(),
        "a stalled frame cannot skip windup"
    );
    assert!(
        game.tool_progress() < 0.32,
        "clamped movement time drives the tool"
    );
    assert!(
        game.advance_tool_frame(),
        "normal bounded advancement reaches contact"
    );
    Ok(())
}

// Protects opt-in inventory, a consumed click during recovery, and explicit reset.
// No renderer, clocks, native input or physics allocations; sub-millisecond work.
#[test]
fn map_inventory_has_one_unbuffered_swing_and_resets_cleanly() -> Result<(), String> {
    let mut walking = game_with_tools(false)?;
    assert!(
        !walking.toggle_tool() && !walking.start_tool_swing(),
        "legacy maps grant no tool"
    );
    assert!(
        !walking.health_settings().enabled,
        "inventory cannot enable health"
    );
    let mut game = game_with_tools(true)?;
    assert!(
        game.tool_equipped() && game.start_tool_swing(),
        "granted crowbar starts drawn"
    );
    assert!(
        !game.start_tool_swing(),
        "recovery never queues another swing"
    );
    assert!(!game.advance_tool(0.1));
    assert!(game.advance_tool(0.1), "one physical contact phase");
    assert!(
        !game.advance_tool(1.0) && !game.advance_tool(1.0),
        "no automatic repeat"
    );
    assert_eq!(
        game.tool_snapshot()
            .get("swings")
            .and_then(serde_json::Value::as_u64),
        Some(1_u64)
    );
    assert!(game.toggle_tool() && !game.start_tool_swing(), "1 holsters");
    assert!(game.toggle_tool() && game.start_tool_swing(), "1 redraws");
    // Regression: animation cancellation must never bypass authored swing speed.
    assert!(!game.advance_tool(0.1));
    assert!(game.toggle_tool() && game.toggle_tool());
    assert!(
        !game.start_tool_swing(),
        "a rapid holster/redraw retains recovery"
    );
    game.cancel_tool_swing();
    assert!(
        !game.start_tool_swing(),
        "menu/focus cancellation retains recovery"
    );
    assert!(
        !game.advance_tool(1.0),
        "a cancelled windup cannot hit later"
    );
    assert!(
        game.start_tool_swing(),
        "recovery eventually permits a fresh click"
    );
    let _respawned = game.developer_respawn()?;
    assert!(
        game.tool_ready(),
        "respawn clears recovery and restores map inventory"
    );
    assert!(
        !game.health_settings().enabled,
        "crowbar never enables map health"
    );
    Ok(())
}

// Protects reach/parallel-ray boundaries and hostile authored tuning. Pure tiny
// numerical fixtures, no duplicate map bake or expensive broad scene sweep.
#[test]
fn tool_tuning_and_hitbox_boundaries_reject_invalid_values() {
    let bounds = Aabb {
        min: [-0.2, 1.0, -2.0],
        max: [0.2, 2.0, -1.5],
    };
    assert!(ray_entry(bounds, Vec3::new(0.0, 1.6, 0.0), Vec3::NEG_Z, 1.49).is_none());
    assert_eq!(
        ray_entry(bounds, Vec3::new(0.0, 1.6, 0.0), Vec3::NEG_Z, 1.5),
        Some(1.5)
    );
    assert!(ray_entry(bounds, Vec3::new(0.3, 1.6, 0.0), Vec3::NEG_Z, 4.0).is_none());
    for reach in [f32::NAN, f32::INFINITY, 0.0, 4.01] {
        assert!(
            ToolSettings {
                reach_m: reach,
                ..ToolSettings::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        ToolSettings {
            damage_hp: 0,
            ..ToolSettings::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        ToolSettings {
            swings_per_second: 0.0,
            ..ToolSettings::default()
        }
        .validate()
        .is_err()
    );
}

// Two one-edge releases exercise the shared game and Quick Test crowbar
// command at rest and before the item solver on takeoff. The blocked variant
// also verifies that clearance failure never consumes the tool swing.
#[test]
fn attack_drops_the_carried_prop_and_starts_a_swing_on_the_same_input() -> Result<(), String> {
    use crate::engine::physics::{BodyMotion, ItemWorld, PhysicsDef, PlayerContext, ShapeDef};
    use crate::game::input::{Control, InputState, ScriptedHold};
    use crate::game::{AppState, Settings, VERTICAL_SUBSTEP};

    for takeoff in [false, true] {
        let mut game = game_with_tools(true)?;
        game.set_app_state(AppState::Playing);
        let mut world = ItemWorld::default();
        let player = game.physical_player_context();
        let center = player.eye + Vec3::NEG_Z;
        world.spawn(
            1,
            &PhysicsDef {
                motion: BodyMotion::Dynamic,
                pickup: true,
                shape: Some(ShapeDef::Sphere { radius: 0.1 }),
                ..PhysicsDef::default()
            },
            (Vec3::splat(-0.1), Vec3::splat(0.1)),
            center,
            0.0,
            1.0,
        )?;
        world.sync_context(player, &[]);
        assert!(
            world.pickup(1, player.eye, player.view, player.yaw),
            "fixture must be carried"
        );

        if takeoff {
            let mut input = InputState::default();
            input.apply_move_script(
                &[ScriptedHold {
                    control: Control::Jump,
                    first_seconds: 0.0,
                    last_seconds: 1.0,
                }],
                0.5,
            );
            game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
            game.update_player_movement_with_items(&mut input, &Settings::default(), &mut world);
            world.sync_context(
                PlayerContext {
                    feet: Vec3::new(center.x, player.feet.y, center.z),
                    eye: center,
                    ..player
                },
                &[],
            );
            assert!(
                super::begin_swing(&mut game, &mut world).is_err(),
                "player penetration must block the swing"
            );
            assert_eq!(world.held(), Some(1));
            assert_eq!(game.tool_progress(), 0.0);
        }

        let accepted = game.physical_player_context();
        world.sync_context(accepted, &[]);
        assert!(
            super::begin_swing(&mut game, &mut world)?,
            "clear release must start the swing"
        );
        assert_eq!(world.held(), None);
        let (_, actual) = world.body_pose_velocity(1).ok_or("missing released prop")?;
        let expected = ((accepted.feet - player.feet) / VERTICAL_SUBSTEP).clamp_length_max(2.0);
        assert_eq!(expected.y > 0.0, takeoff);
        assert!(
            actual.distance(expected) < 1.0e-5,
            "actual {actual:?}, expected {expected:?}"
        );
        assert_eq!(
            game.tool_snapshot()
                .get("swings")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
    }
    Ok(())
}
