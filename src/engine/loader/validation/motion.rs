//! Floating objects, route sweeps and moving door admission.

use crate::engine::level::{ComponentDef, LevelDef, LevelSurfaces};
use glam::Vec3;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

/// One validation call owns at most one exact static collision derivation.
/// This is request-local scratch, never a retained or revision-keyed cache.
pub(super) struct Geometry<'a> {
    level: &'a LevelDef,
    solids: OnceCell<Vec<crate::engine::collision::WallAabb>>,
}

impl<'a> Geometry<'a> {
    pub(super) const fn new(level: &'a LevelDef) -> Self {
        Self {
            level,
            solids: OnceCell::new(),
        }
    }

    fn solids(&self) -> &[crate::engine::collision::WallAabb] {
        self.solids.get_or_init(|| self.level.collision_aabbs())
    }
}

/// Floating props: a floating hull cannot be solid, its authored motion must
/// be finite and bounded, it cannot also be routed, and its whole swept
/// footprint must sit inside one water volume.
///
/// The containment rule is what keeps a float off the rim: the level proves at
/// load that even at the heel's extreme the silhouette (half-diagonal plus the
/// heel's horizontal excursion) fits inside the basin rectangle, so the
/// runtime update has no horizontal freedom to misuse.
pub(super) fn validate_floats(level: &LevelDef) -> Result<(), String> {
    let float_count = level
        .props
        .iter()
        .filter(|prop| prop.float.is_some())
        .count();
    if float_count > crate::engine::level::MAX_LEVEL_FLOAT_PROPS {
        return Err(format!(
            "Level declares {float_count} floating props; the limit is {}",
            crate::engine::level::MAX_LEVEL_FLOAT_PROPS
        ));
    }
    if float_count == 0 {
        return Ok(());
    }
    let water = crate::engine::level::WaterVolumes::from_level(level);
    let ids = level.prop_instance_ids();
    for (index, prop) in level.props.iter().enumerate() {
        let Some(float) = prop.float.as_ref() else {
            continue;
        };
        let context = format!("Float prop {index} (`{}`)", prop.model);
        let (width, height, depth) = validate_float_fields(&context, prop, float)?;
        if let Some(id) = ids.get(index)
            && level.routes.iter().any(|route| route.id.trim() == id)
        {
            return Err(format!(
                "{context} is addressed by a route; a floating prop cannot be routed"
            ));
        }
        let half_diagonal = 0.5 * width.hypot(depth);
        let heel_excursion = 0.5 * height * float.heel_degrees.to_radians().sin();
        let radius = half_diagonal + heel_excursion;
        if !water.contains_disc(prop.x, prop.z, radius) {
            return Err(format!(
                "{context} must be fully inside a water volume: its swept footprint \
                 (radius {radius:.3} m) is not contained at ({}, {})",
                prop.x, prop.z
            ));
        }
    }
    Ok(())
}

/// One float prop's own fields: solid/footprint/motion bounds.
///
/// Returns the scaled `(width, height, depth)` the containment rule needs.
fn validate_float_fields(
    context: &str,
    prop: &crate::engine::level::PropDef,
    float: &crate::engine::level::PropFloatDef,
) -> Result<(f32, f32, f32), String> {
    if prop.solid {
        return Err(format!(
            "{context} must set `solid: false`: a floating hull cannot leave a static collider"
        ));
    }
    if !prop.x.is_finite() || !prop.z.is_finite() || !prop.scale.is_finite() || prop.scale <= 0.0 {
        return Err(format!(
            "{context} position and scale must be finite and its scale positive"
        ));
    }
    let Some(size) = prop.size else {
        return Err(format!(
            "{context} must author `size`: the float contract is validated against its footprint"
        ));
    };
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(format!(
            "{context} size must be three positive finite numbers"
        ));
    }
    let width = size[0] * prop.scale;
    let height = size[1] * prop.scale;
    let depth = size[2] * prop.scale;
    if !float.draft.is_finite() || float.draft <= 0.0 || float.draft >= height {
        return Err(format!(
            "{context} draft must be a finite number above 0 and below its height ({height} m)"
        ));
    }
    if !float.bob.is_finite() || float.bob < 0.0 || float.bob > 0.5 * height {
        return Err(format!(
            "{context} bob must be finite, 0 or positive, and at most half its height"
        ));
    }
    if !float.bob_seconds.is_finite() || float.bob_seconds <= 0.0 {
        return Err(format!(
            "{context} bob_seconds must be a positive finite number"
        ));
    }
    if !float.heel_degrees.is_finite()
        || !(0.0..=crate::engine::level::MAX_FLOAT_HEEL_DEGREES).contains(&float.heel_degrees)
    {
        return Err(format!(
            "{context} heel_degrees must be finite, 0 or positive, and at most {}",
            crate::engine::level::MAX_FLOAT_HEEL_DEGREES
        ));
    }
    if !float.heel_seconds.is_finite() || float.heel_seconds <= 0.0 {
        return Err(format!(
            "{context} heel_seconds must be a positive finite number"
        ));
    }
    if let Some(phase) = float.phase
        && (!phase.is_finite() || !(0.0..=1.0).contains(&phase))
    {
        return Err(format!(
            "{context} phase must be a finite number between 0.0 and 1.0"
        ));
    }
    Ok((width, height, depth))
}

