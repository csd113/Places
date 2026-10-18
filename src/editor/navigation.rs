//! Explicit inspection of the current document using the production nav bake.

use std::sync::mpsc;

use glam::Vec3;

use super::App;
use super::ui::Action;
use crate::engine::door::Doors;
use crate::engine::level::{ComponentDef, LevelDef, LevelSurfaces};
use crate::engine::nav::{NavMesh, NavScratch, Path, PathQuery, PathResult};

const MAX_COVERAGE_CELLS: usize = 12_000;
const QUERY_BUDGET: usize = 65_536;

pub(super) struct Coverage {
    pub position: Vec3,
    pub half: f32,
    pub color: [f32; 4],
}

pub(super) struct Agent {
    pub label: String,
    pub class: usize,
    pub position: Option<Vec3>,
    pub can_open_doors: bool,
}

struct Prepared {
    mesh: NavMesh,
    doors: Doors,
    agents: Vec<Agent>,
    warnings: Vec<String>,
}

struct Job {
    revision: u64,
    receiver: mpsc::Receiver<Result<Prepared, String>>,
}

#[derive(Default)]
pub(super) struct Inspection {
    pub visible: bool,
    pub agent: usize,
    pub start: Option<Vec3>,
    pub goal: Option<Vec3>,
    start_node: Option<usize>,
    goal_node: Option<usize>,
    pub pick_goal: Option<bool>,
    pub coverage: bool,
    pub all_floors: bool,
    pub slice_y: f32,
    pub shown_cells: usize,
    pub total_cells: usize,
    pub feedback: String,
    pub route: Path,
    revision: u64,
    prepared: Option<Prepared>,
    job: Option<Job>,
    scratch: NavScratch,
}

impl Inspection {
    pub const fn is_baking(&self) -> bool {
        self.job.is_some()
    }

    pub const fn is_current(&self) -> bool {
        self.prepared.is_some()
    }

    pub fn selected_agent(&self) -> Option<&Agent> {
        self.prepared.as_ref()?.agents.get(self.agent)
    }

    pub fn agent_count(&self) -> usize {
        self.prepared.as_ref().map_or(0, |data| data.agents.len())
    }

    pub fn body_label(&self) -> String {
        self.selected_agent()
            .and_then(|agent| self.prepared.as_ref()?.mesh.grid().classes.get(agent.class))
            .map_or_else(String::new, |body| {
                format!(
                    "r {:.2} / h {:.2} / step {:.2}",
                    body.radius, body.height, body.step_height
                )
            })
    }

    pub fn movement_label(&self) -> String {
        self.selected_agent()
            .and_then(|agent| {
                Some((
                    agent,
                    self.prepared
                        .as_ref()?
                        .mesh
                        .grid()
                        .classes
                        .get(agent.class)?,
                ))
            })
            .map_or_else(String::new, |(agent, body)| {
                format!(
                    "Slope {:.1} / doors: {}",
                    body.max_slope,
                    if agent.can_open_doors { "yes" } else { "no" }
                )
            })
    }

    pub fn checkpoint(&self) -> serde_json::Value {
        serde_json::json!({
            "visible": self.visible, "current": self.is_current(), "baking": self.is_baking(),
            "agent": self.selected_agent().map(|agent| &agent.label),
            "class": self.selected_agent().map(|agent| agent.class),
            "body": self.body_label(), "can_open_doors": self.selected_agent().map(|agent| agent.can_open_doors),
            "start": self.start.map(|point| point.to_array()), "goal": self.goal.map(|point| point.to_array()),
            "start_node": self.start_node, "goal_node": self.goal_node,
            "feedback": self.feedback, "complete": self.route.complete, "expanded": self.route.expanded,
            "waypoints": self.route.waypoints.iter().map(Vec3::to_array).collect::<Vec<_>>(),
            "door_requests": self.route.door_requests, "coverage": self.coverage,
            "slice_y": self.slice_y, "all_floors": self.all_floors,
            "shown_cells": self.shown_cells, "total_cells": self.total_cells,
            "warnings": self.prepared.as_ref().map(|data| &data.warnings),
        })
    }
}

