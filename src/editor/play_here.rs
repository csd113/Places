//! Ephemeral Play intent; document bytes and history are never used to store a pose.
use super::{
    App, Document,
    ui::{self, Action, Hit},
};
use crate::engine::render::Vertex;
use crate::game::{CollisionWorld, Game, play_spawn::Pose};
use std::path::PathBuf;

pub(super) struct Intent {
    pub root: PathBuf,
    pub level: String,
    pub pose: Option<Pose>,
}
impl Intent {
    pub fn capture(doc: &Document, pose: Option<Pose>) -> Self {
        Self {
            root: doc.root().into(),
            level: doc.level_id().into(),
            pose,
        }
    }
    pub fn matches(&self, doc: &Document) -> bool {
        self.root == doc.root() && self.level == doc.level_id()
    }
}
impl App {
    fn play_world(&self) -> Result<Game, String> {
        let mut level = self
            .document
            .as_ref()
            .ok_or("Open a project first")?
            .level()
            .clone();
        crate::map_compiler::prepare_level(&mut level, &self.catalog, None);
        // Before native item preparation, conservatively reserve authored movable props' bounds.
        for prop in &mut level.props {
            if prop.is_physics_movable() {
                prop.physics = None;
                prop.solid = true;
            }
        }
        Ok(Game::new(
            crate::game::spawn_position(&level),
            level.spawn.yaw_degrees.to_radians(),
            CollisionWorld::from_level(&level),
        ))
    }
    pub(super) fn play_camera(&mut self) -> Result<(), String> {
        self.play_floor_pick = false;
        let pose = self
            .play_world()?
            .validate_play_pose(self.viewport.play_camera()?)?;
        self.start_play(Some(pose))
    }
    pub(super) fn play_floor_click(&mut self, point: [f32; 2]) -> Result<(), String> {
        let floor = self.viewport.play_floor(point)?;
        let camera = self.viewport.play_camera()?;
        let pose = self
            .play_world()?
            .play_pose_on_floor(floor, camera.yaw, camera.pitch)?;
        self.play_floor_pick = false;
        self.start_play(Some(pose))
    }
    pub(super) fn start_play(&mut self, pose: Option<Pose>) -> Result<(), String> {
        if self.compile.is_some()
            || self.play.is_some()
            || self.runtime_active()
            || self.runtime_preparing()
        {
            return Err("A build or test is already running".into());
        }
        if self.navigation.is_baking() || self.safety_job.is_some() {
            return Err("Finish the current navigation or safety check before testing.".into());
        }
        let document = self.document.as_ref().ok_or("Open a project first")?;
        let intent = Intent::capture(document, pose);
        let mut level = document.level().clone();
        document.project().attach_scripts(&mut level)?;
        for source in &mut level.script_sources {
            if let Some(text) = document.script_text(source.path.as_str()) {
                text.clone_into(&mut source.source);
            }
        }
        self.pending_play = None;
        self.play_floor_pick = false;
        self.preview_audio.clear_world();
        self.sound_preview_until = None;
        self.viewport.start_test(&level, intent.pose);
        self.popup = None;
        self.ui_focus = None;
        self.status = "Preparing first-person test in this editor / no lighting bake".into();
        Ok(())
    }
}
pub(super) fn draw(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(v, "Test here in the editor", 140.0, 76.0, ui::INK, 60);
    ui::text(
        v,
        "Temporary pose; saved spawn is unchanged.",
        140.0,
        92.0,
        ui::INK,
        65,
    );
    ui::button(
        v,
        hits,
        [140.0, 108.0, 235.0, 15.0],
        "From camera / Ctrl Shift P",
        Action::PlayCamera,
        true,
    );
    ui::button(
        v,
        hits,
        [140.0, 130.0, 235.0, 15.0],
        "Choose a floor in the scene",
        Action::PlayFloor,
        true,
    );
    ui::text(
        v,
        "Camera: standing height, clear body space.",
        140.0,
        153.0,
        ui::INK,
        65,
    );
    ui::text(
        v,
        "Floor: click visible surface; Esc cancels.",
        140.0,
        166.0,
        ui::INK,
        65,
    );
    for (y, line) in [
        (181.0, "Quick test: approximate light; no lightmap bake."),
        (194.0, "Final package: travel, checkpoints, player forms,"),
        (207.0, "ragdolls, mannequin games and actor carry/use."),
        (220.0, "Automatic ambient/footstep/door audio is omitted."),
    ] {
        ui::text(v, line, 140.0, y, ui::MUTED, 60);
    }
    ui::button(
        v,
        hits,
        [140.0, 237.0, 80.0, 13.0],
        "Cancel",
        Action::ClosePopup,
        true,
    );
}

impl App {
    pub(super) const fn runtime_active(&self) -> bool {
        self.viewport.runtime_active()
    }
    pub(super) const fn runtime_preparing(&self) -> bool {
        self.viewport.runtime_preparing()
    }
    pub(super) fn runtime_message(&self) -> Option<super::test_session::MessageView<'_>> {
        self.viewport.runtime_message()
    }
    pub(super) fn runtime_warning(&self) -> Option<&str> {
        self.viewport.runtime_warning()
    }
    pub(super) fn runtime_status(&self) -> Option<&str> {
        self.viewport.runtime_status()
    }
    pub(super) fn stop_runtime(&mut self) {
        if self.viewport.stop_test(&mut self.preview_audio) {
            if let Some(document) = &self.document {
                self.viewport.rebuild(document.level());
            }
            self.ui_focus = None;
            self.status =
                "Returned to editing. Test changes discarded; unsaved edits retained.".into();
        }
    }
    pub(super) fn poll_runtime(&mut self) {
        if let Some(error) = self.viewport.take_runtime_failure().or_else(|| {
            (self.runtime_preparing() || self.runtime_active())
                .then(|| self.viewport.status().map(str::to_owned))
                .flatten()
        }) {
            self.stop_runtime();
            self.status = format!("Test stopped: {error}");
            self.script_feedback.clone_from(&self.status);
        }
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Only Escape, focus loss and visible test controls have editor meaning during a runtime session."
    )]
    pub(super) fn runtime_event(
        &mut self,
        event: &sdl3::event::Event,
        window: &sdl3::video::Window,
        hits: &[Hit],
    ) -> bool {
        use sdl3::{
            event::{Event, WindowEvent},
            keyboard::Keycode,
            mouse::MouseButton,
        };
        if !self.runtime_active() && !self.runtime_preparing() {
            return false;
        }
        match event {
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                ..
            }
            | Event::Window {
                win_event: WindowEvent::FocusLost | WindowEvent::Hidden | WindowEvent::Minimized,
                ..
            } => self.stop_runtime(),
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(point) = super::input::ui_point(window, [*x, *y])
                    && hits.iter().any(|hit| {
                        hit.contains(point) && matches!(hit.action, Action::Stop | Action::Cancel)
                    })
                {
                    self.stop_runtime();
                } else {
                    self.viewport.runtime_event(event);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::F12),
                repeat: false,
                ..
            } => self.request_capture(),
            _ => self.viewport.runtime_event(event),
        }
        true
    }
}
