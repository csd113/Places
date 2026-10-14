//! Reusable rigid bodies layered beside the established player controller.

mod activation;
mod collision;
pub mod config;
mod contacts;
mod hold;
mod recovery;
mod surfaces;
mod water;
mod world;

pub use activation::ActivationReport;
pub use config::{
    BodyMotion, CompoundPart, PhysicsDef, PrimitiveShapeDef, ShapeDef, WaterPhysicsDef,
};
pub use recovery::{EscapeReason, RecoveryEvent, RecoveryOutcome};
pub use world::{
    BodySnapshot, BodyState, ContactImpulse, ImpactEvent, ItemPose, ItemWorld, PhysicsStats,
    PlayerContext, ResetReport,
};

/// Metres per second squared, shared by player and rigid-body gravity.
pub const GRAVITY: f32 = 9.8;
/// Fixed solver interval. At most twelve intervals are consumed per frame.
pub const FIXED_STEP: f32 = 1.0 / 120.0;
/// Body allocation and per-frame query bound, shared by authored and given items.
pub const MAX_BODIES: usize = 128;
/// Maximum distance to the first physical surface of a pickable body.
pub const PICKUP_REACH: f32 = 2.2;
/// Forward hand target for ordinary small bodies, in metres from the eye.
pub const HOLD_DISTANCE: f32 = 0.85;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod floor_tests;

#[cfg(test)]
mod hull_tests;

#[cfg(test)]
mod water_tests;
