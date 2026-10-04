//! Physical controller invariants, independent of rendering and map aesthetics.

use super::*;

/// Preserve the actual test geometry as inspectable maps only on explicit request.
pub(super) fn export_fixture(source: &str, feet: Vec3, walls: &[WallAabb]) {
    let Some(destination) = std::env::var_os("PLACES_MOVEMENT_FIXTURE_OUT") else {
        return;
    };
    let mut definition: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(source).expect("fixture source");
    drop(definition.insert(
        "walls".to_owned(),
        serde_json::json!(
            walls
                .iter()
                .map(|wall| {
                    serde_json::json!({"x":wall.min_x,"y":wall.min_y,"z":wall.min_z,
            "width":wall.max_x-wall.min_x,"depth":wall.max_z-wall.min_z,
            "height":wall.max_y-wall.min_y})
                })
                .collect::<Vec<_>>()
        ),
    ));
    drop(definition.insert(
        "defaults".to_owned(),
        serde_json::json!({"wall":"core:wallpaper_yellow_01",
        "floor":"core:carpet_beige_01","ceiling":"core:ceiling_panel_01"}),
    ));
    drop(definition.insert(
        "spawn".to_owned(),
        serde_json::json!({"x":feet.x,"z":feet.z,"yaw_degrees":0.0_f64}),
    ));
    let identity = serde_json::to_vec(&serde_json::json!([definition, feet.to_array()]))
        .expect("fixture identity");
    let suffix: String = crate::package::hash::sha256_hex(&identity)
        .chars()
        .take(12)
        .collect();
    let id = format!("movement_audit_{suffix}");
    let thread = std::thread::current();
    let scenario = thread.name().unwrap_or("movement_fixture");
    drop(definition.insert("id".to_owned(), serde_json::json!(id)));
    drop(definition.insert(
        "name".to_owned(),
        serde_json::json!(scenario.rsplit("::").next().unwrap_or(scenario)),
    ));
    let destination_path = std::path::PathBuf::from(destination);
    std::fs::create_dir_all(destination_path.join("sources")).expect("fixture source directory");
    std::fs::create_dir_all(destination_path.join("metadata")).expect("fixture metadata directory");
    std::fs::write(
        destination_path.join("sources").join(format!("{id}.json")),
        serde_json::to_vec_pretty(&definition).expect("inspectable fixture"),
    )
    .expect("preserve fixture source");
    std::fs::write(
        destination_path.join("metadata").join(format!("{id}.json")),
        serde_json::to_vec_pretty(&serde_json::json!({"id":id,"test":scenario,
            "eye_spawn":[feet.x,feet.y+EYE_HEIGHT,feet.z],"yaw_degrees":0.0_f64}))
        .expect("fixture launch metadata"),
    )
    .expect("preserve fixture launch metadata");
}

fn room_game(rooms: &str, feet: Vec3, walls: Vec<WallAabb>) -> Game {
    let source = format!(
        r#"{{"format_version":3,"id":"movement_regression","name":"Movement regression",
        "spawn":{{"x":0,"z":0}},"rooms":{rooms}}}"#
    );
    export_fixture(&source, feet, &walls);
    let level = LevelDef::from_json(&source).expect("deterministic fixture");
    let mut world = CollisionWorld::from_level(&level);
    world.walls.extend(walls);
    let mut game = Game::new(Vec3::new(feet.x, feet.y + EYE_HEIGHT, feet.z), 0.0, world);
    game.set_app_state(AppState::Playing);
    game
}

fn flat_game(feet: Vec3, walls: Vec<WallAabb>) -> Game {
    room_game(
        r#"[{"x":-10,"z":-10,"width":20,"depth":20,"height":8}]"#,
        feet,
        walls,
    )
}

fn frame(game: &mut Game, controls: &[Control], delta: f32) {
    game.sim_delta_seconds = delta;
    game.update_player_movement(&mut InputState::holding(controls), &Settings::default());
}

fn assert_no_box_penetration(game: &Game) {
    for wall in &game.walls {
        if wall.max_y > game.feet_y() + CONTACT_EPS
            && wall.min_y < game.feet_y() + game.body_height() - CONTACT_EPS
        {
            assert!(
                !wall.overlaps_disc(
                    game.player_position.x,
                    game.player_position.z,
                    PLAYER_RADIUS - CONTACT_EPS,
                ),
                "body penetrates {wall:?}, feet={:?}",
                Vec3::new(
                    game.player_position.x,
                    game.feet_y(),
                    game.player_position.z
                )
            );
        }
    }
}

