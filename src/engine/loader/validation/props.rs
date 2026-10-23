//! Static prop options and explicit rigid-body authoring.

use crate::engine::level::{ComponentDef, LevelDef};

/// Placed prop ids, transforms and sizes.
pub(super) fn validate_props(level: &LevelDef) -> Result<(), String> {
    let mut span_slices = 0.0_f32;
    for (i, prop) in level.props.iter().enumerate() {
        if prop.model.trim().is_empty() {
            return Err(format!("Prop {i} must reference a non-empty model id"));
        }
        if !prop.x.is_finite()
            || !prop.y.is_finite()
            || !prop.z.is_finite()
            || !prop.rotation_degrees.is_finite()
            || !prop.pitch_degrees.is_finite()
            || !prop.roll_degrees.is_finite()
            || !prop.scale.is_finite()
        {
            return Err(format!(
                "Prop {i} position, rotation, and scale must be finite numbers"
            ));
        }
        if prop.scale <= 0.0 {
            return Err(format!("Prop {i} scale must be positive"));
        }
        if let Some(size) = prop.size
            && !size.iter().all(|v| v.is_finite() && *v > 0.0)
        {
            return Err(format!(
                "Prop {i} size must contain positive finite numbers"
            ));
        }
        span_slices += validate_static_prop_options(i, prop)?;
        if let Some(door) = &prop.animated_door {
            if prop.solid
                || prop.physics.is_some()
                || prop.float.is_some()
                || prop.span.is_some()
                || prop.pitch_degrees != 0.0
                || prop.roll_degrees != 0.0
                || prop.shadow_only
                || !prop.material_overrides.is_empty()
                || !prop.emission_overrides.is_empty()
                || door.clip.trim().is_empty()
                || door.boxes.is_empty()
                || door.boxes.len() > 32
                || door.boxes.iter().any(|shape| {
                    shape.node.trim().is_empty()
                        || shape
                            .center
                            .iter()
                            .any(|v| !v.is_finite() || v.abs() > 32.0)
                        || shape
                            .size
                            .iter()
                            .any(|v| !v.is_finite() || *v <= 0.0 || *v > 32.0)
                })
            {
                return Err(format!(
                    "Prop {i} animated_door requires an upright fixed placement, a clip and 1..32 finite node boxes"
                ));
            }
            if level
                .routes
                .iter()
                .any(|route| prop.id.as_deref() == Some(route.id.as_str()))
            {
                return Err(format!("Prop {i} animated_door cannot follow a route"));
            }
        }
    }
    if span_slices > 32_768.0 {
        return Err("Level span collision complexity exceeds 32768 slices".into());
    }
    if level
        .props
        .iter()
        .filter(|prop| prop.animated_door.is_some())
        .count()
        > 64
    {
        return Err("A level supports at most 64 animated doors".into());
    }
    let _validated_doors = crate::engine::animated_door::load(level)?;
    Ok(())
}

/// Static fitting/material controls cannot leak into independently posed routes.
fn validate_static_prop_options(
    i: usize,
    prop: &crate::engine::level::PropDef,
) -> Result<f32, String> {
    let mut slices = 0.0_f32;
    if prop.character_texture.is_some()
        && (!matches!(prop.model.as_str(), "mannequin" | "core:mannequin_black")
            || prop.physics.is_some()
            || prop.float.is_some()
            || prop.span.is_some()
            || !prop.material_overrides.is_empty())
    {
        return Err(format!(
            "Prop {i} character_texture requires a canonical animated mannequin without rigid physics, float, span or material overrides"
        ));
    }
    if prop.shadow_only
        && (prop.float.is_some()
            || prop.physics.is_some()
            || prop.span.is_some()
            || !prop.material_overrides.is_empty()
            || prop.light_control().is_some())
    {
        return Err(format!(
            "Prop {i} shadow_only requires an ordinary entity without physics, float, span, material overrides or fixed light control"
        ));
    }
    if (prop.pitch_degrees != 0.0 || prop.roll_degrees != 0.0)
        && (prop.span.is_some() || prop.float.is_some())
    {
        return Err(format!(
            "Prop {i} pitch/roll requires an ordinary or rigid prop; span and float profiles own their upright pose"
        ));
    }
    if let Some(span) = prop.span {
        let dimensions = [span.source_length, span.length];
        if dimensions
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
            || !span.rise.is_finite()
            || span.length * prop.scale > crate::engine::level::MAX_PROP_SPAN_LENGTH_M
            || !(0.1..=4.0).contains(&(span.length / span.source_length))
            || span.rise.abs() > span.length * crate::engine::level::MAX_RAMP_SLOPE
        {
            return Err(format!(
                "Prop {i} span needs positive finite lengths, a 0.1..4 length ratio, at most 32 m after scale, and a walkable slope"
            ));
        }
        // The source-size contract is explicit because collision is built
        // before any GLB is loaded and must use the same contact length.
        if prop
            .size
            .is_none_or(|size| (size[0] - span.source_length).abs() > 0.001)
        {
            return Err(format!(
                "Prop {i} span requires size[0] matching source_length"
            ));
        }
        if prop.solid {
            slices = (span.length * prop.scale / crate::engine::level::PROP_SPAN_COLLISION_STEP_M)
                .ceil();
        }
    }
    if prop.material_overrides.len() > crate::engine::level::MAX_PROP_MATERIALS
        || prop.material_overrides.iter().any(|(slot, material)| {
            usize::from(*slot) >= crate::engine::level::MAX_PROP_MATERIALS
                || material.trim().is_empty()
        })
    {
        return Err(format!(
            "Prop {i} material overrides need non-empty ids and material slots below {}",
            crate::engine::level::MAX_PROP_MATERIALS
        ));
    }
    if (prop.span.is_some() || !prop.material_overrides.is_empty())
        && (prop.physics.is_some()
            || prop.float.is_some()
            || !prop.components.is_empty()
            || !prop.bindings.is_empty())
    {
        return Err(format!(
            "Prop {i} span and material overrides require static scenery without physics, floating motion or entity components/bindings"
        ));
    }
    Ok(slices)
}

