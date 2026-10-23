//! Explicit CPU route acceptance for the editable collection. Never invoked by
//! Cargo tests or CI and never bakes lighting or launches a native application.
use std::io::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};

use glam::Vec3;
use places::engine::collision_index::CollisionIndex;
use places::engine::door::Doors;
use places::engine::entities::{EntityWorld, WorldContext};
use places::engine::level::{ActionDef, ComponentDef};
use places::engine::nav::{NavAgentProfile, NavMesh};
use places::engine::package::collision::CompiledCollision;
use places::engine::project::{ProjectFolder, ProjectPath};
use places::engine::scripting::CampaignProgress;
use places::map_compiler::navigation::{NavBakeInput, NavBakeOptions, bake};

fn position(world: &EntityWorld, id: &str) -> Result<Vec3, String> {
    world
        .handle_of(id)
        .and_then(|handle| world.components().transforms.get(handle))
        .map(|transform| transform.position)
        .ok_or_else(|| format!("Missing actor {id}"))
}
fn tick(
    world: &mut EntityWorld,
    collision: &CompiledCollision,
    index: &CollisionIndex,
    mesh: &NavMesh,
    player: Vec3,
) {
    let _tick = world.tick(&WorldContext {
        delta_seconds: 0.05,
        feet_from: player,
        feet: player,
        eye: Vec3::new(player.x, player.y + 1.6, player.z),
        body_height: 1.8,
        body_half: places::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &collision.walls,
        index,
        floor: &collision.floor,
        nav: Some(mesh),
    });
    let _commands = world.take_commands();
}
fn walk(
    world: &mut EntityWorld,
    collision: &CompiledCollision,
    mesh: &NavMesh,
    actor: &str,
    route: (&[Vec3], bool),
    progress: &places::engine::scripting::SharedCampaign,
    scene: (&str, Vec3),
) -> Result<(), String> {
    let index = CollisionIndex::build(&collision.walls);
    let (id, spawn) = scene;
    let (goals, scripted) = route;
    if scripted {
        let handler = if id == "lantern_hollow" {
            "friend"
        } else {
            "crab"
        };
        let _started = world.dispatch_on(
            actor,
            &[ActionDef::RunScript {
                script: ProjectPath::new(format!("scripts/{id}.luau"))?,
                handler: handler.into(),
            }],
        );
        let mut complete = false;
        for _ in 0_u16..6_000_u16 {
            let at = position(world, actor)?;
            tick(
                world,
                collision,
                &index,
                mesh,
                Vec3::new(at.x + 2.0, at.y, at.z),
            );
            if progress.lock().is_ok_and(|saved| saved.completed(id)) {
                complete = true;
                break;
            }
        }
        if !complete {
            return Err(format!(
                "{id}: did not complete after 300 simulation seconds; actor={:?}; scripts={:?}; ai={}",
                position(world, actor)?,
                world.script_diagnostics(),
                world.ai().debug_report()
            ));
        }
    } else {
        for goal in goals {
            let _navigation = world.dispatch_on(
                actor,
                &[ActionDef::Navigate {
                    target: actor.into(),
                    position: goal.to_array(),
                    speed_mps: None,
                }],
            );
            let mut arrived = false;
            for _ in 0_u16..2_400_u16 {
                tick(world, collision, &index, mesh, spawn);
                if position(world, actor)?.distance(*goal) < 0.20 {
                    arrived = true;
                    break;
                }
            }
            if !arrived {
                return Err(format!(
                    "{id}: blocked toward {goal:?}; actor={:?}",
                    position(world, actor)?
                ));
            }
        }
    }
    Ok(())
}

fn run(
    project: &ProjectFolder,
    id: &str,
    actor: &str,
    goals: &[Vec3],
    scripted: bool,
) -> Result<(), String> {
    let (_, _, mut level) = project.read_level(Some(id))?;
    project.attach_scripts(&mut level)?;
    let collision = CompiledCollision::from_level(&level);
    let doors = Doors::from_level(&level);
    let prop = level
        .props
        .iter()
        .find(|prop| prop.id.as_deref() == Some(actor))
        .ok_or("Missing quest actor")?;
    let profile = prop
        .components
        .iter()
        .find_map(|component| {
            if let ComponentDef::NavAgent {
                radius,
                height,
                step_height,
                max_slope,
                speed_mps: _,
            } = component
            {
                Some(NavAgentProfile {
                    radius: *radius,
                    height: *height,
                    step_height: *step_height,
                    max_slope: *max_slope,
                    can_open_doors: true,
                })
            } else {
                None
            }
        })
        .ok_or("Missing quest body")?;
    let classes = [profile.class()];
    let input = NavBakeInput {
        level: &level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors: &doors,
        classes: &classes,
        obstacles: &[],
        walk_proxies: &[],
    };
    let (grid, report) = bake(
        &input,
        &NavBakeOptions {
            cell_m: 0.2,
            workers: 4,
        },
    )?;
    let mesh = NavMesh::from_record(grid)?;
    let index = CollisionIndex::build(&collision.walls);
    let mut world = EntityWorld::from_level(&level);
    let progress = Arc::new(Mutex::new(CampaignProgress::default()));
    world.attach_campaign(progress.clone());
    let spawn = Vec3::new(level.spawn.x, 0.0, level.spawn.z);
    tick(&mut world, &collision, &index, &mesh, spawn);
    walk(
        &mut world,
        &collision,
        &mesh,
        actor,
        (goals, scripted),
        &progress,
        (id, spawn),
    )?;
    if !world.script_diagnostics().is_empty() {
        return Err(format!("{id}: {:?}", world.script_diagnostics()));
    }
    writeln!(
        std::io::stdout().lock(),
        "{id}: collision/door-aware walking passed; final {:?}; {} navigable cells; completion={}",
        position(&world, actor)?,
        report.walkable_cells.first().copied().unwrap_or_default(),
        progress.lock().is_ok_and(|saved| saved.completed(id))
    )
    .map_err(|problem| problem.to_string())?;
    Ok(())
}
fn main() -> Result<(), String> {
    let project = ProjectFolder::open(Path::new("projects/environments"))?;
    run(
        &project,
        "home",
        "home_caretaker",
        &[
            Vec3::new(1.5, 0.0, 4.3),
            Vec3::new(-1.5, 0.0, 4.3),
            Vec3::new(-1.5, 0.0, 1.5),
            Vec3::new(-8.0, 0.0, 2.2),
        ],
        false,
    )?;
    run(
        &project,
        "lantern_hollow",
        "hollow_missing_friend",
        &[],
        true,
    )?;
    run(&project, "beach", "beach_quest_crab", &[], true)
}
