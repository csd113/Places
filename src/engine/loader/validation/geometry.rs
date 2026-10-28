//! Authored geometry and generated surface admission.

use crate::engine::level::{
    LevelDef, LevelSurfaces, MAX_LEVEL_FLOOR_AREA_M2, MAX_LEVEL_VERTICES, WALL_SLICE_EPS, WallAxis,
};

/// Per-room dimensions, floor elevation and ceiling profile.
pub(super) fn validate_rooms(level: &LevelDef) -> Result<(), String> {
    for (i, r) in level.room_iter().enumerate() {
        if !r.width.is_finite() || !r.depth.is_finite() || !r.height.is_finite() {
            return Err(format!("Room {i} dimensions must be finite numbers"));
        }
        if r.width <= 0.0 || r.depth <= 0.0 || r.height <= 0.0 {
            return Err(format!(
                "Room {i} width, depth, and height must be positive"
            ));
        }
        if r.width > crate::engine::level::MAX_ROOM_EXTENT_M
            || r.depth > crate::engine::level::MAX_ROOM_EXTENT_M
            || r.height > crate::engine::level::MAX_ROOM_HEIGHT_M
        {
            return Err(format!(
                "Room {i} dimensions exceed maximum limits (max {}x{}x{}m)",
                crate::engine::level::MAX_ROOM_EXTENT_M,
                crate::engine::level::MAX_ROOM_EXTENT_M,
                crate::engine::level::MAX_ROOM_HEIGHT_M
            ));
        }
        // Vertical geometry: the room's floor elevation and ceiling profile.
        // A non-finite elevation would push every derived surface out of the
        // world, and a ridge at or below the eave is not a gable at all.
        if !r.floor_y.is_finite() || !(r.floor_y + r.height).is_finite() {
            return Err(format!("Room {i} floor elevation must be a finite number"));
        }
        if let crate::engine::level::CeilingProfileDef::Gable {
            ridge_rise,
            ridge: _,
        } = r.ceiling
        {
            if !ridge_rise.is_finite() {
                return Err(format!(
                    "Room {i} ceiling ridge rise must be a finite number"
                ));
            }
            if ridge_rise <= 0.0 {
                return Err(format!(
                    "Room {i} ceiling ridge rise must be above the eave (got {ridge_rise} m)"
                ));
            }
            if ridge_rise > 50.0 {
                return Err(format!(
                    "Room {i} ceiling ridge rise exceeds the maximum limit (max 50 m)"
                ));
            }
            if !(r.floor_y + r.height + ridge_rise).is_finite() {
                return Err(format!(
                    "Room {i} ceiling ridge height must be a finite number"
                ));
            }
        }
        // The optional ceiling tile frame: a non-finite origin or rotation
        // would make every ceiling UV and decal snap on that room undefined.
        if let Some([tile_x, tile_z]) = r.ceiling_tile_origin
            && (!tile_x.is_finite() || !tile_z.is_finite())
        {
            return Err(format!(
                "Room {i} ceiling tile origin must be finite world coordinates"
            ));
        }
        if r.ceiling_tile_rotation_degrees
            .is_some_and(|rotation| !rotation.is_finite())
        {
            return Err(format!(
                "Room {i} ceiling tile rotation must be a finite number of degrees"
            ));
        }
    }
    Ok(())
}

/// Every per-surface `shine` override a level authors must be a unit value.
///
/// Shine is the author-facing glossiness (`0.0` matte .. `1.0` extremely
/// glossy). A malformed value is a level error rather than a silent clamp: a
/// surface that was meant to be matte and renders glossy (or the reverse) is
/// exactly the kind of mistake the loader exists to surface. A level that
/// authors no shine passes unchanged.
pub(super) fn validate_surface_shine(level: &LevelDef) -> Result<(), String> {
    let check = |label: &str, shine: Option<f32>| -> Result<(), String> {
        let Some(coefficient) = shine else {
            return Ok(());
        };
        if !coefficient.is_finite() || !(0.0..=1.0).contains(&coefficient) {
            return Err(format!(
                "{label} shine must be a finite number between 0.0 and 1.0, found {coefficient:?}"
            ));
        }
        Ok(())
    };
    check("Level default wall", level.defaults.wall_shine)?;
    check("Level default floor", level.defaults.floor_shine)?;
    check("Level default ceiling", level.defaults.ceiling_shine)?;
    for (i, room) in level.room_iter().enumerate() {
        check(&format!("Room {i} floor"), room.shine)?;
        check(&format!("Room {i} ceiling"), room.ceiling_shine)?;
    }
    for (i, wall) in level.walls.iter().enumerate() {
        check(&format!("Wall {i}"), wall.shine)?;
        // Stable order also makes the first rejected face reproducible.
        let mut faces: Vec<_> = wall.face_shine.iter().collect();
        faces.sort_unstable_by_key(|(face, _)| *face);
        for (face, shine) in faces {
            check(&format!("Wall {i} face `{face}`"), Some(*shine))?;
        }
        for (j, opening) in wall.openings.iter().enumerate() {
            check(&format!("Wall {i} opening {j} glass"), opening.glass_shine)?;
        }
    }
    for (i, patch) in level.floor_patches.iter().enumerate() {
        check(&format!("Floor patch {i}"), patch.shine)?;
    }
    for (i, region) in level.floor_regions.iter().enumerate() {
        check(&format!("Floor region {i} floor"), region.shine)?;
        check(&format!("Floor region {i} edge"), region.edge_shine)?;
    }
    for (i, piece) in level.arc_walls.iter().enumerate() {
        check(&format!("Arc wall {i}"), piece.shine)?;
        check(&format!("Arc wall {i} inner"), piece.inner_shine)?;
        check(&format!("Arc wall {i} outer"), piece.outer_shine)?;
        check(&format!("Arc wall {i} cap"), piece.cap_shine)?;
        check(&format!("Arc wall {i} end"), piece.end_shine)?;
    }
    for (i, piece) in level.pillars.iter().enumerate() {
        check(&format!("Pillar {i}"), piece.shine)?;
        check(&format!("Pillar {i} cap"), piece.cap_shine)?;
    }
    Ok(())
}

