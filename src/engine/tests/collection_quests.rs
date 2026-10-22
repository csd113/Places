//! Real collection modules on tiny, asset-free metadata fixtures. These checks
//! protect distinct quest receipts and the VM/reset boundary, without baking or
//! loading a shipped map. Each scenario takes only a few dozen bounded ticks.
use std::sync::{Arc, Mutex};

use glam::Vec3;
use serde_json::{Value, json};

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, TriggerItem, WorldContext};
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};
use crate::engine::project::ProjectPath;
use crate::engine::scripting::{CampaignProgress, ScriptSource, SharedCampaign};

const DESTINATIONS: [&str; 8] = [
    "home",
    "pool",
    "office",
    "arcade",
    "lantern_hollow",
    "winter",
    "beach",
    "aero",
];

fn source(id: &str) -> &'static str {
    match id {
        "home" => include_str!("../../../projects/environments/scripts/home.luau"),
        "pool" => include_str!("../../../projects/environments/scripts/pool.luau"),
        "office" => include_str!("../../../projects/environments/scripts/office.luau"),
        "arcade" => include_str!("../../../projects/environments/scripts/arcade.luau"),
        "lantern_hollow" => {
            include_str!("../../../projects/environments/scripts/lantern_hollow.luau")
        }
        "winter" => include_str!("../../../projects/environments/scripts/winter.luau"),
        "beach" => include_str!("../../../projects/environments/scripts/beach.luau"),
        "aero" => include_str!("../../../projects/environments/scripts/aero.luau"),
        "hub" => include_str!("../../../projects/environments/scripts/hub.luau"),
        _ => "return {complete=function(api) api.complete() end}",
    }
}
fn path(id: &str) -> Result<ProjectPath, String> {
    ProjectPath::new(format!("scripts/{id}.luau"))
}
fn actor(id: &str) -> Value {
    json!({"id":id,"model":"mannequin","x":0_i32,"z":0_i32,"solid":false,
        "components":[{"component":"interactable"},{"component":"animation","clip":"pose_stand"}]})
}
struct Fixture {
    level: LevelDef,
    world: EntityWorld,
    progress: SharedCampaign,
    player: Vec3,
}
impl Fixture {
    fn new(id: &str, props: &[Value], progress: SharedCampaign) -> Result<Self, String> {
        let mut level = LevelDef::from_json(&json!({"schema":"places-level","format_version":1_i32,
            "id":id,"name":id,"spawn":{"x":0_i32,"z":0_i32},"rooms":[{"x":-40_i32,"z":-40_i32,"width":80_i32,"depth":80_i32}],
            "props":props,"doors":[{"id":"kindness_door","x":0_i32,"z":0_i32,"width":1.6_f64,"height":2.3_f64,"locked":true}],
            "sound_emitters":[{"id":"arcade_quest_cheer","x":0_i32,"z":0_i32,"sound":"prop_bell"},
                {"id":"winter_quest_bell","x":0_i32,"z":0_i32,"sound":"prop_bell"},
                {"id":"kindness_music","x":0_i32,"z":0_i32,"sound":"ambience_outdoors"}],
        }).to_string()).map_err(|problem| problem.to_string())?;
        level.scripts.push(path(id)?);
        level.script_sources.push(ScriptSource {
            path: path(id)?,
            source: source(id).into(),
        });
        let mut world = EntityWorld::from_level(&level);
        world.attach_campaign(progress.clone());
        let mut fixture = Self {
            level,
            world,
            progress,
            player: Vec3::new(8., 0., 25.),
        };
        fixture.tick();
        Ok(fixture)
    }
    fn fresh(id: &str, props: &[Value]) -> Result<Self, String> {
        Self::new(id, props, Arc::new(Mutex::new(CampaignProgress::default())))
    }
    fn tick(&mut self) {
        let index = CollisionIndex::build(&[]);
        let floor = WalkableFloor::from_level(&self.level);
        let _tick = self.world.tick(&WorldContext {
            delta_seconds: 0.1,
            feet_from: self.player,
            feet: self.player,
            eye: Vec3::new(self.player.x, self.player.y + 1.6, self.player.z),
            body_height: 1.8,
            walls: &[],
            index: &index,
            floor: &floor,
            nav: None,
        });
        let _commands = self.world.take_commands();
    }
    fn call(&mut self, handler: &str, subject: &str) -> Result<(), String> {
        let _report = self.world.dispatch_on(
            subject,
            &[ActionDef::RunScript {
                script: path(&self.level.id)?,
                handler: handler.into(),
            }],
        );
        self.clean()
    }
    fn clean(&self) -> Result<(), String> {
        if self.world.script_diagnostics().is_empty() {
            Ok(())
        } else {
            Err(format!("{:?}", self.world.script_diagnostics()))
        }
    }
    fn done(&self) -> bool {
        self.progress
            .lock()
            .is_ok_and(|progress| progress.completed(&self.level.id))
    }
    fn items(&mut self, rows: &[(&str, Vec3, f32, bool, bool)]) {
        self.world
            .observe_physical_items(rows.iter().enumerate().map(
                |(index, (id, position, up_y, held, sleeping))| TriggerItem {
                    key: u64::try_from(index).unwrap_or_default(),
                    instance: id,
                    asset: "core:rubber_duck",
                    from: *position,
                    to: *position,
                    half_extents: Vec3::splat(0.05),
                    held: *held,
                    sleeping: *sleeping,
                    up_y: *up_y,
                    path: None,
                    centre_path: None,
                },
            ));
    }
}

