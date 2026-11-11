//! Protects actual locomotion override and reset-to-legacy behavior. One tiny
//! flat CPU fixture, 120 movement ticks per case; no renderer or baking.
use crate::engine::{game_mode::ModeInfo, level::LevelDef};
use crate::game::input::{Control, InputState, ScriptedHold};
use crate::game::{AppState, CollisionWorld, EYE_HEIGHT, Game, Settings};
use glam::Vec3;

#[test]
fn game_modes_override_player_speed_without_enabling_health_or_leaking_on_reset()
-> Result<(), String> {
    let legacy = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"mode","name":"Mode","spawn":{"x":15,"z":30},"rooms":[{"x":0,"z":0,"width":30,"depth":60,"height":3}]}"#).map_err(|error|error.to_string())?;
    let mut custom = legacy.clone();
    custom.game_mode = Some(ModeInfo {
        name: "Fast walking".into(),
        walk_speed: 6.0,
    });
    let spawn = Vec3::new(15.0, EYE_HEIGHT, 30.0);
    let mut game = Game::new(spawn, 0.0, CollisionWorld::from_level(&custom));
    let settings = Settings {
        walk_speed: 2.0,
        ..Settings::default()
    };
    for (level, speed) in [(&custom, 6.0_f32), (&legacy, 2.0_f32)] {
        game.reset_level(spawn, 0.0, CollisionWorld::from_level(level));
        game.set_app_state(AppState::Playing);
        let mut input = InputState::default();
        input.apply_move_script(
            &[ScriptedHold {
                control: Control::MoveForward,
                first_seconds: 0.0,
                last_seconds: 3.0,
            }],
            1.0,
        );
        for _ in 0_u8..120 {
            game.set_benchmark_simulation_delta(1.0 / 60.0);
            game.update_player_movement(&mut input, &settings);
        }
        assert!(
            (game.horizontal_velocity.length() - speed).abs() < 0.001,
            "mode overrides wish speed; legacy uses global setting after reset"
        );
        assert!(
            !game.health_settings().enabled,
            "movement never silently enables health"
        );
    }
    Ok(())
}
