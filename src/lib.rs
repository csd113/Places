//! Shared engine systems, the Places game, and the reusable Map Compiler.
//!
//! Applications depend on the engine; the engine has no dependency on game UI
//! or map-compilation orchestration. See `docs/ARCHITECTURE.md`.

pub mod engine;
pub mod game;
pub mod map_compiler;

#[cfg(test)]
mod architecture_audit;
#[cfg(test)]
mod fog_void_audit;
#[cfg(test)]
mod lighting_audit;
#[cfg(test)]
mod lighting_audit_cases;
#[cfg(test)]
mod lighting_isolation;
#[cfg(test)]
mod lighting_partition_audit;
#[cfg(test)]
mod lighting_repair_regression;
#[cfg(test)]
mod lighting_vertical_audit;
#[cfg(test)]
mod static_prop_lighting_tests;
#[cfg(test)]
mod surface_audit;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod zoo_audit;
