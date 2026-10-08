//! Bounded native entity/movement/capture sequences, enabled only by the benchmark.

use std::collections::VecDeque;
use std::path::PathBuf;

use serde::Deserialize;

/// One action at a ready-world frame, through the ordinary runtime model API.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum LightingAction {
    Spawn {
        key: u64,
        model: String,
        #[serde(default)]
        character: bool,
        position: [f32; 3],
        yaw: f32,
        scale: f32,
    },
    Move {
        key: u64,
        position: [f32; 3],
        yaw: f32,
    },
    Despawn {
        key: u64,
    },
    ToggleFixture {
        fixture: usize,
        enabled: bool,
    },
    Door {
        id: String,
        open: bool,
    },
}

impl LightingAction {
    fn valid(&self) -> bool {
        match self {
            Self::Spawn {
                model,
                position,
                yaw,
                scale,
                key: _,
                character: _,
            } => {
                !model.is_empty()
                    && position.iter().all(|v| v.is_finite())
                    && yaw.is_finite()
                    && scale.is_finite()
                    && *scale > 0.0
            }
            Self::Move {
                position,
                yaw,
                key: _,
            } => position.iter().all(|v| v.is_finite()) && yaw.is_finite(),
            Self::Despawn { key: _ }
            | Self::ToggleFixture {
                fixture: _,
                enabled: _,
            } => true,
            Self::Door { id, open: _ } => !id.is_empty(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    frame: u64,
    #[serde(default)]
    actions: Vec<LightingAction>,
    capture: Option<PathBuf>,
}

/// Entries are consumed once; preparation frames cannot repeat a spawn or capture.
#[derive(Default)]
pub struct LightingSequence {
    actions: VecDeque<(u64, Vec<LightingAction>)>,
    captures: VecDeque<(u64, PathBuf)>,
}

impl LightingSequence {
    pub(super) fn from_env(enabled: bool) -> Self {
        if !enabled {
            return Self::default();
        }
        let Some(path) = std::env::var_os("PLACES_BENCH_LIGHTING_SEQUENCE") else {
            return Self::default();
        };
        match Self::read(&PathBuf::from(path)) {
            Ok(sequence) => sequence,
            Err(error) => {
                crate::logging::warn(format!("[lighting-sequence] {error}"));
                Self::default()
            }
        }
    }

    fn read(path: &std::path::Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > 1_048_576 {
            return Err("expected a regular JSON sequence of at most 1 MiB".to_string());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let steps: Vec<Step> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Self::from_steps(steps)
    }

    fn from_steps(steps: Vec<Step>) -> Result<Self, String> {
        if steps.len() > 4096 {
            return Err("lighting sequence exceeds 4096 steps".to_string());
        }
        let mut value = Self::default();
        let mut previous = 0;
        let mut paths = std::collections::HashSet::new();
        for step in steps {
            if step.frame <= previous
                || step.actions.len() > 64
                || !step.actions.iter().all(LightingAction::valid)
            {
                return Err(
                    "sequence frames must ascend and actions must be bounded and finite"
                        .to_string(),
                );
            }
            previous = step.frame;
            if let Some(path) = step.capture {
                if path.as_os_str().is_empty() || path.exists() || !paths.insert(path.clone()) {
                    return Err("sequence capture paths must be new and distinct".to_string());
                }
                value.captures.push_back((step.frame, path));
            }
            if !step.actions.is_empty() {
                value.actions.push_back((step.frame, step.actions));
            }
        }
        Ok(value)
    }

    pub(super) fn actions_at(&mut self, frame: u64) -> Vec<LightingAction> {
        if self.actions.front().is_some_and(|(due, _)| *due <= frame) {
            return self
                .actions
                .pop_front()
                .map_or_else(Vec::new, |(_, actions)| actions);
        }
        Vec::new()
    }

    pub(super) fn capture_at(&mut self, frame: u64) -> Option<PathBuf> {
        if self.captures.front().is_some_and(|(due, _)| *due <= frame) {
            return self.captures.pop_front().map(|(_, path)| path);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_cannot_repeat_actions_and_captures() -> Result<(), String> {
        let mut sequence = LightingSequence::from_steps(vec![Step {
            frame: 7,
            actions: vec![LightingAction::Despawn { key: 9 }],
            capture: Some(PathBuf::from("/tmp/places-sequence-consumption-test.png")),
        }])?;
        assert!(sequence.actions_at(6).is_empty(), "wait for ready frame");
        assert_eq!(sequence.actions_at(7).len(), 1, "one real action");
        assert!(
            sequence.actions_at(7).is_empty(),
            "same frame never repeats"
        );
        assert!(sequence.capture_at(7).is_some(), "one native capture");
        assert!(sequence.capture_at(7).is_none(), "capture is immutable");
        Ok(())
    }
}
