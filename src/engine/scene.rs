//! Renderer-independent inputs shared by scene simulation and presentation.

/// A character's locomotion pose, supplied by the simulation each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocomotionState {
    /// Grounded and standing still.
    #[default]
    Idle,
    /// Grounded and moving horizontally.
    Walking,
    /// Off the floor: rising in a jump, falling, or leaving the water.
    Airborne,
    /// In deep water with the eye below the surface line.
    Swimming,
    /// In deep water with the eye at or above the surface line.
    SurfaceSwimming,
}

/// The locomotion inputs shared by simulation and character animation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LocomotionSnapshot {
    pub state: LocomotionState,
    /// Actual horizontal speed for the frame, in m/s.
    pub speed: f32,
}
