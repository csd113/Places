//! One arbitration contract for baseline, tasks, authored sequences and holds.
use super::{AiAgent, AiState, tasks::TaskRun};
use crate::engine::entities::id::EntityHandle;
use glam::Vec3;
use serde::Serialize;

/// A world epoch, entity generation and monotonic operation identify every side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OperationToken {
    pub epoch: u64,
    pub owner: EntityHandle,
    pub serial: u64,
}

/// Hard interruptions outrank authored priorities (0..=200).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionOwner {
    #[default]
    Baseline,
    Task,
    Sequence,
    Light,
    Caught,
}

impl MotionOwner {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Task => "task",
            Self::Sequence => "sequence",
            Self::Light => "light",
            Self::Caught => "caught",
        }
    }
}

#[derive(Debug)]
struct BaselineIntent {
    state: AiState,
    goal: Option<Vec3>,
    speed: Option<f32>,
}

#[derive(Debug, Default)]
pub struct Controller {
    pub owner: MotionOwner,
    pub runs: Vec<TaskRun>,
    pub active: Option<OperationToken>,
    pub sequence_owns_motion: bool,
    baseline: Option<BaselineIntent>,
    pub last_result: Option<String>,
}

impl Controller {
    pub fn suspend_baseline(&mut self, agent: &AiAgent) {
        if self.baseline.is_none() {
            self.baseline = Some(BaselineIntent {
                state: agent.state,
                goal: agent.navigation_goal,
                speed: agent.navigation_speed,
            });
        }
    }
    pub fn resume_baseline(&mut self, agent: &mut AiAgent) {
        if let Some(intent) = self.baseline.take() {
            agent.state = intent.state;
            agent.navigation_goal = intent.goal;
            agent.navigation_speed = intent.speed;
            agent.path.clear();
            agent.path_goal = None;
            agent.path_done_goal = None;
            agent.path_failed_at = None;
            agent.current_waypoint = 0;
            agent.speed_mps = 0.0;
        }
    }
    pub fn arbitrate(&mut self, caught: bool, sequence: bool, light: bool) -> MotionOwner {
        self.sequence_owns_motion = sequence;
        self.owner = if caught {
            MotionOwner::Caught
        } else if light {
            MotionOwner::Light
        } else if sequence {
            MotionOwner::Sequence
        } else if self.runs.is_empty() {
            MotionOwner::Baseline
        } else {
            MotionOwner::Task
        };
        self.owner
    }
    #[must_use]
    pub fn owns(&self, token: OperationToken) -> bool {
        self.active == Some(token)
            && !self.sequence_owns_motion
            && matches!(self.owner, MotionOwner::Task | MotionOwner::Light)
            && self
                .runs
                .iter()
                .any(|run| run.token == token && !run.paused)
    }
}

/// Presentation invalidation is independent of locomotion/caught state.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum PoseInvalidation {
    #[default]
    Clean,
    Dirty,
}
impl PoseInvalidation {
    pub fn mark(&mut self) {
        *self = Self::Dirty;
    }
    pub fn take(&mut self) -> bool {
        std::mem::take(self) == Self::Dirty
    }
}