/// Authored entity routes: unique resolving ids, bounded step lists and
/// every waypoint finite, walkable and reachable in a straight line at the
/// entity's own body size.
///
/// A route is a promise that a character can actually walk it, so the checks
/// mirror the runtime: the waypoint must sit on a real walkable surface, each
/// straight segment must stay on the floor without a step taller than the
/// entity can climb, and no wall may block the body anywhere along it. A
/// `solid: true` prop is refused because its own collision box would block
/// its first step.
#[expect(clippy::too_many_lines, reason = "one cohesive route validation pass")] // one cohesive route validation pass
pub(super) fn validate_routes(level: &LevelDef, geometry: &Geometry<'_>) -> Result<(), String> {
    if u64::try_from(level.routes.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_ROUTES
    {
        return Err(format!(
            "Level contains too many entity routes: {} (limit: {})",
            level.routes.len(),
            crate::engine::level::MAX_LEVEL_ROUTES
        ));
    }
    if level.routes.is_empty() {
        return Ok(());
    }
    let ids = level.prop_instance_ids();
    let id_index: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let walls = geometry.solids();
    let collision_index = crate::engine::collision_index::CollisionIndex::build(walls);
    let floor = crate::engine::level::WalkableFloor::from_level(level);
    let surfaces = LevelSurfaces::new(level);
    let mut seen: HashSet<String> = HashSet::new();
    for (route_index, route) in level.routes.iter().enumerate() {
        let id = route.id.trim();
        if id.is_empty() {
            return Err(format!("Entity route {route_index} names no instance id"));
        }
        if !seen.insert(id.to_string()) {
            return Err(format!(
                "Entity route {route_index} duplicates instance `{id}`"
            ));
        }
        let Some(&prop_index) = id_index.get(id) else {
            return Err(format!(
                "Entity route {route_index} targets unknown instance `{id}`"
            ));
        };
        if route.steps.is_empty() {
            return Err(format!("Entity route `{id}` declares no steps"));
        }
        if route.steps.len() > crate::engine::level::MAX_ROUTE_STEPS {
            return Err(format!(
                "Entity route `{id}` declares {} steps; the limit is {}",
                route.steps.len(),
                crate::engine::level::MAX_ROUTE_STEPS
            ));
        }
        let Some(prop) = level.props.get(prop_index) else {
            continue;
        };
        if prop.solid {
            return Err(format!(
                "Entity route `{id}` drives a `solid: true` prop; its own collision box \
                 would block every step. Make the entity non-solid."
            ));
        }
        if prop
            .components
            .iter()
            .any(|component| matches!(component, ComponentDef::Ai(_)))
        {
            return Err(format!(
                "Entity route `{id}` drives an entity that also authors an `ai` component; \
                 one entity has one locomotion owner: keep the route or the AI, not both"
            ));
        }
        let size = prop.resolved_size(crate::engine::level::PROP_FALLBACK_SIZE);
        // The authored `nav_agent` body (when present) is validated exactly as
        // the mover uses it; otherwise validation uses the *wider* axis,
        // clamped to the same minimum the runtime disc uses, so an elongated
        // body can never overlap a wall the runtime disc would miss.
        let (radius, body_height, step_height) = prop
            .components
            .iter()
            .find_map(|component| match component {
                ComponentDef::NavAgent {
                    radius,
                    height,
                    step_height,
                    speed_mps: _,
                    max_slope: _,
                } => Some((*radius, *height, *step_height)),
                ComponentDef::Interactable {
                    prompt: _,
                    reach: _,
                    enabled: _,
                    label: _,
                }
                | ComponentDef::Animation {
                    clip: _,
                    speed: _,
                    looped: _,
                    playing: _,
                }
                | ComponentDef::Audio {
                    sound: _,
                    gain: _,
                    looped: _,
                    enabled: _,
                    playing: _,
                    time_domain: _,
                }
                | ComponentDef::Light {
                    enabled: _,
                    switchable: _,
                    emission_scale: _,
                }
                | ComponentDef::Material {
                    variants: _,
                    current: _,
                }
                | ComponentDef::State { name: _, value: _ }
                | ComponentDef::Lifetime { seconds: _ }
                | ComponentDef::Steam { enabled: _ }
                | ComponentDef::Water { enabled: _ }
                | ComponentDef::NavObstacle {
                    size: _,
                    affects_nav: _,
                }
                | ComponentDef::Fade(_)
                | ComponentDef::Glow(_)
                | ComponentDef::Balloon(_)
                | ComponentDef::Ai(_)
                | ComponentDef::LightResponse(_)
                | ComponentDef::Carry(_) => None,
            })
            .unwrap_or_else(|| {
                (
                    (size[0].max(size[2]) * 0.5).max(crate::engine::entity::ENTITY_MIN_RADIUS_M),
                    size[1].max(0.05),
                    crate::engine::entity::ENTITY_STEP_HEIGHT_M,
                )
            });
        let position = Vec3::new(
            prop.x,
            surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
            prop.z,
        );
        validate_route_steps(
            id,
            &route.steps,
            position,
            radius,
            body_height,
            step_height,
            walls,
            &collision_index,
            &floor,
        )?;
    }
    Ok(())
}

/// A route interpolation count whose adjacent indices remain exact in f32.
fn route_sample_count(requested: f32, context: &str) -> Result<u32, String> {
    const MAX_EXACT_SAMPLES: f32 = 16_777_216.0;
    let count = requested.ceil().max(1.0);
    if !requested.is_finite() || count > MAX_EXACT_SAMPLES {
        return Err(format!(
            "{context} (`move_to`) segment exceeds the exact interpolation sample budget"
        ));
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The finite integer sample count is checked within 1..=2^24 before conversion."
    )]
    let samples = count as u32;
    Ok(samples)
}

