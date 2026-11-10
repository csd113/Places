//! Essential health regressions: integer caps/death and independent kit rules,
//! fractional regeneration across delay boundaries, and persisted entity health
//! with stale-handle rejection. Tiny CPU fixtures; no assets, bake or GPU.
use crate::engine::health::{HealthAction, HealthError, HealthSettings, HealthState};

#[test]
fn caps_terminal_death_and_kit_rules_are_independent() -> Result<(), HealthError> {
    let mut state = HealthState::default();
    let mut settings = enabled_settings();
    let _damage = state.apply(HealthAction::Damage { hp: 3 }, &settings)?;
    settings.health_kits = false;
    assert_eq!(
        state.apply(HealthAction::HealthKit { hp: 2 }, &settings),
        Err(HealthError::DisabledHealthKit),
        "disabled pickups cannot heal"
    );
    assert_eq!(
        state.current_hp(),
        3,
        "rejected action cannot mutate health"
    );
    let _heal = state.apply(HealthAction::Heal { hp: u16::MAX }, &settings)?;
    assert_eq!(
        state.current_hp(),
        6,
        "healing saturates at maximum without overflow"
    );
    let death = state.apply(HealthAction::Damage { hp: u16::MAX }, &settings)?;
    assert!(
        death.is_some_and(super::super::health::HealthChange::became_dead),
        "lethal hit emits death transition"
    );
    settings.health_kits = true;
    assert!(
        state
            .apply(HealthAction::HealthKit { hp: 6 }, &settings)?
            .is_none(),
        "kits never revive dead actors"
    );
    assert!(
        state
            .apply(HealthAction::Damage { hp: 1 }, &settings)?
            .is_none(),
        "dead actors do not emit repeated death events"
    );
    Ok(())
}

#[test]
fn regeneration_accumulates_only_after_delay_and_rejects_nan() -> Result<(), HealthError> {
    let settings = HealthSettings {
        regeneration: true,
        regeneration_hp_per_second: 0.5,
        regeneration_delay_seconds: 1.0,
        ..enabled_settings()
    };
    let mut state = HealthState::default();
    let _damage = state.apply(HealthAction::Damage { hp: 4 }, &settings)?;
    let _tick = state.tick_regeneration(1.5, &settings)?;
    assert_eq!(
        state.current_hp(),
        2,
        "delay crossing earns only half a second"
    );
    let _tick_2 = state.tick_regeneration(1.5, &settings)?;
    assert_eq!(
        state.current_hp(),
        3,
        "fractional HP survives separate frames"
    );
    let snapshot = state;
    assert_eq!(
        state.tick_regeneration(f32::NAN, &settings),
        Err(HealthError::InvalidNumber),
        "NaN cannot poison state"
    );
    assert_eq!(state, snapshot, "invalid delta leaves state unchanged");
    let invalid = HealthSettings {
        regeneration_hp_per_second: f32::INFINITY,
        ..settings
    };
    assert!(
        state
            .apply(HealthAction::Damage { hp: 1 }, &invalid)
            .is_err(),
        "invalid rates reject whole action"
    );
    assert_eq!(state, snapshot, "invalid rules leave state unchanged");
    let _damage_2 = state.apply(HealthAction::Damage { hp: 1 }, &settings)?;
    let _tick_3 = state.tick_regeneration(2.0, &settings)?;
    assert_eq!(
        state.current_hp(),
        2,
        "damage restarts delay and fractional accumulation"
    );
    Ok(())
}

