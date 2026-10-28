//! Light circuits, fixture fields and local-source admission.

use crate::engine::level::LevelDef;

fn validate_light_circuits(level: &LevelDef) -> Result<(), String> {
    let controlled_props = level.props.iter().any(|prop| {
        prop.light_control()
            .is_some_and(|control| control.switchable)
    });
    if controlled_props
        || level
            .ceiling_lights
            .iter()
            .any(|fixture| fixture.switch_group.is_some())
    {
        let fixture_sources = level
            .ceiling_lights
            .iter()
            .filter(|fixture| fixture.switchable)
            .fold(0_usize, |count, fixture| {
                count.saturating_add(fixture.baked_source_count())
            });
        let sources = level
            .props
            .iter()
            .filter(|prop| {
                prop.light_control()
                    .is_some_and(|control| control.switchable)
            })
            .fold(fixture_sources, |count, prop| {
                count.saturating_add(prop.lights.iter().filter(|light| light.enabled).count())
            });
        if sources > crate::engine::lighting::probes::MAX_RUNTIME_PROBE_LIGHTS {
            return Err("Light circuits support at most eight switched light sources".to_string());
        }
    }
    let static_members: Vec<_> = level
        .ceiling_lights
        .iter()
        .filter(|fixture| fixture.static_switch == crate::engine::level::StaticSwitchMode::Prepared)
        .collect();
    if let Some(first) = static_members.first()
        && (static_members.len() > 8
            || first.switch_group.is_none()
            || static_members.iter().any(|fixture| {
                !fixture.enabled
                    || !fixture.switchable
                    || fixture.hold_in_static
                    || fixture.switch_group != first.switch_group
            })
            || level.ceiling_lights.iter().any(|fixture| {
                fixture.switch_group == first.switch_group
                    && fixture.static_switch != crate::engine::level::StaticSwitchMode::Prepared
            }))
    {
        return Err("Static switching supports one initially enabled circuit with one to eight switchable members, all opting in and without hold_in_static".to_string());
    }
    for (i, light) in level.ceiling_lights.iter().enumerate() {
        if let Some(group) = light.switch_group.as_deref() {
            if !light.switchable || group.trim().is_empty() || group.len() > 64 {
                return Err(format!(
                    "Ceiling light {i} switch_group needs a switchable fixture and a nonempty name of at most 64 bytes"
                ));
            }
            if level.ceiling_lights.iter().take(i).any(|other| {
                other.switch_group.as_deref() == Some(group)
                    && (other.enabled != light.enabled
                        || other.hold_in_static != light.hold_in_static)
            }) {
                return Err(format!(
                    "Ceiling light {i} switch_group members must share enabled and hold_in_static states"
                ));
            }
        }
    }
    validate_prop_light_circuits(level)?;
    validate_power_switches(level)?;
    Ok(())
}

