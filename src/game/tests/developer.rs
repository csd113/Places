//! Essential regression for the noclip exit that teleported clear airborne
//! players to spawn. One empty collision world and one simulation update keep
//! this independent of assets, map baking, rendering and wall-clock timing.

use glam::Vec3;

use crate::game::input::InputState;
use crate::game::settings::Settings;
use crate::game::{AppState, CollisionWorld, EYE_HEIGHT, Game};

#[test]
fn noclip_exit_keeps_airborne_pose_and_resumes_gravity() {
    let mut game = Game::new(
        Vec3::new(0.0, EYE_HEIGHT, 0.0),
        0.0,
        CollisionWorld::default(),
    );
    game.set_app_state(AppState::Playing);
    assert!(game.set_noclip(true).is_ok(), "noclip must enable");
    let airborne_eye = Vec3::new(3.0, 9.6, -4.0);
    game.player_position = airborne_eye;
    game.feet_y = airborne_eye.y - EYE_HEIGHT;
    game.player_yaw = 1.25;
    game.player_pitch = 0.375;

    // Live physics can invalidate every otherwise clear destination. The same
    // empty fixture verifies refusal keeps control/pose intact before retrying.
    assert!(
        game.set_noclip_with_clearance(false, |_| false).is_err(),
        "blocked live-body clearance must refuse the exit"
    );
    assert!(game.noclip_enabled(), "refusal must retain noclip control");
    assert_eq!(
        game.player_position.to_array().map(f32::to_bits),
        airborne_eye.to_array().map(f32::to_bits),
        "refusal must preserve the current eye pose"
    );
    let result = game.set_noclip(false);
    assert!(
        result.is_ok(),
        "clear airborne exit must succeed: {result:?}"
    );
    assert_eq!(
        game.player_position.to_array().map(f32::to_bits),
        airborne_eye.to_array().map(f32::to_bits),
        "noclip exit must preserve the exact airborne eye pose"
    );
    assert_eq!(
        game.player_yaw.to_bits(),
        1.25_f32.to_bits(),
        "yaw must remain exact"
    );
    assert_eq!(
        game.player_pitch.to_bits(),
        0.375_f32.to_bits(),
        "pitch must remain exact"
    );
    assert!(
        !game.grounded,
        "an airborne exit must not fabricate support"
    );

    game.set_benchmark_simulation_delta(1.0 / 60.0);
    game.update_player_movement(&mut InputState::default(), &Settings::default());
    assert!(
        game.player_position.y < airborne_eye.y,
        "ordinary gravity must lower the eye after exit"
    );
    assert!(
        game.vertical_velocity < 0.0,
        "ordinary gravity must resume downward velocity"
    );
}
