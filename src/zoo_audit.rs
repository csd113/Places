//! Audits for the shipped `Model Zoo` and the capacity fixtures.
//!
//! The zoo is generated (`tools/levels/build_model_zoo.py`), so these tests are
//! the contract that generation has to satisfy: every registered placeable is
//! displayed, every required demonstration exists, every instance id is stable
//! and unique, and the level loads, validates and collides like any other
//! shipped map. The capacity fixtures are audited here too: the sparse map
//! proves the far-from-origin behaviour, the dense map proves the raised
//! instance/asset/light budgets, and both are the witness set for the collision
//! index's equality with the linear scan.
//!
//! Nothing here bakes a lightmap or uploads anything: parse, validate, resolve
//! the collision world and query it. That keeps the audit cheap enough to run
//! on every checkout while still failing loudly when a generated level drifts.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
// idiomatic in tests; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::collections::HashSet;

use crate::collision_index::CollisionIndex;
use crate::game::{CollisionWorld, Game};
use crate::level::LevelDef;

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{path} must be readable: {error}"))
}

/// The shipped zoo, parsed and validated.
fn zoo() -> LevelDef {
    let level = LevelDef::from_json(&read("assets/levels/model_zoo.json"))
        .expect("the shipped Model Zoo parses");
    crate::loader::validate_level(&level).expect("the shipped Model Zoo validates");
    level
}

fn capacity(name: &str) -> LevelDef {
    let path = format!("tests/fixtures/levels/{name}.json");
    let level = LevelDef::from_json(&read(&path)).unwrap_or_else(|error| panic!("{path}: {error}"));
    crate::loader::validate_level(&level).unwrap_or_else(|error| panic!("{path}: {error}"));
    level
}

/// Every catalogued placeable id, from the shipped catalog.
fn catalog_placeables() -> Vec<String> {
    let catalog = crate::loader::PropCatalog::load_default();
    let mut ids: Vec<String> = catalog
        .assets()
        .entries()
        .into_iter()
        .filter(|entry| entry.is_placeable())
        .map(|entry| entry.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn the_zoo_displays_every_catalogued_placeable() {
    let level = zoo();
    let displayed: HashSet<String> = level.props.iter().map(|prop| prop.model.clone()).collect();
    let mut missing: Vec<String> = catalog_placeables()
        .into_iter()
        .filter(|id| !displayed.contains(id))
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "the Model Zoo must display every placeable; missing {missing:?}"
    );
}

#[test]
fn the_zoo_instance_ids_are_stable_and_unique() {
    let level = zoo();
    let ids = level.prop_instance_ids();
    let unique: HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "zoo instance ids must be unique");
    // The generator's stable scheme: `zoo:<catalog-id>:<role>`.
    for id in &ids {
        assert!(
            id.starts_with("zoo:"),
            "instance id `{id}` must use the generated `zoo:` namespace"
        );
        assert!(
            id.matches(':').count() >= 2,
            "instance id `{id}` must carry a display role"
        );
    }
}

