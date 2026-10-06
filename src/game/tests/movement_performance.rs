//! Opt-in headless controller benchmark, independent of native rendering.
//! Run with `cargo test --release --lib movement_performance -- --ignored --nocapture`.

use super::*;
use std::hint::black_box;
use std::time::Instant;

// Assertions and fixture decoding belong to this opt-in test, not gameplay.
#[expect(
    clippy::expect_used,
    reason = "Assertions and fixture decoding belong to this opt-in test, not gameplay."
)]
#[test]
#[ignore = "opt-in before/after movement hot-path measurement"]
fn measure_controller_hot_path() {
    let mut level = LevelDef::from_json(
        r#"{
        "format_version":3,"id":"movement_performance","name":"movement cost",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-10,"z":-10,"width":80,"depth":80,"height":4}]
    }"#,
    )
    .expect("benchmark fixture");
    let mut results = Vec::new();
    for (label, extra_boxes) in [("ordinary", 0_u32), ("dense 4000", 4000), ("ice", 0)] {
        if label == "ice" {
            level.defaults.floor = "winter:ice_01".to_owned();
        }
        let mut world = CollisionWorld::from_level(&level)
            .with_ground_materials(&level, &super::ice::materials(&level));
        world
            .walls
            .push(WallAabb::with_y(1.0, 0.0, -4.0, 0.01, 3.0, 8.0));
        for index in 0..extra_boxes {
            let x = f32::from(u16::try_from(index % 100).expect("column")).mul_add(0.6, 5.0);
            let z = f32::from(u16::try_from(index / 100).expect("row")).mul_add(0.6, 5.0);
            world.walls.push(WallAabb::with_y(x, 0.0, z, 0.1, 3.0, 0.1));
        }
        super::movement_regression::export_fixture(
            &serde_json::to_string(&level).expect("performance fixture source"),
            Vec3::ZERO,
            &world.walls,
        );
        let mut game = Game::new(
            Vec3::new(0.0, EYE_HEIGHT, 0.0),
            std::f32::consts::FRAC_PI_2,
            world,
        );
        game.set_app_state(AppState::Playing);
        let settings = Settings::default();
        let mut samples = Vec::new();
        for _ in 0_i32..7_i32 {
            let start = Instant::now();
            for frame in 0..20_000_u32 {
                if frame % 240 == 0 {
                    game.reset_spawn_point(
                        Vec3::new(0.0, EYE_HEIGHT, 0.0),
                        std::f32::consts::FRAC_PI_2,
                    );
                }
                game.sim_delta_seconds = 1.0 / 60.0;
                let controls = if frame % 240 == 0 {
                    &[Control::MoveForward, Control::Jump][..]
                } else {
                    &[Control::MoveForward][..]
                };
                game.update_player_movement(&mut InputState::holding(controls), &settings);
                let _black_box_status = black_box(game.player_position);
            }
            samples.push(start.elapsed().as_secs_f64() * 1_000_000.0_f64 / 20_000.0_f64);
        }
        samples.sort_by(f64::total_cmp);
        let median = samples.get(3).copied().expect("seven samples");
        results.push(
            serde_json::json!({"scenario":label,"median_us_per_frame":median,"samples_us":samples}),
        );
    }
    let report = serde_json::to_string_pretty(&results).expect("benchmark report");
    crate::logging::warn(&report);
    if let Some(path) = std::env::var_os("PLACES_MOVEMENT_BENCH_OUT") {
        std::fs::write(path, report).expect("benchmark evidence destination");
    }
}