impl App {
    pub(super) fn invalidate_navigation(&mut self) {
        let nav = &mut self.navigation;
        nav.revision = nav.revision.wrapping_add(1);
        nav.prepared = None;
        nav.start = None;
        nav.goal = None;
        nav.start_node = None;
        nav.goal_node = None;
        nav.pick_goal = None;
        nav.route.clear();
        nav.shown_cells = 0;
        nav.total_cells = 0;
        nav.feedback = "Source changed. Bake current to inspect.".into();
        self.viewport.navigation_guides(Vec::new(), &[], None, None);
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only navigation controls belong to this contextual handler."
    )]
    pub(super) fn navigation_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Navigation => {
                self.navigation.visible = !self.navigation.visible;
                self.navigation.pick_goal = None;
            }
            Action::NavBake => self.start_navigation_bake()?,
            Action::NavAgent(forward) => {
                let count = self.navigation.agent_count();
                if count == 0 {
                    return Err("Bake current navigation first".into());
                }
                self.navigation.agent = if forward {
                    self.navigation
                        .agent
                        .saturating_add(1)
                        .checked_rem(count)
                        .unwrap_or(0)
                } else {
                    self.navigation
                        .agent
                        .checked_sub(1)
                        .unwrap_or_else(|| count.saturating_sub(1))
                };
                self.query_navigation();
            }
            Action::NavPick(goal) => {
                if !self.navigation.is_current() {
                    return Err("Bake current navigation first".into());
                }
                self.navigation.pick_goal = Some(goal);
                self.navigation.feedback = format!(
                    "Click a {} floor cell.",
                    if goal { "goal" } else { "start" }
                );
            }
            Action::NavUseAgent => {
                let position = self
                    .navigation
                    .selected_agent()
                    .and_then(|agent| agent.position)
                    .ok_or("Choose an authored agent instance first")?;
                self.navigation.start = Some(position);
                self.navigation.start_node = None;
                self.navigation.slice_y = position.y;
                self.query_navigation();
            }
            Action::NavCoverage => self.navigation.coverage = !self.navigation.coverage,
            Action::NavAllFloors => self.navigation.all_floors = !self.navigation.all_floors,
            Action::NavSlice(up) => {
                self.navigation.slice_y += if up { 0.5 } else { -0.5 };
                self.navigation.all_floors = false;
            }
            Action::NavClear => {
                self.navigation.start = None;
                self.navigation.goal = None;
                self.navigation.start_node = None;
                self.navigation.goal_node = None;
                self.navigation.pick_goal = None;
                self.navigation.route.clear();
                self.navigation.feedback = "Pick start and goal floor cells.".into();
            }
            _ => return Ok(false),
        }
        self.refresh_navigation_guides();
        Ok(true)
    }

    fn start_navigation_bake(&mut self) -> Result<(), String> {
        if self.navigation.is_baking() {
            return Err("Wait for the current navigation bake".into());
        }
        let mut level = self
            .document
            .as_ref()
            .ok_or("Open a project first")?
            .level()
            .clone();
        let catalog = self.catalog.clone();
        // A second explicit bake starts a fresh inspection. Keeping its former
        // class's route while selecting the new default class would mislead.
        self.invalidate_navigation();
        let (sender, receiver) = mpsc::channel();
        let _thread = std::thread::Builder::new()
            .name("places-editor-navigation".into())
            .spawn(move || {
                crate::map_compiler::prepare_level(&mut level, &catalog, None);
                let result = prepare(&level);
                let _sent = sender.send(result);
            })
            .map_err(|error| format!("Cannot start navigation bake: {error}"))?;
        self.navigation.job = Some(Job {
            revision: self.navigation.revision,
            receiver,
        });
        self.navigation.feedback = "Baking current source; no lighting build.".into();
        Ok(())
    }

    pub(super) fn poll_navigation(&mut self) {
        let Some(job) = &self.navigation.job else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Navigation bake worker stopped".into()),
        };
        let current = job.revision == self.navigation.revision;
        self.navigation.job = None;
        if !current {
            return;
        }
        match result {
            Ok(data) => {
                self.navigation.agent = 0;
                self.navigation.coverage = true;
                self.navigation.slice_y = data
                    .mesh
                    .grid()
                    .cell_y
                    .iter()
                    .copied()
                    .filter(|value| value.is_finite())
                    .min_by(f32::total_cmp)
                    .unwrap_or(0.0);
                self.navigation.prepared = Some(data);
                self.navigation.feedback = "Current source. Pick start and goal.".into();
            }
            Err(error) => self.navigation.feedback = format!("Bake failed: {error}"),
        }
        self.refresh_navigation_guides();
    }

    pub(super) fn navigation_click(&mut self, point: [f32; 2]) -> bool {
        if !self.navigation.visible {
            return false;
        }
        let Some(goal) = self.navigation.pick_goal else {
            return false;
        };
        let Some((origin, direction)) = self.viewport.ray(point) else {
            return true;
        };
        let Some((node, position)) = self.navigation_floor(origin, direction) else {
            self.navigation.feedback = "No baked floor here. Check the height slice.".into();
            return true;
        };
        if goal {
            self.navigation.goal = Some(position);
            self.navigation.goal_node = Some(node);
        } else {
            self.navigation.start = Some(position);
            self.navigation.start_node = Some(node);
        }
        self.navigation.pick_goal = None;
        self.query_navigation();
        self.refresh_navigation_guides();
        true
    }

    fn navigation_floor(&self, origin: Vec3, direction: Vec3) -> Option<(usize, Vec3)> {
        if direction.y.abs() < 0.000_01 {
            return None;
        }
        let grid = self.navigation.prepared.as_ref()?.mesh.grid();
        (0..grid.cell_count())
            .filter_map(|index| {
                let y = grid.surface(index)?;
                if !self.navigation.all_floors && (y - self.navigation.slice_y).abs() > 0.75 {
                    return None;
                }
                let distance = (y - origin.y) / direction.y;
                if distance < 0.0 {
                    return None;
                }
                let position = Vec3::new(
                    direction.x.mul_add(distance, origin.x),
                    direction.y.mul_add(distance, origin.y),
                    direction.z.mul_add(distance, origin.z),
                );
                let (x, z) = grid.coordinates(index)?;
                let (cx, cz) = grid.cell_center(x, z);
                let half = grid.cell_m * 0.5;
                ((position.x - cx).abs() <= half && (position.z - cz).abs() <= half).then_some((
                    distance,
                    index,
                    Vec3::new(cx, y, cz),
                ))
            })
            .min_by(|first, second| first.0.total_cmp(&second.0))
            .map(|(_, node, position)| (node, position))
    }

    fn query_navigation(&mut self) {
        let nav = &mut self.navigation;
        nav.route.clear();
        let (Some(start), Some(goal), Some(data)) = (nav.start, nav.goal, &nav.prepared) else {
            nav.feedback = "Pick start and goal floor cells.".into();
            return;
        };
        let Some(agent) = data.agents.get(nav.agent) else {
            return;
        };
        let query = PathQuery {
            class: agent.class,
            start,
            goal,
            can_open_doors: agent.can_open_doors,
            max_expansions: QUERY_BUDGET,
            doors: &data.doors,
        };
        let grid = data.mesh.grid();
        for (name, position, node) in [
            ("Start", start, nav.start_node),
            ("Goal", goal, nav.goal_node),
        ] {
            if node.is_some_and(|index| {
                !data
                    .mesh
                    .cell_passable(agent.class, index, &data.doors, agent.can_open_doors)
            }) || data
                .mesh
                .nearest(
                    agent.class,
                    position,
                    grid.cell_m,
                    0.05,
                    &data.doors,
                    agent.can_open_doors,
                )
                .is_none()
            {
                nav.feedback = format!("Invalid {name}: body clearance or door blocks this floor.");
                return;
            }
        }
        nav.feedback = match data
            .mesh
            .path_into(&query, &mut nav.scratch, &mut nav.route)
        {
            Ok(()) if nav.route.complete => format!(
                "Reachable / {} waypoints / {} expanded",
                nav.route.waypoints.len(),
                nav.route.expanded
            ),
            Ok(()) if nav.route.expanded < QUERY_BUDGET => {
                nav.route.waypoints.clear();
                nav.route.door_requests.clear();
                format!(
                    "Unreachable: no route to goal / {} expanded",
                    nav.route.expanded
                )
            }
            Ok(()) => format!(
                "Incomplete / budget {QUERY_BUDGET} / {} expanded",
                nav.route.expanded
            ),
            Err(PathResult::Invalid(message)) => format!("Invalid route: {message}"),
            Err(PathResult::Unreachable) => {
                "Unreachable: disconnected floors or blocked route.".into()
            }
            Err(PathResult::Path(_)) => "Incomplete route.".into(),
        };
    }

    fn refresh_navigation_guides(&mut self) {
        let nav = &mut self.navigation;
        let mut coverage = Vec::new();
        nav.total_cells = 0;
        if nav.visible
            && nav.coverage
            && let Some(data) = &nav.prepared
            && let Some(agent) = data.agents.get(nav.agent)
        {
            let grid = data.mesh.grid();
            for index in 0..grid.cell_count() {
                let Some(y) = grid.surface(index) else {
                    continue;
                };
                if !nav.all_floors && (y - nav.slice_y).abs() > 0.75 {
                    continue;
                }
                nav.total_cells = nav.total_cells.saturating_add(1);
                if coverage.len() >= MAX_COVERAGE_CELLS {
                    continue;
                }
                let Some((column_x, column_z)) = grid.coordinates(index) else {
                    continue;
                };
                let (x, z) = grid.cell_center(column_x, column_z);
                let color = if data.mesh.cell_passable(
                    agent.class,
                    index,
                    &data.doors,
                    agent.can_open_doors,
                ) {
                    region_color(grid.region_of(agent.class, index))
                } else {
                    [1.0, 0.18, 0.24, 0.16]
                };
                coverage.push(Coverage {
                    position: Vec3::new(x, y + 0.035, z),
                    half: grid.cell_m * 0.47,
                    color,
                });
            }
        }
        nav.shown_cells = coverage.len();
        let route = if nav.visible {
            nav.route.waypoints.as_slice()
        } else {
            &[]
        };
        self.viewport.navigation_guides(
            coverage,
            route,
            nav.visible.then_some(nav.start).flatten(),
            nav.visible.then_some(nav.goal).flatten(),
        );
    }
}

