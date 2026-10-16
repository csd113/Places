//! Shared engine systems, the Places game, and the reusable Map Compiler.
//!
//! Applications depend on the engine; the engine has no dependency on game UI
//! or map-compilation orchestration. See `docs/ARCHITECTURE.md`.

pub mod engine;
pub mod game;
pub mod map_compiler;