#[test]
fn the_zoo_contains_every_required_demonstration() {
    let level = zoo();
    let by_role = |role: &str| -> Vec<&crate::level::PropDef> {
        level
            .props
            .iter()
            .filter(|prop| {
                prop.id
                    .as_deref()
                    .is_some_and(|id| id.ends_with(&format!(":{role}")))
            })
            .collect()
    };
    // Three mannequin poses and three skeleton poses, each with its own clip.
    let clips: Vec<String> = level
        .props
        .iter()
        .filter(|prop| prop.model == "mannequin" || prop.model == "skeleton")
        .filter_map(|prop| {
            prop.interaction.as_ref().and_then(|interaction| {
                interaction.actions.iter().find_map(|action| match action {
                    crate::level::ActionDef::PlayAnimation { clip, .. } => clip.clone(),
                    crate::level::ActionDef::ToggleLabel { .. }
                    | crate::level::ActionDef::ResetToStart
                    | crate::level::ActionDef::ToggleAnimation { .. }
                    | crate::level::ActionDef::PlayAudio { .. }
                    | crate::level::ActionDef::OpenDoor { .. }
                    | crate::level::ActionDef::CloseDoor { .. }
                    | crate::level::ActionDef::Toggle { .. } => None,
                })
            })
        })
        .collect();
    for required in [
        "pose_stand",
        "pose_arms_up",
        "pose_arms_forward",
        "pose_sit_floor",
        "pose_sit_chair",
    ] {
        assert!(
            clips.iter().any(|clip| clip == required),
            "the zoo must demonstrate `{required}`; found {clips:?}"
        );
    }
    // The skeleton's chair pose must sit on a real chair, at the documented
    // offset (the chair's seat, not an empty pose floating in the air).
    let chair_sit = level
        .props
        .iter()
        .find(|prop| {
            prop.id
                .as_deref()
                .is_some_and(|id| id.ends_with(":chair-sit"))
        })
        .expect("the zoo places a chair-sitting skeleton");
    let chair = level
        .props
        .iter()
        .find(|prop| {
            prop.id
                .as_deref()
                .is_some_and(|id| id.ends_with(":skeleton-seat"))
        })
        .expect("the zoo places the skeleton's chair");
    assert_eq!(chair.model, "core:chair");
    assert!(
        (chair.x - chair_sit.x).abs() < 0.02 && (chair.z - chair_sit.z).abs() < 0.02,
        "the chair and the seated skeleton share one anchor: {chair:?} {chair_sit:?}"
    );
    // Independently routed copies: two rats (walk and run) and two Spooner-Man
    // instances, each with its own route and pose.
    assert_eq!(
        by_role("route").iter().filter(|p| p.model == "rat").count(),
        1
    );
    assert_eq!(
        by_role("run").iter().filter(|p| p.model == "rat").count(),
        1
    );
    assert_eq!(
        level
            .props
            .iter()
            .filter(|p| p.model == "spooner-man")
            .count(),
        2,
        "the zoo demonstrates an independently animated Spooner-Man copy"
    );
    assert_eq!(
        level
            .routes
            .iter()
            .filter(|route| route.steps.iter().any(|step| matches!(
                step,
                crate::level::RouteStepDef::Play { clip, .. } if clip == "sit_down"
            )))
            .count(),
        1,
        "one Spooner-Man route runs the full sit/wait/stand sequence"
    );
    // Two wall switches with their own toggle interaction.
    let toggles = level
        .props
        .iter()
        .filter(|prop| {
            prop.interaction.as_ref().is_some_and(|interaction| {
                interaction
                    .actions
                    .iter()
                    .any(|action| matches!(action, crate::level::ActionDef::ToggleAnimation { .. }))
            })
        })
        .count();
    assert!(
        toggles >= 2,
        "the zoo demonstrates independently operating switches; found {toggles}"
    );
    // The duck floats in a real water volume.
    let duck = level
        .props
        .iter()
        .find(|prop| prop.model == "core:rubber_duck")
        .expect("the zoo displays the duck");
    assert!(duck.float.is_some(), "the duck must float");
    let sample = crate::level::WaterVolumes::from_level(&level).sample(duck.x, duck.z, -0.25);
    assert!(
        sample.is_some(),
        "the duck's water display must be a real water volume"
    );
    // The two luminous props own lights, so they actually illuminate.
    for id in ["core:exit_sign", "home:ball_light"] {
        let prop = level
            .props
            .iter()
            .find(|prop| prop.model == id)
            .unwrap_or_else(|| panic!("the zoo displays {id}"));
        assert!(
            !prop.lights.is_empty(),
            "{id} must author its own light so the showroom demonstration illuminates"
        );
    }
    // Ceiling-grid-aligned vents on the pool ceiling.
    assert!(
        level
            .decals
            .iter()
            .any(|decal| decal.align == crate::level::DecalAlign::CeilingGrid),
        "the zoo must demonstrate ceiling-grid-aligned vent decals"
    );
    // Curved architecture in more than one material.
    assert!(level.arc_walls.len() >= 2);
    assert!(level.pillars.len() >= 2);
    let curved_materials: HashSet<&String> = level
        .arc_walls
        .iter()
        .flat_map(|arc| [arc.material.as_ref(), arc.inner_material.as_ref()])
        .chain(
            level
                .pillars
                .iter()
                .flat_map(|pillar| [pillar.material.as_ref(), pillar.cap_material.as_ref()]),
        )
        .flatten()
        .collect();
    assert!(
        curved_materials.len() >= 2,
        "curved examples must use more than one material: {curved_materials:?}"
    );
}