fn region_color(region: Option<u16>) -> [f32; 4] {
    match region.unwrap_or(0) % 4 {
        0 => [0.2, 0.9, 0.65, 0.24],
        1 => [0.3, 0.65, 1.0, 0.24],
        2 => [0.85, 0.7, 0.2, 0.24],
        _ => [0.7, 0.4, 1.0, 0.24],
    }
}

fn prepare(level: &LevelDef) -> Result<Prepared, String> {
    let mut warnings = Vec::new();
    let (bytes, _report) = crate::map_compiler::bake_navigation(level, 1, &mut warnings)?;
    let grid = crate::engine::package::navigation::read_navigation(&bytes)?;
    let mesh = NavMesh::from_record(grid)?;
    let mut agents: Vec<_> = mesh
        .grid()
        .classes
        .iter()
        .enumerate()
        .map(|(class, _)| Agent {
            label: if class == 0 {
                "Reference human".into()
            } else {
                format!("Baked body {}", class.saturating_add(1))
            },
            class,
            position: None,
            can_open_doors: false,
        })
        .collect();
    let surfaces = LevelSurfaces::new(level);
    for prop in &level.props {
        let position = Vec3::new(
            prop.x,
            surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
            prop.z,
        );
        if let Some(agent) = authored_agent(
            prop.id.clone().unwrap_or_else(|| prop.model.clone()),
            position,
            &prop.components,
            &mesh,
        ) {
            agents.push(agent);
        }
    }
    for point in &level.spawn_points {
        let Some(template) = level
            .spawn_templates
            .iter()
            .find(|template| template.id == point.template)
        else {
            continue;
        };
        let position = Vec3::new(
            point.x,
            point
                .y
                .unwrap_or_else(|| surfaces.floor_y_at(point.x, point.z).unwrap_or(0.0)),
            point.z,
        );
        if let Some(agent) = authored_agent(
            format!("Spawn {}", point.id),
            position,
            &template.components,
            &mesh,
        ) {
            agents.push(agent);
        }
    }
    Ok(Prepared {
        mesh,
        doors: Doors::from_level(level),
        agents,
        warnings,
    })
}

fn authored_agent(
    label: String,
    position: Vec3,
    components: &[ComponentDef],
    mesh: &NavMesh,
) -> Option<Agent> {
    let ai = components.iter().find_map(|component| {
        if let ComponentDef::Ai(def) = component {
            Some(def)
        } else {
            None
        }
    });
    let authored_body = components
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
                crate::engine::package::navigation::NavClass::new(
                    *radius,
                    *height,
                    *step_height,
                    *max_slope,
                )
            } else {
                None
            }
        })
        .or_else(|| ai.map(|_| crate::engine::nav::reference_class()));
    let body = authored_body?;
    let class = mesh.class_index(&body)?;
    Some(Agent {
        label,
        class,
        position: Some(position),
        can_open_doors: ai.is_some_and(|def| def.can_open_doors),
    })
}
