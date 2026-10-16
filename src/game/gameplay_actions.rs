//! Typed gameplay requests shared by the console and future game modes.
//!
//! These requests contain no console syntax, paths, player-controller clones or
//! persistence policy. The native runtime supplies live world/resource owners.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalScope {
    Held,
    MatchingPhysics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsAction {
    Pause,
    Resume,
    Step,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepresentationAction {
    Select(String),
    Place,
    PlacementPhysics(bool),
    Reset,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GameplayAction {
    Noclip(Option<bool>),
    Give(String),
    Respawn,
    Position,
    Remove(RemovalScope),
    Physics(PhysicsAction),
    /// Direct keyed activation for trigger boxes and game modes.
    ActivatePhysics(u64),
    TimeScale(f32),
    Trigger(String),
    TriggerLabels(Option<bool>),
    Become(RepresentationAction),
}
