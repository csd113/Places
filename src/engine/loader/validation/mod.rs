//! Ordered level-admission facade. Domain checks preserve the historical first error.

mod entities;
mod environment;
mod geometry;
mod lighting;
mod motion;
pub(super) mod props;

pub use entities::MAX_LIFETIME_SECONDS;
pub use entities::script_calls;
pub use geometry::point_buried_in_wall;

use crate::engine::level::LevelDef;
use crate::engine::loader::validate_prop_physics;
use entities::{
    collect_sequence_owners, validate_audio_references, validate_bindings, validate_effects,
    validate_instance_ids, validate_sequences, validate_sound_emitters, validate_spawns,
    validate_timers, validate_volumes, validate_zero_delay_cycles,
};
use environment::{
    validate_animated_emissions, validate_element_limits, validate_fog_regions,
    validate_global_illuminators, validate_header, validate_materials, validate_sky,
    validate_void_walls,
};
use geometry::{
    validate_architecture, validate_decal_surfaces, validate_decals, validate_floor_regions,
    validate_geometry_budget, validate_ladders, validate_rooms, validate_surface_shine,
    validate_walls, validate_water,
};
use lighting::{validate_ceiling_lights, validate_prop_lights};
use motion::{validate_doors, validate_floats, validate_routes};
use props::validate_props;

/// Validates level schema, format version, and physical dimensions.
///
/// Preserves intentional overlapping/intersecting geometry without snapping or
/// rejecting. The checks run in their historical order, so the first reported
/// problem is unchanged.
/// # Errors
///
/// Returns the first problem found: an unsupported format version, a missing or
/// duplicate id, non-finite or out-of-range dimensions, a prop outside its
/// budgets, or malformed openings and patches.
pub fn validate_level(level: &LevelDef) -> Result<(), String> {
    validate_header(level)?;
    validate_acoustics(level)?;
    if level.transitions.len() > 256 {
        return Err("Too many level transition destinations".into());
    }
    let mut transition_ids = std::collections::BTreeSet::new();
    for target in &level.transitions {
        crate::engine::project::validate_id(target)?;
        if !transition_ids.insert(target) {
            return Err("Duplicate level transition destination".into());
        }
    }
    crate::engine::scripting::validate_sources(level)?;
    validate_element_limits(level)?;
    validate_materials(level)?;
    validate_sky(level)?;
    if let Some(environment) = level.environment {
        environment.validate()?;
    }
    if level.weather_alternate.is_some() && level.weather.is_none() {
        return Err("weather_alternate requires authored default weather".to_string());
    }
    if let Some(cycle) = &level.weather_cycle {
        cycle.validate()?;
        if !level
            .weather
            .as_ref()
            .zip(level.weather_alternate.as_ref())
            .is_some_and(|(base, alternate)| {
                crate::engine::weather::strength_compatible(base, alternate)
            })
        {
            return Err("weather_cycle requires weather and weather_alternate with equal count and material".to_string());
        }
    }
    for (name, authored_weather) in [
        ("weather", level.weather.as_ref()),
        ("weather_alternate", level.weather_alternate.as_ref()),
    ] {
        let Some(weather) = authored_weather else {
            continue;
        };
        weather.snowfall().validate().map_err(|error| {
            if name == "weather" {
                error
            } else {
                format!("{name}: {error}")
            }
        })?;
        if weather.snowfall().storm_severity > 0.0
            && crate::engine::weather::shelter_count(level)
                > crate::engine::weather::MAX_STORM_SHELTERS
        {
            return Err("storm weather supports at most 32 ceiling/roof shelters".to_string());
        }
    }
    validate_global_illuminators(level)?;
    validate_rooms(level)?;
    validate_surface_shine(level)?;
    validate_floor_regions(level)?;
    validate_water(level)?;
    validate_ladders(level)?;
    validate_walls(level)?;
    validate_architecture(level)?;
    validate_ceiling_lights(level)?;
    validate_props(level)?;
    validate_prop_physics(level)?;
    validate_prop_lights(level)?;
    validate_doors(level)?;
    validate_effects(level)?;
    let mut index = validate_instance_ids(level)?;
    index.sequence_owners = collect_sequence_owners(level);
    validate_volumes(level, &index)?;
    validate_sound_emitters(level)?;
    validate_timers(level)?;
    validate_spawns(level, &index)?;
    validate_bindings(level, &index)?;
    validate_audio_references(level)?;
    validate_sequences(level, &index)?;
    validate_zero_delay_cycles(level, &index)?;
    validate_routes(level)?;
    validate_floats(level)?;
    validate_decals(level)?;
    validate_decal_surfaces(level)?;
    validate_animated_emissions(level)?;
    validate_fog_regions(level)?;
    validate_void_walls(level)?;
    validate_geometry_budget(level)
}

fn validate_acoustics(level: &LevelDef) -> Result<(), String> {
    if level.acoustic_materials.len() > 256 || level.doors.len() > 256 {
        return Err("Acoustics supports at most 256 material overrides and 256 doors".into());
    }
    let mut acoustic_ids = std::collections::BTreeSet::new();
    for material in &level.acoustic_materials {
        let id = &material.material;
        if !acoustic_ids.insert(id) {
            return Err("Duplicate acoustic material override".into());
        }
        if id.is_empty() || id.len() > 256 || !id.contains(':') {
            return Err("Acoustic material requires an existing logical material ID".into());
        }
        material.properties().validate()?;
    }
    for room in &level.rooms {
        room.acoustics.validate()?;
    }
    Ok(())
}
