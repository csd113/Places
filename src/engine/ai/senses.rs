//! Mapped perception and bounded, idempotent alert propagation.
use super::{
    AiAgent, AiOutcome, AiTickContext,
    authoring::{SenseFilter, SenseResponse, TaskDef, TaskRequest},
};
use crate::engine::entities::id::EntityHandle;
use glam::Vec3;

#[derive(Debug)]
pub struct SenseMemory {
    handle: Option<EntityHandle>,
    stamp: f32,
    cursor: usize,
}
impl Default for SenseMemory {
    fn default() -> Self {
        Self {
            handle: None,
            stamp: f32::NEG_INFINITY,
            cursor: 0,
        }
    }
}
#[derive(Debug)]
struct Alert {
    serial: u64,
    point: Vec3,
    radius: f32,
    expires: f32,
    relays: Vec<(Vec3, u8)>,
    seen: Vec<EntityHandle>,
}
#[derive(Debug, Default)]
pub struct Alerts {
    records: Vec<Alert>,
    next: u64,
}
impl Alerts {
    #[must_use]
    pub const fn record_count(&self) -> usize {
        self.records.len()
    }
    fn publish(
        &mut self,
        source: EntityHandle,
        position: Vec3,
        point: Vec3,
        radius: f32,
        hops: u8,
        time: f32,
    ) {
        self.records.retain(|record| record.expires > time);
        if self.records.len() >= 32 {
            return;
        }
        let Some(serial) = self.next.checked_add(1) else {
            return;
        };
        self.next = serial;
        self.records.push(Alert {
            serial,
            point,
            radius,
            expires: time + 6.0,
            relays: vec![(position, hops.min(4))],
            seen: vec![source],
        });
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite bounded eye vectors; propagation counts use saturating operations"
)]
pub fn tick(
    agent: &mut AiAgent,
    ctx: &AiTickContext<'_>,
    alerts: &mut Alerts,
    out: &mut AiOutcome,
) {
    if agent.clock < agent.next_task_sense {
        return;
    }
    agent.next_task_sense = agent.clock + super::PERCEPTION_INTERVAL_S;
    agent
        .sense_memory
        .resize_with(agent.def.senses.len(), SenseMemory::default);
    for (index, rule) in agent.def.senses.iter().enumerate() {
        let memory = agent.sense_memory.get(index);
        let observation = match &rule.filter {
            SenseFilter::Hear { categories, radius } => ctx
                .stimuli
                .iter()
                .filter(|noise| {
                    noise.source != Some(agent.handle)
                        && (categories.is_empty()
                            || categories.iter().any(|kind| kind.matches(&noise.category)))
                        && noise.position.distance(agent.position)
                            <= radius.min(noise.audible_radius(0.0))
                })
                .min_by(|a, b| a.age.total_cmp(&b.age))
                .and_then(|noise| {
                    let stamp = ctx.sim_time - noise.age;
                    memory.is_none_or(|m| stamp > m.stamp + 0.01).then_some((
                        noise.source,
                        noise.position,
                        stamp,
                    ))
                }),
            SenseFilter::See {
                player,
                entities,
                objects,
                object_types,
                range,
                fov_degrees,
            } => {
                let actors = ctx
                    .targets
                    .iter()
                    .filter(|target| {
                        target.handle != agent.handle
                            && !target.caught
                            && if target.handle == super::PLAYER_TARGET_HANDLE {
                                *player
                            } else {
                                *entities
                            }
                    })
                    .map(|target| {
                        (
                            target.handle,
                            target.position + Vec3::Y * (target.height * 0.5),
                            target.position,
                            false,
                        )
                    });
                let props = ctx
                    .objects
                    .iter()
                    .filter(|object| {
                        objects.contains(&object.id) || object_types.contains(&object.object_type)
                    })
                    .map(|object| {
                        (
                            object.handle,
                            object.position + Vec3::Y * 0.05,
                            object.position,
                            true,
                        )
                    });
                let candidates = actors.chain(props);
                let count = candidates.clone().count();
                let first = memory
                    .map_or(0, |previous| previous.cursor)
                    .checked_rem(count)
                    .unwrap_or(0);
                let sighting = candidates
                    .cycle()
                    .skip(first)
                    .take(count.min(8))
                    .filter(|(_, gaze, _, object)| {
                        visible_geometry(agent, ctx, *gaze, *range, *fov_degrees, *object)
                    })
                    .min_by(|(_, _, a, _), (_, _, b, _)| {
                        a.distance_squared(agent.position)
                            .total_cmp(&b.distance_squared(agent.position))
                    })
                    .map(|(handle, _, point, _)| (Some(handle), point, ctx.sim_time));
                if let Some(updated) = agent.sense_memory.get_mut(index) {
                    updated.cursor = first.saturating_add(8);
                }
                sighting
            }
        };
        let Some((handle, point, stamp)) = observation else {
            if matches!(
                rule.filter,
                SenseFilter::See {
                    player: _,
                    entities: _,
                    objects: _,
                    object_types: _,
                    range: _,
                    fov_degrees: _
                }
            ) && let Some(reset_memory) = agent.sense_memory.get_mut(index)
            {
                reset_memory.handle = None;
            }
            continue;
        };
        if matches!(
            rule.filter,
            SenseFilter::See {
                player: _,
                entities: _,
                objects: _,
                object_types: _,
                range: _,
                fov_degrees: _
            }
        ) && agent
            .sense_memory
            .get(index)
            .is_some_and(|previous| previous.handle == handle)
        {
            continue;
        }
        if let Some(updated_memory) = agent.sense_memory.get_mut(index) {
            updated_memory.handle = handle;
            updated_memory.stamp = stamp;
        }
        match &rule.response {
            SenseResponse::Investigate { seconds, radius } => out.task_requests.push((
                agent.handle,
                TaskRequest {
                    key: format!("sense_{index}"),
                    priority: 100,
                    definition: TaskDef::Investigate {
                        point: point.to_array(),
                        seconds: *seconds,
                        radius: *radius,
                    },
                },
            )),
            SenseResponse::Task { request } => {
                out.task_requests.push((agent.handle, request.clone()));
            }
            SenseResponse::Alert { radius, hops } => alerts.publish(
                agent.handle,
                agent.position,
                point,
                *radius,
                *hops,
                ctx.sim_time,
            ),
        }
    }
    alerts
        .records
        .retain(|record| record.expires > ctx.sim_time);
    if !agent.def.receive_alerts {
        return;
    }
    for alert in &mut alerts.records {
        if alert.seen.contains(&agent.handle) || alert.seen.len() >= 256 {
            continue;
        }
        let remaining = alert
            .relays
            .iter()
            .filter(|(origin, hops)| *hops > 0 && origin.distance(agent.position) <= alert.radius)
            .map(|(_, hops)| hops.saturating_sub(1))
            .max();
        if let Some(hops) = remaining {
            alert.seen.push(agent.handle);
            if hops > 0 && alert.relays.len() < 16 {
                alert.relays.push((agent.position, hops));
            }
            out.task_requests.push((
                agent.handle,
                TaskRequest {
                    key: format!("alert_{}", alert.serial),
                    priority: 90,
                    definition: TaskDef::Investigate {
                        point: alert.point.to_array(),
                        seconds: 4.0,
                        radius: 1.0,
                    },
                },
            ));
        }
    }
}

#[must_use]
pub fn visible(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    point: Vec3,
    range: f32,
    fov: f32,
) -> bool {
    visible_geometry(agent, ctx, point, range, fov, false)
}

#[must_use]
pub fn visible_object(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    point: Vec3,
    range: f32,
    fov: f32,
) -> bool {
    visible_geometry(agent, ctx, point, range, fov, true)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite world-space sight vectors"
)]
fn visible_geometry(
    agent: &AiAgent,
    ctx: &AiTickContext<'_>,
    point: Vec3,
    range: f32,
    fov: f32,
    object_endpoint: bool,
) -> bool {
    let eye = agent.position + Vec3::Y * (agent.profile.height * 0.5).max(0.05);
    let offset = point - eye;
    if !point.is_finite() || offset.length() > range {
        return false;
    }
    let flat = Vec3::new(offset.x, 0.0, offset.z).normalize_or_zero();
    if fov < 360.0
        && flat.dot(super::perception::yaw_direction(agent.yaw_degrees))
            < (fov * 0.5).to_radians().cos()
    {
        return false;
    }
    super::perception::sight_clear_with_solids(
        ctx.index,
        ctx.walls,
        ctx.leaves,
        ctx.obstacles,
        eye,
        point,
        object_endpoint,
    )
}
