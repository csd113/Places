//! Locomotion while the current route leg waits for a swinging leaf.

use glam::{Vec2, Vec3};
use std::collections::HashMap;

use crate::engine::collision::{DoorCollider, door_segment_blocked};
use crate::engine::door::{DoorPhase, DoorRuntime};
use crate::engine::entities::id::EntityHandle;
use crate::engine::nav::{NavBounds, NavDoorState, NavMesh, Path, PathQuery, PathResult};

use super::{AgentMove, AiAgent, AiOutcome, AiTickContext, MoveStep, PATH_RETRY_S, STUCK_SECONDS};
use super::{actor_segment_clear, movement};

#[derive(Debug)]
pub(super) struct Approach {
    door: usize,
    waypoint: Vec3,
    stand_off: Option<Vec3>,
    stalled_since: Option<f32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PortalUse {
    Waiting,
    Detour,
    Leaving,
}

/// The planner may keep a route through the door this actor is already opening.
/// Other doors and live solid obstacles retain their normal planning policy.
pub(super) struct OpeningDoor<'a> {
    ctx: &'a AiTickContext<'a>,
    door: &'a DoorRuntime,
    usage: PortalUse,
}

impl NavDoorState for OpeningDoor<'_> {
    fn door_open(&self, id: &str) -> bool {
        (self.usage != PortalUse::Waiting && id == self.door.def.id) || self.ctx.door_open(id)
    }

    fn door_locked(&self, id: &str) -> Option<bool> {
        self.ctx.door_locked(id)
    }

    fn door_obstructed(&self, id: &str) -> bool {
        id != self.door.def.id && self.ctx.door_obstructed(id)
    }

    fn door_leaf(&self, id: &str) -> Option<DoorCollider> {
        if id != self.door.def.id {
            return self.ctx.door_leaf(id);
        }
        (self.usage != PortalUse::Leaving).then(|| self.door.closed_collider())
    }

    fn obstacle_blocked(&self, point: Vec3, radius: f32, height: f32) -> bool {
        self.ctx.obstacle_blocked(point, radius, height)
    }

    fn obstacle_segment_blocked(&self, from: Vec3, to: Vec3, radius: f32, height: f32) -> bool {
        self.ctx.obstacle_segment_blocked(from, to, radius, height)
            || (self.usage == PortalUse::Detour
                && door_segment_blocked(
                    xz(from),
                    xz(to),
                    radius,
                    from.y.min(to.y),
                    height + (to.y - from.y).abs(),
                    &[self.door.collider()],
                ))
    }
}

pub(super) fn plan(
    agent: &mut AiAgent,
    nav: &NavMesh,
    query: &PathQuery<'_>,
    trial: &mut Path,
    bounds: Option<NavBounds>,
    ctx: &AiTickContext<'_>,
) -> Result<(), PathResult> {
    let opening = agent
        .opening_door
        .as_ref()
        .and_then(|approach| ctx.doors.get(approach.door))
        .filter(|door| door.phase() == DoorPhase::Opening && !door.is_locked());
    let overlay = opening
        .map(|door| OpeningDoor {
            ctx,
            door,
            usage: PortalUse::Waiting,
        })
        .or_else(|| blocked_start_portal(ctx, nav, agent.position));
    let opening_query = PathQuery {
        doors: overlay.as_ref().map_or(query.doors, |doors| doors),
        ..*query
    };
    nav.path_into_bounded(&opening_query, &mut agent.scratch, trial, bounds)
}

/// Permit a detour to start inside a blocked portal. The closed throat stays
/// blocked in topology, and the live leaf still blocks physical segments.
fn blocked_start_portal<'a>(
    ctx: &'a AiTickContext<'a>,
    nav: &NavMesh,
    position: Vec3,
) -> Option<OpeningDoor<'a>> {
    let id = portal_at(nav, position)?;
    let door = ctx.doors.get(ctx.doors.index_of(id)?)?;
    (door.phase() == DoorPhase::Opening
        && door.obstructed
        && !door.def.bidirectional
        && !door.is_locked())
    .then_some(OpeningDoor {
        ctx,
        door,
        usage: PortalUse::Detour,
    })
}

const fn xz(point: Vec3) -> Vec2 {
    Vec2::new(point.x, point.z)
}

fn crosses_leaf(agent: &AiAgent, waypoint: Vec3, leaf: &DoorCollider) -> bool {
    door_segment_blocked(
        xz(agent.position),
        xz(waypoint),
        agent.profile.radius,
        agent.position.y.min(waypoint.y),
        agent.profile.height + (waypoint.y - agent.position.y).abs(),
        std::slice::from_ref(leaf),
    )
}

