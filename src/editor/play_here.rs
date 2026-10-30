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
        // Capture before save/export; failures cannot leave an old launch request.
        if self.compile.is_some() || self.play.is_some() {
            return Err("A build or player is already running".into());
        }
        self.pending_play = None;
        let intent = Intent::capture(self.document.as_ref().ok_or("Open a project first")?, pose);
        self.start_build(true)?;
        self.pending_play = Some(intent);
        self.popup = None;
        Ok(())
    }
}
pub(super) fn draw(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(v, "Spawn and test here", 140.0, 76.0, ui::INK, 60);
    ui::text(
        v,
        "Keeps the map's saved player spawn.",
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
    ui::button(
        v,
        hits,
        [140.0, 187.0, 80.0, 13.0],
        "Cancel",
        Action::ClosePopup,
        true,
    );
}
