//! Analytic, allocation-free balloon motion and its fixed-anchor string.

use glam::{Quat, Vec3};

use crate::engine::level::BalloonDef;
use crate::engine::spatial::Aabb;

/// Fixed mesh-node contract: economical curved string, never a rope simulation.
pub const STRING_NODE_NAMES: [&str; 12] = [
    "balloon_string_00",
    "balloon_string_01",
    "balloon_string_02",
    "balloon_string_03",
    "balloon_string_04",
    "balloon_string_05",
    "balloon_string_06",
    "balloon_string_07",
    "balloon_string_08",
    "balloon_string_09",
    "balloon_string_10",
    "balloon_string_11",
];

/// One sampled balloon pose in the placement's local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BalloonPose {
    /// Fixed tie point; it never follows body sway or bob.
    pub anchor: Vec3,
    /// Current bottom of the latex knot.
    pub knot: Vec3,
    /// Control point of the curved quadratic string.
    pub control: Vec3,
    /// Rest knot used to retain stable lighting support across a sway cycle.
    pub rest_knot: Vec3,
    /// Maximum horizontal/vertical motion, for stable conservative bounds.
    pub motion_extent: Vec3,
    /// Body tilt about the knot.
    pub rotation: Quat,
    /// Latex-only uniform scale.
    pub scale: f32,
    /// Linear albedo; never an emissive term.
    pub color: [f32; 3],
}

impl BalloonPose {
    /// String sample at `fraction`, including exact fixed/moving endpoints.
    #[must_use]
    pub fn string_point(self, fraction: f32) -> Vec3 {
        let t = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if t <= 0.0 {
            return self.anchor;
        }
        if t >= 1.0 {
            return self.knot;
        }
        self.anchor
            .lerp(self.control, t)
            .lerp(self.control.lerp(self.knot, t), t)
    }

    /// Stable body lighting support covering every allowed tilt and sway.
    #[must_use]
    pub fn body_bounds(self, authored: Aabb) -> Aabb {
        let radius = Vec3::from_array(authored.min)
            .abs()
            .max(Vec3::from_array(authored.max).abs())
            .length()
            * self.scale;
        let extent = Vec3::new(
            self.motion_extent.x + radius,
            self.motion_extent.y + radius,
            self.motion_extent.z + radius,
        );
        Aabb {
            min: [
                self.rest_knot.x - extent.x,
                self.rest_knot.y - extent.y,
                self.rest_knot.z - extent.z,
            ],
            max: [
                self.rest_knot.x + extent.x,
                self.rest_knot.y + extent.y,
                self.rest_knot.z + extent.z,
            ],
        }
    }

    /// Current latex centre; string height never drags the sample toward the floor.
    #[must_use]
    pub fn body_centre(self, authored: Aabb) -> Vec3 {
        let centre = Vec3::from_array(authored.centre());
        let offset = self.rotation.mul_vec3(Vec3::new(
            centre.x * self.scale,
            centre.y * self.scale,
            centre.z * self.scale,
        ));
        Vec3::new(
            self.knot.x + offset.x,
            self.knot.y + offset.y,
            self.knot.z + offset.z,
        )
    }
}

/// Samples the balloon directly from simulation time; no state integration,
/// random sampling, accumulated drift or frame-rate-dependent offsets.
#[must_use]
pub fn sample(def: &BalloonDef, seconds: f32) -> BalloonPose {
    let simulation_seconds = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    let phase = def.phase.unwrap_or(0.0);
    let shared_phase = def.shared_phase.unwrap_or(phase);
    let angle = std::f32::consts::TAU
        * simulation_seconds
            .mul_add(def.speed_hz, phase)
            .rem_euclid(1.0);
    let lateral = std::f32::consts::TAU
        * simulation_seconds
            .mul_add(def.speed_hz * 0.91, phase + 0.23)
            .rem_euclid(1.0);
    let bob = std::f32::consts::TAU
        * simulation_seconds
            .mul_add(def.speed_hz * 0.83, phase + 0.41)
            .rem_euclid(1.0);
    let shared = std::f32::consts::TAU
        * simulation_seconds
            .mul_add(0.13, shared_phase)
            .rem_euclid(1.0);
    let anchor = Vec3::from_array(def.anchor);
    let offset = Vec3::from_array(def.offset);
    let rest_knot = Vec3::new(
        anchor.x + offset.x,
        anchor.y + offset.y + def.string_length,
        anchor.z + offset.z,
    );
    let displacement = if def.enabled {
        Vec3::new(
            angle
                .sin()
                .mul_add(def.sway_amplitude, shared.sin() * def.shared_sway),
            bob.sin() * def.bob_amplitude,
            lateral
                .sin()
                .mul_add(def.sway_amplitude, shared.cos() * def.shared_sway),
        )
    } else {
        Vec3::ZERO
    };
    let knot = Vec3::new(
        rest_knot.x + displacement.x,
        rest_knot.y + displacement.y,
        rest_knot.z + displacement.z,
    );
    let tilt = if def.enabled {
        def.rotation_degrees.to_radians()
    } else {
        0.0
    };
    let rotation = Quat::from_rotation_x(lateral.sin() * tilt)
        .mul_quat(Quat::from_rotation_z(angle.sin() * tilt));
    let bend = if def.enabled {
        Vec3::new(0.035 * angle.cos(), 0.0, 0.025 * angle.sin())
    } else {
        Vec3::new(0.035, 0.0, 0.0)
    };
    let midpoint = anchor.lerp(knot, 0.5);
    BalloonPose {
        anchor,
        knot,
        control: Vec3::new(midpoint.x + bend.x, midpoint.y, midpoint.z + bend.z),
        rest_knot,
        motion_extent: Vec3::new(
            def.sway_amplitude + def.shared_sway,
            def.bob_amplitude,
            def.sway_amplitude + def.shared_sway,
        ),
        rotation,
        scale: def.scale,
        color: def.color.linear_albedo(),
    }
}
