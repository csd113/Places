//! Small deterministic guide policy, independent of rendering and path finding.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Guide,
    Wait,
    Talk,
    Cheer,
    Flee,
}

impl Mode {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "guide" => Some(Self::Guide),
            "wait" => Some(Self::Wait),
            "talk" => Some(Self::Talk),
            "cheer" => Some(Self::Cheer),
            "flee" => Some(Self::Flee),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    #[default]
    Greeting,
    Pointing,
    Leading,
    Waiting,
    AttentionJump,
    AttentionWave,
    Arrived,
    Talking,
    Cheering,
    Blocked,
}

impl Stage {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "greeting" => Some(Self::Greeting),
            "pointing" => Some(Self::Pointing),
            "leading" => Some(Self::Leading),
            "waiting" => Some(Self::Waiting),
            "attention_jump" => Some(Self::AttentionJump),
            "attention_wave" => Some(Self::AttentionWave),
            "arrived" => Some(Self::Arrived),
            "talking" => Some(Self::Talking),
            "cheering" => Some(Self::Cheering),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Greeting => "greeting",
            Self::Pointing => "pointing",
            Self::Leading => "leading",
            Self::Waiting => "waiting",
            Self::AttentionJump => "attention_jump",
            Self::AttentionWave => "attention_wave",
            Self::Arrived => "arrived",
            Self::Talking => "talking",
            Self::Cheering => "cheering",
            Self::Blocked => "blocked",
        }
    }
}

/// Squared feet distance preserves the strict 20 m boundary without a rounded
/// square root. A malformed observation never authorizes an attention action.
#[must_use]
pub fn attention_allowed(distance_squared: f32) -> bool {
    distance_squared.is_finite() && distance_squared > 400.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub mode: Mode,
    pub stage: Stage,
    pub elapsed: f32,
    pub cooldown: f32,
    pub begun: bool,
    pub waiting: bool,
}

impl Policy {
    #[must_use]
    pub const fn new(mode: Mode) -> Self {
        Self {
            mode,
            stage: Stage::Greeting,
            elapsed: 0.0,
            cooldown: 0.0,
            begun: false,
            waiting: false,
        }
    }

    pub fn enter(&mut self, stage: Stage) {
        if self.stage != stage {
            self.stage = stage;
            self.elapsed = 0.0;
        }
    }

    /// Retains the route continuation when a script temporarily requests a
    /// conversation or celebration. Only selecting a different route resets it.
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.enter(Stage::Waiting);
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.elapsed.is_finite()
            && (0.0..=10_000_000.0).contains(&self.elapsed)
            && self.cooldown.is_finite()
            && (0.0..=8.0).contains(&self.cooldown)
    }

    /// No simulation delta means no change, including an interrupted gesture.
    pub fn advance(&mut self, dt: f32, distance_squared: f32, arrived: bool) {
        if !dt.is_finite() || dt <= 0.0 || !distance_squared.is_finite() {
            return;
        }
        self.elapsed = (self.elapsed + dt.min(0.1)).min(10_000_000.0);
        self.cooldown = (self.cooldown - dt.min(0.1)).max(0.0);
        match self.mode {
            Mode::Wait => self.enter(Stage::Waiting),
            Mode::Talk => self.enter(Stage::Talking),
            Mode::Cheer => self.enter(Stage::Cheering),
            Mode::Guide => self.guide(distance_squared, arrived),
            Mode::Flee => self.enter(if arrived {
                Stage::Arrived
            } else {
                Stage::Leading
            }),
        }
    }

    fn guide(&mut self, distance_squared: f32, arrived: bool) {
        if arrived {
            self.enter(Stage::Arrived);
            return;
        }
        if self.begun {
            if distance_squared > 100.0 {
                self.waiting = true;
            } else if distance_squared <= 36.0 {
                self.waiting = false;
            }
        } else if distance_squared <= 6.25 {
            self.begun = true;
            self.enter(Stage::Pointing);
        }
        if !self.begun || self.waiting {
            self.attention(distance_squared);
        } else if self.stage != Stage::Pointing || self.elapsed >= 2.0 {
            self.enter(Stage::Leading);
        }
    }

    fn attention(&mut self, distance_squared: f32) {
        let resting = if self.begun {
            Stage::Waiting
        } else {
            Stage::Greeting
        };
        if !attention_allowed(distance_squared) {
            self.enter(resting);
            return;
        }
        match self.stage {
            Stage::AttentionJump if self.elapsed >= 1.8 => self.enter(Stage::AttentionWave),
            Stage::AttentionWave if self.elapsed >= 2.4 => {
                self.cooldown = 8.0;
                self.enter(resting);
            }
            Stage::AttentionJump | Stage::AttentionWave => {}
            Stage::Greeting
            | Stage::Pointing
            | Stage::Leading
            | Stage::Waiting
            | Stage::Arrived
            | Stage::Talking
            | Stage::Cheering
            | Stage::Blocked => {
                if self.cooldown <= 0.0 {
                    // An interrupted call still owns its cooldown; oscillating
                    // across the strict boundary cannot spam jumps or voices.
                    self.cooldown = 8.0;
                    self.enter(Stage::AttentionJump);
                } else {
                    self.enter(resting);
                }
            }
        }
    }
}
