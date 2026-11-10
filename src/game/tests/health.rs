//! Small CPU regressions for player ownership, rules replacement and HUD parity.
//! Unique value: shared state tests do not exercise pause/reset/checkpoint/HUD paths.
//! One empty world and tiny UI geometry, no assets, GPU, sleeps or map baking.
use crate::engine::health::{HealthAction, HealthSettings, HealthTarget};
use crate::game::{AppState, CollisionWorld, EYE_HEIGHT, Game};
use glam::Vec3;

// Unique opt-in regression: legacy maps/checkpoints must not simulate health,
// show hearts, accept damage, or inherit a movement lock from disabled death.
// One tiny CPU room, a few JSON bytes and one movement step; no assets or bake.
#[test]
fn ordinary_maps_keep_health_dormant_through_legacy_checkpoints() -> Result<(), String> {
    use crate::game::input::{Control, InputState, ScriptedHold};
    let level = crate::engine::level::LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"walk","name":"Walk","spawn":{"x":2,"z":2},"rooms":[{"width":8,"depth":8}]}"#,
    )
    .map_err(|problem| problem.to_string())?;
    let mut game = Game::new(
        crate::game::spawn_position(&level),
        0.0,
        CollisionWorld::from_level(&level),
    );
    assert!(!game.health_settings().enabled, "legacy maps opt out");
    let dormant = *game.player_health();
    game.set_app_state(AppState::Playing);
    game.set_benchmark_simulation_delta(0.1);
    game.update_player_movement(
        &mut InputState::default(),
        &crate::game::settings::Settings::default(),
    );
    assert_eq!(
        *game.player_health(),
        dormant,
        "disabled clocks do not tick"
    );
    assert!(
        game.apply_health_action(&HealthTarget::Player, HealthAction::Damage { hp: 6 })
            .is_err(),
        "disabled health rejects damage without mutation"
    );
    assert!(
        game.entities_mut()
            .dispatch_health(&HealthTarget::Player, HealthAction::Damage { hp: 1 })
            .is_err(),
        "disabled health does not enqueue authored player damage"
    );
    let mut legacy =
        serde_json::to_value(game.checkpoint_player()?).map_err(|problem| problem.to_string())?;
    assert!(
        legacy.get("health").is_none(),
        "disabled saves omit dormant HP"
    );
    drop(
        legacy
            .as_object_mut()
            .ok_or("Invalid player record")?
            .remove("health_rules"),
    );
    let player = serde_json::from_value(legacy).map_err(|problem| problem.to_string())?;
    game.restore_checkpoint_player(&player)?;
    assert!(
        !game.health_settings().enabled,
        "old saves cannot enable health"
    );
    game.set_health_settings(enabled_settings())?;
    game.apply_health_action(&HealthTarget::Player, HealthAction::Damage { hp: 6 })?;
    game.set_health_settings(HealthSettings::default())?;
    let disabled_dead = *game.player_health();
    let before = game.player_position;
    let mut input = InputState::default();
    input.apply_move_script(
        &[ScriptedHold {
            control: Control::MoveForward,
            first_seconds: 0.0,
            last_seconds: 1.0,
        }],
        0.0,
    );
    game.set_benchmark_simulation_delta(0.1);
    game.update_player_movement(&mut input, &crate::game::settings::Settings::default());
    assert!(
        game.player_position.z < before.z,
        "disabled death cannot block walking"
    );
    assert_eq!(
        *game.player_health(),
        disabled_dead,
        "disabled state stays frozen"
    );
    let mut vertices = Vec::new();
    crate::game::ui::append_player_health(
        &mut vertices,
        game.player_health(),
        game.health_settings(),
    );
    assert!(vertices.is_empty(), "disabled maps add no health display");
    Ok(())
}

