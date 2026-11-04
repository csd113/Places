//! Portable AI intent uses semantic identities, never world-local handles.
use super::{AiAgent, AiState, EntityHandle, PLAYER_TARGET_HANDLE};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    intent: String,
    point: Option<Vec3>,
    target: Option<String>,
    clock: f32,
    wait: f32,
    cooldown: f32,
    caught: bool,
    home: Vec3,
    home_yaw: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search: Option<super::PredatorSearchMemory>,
}

impl Record {
    /// # Errors
    /// Refuses tasks or regions tied to the departing map's runtime.
    pub fn capture(
        agent: &AiAgent,
        name: impl Fn(EntityHandle) -> Option<String>,
    ) -> Result<Self, String> {
        if !agent.controller.runs.is_empty()
            || agent.controller.active.is_some()
            || agent.controller.sequence_owns_motion
            || agent.sequence_state.is_some()
            || agent.def.home_bounds.is_some()
        {
            return Err(format!(
                "NPC '{}' has a map-bound task, sequence or home region; finish it before elevator travel",
                agent.instance_id
            ));
        }
        let point = match agent.state {
            AiState::Wander { destination }
            | AiState::Flee {
                destination,
                threat: _,
            }
            | AiState::Hide { destination }
            | AiState::Return { destination } => Some(destination),
            AiState::Investigate { point } => Some(point),
            AiState::Idle
            | AiState::Follow { target: _ }
            | AiState::Hidden
            | AiState::Pursue { target: _ }
            | AiState::Catch { target: _ }
            | AiState::Scripted
            | AiState::Caught => None,
        };
        let target = match agent.state {
            AiState::Follow { target } | AiState::Pursue { target } | AiState::Catch { target } => {
                Some(target)
            }
            AiState::Flee {
                threat,
                destination: _,
            } => threat,
            AiState::Idle
            | AiState::Wander { destination: _ }
            | AiState::Hide { destination: _ }
            | AiState::Hidden
            | AiState::Return { destination: _ }
            | AiState::Investigate { point: _ }
            | AiState::Scripted
            | AiState::Caught => None,
        }
        .and_then(|handle| {
            if handle == PLAYER_TARGET_HANDLE {
                Some("@player".into())
            } else {
                name(handle)
            }
        });
        Ok(Self {
            intent: agent.state.name().into(),
            point,
            target,
            clock: agent.clock,
            wait: agent.wait_remaining,
            cooldown: agent.catch_cooldown,
            caught: agent.caught,
            home: agent.home,
            home_yaw: agent.home_yaw,
            search: agent.search.clone(),
        })
    }

    pub fn transform(&mut self, from: Vec3, to: Vec3, rotation: Quat) {
        self.point = self
            .point
            .map(|p| crate::engine::elevator::map_cabin_point(from, to, rotation, p));
        self.home = crate::engine::elevator::map_cabin_point(from, to, rotation, self.home);
        if let Some(memory) = self.search.as_mut() {
            memory.point =
                crate::engine::elevator::map_cabin_point(from, to, rotation, memory.point);
        }
        let facing = rotation.mul_quat(Quat::from_rotation_y(self.home_yaw.to_radians()));
        self.home_yaw = facing.to_euler(glam::EulerRot::YXZ).0.to_degrees();
    }

    /// A post outside the cabin belongs to the departing map. The visitor's
    /// arrival becomes its new post; an in-cabin post retains its exact offset.
    pub fn cabin_home(
        &mut self,
        frame: &crate::engine::elevator::CabinFrame,
        position: Vec3,
        yaw: f32,
    ) {
        let home = frame.local(self.home);
        if home.x.abs() >= 1.2 || home.z <= 0.36 || home.z >= 2.4 || home.y.abs() > 0.25 {
            self.home = position;
            self.home_yaw = yaw;
        }
    }

    pub fn remap_target(&mut self, names: &[(String, String)]) {
        if let Some(target) = self.target.as_mut()
            && let Some((_, replacement)) = names.iter().find(|(old, _)| old == target)
        {
            target.clone_from(replacement);
        }
    }

    pub fn retain_targets(&mut self, ids: &[String]) {
        if self
            .target
            .as_ref()
            .is_some_and(|id| id != "@player" && !ids.contains(id))
        {
            self.target = None;
        }
    }

    /// # Errors
    /// Rejects malformed clocks or goals before installing the retained intent.
    pub fn restore(
        &self,
        agent: &mut AiAgent,
        resolve: impl Fn(&str) -> Option<EntityHandle>,
    ) -> Result<(), String> {
        if self.search.as_ref().is_some_and(|memory| !memory.valid()) {
            return Err("Invalid transported NPC search memory".into());
        }
        for v in [self.clock, self.wait, self.cooldown] {
            crate::engine::entities::checkpoint::number(v, 10_000_000.0)?;
        }
        if self
            .point
            .is_some_and(|p| !p.is_finite() || p.abs().max_element() > 100_000.0)
        {
            return Err("Invalid transported NPC destination".into());
        }
        let resolved_target = self.target.as_deref().and_then(|id| {
            if id == "@player" {
                Some(PLAYER_TARGET_HANDLE)
            } else {
                resolve(id)
            }
        });
        agent.state = match (self.intent.as_str(), self.point, resolved_target) {
            ("wander", Some(destination), _) => AiState::Wander { destination },
            ("flee", Some(destination), threat) => AiState::Flee {
                destination,
                threat,
            },
            ("hide", Some(destination), _) => AiState::Hide { destination },
            ("return", Some(destination), _) => AiState::Return { destination },
            ("investigate", Some(point), _) => AiState::Investigate { point },
            ("follow", _, Some(target)) => AiState::Follow { target },
            ("pursue", _, Some(target)) => AiState::Pursue { target },
            ("catch", _, Some(target)) => AiState::Catch { target },
            ("caught", _, _) => AiState::Caught,
            ("hidden", _, _) => AiState::Hidden,
            _ => AiState::Idle,
        };
        agent.clock = self.clock;
        agent.wait_remaining = self.wait;
        agent.catch_cooldown = self.cooldown;
        agent.caught = self.caught;
        agent.home = self.home;
        agent.home_yaw = self.home_yaw;
        agent.path.clear();
        agent.path_goal = None;
        agent.path_done_goal = None;
        agent.path_failed_at = None;
        agent.navigation_goal = None;
        agent.current_waypoint = 0;
        agent.speed_mps = 0.0;
        agent.last_known = None;
        agent.search.clone_from(&self.search);
        agent.contact = None;
        agent.interest = None;
        agent.class = None;
        agent.reported_unplaced = false;
        Ok(())
    }
}

impl super::AiWorld {
    pub(crate) fn restore_passenger(
        &mut self,
        id: &str,
        record: &Record,
        position: Vec3,
        yaw: f32,
        resolve: impl Fn(&str) -> Option<EntityHandle>,
    ) -> Result<(), String> {
        let agent = self
            .agents
            .iter_mut()
            .find(|a| a.instance_id == id)
            .ok_or("Transported NPC is missing")?;
        record.restore(agent, resolve)?;
        agent.position = position;
        agent.yaw_degrees = yaw;
        Ok(())
    }
}