#[test]
fn stacked_storey_ceiling_below_feet_cannot_teleport_a_jump() {
    let mut game = room_game(
        r#"[{"x":-5,"z":-5,"width":10,"depth":10,"floor_y":0,"height":2.4},
            {"x":-5,"z":-5,"width":10,"depth":10,"floor_y":2.6,"height":3}]"#,
        Vec3::new(0.0, 2.6, 0.0),
        vec![],
    );
    frame(&mut game, &[Control::Jump], 1.0 / 60.0);
    assert!(
        game.feet_y() > 2.6,
        "ceiling below feet moved player to {}",
        game.feet_y()
    );
}

#[test]
fn pit_stacked_balcony_jump_never_produces_vertical_teleport() {
    let level = LevelDef::from_json(include_str!(
        "../../../tests/fixtures/levels/level0_pit.json"
    ))
    .expect("actual Pit source");
    let mut game = game_for(&level);
    game.reset_spawn_point(Vec3::new(50.2, EYE_HEIGHT, -29.2), std::f32::consts::PI);
    game.set_app_state(AppState::Playing);
    let delta = 1.0 / 60.0;
    let mut previous = game.feet_y();
    for index in 0_i32..90_i32 {
        let controls = if index == 0_i32 {
            vec![Control::MoveForward, Control::Jump]
        } else if index < 35_i32 {
            vec![Control::MoveForward]
        } else {
            vec![]
        };
        frame(&mut game, &controls, delta);
        let drop = previous - game.feet_y();
        assert!(
            drop < 0.15,
            "Pit balcony frame {index}: vertical drop {drop}, feet={}",
            game.feet_y()
        );
        assert_no_box_penetration(&game);
        previous = game.feet_y();
    }
}

#[test]
fn falling_body_edge_cannot_enter_a_prop_when_center_misses_top() {
    let mut game = flat_game(
        Vec3::new(-0.15, 1.2, 0.5),
        vec![WallAabb::with_y(0.0, 0.0, 0.0, 1.0, 0.9, 1.0)],
    );
    game.grounded = false;
    for _ in 0_i32..60_i32 {
        frame(&mut game, &[], 1.0 / 60.0);
        assert_no_box_penetration(&game);
    }
}

#[test]
fn low_ceiling_covers_the_body_disc_at_its_edge() {
    let mut game = room_game(
        r#"[{"x":0,"z":-2,"width":4,"depth":4,"height":2.05},
            {"x":-4,"z":-2,"width":4,"depth":4,"height":8}]"#,
        Vec3::new(-0.15, 0.0, 0.0),
        vec![],
    );
    for index in 0_i32..60_i32 {
        frame(
            &mut game,
            if index == 0_i32 {
                &[Control::Jump]
            } else {
                &[]
            },
            1.0 / 60.0,
        );
        assert!(
            game.feet_y() + game.body_height() <= 2.05 + CONTACT_EPS,
            "head entered ceiling edge: {}",
            game.feet_y() + game.body_height()
        );
    }
}

#[test]
fn air_spawn_requires_actual_support_instead_of_an_xz_floor() {
    let mut game = flat_game(Vec3::new(0.0, 3.0, 0.0), vec![]);
    assert!(!game.grounded, "a floor three metres below is not support");
    frame(&mut game, &[], 1.0 / 30.0);
    assert!(game.feet_y() < 3.0 && game.vertical_velocity < 0.0);
}

#[test]
fn real_solid_step_at_maximum_height_is_traversable() {
    let mut game = flat_game(
        Vec3::new(-1.0, 0.0, 0.5),
        vec![WallAabb::with_y(
            0.0,
            0.0,
            0.0,
            2.0,
            PLAYER_STEP_HEIGHT,
            1.0,
        )],
    );
    game.player_yaw = std::f32::consts::FRAC_PI_2;
    for _ in 0_i32..35_i32 {
        frame(&mut game, &[Control::MoveForward], 1.0 / 60.0);
        assert_no_box_penetration(&game);
    }
    assert!(
        game.player_position.x > 0.4 && (game.feet_y() - PLAYER_STEP_HEIGHT).abs() < STEP_EPS,
        "valid step remained blocked at {:?}",
        game.player_position
    );
}

