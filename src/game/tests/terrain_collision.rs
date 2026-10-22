//! A tiny elevated ramp and stair flight protect airborne terrain contacts.
//! These CPU-only controller paths reproduce the rising-slope fall-through
//! without assets, a renderer or a full-map bake; a few hundred ticks suffice.

use glam::Vec3;

use crate::engine::level::LevelDef;
use crate::game::{CollisionWorld, EYE_HEIGHT, Game, HorizontalMode};

fn terrain_game() -> Result<Game, String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"terrain_contact","name":"Terrain contact",
        "spawn":{"x":2,"z":1},"rooms":[{"x":0,"z":0,"width":10,"depth":10,"height":14,"ceiling":{"kind":"open"}}],
        "ramps":[{"x":1,"z":2,"width":2,"depth":6,"offset_y":6.4,"rise":3.2}],
        "stairs":[{"x":5,"z":2,"width":2,"depth":6,"rise":3.2,"steps":16}]}"#,
    ).map_err(|error| error.to_string())?;
    Ok(Game::new(
        Vec3::new(2.0, 8.0, 1.0),
        0.0,
        CollisionWorld::from_level(&level),
    ))
}

#[test]
fn airborne_ascent_cannot_enter_the_ramp_when_its_surface_overtakes_the_feet() -> Result<(), String>
{
    for (along_x, rise) in [(false, 3.2_f32), (false, -3.2), (true, 3.2), (true, -3.2)] {
        for intervals in [
            [1.0 / 30.0; 3],
            [1.0 / 144.0; 3],
            [1.0 / 30.0, 1.0 / 144.0, 1.0 / 60.0],
        ] {
            let level = LevelDef::from_json(&serde_json::json!({
                "schema":"places-level", "format_version":1_u32,"id":"slope","name":"Slope",
                "spawn":{"x":0_u32,"z":0_u32},
                "rooms":[{"x":0_u32,"z":0_u32,"width":10_u32,"depth":10_u32,"height":14_u32,"ceiling":{"kind":"open"}}],
                "ramps":[{"x":if along_x {2_u32} else {1},"z":if along_x {1_u32} else {2},
                    "width":if along_x {6_u32} else {2},"depth":if along_x {2_u32} else {6},
                    "offset_y":if rise>0.0 {6.4_f32} else {9.6},"rise":rise}]
            }).to_string()).map_err(|error|error.to_string())?;
            let start = if rise > 0.0 { 3.0_f32 } else { 7.0 };
            let eye = if along_x {
                Vec3::new(start, 9.0, 2.0)
            } else {
                Vec3::new(2.0, 9.0, start)
            };
            let direction = if along_x { Vec3::X } else { Vec3::Z };
            let motion = Vec3::new(
                direction.x * rise.signum(),
                0.0,
                direction.z * rise.signum(),
            );
            let mut game = Game::new(eye, 0.0, CollisionWorld::from_level(&level));
            game.player_position = eye;
            game.feet_y = 9.0 - EYE_HEIGHT;
            game.grounded = false;
            for delta in intervals.into_iter().cycle().take(240) {
                game.move_horizontal(motion, 3.0, delta, HorizontalMode::Airborne);
                let _step = game.integrate_vertical_substep(delta);
                let surface = game
                    .floor
                    .height_at(game.player_position.x, game.player_position.z)
                    .ok_or("missing ramp support")?;
                assert!(
                    game.feet_y + 0.001 >= surface,
                    "rising terrain must never overtake the feet: along_x={along_x} rise={rise} feet={} surface={surface}",
                    game.feet_y
                );
                if game.grounded {
                    break;
                }
            }
            assert!(
                game.grounded,
                "the fall must land on the slope at constant or variable intervals"
            );
        }
    }
    Ok(())
}

#[test]
fn falling_at_stair_tread_and_side_edges_retains_real_support() -> Result<(), String> {
    for (x, z) in [(4.8, 4.0), (5.01, 4.249), (6.0, 7.99)] {
        let mut game = terrain_game()?;
        game.player_position = Vec3::new(x, 6.0, z);
        game.feet_y = 6.0 - EYE_HEIGHT;
        game.grounded = false;
        for _ in 0_u16..180 {
            let _step = game.integrate_vertical_substep(1.0 / 120.0);
            if game.grounded {
                break;
            }
        }
        assert!(
            game.grounded && game.feet_y > 1.0,
            "the feet disc must catch the actual tread even beyond the flight centre"
        );
    }
    Ok(())
}

// The original native Relay stair jump lost contact, fell through the flight
// and reset. Unlike the earlier vertical-contact tests, this drives ordinary
// held input, jump debounce, horizontal sweeps and recovery together.
#[test]
fn relay_stair_and_elevated_ramp_jump_keep_real_terrain_contact() -> Result<(), String> {
    use crate::game::input::{Control, InputState, ScriptedHold};
    use crate::game::{AppState, Settings, VERTICAL_SUBSTEP};
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"relay_contact","name":"Relay contact","spawn":{"x":1.25,"z":3},"rooms":[{"x":0,"z":0,"width":9,"depth":12,"height":14,"ceiling":{"kind":"open"}}],"stairs":[{"x":0,"z":2,"width":2.5,"depth":6,"rise":3.2,"steps":16}],"ramps":[{"x":4,"z":2,"width":2.5,"depth":6,"offset_y":6.4,"rise":3.2}]}"#).map_err(|error|error.to_string())?;
    for (x, eye) in [(1.25_f32, 2.2_f32), (5.25, 8.6)] {
        let mut game = Game::new(
            Vec3::new(x, eye, 3.0),
            std::f32::consts::PI,
            CollisionWorld::from_level(&level),
        );
        game.set_app_state(AppState::Playing);
        let script = [
            ScriptedHold {
                control: Control::MoveForward,
                first_seconds: 0.1,
                last_seconds: 1.7,
            },
            ScriptedHold {
                control: Control::Jump,
                first_seconds: 0.45,
                last_seconds: 0.5,
            },
        ];
        let mut command = InputState::default();
        for tick in 0_u16..145 {
            command.apply_move_script(&script, f32::from(tick) * VERTICAL_SUBSTEP);
            game.set_benchmark_simulation_delta(VERTICAL_SUBSTEP);
            game.update_player_movement(&mut command, &Settings::default());
            let surface = game
                .floor
                .height_at(game.player_position.x, game.player_position.z)
                .ok_or("missing fixture terrain")?;
            assert!(
                game.feet_y() + 0.001 >= surface,
                "ordinary Relay jump must not fall into its terrain: x={x} tick={tick} feet={} surface={surface}",
                game.feet_y()
            );
        }
        assert!(
            game.player_position.z > 5.0 && game.reset_count() == 0,
            "the jump must progress up the unchanged flight/ramp without recovery"
        );
    }
    Ok(())
}