/// Local floor regions: position, size, materials and room containment.
pub(super) fn validate_floor_regions(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.floor_regions.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_FLOOR_REGIONS
    {
        return Err(format!(
            "Level contains too many floor regions: {} (limit: {})",
            level.floor_regions.len(),
            crate::engine::level::MAX_LEVEL_FLOOR_REGIONS
        ));
    }
    for (i, region) in level.floor_regions.iter().enumerate() {
        if !region.x.is_finite()
            || !region.z.is_finite()
            || !region.width.is_finite()
            || !region.depth.is_finite()
            || !region.offset_y.is_finite()
        {
            return Err(format!(
                "Floor region {i} position, size, and offset must be finite numbers"
            ));
        }
        if region.width <= 0.0 || region.depth <= 0.0 {
            return Err(format!("Floor region {i} width and depth must be positive"));
        }
        if region
            .material
            .as_deref()
            .is_some_and(|material| material.trim().is_empty())
            || region
                .edge_material
                .as_deref()
                .is_some_and(|material| material.trim().is_empty())
        {
            return Err(format!(
                "Floor region {i} materials must be non-empty ids when specified"
            ));
        }

        // A region has to describe a floor inside a room: one that overlaps
        // nothing is a typo, and one whose surface is at or above the room's
        // ceiling has no interior volume to stand in.
        let (rx0, rx1, rz0, rz1) = region.bounds();
        let mut overlaps_room = false;
        for (room_index, room) in level.room_iter().enumerate() {
            let (x0, x1, z0, z1) = room.bounds();
            if rx1 <= x0 || rx0 >= x1 || rz1 <= z0 || rz0 >= z1 {
                continue;
            }
            overlaps_room = true;
            let floor = room.floor_y + region.offset();
            if !floor.is_finite() || floor >= room.eave_y() {
                return Err(format!(
                    "Floor region {i} sits at or above the ceiling of room {room_index} \
                     ({floor:.2} m vs eave {:.2} m)",
                    room.eave_y()
                ));
            }
        }
        if !overlaps_room {
            return Err(format!("Floor region {i} lies outside every room section"));
        }
    }
    Ok(())
}

/// Water volumes: position, shape, size/radius, surface, material and depth.
///
/// A rectangle requires `width`/`depth`; a circle requires `radius` and may
/// author `width`/`depth` only as its own bounding box (`2 * radius`), so a
/// contradictory record can never make the drawn disc and the membership test
/// disagree. A volume whose surface sits at or below the floor beneath it is a
/// typo the author has to see (the water would be hidden inside the geometry),
/// so the walkable floor is sampled inside the volume's *own* footprint — the
/// rectangle, or the disc's interior rather than its bounding box corners —
/// and compared against the authored surface. A volume that overlaps no room
/// is rejected like a floor region that overlaps none.
pub(super) fn validate_water(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.water.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_WATER_VOLUMES
    {
        return Err(format!(
            "Level contains too many water volumes: {} (limit: {})",
            level.water.len(),
            crate::engine::level::MAX_LEVEL_WATER_VOLUMES
        ));
    }
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    for (i, volume) in level.water.iter().enumerate() {
        validate_water_shape(i, volume)?;
        validate_water_contract(i, volume)?;
        validate_water_footprint(i, volume, &surfaces)?;
    }
    Ok(())
}

