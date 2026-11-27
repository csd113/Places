//! The complete API-03 wire vocabulary. No paths or untyped operations cross it.

use serde::{Deserialize, Serialize};

pub(super) const MAX_PACKET: usize = 4096;
pub(super) const MAX_REPLY: usize = 8192;
pub(super) const MAX_ACTIONS: usize = 3;
pub(super) const MAX_POINTS: usize = 8;
pub(super) const MAX_REQUESTS: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Call {
    Inspect {
        #[serde(default)]
        samples: Vec<[f32; 2]>,
    },
    Fixtures {
        #[serde(default)]
        after: Option<String>,
    },
    Preview {
        request_id: u32,
        context: u64,
        actions: Vec<Action>,
    },
    Apply {
        request_id: u32,
        digest: String,
    },
    Cancel {
        request_id: u32,
        digest: String,
    },
    Receipt {
        request_id: u32,
        digest: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "capability", deny_unknown_fields)]
pub(super) enum Action {
    #[serde(rename = "railing.place_path@1")]
    Railing { points: Vec<[f32; 2]> },
    #[serde(rename = "escalator.connect_landings@1")]
    Escalator { lower: [f32; 2], upper: [f32; 2] },
    #[serde(rename = "light.replace_fixture@1")]
    Fixture { light: u32, asset_id: String },
}

impl Action {
    pub(super) fn bounded(&self) -> bool {
        let valid = |point: &[f32; 2]| {
            point
                .iter()
                .all(|x| x.is_finite() && (0.0..1.0).contains(x))
        };
        match self {
            Self::Railing { points } => {
                (2..=MAX_POINTS).contains(&points.len()) && points.iter().all(valid)
            }
            Self::Escalator { lower, upper } => valid(lower) && valid(upper),
            Self::Fixture { asset_id, light: _ } => bounded_id(asset_id),
        }
    }
}

pub(super) fn bounded_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'_' | b'-' | b':'))
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Code {
    InvalidInput,
    StaleContext,
    NotReady,
    WrongSelection,
    LockedTarget,
    UnsupportedFloor,
    Rejected,
    IdConflict,
    Limit,
    UnknownRequest,
    GrantLost,
}

#[derive(Clone, Serialize)]
pub(super) struct Failure {
    pub code: Code,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<usize>,
}

const MAX_MESSAGE_CHARS: usize = 240;

pub(super) fn truncate_message(message: &mut String) {
    if let Some((index, _)) = message.char_indices().nth(MAX_MESSAGE_CHARS) {
        message.truncate(index);
    }
}

impl Failure {
    pub(super) fn new(code: Code, message: &str) -> Self {
        Self {
            code,
            message: message.chars().take(MAX_MESSAGE_CHARS).collect(),
            action: None,
        }
    }

    pub(super) const fn at(mut self, index: usize) -> Self {
        self.action = Some(index);
        self
    }
}