/// One route's ordered steps: finite data, real floors and clear straight
/// segments between consecutive waypoints.
#[expect(
    clippy::too_many_arguments,
    reason = "Route validation keeps authored step inputs, entity body dimensions, floor and collision data explicit."
)]
fn validate_route_steps(
    id: &str,
    steps: &[crate::engine::level::RouteStepDef],
    start: Vec3,
    radius: f32,
    body_height: f32,
    step_height: f32,
    walls: &[crate::engine::collision::WallAabb],
    collision_index: &crate::engine::collision_index::CollisionIndex,
    floor: &crate::engine::level::WalkableFloor,
) -> Result<(), String> {
    let mut position = start;
    for (step_index, step) in steps.iter().enumerate() {
        let context = format!("Entity route `{id}` step {step_index}");
        match step {
            crate::engine::level::RouteStepDef::MoveTo { x, z, speed } => {
                if !x.is_finite() || !z.is_finite() || !speed.is_finite() {
                    return Err(format!(
                        "{context} (`move_to`) position and speed must be finite"
                    ));
                }
                if *speed <= 0.0 || *speed > crate::engine::level::MAX_ROUTE_SPEED_MPS {
                    return Err(format!(
                        "{context} (`move_to`) speed must be between 0 and {} m/s",
                        crate::engine::level::MAX_ROUTE_SPEED_MPS
                    ));
                }
                let waypoint_y = route_path_is_clear(
                    &context,
                    walls,
                    collision_index,
                    floor,
                    position,
                    (*x, *z),
                    radius,
                    body_height,
                    step_height,
                )?;
                position = Vec3::new(*x, waypoint_y, *z);
            }
            crate::engine::level::RouteStepDef::Face { yaw_degrees } => {
                if !yaw_degrees.is_finite() {
                    return Err(format!("{context} (`face`) yaw must be finite"));
                }
            }
            crate::engine::level::RouteStepDef::Wait { seconds } => {
                if !seconds.is_finite()
                    || *seconds <= 0.0
                    || *seconds > crate::engine::level::MAX_ROUTE_WAIT_SECONDS
                {
                    return Err(format!(
                        "{context} (`wait`) seconds must be between 0 and {}",
                        crate::engine::level::MAX_ROUTE_WAIT_SECONDS
                    ));
                }
            }
            crate::engine::level::RouteStepDef::Play {
                clip,
                seconds,
                looped: _,
            } => {
                if clip.trim().is_empty() {
                    return Err(format!("{context} (`play`) needs a clip name"));
                }
                if !seconds.is_finite()
                    || *seconds <= 0.0
                    || *seconds > crate::engine::level::MAX_ROUTE_PLAY_SECONDS
                {
                    return Err(format!(
                        "{context} (`play`) seconds must be between 0 and {}",
                        crate::engine::level::MAX_ROUTE_PLAY_SECONDS
                    ));
                }
            }
        }
    }
    Ok(())
}