#[test]
fn player_health_obeys_pause_rules_and_explicit_reset() -> Result<(), String> {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_health_settings(HealthSettings {
        regeneration: true,
        regeneration_hp_per_second: 10.0,
        regeneration_delay_seconds: 0.0,
        ..enabled_settings()
    })?;
    game.apply_health_action(&HealthTarget::Player, HealthAction::Damage { hp: 3 })?;
    game.set_benchmark_simulation_delta(0.1);
    game.update_player_movement(
        &mut crate::game::input::InputState::default(),
        &crate::game::settings::Settings::default(),
    );
    assert_eq!(
        game.player_health().current_hp(),
        3,
        "menu frames cannot heal the player"
    );
    game.set_app_state(AppState::Playing);
    game.set_benchmark_simulation_delta(0.1);
    game.update_player_movement(
        &mut crate::game::input::InputState::default(),
        &crate::game::settings::Settings::default(),
    );
    assert_eq!(
        game.player_health().current_hp(),
        4,
        "playing advances map-configured regeneration"
    );
    game.set_health_settings(HealthSettings {
        health_kits: false,
        ..enabled_settings()
    })?;
    assert!(
        game.apply_health_action(&HealthTarget::Player, HealthAction::HealthKit { hp: 2 })
            .is_err(),
        "disabled kits cannot heal"
    );
    assert_eq!(
        game.player_health().current_hp(),
        4,
        "replacing rules does not replace health"
    );
    game.set_benchmark_player_position(Vec3::new(1.0, EYE_HEIGHT, 1.0), 0.0, 0.0);
    assert_eq!(
        game.player_health().current_hp(),
        4,
        "repositioning is not respawn"
    );
    game.apply_health_action(&HealthTarget::Player, HealthAction::Damage { hp: 6 })?;
    assert!(
        game.player_health().is_dead(),
        "lethal damage enters terminal state"
    );
    game.apply_health_action(&HealthTarget::Player, HealthAction::Heal { hp: 6 })?;
    assert!(
        game.player_health().is_dead(),
        "heal is not an implicit revive"
    );
    game.reset_to_spawn();
    assert_eq!(
        game.player_health().current_hp(),
        6,
        "explicit run reset restores player"
    );
    Ok(())
}

#[test]
fn health_hud_preserves_half_hearts_and_optional_display() -> Result<(), String> {
    let health =
        crate::engine::health::HealthState::new(5, 6).map_err(|problem| problem.to_string())?;
    let mut vertices = Vec::new();
    crate::game::ui::append_player_health(&mut vertices, &health, &enabled_settings());
    assert_eq!(
        vertices.iter().filter(|v| v.color[0] > 0.9).count(),
        30,
        "five separately filled halves"
    );
    assert_eq!(
        vertices.iter().filter(|v| v.color[0] < 0.2).count(),
        6,
        "one unfilled half"
    );
    vertices.clear();
    crate::game::ui::append_player_health(
        &mut vertices,
        &health,
        &HealthSettings {
            show_player_health: false,
            ..enabled_settings()
        },
    );
    assert!(vertices.is_empty(), "hidden health adds no HUD geometry");
    let odd =
        crate::engine::health::HealthState::new(5, 5).map_err(|problem| problem.to_string())?;
    crate::game::ui::append_player_health(&mut vertices, &odd, &enabled_settings());
    assert_eq!(
        vertices.len(),
        30,
        "an odd maximum has no nonexistent empty half"
    );
    vertices.clear();
    let large = crate::engine::health::HealthState::new(60_000, 65_535)
        .map_err(|problem| problem.to_string())?;
    crate::game::ui::append_player_health(&mut vertices, &large, &enabled_settings());
    assert!(
        vertices.len() < 200,
        "large maxima use an exact, bounded numeric display"
    );
    Ok(())
}

#[test]
fn health_console_rejects_ambiguous_or_fractional_hp() {
    for invalid in [
        "/health player damage 0",
        "/health player damage 1.5",
        "/health player kit NaN",
        "/health player damage -1",
        "/health player heal 65536",
        "/health player revive 6",
    ] {
        assert!(
            crate::game::console::command::parse(invalid).is_err(),
            "invalid action must be refused: {invalid}"
        );
    }
    assert!(
        crate::game::console::command::parse("/health player damage 1").is_ok(),
        "one HP remains half a heart"
    );
}

fn enabled_settings() -> HealthSettings {
    HealthSettings {
        enabled: true,
        ..HealthSettings::default()
    }
}