fn current_door(agent: &AiAgent, ctx: &AiTickContext<'_>, waypoint: Vec3) -> Option<usize> {
    if !agent.profile.can_open_doors {
        return None;
    }
    agent.path.door_requests.iter().find_map(|id| {
        let index = ctx.doors.index_of(id)?;
        let door = ctx.doors.get(index)?;
        (!door.is_locked()
            && door.phase() != DoorPhase::Open
            && crosses_leaf(agent, waypoint, &door.closed_collider()))
        .then_some(index)
    })
}

fn portal_at(nav: &NavMesh, point: Vec3) -> Option<&str> {
    let grid = nav.grid();
    let (x, z) = grid.cell_at(point.x, point.z)?;
    grid.column_range(x, z)
        .filter_map(|node| grid.surface(node).map(|y| (node, (y - point.y).abs())))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .and_then(|(node, _)| grid.cell_portal.get(node))
        .and_then(|portal| grid.portals.get(usize::from(*portal)))
        .map(|portal| portal.door.as_str())
}

/// A new leg can leave the old door's portal without operating the door.
/// The ordinary physical mover still sweeps the live leaf on every step.
pub(super) fn leaving_portal<'a>(
    agent: &AiAgent,
    ctx: &'a AiTickContext<'a>,
    nav: &NavMesh,
    waypoint: Vec3,
) -> Option<OpeningDoor<'a>> {
    if ctx.doors.is_empty() {
        return None;
    }
    let id = portal_at(nav, agent.position)?;
    let door = ctx.doors.get(ctx.doors.index_of(id)?)?;
    (door.phase() == DoorPhase::Opening
        && !door.def.bidirectional
        && !door.is_locked()
        && !crosses_leaf(agent, waypoint, &door.closed_collider()))
    .then_some(OpeningDoor {
        ctx,
        door,
        usage: PortalUse::Leaving,
    })
}

fn request_open(agent: &AiAgent, door: &DoorRuntime, out: &mut AiOutcome) {
    if door.phase() != DoorPhase::Opening
        && !out.door_requests.iter().any(|(id, _)| id == &door.def.id)
    {
        out.door_requests.push((door.def.id.clone(), agent.handle));
    }
}

pub(super) fn advance(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    actors: &HashMap<EntityHandle, Vec3>,
    nav: &NavMesh,
    waypoint: Vec3,
    speed: f32,
    out: &mut AiOutcome,
) -> bool {
    let Some(index) = current_door(agent, ctx, waypoint) else {
        agent.opening_door = None;
        return false;
    };
    let Some(door) = ctx.doors.get(index) else {
        return false;
    };
    let previously_waiting = agent
        .opening_door
        .take()
        .filter(|approach| approach.door == index);
    if previously_waiting.is_none() {
        agent.reset_stuck();
    }
    let mut approach = previously_waiting.unwrap_or(Approach {
        door: index,
        waypoint,
        stand_off: None,
        stalled_since: None,
    });
    if !approach
        .waypoint
        .abs_diff_eq(waypoint, movement::ARRIVE_RADIUS_M)
    {
        approach.waypoint = waypoint;
        approach.stand_off = None;
    }
    agent.speed_mps = 0.0;
    let needs_retreat = !door.def.bidirectional
        && (door.opening_sweep_overlaps(
            agent.position,
            agent.profile.radius,
            agent.profile.height,
        ) || (approach.stand_off.is_some()
            && portal_at(nav, agent.position) == Some(door.def.id.as_str())));
    if needs_retreat {
        if approach.stand_off.is_none() && approach.stalled_since.is_none() {
            approach.stand_off = find_stand_off(agent, ctx, actors, nav, door);
        }
        if let Some(target) = approach.stand_off {
            if let MoveStep::Moved {
                position,
                yaw_degrees,
                speed_mps,
            } = step(agent, ctx, target, speed, ctx.delta)
                && agent.allows_position(position)
                && actor_segment_clear(agent, agent.position, position, ctx, actors)
            {
                agent.position = position;
                agent.yaw_degrees = yaw_degrees;
                agent.speed_mps = speed_mps;
                agent.reset_stuck();
                approach.stalled_since = None;
                agent.opening_door = Some(approach);
                out.moved
                    .push((agent.handle, position, yaw_degrees, agent.spawned));
                return true;
            }
            approach.stand_off = None;
        }
        request_open(agent, door, out);
    } else {
        request_open(agent, door, out);
        if !door.obstructed {
            agent.reset_stuck();
            approach.stalled_since = None;
            agent.opening_door = Some(approach);
            return true;
        }
    }
    let stalled_since = *approach
        .stalled_since
        .get_or_insert(agent.clock - ctx.delta);
    agent.stuck_seconds = agent.clock - stalled_since;
    if agent.stuck_seconds >= STUCK_SECONDS && door.obstructed {
        agent.path.clear();
        if agent.ordinary_wandering() {
            agent.path_goal = None;
        }
        agent.path_done_goal = None;
        agent.path_failed_at = Some(agent.clock - PATH_RETRY_S);
        agent.current_waypoint = 0;
        agent.reset_stuck();
    } else {
        if agent.stuck_seconds >= STUCK_SECONDS {
            agent.reset_stuck();
            approach.stalled_since = None;
        }
        agent.opening_door = Some(approach);
    }
    true
}