#[test]
fn collection_physical_receipts_reject_held_wrong_and_unsettled_items() -> Result<(), String> {
    let mut pool = Fixture::fresh(
        "pool",
        &[actor("pool_attendant"), actor("pool_physics_duck")],
    )?;
    pool.call("talk", "pool_attendant")?;
    for _ in 0_u8..45_u8 {
        pool.items(&[(
            "pool_physics_duck",
            Vec3::new(7., -0.1, 16.),
            1.,
            true,
            false,
        )]);
        pool.tick();
    }
    assert!(!pool.done(), "a held duck cannot win a flotation trial");
    for _ in 0_u8..45_u8 {
        pool.items(&[(
            "pool_physics_duck",
            Vec3::new(7., -0.1, 16.),
            1.,
            false,
            false,
        )]);
        pool.tick();
    }
    pool.clean()?;
    assert!(
        pool.done(),
        "four seconds of real upright released flotation completes Pool"
    );

    let mut office = Fixture::fresh(
        "office",
        &[
            actor("office_archivist"),
            actor("office_scrap_1"),
            actor("office_scrap_2"),
            actor("office_scrap_3"),
            actor("wrong"),
        ],
    )?;
    office.call("talk", "office_archivist")?;
    let inside = Vec3::new(6.9, 7.24, 7.);
    office.items(&[
        ("office_scrap_1", inside, 1., false, true),
        ("office_scrap_2", inside, 1., false, true),
        ("wrong", inside, 1., false, true),
    ]);
    office.tick();
    assert!(!office.done(), "another item cannot replace the third page");
    office.items(&[
        ("office_scrap_1", inside, 1., false, true),
        ("office_scrap_2", inside, 1., false, true),
        ("office_scrap_3", inside, 1., false, false),
    ]);
    office.tick();
    assert!(!office.done(), "a crossing page must settle in the crate");
    // Recovery is an actual group reset action; the fixture declares the group.
    for _ in 0_u8..2_u8 {
        office.items(&[
            ("office_scrap_1", inside, 1., false, true),
            ("office_scrap_2", inside, 1., false, true),
            ("office_scrap_3", inside, 1., false, true),
        ]);
        office.tick();
    }
    office.clean()?;
    assert!(
        office.done(),
        "all three exact settled pages complete Office"
    );
    Ok(())
}

#[test]
fn collection_bowling_requires_a_witnessed_blue_ball_launch_and_fallen_pins() -> Result<(), String>
{
    let pins = [
        "lane_1_pin_0_0",
        "lane_1_pin_1_0",
        "lane_1_pin_1_1",
        "lane_1_pin_2_0",
        "lane_1_pin_2_1",
        "lane_1_pin_2_2",
        "lane_1_pin_3_0",
        "lane_1_pin_3_1",
        "lane_1_pin_3_2",
        "lane_1_pin_3_3",
    ];
    let mut props = pins.iter().map(|id| actor(id)).collect::<Vec<_>>();
    props.extend([actor("arcade_attendant"), actor("rack_0_ball_2")]);
    props.push(json!({"id":"lane_1_secret_switch","model":"mannequin","x":0_i32,"z":0_i32,"components":[{"component":"state","name":"miniatures","value":false}]}));
    let mut arcade = Fixture::fresh("arcade", &props)?;
    let upright = pins
        .iter()
        .map(|id| (*id, Vec3::new(10.65, 0., 3.3), 1., false, true))
        .collect::<Vec<_>>();
    arcade.items(&upright);
    arcade.call("talk", "arcade_attendant")?;
    let down = pins
        .iter()
        .map(|id| (*id, Vec3::new(10.65, 0., 3.3), 0., false, true))
        .collect::<Vec<_>>();
    for _ in 0_u8..15_u8 {
        arcade.items(&down);
        arcade.tick();
    }
    assert!(
        !arcade.done(),
        "fallen pins alone cannot complete the challenge"
    );
    arcade.world.observe_held_item(Some("rack_0_ball_2".into()));
    arcade.items(&upright);
    arcade.tick();
    arcade.world.observe_held_item(None);
    let mut launched = down;
    launched.push(("rack_0_ball_2", Vec3::new(10.65, 0., 19.), 1., false, false));
    for _ in 0_u8..15_u8 {
        arcade.items(&launched);
        arcade.tick();
    }
    arcade.clean()?;
    assert!(
        arcade.done(),
        "a real released blue ball and ten fallen pins score once"
    );
    Ok(())
}

