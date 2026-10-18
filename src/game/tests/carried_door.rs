//! One tiny metadata collision world protects exact carried-door selection,
//! fallback set-down and real wall/body obstruction, without rendering or bake.
use crate::engine::collision::WallAabb;
use crate::engine::level::{ActionDef, LevelDef};
use crate::game::{CollisionWorld, EYE_HEIGHT, Game};
use glam::Vec3;

#[test]
fn carried_door_target_preserves_reach_occlusion_and_set_down() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"carried","name":"Carried","spawn":{"x":0.5,"z":1},"doors":[{"id":"door","x":0,"z":0,"width":1,"height":2,"thickness":0.05,"components":[{"component":"interactable"}],"bindings":[{"on":"interact","actions":[{"action":"toggle"}]}]}]}"#).map_err(|error| error.to_string())?;
    let mut game = Game::new(
        Vec3::new(0.5, EYE_HEIGHT, 1.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    let target = game.carried_door_target(None).ok_or("aimed door missing")?;
    assert!(target.1 < 1.1, "uses the leaf surface");
    let _lock = game.dispatch_actions(
        &[ActionDef::Lock {
            target: Some("door".into()),
        }],
        None,
    );
    assert!(
        game.carried_door_target(None).is_none(),
        "locked door must preserve set-down onto a receiver in front of it"
    );
    let _unlock = game.dispatch_actions(
        &[ActionDef::Unlock {
            target: Some("door".into()),
        }],
        None,
    );
    assert!(
        game.carried_door_target(None).is_some(),
        "unlocking restores contextual door priority"
    );
    assert!(
        game.carried_door_target(Some(0.25)).is_none(),
        "nearer physical obstruction retains drop"
    );
    assert!(
        game.carried_door_target(Some(2.0)).is_some(),
        "body behind door cannot steal interaction"
    );
    game.player_yaw = std::f32::consts::PI;
    assert!(
        game.carried_door_target(None).is_none(),
        "looking away deliberately selects drop"
    );
    game.player_yaw = 0.0;
    game.player_position.z = 10.0;
    assert!(
        game.carried_door_target(None).is_none(),
        "carrying cannot extend door reach"
    );
    game.player_position.z = 1.0;
    game.set_walls(vec![WallAabb {
        min_x: -1.0,
        max_x: 2.0,
        min_y: 0.0,
        max_y: 3.0,
        min_z: 0.4,
        max_z: 0.5,
        step_up: 0.0,
    }]);
    assert!(
        game.carried_door_target(None).is_none(),
        "wall occlusion remains authoritative"
    );
    Ok(())
}
