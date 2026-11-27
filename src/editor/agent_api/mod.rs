//! Editor-owned commands and the visible Windows agent session.
//! Only the bounded, reversible pilot vocabulary is exposed.

mod pilot;
mod scene;

#[cfg(windows)]
mod batch;
#[cfg(windows)]
mod wire;

#[cfg(windows)]
mod protocol;
#[cfg(windows)]
mod session_windows;
#[cfg(windows)]
mod transport_windows;
#[cfg(windows)]
mod visibility_windows;

pub(super) use pilot::{ConnectEscalator, PilotCommand, RailPath, ReplaceFixture};
pub(super) use scene::SceneStamp;

#[cfg(windows)]
pub(super) use session_windows::Session;

#[cfg(not(windows))]
#[derive(Default)]
pub(super) struct Session(Option<&'static str>);

#[cfg(not(windows))]
impl Session {
    pub(super) const fn label(&self) -> Option<&'static str> {
        self.0
    }
}

use super::App;
use sdl3::{
    event::{Event, WindowEvent},
    video::Window,
};

pub(super) fn client(arguments: &[String]) -> Result<String, String> {
    #[cfg(windows)]
    {
        session_windows::client(arguments)
    }
    #[cfg(not(windows))]
    {
        let _arguments = arguments;
        Err("Agent transport is unavailable on this platform".into())
    }
}

impl App {
    pub(super) fn install_agent(&mut self, window: &Window) {
        #[cfg(windows)]
        self.agent.install(window);
        #[cfg(not(windows))]
        let _unsupported = (&self.agent, window);
    }

    pub(super) fn agent_toggle(&mut self, window: &Window) -> Result<(), String> {
        #[cfg(windows)]
        {
            if self.agent.pause() {
                self.status =
                    "Agent paused. Old session revoked. Use Resume Agent to grant access again."
                        .into();
                return Ok(());
            }
            let scope = self
                .agent_scope()
                .ok_or("Open an idle project and close editor tools before enabling the agent")?
                .clone();
            let command = self.agent.enable(&scope)?;
            if !Self::copy_agent_command(
                #[cfg(test)]
                &mut self.agent_clipboard_capture,
                window,
                &command,
            ) {
                let _revoked = self.agent.pause();
                return Err("Could not copy the session command. Agent remains paused.".into());
            }
            self.status = "Pilot edits enabled: railings, escalators, light fixtures. Session copied. Take Over revokes access.".into();
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _unsupported = (&self.agent, window);
            Err("Agent sessions are unavailable on this platform".into())
        }
    }

    #[cfg(windows)]
    fn copy_agent_command(
        #[cfg(test)] output: &mut Option<String>,
        window: &Window,
        command: &str,
    ) -> bool {
        #[cfg(test)]
        if let Some(capture) = output {
            command.clone_into(capture);
            return true;
        }
        window
            .subsystem()
            .clipboard()
            .set_clipboard_text(command)
            .is_ok()
    }

    pub(super) fn agent_before_event(&mut self, event: &Event, window: &Window) {
        if matches!(
            event,
            Event::Window { window_id, win_event: WindowEvent::Hidden | WindowEvent::Minimized | WindowEvent::Occluded | WindowEvent::DisplayChanged(_), timestamp: _ }
                if *window_id == window.id()
        ) {
            self.pause_agent();
        }
    }

    pub(super) fn pause_agent(&mut self) {
        #[cfg(windows)]
        if self.agent.pause() {
            self.status =
                "Agent paused. Old session revoked. Resume only in a visible editor.".into();
        }
    }

    pub(super) fn agent_after_event(&mut self) {
        #[cfg(not(windows))]
        let _unsupported = &self.agent;
        #[cfg(windows)]
        {
            if !self.agent.active() {
                return;
            }
            let scope = self.agent_scope().cloned();
            if self.agent.retain_scope(scope.as_ref()) {
                self.status = "Agent paused because the editor or project changed. Resume to grant access again.".into();
            } else {
                let mut pilot = std::mem::take(&mut self.agent.pilot);
                pilot.observe(self);
                self.agent.pilot = pilot;
            }
        }
    }

    pub(super) fn agent_tick(&mut self, presented: bool) {
        #[cfg(windows)]
        {
            use session_windows::Tick;
            let scope = self
                .agent
                .active()
                .then(|| self.agent_scope().cloned())
                .flatten();
            if self.viewport.document_scene_ready() {
                self.agent.refresh = None;
            }
            let (tick, pending) = self.agent.poll(scope.as_ref(), presented);
            match tick {
                Tick::Idle => {}
                Tick::Paused => {
                    self.status =
                        "Agent paused. Visibility or access changed. Use Resume Agent.".into();
                }
                Tick::Read => {
                    self.status = "Agent read session status. Take Over revokes access.".into();
                }
            }
            if let (Some(call), Some(binding)) = (pending, self.agent.pilot_binding()) {
                if !self.agent.admitted(scope.as_ref()) {
                    self.pause_agent();
                    return;
                }
                let mut pilot = std::mem::take(&mut self.agent.pilot);
                let reply = pilot.dispatch(self, call, binding);
                self.agent.pilot = pilot;
                let reply_scope = self.agent_scope().cloned();
                let _sent = self.agent.reply_pilot(&reply, reply_scope.as_ref());
                if self.agent.active() {
                    let outcome = reply
                        .get("outcome")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("rejected");
                    self.status = format!("Agent request {outcome}. Take Over revokes access.");
                }
            }
        }
        #[cfg(not(windows))]
        let _unsupported = (&self.agent, presented);
    }

    #[cfg(windows)]
    fn agent_scope(&self) -> Option<&SceneStamp> {
        if self.closing
            || self.dialog_open
            || self.popup.is_some()
            || self.runtime_active()
            || self.runtime_preparing()
            || self.compile.is_some()
            || self.play.is_some()
            || (self.viewport.is_preparing()
                && !self.agent.refresh.as_ref().is_some_and(|refresh| {
                    self.document
                        .as_ref()
                        .is_some_and(|doc| doc.scene_stamp().matches(refresh))
                }))
            || self.viewport.local_progress().is_some()
            || self.placement.is_some()
            || self.translation.is_some()
            || self.wall_drag.is_some()
            || self.escalator.is_some()
            || self.elevators.mode.is_some()
            || self.lighting.painting
            || self.navigation.pick_goal.is_some()
            || self.play_floor_pick
        {
            return None;
        }
        self.document.as_ref().map(super::Document::scene_stamp)
    }
}