fn step(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    target: Vec3,
    speed: f32,
    delta: f32,
) -> MoveStep {
    AgentMove {
        position: agent.position,
        yaw_degrees: agent.yaw_degrees,
        radius: agent.profile.radius,
        height: agent.profile.height,
        step_height: agent.profile.step_height,
        max_slope: agent.profile.max_slope,
        speed_mps: speed,
    }
    .step_with_solids(
        target,
        delta,
        ctx.walls,
        ctx.index,
        ctx.floor,
        ctx.ceiling,
        ctx.leaves,
        ctx.obstacles,
    )
}

/// Search near-to-far on the approach side. A stand-off needs both portal
/// clearance and a full physical sweep from the current feet. The retreat's
/// next tick repeats the same physical checks against newly moved obstructions.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite metre-scale sampling and steering vectors."
)]
fn find_stand_off(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    actors: &HashMap<EntityHandle, Vec3>,
    nav: &NavMesh,
    door: &DoorRuntime,
) -> Option<Vec3> {
    let (nx, nz) = door.def.closed_normal();
    let normal = Vec2::new(nx, nz);
    let hinge = Vec2::new(door.def.x, door.def.z);
    let offset = xz(agent.position) - hinge;
    let outward = normal * 1.0_f32.copysign(offset.dot(normal));
    let spacing = nav.grid().cell_m.clamp(0.1, 0.3);
    // One mover call resolves up to 64 substeps at this spacing. Longer
    // probes use consecutive sweeps with the same collision rules below.
    let reach = door.def.width + door.def.thickness + agent.profile.radius + spacing;
    for ring in 1_u16..=128 {
        let distance = spacing * f32::from(ring);
        if distance > reach + spacing {
            break;
        }
        for angle in [0.0_f32, 22.5, -22.5, 45.0, -45.0, 67.5, -67.5, 90.0, -90.0] {
            let direction = Vec2::from_angle(angle.to_radians()).rotate(outward);
            let point = xz(agent.position) + direction * distance;
            let Some((column, row)) = nav.grid().cell_at(point.x, point.y) else {
                continue;
            };
            let (x, z) = nav.grid().cell_center(column, row);
            let target = Vec3::new(x, agent.position.y, z);
            if (xz(target) - hinge).dot(outward) < 0.0
                || !agent.allows_position(target)
                || portal_at(nav, target) == Some(door.def.id.as_str())
                || door.opening_sweep_overlaps(target, agent.profile.radius, agent.profile.height)
            {
                continue;
            }
            if let Some(reached) = clear_retreat(agent, ctx, actors, target)
                && agent.allows_position(reached)
                && portal_at(nav, reached) != Some(door.def.id.as_str())
                && !door.opening_sweep_overlaps(reached, agent.profile.radius, agent.profile.height)
            {
                return Some(reached);
            }
        }
    }
    None
}

fn clear_retreat(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    actors: &HashMap<EntityHandle, Vec3>,
    target: Vec3,
) -> Option<Vec3> {
    let delta = movement::MAX_MOVE_STEP_S;
    let mut mover = AgentMove {
        position: agent.position,
        yaw_degrees: agent.yaw_degrees,
        radius: agent.profile.radius,
        height: agent.profile.height,
        step_height: agent.profile.step_height,
        max_slope: agent.profile.max_slope,
        speed_mps: 1.0 / delta,
    };
    for _ in 0_u8..24 {
        let from = mover.position;
        let result = mover.step_with_solids(
            target,
            delta,
            ctx.walls,
            ctx.index,
            ctx.floor,
            ctx.ceiling,
            ctx.leaves,
            ctx.obstacles,
        );
        if !matches!(
            result,
            MoveStep::Moved {
                position: _,
                yaw_degrees: _,
                speed_mps: _
            }
        ) || !actor_segment_clear(agent, from, mover.position, ctx, actors)
        {
            return None;
        }
        if xz(mover.position).distance(xz(target)) < 0.001 {
            return Some(mover.position);
        }
    }
    None
}