/// True when a straight route segment stays clear of walls and floor breaks.
///
/// The segment is sampled every 10 cm; each sample must rest on a walkable
/// floor, and no wall may block the body there. The height change between
/// consecutive samples is bounded by the entity step, so a route cannot climb
/// a cliff in one sample.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded world-space segment sampling"
)] // bounded world-space segment sampling
#[expect(
    clippy::too_many_arguments,
    reason = "Route validation keeps authored step inputs, entity body dimensions, floor and collision data explicit."
)]
fn route_path_is_clear(
    context: &str,
    walls: &[crate::engine::collision::WallAabb],
    collision_index: &crate::engine::collision_index::CollisionIndex,
    floor: &crate::engine::level::WalkableFloor,
    from: Vec3,
    to: (f32, f32),
    radius: f32,
    body_height: f32,
    step_height: f32,
) -> Result<f32, String> {
    const SAMPLE_M: f32 = 0.02;
    let start = glam::Vec2::new(from.x, from.z);
    let end = glam::Vec2::new(to.0, to.1);
    let delta = end - start;
    let distance = delta.length();
    if !distance.is_finite() {
        return Err(format!("{context} (`move_to`) segment is not finite"));
    }
    let samples = route_sample_count(distance / SAMPLE_M, context)?;
    let mut previous_y = from.y;
    for index in 0..=samples {
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "Both index and nonzero sample count are checked at or below f32's exact integer limit."
        )]
        let t = index as f32 / samples as f32;
        let point = start + delta * t;
        let Some(floor_y) = floor.walk_height_near(point.x, point.y, previous_y, step_height)
        else {
            return Err(format!(
                "{context} (`move_to`) crosses off the walkable floor at \
                 ({:.2}, {:.2})",
                point.x, point.y
            ));
        };
        if (floor_y - previous_y).abs() > step_height + crate::engine::collision::STEP_EPS {
            return Err(format!(
                "{context} (`move_to`) steps more than {step_height:.2} m at ({:.2}, {:.2}); \
                 the entity cannot climb it",
                point.x, point.y
            ));
        }
        let mut blocked = false;
        collision_index.for_each_disc(point.x, point.y, radius, walls, |wall| {
            if wall.blocks_body(floor_y, body_height)
                && wall.overlaps_disc(point.x, point.y, radius)
            {
                blocked = true;
            }
        });
        if blocked {
            return Err(format!(
                "{context} (`move_to`) is blocked by geometry at ({:.2}, {:.2})",
                point.x, point.y
            ));
        }
        previous_y = floor_y;
    }
    Ok(previous_y)
}

/// Doors: count, geometry, swing, state and interaction contract.
///
/// A door is a physical leaf, so the checks are geometric: a positive leaf,
/// a non-zero bounded swing, finite positive speeds, a swing direction that
/// agrees with its sign, a closed leaf that does not start inside solid
/// geometry, and an interaction reach inside the engine cap. A door's id is
/// validated with every other instance id, so a door and a prop can never share
/// a name.
pub(super) fn validate_doors(level: &LevelDef, geometry: &Geometry<'_>) -> Result<(), String> {
    if u64::try_from(level.doors.len()).unwrap_or(u64::MAX) > crate::engine::level::MAX_LEVEL_DOORS
    {
        return Err(format!(
            "Level contains too many doors: {} (limit: {})",
            level.doors.len(),
            crate::engine::level::MAX_LEVEL_DOORS
        ));
    }
    if level.doors.is_empty() {
        return Ok(());
    }
    let surfaces = LevelSurfaces::new(level);
    let solids = geometry.solids();
    for (i, door) in level.doors.iter().enumerate() {
        validate_door_fields(i, door)?;
        // The hinge must stand somewhere with a floor: a door floating in the
        // void has no base to swing from.
        if surfaces.floor_y_at(door.x, door.z).is_none() {
            return Err(format!(
                "Door {i} hinge ({:.2}, {:.2}) is outside every room section",
                door.x, door.z
            ));
        }
        validate_door_leaf_is_clear(i, door, level, solids)?;
    }
    Ok(())
}

