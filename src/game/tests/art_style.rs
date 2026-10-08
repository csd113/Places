//! Hero-only traversal and compiled navigation checks for decorative refinement.

use super::*;

fn exercise_routes(mut game: Game) -> serde_json::Value {
    let settings = Settings::default();
    game.set_app_state(AppState::Playing);
    game.reset_spawn_point(Vec3::new(5.0, EYE_HEIGHT, 4.2), 90.0_f32.to_radians());
    game.sim_delta_seconds = 1.0 / 60.0;
    let mut endpoints = Vec::new();
    for (yaw, frames, target) in [
        (90.0_f32, 82_usize, Vec2::new(9.1, 4.2)),
        (180.0, 60_usize, Vec2::new(9.1, 7.2)),
        (0.0, 60_usize, Vec2::new(9.1, 4.2)),
        (270.0, 82_usize, Vec2::new(5.0, 4.2)),
    ] {
        game.player_yaw = yaw.to_radians();
        let mut input = InputState::holding(&[Control::MoveForward]);
        for _ in 0..frames {
            game.update_player_movement(&mut input, &settings);
            assert!(game.player_position.is_finite());
            assert!(game.feet_y().abs() < 1.0e-4, "trim must not become a step");
            for wall in &game.walls {
                if wall.blocks_body(game.feet_y(), game.body_height()) {
                    assert!(
                        !wall.overlaps_disc(
                            game.player_position.x,
                            game.player_position.z,
                            PLAYER_RADIUS - CONTACT_EPS
                        ),
                        "hero body penetrates {wall:?} at {:?}",
                        game.player_position
                    );
                }
            }
        }
        let actual = Vec2::new(game.player_position.x, game.player_position.z);
        assert!(
            actual.distance(target) < 0.03,
            "hero route: {actual:?} != {target:?}"
        );
        endpoints.push(game.player_position.to_array());
    }
    assert!(
        game.entities()
            .handle_of("comparison_dynamic_chair")
            .is_some()
    );
    assert!(game.entities().handle_of("coverage_ghost").is_some());
    serde_json::json!({"frames":284_usize,"collision_boxes":game.walls.len(),"route_endpoints":endpoints,
        "standing_height_m":game.body_height(),"radius_m":PLAYER_RADIUS,
        "runtime_chair_and_ghost_spawned":true})
}

#[test]
fn hero_refinement_keeps_living_room_and_hall_traversable() {
    for source in [
        include_str!("../../../tests/fixtures/levels/art_style_hero_transparency.json"),
        include_str!("../../../tests/fixtures/levels/art_style_hero_refined.json"),
    ] {
        let level = LevelDef::from_json(source).expect("hero source");
        drop(exercise_routes(game_for(&level)));
    }
}

#[test]
fn hero_decorations_do_not_add_collision_and_prior_surface_controls_survive() {
    let mut baseline: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/levels/art_style_hero_transparency.json"
    ))
    .expect("baseline");
    let refined: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/levels/art_style_hero_refined.json"
    ))
    .expect("refined");
    let template = baseline
        .get_mut("spawn_templates")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|templates| {
            templates.iter_mut().find(|template| {
                template.get("id").and_then(serde_json::Value::as_str) == Some("comparison_chair")
            })
        })
        .and_then(serde_json::Value::as_object_mut)
        .expect("comparison template");
    drop(template.insert(
        "model".to_owned(),
        serde_json::json!("home:dining_chair_refined"),
    ));
    for field in [
        "spawn",
        "walls",
        "sky",
        "global_illuminators",
        "ceiling_lights",
        "timers",
        "spawn_templates",
        "spawn_points",
        "floor_regions",
        "water",
        "void_walls",
    ] {
        assert_eq!(baseline.get(field), refined.get(field), "preserve {field}");
    }
    let before = LevelDef::from_json(&baseline.to_string()).expect("before");
    let after = LevelDef::from_json(&refined.to_string()).expect("after");
    assert_eq!(
        before.collision_aabbs().len(),
        after.collision_aabbs().len()
    );
    let table = after
        .props
        .iter()
        .find(|prop| prop.id.as_deref() == Some("coffee_table"))
        .expect("the table remains");
    assert!(
        after
            .collision_aabbs()
            .iter()
            .any(|wall| wall.overlaps_disc(table.x, table.z, PLAYER_RADIUS))
    );
}

#[test]
#[ignore = "explicit preserved hero package and asset root required; compiled route audit"]
fn audit_compiled_hero_routes() -> Result<(), String> {
    let path = std::env::var("PLACES_HERO_ROUTE_PACKAGE").map_err(|error| error.to_string())?;
    let output = std::env::var("PLACES_HERO_ROUTE_OUT").map_err(|error| error.to_string())?;
    let opened = crate::package::world::open(std::path::Path::new(&path))?;
    let mut assets = crate::props::PropAssets::load_default();
    let variant = crate::package::world::load_variant(
        std::path::Path::new(&path),
        &opened.manifest,
        crate::quality::LightmapQuality::Full,
        &mut assets,
    )?;
    let mesh = crate::nav::NavMesh::from_record(variant.navigation)?;
    let class = mesh
        .class_index(&crate::nav::reference_class())
        .ok_or("hero lacks player nav class")?;
    let mut scratch = crate::nav::NavScratch::new();
    for (start, goal) in [
        (Vec3::new(5.0, 0.0, 4.2), Vec3::new(9.1, 0.0, 7.2)),
        (Vec3::new(9.1, 0.0, 7.2), Vec3::new(5.0, 0.0, 4.2)),
    ] {
        assert!(
            matches!(
                mesh.path(
                    &crate::nav::PathQuery {
                        class,
                        start,
                        goal,
                        can_open_doors: true,
                        max_expansions: 65_536,
                        doors: &crate::nav::NoDoors
                    },
                    &mut scratch
                ),
                crate::nav::PathResult::Path(_)
            ),
            "compiled hero navigation must cross the passage both ways"
        );
    }
    let nav_cells = mesh.grid().cell_count();
    let world = CollisionWorld::from_compiled(&opened.level, variant.collision, Some(mesh));
    let game = Game::new(
        spawn_position(&opened.level),
        opened.level.spawn.yaw_degrees.to_radians(),
        world,
    );
    let report = serde_json::json!({"movement":exercise_routes(game),"package":path,
        "navigation_cells":nav_cells,"navigation_both_directions":true});
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
