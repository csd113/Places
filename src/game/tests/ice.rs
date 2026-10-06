//! Ice traction, transitions and support regressions through the real controller.
use super::*;
use crate::assets::AssetCatalog;
use crate::materials::{GroundSurface, MaterialTable};

pub(super) fn materials(level: &LevelDef) -> MaterialTable {
    let catalog = AssetCatalog::from_json_str(include_str!("../../../assets/catalog.json"))
        .expect("shipped material catalog parses");
    MaterialTable::logical(level, &catalog, None)
}

fn level() -> LevelDef {
    LevelDef::from_json(r#"{
        "format_version":3,"id":"ice_traction","name":"Ice traction",
        "spawn":{"x":0,"z":0},
        "defaults":{"wall":"core:concrete_01","floor":"winter:snow_01","ceiling":"core:concrete_01"},
        "rooms":[{"x":-20,"z":-20,"width":40,"depth":40,"height":8}],
        "floor_regions":[{"x":0,"z":-15,"width":15,"depth":30,"material":"winter:ice_01"}]
    }"#).expect("ice fixture parses")
}

fn at(level: &LevelDef, x: f32, z: f32, hz: f32) -> Game {
    let world = CollisionWorld::from_level(level).with_ground_materials(level, &materials(level));
    let y = world.floor.height_at(x, z).expect("fixture floor");
    let mut game = Game::new(Vec3::new(x, y + EYE_HEIGHT, z), 0.0, world);
    game.set_app_state(AppState::Playing);
    game.sim_delta_seconds = 1.0 / hz;
    game
}

fn advance(game: &mut Game, controls: &[Control], frames: usize) {
    let mut input = InputState::holding(controls);
    for _ in 0..frames {
        game.update_player_movement(&mut input, &Settings::default());
    }
}

#[test]
fn ice_coasts_brakes_and_turns_with_bounded_controllable_speed() {
    let level = level();
    let mut ice = at(&level, 4.0, 0.0, 60.0);
    let mut snow = at(&level, -4.0, 0.0, 60.0);
    for game in [&mut ice, &mut snow] {
        advance(game, &[Control::MoveForward], 60);
    }
    let start_ice = ice.player_position.z;
    let start_snow = snow.player_position.z;
    advance(&mut ice, &[], 60);
    advance(&mut snow, &[], 60);
    assert!(
        (snow.player_position.z - start_snow).abs() < 1e-5,
        "normal friction restores immediate stopping"
    );
    assert!(
        start_ice - ice.player_position.z > 1.3 && start_ice - ice.player_position.z < 1.6,
        "restrained one-second coast: {}",
        start_ice - ice.player_position.z
    );
    advance(&mut ice, &[], 240);
    assert!(
        ice.horizontal_velocity == Vec2::ZERO,
        "ice eventually stops"
    );
    advance(&mut ice, &[Control::MoveForward], 60);
    let before = ice.player_position.z;
    advance(&mut ice, &[Control::MoveBackward], 6);
    assert!(
        ice.player_position.z < before,
        "reversal spends forward momentum first"
    );
    advance(&mut ice, &[Control::MoveBackward], 54);
    assert!(
        ice.horizontal_velocity.y > 2.7,
        "reversal remains controllable"
    );
    advance(&mut ice, &[Control::MoveForward, Control::StrafeRight], 60);
    assert!(
        ice.horizontal_velocity.length() <= 3.01,
        "diagonal input never exceeds walk speed"
    );
}

#[test]
fn entering_and_leaving_ice_restores_normal_input_and_friction() {
    let level = level();
    let mut game = at(&level, -0.6, 0.0, 60.0);
    advance(&mut game, &[Control::StrafeRight], 20);
    assert!(game.player_position.x > 0.3, "walked onto ice");
    let before = game.player_position.x;
    advance(&mut game, &[], 15);
    assert!(
        game.player_position.x > before + 0.5,
        "entry preserves momentum"
    );
    advance(&mut game, &[Control::StrafeLeft], 90);
    assert!(game.player_position.x < 0.0, "walked back onto snow");
    let stopped = game.player_position;
    advance(&mut game, &[], 1);
    assert!(
        game.player_position.distance(stopped) < 1e-5,
        "first snow frame stops immediately"
    );
    assert!(
        game.horizontal_velocity == Vec2::ZERO,
        "no stored coast survives on snow"
    );
}

#[test]
fn jumping_keeps_ice_momentum_landing_resumes_traction_and_reset_clears_it() {
    let level = level();
    let mut game = at(&level, 4.0, 2.0, 60.0);
    advance(&mut game, &[Control::MoveForward], 60);
    let before = game.player_position;
    advance(&mut game, &[Control::Jump], 1);
    assert!(
        !game.grounded && game.vertical_velocity > 0.0,
        "ordinary ice jump"
    );
    advance(&mut game, &[], 30);
    assert!(
        game.player_position.z < before.z - 1.4,
        "airborne ice takeoff retains momentum"
    );
    assert!(game.feet_y > 0.8, "normal ballistic jump height");
    advance(&mut game, &[], 90);
    assert!(
        game.grounded && !game.swimming && game.feet_y.abs() < 1e-4,
        "lands without clipping or swimming"
    );
    assert!(
        game.horizontal_velocity.length() > 0.1,
        "landing coasts on ice"
    );
    game.reset_to_spawn();
    assert!(
        game.horizontal_velocity == Vec2::ZERO && !game.ice_airborne,
        "reset spends momentum"
    );
}

