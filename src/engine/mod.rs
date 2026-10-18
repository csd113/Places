//! Reusable scene, content, simulation and rendering systems.
//!
//! Game/player controls and tool interfaces are application-owned. GPU objects
//! remain private to the renderer backend; compiled records stay CPU-owned.

pub mod ai;
pub mod assets;
pub mod audio;
pub mod canonical_json;
pub mod collision;
pub mod collision_index;
pub(crate) mod diagnostics;
pub mod door;
pub mod entities;
pub mod entity;
pub mod environment;
pub mod font;
pub mod gltf;
pub mod input;
pub mod interact;
pub mod level;
pub mod lighting;
pub mod loader;
pub mod logging;
pub mod materials;
pub mod nav;
pub mod package;
pub mod paths;
pub mod perf;
pub mod physics;
pub mod progress;
pub mod project;
pub mod props;
pub mod quality;
pub mod render;
pub mod runtime_work;
pub mod scene;
pub mod spatial;
pub mod weather;

#[cfg(test)]
mod tests;

pub mod scripting;