const FRAME_PATTERNS: &[(&str, &[f32])] = &[
    ("30 Hz", &[1.0 / 30.0]),
    ("60 Hz", &[1.0 / 60.0]),
    ("120 Hz", &[1.0 / 120.0]),
    ("144 Hz", &[1.0 / 144.0]),
    ("long frames", &[1.0 / 144.0, 1.0 / 30.0, 0.1, 1.0 / 60.0]),
];

fn run_for(game: &mut Game, controls: &[Control], seconds: f32, pattern: &[f32]) -> f32 {
    let mut input = InputState::holding(controls);
    let mut elapsed = 0.0;
    let mut highest = game.feet_y();
    for delta in pattern.iter().cycle() {
        if elapsed >= seconds {
            break;
        }
        game.sim_delta_seconds = delta.min(seconds - elapsed);
        game.update_player_movement(&mut input, &Settings::default());
        elapsed += game.sim_delta_seconds;
        highest = highest.max(game.feet_y());
        assert_no_box_penetration(game);
        assert!(game.player_position.is_finite() && game.vertical_velocity.is_finite());
    }
    highest
}

#[test]
fn ceilings_stop_upward_velocity_preserve_lateral_motion_and_then_fall_at_every_rate() {
    for (label, pattern) in FRAME_PATTERNS {
        for ceiling in [PLAYER_HEIGHT + 0.0005, 2.05, PLAYER_HEIGHT + 0.98] {
            let rooms =
                format!(r#"[{{"x":-10,"z":-10,"width":20,"depth":20,"height":{ceiling}}}]"#);
            let mut game = room_game(&rooms, Vec3::ZERO, vec![]);
            game.player_yaw = std::f32::consts::FRAC_PI_2;
            let mut elapsed = 0.0;
            let mut input = InputState::holding(&[Control::Jump, Control::MoveForward]);
            let mut bumped = false;
            let mut fell = false;
            for delta in pattern.iter().cycle() {
                if elapsed >= 1.6 {
                    break;
                }
                game.sim_delta_seconds = *delta;
                game.update_player_movement(&mut input, &Settings::default());
                elapsed += delta;
                assert!(
                    game.feet_y() + game.body_height() <= ceiling + CONTACT_EPS,
                    "{label}: ceiling {ceiling}"
                );
                if game.ceiling_contact_this_frame {
                    bumped = true;
                    assert!(
                        game.vertical_velocity <= 0.0,
                        "{label}: upward velocity survived ceiling"
                    );
                    if ceiling - PLAYER_HEIGHT >= 0.001 {
                        assert!(!game.grounded, "ceiling was counted as ground");
                    }
                }
                fell |= bumped && game.vertical_velocity < 0.0;
            }
            assert!(
                bumped && (fell || ceiling - PLAYER_HEIGHT < 0.001) && game.grounded,
                "{label}: {ceiling}, bump={bumped}, fall={fell}"
            );
            assert!(
                game.player_position.x > 4.5,
                "ceiling stopped tangent movement"
            );
        }
    }
}

#[test]
fn airborne_side_contact_never_steps_or_becomes_ground_and_slides() {
    for (label, pattern) in FRAME_PATTERNS {
        let mut game = flat_game(
            Vec3::new(-0.5, 0.0, 0.0),
            vec![WallAabb::with_y(0.0, 0.0, -4.0, 0.01, 4.0, 8.0).allowing_step()],
        );
        game.player_yaw = std::f32::consts::FRAC_PI_2;
        let apex = run_for(
            &mut game,
            &[Control::Jump, Control::MoveForward, Control::StrafeRight],
            0.5,
            pattern,
        );
        assert!(
            apex <= JUMP_APEX_M + STEP_EPS,
            "{label}: invalid step {apex}"
        );
        assert!(!game.grounded, "wall counted as support at {label}");
        assert!(game.player_position.x <= -PLAYER_RADIUS + CONTACT_EPS);
        assert!(game.player_position.z > 0.8, "{label}: no wall slide");
    }
}

#[test]
fn floor_two_walls_and_ceiling_corner_remain_nonpenetrating() {
    for (_, pattern) in FRAME_PATTERNS {
        let mut game = flat_game(
            Vec3::new(-0.6, 0.0, -0.6),
            vec![
                WallAabb::with_y(0.0, 0.0, -4.0, 0.01, 5.0, 4.01),
                WallAabb::with_y(-4.0, 0.0, 0.0, 4.01, 5.0, 0.01),
                WallAabb::with_y(-4.0, 2.05, -4.0, 4.0, 0.005, 4.0),
            ],
        );
        game.player_yaw = 135.0_f32.to_radians();
        for _ in 0_i32..5_i32 {
            let _run_for_status = run_for(
                &mut game,
                &[Control::Jump, Control::MoveForward],
                1.2,
                pattern,
            );
            let _run_for_status_2 = run_for(&mut game, &[], 0.1, pattern);
        }
        assert!(game.grounded);
        assert!(game.player_position.x <= -PLAYER_RADIUS + CONTACT_EPS);
        assert!(game.player_position.z <= -PLAYER_RADIUS + CONTACT_EPS);
    }
}

#[test]
fn thin_floor_and_overhead_slab_cannot_be_tunnelled_through() {
    for (_, pattern) in FRAME_PATTERNS {
        let slab = WallAabb::with_y(-2.0, 2.0, -2.0, 4.0, 0.0005, 4.0);
        let mut falling = flat_game(Vec3::new(0.0, 6.0, 0.0), vec![slab]);
        falling.vertical_velocity = -60.0;
        let _run_for_status = run_for(&mut falling, &[], 0.2, pattern);
        assert!(falling.grounded && (falling.feet_y() - slab.max_y).abs() <= CONTACT_EPS);
        let mut rising = flat_game(Vec3::ZERO, vec![slab]);
        rising.vertical_velocity = 60.0;
        rising.grounded = false;
        let _run_for_status_2 = run_for(&mut rising, &[], 0.1, pattern);
        assert!(rising.feet_y() + rising.body_height() < slab.min_y);
        assert!(rising.vertical_velocity < 0.0);
    }
}

#[test]
fn side_entry_into_a_low_room_ceiling_is_blocked_without_a_vertical_teleport() {
    let mut game = room_game(
        r#"[{"x":0,"z":-4,"width":4,"depth":8,"height":2.05},
            {"x":-4,"z":-4,"width":4,"depth":8,"height":8}]"#,
        Vec3::new(-0.4, 0.5, 0.0),
        vec![],
    );
    game.player_yaw = std::f32::consts::FRAC_PI_2;
    game.vertical_velocity = 1.0;
    let before = game.feet_y();
    frame(&mut game, &[Control::MoveForward], 0.1);
    assert!(game.player_position.x <= -PLAYER_RADIUS + CONTACT_EPS);
    assert!((game.feet_y() - before).abs() < 0.11);
    assert!(!game.grounded);
}

