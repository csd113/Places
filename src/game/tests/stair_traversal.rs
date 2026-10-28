//! Essential stair regression: the whole grounded hull used discrete treads
//! despite the established nosing profile. Four tiny CPU flights cover both
//! directions, max riser/min tread, pause/reversal, hull support and fall camera handoff.
//! No renderer, assets or bake; under 2,000 deterministic movement samples.
use crate::engine::level::LevelDef;
use crate::game::{CollisionWorld, EYE_HEIGHT, Game, HorizontalMode, VERTICAL_SUBSTEP};
use glam::{Vec2, Vec3};

#[test]
fn grounded_stairs_follow_nosings_without_losing_physical_clearance() -> Result<(), String> {
    for (rise, steps, run) in [
        (1.8_f32, 9_u32, 6.0_f32),
        (3.6, 8, 6.0),
        (2.4, 16, 2.4),
        (3.6, 8, 1.2),
    ] {
        let level = LevelDef::from_json(&serde_json::json!({
            "schema":"places-level","format_version":1_u32,"id":"stairs","name":"Stairs",
            "spawn":{"x":2_u32,"z":1_u32},
            "rooms":[{"x":0_u32,"z":0_u32,"width":8_u32,"depth":12_u32,"height":8_u32}],
            "stairs":[{"x":1_u32,"z":2_u32,"width":1_u32,"depth":run,"rise":rise,"steps":steps}],
            "floor_regions":[{"x":1_u32,"z":2.0_f32+run,"width":1_u32,"depth":3_u32,"offset_y":rise}]
        }).to_string()).map_err(|error|error.to_string())?;
        let mut game = Game::new(
            Vec3::new(1.5, EYE_HEIGHT, 1.0),
            0.0,
            CollisionWorld::from_level(&level),
        );
        game.grounded = true;
        let mut largest_interior_delta = 0.0_f32;
        for direction in [1.0_f32, -1.0] {
            for _ in 0_u16..240 {
                let previous = game.feet_y();
                let before_z = game.player_position.z;
                game.move_horizontal(
                    Vec3::new(0.0, 0.0, direction),
                    2.8,
                    VERTICAL_SUBSTEP,
                    HorizontalMode::Walk,
                );
                game.advance_step_eye(previous, true);
                game.refresh_ground_support();
                let point = Vec2::new(game.player_position.x, game.player_position.z);
                let real = game
                    .floor
                    .hull_support(
                        point,
                        game.body_half() - super::super::CONTACT_EPS,
                        rise + 0.5,
                    )
                    .ok_or("missing physical support")?;
                assert!(
                    game.feet_y() + 0.001 >= real.0,
                    "walking hull cannot enter a tread"
                );
                assert!(game.grounded, "stair support must stay grounded: {point:?}");
                if point.y > 2.8 && point.y < run + 1.0 && before_z > 2.8 && before_z < run + 1.0 {
                    largest_interior_delta =
                        largest_interior_delta.max((game.feet_y() - previous).abs());
                }
                if direction > 0.0 && point.y > run + 2.7 || direction < 0.0 && point.y < 1.0 {
                    break;
                }
            }
            for _ in 0_u8..20 {
                game.advance_step_eye(game.feet_y(), true);
            }
            assert!(
                game.step_eye_lag.abs() < 0.001,
                "stopped eye must settle exactly"
            );
        }
        assert!(
            largest_interior_delta < 0.09,
            "interior traversal cannot jump a riser: {largest_interior_delta}"
        );
        assert!(
            game.player_position.z < 1.1 && game.feet_y().abs() < 0.001,
            "both directions and landings remain traversable"
        );
        game.step_eye_lag = 0.4;
        game.grounded = false;
        game.advance_step_eye(game.feet_y(), true);
        assert!(
            game.step_eye_lag.abs() < f32::EPSILON,
            "falls immediately own their camera"
        );
    }
    Ok(())
}
