//! Reusable scene, content, simulation and rendering systems.
//!
//! Game/player controls and tool interfaces are application-owned. GPU objects
//! remain private to the renderer backend; compiled records stay CPU-owned.

pub mod ai;
pub mod assets;
pub mod audio;
pub mod canonical_json;
pub mod collision;
pub mod diagnostics;
pub mod entities;
pub mod entity;
pub mod gltf;
pub mod input;
pub mod interact;
pub mod level;
pub mod lighting;
pub mod loader;
pub mod materials;
pub mod mechanisms;
pub mod nav;
pub mod package;
pub mod paths;
pub mod physics;
pub mod project;
pub mod props;
pub mod quality;
pub mod render;
pub mod rules;
pub mod runtime;
pub mod scene;
pub mod spatial;

pub use collision::index as collision_index;
pub use diagnostics::{logging, perf, progress};
pub use entities::{carry as carry_visibility, feltling, tall_guest};
pub use level::environment;
pub(crate) use level::structure;
pub use mechanisms::{animated_door, door, elevator};
pub use render::{font, weather};
pub use rules::{game_mode, health, tools};
pub(crate) use runtime::resources;
pub use runtime::work as runtime_work;

#[cfg(test)]
mod tests;

pub mod scripting;