#[test]
fn the_zoo_light_props_really_illuminate() {
    let level = zoo();
    let lighting = crate::lighting::LevelLighting::bake(&level);
    let exit = level
        .props
        .iter()
        .find(|prop| prop.model == "core:exit_sign")
        .expect("the zoo places the exit sign");
    let ball = level
        .props
        .iter()
        .find(|prop| prop.model == "home:ball_light")
        .expect("the zoo places the ball light");
    // Both props hang at the ceiling (the sign's face and the orb sit just
    // below it); sample the air right in front of the sign and under the orb,
    // inside each light's own range.
    let exit_sample = lighting.sample(exit.x, 4.2, exit.z + 0.5);
    let ball_sample = lighting.sample(ball.x, 3.6, ball.z);
    assert!(
        exit_sample.g > exit_sample.r,
        "the exit sign casts green: {exit_sample:?}"
    );
    assert!(
        ball_sample.r > ball_sample.b,
        "the ball light casts warm white: {ball_sample:?}"
    );
    let mut dark = level.clone();
    for prop in &mut dark.props {
        for light in &mut prop.lights {
            light.enabled = false;
        }
    }
    let dark_lighting = crate::lighting::LevelLighting::bake(&dark);
    let exit_dark = dark_lighting.sample(exit.x, 4.2, exit.z + 0.5);
    let ball_dark = dark_lighting.sample(ball.x, 3.6, ball.z);
    assert!(
        exit_sample.g > exit_dark.g + 0.01,
        "the sign's light adds green to the hall: {exit_sample:?} vs {exit_dark:?}"
    );
    assert!(
        ball_sample.r > ball_dark.r + 0.01,
        "the lamp adds light under its orb: {ball_sample:?} vs {ball_dark:?}"
    );
}

#[test]
fn the_zoo_spawn_is_clear_and_on_the_floor() {
    let level = zoo();
    let world = CollisionWorld::from_level(&level);
    let spawn = crate::game::spawn_position(&level);
    assert!(
        world
            .floor
            .walk_height_at(level.spawn.x, level.spawn.z)
            .is_some(),
        "the zoo spawn must stand on a real floor"
    );
    for wall in &world.walls {
        assert!(
            !wall.overlaps_disc(
                level.spawn.x,
                level.spawn.z,
                crate::collision::PLAYER_RADIUS * 1.5
            ),
            "the zoo spawn must have clear space around it; blocked by {wall:?}"
        );
    }
    assert!(
        world
            .ceiling
            .ceiling_y_at(level.spawn.x, level.spawn.z)
            .is_some_and(|ceiling| ceiling >= spawn.y + 0.2),
        "the zoo spawn must have headroom"
    );
}

#[test]
fn the_zoo_is_well_lit_by_its_own_fixtures() {
    let level = zoo();
    assert!(
        level.ceiling_lights.len() >= 60,
        "the showroom needs a real lighting grid, found {}",
        level.ceiling_lights.len()
    );
    // The fixtures must cover the whole hall, not cluster over one corner: the
    // general illumination may not depend on the two emissive display props.
    let room = level.room_iter().next().expect("the zoo has a room");
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for light in &level.ceiling_lights {
        min_x = min_x.min(light.x);
        max_x = max_x.max(light.x);
        min_z = min_z.min(light.z);
        max_z = max_z.max(light.z);
    }
    assert!(
        max_x - min_x >= room.width * 0.6 && max_z - min_z >= room.depth * 0.6,
        "the fixture grid must span the hall: x {min_x}..{max_x} of {}, z {min_z}..{max_z} of {}",
        room.width,
        room.depth
    );
}