#[test]
fn collection_ice_course_requires_order_bank_recovery_and_return_to_coach() -> Result<(), String> {
    let mut winter = Fixture::fresh("winter", &[actor("winter_coach")])?;
    winter.call("talk", "winter_coach")?;
    winter.player = Vec3::new(13., -0.16, -10.);
    winter.tick();
    winter.call("talk", "winter_coach")?;
    winter.player = Vec3::new(8., -0.16, -7.);
    winter.tick();
    winter.player = Vec3::new(5., 0., -7.);
    winter.tick();
    winter.player = Vec3::new(9., -0.16, -13.);
    winter.tick();
    winter.call("talk", "winter_coach")?;
    assert!(
        !winter.done(),
        "leaving the ice discards an unfinished figure"
    );
    for (x, z) in [(8., -7.), (13., -10.), (9., -13.)] {
        winter.player = Vec3::new(x, -0.16, z);
        winter.tick();
    }
    assert!(
        !winter.done(),
        "the coach must acknowledge the completed figure"
    );
    winter.call("talk", "winter_coach")?;
    winter.clean()?;
    assert!(
        winter.done(),
        "ordered course plus the coach hand-in completes Winter"
    );
    Ok(())
}

#[test]
fn campaign_survives_fresh_vms_reset_revisits_and_gates_the_hub_in_two_orders() -> Result<(), String>
{
    for order in [
        DESTINATIONS,
        [
            "aero",
            "beach",
            "winter",
            "lantern_hollow",
            "arcade",
            "office",
            "pool",
            "home",
        ],
    ] {
        let progress = Arc::new(Mutex::new(CampaignProgress::default()));
        for (index, id) in order.into_iter().enumerate() {
            // A four-line module isolates transactional campaign publication.
            let mut fixture = Fixture::new("boundary", &[actor("caller")], progress.clone())?;
            fixture.level.id = id.into();
            let mut level = fixture.level.clone();
            level.script_sources.first_mut().ok_or("Missing boundary source")?.source="return { complete=function(api) api.complete() end, fail=function(api) api.complete(); error('rollback') end }".into();
            level.id = id.into();
            fixture.world = EntityWorld::from_level(&level);
            fixture.world.attach_campaign(progress.clone());
            fixture.level = level;
            // Use the source's declared path (boundary), independently of destination ID.
            let action = ActionDef::RunScript {
                script: path("boundary")?,
                handler: "complete".into(),
            };
            let _report = fixture
                .world
                .dispatch_on("caller", std::slice::from_ref(&action));
            fixture.world.reset_runtime();
            let _repeat = fixture.world.dispatch_on("caller", &[action]);
            assert_eq!(
                progress
                    .lock()
                    .map_err(|problem| problem.to_string())?
                    .count(),
                index + 1,
                "completion and reset are idempotent"
            );
            let mut hub = Fixture::new("hub", &[actor("kindness_host")], progress.clone())?;
            assert_eq!(
                hub.world.doors().is_locked("kindness_door"),
                Some(index != 7),
                "the real hub script opens only after all eight"
            );
            hub.clean()?;
            hub.world.reset_runtime();
            hub.tick();
            hub.clean()?;
        }
    }
    let progress = Arc::new(Mutex::new(CampaignProgress::default()));
    let mut level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"rollback","name":"Rollback","spawn":{"x":0,"z":0},"props":[{"id":"caller","model":"core:crate","x":0,"z":0}]}"#).map_err(|problem|problem.to_string())?;
    level.script_sources.push(ScriptSource {
        path: path("rollback")?,
        source: "return {fail=function(api) api.complete(); error('rollback') end}".into(),
    });
    let mut world = EntityWorld::from_level(&level);
    world.attach_campaign(progress.clone());
    let _report = world.dispatch_on(
        "caller",
        &[ActionDef::RunScript {
            script: path("rollback")?,
            handler: "fail".into(),
        }],
    );
    assert_eq!(
        progress
            .lock()
            .map_err(|problem| problem.to_string())?
            .count(),
        0,
        "a failed handler cannot publish campaign completion"
    );
    Ok(())
}