/// Prop members reuse fixture circuits and retain their fixed Static hold.
fn validate_prop_light_circuits(level: &LevelDef) -> Result<(), String> {
    let mut states = level
        .ceiling_lights
        .iter()
        .filter_map(|fixture| {
            fixture
                .switch_group
                .as_deref()
                .map(|group| (group, (fixture.enabled, fixture.hold_in_static)))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for (index, prop) in level.props.iter().enumerate() {
        let Some(group) = prop.switch_group.as_deref() else {
            continue;
        };
        let Some(control) = prop.light_control().filter(|control| control.switchable) else {
            return Err(format!(
                "Prop {index} switch_group requires a switchable light component on a fixed prop"
            ));
        };
        if group.trim().is_empty() || group.len() > 64 {
            return Err(format!(
                "Prop {index} switch_group needs a nonempty name of at most 64 bytes"
            ));
        }
        let state = (control.enabled, true);
        if states.get(group).is_some_and(|other| *other != state) {
            return Err(format!(
                "Prop {index} switch_group members must share enabled and hold_in_static states"
            ));
        }
        let _previous = states.insert(group, state);
    }
    Ok(())
}

/// Ceiling/wall fixture position, intensity, colour and mounting height.
pub(super) fn validate_ceiling_lights(level: &LevelDef) -> Result<(), String> {
    validate_light_circuits(level)?;
    if level
        .ceiling_lights
        .iter()
        .filter(|light| light.ultraviolet > 0.0)
        .count()
        > crate::engine::render::MAX_BLACKLIGHTS
    {
        return Err(
            "At most four ultraviolet fixtures may be authored, including disabled fixtures"
                .to_string(),
        );
    }
    for (i, light) in level.ceiling_lights.iter().enumerate() {
        if !light.ultraviolet.is_finite() || !(0.0..=8.0).contains(&light.ultraviolet) {
            return Err(format!(
                "Ceiling light {i} ultraviolet must be finite and between 0 and 8"
            ));
        }
        if light.caustics == Some(true) && !light.supports_caustics() {
            return Err(format!(
                "Ceiling light {i} caustics is supported only by pool round/wall fixture families"
            ));
        }
        if !light.x.is_finite() || !light.z.is_finite() || !light.rotation_degrees.is_finite() {
            return Err(format!(
                "Ceiling light {i} position and rotation must be finite numbers"
            ));
        }
        // The optional fixture intensity (`brightness`, also accepted as
        // `intensity`). Omitted means the standard 1.0 fixture; negative or
        // non-finite values are rejected, high values are clamped while baking.
        if let Some(brightness) = light.brightness {
            if !brightness.is_finite() {
                return Err(format!(
                    "Ceiling light {i} intensity must be a finite number"
                ));
            }
            if brightness < 0.0 {
                return Err(format!("Ceiling light {i} intensity cannot be negative"));
            }
        }
        // The optional emitted colour. Omitted means the standard warm
        // fixture; channels outside `[0, 1]` or non-finite values are
        // malformed data, exactly like a negative intensity.
        if let Some(color) = light.color
            && !color.is_valid()
        {
            return Err(format!(
                "Ceiling light {i} colour channels must be finite numbers between 0 and {}",
                crate::engine::lighting::MAX_LIGHT_COLOR
            ));
        }
        // A wall fixture is authored at its own world height. A ceiling
        // fixture normally derives its height from the ceiling, but may author
        // a world `y` to mount at a chosen height (a stacked building uses it
        // to pick a storey); either way an authored height must be finite.
        if light.mount == crate::engine::level::LightMount::Wall {
            match light.y {
                Some(y) if y.is_finite() => {}
                Some(_) => {
                    return Err(format!(
                        "Wall light {i} height (`y`) must be a finite number"
                    ));
                }
                None => {
                    return Err(format!(
                        "Wall light {i} needs a world height (`y`); a wall fixture cannot \
                         derive one from the ceiling"
                    ));
                }
            }
        }
        if let Some(y) = light.y
            && !y.is_finite()
        {
            return Err(format!(
                "Ceiling light {i} height (`y`) must be a finite number when authored"
            ));
        }
        // Optional range/falloff: a fixture may shape its own pool, but
        // malformed numbers are rejected rather than silently clamped, exactly
        // like an intensity.
        if let Some(range) = light.range
            && !(range.is_finite() && range > 0.0)
        {
            return Err(format!(
                "Ceiling light {i} range must be a positive finite number of metres"
            ));
        }
        // The optional independent emissive strength of the visible face.
        if let Some(emission) = light.emission
            && !(emission.is_finite() && emission >= 0.0)
        {
            return Err(format!(
                "Ceiling light {i} emission must be a finite number that is not negative"
            ));
        }
    }
    Ok(())
}

/// Check the retained source budget without constructing a source array.
fn validate_light_source_counts(counts: impl IntoIterator<Item = usize>) -> Result<(), String> {
    counts.into_iter().try_fold(0_u64, |count, next| {
        let total = count.saturating_add(u64::try_from(next).unwrap_or(u64::MAX));
        if total > crate::engine::level::MAX_LEVEL_LIGHT_SOURCES {
            Err(format!("Level declares {total} local light sources; the limit is {} including disabled sources", crate::engine::level::MAX_LEVEL_LIGHT_SOURCES))
        } else { Ok(total) }
    }).map(|_| ())
}

/// Validate the generic light sources a placed object owns.
///
/// These are the engine-level lights of [`crate::engine::lighting::LightSource`]: a
/// shape, a local offset, a colour and a pool. A malformed light is a level
/// error — unlike an unknown prop model, which degrades to a placeholder box —
/// because a light is authored data the engine can check completely.
pub(super) fn validate_prop_lights(level: &LevelDef) -> Result<(), String> {
    validate_light_source_counts(
        level
            .ceiling_lights
            .iter()
            .map(crate::engine::level::LightFixtureDef::baked_source_count)
            .chain(level.props.iter().map(|prop| prop.lights.len())),
    )?;
    for (i, prop) in level.props.iter().enumerate() {
        if prop.lights.len() > crate::engine::level::MAX_PROP_LIGHTS {
            return Err(format!(
                "Prop {i} declares {} attached lights; the limit is {}",
                prop.lights.len(),
                crate::engine::level::MAX_PROP_LIGHTS
            ));
        }
        for (j, light) in prop.lights.iter().enumerate() {
            if !light.offset.iter().all(|value| value.is_finite())
                || !light.rotation_degrees.is_finite()
            {
                return Err(format!(
                    "Prop {i} light {j} offset and rotation must be finite numbers"
                ));
            }
            if let Some(intensity) = light.intensity
                && !(intensity.is_finite() && intensity >= 0.0)
            {
                return Err(format!(
                    "Prop {i} light {j} intensity must be a finite number that is not negative"
                ));
            }
            if let Some(color) = light.color
                && !color.is_valid()
            {
                return Err(format!(
                    "Prop {i} light {j} colour channels must be finite numbers between 0 and {}",
                    crate::engine::lighting::MAX_LIGHT_COLOR
                ));
            }
            if let Some(range) = light.range
                && !(range.is_finite() && range > 0.0)
            {
                return Err(format!(
                    "Prop {i} light {j} range must be a positive finite number of metres"
                ));
            }
            if !light.shape().is_valid() {
                return Err(format!(
                    "Prop {i} light {j} has malformed {} dimensions; every extent must be \
                     finite, positive and within the engine caps",
                    light.shape().name()
                ));
            }
        }
    }
    Ok(())
}

fn validate_power_switches(level: &LevelDef) -> Result<(), String> {
    for (index, prop) in level.props.iter().enumerate() {
        if let Some(group) = prop.light_switch.as_deref() {
            if prop.physics.is_some()
                || prop.float.is_some()
                || prop.animated_door.is_some()
                || !prop.components.iter().any(|component| {
                    matches!(
                        component,
                        crate::engine::level::ComponentDef::Interactable {
                            enabled: true,
                            prompt: _,
                            reach: _,
                            label: _
                        }
                    )
                })
                || !level
                    .ceiling_lights
                    .iter()
                    .any(|fixture| fixture.switch_group.as_deref() == Some(group))
            {
                return Err(format!(
                    "Prop {index} circuit switch needs a fixed enabled interactable and an existing fixture circuit"
                ));
            }
            if !level.requires_full_lighting
                && level
                    .ceiling_lights
                    .iter()
                    .filter(|fixture| fixture.switch_group.as_deref() == Some(group))
                    .any(|fixture| {
                        fixture.static_switch != crate::engine::level::StaticSwitchMode::Prepared
                    })
            {
                return Err(format!(
                    "Prop {index} circuit switch requires Full lighting or exact prepared Static switching"
                ));
            }
        }
    }
    Ok(())
}