/// Rigid props have one pose owner and never leave immutable collision or light.
pub(in crate::engine) fn validate_prop_physics(level: &LevelDef) -> Result<(), String> {
    let fixed_count = level
        .props
        .iter()
        .filter(|prop| prop.has_fixed_physics_shape())
        .count();
    if fixed_count > 1024 {
        return Err("Level contains too many explicit fixed physics shapes (limit: 1024)".into());
    }
    let count = level
        .props
        .iter()
        .filter(|prop| prop.is_physics_movable())
        .count();
    if count > crate::engine::level::MAX_LEVEL_PHYSICS_PROPS {
        return Err(format!(
            "Level contains too many movable physics props: {count} (limit: {})",
            crate::engine::level::MAX_LEVEL_PHYSICS_PROPS
        ));
    }
    let ids = if count == 0 && fixed_count == 0 {
        Vec::new()
    } else {
        level.prop_instance_ids()
    };
    for (index, prop) in level.props.iter().enumerate() {
        let Some(physics) = prop.physics.as_ref() else {
            continue;
        };
        let context = format!("Prop {index} (`{}`)", prop.model);
        physics
            .validate()
            .and_then(|()| physics.validate_scale(prop.scale))
            .map_err(|error| format!("{context}: {error}"))?;
        if !physics.motion.is_movable() && !prop.has_fixed_physics_shape() {
            continue;
        }
        if [prop.x, prop.y, prop.z].iter().any(|coordinate| {
            coordinate.abs() > crate::engine::physics::config::MAX_ITEM_POSITION_M
        }) {
            return Err(format!(
                "{context} physics coordinates must stay within {} metres of the origin",
                crate::engine::physics::config::MAX_ITEM_POSITION_M
            ));
        }
        if prop.size.is_some_and(|size| {
            size.iter().any(|dimension| {
                let scaled = *dimension * prop.scale;
                !(crate::engine::physics::config::MIN_SHAPE_LENGTH_M
                    ..=crate::engine::physics::config::MAX_SHAPE_LENGTH_M)
                    .contains(&scaled)
            })
        }) {
            return Err(format!(
                "{context} physics size after scale must stay between {} and {} metres",
                crate::engine::physics::config::MIN_SHAPE_LENGTH_M,
                crate::engine::physics::config::MAX_SHAPE_LENGTH_M
            ));
        }
        if ids
            .get(index)
            .is_some_and(|id| level.routes.iter().any(|route| route.id.trim() == id))
        {
            return Err(format!(
                "{context} physics cannot also follow an entity route"
            ));
        }
        if !physics.motion.is_movable() {
            validate_fixed_prop_physics(prop, &context)?;
            continue;
        }
        if prop.float.is_some() || !prop.lights.is_empty() {
            return Err(format!(
                "{context} movable physics cannot declare float or attached lights"
            ));
        }
        if prop.components.iter().any(|component| {
            !matches!(
                component,
                ComponentDef::Audio {
                    sound: _,
                    gain: _,
                    looped: _,
                    enabled: _,
                    playing: _,
                    time_domain: _
                }
            )
        }) || !prop.bindings.is_empty()
        {
            return Err(format!(
                "{context} movable physics supports audio components only; \
                 other components and item-owned bindings are unsupported"
            ));
        }
    }
    Ok(())
}

fn validate_fixed_prop_physics(
    prop: &crate::engine::level::PropDef,
    context: &str,
) -> Result<(), String> {
    if prop.float.is_some()
        || prop.components.iter().any(|component| {
            matches!(
                component,
                ComponentDef::Animation {
                    clip: _,
                    speed: _,
                    looped: _,
                    playing: _
                } | ComponentDef::Ai(_)
                    | ComponentDef::Balloon(_)
                    | ComponentDef::Lifetime { seconds: _ }
            )
        })
    {
        return Err(format!(
            "{context} explicit fixed physics cannot declare floating, animation, AI, balloon or lifetime motion"
        ));
    }
    Ok(())
}
