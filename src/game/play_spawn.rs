//! Runtime-only editor launch poses. No level or checkpoint serialization.
use super::{EYE_HEIGHT, Game, MAX_PITCH};
use crate::engine::collision::{
    CONTACT_EPS, PLAYER_HEIGHT, PLAYER_RADIUS, PLAYER_STEP_HEIGHT, STEP_EPS,
};
use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}
impl Pose {
    pub fn new(eye: Vec3, yaw: f32, pitch: f32) -> Result<Self, String> {
        if !eye.is_finite() || !yaw.is_finite() || !pitch.is_finite() {
            return Err("Play here requires a finite position and direction".into());
        }
        Ok(Self {
            eye,
            yaw: yaw.rem_euclid(std::f32::consts::TAU),
            pitch: pitch.clamp(-MAX_PITCH, MAX_PITCH),
        })
    }
    pub fn encode(self) -> String {
        format!(
            "{},{},{},{},{}",
            self.eye.x,
            self.eye.y,
            self.eye.z,
            self.yaw.to_degrees(),
            self.pitch.to_degrees()
        )
    }
    pub fn parse(value: &str) -> Option<Self> {
        let values = value
            .split(',')
            .map(|part| part.trim().parse::<f32>().ok())
            .collect::<Option<Vec<_>>>()?;
        let [x, y, z, yaw, pitch] = values.as_slice() else {
            return None;
        };
        Self::new(Vec3::new(*x, *y, *z), yaw.to_radians(), pitch.to_radians()).ok()
    }
}
impl Game {
    /// Requires the actual camera eye to be near standing height on this storey.
    /// Never substitutes a world plane or searches for a different horizontal point.
    pub(crate) fn validate_play_pose(&self, pose: Pose) -> Result<Pose, String> {
        let checked = Pose::new(pose.eye, pose.yaw, pose.pitch)?;
        let feet = Vec3::new(checked.eye.x, checked.eye.y - EYE_HEIGHT, checked.eye.z);
        let floor = self
            .floor
            .height_below(feet.x, feet.z, feet.y + STEP_EPS)
            .ok_or("Play here: no walkable floor under this position on this storey")?;
        if feet.y - floor > PLAYER_STEP_HEIGHT + STEP_EPS {
            return Err("Play here: move the camera to standing height, or choose a floor".into());
        }
        if !self.body_fits_for(feet, PLAYER_HEIGHT, PLAYER_RADIUS) {
            return Err(
                "Play here: a standing player overlaps a wall, ceiling, door or prop".into(),
            );
        }
        Ok(checked)
    }
    pub(crate) fn play_pose_on_floor(
        &self,
        floor: Vec3,
        yaw: f32,
        pitch: f32,
    ) -> Result<Pose, String> {
        if !floor.is_finite() {
            return Err("Play here: invalid floor hit".into());
        }
        let center = self
            .floor
            .height_below(floor.x, floor.z, floor.y + STEP_EPS)
            .filter(|height| (*height - floor.y).abs() <= STEP_EPS)
            .ok_or("Play here: click an actual walkable floor")?;
        let height = self
            .floor
            .hull_support(
                Vec2::new(floor.x, floor.z),
                PLAYER_RADIUS - CONTACT_EPS,
                center + PLAYER_STEP_HEIGHT + STEP_EPS,
            )
            .map_or(center, |support| support.0);
        self.validate_play_pose(Pose::new(
            Vec3::new(floor.x, height + EYE_HEIGHT, floor.z),
            yaw,
            pitch,
        )?)
    }
    pub(crate) fn apply_play_pose(&mut self, pose: Pose) -> Result<(), String> {
        let checked = self.validate_play_pose(pose)?;
        self.reset_spawn_point(checked.eye, checked.yaw);
        self.player_pitch = checked.pitch;
        Ok(())
    }
}
