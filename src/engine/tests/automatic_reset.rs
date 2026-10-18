//! Three tiny Luau handlers protect reset propagation and tail cancellation.
//! No assets, device, bake or timers; this adds only milliseconds of VM work.

use crate::engine::level::LevelDef;
use crate::engine::project::ProjectPath;
use crate::engine::scripting::ScriptSource;
use crate::game::input::InputState;
use crate::game::settings::Settings;
use crate::game::{AppState, CollisionWorld, EYE_HEIGHT, Game};
use glam::Vec3;

fn game(handler: &str) -> Result<Game, String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"reset","name":"Reset","spawn":{"x":0,"z":0},"rooms":[{"x":-3,"z":-3,"width":6,"depth":6}]}"#).map_err(|error| error.to_string())?;
    level.script_sources.push(ScriptSource {
        path: ProjectPath::new("scripts/reset.luau".into())?,
        source: format!("return {{{handler}=function(api) api.message('Discarded',{{'before reset'}}); api.reset() end, {}=function(api) api.message('Stale',{{'must not run'}}) end}}", if handler == "start" { "update" } else { "start" }),
    });
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    game.set_app_state(AppState::Playing);
    game.set_benchmark_simulation_delta(0.016);
    Ok(game)
}

#[test]
fn automatic_and_hint_resets_reach_shared_lifecycle_and_cancel_tail() -> Result<(), String> {
    for handler in ["start", "update"] {
        let mut game = game(handler)?;
        game.player_position.x = 1.0;
        game.update_player_movement(&mut InputState::default(), &Settings::default());
        assert_eq!(
            game.reset_count(),
            1,
            "{handler} reset must reach the player once"
        );
        assert_eq!(
            game.world_reset_count(),
            1,
            "{handler} must use the shared reset lifecycle"
        );
        assert_eq!(
            game.last_world_tick().actions_run,
            if handler == "start" { 2 } else { 3 },
            "reset stops the remaining automatic callbacks"
        );
        assert!(
            game.player_position.x.abs() < f32::EPSILON,
            "{handler} restores authored spawn"
        );
        assert!(
            game.entities_mut().take_commands().is_empty(),
            "reset cancels staged guidance and later callbacks"
        );
        game.set_benchmark_simulation_delta(0.016);
        game.update_player_movement(&mut InputState::default(), &Settings::default());
        assert_eq!(
            game.reset_count(),
            2,
            "reset re-arms without recursive callbacks in one frame"
        );
    }
    let mut game = game("hint")?;
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    let _initial_guidance = game.entities_mut().take_commands();
    game.request_hint();
    assert_eq!(
        game.world_reset_count(),
        1,
        "hint reset must reach the shared lifecycle"
    );
    assert!(
        game.entities_mut().take_commands().is_empty(),
        "hint reset clears pre-reset presentation commands"
    );
    Ok(())
}
