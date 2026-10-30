//! Editor camera policy; projection remains the engine camera's responsibility.

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::engine::render::{DrawableSize, RenderCamera};
use crate::engine::spatial::Aabb;

pub(super) struct Camera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::new(3.0, 0.5, 3.0),
            yaw: -0.65,
            pitch: -0.8,
            distance: 13.0,
        }
    }
}

impl Camera {
    /// Explicit native acceptance viewpoint, through the same orbit camera.
    pub fn set_pose(&mut self, pose: crate::game::play_spawn::Pose) {
        self.yaw = pose.yaw;
        self.pitch = pose.pitch;
        self.target = self.forward().mul_add(Vec3::splat(self.distance), pose.eye);
    }

    fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Camera vector operations contain only bounded floating point components."
    )]
    pub fn render_camera(&self) -> RenderCamera {
        RenderCamera::new(
            self.target - self.forward() * self.distance,
            self.yaw,
            self.pitch,
            60.0,
        )
    }

    pub fn orbit(&mut self, delta: [f32; 2]) {
        self.yaw = delta[0]
            .mul_add(0.006, self.yaw)
            .rem_euclid(std::f32::consts::TAU);
        self.pitch = delta[1].mul_add(-0.006, self.pitch).clamp(-1.5, 1.5);
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Camera pan is finite floating point vector arithmetic."
    )]
    pub fn pan(&mut self, delta: [f32; 2]) {
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let up = right.cross(self.forward());
        let scale = self.distance * 0.0015;
        self.target += (-right * delta[0] + up * delta[1]) * scale;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount * 0.14).exp()).clamp(0.3, 90.0);
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Fly movement contains only finite floating point vector operations."
    )]
    pub fn fly(&mut self, direction: Vec3, seconds: f32) {
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let delta = right * direction.x + Vec3::Y * direction.y + self.forward() * direction.z;
        self.target += delta.normalize_or_zero() * seconds * self.distance.max(3.0) * 0.8;
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Framing uses finite scene bounds and floating point vector arithmetic."
    )]
    pub fn frame(&mut self, bounds: Aabb) {
        if bounds.is_empty() {
            return;
        }
        self.target = Vec3::from_array(bounds.centre());
        let size = Vec3::from_array(bounds.max) - Vec3::from_array(bounds.min);
        self.distance = (size.length() * 1.3).clamp(2.0, 90.0);
        self.pitch = -0.8;
    }

    pub fn matrix(&self, size: DrawableSize) -> Mat4 {
        self.render_camera().view_projection(size).0
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Unprojection uses finite floating point matrices and viewport coordinates."
    )]
    pub fn ray(&self, point: [f32; 2], size: DrawableSize) -> Option<(Vec3, Vec3)> {
        if size.is_empty() {
            return None;
        }
        let viewport = size.ui_viewport();
        let physical = Vec2::new(
            pixel_float(viewport.x) + point[0] * pixel_float(viewport.width) / 480.0,
            pixel_float(viewport.y) + point[1] * pixel_float(viewport.height) / 272.0,
        );
        let ndc = Vec2::new(
            physical.x * 2.0 / dimension(size.width) - 1.0,
            1.0 - physical.y * 2.0 / dimension(size.height),
        );
        let inverse = self.matrix(size).inverse();
        let near = inverse * Vec4::new(ndc.x, ndc.y, -1.0, 1.0);
        let far = inverse * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        if near.w.abs() < 0.000_001 || far.w.abs() < 0.000_001 {
            return None;
        }
        let origin = near.truncate() / near.w;
        let direction = (far.truncate() / far.w - origin).normalize_or_zero();
        Some((origin, direction))
    }
}

pub(super) fn dimension(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

pub(super) fn pixel_float(value: i32) -> f32 {
    f32::from(i16::try_from(value).unwrap_or(i16::MAX))
}