#[test]
fn steps_require_ground_and_full_head_clearance() {
    for height in [PLAYER_STEP_HEIGHT, PLAYER_STEP_HEIGHT + 0.01] {
        for ceiling in [2.0, 3.0] {
            let mut game = flat_game(
                Vec3::new(-0.6, 0.0, 0.0),
                vec![
                    WallAabb::with_y(0.0, 0.0, -2.0, 2.0, height, 4.0),
                    WallAabb::with_y(-2.0, ceiling, -2.0, 4.0, 0.01, 4.0),
                ],
            );
            game.player_yaw = std::f32::consts::FRAC_PI_2;
            let _run_for_status = run_for(&mut game, &[Control::MoveForward], 0.5, &[1.0 / 30.0]);
            if height <= PLAYER_STEP_HEIGHT && ceiling >= PLAYER_HEIGHT + height {
                assert!(game.player_position.x > 0.0);
            } else {
                assert!(game.player_position.x <= -PLAYER_RADIUS + CONTACT_EPS);
                assert!(game.feet_y().abs() <= CONTACT_EPS);
            }
        }
    }
}

#[test]
fn ledge_departure_and_fall_distance_are_equivalent_across_frame_rates() {
    let mut results = Vec::new();
    for (label, pattern) in FRAME_PATTERNS {
        let mut fall = room_game(
            r#"[{"x":-10,"z":-10,"width":20,"depth":20,"height":20}]"#,
            Vec3::new(0.0, 8.0, 0.0),
            vec![],
        );
        let _run_for_status = run_for(&mut fall, &[], 1.0, pattern);
        assert!(!fall.grounded);
        results.push((label, fall.feet_y(), fall.vertical_velocity));
        let mut ledge = flat_game(
            Vec3::new(-0.6, 1.5, 0.0),
            vec![WallAabb::with_y(-2.0, 0.0, -2.0, 2.0, 1.5, 4.0)],
        );
        ledge.player_yaw = std::f32::consts::FRAC_PI_2;
        assert!(ledge.grounded);
        let _run_for_status_2 = run_for(
            &mut ledge,
            &[Control::MoveForward, Control::StrafeRight],
            1.5,
            pattern,
        );
        assert!(
            ledge.feet_y().abs() < CONTACT_EPS && ledge.grounded,
            "{label}: ledge fall"
        );
    }
    for (label, feet, velocity) in &results {
        assert!(
            (feet - 3.1).abs() < 0.002,
            "{label}: ballistic fall {feet}, {results:?}"
        );
        assert!(
            (velocity + GRAVITY).abs() < 0.002,
            "{label}: gravity {velocity}"
        );
    }
    crate::logging::warn(format_args!("fall matrix {results:?}"));
}

