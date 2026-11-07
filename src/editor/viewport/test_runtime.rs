//! Transient runtime ownership beside the existing renderer and preparation worker.
use std::time::Instant;

use super::{LocalPreview, Viewport};
use crate::editor::{test_prepare::PreparedTest, test_session::Session};
use crate::engine::{audio::AudioSystem, level::LevelDef};
use crate::game::play_spawn::Pose;

#[derive(Default)]
pub(super) struct Testing {
    pub(super) requested: Option<Instant>,
    pub(super) pose: Option<Pose>,
    pub(super) pending: Option<Box<PreparedTest>>,
    pub(super) session: Option<Box<Session>>,
    pub(super) failure: Option<String>,
    pub(super) metrics: serde_json::Value,
    progress: String,
    trace: Option<String>,
}

impl Viewport {
    pub fn start_test(&mut self, level: &LevelDef, pose: Option<Pose>) {
        self.cancel_navigation();
        self.worker.cancel();
        self.renderer.cancel_prepared_install();
        self.uploading = false;
        self.pending_objects = None;
        self.frame_next = false;
        self.local_preview = LocalPreview::Inactive;
        self.testing = Testing {
            requested: Some(Instant::now()),
            progress: "Preparing private gameplay snapshot / no lighting bake".into(),
            pose,
            ..Testing::default()
        };
        self.worker.request_test(level);
    }

    pub const fn runtime_active(&self) -> bool {
        self.testing.session.is_some()
    }
    pub fn runtime_cinematic(&self) -> bool {
        self.testing
            .session
            .as_deref()
            .is_some_and(crate::editor::test_session::Session::cinematic_controls_blocked)
    }
    pub const fn runtime_preparing(&self) -> bool {
        self.testing.requested.is_some() && self.testing.session.is_none()
    }
    pub fn runtime_status(&self) -> Option<&str> {
        self.testing
            .failure
            .as_deref()
            .or_else(|| {
                self.testing
                    .session
                    .as_ref()
                    .map(|session| session.status())
            })
            .or_else(|| {
                self.runtime_preparing()
                    .then_some(self.testing.progress.as_str())
            })
    }
    pub fn runtime_message(&self) -> Option<crate::editor::test_session::MessageView<'_>> {
        self.testing
            .session
            .as_ref()
            .and_then(|session| session.message_view())
    }
    pub fn runtime_warning(&self) -> Option<&str> {
        self.testing
            .session
            .as_ref()
            .map(|session| session.warning())
            .or_else(|| {
                self.testing
                    .pending
                    .as_ref()
                    .map(|prepared| prepared.warning.as_str())
            })
    }
    pub fn runtime_trace(&self) -> Option<&str> {
        self.testing.trace.as_deref()
    }
    pub fn runtime_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "active":self.runtime_active(), "preparing":self.runtime_preparing(),
            "status":self.runtime_status(), "metrics":self.testing.metrics,
            "progress":self.worker.progress.label(),
            "session":self.testing.session.as_ref().map(|session|session.snapshot()),
            "policy":"private unsaved snapshot; same editor window; no package, profile or lighting bake"
        })
    }
    pub fn runtime_event(&mut self, event: &sdl3::event::Event) {
        if let Some(session) = &mut self.testing.session {
            session.event(event);
        }
    }
    pub const fn take_runtime_failure(&mut self) -> Option<String> {
        self.testing.failure.take()
    }

    pub(super) fn advance_test(&mut self, delta: f32, audio: &mut AudioSystem) {
        if self.runtime_preparing() {
            self.testing.progress = self.worker.progress.label();
            if let Some(error) = self.worker.error() {
                self.testing.failure = Some(error.into());
                return;
            }
            if !self.uploading
                && let Some(prepared) = self.testing.pending.take()
            {
                match Session::new(
                    *prepared,
                    self.testing.pose.take(),
                    &mut self.renderer,
                    audio,
                ) {
                    Ok(session) => {
                        let startup_ms = self
                            .testing
                            .requested
                            .take()
                            .map(|time| time.elapsed().as_secs_f64() * 1_000.0_f64);
                        self.testing.metrics = serde_json::json!({"startup_ms":startup_ms});
                        self.testing.session = Some(Box::new(session));
                    }
                    Err(error) => self.testing.failure = Some(error),
                }
            }
        }
        if let Some(session) = &mut self.testing.session
            && let Err(error) = session.update(delta, &mut self.renderer, audio)
        {
            self.testing.failure = Some(error);
        }
    }

    pub fn stop_test(&mut self, audio: &mut AudioSystem) -> bool {
        if !self.runtime_active() && !self.runtime_preparing() {
            return false;
        }
        let started = Instant::now();
        let snapshot = self.testing.session.take().map(|mut session| {
            self.testing.trace = Some(session.trace());
            session.stop(audio);
            session.snapshot()
        });
        audio.clear_world();
        self.testing.pending = None;
        self.testing.requested = None;
        self.testing.pose = None;
        self.testing.failure = None;
        self.worker.cancel();
        self.renderer.cancel_prepared_install();
        self.uploading = false;
        self.pending_objects = None;
        self.renderer.unload_level();
        let _cleared = self.renderer.set_flashlight(None);
        self.renderer.set_editor_ui();
        self.cancel_navigation();
        let startup = self.testing.metrics.get("startup_ms").cloned();
        self.testing.metrics = serde_json::json!({"startup_ms":startup,"stop_handler_ms":started.elapsed().as_secs_f64()*1_000.0_f64,"last_session":snapshot});
        true
    }
}
