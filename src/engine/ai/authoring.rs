//! Stable shared behaviour contracts for serialized content and scoped Luau.
use serde::{Deserialize, Serialize};

/// An exact item or a catalogue model type. Searches only consider observed loose bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "match",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ItemSelector {
    Id(String),
    ObjectType(String),
}

/// Interaction uses a reachable feet point and a separate physical contact point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsePoint {
    pub target: String,
    pub approach: [f32; 3],
    pub contact: [f32; 3],
    #[serde(default)]
    pub yaw_degrees: Option<f32>,
    #[serde(default = "reach")]
    pub reach: f32,
    #[serde(default)]
    pub kind: UseKind,
    #[serde(default = "contact_seconds")]
    pub contact_seconds: f32,
    #[serde(default)]
    pub hold_seconds: f32,
    #[serde(default)]
    pub clip: Option<String>,
    /// Optional posed node required to touch the contact point before interaction.
    #[serde(default)]
    pub socket: Option<String>,
}
const fn reach() -> f32 {
    1.5
}
const fn contact_seconds() -> f32 {
    0.35
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UseKind {
    #[default]
    Interact,
    Door,
    Chair,
    Switch,
    Shelf,
}

/// A routine is deliberately a bounded list of the same engine behaviours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskDef {
    Move {
        point: [f32; 3],
        #[serde(default)]
        speed: Option<f32>,
    },
    Wait {
        seconds: f32,
    },
    Investigate {
        point: [f32; 3],
        seconds: f32,
        radius: f32,
    },
    Fetch {
        item: ItemSelector,
        destination: UsePoint,
        #[serde(default = "search_radius")]
        search_radius: f32,
    },
    Use {
        point: UsePoint,
    },
    Follow {
        target: String,
        distance: f32,
        memory_seconds: f32,
    },
    Routine {
        steps: Vec<Self>,
        #[serde(default)]
        looped: bool,
    },
    Patrol {
        points: Vec<[f32; 3]>,
        speed: f32,
        memory_seconds: f32,
        #[serde(default)]
        track_player: bool,
        #[serde(default = "decision_seconds")]
        decision_seconds: f32,
    },
}
const fn decision_seconds() -> f32 {
    0.8
}
const fn search_radius() -> f32 {
    30.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRequest {
    pub key: String,
    pub priority: u8,
    pub definition: TaskDef,
}

/// Typed gameplay noise classes. Ambient audio is opt-in through `sound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoiseKind {
    Movement,
    Door,
    Interact,
    Spawn,
    Impact,
    Sound,
    Alert,
}
impl NoiseKind {
    #[must_use]
    pub fn matches(self, label: &str) -> bool {
        matches!(
            (self, label),
            (Self::Movement, "movement")
                | (Self::Door, "door")
                | (Self::Interact, "interact")
                | (Self::Spawn, "spawn")
                | (Self::Impact, "impact")
                | (Self::Sound, "sound")
                | (Self::Alert, "alert")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "sense", rename_all = "snake_case", deny_unknown_fields)]
pub enum SenseFilter {
    Hear {
        #[serde(default)]
        categories: Vec<NoiseKind>,
        radius: f32,
    },
    See {
        #[serde(default)]
        player: bool,
        #[serde(default)]
        entities: bool,
        #[serde(default)]
        objects: Vec<String>,
        #[serde(default)]
        object_types: Vec<String>,
        range: f32,
        fov_degrees: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case", deny_unknown_fields)]
pub enum SenseResponse {
    Investigate { seconds: f32, radius: f32 },
    Task { request: TaskRequest },
    Alert { radius: f32, hops: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SenseRule {
    pub filter: SenseFilter,
    pub response: SenseResponse,
}

pub fn name_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 96 && !value.chars().any(char::is_control)
}
fn position_valid(point: &[f32; 3]) -> bool {
    point.iter().all(|v| v.is_finite() && v.abs() <= 100_000.0)
}
fn bounded(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && (min..=max).contains(&value)
}
impl UsePoint {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        name_valid(&self.target)
            && position_valid(&self.approach)
            && position_valid(&self.contact)
            && self
                .yaw_degrees
                .is_none_or(|yaw| bounded(yaw, -3600.0, 3600.0))
            && bounded(self.reach, 0.05, 3.0)
            && bounded(self.contact_seconds, 0.05, 30.0)
            && bounded(self.hold_seconds, 0.0, 600.0)
            && self.clip.as_deref().is_none_or(name_valid)
            && self.socket.as_deref().is_none_or(name_valid)
    }
}
impl TaskDef {
    #[must_use]
    pub fn requires_objects(&self) -> bool {
        match self {
            Self::Fetch {
                item: _,
                destination: _,
                search_radius: _,
            }
            | Self::Use { point: _ } => true,
            Self::Routine { steps, looped: _ } => steps.iter().any(Self::requires_objects),
            Self::Move { point: _, speed: _ }
            | Self::Wait { seconds: _ }
            | Self::Investigate {
                point: _,
                seconds: _,
                radius: _,
            }
            | Self::Follow {
                target: _,
                distance: _,
                memory_seconds: _,
            }
            | Self::Patrol {
                points: _,
                speed: _,
                memory_seconds: _,
                track_player: _,
                decision_seconds: _,
            } => false,
        }
    }
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.bounded_nodes(&mut 256) && self.valid_at(0)
    }
    fn bounded_nodes(&self, remaining: &mut u16) -> bool {
        let Some(next) = remaining.checked_sub(1) else {
            return false;
        };
        *remaining = next;
        if let Self::Routine { steps, looped: _ } = self {
            steps.iter().all(|step| step.bounded_nodes(remaining))
        } else {
            true
        }
    }
    pub fn references_into(&self, ids: &mut Vec<String>) {
        match self {
            Self::Fetch {
                item,
                destination,
                search_radius: _,
            } => {
                if let ItemSelector::Id(id) = item {
                    ids.push(id.clone());
                }
                ids.push(destination.target.clone());
            }
            Self::Use { point } => ids.push(point.target.clone()),
            Self::Routine { steps, looped: _ } => {
                for step in steps {
                    step.references_into(ids);
                }
            }
            Self::Move { point: _, speed: _ }
            | Self::Wait { seconds: _ }
            | Self::Investigate {
                point: _,
                seconds: _,
                radius: _,
            }
            | Self::Follow {
                target: _,
                distance: _,
                memory_seconds: _,
            }
            | Self::Patrol {
                points: _,
                speed: _,
                memory_seconds: _,
                track_player: _,
                decision_seconds: _,
            } => {}
        }
    }
    fn valid_at(&self, depth: u8) -> bool {
        if depth > 3 {
            return false;
        }
        match self {
            Self::Move { point, speed } => {
                position_valid(point) && speed.is_none_or(|v| bounded(v, 0.05, 8.0))
            }
            Self::Wait { seconds } => bounded(*seconds, 0.01, 600.0),
            Self::Investigate {
                point,
                seconds,
                radius,
            } => {
                position_valid(point)
                    && bounded(*seconds, 0.1, 120.0)
                    && bounded(*radius, 0.0, 20.0)
            }
            Self::Fetch {
                item,
                destination,
                search_radius,
            } => {
                (match item {
                    ItemSelector::Id(v) | ItemSelector::ObjectType(v) => name_valid(v),
                }) && destination.is_valid()
                    && bounded(*search_radius, 0.1, 100.0)
            }
            Self::Use { point } => point.is_valid(),
            Self::Follow {
                target,
                distance,
                memory_seconds,
            } => {
                name_valid(target)
                    && bounded(*distance, 0.5, 10.0)
                    && bounded(*memory_seconds, 0.1, 120.0)
            }
            Self::Routine { steps, looped: _ } => {
                !steps.is_empty()
                    && steps.len() <= 32
                    && steps.iter().all(|s| s.valid_at(depth.saturating_add(1)))
                    && steps
                        .iter()
                        .any(|s| !matches!(s,Self::Wait{seconds} if *seconds < 0.05))
            }
            Self::Patrol {
                points,
                speed,
                memory_seconds,
                track_player: _,
                decision_seconds,
            } => {
                !points.is_empty()
                    && points.len() <= 32
                    && points.iter().all(position_valid)
                    && bounded(*speed, 0.05, 8.0)
                    && bounded(*decision_seconds, 0.1, 5.0)
                    && bounded(*memory_seconds, 0.1, 120.0)
            }
        }
    }
}
impl TaskRequest {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        name_valid(&self.key) && self.priority <= 200 && self.definition.is_valid()
    }
}
impl SenseRule {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let filter = match &self.filter {
            SenseFilter::Hear { categories, radius } => {
                categories.len() <= 7 && bounded(*radius, 0.0, 100.0)
            }
            SenseFilter::See {
                player,
                entities,
                objects,
                object_types,
                range,
                fov_degrees,
            } => {
                (*player || *entities || !objects.is_empty() || !object_types.is_empty())
                    && objects.len() <= 32
                    && object_types.len() <= 32
                    && objects.iter().chain(object_types).all(|s| name_valid(s))
                    && bounded(*range, 0.0, 100.0)
                    && bounded(*fov_degrees, 0.0, 360.0)
            }
        };
        filter
            && match &self.response {
                SenseResponse::Investigate { seconds, radius } => {
                    bounded(*seconds, 0.1, 120.0) && bounded(*radius, 0.0, 20.0)
                }
                SenseResponse::Task { request } => request.is_valid(),
                SenseResponse::Alert { radius, hops } => bounded(*radius, 0.1, 50.0) && *hops <= 4,
            }
    }
}

/// Capability discovery runs only during package preparation/validation.
#[must_use]
pub fn level_requires_shared_ai(level: &crate::engine::level::LevelDef) -> bool {
    use crate::engine::level::{ActionDef, ComponentDef};
    if !level.scripts.is_empty() || !level.script_sources.is_empty() {
        return true;
    }
    let components = level.props.iter().flat_map(|prop| &prop.components).chain(
        level
            .spawn_templates
            .iter()
            .flat_map(|template| &template.components),
    );
    if components.into_iter().any(|component|matches!(component,ComponentDef::Ai(def) if !def.tasks.is_empty() || !def.senses.is_empty() || def.receive_alerts)){return true;}
    level
        .all_bindings()
        .iter()
        .flat_map(|bindings| bindings.iter())
        .flat_map(|binding| &binding.actions)
        .any(ActionDef::uses_shared_ai)
        || level
            .sequences
            .iter()
            .flat_map(|sequence| &sequence.steps)
            .flat_map(crate::engine::entities::sequences::SequenceStepDef::actions)
            .any(ActionDef::uses_shared_ai)
}