// These two boundary scenes cover the remaining scripted hand-in rules. Item
// grasp and opaque whole-assembly retirement already have independent fixtures;
// this adds only quest publication timing and the valve puzzle's exact solution.
#[test]
fn collection_home_waits_for_retirement_and_aero_requires_exact_pressure() -> Result<(), String> {
    let caretaker: Value=serde_json::from_str(r#"{"id":"home_caretaker","model":"mannequin","x":0,"z":0,"components":[{"component":"interactable"},{"component":"animation","clip":"pose_stand"},{"component":"nav_agent","radius":0.25,"height":1.8,"speed_mps":0.75},{"component":"ai","behavior":"idler"},{"component":"carry","item":"home_requested_cushion","socket":"hand.R","idle_clip":"carry_idle","walk_clip":"carry_walk"}]}"#).map_err(|problem|problem.to_string())?;
    let cushion: Value=serde_json::from_str(r#"{"id":"home_requested_cushion","model":"home:cushion","x":0,"z":0,"physics":{"motion":"dynamic","pickup":true}}"#).map_err(|problem|problem.to_string())?;
    let mut home = Fixture::fresh("home", &[caretaker, cushion])?;
    home.world.observe_held_item(Some("wrong_mug".into()));
    home.call("talk", "home_caretaker")?;
    home.tick();
    assert!(!home.done(), "a wrong hand-in cannot record Home");
    home.world
        .observe_held_item(Some("home_requested_cushion".into()));
    home.call("talk", "home_caretaker")?;
    home.world
        .confirm_handoff("home_caretaker", "home_requested_cushion");
    home.tick();
    for (x, z) in [(1.5, 4.3), (-1.5, 4.3), (-1.5, 1.5), (-8.0, 2.2)] {
        home.items(&[("home_caretaker", Vec3::new(x, 0.0, z), 1.0, false, false)]);
        home.tick();
    }
    home.tick();
    assert!(
        !home.done(),
        "arrival alone is insufficient while the carry assembly is visible"
    );
    home.world.confirm_carry_retired("home_caretaker");
    home.tick();
    assert!(
        home.done(),
        "completion follows the presentation owner's hidden-retirement confirmation"
    );

    let mut aero = Fixture::fresh(
        "aero",
        &[
            actor("aero_gardener"),
            actor("aero_valve_blue"),
            actor("aero_valve_white"),
            actor("aero_valve_lime"),
        ],
    )?;
    aero.level.effects =
        serde_json::from_str(r#"[{"id":"aero_fountain_mist","kind":"steam","enabled":false}]"#)
            .map_err(|problem| problem.to_string())?;
    aero.level.sound_emitters =
        serde_json::from_str(r#"[{"id":"aero_fountain_song","sound":"water_emitter"}]"#)
            .map_err(|problem| problem.to_string())?;
    aero.level.spawn_templates = serde_json::from_str(r#"[{"id":"fish","model":"beach:fish"}]"#)
        .map_err(|problem| problem.to_string())?;
    aero.level.spawn_points =
        serde_json::from_str(r#"[{"id":"aero_fish_point","template":"fish","x":0,"z":0}]"#)
            .map_err(|problem| problem.to_string())?;
    aero.world = EntityWorld::from_level(&aero.level);
    aero.world.attach_campaign(aero.progress.clone());
    aero.tick();
    aero.call("talk", "aero_gardener")?;
    aero.call("valve", "aero_valve_white")?;
    aero.call("talk", "aero_gardener")?;
    assert!(
        !aero.done(),
        "ambient switching and an incorrect pressure do not complete Aero"
    );
    aero.call("valve", "aero_valve_blue")?;
    aero.call("valve", "aero_valve_lime")?;
    aero.call("talk", "aero_gardener")?;
    assert!(!aero.done(), "all valves on gives seven, not five");
    aero.call("valve", "aero_valve_white")?;
    aero.call("talk", "aero_gardener")?;
    assert!(
        aero.done(),
        "one plus four, acknowledged by the gardener, completes Aero"
    );
    assert!(
        aero.world.handle_of("aero_reward_fish").is_some(),
        "the puzzle produces a real scene entity"
    );
    aero.call("talk", "aero_gardener")?;
    aero.clean()?;
    aero.world = EntityWorld::from_level(&aero.level);
    aero.world.attach_campaign(aero.progress.clone());
    aero.tick();
    aero.clean()?;
    assert!(
        aero.world.handle_of("aero_reward_fish").is_some(),
        "a completed revisit restores the fountain scene"
    );
    assert_eq!(
        aero.progress
            .lock()
            .map_err(|problem| problem.to_string())?
            .count(),
        1,
        "revisiting cannot duplicate completion"
    );
    Ok(())
}
