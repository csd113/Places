//! Editor-loop ownership of the consent, document epoch and ephemeral pipe.

use sdl3::video::Window;

use super::{
    SceneStamp, batch,
    protocol::{self, Nonce, Reply},
    transport_windows::{self, Input, Pipe},
    visibility_windows::{Monitor, VisibilityDenial},
    wire::{self, Call},
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
    pub(super) pilot: batch::State,
    pub(super) refresh: Option<SceneStamp>,
}

impl Session {
    #[cfg(test)]
    pub(super) const fn monitor_for_qa(&self) -> Option<&Monitor> {
        self.monitor.as_ref()
    }

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
            self.pilot = batch::State::default();
            self.refresh = None;
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
        Monitor::acknowledge_interruptions();
        if !self.presented {
            return Err("Agent needs a presented editor frame".into());
        }
        monitor.verified().map_err(VisibilityDenial::status)?;
        let nonce = transport_windows::nonce().map_err(|error| error.to_string())?;
        let pipe = Pipe::open(nonce, monitor.session())
            .map_err(|error| format!("Cannot open private editor session: {error}"))?;

        monitor.verified().map_err(VisibilityDenial::status)?;
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
    #[cfg(test)]
    fn tick(&mut self, scope: Option<&SceneStamp>, drawn: bool) -> Tick {
        self.poll(scope, drawn).0
    }

    pub(super) fn poll(&mut self, scope: Option<&SceneStamp>, drawn: bool) -> (Tick, Option<Call>) {
        self.presented = drawn;
        if Monitor::refresh_pending() {
            // Close the pipe before registering for the new display state.
            // A positive initial callback never creates or restores a grant.
            let revoked = self.pause();
            if let Some(monitor) = &mut self.monitor {
                monitor.refresh_power();
            }
            return (if revoked { Tick::Paused } else { Tick::Idle }, None);
        }
        if !self.active() {
            return (Tick::Idle, None);
        }
        if !self.admitted(scope) {
            let _revoked = self.pause();
            return (Tick::Paused, None);
        }
        let Some(live) = self.live.as_mut() else {
            return (Tick::Idle, None);
        };
        let reply = match live.pipe.poll() {
            Some(Input::Message(bytes)) => match protocol::pilot_call(&bytes, live.grant.nonce) {
                Some(Ok(call)) => return (Tick::Idle, Some(call)),
                Some(Err(error)) => error,
                None => protocol::dispatch(&bytes, live.grant.nonce),
            },
            Some(Input::Malformed) => Reply::BadMessage,
            None => return (Tick::Idle, None),
        };
        if !self.admitted(scope) {
            let _revoked = self.pause();
            return (Tick::Paused, None);
        }
        if self
            .live
            .as_mut()
            .is_some_and(|session| session.pipe.reply(reply))
            && reply == Reply::Ready
        {
            (Tick::Read, None)
        } else {
            (Tick::Idle, None)
        }
    }

    pub(super) fn reply_pilot(
        &mut self,
        reply: &serde_json::Value,
        scope: Option<&SceneStamp>,
    ) -> bool {
        if !self.admitted(scope) {
            let _revoked = self.pause();
            return false;
        }
        self.live
            .as_mut()
            .is_some_and(|live| match protocol::pilot_reply(reply) {
                Ok(packet) => live.pipe.reply_packet(&packet),
                Err(_) => live.pipe.reply(Reply::BadMessage),
            })
    }

    pub(super) fn pilot_binding(&self) -> Option<Nonce> {
        self.live.as_ref().map(|live| live.grant.nonce)
    }

    pub(super) fn admitted(&self, scope: Option<&SceneStamp>) -> bool {
        let verified = self.presented
            && self
                .monitor
                .as_ref()
                .is_some_and(|monitor| monitor.verified().is_ok());
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

pub(in crate::editor) fn client(arguments: &[String]) -> Result<String, String> {
    let nonce = |session| protocol::decode(session).ok_or("Invalid editor session code");
    match arguments {
        [operation, session] if operation == "status" => {
            transport_windows::probe(nonce(session)?).map(str::to_owned)
        }
        [operation, session, payload] if operation == "call" => {
            if payload.len() > wire::MAX_PACKET.saturating_sub(protocol::REQUEST_LEN) {
                return Err("Pilot request is too large".into());
            }
            let call = serde_json::from_str::<Call>(payload)
                .map_err(|error| format!("Invalid pilot request: {error}"))?;
            transport_windows::call(
                nonce(session)?,
                payload.as_bytes(),
                matches!(
                    call,
                    Call::Apply {
                        request_id: _,
                        digest: _
                    }
                ),
            )
        }
        _ => Err("Usage: places-editor --agent status SESSION | --agent call SESSION JSON".into()),
    }
}

#[cfg(test)]
#[path = "../tests/agent_session.rs"]
mod tests;
