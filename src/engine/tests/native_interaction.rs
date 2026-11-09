//! Value: reproduce a native Feltling's self-occluded model aim box and label,
//! then verify owner-index compaction and moving/reset poses. Two tiny rooms,
//! two native definitions, one coarse CPU navigation grid and tens of ticks;
//! uses canonical character assets, with no lighting bake, GPU or full map.
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, WorldContext};
use crate::engine::interact::{
    clear_interactable_line_of_sight_indexed, nearest_target, nearest_target_indexed,
};
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};
use glam::Vec3;

fn fixture(other_z: f32, other_x: f32) -> Result<LevelDef, String> {
    LevelDef::from_json(&format!(r#"{{"schema":"places-level","format_version":1,"id":"native_aim","name":"Native aim","spawn":{{"x":8,"z":8}},"rooms":[{{"x":-5,"z":-5,"width":15,"depth":15}}],"props":[
    {{"id":"other","model":"feltling","x":{other_x},"z":{other_z},"solid":false,"feltling":{{"mode":"wait"}}}},
    {{"id":"actor","model":"feltling","x":0,"z":-2,"size":[0.331083,0.60982,0.439302],"solid":false,"feltling":{{"mode":"flee","routes":[{{"id":"leave","points":[[2,0,-2]]}}]}},"components":[{{"component":"interactable","prompt":"Talk"}}]}}
    ]}}"#)).map_err(|e|e.to_string())
}

fn visible(
    world: &EntityWorld,
    walls: &[WallAabb],
    leaves: &[DoorCollider],
) -> Result<(bool, bool), String> {
    let item = world
        .interactables()
        .items()
        .iter()
        .find(|i| i.id == "actor")
        .ok_or("missing actor")?;
    let x = item.bounds.centre()[0];
    let eye = Vec3::new(x, 0.4, 0.0);
    let index = CollisionIndex::build(walls);
    let aimed = nearest_target_indexed(
        eye,
        Vec3::NEG_Z,
        world.interactables().items(),
        &index,
        walls,
        leaves,
    );
    assert_eq!(
        aimed,
        nearest_target(
            eye,
            Vec3::NEG_Z,
            world.interactables().items(),
            walls,
            leaves
        ),
        "linear/indexed ownership must agree"
    );
    Ok((
        aimed.is_some(),
        clear_interactable_line_of_sight_indexed(
            Vec3::new(x, 0.2, 0.0),
            item.anchor,
            item,
            &index,
            walls,
            leaves,
        ),
    ))
}

#[test]
fn native_body_excludes_only_its_owner_for_aim_and_labels() -> Result<(), String> {
    let world = EntityWorld::from_level(&fixture(-2.0, 3.0)?);
    assert_eq!(visible(&world, &[], world.door_colliders())?, (true, true));
    let wall = [WallAabb::with_y(-0.5, 0.0, -1.0, 1.0, 2.0, 0.1)];
    assert_eq!(
        visible(&world, &wall, world.door_colliders())?,
        (false, false),
        "wall still blocks"
    );
    let mut leaves = world.door_colliders().to_vec();
    leaves.push(DoorCollider::from_pose(
        [-0.5, 0.0, -1.0],
        [1.0, 0.0],
        1.0,
        0.1,
        2.0,
    ));
    assert_eq!(
        visible(&world, &[], &leaves)?,
        (false, false),
        "unrelated door still blocks"
    );
    let blocked = EntityWorld::from_level(&fixture(-1.0, 0.0)?);
    assert_eq!(
        visible(&blocked, &[], blocked.door_colliders())?,
        (false, false),
        "other native creature still blocks"
    );
    Ok(())
}

#[test]
fn native_body_ranges_follow_live_motion_compaction_and_reload() -> Result<(), String> {
    let level = fixture(-2.0, 3.0)?;
    let mut world = EntityWorld::from_level(&level);
    let range = |candidate: &EntityWorld| {
        candidate
            .interactables()
            .items()
            .iter()
            .find(|i| i.id == "actor")
            .and_then(|i| i.body_range)
    };
    assert_eq!(range(&world), Some((1, 2)));
    let _report = world.dispatch_on("other", &[ActionDef::Disable { target: None }]);
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    let ceiling = crate::engine::level::WalkableCeiling::from_level(&level);
    let doors = crate::engine::door::Doors::from_level(&level);
    let classes = [crate::engine::feltling::navigation_class()];
    let (grid, _navigation_report) = crate::map_compiler::navigation::bake(
        &crate::map_compiler::navigation::NavBakeInput {
            level: &level,
            walls: &[],
            floor: &floor,
            ceiling: &ceiling,
            doors: &doors,
            classes: &classes,
            obstacles: &[],
            walk_proxies: &[],
        },
        &crate::map_compiler::navigation::NavBakeOptions {
            cell_m: 0.5,
            workers: 1,
        },
    )?;
    let nav = crate::engine::nav::NavMesh::from_record(grid)?;
    for _ in 0_u8..30 {
        let _tick = world.tick(&WorldContext {
            player_walk_speed: 3.75,
            delta_seconds: 0.02,
            feet_from: Vec3::splat(8.0),
            feet: Vec3::splat(8.0),
            eye: Vec3::new(8.0, 1.6, 8.0),
            body_height: 1.8,
            body_half: 0.4,
            form_model: None,
            walls: &[],
            index: &index,
            floor: &floor,
            nav: Some(&nav),
        });
    }
    assert_eq!(
        range(&world),
        Some((0, 1)),
        "disabled prior owner cannot leave stale indices"
    );
    let item = world
        .interactables()
        .items()
        .iter()
        .find(|i| i.id == "actor")
        .ok_or("missing actor")?;
    assert!(
        item.bounds.centre()[0] > 0.5,
        "native route republishes the moving model target"
    );
    assert_eq!(visible(&world, &[], world.door_colliders())?, (true, true));
    world.resolve(&level);
    assert_eq!(range(&world), Some((1, 2)));
    assert_eq!(visible(&world, &[], world.door_colliders())?, (true, true));
    Ok(())
}

/// Value: the reproduced disc/square corner mismatch stranded the player in
/// the native guide's published body. Six tiny CPU admission queries cover
/// crouched/standing overlap, separated space and vertical exclusion; no bake.
#[test]
fn native_feltling_cannot_admit_a_player_hull_corner_overlap() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"guide_player_corner",
        "name":"Guide player corner","spawn":{"x":3,"z":3},
        "rooms":[{"x":-3,"z":-3,"width":6,"depth":6}],
        "props":[{"id":"actor","model":"feltling","x":0,"z":0,
        "solid":false,"feltling":{"mode":"wait"}}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let actors = crate::engine::feltling::load(&level)?;
    let actor = actors.first().ok_or("native actor absent")?;
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    for height in [0.9, 1.8] {
        for (feet, expected) in [
            (Vec3::new(0.746_958, 0.0, 0.599_512), false),
            (Vec3::new(0.81, 0.0, 0.599_512), true),
            (Vec3::new(0.746_958, 1.0, 0.599_512), true),
        ] {
            let ctx = WorldContext {
                delta_seconds: 1.0 / 60.0,
                player_walk_speed: 3.75,
                feet_from: feet,
                feet,
                eye: Vec3::new(feet.x, feet.y, feet.z),
                body_height: height,
                body_half: 0.4,
                form_model: None,
                walls: &[],
                index: &index,
                floor: &floor,
                nav: None,
            };
            assert_eq!(
                actor.clear(&ctx, &[], &[]),
                expected,
                "native square body must preserve player hull clearance at {feet:?}, height{height}"
            );
        }
    }
    Ok(())
}