#[test]
fn narrow_passage_crouch_and_stand_clearance_remain_physical() {
    let mut game = flat_game(
        Vec3::new(-1.0, 0.0, 0.0),
        vec![
            WallAabb::with_y(0.0, 1.1, -0.5, 2.0, 0.05, 1.0),
            WallAabb::with_y(-2.0, 0.0, -0.4, 6.0, 3.0, 0.05),
            WallAabb::with_y(-2.0, 0.0, 0.35, 6.0, 3.0, 0.05),
        ],
    );
    game.player_yaw = std::f32::consts::FRAC_PI_2;
    let _run_for_status = run_for(&mut game, &[Control::MoveForward], 0.4, &[1.0 / 60.0]);
    assert!(game.player_position.x < 0.0);
    frame(&mut game, &[Control::Crouch], 1.0 / 60.0);
    let _run_for_status_2 = run_for(&mut game, &[Control::MoveForward], 0.5, &[1.0 / 60.0]);
    assert!(game.player_position.x > 0.5);
    frame(&mut game, &[Control::Crouch], 1.0 / 60.0);
    assert_eq!(game.stance, Stance::Crouched);
    assert_no_box_penetration(&game);
}

#[test]
fn a_room_ceiling_is_support_from_above_and_never_a_downward_clamp() {
    let mut game = room_game(
        r#"[{"x":-4,"z":-4,"width":8,"depth":8,"height":2.4}]"#,
        Vec3::new(0.0, 2.4, 0.0),
        vec![],
    );
    assert!(game.grounded && (game.feet_y() - 2.4).abs() <= CONTACT_EPS);
    frame(&mut game, &[Control::Jump], 1.0 / 30.0);
    assert!(game.feet_y() > 2.4 && !game.grounded);
}

#[test]
fn grounded_floor_region_steps_never_embed_the_body_in_their_rims() {
    let level = LevelDef::from_json(
        r#"{
        "format_version":3,"id":"step_rim_regression","name":"step rims",
        "spawn":{"x":-1,"z":0},
        "rooms":[{"x":-4,"z":-4,"width":8,"depth":8,"height":4}],
        "floor_regions":[{"x":0,"z":-2,"width":2,"depth":4,"offset_y":0.4}]
    }"#,
    )
    .expect("valid step fixture");
    export_fixture(
        &serde_json::to_string(&level).expect("step source"),
        Vec3::new(-1.0, 0.0, 0.0),
        &[],
    );
    for (_, pattern) in FRAME_PATTERNS {
        let mut game = game_for(&level);
        play_at(&mut game, -1.0, 0.0, 0.0, 90.0, 0.0);
        let _run_for_status = run_for(&mut game, &[Control::MoveForward], 1.3, pattern);
        assert!(game.player_position.x > 2.5 && game.grounded);
        assert!(game.feet_y().abs() < CONTACT_EPS);
    }
}

#[test]
fn spawn_overlap_recovery_is_clear_bounded_and_deterministic() {
    let box_wall = WallAabb::with_y(-1.0, 0.0, -1.0, 2.0, 1.0, 2.0);
    let first = flat_game(Vec3::ZERO, vec![box_wall]);
    let second = flat_game(Vec3::ZERO, vec![box_wall]);
    assert_eq!(first.player_position, second.player_position);
    assert_no_box_penetration(&first);
    assert!(
        Vec3::new(
            first.player_position.x,
            first.feet_y(),
            first.player_position.z
        )
        .length()
            <= PLAYER_HEIGHT * 2.0
    );
    assert!((first.feet_y() - 1.0).abs() <= CONTACT_EPS * 2.0);
}

