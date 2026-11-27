//! In-process editor command foundation. No dispatch endpoint or writer lives here.

mod pilot;
mod scene;

pub(super) use pilot::{ConnectEscalator, PilotCommand, RailPath, ReplaceFixture};
pub(super) use scene::SceneStamp;
