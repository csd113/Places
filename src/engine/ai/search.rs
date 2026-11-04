//! Predator search uses observations, bounded clocks and completed routes only.
use super::{AiAgent, AiOutcome, AiState, AiTickContext, PathQuery};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    /// Last observed visual/noise point; never a hidden target lookup.
    pub point: Vec3,
    /// Agent-clock deadline, preserved by checkpoints and cabin transport.
    pub until: f32,
    /// Last handled noise timestamp, so a retained stimulus cannot rearm itself.
    pub heard_at: f32,
}

impl Memory {
    pub(super) fn valid(&self) -> bool {
        self.point.is_finite()
            && self.point.abs().max_element() <= 100_000.0
            && self.until.is_finite()
            && (0.0..=10_000_000.0).contains(&self.until)
            && self.heard_at.is_finite()
            && (0.0..=10_000_000.0).contains(&self.heard_at)
    }
}

pub(super) fn observe_visual(agent: &mut AiAgent, point: Vec3) {
    agent.search = Some(Memory {
        point,
        until: agent.last_seen_at + agent.def.search_after_seconds,
        heard_at: agent.clock,
    });
}

fn clear_route(agent: &mut AiAgent) {
    agent.path.clear();
    agent.path_goal = None;
    agent.path_failed_at = None;
    agent.path_done_goal = None;
    agent.path_age = 0.0;
}

pub(super) fn decide(agent: &mut AiAgent, ctx: &AiTickContext<'_>, out: &mut AiOutcome) {
    // Hearing remains a real reaction, but a retained sound cannot continuously
    // refresh an investigation. New sounds get a short visit, not visual sight.
    if let Some(contact) = agent.contact.as_ref().filter(|contact| contact.heard_only) {
        let heard_at = (agent.clock - contact.age).max(0.0);
        if agent
            .search
            .as_ref()
            .is_none_or(|memory| heard_at > memory.heard_at + 0.05)
        {
            let until = heard_at + agent.def.search_after_seconds.min(2.0);
            let previous = agent.search.as_ref().map_or(until, |memory| memory.until);
            agent.search = Some(Memory {
                point: contact.position,
                until: until.max(previous),
                heard_at,
            });
            clear_route(agent);
        }
    }
    agent.interest = None;
    let Some(memory) = agent.search.as_ref() else {
        if matches!(
            agent.state,
            AiState::Pursue { target: _ } | AiState::Investigate { point: _ }
        ) {
            super::set_state(agent, AiState::Idle, out);
        }
        super::decide_idler(agent, ctx, out);
        return;
    };
    let point = memory.point;
    if agent.clock < memory.until {
        if !matches!(agent.state, AiState::Investigate { point: goal } if goal == point) {
            super::set_state(agent, AiState::Investigate { point }, out);
            clear_route(agent);
        }
        return;
    }
    if let AiState::Wander { destination } = agent.state {
        let finished = agent.position.distance(destination) < 0.5
            || agent.path_done_goal == Some(destination)
            || agent.path_failed_at.is_some();
        if !finished {
            return;
        }
        agent.wait_remaining = agent.def.idle_seconds.min(1.0);
    }
    if !matches!(agent.state, AiState::Idle) {
        super::set_state(agent, AiState::Idle, out);
        clear_route(agent);
    }
    agent.wait_remaining = (agent.wait_remaining - ctx.delta).max(0.0);
    if agent.wait_remaining > 0.0 {
        return;
    }
    if let Some(destination) = destination(agent, ctx, out, point) {
        super::set_state(agent, AiState::Wander { destination }, out);
    }
}

fn destination(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    out: &mut AiOutcome,
    avoid: Vec3,
) -> Option<Vec3> {
    if !super::path_attempt_due(agent) {
        return None;
    }
    let nav = ctx.nav?;
    let class = agent.class?;
    let bounds = agent.bounds();
    if bounds.is_some_and(|volume| !volume.min.cmple(volume.max).all()) {
        // A valid authored rectangle can still be narrower than this body.
        // It has no search destination; do not clamp against inverted bounds.
        agent.last_path_attempt_at = agent.clock;
        return None;
    }
    let salt = f32::from(u16::try_from(agent.perception_phase % 31).unwrap_or(0));
    let base = agent.clock.mul_add(0.7, salt);
    agent.last_path_attempt_at = agent.clock;
    for index in 0_u16..16 {
        let angle = f32::from(index).mul_add(std::f32::consts::TAU / 16.0, base);
        let (sin, cos) = angle.sin_cos();
        let fraction = f32::from(index % 4).mul_add(0.2, 0.4);
        let distance = (agent.def.search_radius * fraction).max(2.0);
        let mut candidate = Vec3::new(
            cos.mul_add(distance, agent.position.x),
            agent.position.y,
            sin.mul_add(distance, agent.position.z),
        );
        if let Some(volume) = bounds {
            candidate = candidate.clamp(volume.min, volume.max);
        }
        let Some(point) = nav.nearest(
            class,
            candidate,
            2.5,
            1.5,
            ctx,
            agent.profile.can_open_doors,
        ) else {
            continue;
        };
        if point.position.distance(avoid) < 2.0
            || point.position.distance(agent.position) < 1.5
            || point.position.distance(agent.position) > agent.def.search_radius + 0.01
            || bounds.is_some_and(|volume| !volume.contains(point.position))
        {
            continue;
        }
        if !super::reserve_path_query(agent, out) {
            break;
        }
        let query = PathQuery {
            class,
            start: agent.position,
            goal: point.position,
            can_open_doors: agent.profile.can_open_doors,
            max_expansions: super::RUNTIME_PATH_EXPANSIONS,
            doors: ctx,
        };
        let result = nav.path_into_bounded(&query, &mut agent.scratch, &mut agent.path, bounds);
        if result.is_ok() && agent.path.complete {
            agent.path_goal = Some(point.position);
            agent.path_door_version = ctx.door_version;
            agent.path_age = 0.0;
            agent.path_failed_at = None;
            agent.path_done_goal = None;
            agent.current_waypoint = 0;
            return Some(point.position);
        }
    }
    // No reachable alternate area is a safe standstill with bounded retries.
    // Do not leave a rejected partial route installed for the mover to follow.
    agent.path.clear();
    agent.path_goal = None;
    None
}
