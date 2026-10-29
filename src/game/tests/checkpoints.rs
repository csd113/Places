//! Essential boundary regression: one tiny semantic world protects persisted
//! typed progression/door state plus real atomic replacement, recovery and bounds.
//! Unique value: existing campaign tests cannot restore gameplay or reject a
//! damaged payload. A handful of small temporary writes; no assets, GPU or bake.
use super::{Identity, Snapshot, read_at, write_at};
use crate::{
    engine::{
        entities::components::StateValue,
        level::{ActionDef, LevelDef},
    },
    game::{CollisionWorld, Game},
};

#[test]
fn gameplay_checkpoint_roundtrip_recovers_previous_and_preserves_failed_publication()
-> Result<(), String> {
    let (level, snapshot) = snapshot_fixture()?;
    let folder =
        std::env::temp_dir().join(format!("places-gameplay-checkpoint-{}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|problem| problem.to_string())?;
    let path = folder.join("checkpoint.json");
    let identity = Identity {
        project: "fixture".into(),
        level: "checkpoint".into(),
        content: "a".repeat(64),
    };
    write_at(&path, &identity, "First", &snapshot)?;
    let original = std::fs::read(&path).map_err(|problem| problem.to_string())?;
    std::fs::create_dir(path.with_extension("previous")).map_err(|problem| problem.to_string())?;
    assert!(
        write_at(&path, &identity, "Refused", &snapshot).is_err(),
        "backup/publication failure must report failure"
    );
    assert_eq!(
        std::fs::read(&path).map_err(|problem| problem.to_string())?,
        original,
        "failed writes preserve the last valid primary"
    );
    std::fs::remove_dir(path.with_extension("previous")).map_err(|problem| problem.to_string())?;
    write_at(&path, &identity, "Second", &snapshot)?;
    std::fs::write(&path, b"{truncated").map_err(|problem| problem.to_string())?;
    let recovered = read_at(&path, &identity)?.ok_or("Missing recovery")?;
    assert!(
        recovered.recovered,
        "a corrupt primary recovers the verified previous save"
    );
    assert_eq!(
        recovered.name, "First",
        "recovery never promotes an incomplete temp file"
    );
    let mut restored = Game::new(
        crate::game::spawn_position(&level),
        0.0,
        CollisionWorld::from_level(&level),
    );
    restored
        .entities_mut()
        .restore_checkpoint(&recovered.snapshot.world)?;
    restored.restore_checkpoint_player(&recovered.snapshot.player)?;
    let handle = restored
        .entities()
        .handle_of("quest")
        .ok_or("Missing quest")?;
    assert_eq!(
        restored
            .entities()
            .components()
            .states
            .get(handle)
            .and_then(|state| state.get("count"))
            .and_then(StateValue::as_int),
        Some(3),
        "real typed progression survives reconstruction"
    );
    assert_eq!(
        restored.doors().is_locked("door"),
        Some(true),
        "door lock survives reconstruction"
    );
    let mut changed = identity.clone();
    changed.content = "b".repeat(64);
    assert!(
        read_at(&path, &changed).is_err(),
        "changed content cannot inherit checkpoint state"
    );
    std::fs::write(path.with_extension("previous"), b"bad")
        .map_err(|problem| problem.to_string())?;
    assert!(
        read_at(&path, &identity).is_err(),
        "two corrupt copies must offer a fresh start"
    );
    std::fs::write(&path, vec![b' '; 4_194_305]).map_err(|problem| problem.to_string())?;
    assert!(
        read_at(&path, &identity).is_err(),
        "input admission bounds memory before decoding"
    );
    std::fs::remove_file(&path)
        .and_then(|()| std::fs::remove_file(path.with_extension("previous")))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|problem| problem.to_string())?;
    Ok(())
}

fn snapshot_fixture() -> Result<(LevelDef, Snapshot), String> {
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"checkpoint","name":"Checkpoint","saving":true,"spawn":{"x":2,"z":2},"rooms":[{"width":6,"depth":6}],"ceiling_lights":[{"id":"lamp","fixture":"core:fluorescent_panel_01","x":2,"z":2,"switchable":true}],"props":[{"id":"quest","model":"core:chair","x":1,"z":1,"components":[{"component":"state","name":"count","value":0}]}],"doors":[{"id":"door","x":0,"z":0,"width":1,"height":2}]}"#).map_err(|problem|problem.to_string())?;
    let mut game = Game::new(
        crate::game::spawn_position(&level),
        0.0,
        CollisionWorld::from_level(&level),
    );
    let _initial = game.entities_mut().take_commands();
    let _change = game.entities_mut().dispatch_on(
        "quest",
        &[
            ActionDef::SetState {
                target: None,
                name: "count".into(),
                value: StateValue::Int(3),
            },
            ActionDef::SetLight {
                target: Some("lamp".into()),
                on: false,
            },
            ActionDef::Lock {
                target: Some("door".into()),
            },
        ],
    );
    // Flush bounded state-change events through an ordinary tick before capture.
    let floor = crate::engine::level::WalkableFloor::from_level(&level);
    let index = crate::engine::collision_index::CollisionIndex::build(&[]);
    let _tick = game
        .entities_mut()
        .tick(&crate::engine::entities::WorldContext {
            delta_seconds: 0.016,
            feet_from: glam::Vec3::new(2.0, 0.0, 2.0),
            feet: glam::Vec3::new(2.0, 0.0, 2.0),
            eye: glam::Vec3::new(2.0, 1.6, 2.0),
            body_height: 1.8,
            body_half: crate::engine::collision::PLAYER_RADIUS,
            form_model: None,
            walls: &[],
            index: &index,
            floor: &floor,
            nav: None,
        });
    let _commands = game.entities_mut().take_commands();
    let snapshot = Snapshot {
        player: game.checkpoint_player()?,
        world: game.entities().checkpoint()?,
        items: Vec::new(),
        flashlight: true,
    };
    Ok((level, snapshot))
}
