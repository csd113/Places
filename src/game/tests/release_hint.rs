//! One tiny body reproduces a failed release followed by restored clearance.
//! This protects the stale HUD warning and ensures querying it never drops the
//! item or weakens collision refusal. No assets, rendering device or map bake.

use super::Items;
use crate::engine::level::LevelDef;
use crate::engine::physics::{BodyMotion, PhysicsDef, PlayerContext, ShapeDef};
use glam::Vec3;

#[test]
fn release_hint_clears_after_contact_ends_without_releasing_item() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"release_hint","name":"Release hint","spawn":{"x":0,"z":0}}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut items = Items::new();
    items.world = crate::engine::physics::ItemWorld::from_level(&level, &[]);
    items.world.spawn_authored(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius: 0.1 }),
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1)),
        Vec3::new(0.0, 1.4, -1.0),
        0.0,
        1.0,
    )?;
    let clear = PlayerContext {
        eye: Vec3::new(0.0, 1.5, 0.0),
        feet: Vec3::ZERO,
        height: 1.8,
        view: Vec3::NEG_Z,
        yaw: 0.0,
        active: true,
    };
    items.world.sync_context(clear, &[]);
    assert!(items.world.pickup(1, clear.eye, clear.view, clear.yaw));
    assert!(items.world.can_drop());
    items.world.sync_context(
        PlayerContext {
            eye: Vec3::new(0.0, 1.5, -1.0),
            feet: Vec3::new(0.0, 0.0, -1.0),
            ..clear
        },
        &[],
    );
    assert!(!items.world.can_drop());
    assert!(!items.world.drop(), "overlapping releases must still fail");
    items.release_blocked = true;
    let mut vertices = Vec::new();
    items.append_release_hint(&mut vertices);
    assert!(items.release_blocked);
    assert!(
        !vertices.is_empty(),
        "actual obstruction retains the warning"
    );
    assert_eq!(items.world.held(), Some(1));

    items.world.sync_context(clear, &[]);
    vertices.clear();
    items.append_release_hint(&mut vertices);
    assert!(
        !items.release_blocked,
        "clearance must clear the stale warning"
    );
    assert_eq!(vertices, []);
    assert_eq!(items.world.held(), Some(1), "HUD checks preserve custody");
    assert!(items.world.drop(), "a clear release still succeeds");
    Ok(())
}
