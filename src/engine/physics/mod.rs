//! Reusable rigid bodies layered beside the established player controller.

mod activation;
mod collision;
pub mod config;
mod contact_solver;
mod contacts;
mod hold;
mod impacts;
pub mod ragdoll;
mod recovery;
mod surfaces;
mod trigger_path;
mod water;
mod world;

pub use activation::ActivationReport;
pub use config::{
    BodyMotion, CompoundPart, PhysicsDef, PrimitiveShapeDef, ShapeDef, WaterPhysicsDef,
};
pub(crate) use hold::{HeldPlayerContact, HeldPlayerSlide};
pub(crate) use impacts::ImpactAudio;
pub use impacts::{ImpactEvent, ImpactRelease, ReleaseIdentity, SurfaceMaterial};
pub use recovery::{EscapeReason, RecoveryEvent, RecoveryOutcome};
pub use trigger_path::ItemTriggerBody;
pub use world::checkpoint;
pub(crate) use world::prop_fracture;
pub use world::{
    BodySnapshot, BodyState, ContactImpulse, ItemPose, ItemWorld, PhysicsStats, PlayerContext,
    ResetReport, SimulationState,
};

/// Metres per second squared, shared by player and rigid-body gravity.
pub const GRAVITY: f32 = 9.8;
/// Fixed solver interval. At most twelve intervals are consumed per frame.
pub const FIXED_STEP: f32 = 1.0 / 120.0;
/// Ordinary item limit, shared by authored and given items.
pub const MAX_BODIES: usize = crate::engine::level::MAX_LEVEL_PHYSICS_PROPS;
/// Shared transient reserve for bottle, gingerbread house and snow globe fragments.
pub(crate) const MAX_BREAK_FRAGMENTS: usize = 20;
/// Maximum number of item bodies, including transient fracture pieces.
pub(crate) const MAX_ITEM_BODIES: usize = MAX_BODIES.saturating_add(MAX_BREAK_FRAGMENTS);
/// Ordinary items plus the supported articulated skeletons (15 bones each).
pub const MAX_SOLVER_BODIES: usize = MAX_BODIES + ragdoll::MAX_RAGDOLLS * 15;
/// Maximum distance to the first physical surface of a pickable body.
pub const PICKUP_REACH: f32 = 2.2;
/// Forward hand target for ordinary small bodies, in metres from the eye.
pub const HOLD_DISTANCE: f32 = 0.85;

#[cfg(test)]
#[path = "../tests/settled_interpolation.rs"]
mod settled_interpolation;

#[cfg(test)]
#[path = "../tests/ragdoll.rs"]
mod ragdoll_tests;