#[test]
fn a_floating_room_floor_blocks_both_a_jump_from_below_and_side_entry() {
    let rooms = r#"[{"x":-4,"z":-4,"width":8,"depth":8,"height":8},
        {"x":0,"z":-2,"width":2,"depth":4,"floor_y":1.95,"height":4,"ceiling":{"kind":"open"}}]"#;
    for (_, pattern) in FRAME_PATTERNS {
        let mut below = room_game(rooms, Vec3::new(1.0, 0.0, 0.0), vec![]);
        let apex = run_for(&mut below, &[Control::Jump], 1.2, pattern);
        assert!(apex <= 1.95 - PLAYER_HEIGHT + CONTACT_EPS);
        assert!(below.grounded);
        let mut side = room_game(rooms, Vec3::new(-0.4, 0.5, 0.0), vec![]);
        side.player_yaw = std::f32::consts::FRAC_PI_2;
        side.vertical_velocity = 1.0;
        let _run_for_status = run_for(&mut side, &[Control::MoveForward], 0.1, pattern);
        assert!(side.player_position.x <= -PLAYER_RADIUS + CONTACT_EPS);
        assert!(!side.grounded);
    }
}

#[test]
fn repeated_jump_cycles_preserve_apex_and_land_at_every_frame_pattern() {
    let mut results = Vec::new();
    for (label, pattern) in FRAME_PATTERNS {
        let mut game = flat_game(Vec3::ZERO, vec![]);
        let mut highest: f32 = 0.0;
        for _ in 0_i32..5_i32 {
            highest = highest.max(run_for(&mut game, &[Control::Jump], 1.2, pattern));
            assert!(game.grounded && game.feet_y().abs() <= CONTACT_EPS);
            let _run_for_status = run_for(&mut game, &[], 0.1, pattern);
        }
        assert!(
            (highest - JUMP_APEX_M).abs() < 0.002,
            "{label}: apex {highest}"
        );
        results.push((label, highest));
    }
    crate::logging::warn(format_args!("jump matrix {results:?}"));
}

#[test]
fn ceiling_impact_is_found_when_the_jump_apex_is_between_clear_endpoints() {
    let ceiling = PLAYER_HEIGHT + 0.99997;
    let mut game = flat_game(
        Vec3::new(0.0, 1.0 - 0.04 * 0.04 / (2.0 * GRAVITY), 0.0),
        vec![WallAabb::with_y(-2.0, ceiling, -2.0, 4.0, 0.001, 4.0)],
    );
    game.vertical_velocity = 0.04;
    game.grounded = false;
    assert!(game.feet_y() + PLAYER_HEIGHT < ceiling);
    frame(&mut game, &[], VERTICAL_SUBSTEP);
    assert!(game.ceiling_contact_this_frame);
    assert!(game.vertical_velocity <= 0.0 && game.feet_y() + PLAYER_HEIGHT <= ceiling);
}

#[test]
fn landing_on_a_room_roof_edge_uses_the_whole_body_disc() {
    let mut game = room_game(
        r#"[{"x":0,"z":-2,"width":4,"depth":4,"height":2.4},
            {"x":-4,"z":-2,"width":4,"depth":4,"height":8}]"#,
        Vec3::new(-0.15, 3.0, 0.0),
        vec![],
    );
    for _ in 0_i32..90_i32 {
        frame(&mut game, &[], 1.0 / 60.0);
        assert!(game.feet_y() >= 2.4 - CONTACT_EPS, "roof edge entered body");
    }
    assert!(game.grounded && (game.feet_y() - 2.4).abs() <= CONTACT_EPS);
}

#[test]
fn spawn_across_a_gabled_roof_uses_separate_bounded_recovery() {
    let mut game = room_game(
        r#"[{"x":-5,"z":-5,"width":10,"depth":10,"height":2.4,
            "ceiling":{"kind":"gable","ridge":"x","ridge_rise":2.0}}]"#,
        Vec3::new(0.0, 3.0, 3.5),
        vec![],
    );
    assert!(game.feet_y() > 3.1 && game.feet_y() < 3.2);
    assert!(game.body_fits_at(Vec3::new(0.0, game.feet_y(), 3.5)));
    for _ in 0_i32..60_i32 {
        frame(&mut game, &[], 1.0 / 60.0);
        assert!(game.feet_y() > 3.1);
    }
    assert!(game.grounded);
}