#[test]
fn health_checkpoint_preserves_death_and_rejects_stale_handles() -> Result<(), String> {
    use crate::engine::{entities::EntityWorld, level::LevelDef};
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"health","name":"Health","spawn":{"x":-2,"z":-2},"volumes":[{"id":"actor","x":0,"z":0,"width":1,"depth":1,"bottom_y":0,"top_y":1}]}"#).map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    world
        .set_health_settings(enabled_settings())
        .map_err(|error| error.to_string())?;
    let old = world.handle_of("actor").ok_or("missing actor")?;
    let death_change = world
        .apply_entity_health(old, HealthAction::Damage { hp: 6 })
        .map_err(|error| error.to_string())?;
    let encoded = serde_json::to_vec(&world.checkpoint()?).map_err(|error| error.to_string())?;
    let saved = serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
    world.resolve(&level);
    world
        .set_health_settings(enabled_settings())
        .map_err(|error| error.to_string())?;
    assert!(
        world.health_for_handle(old).is_none(),
        "old handle cannot read new actor"
    );
    assert_eq!(
        world.apply_entity_health(old, HealthAction::Damage { hp: 1 }),
        Err(HealthError::MissingTarget),
        "old handle cannot damage new actor"
    );
    world
        .dispatch_health(
            &crate::engine::health::HealthTarget::Entity {
                id: " actor ".into(),
            },
            HealthAction::Damage { hp: 1 },
        )
        .map_err(|error| error.to_string())?;
    assert_eq!(
        world.entity_health("actor").map(HealthState::current_hp),
        Some(5),
        "dispatch trims authored identity consistently with validation"
    );
    world.restore_checkpoint(&saved)?;
    assert!(
        world.take_health_events().is_empty(),
        "checkpoint publication discards pre-restore changes"
    );
    assert!(
        world
            .entity_health("actor")
            .is_some_and(HealthState::is_dead),
        "loading checkpoint must not revive"
    );
    assert!(
        serde_json::from_str::<HealthState>(
            r#"{"current_hp":7,"max_hp":6,"since_damage":0,"regeneration_fraction":0}"#
        )
        .is_err(),
        "invalid saved HP is rejected"
    );
    world
        .dispatch_health(
            &crate::engine::health::HealthTarget::Player,
            HealthAction::Damage { hp: 1 },
        )
        .map_err(|error| error.to_string())?;
    world.record_health_change(
        crate::engine::health::HealthTarget::Entity { id: "actor".into() },
        death_change.ok_or("missing death change")?,
    );
    world.reset_runtime();
    assert_eq!(
        world.entity_health("actor").map(HealthState::current_hp),
        Some(6),
        "explicit reset revives entities to their initial health"
    );
    assert!(
        world.take_health_events().is_empty(),
        "reset clears stale health events"
    );
    assert!(
        !world.take_commands().iter().any(|command| matches!(
            command,
            crate::engine::entities::WorldCommand::PlayerHealth { action: _ }
        )),
        "reset discards pending player damage"
    );
    let action = crate::engine::level::ActionDef::Health {
        target: crate::engine::health::HealthTarget::Player,
        action: HealthAction::HealthKit { hp: 2 },
    };
    let json = serde_json::to_string(&action).map_err(|error| error.to_string())?;
    assert_eq!(
        serde_json::from_str::<crate::engine::level::ActionDef>(&json)
            .map_err(|error| error.to_string())?,
        action,
        "nested health action cannot collide with outer action tag"
    );
    Ok(())
}

// Unique regression: optional hit protection must permit the first hit, expire
// with regeneration off, and survive save/load. Pure arithmetic/JSON fixture.
#[test]
fn configurable_hit_protection_expires_and_survives_serialization() -> Result<(), String> {
    let settings = HealthSettings {
        damage_cooldown_seconds: 0.25,
        ..enabled_settings()
    };
    let mut state = HealthState::default();
    assert!(
        state
            .apply(HealthAction::Damage { hp: 1 }, &settings)
            .map_err(|error| error.to_string())?
            .is_some(),
        "first hit must never be protected"
    );
    let encoded = serde_json::to_vec(&state).map_err(|error| error.to_string())?;
    let mut restored: HealthState =
        serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
    assert!(
        restored
            .apply(HealthAction::Damage { hp: 1 }, &settings)
            .map_err(|error| error.to_string())?
            .is_none(),
        "save/load retains remaining protection"
    );
    let _tick = restored
        .tick_regeneration(0.25, &settings)
        .map_err(|error| error.to_string())?;
    assert!(
        restored
            .apply(HealthAction::Damage { hp: 1 }, &settings)
            .map_err(|error| error.to_string())?
            .is_some(),
        "hit at expiry is accepted even with regeneration disabled"
    );
    assert_eq!(restored.current_hp(), 4, "only accepted hits reduce HP");
    let unprotected = HealthSettings {
        damage_cooldown_seconds: 0.0,
        ..settings
    };
    assert!(
        restored
            .apply(HealthAction::Damage { hp: 1 }, &unprotected)
            .map_err(|error| error.to_string())?
            .is_some(),
        "zero protection immediately accepts hits even during an earlier interval"
    );
    assert_eq!(
        restored.current_hp(),
        3,
        "disabled protection cannot retain a stale interval"
    );
    Ok(())
}

