//! Bounded shared task interpreter. Navigation, physics, rendering and action
//! dispatch retain their existing owners; this module produces typed intent.
use super::{
    AiAgent, AiOutcome, AiState, AiTickContext,
    authoring::{ItemSelector, TaskDef, TaskRequest, UsePoint},
    controller::OperationToken,
};
use crate::engine::entities::id::EntityHandle;
use glam::Vec3;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct UseAck {
    pub token: OperationToken,
    pub target: String,
    pub completed_at: f32,
}
pub const MAX_TASKS: usize = 8;
pub const MAX_OBJECTS: usize = 512;
pub const MAX_OBJECT_REFERENCES: usize = 512;

/// Immutable observation copied at the existing perception cadence.
#[derive(Debug, Clone)]
pub struct AiObject {
    pub handle: EntityHandle,
    pub id: String,
    pub object_type: String,
    pub position: Vec3,
    pub loose: bool,
    pub radius: f32,
    pub carried_by: Option<EntityHandle>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskEffect {
    Pickup {
        token: OperationToken,
        item: String,
    },
    Putdown {
        token: OperationToken,
        contact: [f32; 3],
    },
    Use {
        token: OperationToken,
        point: UsePoint,
    },
    Animate {
        token: OperationToken,
        clip: String,
    },
}

#[derive(Debug)]
pub struct TaskRun {
    pub token: OperationToken,
    pub request: TaskRequest,
    pub progress: Progress,
    pub paused: bool,
    pub references: Vec<String>,
}
#[derive(Debug, Default)]
pub struct Progress {
    pub phase: u8,
    pub elapsed: f32,
    pub stage_seconds: f32,
    pub index: usize,
    pub item: Option<EntityHandle>,
    pub last_known: Option<Vec3>,
    pub lost_seconds: f32,
    pub next_observation: f32,
    pub scan_cursor: usize,
    pub contact_requested_at: f32,
    pub restore_pose: bool,
    pub attempts: u8,
    pub observed_radius: f32,
    pub child: Option<Box<Self>>,
    pub status: &'static str,
}
#[must_use]
pub fn pending_contact_allowed(def: &TaskDef, progress: &Progress) -> bool {
    match def {
        TaskDef::Fetch {
            item: _,
            destination: _,
            search_radius: _,
        } => progress.phase >= 3,
        TaskDef::Routine { steps, looped: _ } => steps
            .get(progress.index)
            .zip(progress.child.as_ref())
            .is_some_and(|(definition, child)| pending_contact_allowed(definition, child)),
        TaskDef::Move { point: _, speed: _ }
        | TaskDef::Wait { seconds: _ }
        | TaskDef::Investigate {
            point: _,
            seconds: _,
            radius: _,
        }
        | TaskDef::Use { point: _ }
        | TaskDef::Follow {
            target: _,
            distance: _,
            memory_seconds: _,
        }
        | TaskDef::Patrol {
            points: _,
            speed: _,
            memory_seconds: _,
            track_player: _,
            decision_seconds: _,
        } => true,
    }
}
pub fn recover(def: &TaskDef, progress: &mut Progress) {
    match def {
        TaskDef::Fetch {
            item: _,
            destination: _,
            search_radius: _,
        } => *progress = Progress::default(),
        TaskDef::Routine { steps, looped: _ } => {
            if let Some((definition, child)) =
                steps.get(progress.index).zip(progress.child.as_mut())
            {
                recover(definition, child);
            }
        }
        TaskDef::Use { point: _ } => progress.restore_pose = true,
        TaskDef::Move { point: _, speed: _ }
        | TaskDef::Wait { seconds: _ }
        | TaskDef::Investigate {
            point: _,
            seconds: _,
            radius: _,
        }
        | TaskDef::Follow {
            target: _,
            distance: _,
            memory_seconds: _,
        }
        | TaskDef::Patrol {
            points: _,
            speed: _,
            memory_seconds: _,
            track_player: _,
            decision_seconds: _,
        } => {}
    }
}
impl Progress {
    #[must_use]
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"status":self.status,"attempts":self.attempts,"phase":self.phase,"elapsed":self.elapsed,"stage_seconds":self.stage_seconds,"index":self.index,"item":self.item.map(|handle|[handle.index(),handle.generation()]),"last_known":self.last_known.map(|point|point.to_array()),"lost_seconds":self.lost_seconds,"child":self.child.as_ref().map(|child|child.snapshot())})
    }
}
impl TaskRun {
    #[must_use]
    pub fn new(token: OperationToken, request: TaskRequest) -> Self {
        let mut references = Vec::new();
        request.definition.references_into(&mut references);
        references.sort();
        references.dedup();
        Self {
            references,
            token,
            request,
            progress: Progress::default(),
            paused: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Running,
    Complete,
    Failed(&'static str),
}

fn goal(agent: &mut AiAgent, point: Vec3, speed: Option<f32>) {
    if agent
        .navigation_goal
        .is_none_or(|old| old.distance(point) > 0.25)
    {
        agent.path.clear();
        agent.path_goal = None;
        agent.path_done_goal = None;
        agent.path_failed_at = None;
        agent.stuck_seconds = 0.0;
    }
    if agent.task_clip.is_some() {
        agent.pose_dirty.mark();
    }
    agent.task_clip = None;
    agent.navigation_goal = Some(point);
    agent.navigation_speed = speed;
    agent.state = AiState::Investigate { point };
}
fn arrived(agent: &AiAgent, point: Vec3, radius: f32) -> bool {
    agent.position.distance(point) <= radius
}
fn stop(agent: &mut AiAgent) {
    agent.navigation_goal = None;
    agent.navigation_speed = None;
    agent.state = AiState::Idle;
    agent.path.clear();
    agent.path_goal = None;
    agent.speed_mps = 0.0;
}

/// Runs only the selected task; suspended clocks never advance. Cancellation
/// removes the token before the application's physical command boundary.
pub fn tick(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    reservations: &mut BTreeMap<EntityHandle, OperationToken>,
    out: &mut AiOutcome,
) -> bool {
    let mut controller = std::mem::take(&mut agent.controller);
    let selected = controller
        .runs
        .iter()
        .enumerate()
        .filter(|(_, run)| !run.paused)
        .max_by_key(|(_, run)| (run.request.priority, std::cmp::Reverse(run.token.serial)))
        .map(|(index, _)| index);
    let Some(index) = selected else {
        if !controller.runs.is_empty() {
            stop(agent);
            if agent.task_clip.is_some() {
                agent.pose_dirty.mark();
            }
            agent.task_clip = None;
            controller.active = None;
            agent.controller = controller;
            return true;
        }
        controller.resume_baseline(agent);
        agent.controller = controller;
        return false;
    };
    let Some(selected_token) = controller.runs.get(index).map(|candidate| candidate.token) else {
        agent.controller = controller;
        return false;
    };
    if controller.active != Some(selected_token) {
        if let Some(previous) = controller.active {
            reservations.retain(|_, owner| *owner != previous);
            if let Some(old) = controller.runs.iter_mut().find(|run| run.token == previous) {
                // Fetch must re-observe/re-reserve after an ownership interruption.
                recover(&old.request.definition, &mut old.progress);
            }
        }
        controller.suspend_baseline(agent);
        stop(agent);
        agent.task_clip = None;
        agent.pose_dirty.mark();
    }
    let Some(run) = controller.runs.get_mut(index) else {
        agent.controller = controller;
        return false;
    };
    controller.active = Some(run.token);
    let result = step(
        &run.request.definition,
        &mut run.progress,
        run.token,
        agent,
        ctx,
        reservations,
        out,
    );
    if result != Step::Running {
        let token = run.token;
        controller.last_result = Some(format!(
            "{}:{}",
            run.request.key,
            match result {
                Step::Complete => "complete",
                Step::Failed(reason) => reason,
                Step::Running => "running",
            }
        ));
        reservations.retain(|_, owner| *owner != token);
        drop(controller.runs.remove(index));
        controller.active = None;
        agent.task_clip = None;
        agent.pose_dirty.mark();
        stop(agent);
        if controller.runs.is_empty() {
            controller.resume_baseline(agent);
        }
    }
    agent.controller = controller;
    true
}

/// Deterministic expanding search samples only at the perception cadence.
/// Route planning and physical obstruction remain owned by the shared mover.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite bounded search-ring vectors"
)]
fn search_item(agent: &mut AiAgent, p: &mut Progress, radius: f32) {
    let reached = agent
        .navigation_goal
        .is_none_or(|point| arrived(agent, point, 0.35));
    if agent.navigation_goal.is_some()
        && !(p.stage_seconds >= 3.0 && (reached || agent.path_failed_at.is_some()))
        && p.stage_seconds < 12.0
    {
        return;
    }
    let offset = match p.index % 8 {
        0 => Vec3::X,
        1 => Vec3::new(1.0, 0.0, 1.0),
        2 => Vec3::Z,
        3 => Vec3::new(-1.0, 0.0, 1.0),
        4 => Vec3::NEG_X,
        5 => Vec3::new(-1.0, 0.0, -1.0),
        6 => Vec3::NEG_Z,
        _ => Vec3::new(1.0, 0.0, -1.0),
    }
    .normalize();
    let ring = u16::try_from(p.index.checked_div(8).unwrap_or(0))
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let distance = (3.0 * f32::from(ring)).min(radius);
    goal(
        agent,
        p.last_known.unwrap_or(agent.home) + offset * distance,
        None,
    );
    p.index = p.index.saturating_add(1);
    p.stage_seconds = 0.0;
}

#[expect(
    clippy::too_many_lines,
    reason = "one bounded task transition with explicit immutable world input and side-effect output"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite bounded world-space vector arithmetic; integer indices use checked/saturating operations"
)]
fn step(
    def: &TaskDef,
    p: &mut Progress,
    token: OperationToken,
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    reservations: &mut BTreeMap<EntityHandle, OperationToken>,
    out: &mut AiOutcome,
) -> Step {
    p.elapsed += ctx.delta;
    p.stage_seconds += ctx.delta;
    if p.elapsed > 120.0
        && !matches!(
            def,
            TaskDef::Routine {
                steps: _,
                looped: _
            } | TaskDef::Follow {
                target: _,
                distance: _,
                memory_seconds: _
            } | TaskDef::Patrol {
                points: _,
                speed: _,
                memory_seconds: _,
                track_player: _,
                decision_seconds: _
            }
        )
    {
        return Step::Failed("timeout");
    }
    match def {
        TaskDef::Move { point, speed } => {
            p.status = "moving";
            let target = Vec3::from_array(*point);
            if arrived(agent, target, 0.15) {
                stop(agent);
                Step::Complete
            } else {
                goal(agent, target, *speed);
                Step::Running
            }
        }
        TaskDef::Wait { seconds } => {
            p.status = "waiting";
            stop(agent);
            if p.elapsed >= *seconds {
                Step::Complete
            } else {
                Step::Running
            }
        }
        TaskDef::Investigate {
            point,
            seconds,
            radius,
        } => {
            p.status = "searching_last_known";
            if p.elapsed >= *seconds {
                stop(agent);
                return Step::Complete;
            }
            let center = Vec3::from_array(*point);
            let offset = match p.index % 5 {
                0 => Vec3::ZERO,
                1 => Vec3::X,
                2 => Vec3::Z,
                3 => Vec3::NEG_X,
                _ => Vec3::NEG_Z,
            };
            let target = center + offset * *radius;
            if arrived(agent, target, 0.35) || p.stage_seconds >= 2.0 {
                p.index = p.index.saturating_add(1);
                p.stage_seconds = 0.0;
            }
            goal(agent, target, None);
            Step::Running
        }
        TaskDef::Use { point } => use_point(point, p, token, agent, ctx, out, None),
        TaskDef::Fetch {
            item,
            destination,
            search_radius,
        } => {
            p.status = match p.phase {
                0 => "searching_item",
                1 => "approaching_item",
                2 => "awaiting_physical_pickup",
                3 => "delivering",
                _ => "awaiting_physical_release",
            };
            if p.phase == 0 {
                if ctx.sim_time < p.next_observation {
                    return Step::Running;
                }
                p.next_observation = ctx.sim_time + super::PERCEPTION_INTERVAL_S;
                let first = p.scan_cursor.checked_rem(ctx.objects.len()).unwrap_or(0);
                p.scan_cursor = first.saturating_add(8);
                let candidate = ctx
                    .objects
                    .iter()
                    .cycle()
                    .skip(first)
                    .take(ctx.objects.len().min(8))
                    .filter(|object| {
                        object.loose
                            && object.carried_by.is_none()
                            && object.position.distance(agent.position) <= *search_radius
                            && super::senses::visible_object(
                                agent,
                                ctx,
                                object.position,
                                agent.def.sight_range.max(0.1).min(*search_radius),
                                agent.def.sight_fov_degrees,
                            )
                            && reservations
                                .get(&object.handle)
                                .is_none_or(|owner| *owner == token)
                            && match item {
                                ItemSelector::Id(id) => *id == object.id,
                                ItemSelector::ObjectType(kind) => *kind == object.object_type,
                            }
                    })
                    .min_by(|a, b| {
                        a.position
                            .distance_squared(agent.position)
                            .total_cmp(&b.position.distance_squared(agent.position))
                    });
                if let Some(object) = candidate {
                    let _old = reservations.insert(object.handle, token);
                    p.item = Some(object.handle);
                    p.last_known = Some(object.position);
                    p.observed_radius = object.radius;
                    p.lost_seconds = 0.0;
                    p.phase = 1;
                    p.stage_seconds = 0.0;
                } else {
                    search_item(agent, p, *search_radius);
                    return Step::Running;
                }
            }
            let Some(object) = p
                .item
                .and_then(|handle| ctx.objects.iter().find(|object| object.handle == handle))
            else {
                reservations.retain(|_, owner| *owner != token);
                p.phase = 0;
                p.item = None;
                stop(agent);
                return Step::Running;
            };
            if p.phase < 3 && object.carried_by == Some(agent.handle) {
                p.phase = 3;
                p.stage_seconds = 0.0;
            }
            if p.phase < 3 && !object.loose {
                reservations.retain(|_, owner| *owner != token);
                p.phase = 0;
                p.item = None;
                stop(agent);
                return Step::Running;
            }
            if p.phase == 1 {
                p.lost_seconds += ctx.delta;
                if ctx.sim_time >= p.next_observation {
                    p.next_observation = ctx.sim_time + super::PERCEPTION_INTERVAL_S;
                    if super::senses::visible_object(
                        agent,
                        ctx,
                        object.position,
                        agent.def.sight_range.max(0.1).min(*search_radius),
                        agent.def.sight_fov_degrees,
                    ) {
                        p.last_known = Some(object.position);
                        p.observed_radius = object.radius;
                        p.lost_seconds = 0.0;
                    }
                }
                let observed = p.last_known.unwrap_or(agent.position);
                if p.lost_seconds > 4.0 || (p.lost_seconds > 0.4 && arrived(agent, observed, 0.6)) {
                    reservations.retain(|_, owner| *owner != token);
                    p.phase = 0;
                    p.item = None;
                    p.stage_seconds = 0.0;
                    stop(agent);
                    return Step::Running;
                }
                // Approach the item's observed surface without routing the actor through its physical box.
                let flat = Vec3::new(
                    agent.position.x - observed.x,
                    0.0,
                    agent.position.z - observed.z,
                )
                .normalize_or_zero();
                let approach_distance = (agent.profile.radius + p.observed_radius + 0.01).max(0.35);
                let candidate =
                    Vec3::new(observed.x, observed.y, observed.z) + flat * approach_distance;
                let target = ctx
                    .nav
                    .and_then(|nav| {
                        agent.class.and_then(|class| {
                            nav.nearest(
                                class,
                                candidate,
                                1.0,
                                2.0,
                                ctx,
                                agent.profile.can_open_doors,
                            )
                        })
                    })
                    .map_or_else(
                        || Vec3::new(candidate.x, agent.position.y, candidate.z),
                        |point| Vec3::new(candidate.x, point.position.y, candidate.z),
                    );
                if arrived(agent, target, 0.06)
                    && p.lost_seconds <= super::PERCEPTION_INTERVAL_S * 2.0
                    && super::perception::sight_clear_with_leaves(
                        ctx.index,
                        ctx.walls,
                        ctx.leaves,
                        agent.position + Vec3::Y * 0.8,
                        observed + Vec3::Y * 0.05,
                    )
                {
                    stop(agent);
                    let toward = Vec3::new(
                        observed.x - agent.position.x,
                        0.0,
                        observed.z - agent.position.z,
                    );
                    agent.yaw_degrees = toward.x.atan2(toward.z).to_degrees();
                    out.moved.push((
                        agent.handle,
                        agent.position,
                        agent.yaw_degrees,
                        agent.spawned,
                    ));
                    out.task_effects.push(TaskEffect::Pickup {
                        token,
                        item: object.id.clone(),
                    });
                    p.phase = 2;
                    p.stage_seconds = 0.0;
                } else {
                    goal(agent, target, None);
                }
            } else if p.phase == 2 {
                stop(agent);
                if p.stage_seconds > 5.0 {
                    reservations.retain(|_, owner| *owner != token);
                    p.attempts = p.attempts.saturating_add(1);
                    if p.attempts >= 3 {
                        return Step::Failed("pickup_contact_failed");
                    }
                    p.phase = 0;
                    p.item = None;
                    p.stage_seconds = 0.0;
                    agent.task_clip = None;
                    agent.pose_dirty.mark();
                }
            } else if p.phase == 3 || p.phase == 4 {
                let child = p.child.get_or_insert_with(|| Box::new(Progress::default()));
                child.elapsed += ctx.delta;
                child.stage_seconds += ctx.delta;
                if p.phase == 3
                    && child.phase == 0
                    && arrived(agent, Vec3::from_array(destination.approach), 0.06)
                {
                    out.task_effects.push(TaskEffect::Putdown {
                        token,
                        contact: destination.contact,
                    });
                    p.phase = 4;
                }
                if p.phase == 3 && object.carried_by != Some(agent.handle) {
                    reservations.retain(|_, owner| *owner != token);
                    return Step::Failed("cargo_lost");
                }
                match use_point(
                    destination,
                    child,
                    token,
                    agent,
                    ctx,
                    out,
                    Some(object.position),
                ) {
                    Step::Complete => {
                        if object.carried_by.is_none() && object.loose {
                            return Step::Complete;
                        }
                        if child.stage_seconds > 5.0 {
                            return Step::Failed("putdown_contact_failed");
                        }
                    }
                    Step::Failed(reason) => return Step::Failed(reason),
                    Step::Running => {}
                }
            }
            Step::Running
        }
        TaskDef::Follow {
            target,
            distance,
            memory_seconds,
        } => {
            p.status = "following";
            p.lost_seconds += ctx.delta;
            if ctx.sim_time >= p.next_observation {
                p.next_observation = ctx.sim_time + super::PERCEPTION_INTERVAL_S;
                if let Some(actor) = ctx
                    .targets
                    .iter()
                    .find(|actor| actor.instance_id == *target && !actor.caught)
                    && super::senses::visible(
                        agent,
                        ctx,
                        actor.position + Vec3::Y * (actor.height * 0.5),
                        agent.def.sight_range,
                        agent.def.sight_fov_degrees,
                    )
                {
                    p.last_known = Some(actor.position);
                    p.lost_seconds = 0.0;
                }
            }
            p.status = if p.lost_seconds < super::PERCEPTION_INTERVAL_S {
                "following"
            } else {
                "searching"
            };
            if p.lost_seconds > *memory_seconds {
                stop(agent);
                p.status = "target_lost";
                return Step::Running;
            }
            if let Some(point) = p.last_known {
                if arrived(agent, point, *distance) {
                    stop(agent);
                } else {
                    goal(agent, point, None);
                }
            }
            Step::Running
        }
        TaskDef::Routine { steps, looped } => {
            p.status = "routine";
            let Some(definition) = steps.get(p.index) else {
                if *looped {
                    p.index = 0;
                    p.child = None;
                    return Step::Running;
                }
                return Step::Complete;
            };
            let child = p.child.get_or_insert_with(|| Box::new(Progress::default()));
            match step(definition, child, token, agent, ctx, reservations, out) {
                Step::Complete => {
                    p.index = p.index.saturating_add(1);
                    p.child = None;
                    Step::Running
                }
                other @ (Step::Running | Step::Failed(_)) => other,
            }
        }
        TaskDef::Patrol {
            points,
            speed,
            memory_seconds,
            track_player,
            decision_seconds,
        } => {
            p.status = "patrol";
            if p.phase == 0 {
                p.stage_seconds = *decision_seconds;
                p.phase = 1;
            }
            if *track_player
                && (ctx.player_visible.contains(&agent.handle)
                    || agent.contact.as_ref().is_some_and(|contact| {
                        !contact.heard_only && contact.handle == super::PLAYER_TARGET_HANDLE
                    }))
                && let Some(player) = ctx.target(super::PLAYER_TARGET_HANDLE)
            {
                p.last_known = Some(player.position);
                p.lost_seconds = ctx.sim_time;
            }
            if p.stage_seconds < *decision_seconds {
                return Step::Running;
            }
            p.stage_seconds = 0.0;
            if *track_player
                && ctx.sim_time - p.lost_seconds < *memory_seconds
                && let Some(point) = p.last_known
            {
                p.status = "last_known_pursuit";
                goal(agent, point, Some(*speed));
                return Step::Running;
            }
            if let Some(point) = points
                .get(p.index.checked_rem(points.len()).unwrap_or(0))
                .copied()
            {
                let current = Vec3::from_array(point);
                let delta = (agent.position - current).abs();
                if delta.x + delta.y + delta.z < 0.8 {
                    p.index = p.index.saturating_add(1);
                }
                let target = points
                    .get(p.index.checked_rem(points.len()).unwrap_or(0))
                    .copied()
                    .map_or(current, Vec3::from_array);
                goal(agent, target, Some(*speed));
            }
            Step::Running
        }
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite world-space contact and eye vectors"
)]
fn use_point(
    point: &UsePoint,
    p: &mut Progress,
    token: OperationToken,
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    out: &mut AiOutcome,
    delivery_body: Option<Vec3>,
) -> Step {
    let approach = Vec3::from_array(point.approach);
    let contact = Vec3::from_array(point.contact);
    p.status = "approaching_interaction";
    if point.kind == super::authoring::UseKind::Door
        && ctx.doors.is_locked(&point.target) == Some(true)
    {
        return Step::Failed("door_locked");
    }
    if !ctx.objects.iter().any(|object| object.id == point.target)
        && ctx.doors.index_of(&point.target).is_none()
    {
        return Step::Failed("interaction_removed");
    }
    if !arrived(agent, approach, 0.06) {
        p.stage_seconds = 0.0;
        goal(agent, approach, None);
        return Step::Running;
    }
    stop(agent);
    if let Some(yaw) = point.yaw_degrees.map(|yaw| yaw.rem_euclid(360.0))
        && (agent.yaw_degrees - yaw).abs() > 0.001
    {
        agent.yaw_degrees = yaw;
        out.moved
            .push((agent.handle, agent.position, yaw, agent.spawned));
    }
    p.status = "contact";
    if let Some(object) = ctx.objects.iter().find(|object| object.id == point.target)
        && object.position.distance(contact) > object.radius.max(1.5)
    {
        return Step::Failed("contact_point_off_target");
    }
    let eye = agent.position + Vec3::Y * (agent.profile.height * 0.65);
    if eye.distance(contact) > point.reach
        || !super::perception::sight_clear_with_ignored_body(
            ctx.index,
            ctx.walls,
            ctx.leaves,
            ctx.obstacles,
            eye,
            contact,
            delivery_body.or(Some(contact)),
        )
    {
        return Step::Failed("interaction_out_of_reach");
    }
    if p.restore_pose {
        p.restore_pose = false;
        if p.phase < 2 {
            p.phase = 0;
            p.stage_seconds = 0.0;
        } else if !ctx.use_acks.iter().any(|ack| {
            ack.token == token
                && ack.target == point.target
                && ack.completed_at >= p.contact_requested_at
        }) && let Some(clip) = &point.clip
        {
            out.task_effects.push(TaskEffect::Animate {
                token,
                clip: clip.clone(),
            });
        }
    }
    if p.phase == 0 {
        if let Some(clip) = &point.clip {
            out.task_effects.push(TaskEffect::Animate {
                token,
                clip: clip.clone(),
            });
        }
        p.phase = 1;
        p.stage_seconds = 0.0;
    }
    if p.phase == 1 && p.stage_seconds >= point.contact_seconds {
        out.task_effects.push(TaskEffect::Use {
            token,
            point: point.clone(),
        });
        p.phase = 2;
        p.contact_requested_at = ctx.sim_time;
        p.stage_seconds = 0.0;
        return Step::Running;
    }
    if p.phase == 2
        && p.stage_seconds > 5.0
        && !ctx.use_acks.iter().any(|ack| {
            ack.token == token
                && ack.target == point.target
                && ack.completed_at >= p.contact_requested_at
        })
    {
        return Step::Failed("hand_contact_failed");
    }
    if p.phase == 2
        && p.stage_seconds >= point.hold_seconds
        && ctx.use_acks.iter().any(|ack| {
            ack.token == token
                && ack.target == point.target
                && ack.completed_at >= p.contact_requested_at
        })
    {
        Step::Complete
    } else {
        Step::Running
    }
}
