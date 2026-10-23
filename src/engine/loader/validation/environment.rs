//! Source identity, resource budgets and environmental validation.

use crate::engine::level::LevelDef;

/// Regional fog volumes: identity, finite ordered bounds, bounded density,
/// colour and falloff, and the count cap.
///
/// A malformed region is a level error rather than a silent clamp: a layer
/// the author meant to sit over a yard and that instead covers the whole map
/// (or none of it) has to be visible at build time. The check is a pure
/// schema check; resolution against the global atmosphere happens at install.
pub(super) fn validate_fog_regions(level: &LevelDef) -> Result<(), String> {
    let count = level
        .fog_regions
        .len()
        .saturating_add(level.ground_fog.len());
    if u64::try_from(count).unwrap_or(u64::MAX)
        > u64::try_from(crate::engine::level::MAX_FOG_REGIONS).unwrap_or(u64::MAX)
    {
        return Err(format!(
            "Level contains too many fog regions: {count} (limit: {})",
            crate::engine::level::MAX_FOG_REGIONS
        ));
    }
    let mut ids: Vec<&str> = Vec::with_capacity(count);
    for (i, id) in level
        .fog_regions
        .iter()
        .map(|region| region.id.trim())
        .chain(level.ground_fog.iter().map(|region| region.id.trim()))
        .enumerate()
    {
        if id.is_empty() {
            return Err(format!("fog region {i} names no id"));
        }
        if id.chars().count() > crate::engine::level::MAX_FOG_REGION_ID_CHARS {
            return Err(format!(
                "fog region {i} ('{id}') has an id longer than {} characters",
                crate::engine::level::MAX_FOG_REGION_ID_CHARS
            ));
        }
        if ids.contains(&id) {
            return Err(format!("fog region {i} ('{id}') repeats an id"));
        }
        ids.push(id);
    }
    for region in &level.fog_regions {
        validate_fog_volume(region)?;
    }
    for layer in &level.ground_fog {
        if !layer.height.is_finite() || layer.height <= 0.0 {
            return Err(format!(
                "ground fog '{}' height must be finite and positive",
                layer.id
            ));
        }
        validate_fog_volume(&layer.as_region())?;
    }
    Ok(())
}

fn validate_fog_volume(region: &crate::engine::level::FogRegionDef) -> Result<(), String> {
    let id = &region.id;

    if !region
        .min
        .iter()
        .chain(region.max.iter())
        .all(|value| value.is_finite())
    {
        return Err(format!("fog region '{id}' bounds must be finite numbers"));
    }
    if !(region.min[0] < region.max[0]
        && region.min[1] < region.max[1]
        && region.min[2] < region.max[2])
    {
        return Err(format!(
            "fog region '{id}' must have min below max on every axis"
        ));
    }
    if region
        .min
        .iter()
        .zip(&region.max)
        .any(|(min, max)| !(max - min).is_finite())
    {
        return Err(format!("fog region '{id}' dimensions must remain finite"));
    }
    if !region.density.is_finite()
        || !(0.0..=crate::engine::level::MAX_FOG_REGION_DENSITY).contains(&region.density)
    {
        return Err(format!(
            "fog region '{id}' has density {} (limit {})",
            region.density,
            crate::engine::level::MAX_FOG_REGION_DENSITY
        ));
    }
    if let Some(color) = region.color
        && !color
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    {
        return Err(format!(
            "fog region '{id}' colour components must be between 0.0 and 1.0"
        ));
    }
    if let Some(falloff) = region.falloff_m
        && (!falloff.is_finite() || falloff < 0.0)
    {
        return Err(format!(
            "fog region '{id}' has a falloff_m of {falloff} \
                 (must be a finite value at or above 0.0)"
        ));
    }
    if region.ground_y.is_some_and(|value| !value.is_finite())
        || region.top_y.is_some_and(|value| !value.is_finite())
    {
        return Err(format!(
            "fog region '{id}' ground_y and top_y must be finite when authored"
        ));
    }
    let ground_y = region.ground_y.unwrap_or(region.min[1]);
    let top_y = region.top_y.unwrap_or(region.max[1]);
    if top_y > ground_y && !(top_y - ground_y).is_finite() {
        return Err(format!("fog region '{id}' height fade must remain finite"));
    }
    Ok(())
}

