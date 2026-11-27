//! Editor-loop ownership of the consent, document epoch and ephemeral pipe.

use sdl3::video::Window;

use super::{
    SceneStamp,
    protocol::{self, Nonce, Reply},
    transport_windows::{self, Input, Pipe},
    visibility_windows::Monitor,
};

struct Grant {
    scope: SceneStamp,
    nonce: Nonce,
}

impl Grant {
    fn permits(&self, scene: Option<&SceneStamp>, verified: bool) -> bool {
        verified && scene.is_some_and(|live| live.same_epoch(&self.scope))
    }
}

struct Live {
    grant: Grant,
    pipe: Pipe,
}

#[derive(Default)]
pub(in crate::editor) struct Session {
    monitor: Option<Monitor>,
    live: Option<Live>,
    paused: bool,
    presented: bool,
}

impl Session {
    pub(in crate::editor) fn install(&mut self, window: &Window) {
        self.monitor = Monitor::install(window);
    }

    pub(in crate::editor) const fn label(&self) -> Option<&'static str> {
        if self.live.is_some() {
            Some("Take Over")
        } else if self.monitor.is_none() {
            None
        } else if self.paused {
            Some("Resume Agent")
        } else {
            Some("Agent Off")
        }
    }

    pub(in crate::editor) const fn active(&self) -> bool {
        self.live.is_some()
    }

    pub(in crate::editor) fn pause(&mut self) -> bool {
        if self.live.take().is_some() {
            self.paused = true;
            true
        } else {
            false
        }
    }

    pub(in crate::editor) fn retain_scope(&mut self, scope: Option<&SceneStamp>) -> bool {
        if self
            .live
            .as_ref()
            .is_some_and(|live| !live.grant.permits(scope, true))
        {
            return self.pause();
        }
        false
    }

    pub(in crate::editor) fn enable(&mut self, scene: &SceneStamp) -> Result<String, String> {
        let monitor = self
            .monitor
            .as_ref()
            .ok_or("Agent sessions are unavailable in this editor")?;
        let _previous = Monitor::interrupted();
        if !self.presented || !monitor.verified() {
            return Err(
                "Agent needs an uncovered editor on the active, unlocked local display".into(),
            );
        }
        let nonce = transport_windows::nonce().map_err(|error| error.to_string())?;
        let pipe = Pipe::open(nonce, monitor.session())
            .map_err(|error| format!("Cannot open private editor session: {error}"))?;
        if Monitor::interrupted() || !monitor.verified() {
            return Err("Editor visibility changed before the session could start".into());
        }
        self.live = Some(Live {
            grant: Grant {
                scope: scene.clone(),
                nonce,
            },
            pipe,
        });
        self.paused = false;
        Ok(format!(
            "places-editor --agent status {}",
            protocol::encode(nonce)
        ))
    }

    /// At most one fixed-size request per presented frame, with fresh OS and
    /// grant checks before admission and again before releasing the reply.
    pub(in crate::editor) fn tick(&mut self, scope: Option<&SceneStamp>, drawn: bool) -> Tick {
        self.presented = drawn;
        if Monitor::refresh_pending() {
            // Close the pipe before registering for the new display state.
            // A positive initial callback never creates or restores a grant.
            let revoked = self.pause();
            if let Some(monitor) = &mut self.monitor {
                monitor.refresh_power();
            }
            return if revoked { Tick::Paused } else { Tick::Idle };
        }
        if !self.active() {
            return Tick::Idle;
        }
        if !self.admitted(scope) {
            let _revoked = self.pause();
            return Tick::Paused;
        }
        let dispatch = self.live.as_mut().and_then(|live| match live.pipe.poll() {
            Some(Input::Message(bytes)) => Some(protocol::dispatch(&bytes, live.grant.nonce)),
            Some(Input::Malformed) => Some(Reply::BadMessage),
            None => None,
        });
        let Some(reply) = dispatch else {
            return Tick::Idle;
        };
        if !self.admitted(scope) {
            let _revoked = self.pause();
            return Tick::Paused;
        }
        if self
            .live
            .as_mut()
            .is_some_and(|live| live.pipe.reply(reply))
            && reply == Reply::Ready
        {
            Tick::Read
        } else {
            Tick::Idle
        }
    }

    fn admitted(&self, scope: Option<&SceneStamp>) -> bool {
        let verified = self.presented
            && !Monitor::interrupted()
            && self.monitor.as_ref().is_some_and(Monitor::verified);
        self.live
            .as_ref()
            .is_some_and(|live| live.grant.permits(scope, verified))
    }
}

#[derive(Clone, Copy)]
pub(in crate::editor) enum Tick {
    Idle,
    Paused,
    Read,
}

pub(in crate::editor) fn client(arguments: &[String]) -> Result<&'static str, String> {
    let [operation, session] = arguments else {
        return Err("Usage: places-editor --agent status SESSION".into());
    };
    if operation != "status" {
        return Err("Only the read-only status command is available".into());
    }
    let nonce = protocol::decode(session).ok_or("Invalid editor session code")?;
    transport_windows::probe(nonce)
}

#[cfg(test)]
#[path = "../tests/agent_session.rs"]
mod tests;