/// One water volume's shape contract: finite position and surface, and the
/// matching `width`/`depth` or `radius` for its shape.
fn validate_water_shape(
    i: usize,
    volume: &crate::engine::level::WaterVolumeDef,
) -> Result<(), String> {
    use crate::engine::level::WaterShape;

    if !volume.x.is_finite() || !volume.z.is_finite() || !volume.surface_y.is_finite() {
        return Err(format!(
            "Water volume {i} position and surface must be finite numbers"
        ));
    }
    match volume.shape {
        WaterShape::Rect => {
            let Some(width) = volume.width else {
                return Err(format!(
                    "Water volume {i} is a rectangle and must author width"
                ));
            };
            let Some(depth) = volume.depth else {
                return Err(format!(
                    "Water volume {i} is a rectangle and must author depth"
                ));
            };
            if !width.is_finite() || width <= 0.0 || !depth.is_finite() || depth <= 0.0 {
                return Err(format!(
                    "Water volume {i} width and depth must be finite and positive"
                ));
            }
            if volume.radius.is_some() {
                return Err(format!(
                    "Water volume {i} authors radius on a rectangle; use \
                     \"shape\": \"circle\" for a circular pool"
                ));
            }
        }
        WaterShape::Circle => {
            let Some(radius) = volume.radius else {
                return Err(format!(
                    "Water volume {i} is a circle and must author radius"
                ));
            };
            if !radius.is_finite() || radius <= 0.0 {
                return Err(format!(
                    "Water volume {i} radius must be a finite number greater than 0"
                ));
            }
            // The bounding box is a derivation, never a second source of
            // truth: a circle may author `width`/`depth` only as its own
            // diameter (a generator that stamps the box verbatim), and a
            // mismatch is rejected by name.
            let diameter = 2.0 * radius;
            for (name, value) in [("width", volume.width), ("depth", volume.depth)] {
                if let Some(dimension) = value
                    && (!dimension.is_finite() || (dimension - diameter).abs() > 1.0e-4)
                {
                    return Err(format!(
                        "Water volume {i} {name} ({dimension:?}) must be absent or equal \
                         2 * radius ({diameter:?}); a circle's bounding box is derived"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// One water volume's material/opacity/depth contract.
fn validate_water_contract(
    i: usize,
    volume: &crate::engine::level::WaterVolumeDef,
) -> Result<(), String> {
    if volume
        .material
        .as_deref()
        .is_some_and(|material| material.trim().is_empty())
    {
        return Err(format!(
            "Water volume {i} material must be a non-empty id when specified"
        ));
    }
    if let Some(opacity) = volume.opacity
        && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
    {
        return Err(format!(
            "Water volume {i} opacity must be a finite number between 0.0 and 1.0"
        ));
    }
    if !volume.attenuation_per_metre.is_finite()
        || !(0.0..=16.0).contains(&volume.attenuation_per_metre)
    {
        return Err(format!(
            "Water volume {i} attenuation_per_metre must be finite in 0..=16"
        ));
    }
    if !volume.motion.is_valid() {
        return Err(format!(
            "Water volume {i} motion requires finite speed in 0..=8, strength in 0..=1, flow_speed in 0..=4 m/s and direction_degrees in 0..=360"
        ));
    }
    if !volume.caustics.is_valid() {
        return Err(format!(
            "Water volume {i} caustics requires finite tint channels in 0..=1, speed in 0..=8 and strength in 0..=1"
        ));
    }
    if let Some(bottom) = volume.bottom_y
        && (!bottom.is_finite() || bottom >= volume.surface_y)
    {
        return Err(format!(
            "Water volume {i} bottom_y must be finite and below its surface_y"
        ));
    }
    Ok(())
}

/// One water volume's non-empty footprint and its overlap with a room.
///
/// The floor samples are the volume's own footprint: a rectangle's centre and
/// corners (inset off a room seam), or a circle's centre and four interior
/// points, so a bounding box corner the disc does not cover never decides
/// anything.
fn validate_water_footprint(
    i: usize,
    volume: &crate::engine::level::WaterVolumeDef,
    surfaces: &crate::engine::level::LevelSurfaces<'_>,
) -> Result<(), String> {
    use crate::engine::level::WaterShape;

    let (x0, x1, z0, z1) = volume.bounds();
    if x1 <= x0 || z1 <= z0 {
        return Err(format!(
            "Water volume {i} has an empty footprint; its width/depth or radius \
             must describe a positive extent"
        ));
    }
    let mut overlaps_room = false;
    let mut samples: [(f32, f32); 5] = match volume.shape {
        WaterShape::Rect => [
            (f32::midpoint(x0, x1), f32::midpoint(z0, z1)),
            (x0, z0),
            (x1, z0),
            (x1, z1),
            (x0, z1),
        ],
        WaterShape::Circle => {
            let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
            // The centre and four interior points at half the radius: the
            // disc's own footprint, not the bounding box corners a square
            // would wrongly include.
            let offset = volume.radius.unwrap_or(0.0) * 0.353_553_4;
            [
                (cx, cz),
                (cx - offset, cz - offset),
                (cx + offset, cz - offset),
                (cx + offset, cz + offset),
                (cx - offset, cz + offset),
            ]
        }
    };
    if volume.shape == WaterShape::Rect {
        // Inset the corner samples so a volume that shares an edge with a
        // room boundary is not rejected by floating-point noise on the seam.
        // The inset is scaled down for a very small footprint so the clamp
        // bounds can never invert (which would panic).
        let inset_x = 1.0e-3_f32.min((x1 - x0) * 0.25);
        let inset_z = 1.0e-3_f32.min((z1 - z0) * 0.25);
        for (x, z) in &mut samples {
            *x = x.clamp(x0 + inset_x, x1 - inset_x);
            *z = z.clamp(z0 + inset_z, z1 - inset_z);
        }
    }
    for (x, z) in samples {
        let Some(floor) = surfaces.floor_y_at(x, z) else {
            continue;
        };
        overlaps_room = true;
        if floor > volume.surface_y + 1.0e-2 {
            return Err(format!(
                "Water volume {i} surface ({:.2} m) is below the floor at ({x:.2}, {z:.2}) \
                 ({floor:.2} m); raise surface_y above the floor it covers",
                volume.surface_y
            ));
        }
    }
    if !overlaps_room {
        return Err(format!("Water volume {i} lies outside every room section"));
    }
    Ok(())
}

/// Climbable ladder volumes: finite footprint and reach, positive size, top
/// above bottom, and a footprint that overlaps a room section.
///
/// A ladder is the space the player climbs through, not the prop that draws the
/// rails, so the checks mirror the water volumes: the footprint must be real
/// geometry the player can reach, and a top at or below the bottom is a typo
/// the author has to see rather than an inert climb volume.
pub(super) fn validate_ladders(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.ladders.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_LADDERS
    {
        return Err(format!(
            "Level contains too many ladders: {} (limit: {})",
            level.ladders.len(),
            crate::engine::level::MAX_LEVEL_LADDERS
        ));
    }
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    for (i, ladder) in level.ladders.iter().enumerate() {
        if !ladder.x.is_finite()
            || !ladder.z.is_finite()
            || !ladder.width.is_finite()
            || !ladder.depth.is_finite()
            || !ladder.bottom_y.is_finite()
            || !ladder.top_y.is_finite()
            || !ladder.facing_degrees.is_finite()
        {
            return Err(format!(
                "Ladder {i} position, size, reach and facing must be finite numbers"
            ));
        }
        if ladder.width <= 0.0 || ladder.depth <= 0.0 {
            return Err(format!("Ladder {i} width and depth must be positive"));
        }
        if ladder.top_y <= ladder.bottom_y {
            return Err(format!(
                "Ladder {i} top_y ({:.2} m) must be above its bottom_y ({:.2} m)",
                ladder.top_y, ladder.bottom_y
            ));
        }
        let (x0, x1, z0, z1) = ladder.bounds();
        let mut overlaps_room = false;
        let mut samples: [(f32, f32); 5] = [
            (f32::midpoint(x0, x1), f32::midpoint(z0, z1)),
            (x0, z0),
            (x1, z0),
            (x1, z1),
            (x0, z1),
        ];
        // The inset is scaled down for a very small footprint so the clamp
        // bounds can never invert (which would panic).
        let inset_x = 1.0e-3_f32.min((x1 - x0) * 0.25);
        let inset_z = 1.0e-3_f32.min((z1 - z0) * 0.25);
        for (x, z) in &mut samples {
            *x = x.clamp(x0 + inset_x, x1 - inset_x);
            *z = z.clamp(z0 + inset_z, z1 - inset_z);
        }
        for (x, z) in samples {
            if surfaces.floor_y_at(x, z).is_some() {
                overlaps_room = true;
                break;
            }
        }
        if !overlaps_room {
            return Err(format!("Ladder {i} lies outside every room section"));
        }
    }
    Ok(())
}

/// The generic architectural pieces: ramps, staircases, half walls, columns,
/// archways, guardrails, thresholds and baseboards.
///
/// Every piece is validated on the same contract its geometry is built from:
/// finite dimensions, materials that are non-empty when authored, an overlap
/// with a room where the piece is a walking surface, and — for ramps and
/// staircases — a slope or riser the player controller can actually climb.
/// Invalid dimensions are named errors, never silently clamped geometry.
pub(super) fn validate_architecture(level: &LevelDef) -> Result<(), String> {
    validate_ramps(level)?;
    validate_stairs(level)?;
    validate_half_walls(level)?;
    validate_columns(level)?;
    validate_arc_walls(level)?;
    validate_pillars(level)?;
    validate_archways(level)?;
    validate_guardrails(level)?;
    validate_thresholds(level)?;
    validate_baseboards(level)?;
    crate::map_compiler::trim::validate_attachments(level)?;
    validate_architecture_overlaps(level)
}

/// True when an axis-aligned rectangle overlaps any room's footprint.
pub(super) fn rect_overlaps_room(level: &LevelDef, bounds: (f32, f32, f32, f32)) -> bool {
    let (x0, x1, z0, z1) = bounds;
    level.room_iter().any(|room| {
        let (rx0, rx1, rz0, rz1) = room.bounds();
        x1 > rx0 && x0 < rx1 && z1 > rz0 && z0 < rz1
    })
}

/// True when two axis-aligned rectangles overlap by a real area.
fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.1 > b.0 && a.0 < b.1 && a.3 > b.2 && a.2 < b.3
}

/// True when an optional material id is present but blank.
fn blank_material(material: Option<&str>) -> bool {
    material.is_some_and(|id| id.trim().is_empty())
}

/// Ramps: a walkable slope inside a room, shallow enough to climb.
fn validate_ramps(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.ramps.len()).unwrap_or(u64::MAX) > crate::engine::level::MAX_LEVEL_RAMPS
    {
        return Err(format!(
            "Level contains too many ramps: {} (limit: {})",
            level.ramps.len(),
            crate::engine::level::MAX_LEVEL_RAMPS
        ));
    }
    for (i, ramp) in level.ramps.iter().enumerate() {
        if !ramp.x.is_finite()
            || !ramp.z.is_finite()
            || !ramp.width.is_finite()
            || !ramp.depth.is_finite()
            || !ramp.offset_y.is_finite()
            || !ramp.rise.is_finite()
        {
            return Err(format!(
                "Ramp {i} position, size, offset and rise must be finite numbers"
            ));
        }
        if ramp.width <= 0.0 || ramp.depth <= 0.0 {
            return Err(format!("Ramp {i} width and depth must be positive"));
        }
        if ramp.rise.abs() <= 1e-3 {
            return Err(format!(
                "Ramp {i} has no rise; use a floor region for a flat material change"
            ));
        }
        if ramp.rise.abs() > crate::engine::level::MAX_RAMP_RISE_M {
            return Err(format!(
                "Ramp {i} rise exceeds the maximum of {} m",
                crate::engine::level::MAX_RAMP_RISE_M
            ));
        }
        let length = ramp.length();
        if ramp.rise.abs() > crate::engine::level::MAX_RAMP_SLOPE * length {
            return Err(format!(
                "Ramp {i} is too steep to walk: {:.2} m of rise over {:.2} m of run \
                 (limit {} m per metre)",
                ramp.rise.abs(),
                length,
                crate::engine::level::MAX_RAMP_SLOPE
            ));
        }
        if blank_material(ramp.material.as_deref()) || blank_material(ramp.edge_material.as_deref())
        {
            return Err(format!(
                "Ramp {i} materials must be non-empty ids when specified"
            ));
        }
        if !rect_overlaps_room(level, ramp.bounds()) {
            return Err(format!("Ramp {i} lies outside every room section"));
        }
        // The ramp's high end has to stay under the ceiling of every room it
        // crosses, or the walking surface would pass through the ceiling. Every
        // room it crosses must also share one floor plane: the mesh and the
        // lightmap are generated once, from the room under the ramp's centre,
        // while the walkable surface resolves each room's own floor.
        let mut ramp_floor: Option<f32> = None;
        for (room_index, room) in level.room_iter().enumerate() {
            if !rects_overlap(ramp.bounds(), room.bounds()) {
                continue;
            }
            let high = room.floor_y + ramp.high_offset();
            if !high.is_finite() || high >= room.eave_y() {
                return Err(format!(
                    "Ramp {i} rises to or above the ceiling of room {room_index} \
                     ({high:.2} m vs eave {:.2} m)",
                    room.eave_y()
                ));
            }
            match ramp_floor {
                None => ramp_floor = Some(room.floor_y),
                Some(floor) if (floor - room.floor_y).abs() > 1.0e-4 => {
                    return Err(format!(
                        "Ramp {i} spans rooms with different floors ({floor:.2} m vs \
                         {:.2} m in room {room_index}); the ramp is drawn on one floor \
                         plane, so every room it crosses must share it",
                        room.floor_y
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Staircases: climeable risers, usable treads, and a top tread that stays
/// under the ceiling.
fn validate_stairs(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.stairs.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_STAIRS
    {
        return Err(format!(
            "Level contains too many staircases: {} (limit: {})",
            level.stairs.len(),
            crate::engine::level::MAX_LEVEL_STAIRS
        ));
    }
    for (i, stair) in level.stairs.iter().enumerate() {
        if !stair.x.is_finite()
            || !stair.z.is_finite()
            || !stair.width.is_finite()
            || !stair.depth.is_finite()
            || !stair.offset_y.is_finite()
            || !stair.rise.is_finite()
        {
            return Err(format!(
                "Staircase {i} position, size, offset and rise must be finite numbers"
            ));
        }
        if stair.width <= 0.0 || stair.depth <= 0.0 {
            return Err(format!("Staircase {i} width and depth must be positive"));
        }
        if stair.step_count() < 2 {
            return Err(format!(
                "Staircase {i} needs at least 2 steps (found {})",
                stair.step_count()
            ));
        }
        if stair.rise() <= 0.0 {
            return Err(format!("Staircase {i} rise must be positive"));
        }
        if stair.rise() > crate::engine::level::MAX_RAMP_RISE_M {
            return Err(format!(
                "Staircase {i} rise exceeds the maximum of {} m",
                crate::engine::level::MAX_RAMP_RISE_M
            ));
        }
        let riser = stair.riser_height();
        if riser > crate::engine::level::MAX_STAIR_RISER_M + 1e-4 {
            return Err(format!(
                "Staircase {i} riser is {riser:.2} m, taller than the {:.2} m walkable step; \
                 add steps or reduce the rise",
                crate::engine::level::MAX_STAIR_RISER_M
            ));
        }
        let tread = stair.tread_depth();
        if tread < crate::engine::level::MIN_STAIR_TREAD_M {
            return Err(format!(
                "Staircase {i} tread is {tread:.2} m, shallower than the {} m minimum",
                crate::engine::level::MIN_STAIR_TREAD_M
            ));
        }
        if blank_material(stair.material.as_deref())
            || blank_material(stair.riser_material.as_deref())
            || blank_material(stair.side_material.as_deref())
        {
            return Err(format!(
                "Staircase {i} materials must be non-empty ids when specified"
            ));
        }
        if !rect_overlaps_room(level, stair.bounds()) {
            return Err(format!("Staircase {i} lies outside every room section"));
        }
        // Every room the flight crosses must share one floor plane: the mesh is
        // generated once from the room under the flight's centre, while the
        // walkable surface resolves each room's own floor.
        let mut stair_floor: Option<f32> = None;
        for (room_index, room) in level.room_iter().enumerate() {
            let (sx0, sx1, sz0, sz1) = stair.bounds();
            let (rx0, rx1, rz0, rz1) = room.bounds();
            if sx1.min(rx1) - sx0.max(rx0) <= 1.0e-4 || sz1.min(rz1) - sz0.max(rz0) <= 1.0e-4 {
                continue;
            }
            let top = room.floor_y + stair.top_offset();
            if !top.is_finite() || top >= room.eave_y() {
                return Err(format!(
                    "Staircase {i} climbs to or above the ceiling of room {room_index} \
                     ({top:.2} m vs eave {:.2} m)",
                    room.eave_y()
                ));
            }
            match stair_floor {
                None => stair_floor = Some(room.floor_y),
                Some(floor) if (floor - room.floor_y).abs() > 1.0e-4 => {
                    return Err(format!(
                        "Staircase {i} spans rooms with different floors ({floor:.2} m vs \
                         {:.2} m in room {room_index}); the flight is drawn on one floor \
                         plane, so every room it crosses must share it",
                        room.floor_y
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Half walls: an authored height and usable footprint.
fn validate_half_walls(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.half_walls.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_HALF_WALLS
    {
        return Err(format!(
            "Level contains too many half walls: {} (limit: {})",
            level.half_walls.len(),
            crate::engine::level::MAX_LEVEL_HALF_WALLS
        ));
    }
    for (i, piece) in level.half_walls.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.width.is_finite()
            || !piece.depth.is_finite()
            || !piece.height.is_finite()
        {
            return Err(format!(
                "Half wall {i} position and dimensions must be finite numbers"
            ));
        }
        if piece.width <= 0.0 || piece.depth <= 0.0 || piece.height <= 0.0 {
            return Err(format!(
                "Half wall {i} width, depth and height must be positive"
            ));
        }
        if piece.height > 50.0 {
            return Err(format!("Half wall {i} height exceeds the maximum of 50 m"));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Half wall {i} base height must be a finite number"));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.end_material.as_deref())
            || blank_material(piece.cap_material.as_deref())
        {
            return Err(format!(
                "Half wall {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Columns: a solid post with an optional authored height.
fn validate_columns(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.columns.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_COLUMNS
    {
        return Err(format!(
            "Level contains too many columns: {} (limit: {})",
            level.columns.len(),
            crate::engine::level::MAX_LEVEL_COLUMNS
        ));
    }
    for (i, piece) in level.columns.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.width.is_finite()
            || !piece.depth.is_finite()
        {
            return Err(format!(
                "Column {i} position and dimensions must be finite numbers"
            ));
        }
        if piece.width <= 0.0 || piece.depth <= 0.0 {
            return Err(format!("Column {i} width and depth must be positive"));
        }
        if let Some(height) = piece.height
            && (!height.is_finite() || height <= 0.0)
        {
            return Err(format!(
                "Column {i} height must be a positive finite number when authored"
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Column {i} base height must be a finite number"));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.cap_material.as_deref())
        {
            return Err(format!(
                "Column {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Arc walls: a curved solid slab whose radii, sweep and tessellation are all
/// usable, with named diagnostics for every degenerate dimension.
fn validate_arc_walls(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.arc_walls.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_ARC_WALLS
    {
        return Err(format!(
            "Level contains too many arc walls: {} (limit: {})",
            level.arc_walls.len(),
            crate::engine::level::MAX_LEVEL_ARC_WALLS
        ));
    }
    for (i, piece) in level.arc_walls.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.radius.is_finite()
            || !piece.thickness.is_finite()
        {
            return Err(format!(
                "Arc wall {i} centre, radius and thickness must be finite numbers"
            ));
        }
        if piece.radius <= 0.0 {
            return Err(format!(
                "Arc wall {i} radius must be positive (found {})",
                piece.radius
            ));
        }
        if piece.thickness <= 0.0 || piece.thickness >= piece.radius * 2.0 {
            return Err(format!(
                "Arc wall {i} thickness must be positive and thinner than twice its radius \
                 (radius {}, thickness {})",
                piece.radius, piece.thickness
            ));
        }
        if !piece.start_degrees.is_finite() || !piece.sweep_degrees.is_finite() {
            return Err(format!(
                "Arc wall {i} start and sweep angles must be finite numbers"
            ));
        }
        if piece.sweep_degrees.abs() <= 1.0e-3 || piece.sweep_degrees.abs() > 360.0 + 1.0e-3 {
            return Err(format!(
                "Arc wall {i} sweep must be a non-zero angle up to 360 degrees (found {})",
                piece.sweep_degrees
            ));
        }
        if let Some(segments) = piece.segments
            && !(crate::engine::level::ROUND_SEGMENTS_MIN
                ..=crate::engine::level::ROUND_SEGMENTS_MAX)
                .contains(&segments)
        {
            return Err(format!(
                "Arc wall {i} segments must be between {} and {}, found {segments}",
                crate::engine::level::ROUND_SEGMENTS_MIN,
                crate::engine::level::ROUND_SEGMENTS_MAX
            ));
        }
        if let Some(height) = piece.height
            && (!height.is_finite() || height <= 0.0)
        {
            return Err(format!(
                "Arc wall {i} height must be a positive finite number when authored"
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!(
                "Arc wall {i} base height must be a finite number when authored"
            ));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.inner_material.as_deref())
            || blank_material(piece.outer_material.as_deref())
            || blank_material(piece.cap_material.as_deref())
            || blank_material(piece.end_material.as_deref())
        {
            return Err(format!(
                "Arc wall {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Circular pillars: a solid round post with a usable radius, height and
/// tessellation.
fn validate_pillars(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.pillars.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_PILLARS
    {
        return Err(format!(
            "Level contains too many pillars: {} (limit: {})",
            level.pillars.len(),
            crate::engine::level::MAX_LEVEL_PILLARS
        ));
    }
    for (i, piece) in level.pillars.iter().enumerate() {
        if !piece.x.is_finite() || !piece.z.is_finite() || !piece.radius.is_finite() {
            return Err(format!(
                "Pillar {i} centre and radius must be finite numbers"
            ));
        }
        if piece.radius <= 0.0 {
            return Err(format!(
                "Pillar {i} radius must be positive (found {})",
                piece.radius
            ));
        }
        if let Some(segments) = piece.segments
            && !(crate::engine::level::ROUND_SEGMENTS_MIN
                ..=crate::engine::level::ROUND_SEGMENTS_MAX)
                .contains(&segments)
        {
            return Err(format!(
                "Pillar {i} segments must be between {} and {}, found {segments}",
                crate::engine::level::ROUND_SEGMENTS_MIN,
                crate::engine::level::ROUND_SEGMENTS_MAX
            ));
        }
        if let Some(height) = piece.height
            && (!height.is_finite() || height <= 0.0)
        {
            return Err(format!(
                "Pillar {i} height must be a positive finite number when authored"
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!(
                "Pillar {i} base height must be a finite number when authored"
            ));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.cap_material.as_deref())
        {
            return Err(format!(
                "Pillar {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Archways: an opening that fits inside its block, with a crown above the
/// springing line.
fn validate_archways(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.archways.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_ARCHWAYS
    {
        return Err(format!(
            "Level contains too many archways: {} (limit: {})",
            level.archways.len(),
            crate::engine::level::MAX_LEVEL_ARCHWAYS
        ));
    }
    for (i, piece) in level.archways.iter().enumerate() {
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.width.is_finite()
            || !piece.depth.is_finite()
            || !piece.height.is_finite()
            || !piece.opening_width.is_finite()
            || !piece.opening_height.is_finite()
            || !piece.arch_rise.is_finite()
        {
            return Err(format!(
                "Archway {i} position and dimensions must be finite numbers"
            ));
        }
        if piece.width <= 0.0 || piece.depth <= 0.0 || piece.height <= 0.0 {
            return Err(format!(
                "Archway {i} width, depth and height must be positive"
            ));
        }
        if piece.opening_width <= 0.0 || piece.opening_height <= 0.0 {
            return Err(format!(
                "Archway {i} opening width and height must be positive"
            ));
        }
        if piece.arch_rise < 0.0 {
            return Err(format!("Archway {i} arch rise cannot be negative"));
        }
        if piece.arch_rise >= piece.opening_height {
            return Err(format!(
                "Archway {i} arch rise ({:.2} m) must be lower than its opening height \
                 ({:.2} m); a flat lintel is `arch_rise: 0`",
                piece.arch_rise, piece.opening_height
            ));
        }
        if piece.height < piece.opening_height {
            return Err(format!(
                "Archway {i} block is shorter than its opening ({:.2} m vs {:.2} m)",
                piece.height, piece.opening_height
            ));
        }
        if piece.height > 50.0 {
            return Err(format!("Archway {i} height exceeds the maximum of 50 m"));
        }
        let length = piece.length();
        let minimum_pier = crate::engine::level::ARCHWAY_MIN_PIER_M;
        if piece.opening_width > (-2.0f32).mul_add(minimum_pier, length) {
            return Err(format!(
                "Archway {i} opening is too wide for its block: {:.2} m opening in a {:.2} m \
                 block (each pier needs at least {:.2} m)",
                piece.opening_width, length, minimum_pier
            ));
        }
        if piece.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Archway {i} base height must be a finite number"));
        }
        if blank_material(piece.material.as_deref())
            || blank_material(piece.reveal_material.as_deref())
        {
            return Err(format!(
                "Archway {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Guardrails: a sane rail height, post spacing and slope.
fn validate_guardrails(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.guardrails.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_GUARDRAILS
    {
        return Err(format!(
            "Level contains too many guardrails: {} (limit: {})",
            level.guardrails.len(),
            crate::engine::level::MAX_LEVEL_GUARDRAILS
        ));
    }
    for (i, rail) in level.guardrails.iter().enumerate() {
        if !rail.x.is_finite()
            || !rail.z.is_finite()
            || !rail.length.is_finite()
            || !rail.rotation_degrees.is_finite()
            || !rail.height.is_finite()
            || rail.rise.is_some_and(|rise| !rise.is_finite())
            || !rail.post_spacing.is_finite()
        {
            return Err(format!(
                "Guardrail {i} position and dimensions must be finite numbers"
            ));
        }
        if rail.length <= 0.0 {
            return Err(format!("Guardrail {i} length must be positive"));
        }
        if !(0.2..=2.0).contains(&rail.height) {
            return Err(format!(
                "Guardrail {i} height must be between 0.2 and 2.0 m (got {:.2} m)",
                rail.height
            ));
        }
        if !(0.2..=3.0).contains(&rail.post_spacing) {
            return Err(format!(
                "Guardrail {i} post spacing must be between 0.2 and 3.0 m (got {:.2} m)",
                rail.post_spacing
            ));
        }
        if rail.rise().abs() > crate::engine::level::MAX_RAMP_SLOPE * rail.length {
            return Err(format!(
                "Guardrail {i} slopes too steeply: {:.2} m of rise over {:.2} m of run",
                rail.rise().abs(),
                rail.length
            ));
        }
        if rail.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Guardrail {i} base height must be a finite number"));
        }
        if blank_material(rail.material.as_deref()) || blank_material(rail.post_material.as_deref())
        {
            return Err(format!(
                "Guardrail {i} materials must be non-empty ids when specified"
            ));
        }
    }
    Ok(())
}

/// Threshold strips: a small, floor-hugging trim piece over a level floor.
fn validate_thresholds(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.thresholds.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_THRESHOLDS
    {
        return Err(format!(
            "Level contains too many thresholds: {} (limit: {})",
            level.thresholds.len(),
            crate::engine::level::MAX_LEVEL_THRESHOLDS
        ));
    }
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    for (i, strip) in level.thresholds.iter().enumerate() {
        if !strip.x.is_finite()
            || !strip.z.is_finite()
            || !strip.length.is_finite()
            || !strip.thickness.is_finite()
            || !strip.height.is_finite()
            || !strip.rotation_degrees.is_finite()
        {
            return Err(format!(
                "Threshold {i} position and dimensions must be finite numbers"
            ));
        }
        if strip.length <= 0.0 {
            return Err(format!("Threshold {i} length must be positive"));
        }
        if !(0.02..=0.5).contains(&strip.thickness) {
            return Err(format!(
                "Threshold {i} thickness must be between 0.02 and 0.5 m (got {:.3} m)",
                strip.thickness
            ));
        }
        if !(0.002..=0.05).contains(&strip.height) {
            return Err(format!(
                "Threshold {i} height must be between 0.002 and 0.05 m (got {:.3} m)",
                strip.height
            ));
        }
        if strip.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Threshold {i} base height must be a finite number"));
        }
        if blank_material(strip.material.as_deref()) {
            return Err(format!(
                "Threshold {i} materials must be non-empty ids when specified"
            ));
        }
        // The strip sits on a floor: it must resolve one, and its ends must
        // stand at (essentially) the same height, or half of it floats.
        let Some(centre) = surfaces.floor_y_at(strip.x, strip.z) else {
            return Err(format!("Threshold {i} lies outside every room section"));
        };
        let half_length = strip.length * 0.5;
        let half_thickness = strip.thickness() * 0.5;
        let mut low = centre;
        let mut high = centre;
        for (along, across) in [
            (-half_length, -half_thickness),
            (half_length, -half_thickness),
            (-half_length, half_thickness),
            (half_length, half_thickness),
        ] {
            let (px, pz) = strip.point_at_offset(along, across);
            let Some(y) = surfaces.floor_y_at(px, pz) else {
                return Err(format!(
                    "Threshold {i} spans the point ({px:.2}, {pz:.2}) outside every room section"
                ));
            };
            low = low.min(y);
            high = high.max(y);
        }
        if high - low > 0.05 {
            return Err(format!(
                "Threshold {i} spans a floor height change of {:.2} m; threshold strips \
                 belong on a level floor",
                high - low
            ));
        }
        // A strip buried in a wall's solid (not in one of its openings) is
        // invisible, so it is rejected like a buried baseboard.
        let mid_y = strip.height().mul_add(0.5, centre);
        if let Some(wall) = point_buried_in_wall(level, strip.x, strip.z, mid_y) {
            return Err(format!(
                "Threshold {i} is buried inside wall {wall}: place the strip in the \
                 opening it crosses, not inside the wall solid"
            ));
        }
    }
    Ok(())
}

/// Baseboards: a thin trim run with usable proportions, placed where its front
/// face can actually be seen.
fn validate_baseboards(level: &LevelDef) -> Result<(), String> {
    if u64::try_from(level.baseboards.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_BASEBOARDS
    {
        return Err(format!(
            "Level contains too many baseboards: {} (limit: {})",
            level.baseboards.len(),
            crate::engine::level::MAX_LEVEL_BASEBOARDS
        ));
    }
    for (i, board) in level.baseboards.iter().enumerate() {
        if !board.x.is_finite()
            || !board.z.is_finite()
            || !board.length.is_finite()
            || !board.rotation_degrees.is_finite()
            || !board.height.is_finite()
            || !board.thickness.is_finite()
            || !board.uv_offset.is_finite()
            || board.miters.is_some_and(|ends| {
                ends.into_iter()
                    .any(|value| !value.is_finite() || value.abs() > 8.0)
            })
        {
            return Err(format!(
                "Baseboard {i} position and dimensions must be finite numbers"
            ));
        }
        let (front_start, front_end) = board.span_at(board.thickness());
        if board.length <= 0.0 || front_end <= front_start {
            return Err(format!("Baseboard {i} length must be positive"));
        }
        if !(0.01..=1.0).contains(&board.height) {
            return Err(format!(
                "Baseboard {i} height must be between 0.01 and 1.0 m (got {:.3} m)",
                board.height
            ));
        }
        if !(0.004..=0.2).contains(&board.thickness) {
            return Err(format!(
                "Baseboard {i} thickness must be between 0.004 and 0.2 m (got {:.3} m)",
                board.thickness
            ));
        }
        if board.y.is_some_and(|y| !y.is_finite()) {
            return Err(format!("Baseboard {i} base height must be a finite number"));
        }
        if blank_material(board.material.as_deref()) {
            return Err(format!(
                "Baseboard {i} materials must be non-empty ids when specified"
            ));
        }
        // A board whose whole cross-section lies inside a wall is invisible.
        // The room boundary is the *centre* of the wall that straddles it, so
        // the natural "place the run at x = 0" lands the board inside the wall;
        // the run's back plane belongs on the wall's inner face.
        let (mx, mz) = board.point_at(0.5, board.thickness() * 0.5);
        let mid_y = board
            .height()
            .mul_add(0.5, board.base_y(&LevelSurfaces::new(level)));
        if !board.generated
            && let Some(wall) = point_buried_in_wall(level, mx, mz, mid_y)
        {
            return Err(format!(
                "Baseboard {i} is buried inside wall {wall}: place the run so its back \
                 plane lies on the wall's face (the room edge is the wall's centre plane)"
            ));
        }
    }
    Ok(())
}

/// The index of a wall whose solid contains `(x, z, y)`, openings respected.
///
/// A point exactly on a wall face is *not* inside: every wall boundary is
/// exclusive by [`WALL_SLICE_EPS`], so a surface mounted flush on the face
/// counts as visible. Openings are cut first, so trim floating in a doorway is
/// left alone: it is visible through the hole.
pub fn point_buried_in_wall(level: &LevelDef, x: f32, z: f32, y: f32) -> Option<usize> {
    if !x.is_finite() || !z.is_finite() || !y.is_finite() {
        return None;
    }
    let surfaces = LevelSurfaces::new(level);
    for (index, wall) in level.walls.iter().enumerate() {
        let (x0, x1) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (z0, z1) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        if x <= x0 + WALL_SLICE_EPS
            || x >= x1 - WALL_SLICE_EPS
            || z <= z0 + WALL_SLICE_EPS
            || z >= z1 - WALL_SLICE_EPS
        {
            continue;
        }
        let (origin_x, origin_z) = wall.length_origin();
        let offset = match wall.axis() {
            WallAxis::X => x - origin_x,
            WallAxis::Z => z - origin_z,
        };
        for slice in surfaces.wall_solid_slices(wall) {
            if offset > slice.start + WALL_SLICE_EPS
                && offset < slice.end - WALL_SLICE_EPS
                && y > slice.bottom + WALL_SLICE_EPS
                && y < slice.top - WALL_SLICE_EPS
            {
                return Some(index);
            }
        }
    }
    None
}

/// Cross-piece checks that no single piece can answer on its own: two walking
/// surfaces may not overlap, and a floor region may not be authored inside a
/// ramp or staircase.
fn validate_architecture_overlaps(level: &LevelDef) -> Result<(), String> {
    for (ri, ramp) in level.ramps.iter().enumerate() {
        for (si, stair) in level.stairs.iter().enumerate() {
            if rects_overlap(ramp.bounds(), stair.bounds()) {
                return Err(format!(
                    "Ramp {ri} overlaps staircase {si}; a space has one walking surface"
                ));
            }
        }
        for (fi, region) in level.floor_regions.iter().enumerate() {
            if rects_overlap(ramp.bounds(), region.bounds()) {
                return Err(format!(
                    "Floor region {fi} overlaps ramp {ri}; a ramp is a floor surface itself \
                     and the two cannot share a footprint"
                ));
            }
        }
    }
    for (si, stair) in level.stairs.iter().enumerate() {
        for (fi, region) in level.floor_regions.iter().enumerate() {
            if rects_overlap(stair.bounds(), region.bounds()) {
                return Err(format!(
                    "Floor region {fi} overlaps staircase {si}; a staircase is a floor surface \
                     itself and the two cannot share a footprint"
                ));
            }
        }
    }
    Ok(())
}

/// Wall dimensions and opening cutouts.
pub(super) fn validate_walls(level: &LevelDef) -> Result<(), String> {
    for (i, w) in level.walls.iter().enumerate() {
        if !w.x.is_finite()
            || !w.y.is_finite()
            || !w.z.is_finite()
            || !w.width.is_finite()
            || !w.depth.is_finite()
        {
            return Err(format!("Wall {i} dimensions must be finite numbers"));
        }
        if let Some(h) = w.height {
            if !h.is_finite() {
                return Err(format!("Wall {i} dimensions must be finite numbers"));
            }
            if h <= 0.0 {
                return Err(format!(
                    "Wall {i} width, depth, and height must be positive"
                ));
            }
        }
        if w.width <= 0.0 || w.depth <= 0.0 {
            return Err(format!(
                "Wall {i} width, depth, and height must be positive"
            ));
        }

        // Openings are optional cutouts; openings that do not overlap the
        // wall's vertical range are allowed (they simply produce no cut).
        if w.openings.len() > crate::engine::level::MAX_WALL_OPENINGS {
            return Err(format!(
                "Wall {i} has too many openings: {} (limit: {})",
                w.openings.len(),
                crate::engine::level::MAX_WALL_OPENINGS
            ));
        }
        for (j, opening) in w.openings.iter().enumerate() {
            if !opening.offset.is_finite()
                || !opening.width.is_finite()
                || !opening.height.is_finite()
                || !opening.sill.is_finite()
            {
                return Err(format!("Wall {i} opening {j} contains non-finite numbers"));
            }
            if opening.width <= 0.0 || opening.height <= 0.0 {
                return Err(format!(
                    "Wall {i} opening {j} must have a positive width and height"
                ));
            }
            if opening.sill < 0.0 {
                return Err(format!(
                    "Wall {i} opening {j} cannot have a negative sill height"
                ));
            }
            if opening.offset < 0.0 {
                return Err(format!("Wall {i} opening {j} starts before the wall"));
            }
            if opening.end() > w.length() + 1e-3 {
                let prefix = if opening.is_door() {
                    "Door opening"
                } else if opening.kind == "window" {
                    "Window opening"
                } else {
                    "Opening"
                };
                return Err(format!(
                    "{prefix} extends beyond this wall (wall {i}: opening ends at {:.2} m, wall is {:.2} m long)",
                    opening.end(),
                    w.length()
                ));
            }
        }
    }
    Ok(())
}

/// Decal transforms, sizes and material ids.
pub(super) fn validate_decals(level: &LevelDef) -> Result<(), String> {
    for (i, decal) in level.decals.iter().enumerate() {
        if !decal.x.is_finite()
            || !decal.y.is_finite()
            || !decal.z.is_finite()
            || !decal.rotation_degrees.is_finite()
        {
            return Err(format!(
                "Decal {i} position and rotation must be finite numbers"
            ));
        }
        if !decal.width.is_finite() || !decal.height.is_finite() {
            return Err(format!("Decal {i} size must be finite numbers"));
        }
        if decal.width <= 0.0 || decal.height <= 0.0 {
            return Err(format!("Decal {i} width and height must be positive"));
        }
        if decal.width > crate::engine::level::MAX_DECAL_SIZE_M
            || decal.height > crate::engine::level::MAX_DECAL_SIZE_M
        {
            return Err(format!(
                "Decal {i} is larger than the {} m limit ({} x {})",
                crate::engine::level::MAX_DECAL_SIZE_M,
                decal.width,
                decal.height
            ));
        }
        if decal.material.trim().is_empty() {
            return Err(format!("Decal {i} must reference a non-empty material id"));
        }
    }
    Ok(())
}

/// Decals on horizontal surfaces are snapped to the real surface height, so
/// they follow an elevated room or a recessed region.
///
/// A gable ceiling is a sloped surface and cannot carry a single planar decal,
/// and a decal whose footprint straddles a height change (a recess edge, a room
/// boundary at a different elevation) cannot be projected onto one plane
/// either: both are rejected clearly instead of being drawn at a nonsense
/// height.
pub(super) fn validate_decal_surfaces(level: &LevelDef) -> Result<(), String> {
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    for (i, decal) in level.decals.iter().enumerate() {
        if decal.surface.is_ceiling() && !surfaces.ceiling_is_flat_at(decal.x, decal.z) {
            return Err(format!(
                "Ceiling decal {i} targets a gable ceiling; sloped ceiling decals are not supported"
            ));
        }
        if !decal.surface.is_horizontal() {
            continue;
        }
        let Some(corners) = crate::engine::render::decal_quad_points(decal) else {
            continue;
        };
        let heights = corners.map(|point| match decal.surface {
            crate::engine::level::DecalSurface::Floor => {
                surfaces.floor_y_at(point[0], point[2]).unwrap_or(point[1])
            }
            crate::engine::level::DecalSurface::Ceiling
            | crate::engine::level::DecalSurface::WallNorth
            | crate::engine::level::DecalSurface::WallSouth
            | crate::engine::level::DecalSurface::WallWest
            | crate::engine::level::DecalSurface::WallEast => {
                surfaces.ceiling_y_at(point[0], point[2])
            }
        });
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(low, high), y| {
            (low.min(*y), high.max(*y))
        });
        if high - low > 0.05 {
            return Err(format!(
                "Decal {i} spans a floor or ceiling height change ({low:.2} m to {high:.2} m); \
                 place it entirely on one surface"
            ));
        }
    }
    Ok(())
}

/// 5. Generated-geometry complexity budget, checked after per-element
///    validation so dimension errors take precedence. This bounds the vertex
///    buffer built at load time, protecting the process from levels that would
///    otherwise exhaust memory. Overlapping/intersecting geometry is explicitly
///    allowed and is not validated here.
pub(super) fn validate_geometry_budget(level: &LevelDef) -> Result<(), String> {
    let estimate = level.estimate_geometry();
    if estimate.floor_area_m2 > MAX_LEVEL_FLOOR_AREA_M2 {
        return Err(format!(
            "Level floor area is too large: {} m^2 (limit: {MAX_LEVEL_FLOOR_AREA_M2} m^2). \
             Use smaller or fewer room sections.",
            estimate.floor_area_m2
        ));
    }
    if estimate.total_vertices > MAX_LEVEL_VERTICES {
        return Err(format!(
            "Level geometry is too complex: ~{} vertices (limit: {MAX_LEVEL_VERTICES}). \
             Reduce rooms, walls or ceiling lights.",
            estimate.total_vertices
        ));
    }
    Ok(())
}