/// Void walls: finite ordered boxes, a non-empty well-formed material id, the
/// count cap, and unique ids when present.
pub(super) fn validate_void_walls(level: &LevelDef) -> Result<(), String> {
    let count = level.void_walls.len();
    if u64::try_from(count).unwrap_or(u64::MAX)
        > u64::try_from(crate::engine::level::MAX_VOID_WALLS).unwrap_or(u64::MAX)
    {
        return Err(format!(
            "Level contains too many void walls: {count} (limit: {})",
            crate::engine::level::MAX_VOID_WALLS
        ));
    }
    let mut ids: Vec<&str> = Vec::with_capacity(count);
    for (i, piece) in level.void_walls.iter().enumerate() {
        if let Some(authored) = &piece.id {
            let id = authored.trim();
            if id.is_empty() {
                return Err(format!("void wall {i} has an empty id"));
            }
            if id.chars().count() > crate::engine::level::MAX_VOID_WALL_ID_CHARS {
                return Err(format!(
                    "void wall {i} ('{id}') has an id longer than {} characters",
                    crate::engine::level::MAX_VOID_WALL_ID_CHARS
                ));
            }
            if ids.contains(&id) {
                return Err(format!("void wall {i} ('{id}') repeats an id"));
            }
            ids.push(id);
        }
        let label = piece
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map_or_else(|| i.to_string(), |id| format!("{i} ('{id}')"));
        if !piece
            .min
            .iter()
            .chain(piece.max.iter())
            .all(|value| value.is_finite())
        {
            return Err(format!("void wall {label} bounds must be finite numbers"));
        }
        if !(piece.min[0] < piece.max[0]
            && piece.min[1] < piece.max[1]
            && piece.min[2] < piece.max[2])
        {
            return Err(format!(
                "void wall {label} must have min below max on every axis"
            ));
        }
        let material = piece.material.trim();
        if material.is_empty() {
            return Err(format!("void wall {label} names no material"));
        }
        if !crate::engine::assets::is_valid_asset_id(material) {
            return Err(format!(
                "void wall {label} material `{material}` is not a well-formed logical id"
            ));
        }
    }
    Ok(())
}

/// Validate the bounded global-light list and its independent ID namespace.
pub(super) fn validate_global_illuminators(level: &LevelDef) -> Result<(), String> {
    if level.global_illuminators.len() > crate::engine::level::MAX_GLOBAL_ILLUMINATORS {
        return Err(format!(
            "level may declare at most {} global illuminators",
            crate::engine::level::MAX_GLOBAL_ILLUMINATORS
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for light in &level.global_illuminators {
        light.validate()?;
        if !ids.insert(&light.id) {
            return Err(format!("duplicate global illuminator id `{}`", light.id));
        }
    }
    Ok(())
}

/// The optional sky: a well-formed texture id and bounded brightness/radiance.
/// Catalog resolution is a load-time decision, like other texture references.
pub(super) fn validate_sky(level: &LevelDef) -> Result<(), String> {
    let Some(sky) = &level.sky else {
        return Ok(());
    };
    let id = sky.texture.trim();
    if sky.ambient_color.is_some_and(|color| {
        color
            .iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
    }) {
        return Err("sky ambient_color must have finite linear channels in 0..=1".into());
    }
    if id.is_empty() {
        return Err("sky names no texture".to_string());
    }
    if !crate::engine::assets::is_valid_asset_id(id) {
        return Err(format!(
            "sky texture `{id}` is not a well-formed logical id"
        ));
    }
    if !sky.brightness.is_finite() || sky.brightness < 0.0 {
        return Err(format!(
            "sky brightness must be a finite value at or above 0.0, found {}",
            sky.brightness
        ));
    }
    if sky.brightness > crate::engine::level::MAX_SKY_BRIGHTNESS {
        return Err(format!(
            "sky brightness must be at most {}, found {}",
            crate::engine::level::MAX_SKY_BRIGHTNESS,
            sky.brightness
        ));
    }
    if !sky.ambient.is_finite()
        || !(0.0..=crate::engine::level::MAX_SKY_AMBIENT).contains(&sky.ambient)
    {
        return Err(format!(
            "sky ambient must be between 0.0 and {}, found {}",
            crate::engine::level::MAX_SKY_AMBIENT,
            sky.ambient
        ));
    }
    Ok(())
}

/// Animated emissions: a known effect, a finite rate and a bounded depth.
///
/// A malformed animation is a level error rather than a silent no-op: a sign
/// that was meant to breathe and does not is a bug the author has to see.
pub(super) fn validate_animated_emissions(level: &LevelDef) -> Result<(), String> {
    for (i, animation) in level.animated_emissions.iter().enumerate() {
        let id = animation.material.trim();
        if id.is_empty() {
            return Err(format!("Animated emission {i} names no material"));
        }
        let effect = animation
            .effect
            .as_deref()
            .map(str::trim)
            .filter(|effect| !effect.is_empty());
        if let Some(effect_name) = effect
            && crate::engine::render::AnimationEffect::parse(effect_name).is_none()
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) has an unknown effect `{effect_name}`; \
                 expected `pulse` or `flicker`"
            ));
        }
        if let Some(hz) = animation.hz
            && (!hz.is_finite() || hz <= 0.0 || hz > crate::engine::render::MAX_FLICKER_HZ)
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) must have a rate between 0 and {} Hz",
                crate::engine::render::MAX_FLICKER_HZ
            ));
        }
        if let Some(depth) = animation.depth
            && (!depth.is_finite()
                || depth <= 0.0
                || depth > crate::engine::render::MAX_ANIMATION_DEPTH)
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) must have a depth between 0 and {}",
                crate::engine::render::MAX_ANIMATION_DEPTH
            ));
        }
        if let Some(phase) = animation.phase
            && !phase.is_finite()
        {
            return Err(format!(
                "Animated emission {i} (`{id}`) has a non-finite phase"
            ));
        }
    }
    Ok(())
}

