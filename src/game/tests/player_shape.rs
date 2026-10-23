//! Value: reproduces tiny-form traversal and atomic refused expansion, absent
//! from the human-only movement coverage. Two-room-free CPU geometry, 64 sweeps,
//! no renderer, assets or map bake; bounded milliseconds and kilobytes.
use crate::engine::level::LevelDef;
use crate::game::player_shape::PlayerShape;
use crate::game::{CollisionWorld, Game, HorizontalMode};
use glam::Vec3;

#[test]
fn small_body_crosses_header_and_failed_human_expansion_preserves_pose() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"shape","name":"Shape","spawn":{"x":2,"z":2},
        "rooms":[{"x":0,"z":0,"width":8,"depth":4,"height":4}],
        "walls":[{"x":3,"y":0.25,"z":0,"width":0.4,"depth":4,"height":3.75}]}"#,
    )
    .map_err(|e| e.to_string())?;
    let mut game = Game::new(
        Vec3::new(2.0, 1.6, 2.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    let cup = PlayerShape::from_bounds(([-0.06, 0.0, -0.04], [0.06, 0.09, 0.04]), 1.0)?;
    assert!(game.form_fits(cup));
    game.commit_form(Some("home:mug".into()), cup);
    for _ in 0_u8..32 {
        game.move_horizontal(Vec3::X, 1.0, 0.0375, HorizontalMode::Walk);
    }
    assert!((3.1..3.3).contains(&game.player_position.x));
    let before = (
        game.player_position,
        game.feet_y(),
        game.body_half(),
        game.body_height(),
    );
    for _ in 0_u8..32 {
        assert!(!game.form_fits(PlayerShape::human()));
    }
    assert_eq!(
        before,
        (
            game.player_position,
            game.feet_y(),
            game.body_half(),
            game.body_height()
        )
    );
    for _ in 0_u8..32 {
        game.move_horizontal(Vec3::X, 1.0, 0.0375, HorizontalMode::Walk);
    }
    assert!(game.form_fits(PlayerShape::human()));
    game.commit_form(None, PlayerShape::human());
    assert_eq!(game.body_half(), 0.4);
    assert_eq!(game.body_height(), 1.8);
    game.commit_form(Some("home:mug".into()), cup);
    game.reset_to_spawn();
    assert!(game.form_model().is_none());
    assert_eq!(game.body_height(), 1.8);
    Ok(())
}

#[test]
fn measured_envelope_covers_every_yaw_and_rejects_invalid_bounds() -> Result<(), String> {
    let bounds = ([-0.2, 0.0, -0.7], [0.2, 1.2, 0.7]);
    let shape = PlayerShape::from_bounds(bounds, 1.0)?;
    for degree in 0_u16..360 {
        let yaw = f32::from(degree).to_radians();
        for x in [-0.2_f32, 0.2] {
            for z in [-0.7_f32, 0.7] {
                assert!(yaw.cos().mul_add(x, yaw.sin() * z).abs() <= shape.half + 0.00001);
            }
        }
    }
    assert!(PlayerShape::from_bounds(bounds, f32::NAN).is_err());
    assert!(PlayerShape::from_bounds(bounds, 4.0).is_err());
    assert!(PlayerShape::from_bounds(([0.0; 3], [0.0; 3]), 1.0).is_err());
    Ok(())
}