// Unique authored-health regression: different maximum/starting values must
// survive reset, save/load and spawned slot reuse. Three metadata-only props,
// one spawn template, no assets/GPU/bake; proportional submillisecond fixture.
#[test]
fn authored_player_and_individual_health_survive_lifecycle() -> Result<(), String> {
    use crate::engine::{
        entities::EntityWorld,
        level::{ActionDef, LevelDef},
    };
    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"authored_health","name":"Health",
        "spawn":{"x":0,"z":0}, "health":{"enabled":true,"player_max_hp":10,"player_starting_hp":7},
        "props":[
          {"id":"actor","model":"core:chair","x":0,"z":0,"components":[{"component":"health","max_hp":12,"starting_hp":4}]},
          {"id":"dead","model":"core:chair","x":1,"z":0,"components":[{"component":"health","max_hp":8,"starting_hp":0}]},
          {"id":"ordinary","model":"core:chair","x":2,"z":0}],
        "spawn_templates":[{"id":"template","model":"core:chair","components":[{"component":"health","max_hp":9,"starting_hp":3}]}],
        "spawn_points":[{"id":"point","x":2,"z":2,"template":"template"}]
    }"#).map_err(|error| error.to_string())?;
    check_initial_player_rules(&level.health)?;
    let mut world = EntityWorld::from_level(&level);
    let actor = world.handle_of("actor").ok_or("missing actor")?;
    assert_eq!(
        world
            .entity_health("actor")
            .map(|state| (state.current_hp(), state.max_hp())),
        Some((4, 12)),
        "individual actor uses authored values"
    );
    assert!(
        world
            .entity_health("dead")
            .is_some_and(HealthState::is_dead),
        "zero starting health remains dead"
    );
    assert_eq!(
        world.entity_health("ordinary").map(HealthState::current_hp),
        Some(6),
        "unconfigured actors retain normal defaults"
    );
    let _damage = world
        .apply_entity_health(actor, HealthAction::Damage { hp: 2 })
        .map_err(|error| error.to_string())?;
    let encoded = serde_json::to_vec(&world.checkpoint()?).map_err(|error| error.to_string())?;
    let snapshot = serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
    world.reset_runtime();
    assert_eq!(
        world.entity_health("actor").map(HealthState::current_hp),
        Some(4),
        "reset restores authored starting health, not full health"
    );
    world.restore_checkpoint(&snapshot)?;
    assert_eq!(
        world
            .entity_health("actor")
            .map(|state| (state.current_hp(), state.max_hp())),
        Some((2, 12)),
        "checkpoint does not silently heal an injured actor"
    );
    world.reset_runtime();
    assert_eq!(
        world.entity_health("actor").map(HealthState::current_hp),
        Some(4),
        "authored starting health survives checkpoint restore"
    );
    let spawn = ActionDef::SpawnEntity {
        template: None,
        point: Some("point".into()),
        group: None,
        name: Some("spawned".into()),
    };
    let _report = world.dispatch_on("actor", std::slice::from_ref(&spawn));
    let spawned = world.handle_of("spawned").ok_or("missing spawned actor")?;
    assert_eq!(
        world
            .health_for_handle(spawned)
            .map(|state| (state.current_hp(), state.max_hp())),
        Some((3, 9)),
        "spawn template configures independent health"
    );
    let _lethal = world
        .apply_entity_health(spawned, HealthAction::Damage { hp: 9 })
        .map_err(|error| error.to_string())?;
    let _despawn = world.dispatch_on(
        "actor",
        &[ActionDef::DespawnEntity {
            target: "spawned".into(),
        }],
    );
    let _respawn = world.dispatch_on("actor", &[spawn]);
    assert!(
        world.health_for_handle(spawned).is_none(),
        "spawn reuse rejects stale health handle"
    );
    assert_eq!(
        world.entity_health("spawned").map(HealthState::current_hp),
        Some(3),
        "new spawn inherits authored health, never predecessor death"
    );
    Ok(())
}

fn check_initial_player_rules(settings: &HealthSettings) -> Result<(), String> {
    let initial_player = settings
        .initial_player_health()
        .map_err(|error| error.to_string())?;
    assert_eq!(
        (initial_player.current_hp(), initial_player.max_hp()),
        (7, 10),
        "player current and maximum are separately authored"
    );
    assert!(
        HealthSettings {
            player_max_hp: 0,
            ..enabled_settings()
        }
        .validate()
        .is_err(),
        "zero maximum rejected"
    );
    assert!(
        HealthSettings {
            player_starting_hp: 7,
            ..enabled_settings()
        }
        .validate()
        .is_err(),
        "starting HP cannot exceed maximum"
    );
    Ok(())
}

fn enabled_settings() -> HealthSettings {
    HealthSettings {
        enabled: true,
        ..HealthSettings::default()
    }
}