/// Format version, identity and spawn point.
pub(super) fn validate_header(level: &LevelDef) -> Result<(), String> {
    // 1. Schema identity and format version
    if level.schema != crate::engine::level::LEVEL_SCHEMA {
        return Err(
            "Unsupported level schema; author this source as places-level revision 1".into(),
        );
    }
    if level.format_version != crate::engine::level::LEVEL_FORMAT_VERSION {
        return Err(format!(
            "Unsupported level format_version: {} (expected {})",
            level.format_version,
            crate::engine::level::LEVEL_FORMAT_VERSION
        ));
    }

    // 2. Identity
    if level.id.trim().is_empty() {
        return Err("Level 'id' cannot be empty".into());
    }
    if level.name.trim().is_empty() {
        return Err("Level 'name' cannot be empty".into());
    }

    // 3. Spawn point sanity
    if !level.spawn.x.is_finite()
        || !level.spawn.z.is_finite()
        || !level.spawn.yaw_degrees.is_finite()
    {
        return Err("Player spawn coordinates or orientation contain non-finite numbers".into());
    }

    Ok(())
}

/// Per-element count budgets.
///
/// Each of these is an authoring bound, not a format limit: the engine's data
/// structures are `Vec`/`HashMap`-backed and the per-frame queries go through
/// the collision index, so the caps exist to refuse a pathological or
/// accidentally huge file before it becomes resident, not to define what fits.
/// The values are sized from the extended capacity fixture
/// (`tools/levels/build_capacity_fixtures.py`, which authors one past the
/// *former* cap in every category and is measured on the release build); a
/// level above them is genuinely outside the verified envelope rather than
/// merely large.
pub(super) fn validate_element_limits(level: &LevelDef) -> Result<(), String> {
    let room_count = level.room_iter().count();
    if u64::try_from(room_count).unwrap_or(u64::MAX) > crate::engine::level::MAX_LEVEL_ROOMS {
        return Err(format!(
            "Level contains too many rooms: {room_count} (limit: {})",
            crate::engine::level::MAX_LEVEL_ROOMS
        ));
    }
    if u64::try_from(level.walls.len()).unwrap_or(u64::MAX) > crate::engine::level::MAX_LEVEL_WALLS
    {
        return Err(format!(
            "Level contains too many walls: {} (limit: {})",
            level.walls.len(),
            crate::engine::level::MAX_LEVEL_WALLS
        ));
    }
    if u64::try_from(level.ceiling_lights.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_CEILING_LIGHTS
    {
        return Err(format!(
            "Level contains too many ceiling lights: {} (limit: {})",
            level.ceiling_lights.len(),
            crate::engine::level::MAX_LEVEL_CEILING_LIGHTS
        ));
    }
    if u64::try_from(level.props.len()).unwrap_or(u64::MAX) > crate::engine::level::MAX_LEVEL_PROPS
    {
        return Err(format!(
            "Level contains too many props: {} (limit: {})",
            level.props.len(),
            crate::engine::level::MAX_LEVEL_PROPS
        ));
    }
    if u64::try_from(level.decals.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_DECALS
    {
        return Err(format!(
            "Level contains too many decals: {} (limit: {})",
            level.decals.len(),
            crate::engine::level::MAX_LEVEL_DECALS
        ));
    }
    if u64::try_from(level.floor_patches.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_FLOOR_PATCHES
    {
        return Err(format!(
            "Level contains too many floor patches: {} (limit: {})",
            level.floor_patches.len(),
            crate::engine::level::MAX_LEVEL_FLOOR_PATCHES
        ));
    }
    Ok(())
}

/// The level's distinct material budget.
///
/// The count is exactly the id set [`crate::engine::materials::referenced_material_ids`]
/// resolves — the set the renderer builds its
/// [`crate::engine::materials::MaterialTable`] from — so this validator, the loader and
/// the draw path agree on what "a material" is by construction. The check runs
/// before any image is decoded or uploaded, so an over-budget file is refused
/// by name instead of saturating a material index.
pub(super) fn validate_materials(level: &LevelDef) -> Result<(), String> {
    let count = u64::try_from(crate::engine::materials::referenced_material_ids(level).len())
        .unwrap_or(u64::MAX);
    if count > crate::engine::level::MAX_LEVEL_MATERIALS {
        return Err(format!(
            "Level declares too many distinct materials: {count} (limit {})",
            crate::engine::level::MAX_LEVEL_MATERIALS
        ));
    }
    Ok(())
}