#[test]
fn the_dense_capacity_fixture_holds_and_collides_at_scale() {
    let level = capacity("capacity_dense");
    assert!(
        level.props.len() > 5_000,
        "the dense fixture must exceed the historical 5000-prop cap; found {}",
        level.props.len()
    );
    let models: HashSet<&String> = level.props.iter().map(|prop| &prop.model).collect();
    assert!(
        models.len() >= 40,
        "the dense fixture must carry real unique-asset pressure; found {}",
        models.len()
    );
    assert!(level.ceiling_lights.len() >= 100);
    assert!(level.routes.len() >= 16);
    let world = CollisionWorld::from_level(&level);
    assert!(
        world.walls.len() > 2_000,
        "the dense fixture's collision world must be large: {} boxes",
        world.walls.len()
    );
    // The indexed queries must agree with the linear scan over the real world.
    let index = CollisionIndex::build(&world.walls);
    let mut state = 0x51E3D_u32;
    for _ in 0..256 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        #[allow(clippy::cast_precision_loss)]
        let x = (state % 40_000) as f32 / 100.0 - 200.0;
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        #[allow(clippy::cast_precision_loss)]
        let z = (state % 40_000) as f32 / 100.0 - 200.0;
        assert_eq!(
            crate::collision::highest_support_top(x, z, 3.0, &world.walls),
            crate::collision::highest_support_top_indexed(&index, x, z, 3.0, &world.walls),
            "support at ({x}, {z})"
        );
        assert_eq!(
            crate::collision::resolve_player_collision_for_body(
                glam::Vec2::new(x, z),
                crate::collision::PLAYER_RADIUS,
                0.0,
                crate::collision::PLAYER_HEIGHT,
                &world.walls,
            ),
            crate::collision::resolve_player_collision_for_body_indexed(
                &index,
                glam::Vec2::new(x, z),
                crate::collision::PLAYER_RADIUS,
                0.0,
                crate::collision::PLAYER_HEIGHT,
                &world.walls,
            ),
            "resolve at ({x}, {z})"
        );
    }
}

#[test]
fn the_sparse_capacity_fixture_works_kilometres_from_the_origin() {
    let level = capacity("capacity_sparse");
    let world = CollisionWorld::from_level(&level);
    // Every room's floor, collision, water, trigger and route is a satellite
    // at ±2 km; the controller must stand, walk and collide there.
    assert!(level.props.iter().any(|prop| prop.x.abs() > 1_900.0));
    assert!(!level.water.is_empty());
    assert!(!level.area_triggers.is_empty());
    assert!(!level.routes.is_empty());
    for room in level.room_iter() {
        let cx = room.x + room.width * 0.5;
        let cz = room.z + room.depth * 0.5;
        assert!(
            world.floor.walk_height_at(cx, cz).is_some(),
            "the walkable floor must resolve at ({cx}, {cz})"
        );
        assert!(
            world.walls.iter().any(|wall| {
                wall.overlaps_disc(room.x, room.z, 0.5)
                    || wall.overlaps_disc(room.x + room.width, room.z + room.depth, 0.5)
            }),
            "the room's own shell must be in the collision world at ({cx}, {cz})"
        );
    }
    // A rat walk far from the origin: the route must advance, not stall.
    let routes = &world.routes;
    let route = routes
        .get("far_rat")
        .expect("the sparse fixture routes a rat");
    let mut state = route.new_state();
    let index = CollisionIndex::build(&world.walls);
    let route_world = crate::entity::RouteWorld {
        walls: &world.walls,
        floor: &world.floor,
        index: &index,
    };
    let start = state.position;
    for _ in 0..600 {
        route.advance(&mut state, 1.0 / 60.0, &route_world);
    }
    assert!(
        (state.position - start).length() > 1.0,
        "the 2 km route must actually move; travelled {}",
        (state.position - start).length()
    );
    assert!(!state.blocked, "the 2 km route must not stall: {state:?}");
    // The reset trigger in the opposite quadrant must be reachable and bounded.
    let trigger = &world.triggers;
    assert!(
        !trigger.is_empty(),
        "the sparse fixture authors a reset trigger"
    );
}