/// One door's own dimensions, swing, state and lock contract.
///
/// The leaf's authored prompt and reach live in its `interactable` component
/// and are validated by [`validate_instance_ids`] with every other component.
fn validate_door_fields(i: usize, door: &crate::engine::level::DoorDef) -> Result<(), String> {
    if !door.x.is_finite()
        || !door.y.is_finite()
        || !door.z.is_finite()
        || !door.rotation_degrees.is_finite()
        || !door.width.is_finite()
        || !door.height.is_finite()
        || !door.thickness.is_finite()
        || !door.swing_degrees.is_finite()
        || !door.open_speed_degrees.is_finite()
    {
        return Err(format!("Door {i} geometry must be finite numbers"));
    }
    if door.width <= 0.0 || door.height <= 0.0 || door.thickness <= 0.0 {
        return Err(format!(
            "Door {i} width, height and thickness must be positive"
        ));
    }
    if door.width > crate::engine::level::MAX_DOOR_DIMENSION_M
        || door.height > crate::engine::level::MAX_DOOR_DIMENSION_M
        || door.thickness > crate::engine::level::MAX_DOOR_DIMENSION_M
    {
        return Err(format!(
            "Door {i} dimensions exceed the {} m limit",
            crate::engine::level::MAX_DOOR_DIMENSION_M
        ));
    }
    if door.swing_degrees.abs() < crate::engine::level::MIN_DOOR_SWING_DEGREES
        || door.swing_degrees.abs() > crate::engine::level::MAX_DOOR_SWING_DEGREES
    {
        return Err(format!(
            "Door {i} swing_degrees must be between {} and {} degrees",
            crate::engine::level::MIN_DOOR_SWING_DEGREES,
            crate::engine::level::MAX_DOOR_SWING_DEGREES
        ));
    }
    if door.open_speed_degrees <= 0.0
        || door.open_speed_degrees > crate::engine::level::MAX_DOOR_SPEED_DEGREES
    {
        return Err(format!(
            "Door {i} open_speed_degrees must be between 0 and {}",
            crate::engine::level::MAX_DOOR_SPEED_DEGREES
        ));
    }
    if let Some(close) = door.close_speed_degrees
        && (!close.is_finite()
            || close <= 0.0
            || close > crate::engine::level::MAX_DOOR_SPEED_DEGREES)
    {
        return Err(format!(
            "Door {i} close_speed_degrees must be between 0 and {}",
            crate::engine::level::MAX_DOOR_SPEED_DEGREES
        ));
    }
    if let Some(depth) = door.frame_depth
        && (!depth.is_finite()
            || depth <= 0.0
            || depth > crate::engine::level::MAX_DOOR_FRAME_DEPTH_M)
    {
        return Err(format!(
            "Door {i} frame_depth must be between 0 and {} m",
            crate::engine::level::MAX_DOOR_FRAME_DEPTH_M
        ));
    }
    if let Some(center) = door.frame_center {
        let depth = door.frame_depth.ok_or_else(|| {
            format!("Door {i} frame_center needs a frame_depth to be measured inside")
        })?;
        if !center.is_finite() || center.abs() > depth {
            return Err(format!(
                "Door {i} frame_center must be within ±frame_depth ({depth} m)"
            ));
        }
    }
    // A door's authored prompt and reach now live in its `interactable`
    // component and are validated with every other component by
    // [`validate_instance_ids`].
    Ok(())
}

/// The closed leaf must be clear of solid geometry.
///
/// The hinge, the leaf centre and the latch edge are sampled: this is the
/// authoring mistake that matters (a leaf inside a wall or a closed cabinet).
/// A full sweep belongs to the runtime, not the loader.
fn validate_door_leaf_is_clear(
    i: usize,
    door: &crate::engine::level::DoorDef,
    level: &LevelDef,
    solids: &[crate::engine::collision::WallAabb],
) -> Result<(), String> {
    let base = door.base_y(level);
    let (dx, dz) = door.closed_direction();
    let half = door.width * 0.5;
    let samples = [
        (door.x, door.z),
        (dx.mul_add(half, door.x), dz.mul_add(half, door.z)),
        (
            dx.mul_add(door.width, door.x),
            dz.mul_add(door.width, door.z),
        ),
    ];
    for (sx, sz) in samples {
        for solid in solids {
            if solid.blocks_body(base, door.height)
                && sx > solid.min_x
                && sx < solid.max_x
                && sz > solid.min_z
                && sz < solid.max_z
            {
                return Err(format!(
                    "Door {i} leaf starts inside solid geometry at ({sx:.2}, {sz:.2}); \
                     cut a wall opening for it"
                ));
            }
        }
    }
    Ok(())
}