#[test]
fn ice_motion_is_consistent_across_frame_rates() {
    let level = level();
    let mut end_points = Vec::new();
    for (hz, second) in [(30.0, 30_usize), (60.0, 60), (144.0, 144)] {
        let mut game = at(&level, 4.0, 2.0, hz);
        advance(&mut game, &[Control::MoveForward], second);
        advance(&mut game, &[], second);
        advance(&mut game, &[Control::StrafeRight], second);
        end_points.push(game.player_position);
    }
    for point in &end_points {
        assert!(
            point.distance(*end_points.first().expect("frame-rate reference")) < 0.04,
            "frame-rate drift exceeds 4 cm: {end_points:?}"
        );
    }
}

#[test]
fn ice_wall_collision_spends_momentum_without_vertical_recovery() {
    let mut level = level();
    level.walls.push(
        serde_json::from_str(r#"{"x":6,"z":-10,"width":0.1,"depth":20,"height":3}"#)
            .expect("wall fixture"),
    );
    let mut game = at(&level, 4.0, 0.0, 30.0);
    advance(&mut game, &[Control::StrafeRight], 60);
    assert!(
        game.player_position.x <= 6.0 - PLAYER_RADIUS + 1e-4,
        "swept ice body stays outside thin wall"
    );
    assert!(
        game.feet_y.abs() < 1e-4 && game.grounded,
        "no vertical teleport at contact"
    );
    let blocked = game.player_position;
    advance(&mut game, &[], 30);
    assert!(
        game.player_position.distance(blocked) < 1e-4,
        "blocked momentum is gone"
    );
    advance(&mut game, &[Control::StrafeLeft], 30);
    assert!(
        game.player_position.x < blocked.x - 2.0,
        "can steer away from collision"
    );
}

#[test]
fn snow_overrides_and_slopes_sample_the_actual_support_material() {
    let mut level = level();
    level.floor_regions.push(
        serde_json::from_str(
            r#"{"x":7,"z":-2,"width":2,"depth":4,"offset_y":0.2,"material":"winter:snow_01"}"#,
        )
        .expect("snow region"),
    );
    level.ramps.push(serde_json::from_str(r#"{"x":2,"z":4,"width":3,"depth":4,"offset_y":0,"rise":0.3,"material":"winter:snow_packed_01"}"#).expect("snow ramp"));
    let surfaces = GroundSurfaces::from_level(&level, &materials(&level));
    assert_eq!(surfaces.at(4.0, 0.0, 0.0), GroundSurface::Ice);
    assert_eq!(
        surfaces.at(4.0, 0.0, 0.9),
        GroundSurface::Normal,
        "a prop top over ice stays ordinary"
    );
    assert_eq!(
        surfaces.at(8.0, 0.0, 0.2),
        GroundSurface::Normal,
        "later snow region wins"
    );
    assert_eq!(
        surfaces.at(3.0, 5.0, 0.075),
        GroundSurface::Normal,
        "snow ramp overrides ice below"
    );
    let mut game = at(&level, 3.5, 7.5, 60.0);
    advance(&mut game, &[Control::MoveForward], 90);
    assert!(
        game.player_position.z < 3.2 && game.grounded,
        "snow slope reaches ice: {:?}, grounded={}",
        game.player_position,
        game.grounded
    );
    assert!(
        game.feet_y.abs() < 1e-4,
        "ramp exit preserves physical support"
    );
    advance(&mut game, &[Control::MoveBackward], 120);
    assert!(
        game.player_position.z > 4.5 && game.grounded,
        "ice reversal climbs snow slope"
    );
    let before = game.player_position;
    advance(&mut game, &[], 1);
    assert!(
        game.player_position.distance(before) < 1e-5,
        "ramp immediately restores normal friction"
    );
}

#[test]
fn liquid_water_materials_and_non_ice_worlds_keep_their_old_behavior() {
    let mut level = level();
    level.floor_regions.clear();
    level.water.push(
        serde_json::from_str(
            r#"{"x":-10,"z":-10,"width":20,"depth":20,"surface_y":2,"bottom_y":0}"#,
        )
        .expect("liquid water"),
    );
    let mut game = at(&level, 0.0, 0.0, 60.0);
    advance(&mut game, &[], 60);
    assert!(game.swimming, "normal water still swims");
    assert!(
        !game.ice_airborne,
        "liquid volumes never become slippery ground"
    );
}