#[test]
fn the_raised_caps_accept_content_past_the_old_boundary() {
    // The historical loader caps were 500 rooms / 5000 walls / 5000 lights /
    // 5000 props. A level one past each boundary must load and collide; the
    // raised caps are only real if the engine actually handles them.
    let mut rooms: Vec<String> = Vec::new();
    let mut walls: Vec<String> = Vec::new();
    let mut props: Vec<String> = Vec::new();
    let mut lights: Vec<String> = Vec::new();
    for index in 0..5_001 {
        let x = (index % 71) as f32 * 2.0;
        let z = (index / 71) as f32 * 2.0;
        props.push(format!(
            r#"{{"id": "p{index}", "model": "core:crate", "x": {x}, "z": {z}, "size": [0.6, 0.6, 0.6], "solid": false}}"#
        ));
    }
    for index in 0..5_001 {
        let x = (index % 71) as f32 * 2.0 + 1.0;
        let z = (index / 71) as f32 * 2.0 + 1.0;
        lights.push(format!(
            r#"{{"id": "l{index}", "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}}}"#
        ));
    }
    for index in 0..5_001 {
        let x = (index % 71) as f32 * 2.0 + 0.5;
        let z = (index / 71) as f32 * 2.0 + 0.5;
        walls.push(format!(
            r#"{{"x": {x}, "z": {z}, "width": 0.3, "depth": 0.3, "height": 2.0}}"#
        ));
    }
    for index in 0..501 {
        let x = (index % 23) as f32 * 6.0;
        let z = (index / 23) as f32 * 6.0;
        rooms.push(format!(
            r#"{{"x": {x}, "z": {z}, "width": 5.0, "depth": 5.0, "height": 3.0}}"#
        ));
    }
    let json = format!(
        r#"{{"format_version": 2, "id": "beyond_old_caps", "name": "Beyond Old Caps",
            "spawn": {{"x": 1.0, "z": 1.0}},
            "rooms": [{}], "walls": [{}], "ceiling_lights": [{}], "props": [{}]}}"#,
        rooms.join(","),
        walls.join(","),
        lights.join(","),
        props.join(",")
    );
    let level = LevelDef::from_json(&json).expect("a level past the old caps parses");
    crate::loader::validate_level(&level).expect("a level past the old caps validates");
    let world = CollisionWorld::from_level(&level);
    assert!(world.walls.len() > 5_000);
    let index = CollisionIndex::build(&world.walls);
    assert_eq!(index.len(), world.walls.len());
    // One of the 0.3 m wall stubs sits at (0.5, 0.5)..(0.8, 0.8); a point over
    // it must resolve support through both the linear and the indexed query.
    assert_eq!(
        crate::collision::highest_support_top(0.6, 0.6, 3.0, &world.walls),
        crate::collision::highest_support_top_indexed(&index, 0.6, 0.6, 3.0, &world.walls),
    );
    assert!(
        crate::collision::highest_support_top(0.6, 0.6, 3.0, &world.walls).is_some(),
        "the indexed support query must answer in a 5001-wall world"
    );
    // A game built on it takes its first movement step without panicking.
    let mut game = Game::new(crate::game::spawn_position(&level), 0.0, world);
    game.set_app_state(crate::game::AppState::Playing);
    let mut input = crate::input::InputState::default();
    let settings = crate::settings::Settings::default();
    game.update_player_movement(&mut input, &settings);
}
