//! Reusable clip/socket whole-prey contract. Only the pinned Feltling profile is
//! currently admitted; a future creature needs its own geometry certificate.
use crate::engine::collision::DoorCollider;
use glam::{Mat4, Quat, Vec3};

pub const HUNTER_SHA256: &str = "74bc2d7882f1952ac2a97db4a3e5d4cab4e3f769e82c362b4fff866625611be1";

pub const SETUP: Vec3 = Vec3::new(0.000_048_861_653, 0.0, 1.702_853_8);
pub const SOCKET: &str = "mouth_capture_socket";
pub const CAPTURE_CLIP: &str = "capture_prey";
pub const SWALLOW_CLIP: &str = "swallow_prey";
pub const CAPTURE_SECONDS: f32 = 3.2;
pub const CONTACT_SECONDS: f32 = 1.25;
pub const SWALLOW_SECONDS: f32 = 5.9;
pub const INSIDE_SECONDS: f32 = 1.95;
pub const CONSUME_SECONDS: f32 = 2.55;

/// World-private candidate. The hunter receives coordinates only through a
/// validated beam query. Identity includes the entity store generation.
#[derive(Clone)]
pub struct AcousticTarget {
    pub id: String,
    pub key: u64,
    pub position: Vec3,
    pub crown: Vec3,
    pub heading: f32,
    pub collider: DoorCollider,
    pub leaf_offset: usize,
}

pub struct Echo {
    pub key: u64,
    pub position: Vec3,
    pub hit: Vec3,
    pub heading: f32,
}

#[must_use]
pub fn smooth(value: f32) -> f32 {
    let x = value.clamp(0.0, 1.0);
    x * x * (-2.0_f32).mul_add(x, 3.0)
}

/// A rigid contact and transit contract, independent of the prey controller.
/// Admission still requires a creature adapter and a geometry certificate.
#[derive(Debug)]
pub struct CaptureProfile {
    pub id: &'static str,
    pub prey_sha256: &'static str,
    pub setup: Vec3,
    pub socket: &'static str,
    pub capture_clip: &'static str,
    pub swallow_clip: &'static str,
    pub capture_seconds: f32,
    pub contact_seconds: f32,
    pub swallow_seconds: f32,
    pub inside_seconds: f32,
    pub consume_seconds: f32,
    pub local_translation: Vec3,
    pub local_rotation: Quat,
    pub transit_distance: f32,
    pub transit_start: f32,
    pub transit_seconds: f32,
    pub settle_distance: f32,
    pub settle_seconds: f32,
    pub lumen_material: u16,
    pub uv_row_origin: f32,
    pub uv_row_step: f32,
    pub lumen_first_row: f32,
    pub crown_depth: f32,
}

pub static FELTLING: CaptureProfile = CaptureProfile {
    id: "feltling-full-size-v03",
    prey_sha256: "f89602b39812c5112007e324ee358bdef523a8ba9bc697d2636824aea4ee1d7b",
    setup: SETUP,
    socket: SOCKET,
    capture_clip: CAPTURE_CLIP,
    swallow_clip: SWALLOW_CLIP,
    capture_seconds: CAPTURE_SECONDS,
    contact_seconds: CONTACT_SECONDS,
    swallow_seconds: SWALLOW_SECONDS,
    inside_seconds: INSIDE_SECONDS,
    consume_seconds: CONSUME_SECONDS,
    local_translation: Vec3::new(0.0, 0.004_078_136_4, 0.574_985_2),
    local_rotation: Quat::from_xyzw(-0.709_609_9, 0.0, 0.0, 0.704_594_8),
    transit_distance: 0.625,
    transit_start: 0.25,
    transit_seconds: 1.70,
    settle_distance: 0.050_35,
    settle_seconds: 0.40,
    lumen_material: 1,
    uv_row_origin: 0.475,
    uv_row_step: 0.045,
    lumen_first_row: 2.0,
    crown_depth: 0.18,
};

impl CaptureProfile {
    /// Full rigid socket composition; scale is deliberately absent.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite bounded profile translations in metres; authored unit socket quaternions."
    )]
    pub fn attached_pose(&self, socket: Mat4, swallow_seconds: Option<f32>) -> (Vec3, Quat) {
        let (_, socket_rotation, _) = socket.to_scale_rotation_translation();
        let rotation = (socket_rotation * self.local_rotation.normalize()).normalize();
        let mut position = socket.transform_point3(self.local_translation);
        if let Some(time) = swallow_seconds {
            position -= socket.transform_vector3(Vec3::Z).normalize_or_zero()
                * (self.transit_distance
                    * smooth((time - self.transit_start) / self.transit_seconds));
            position -= rotation
                * Vec3::new(
                    0.0,
                    0.0,
                    self.settle_distance * smooth(time / self.settle_seconds),
                );
        }
        (position, rotation)
    }
}
