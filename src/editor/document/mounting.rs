//! Explicit surface mounting. Numeric edits remain unconstrained so intentional
//! embedded scenery can be saved without a later transform silently repairing it.

use glam::{EulerRot, Quat, Vec3};

use super::{Document, ObjectId, Transform, objects};
use crate::engine::level::{
    CeilingProfileDef, LevelDef, LevelSurfaces, LightMount, WallAxis, wall_solid_slices_profiled,
};
use crate::engine::loader::PropCatalog;
use crate::engine::props::PropAssets;
use crate::engine::spatial::Aabb;

/// Two millimetres separate mounting backs from their carrier, beyond the
/// geometry checker's 1.5 mm coplanarity tolerance without visible floating.
const CLEARANCE: f32 = 0.002;

impl Document {
    /// Mounts a prop to its floor or a fixture to its declared ceiling/wall.
    /// The actual model bounds, mounting back and carrier normal determine the
    /// origin; optional snapping affects only the carrier's tangent coordinates.
    ///
    /// # Errors
    /// Missing carriers/models or a mounting footprint that cannot fit are
    /// refused atomically. Gabled fixture mounts require manual placement since
    /// the fixture source contract represents yaw only.
    pub fn mount_to_surface(&mut self, id: ObjectId, snap_step: Option<f32>) -> Result<(), String> {
        if snap_step.is_some_and(|step| !step.is_finite() || step <= 0.0) {
            return Err("Mount snapping needs a positive finite step".into());
        }
        let catalog = PropCatalog::load_default();
        let mut assets = PropAssets::load_default();
        let proposed = mounted_transform(
            self.level(),
            id,
            self.transform(id)?,
            snap_step,
            &catalog,
            &mut assets,
        )?;
        let mut edited = self.raw().clone();
        objects::set_transform(&mut edited, self.level(), id, proposed)?;
        if matches!(id, ObjectId::Light(_)) {
            // Explicit mounting owns the final point. Compiler grid alignment
            // must not subsequently move the housing off its checked carrier.
            drop(objects::object_mut(&mut edited, id)?.insert("align".into(), "none".into()));
        }
        self.accept_edit(edited)
    }
}

fn mounted_transform(
    level: &LevelDef,
    id: ObjectId,
    current: Transform,
    snap_step: Option<f32>,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
) -> Result<Transform, String> {
    match id {
        ObjectId::Prop(index) => {
            let prop = level
                .props
                .get(index)
                .ok_or("The selected prop no longer exists")?;
            if prop.span.is_some() || prop.float.is_some() {
                return Err("Fitted spans and floating props own their mounting pose; use their authored placement".into());
            }
            let definition = catalog.get(&prop.model);
            let bounds = model_bounds(
                definition.model.as_deref(),
                prop.size.unwrap_or(definition.size),
                assets,
            )?;
            floor_mount(level, current, bounds, snap_step)
        }
        ObjectId::Light(index) => {
            let fixture = level
                .ceiling_lights
                .get(index)
                .ok_or("The selected fixture no longer exists")?;
            let wall_family = crate::engine::lighting::fixture_profile(&fixture.fixture).kind
                == crate::engine::lighting::FixtureKind::WallSconce;
            if wall_family != (fixture.mount == LightMount::Wall) {
                return Err("Choose a fixture family matching the carrier: wall sconces mount to walls; ceiling housings mount to ceilings".into());
            }
            let bounds = if let Some(model_id) =
                crate::engine::lighting::fixture_model_id(&fixture.fixture)
            {
                let definition = catalog.get(model_id);
                model_bounds(definition.model.as_deref(), definition.size, assets)?
            } else {
                let profile = crate::engine::lighting::fixture_profile(&fixture.fixture);
                match fixture.mount {
                    LightMount::Ceiling => Aabb {
                        min: [-profile.half_width, -0.1, -profile.half_depth],
                        max: [profile.half_width, 0.0, profile.half_depth],
                    },
                    LightMount::Wall => Aabb {
                        min: [-profile.half_width, -0.1, 0.0],
                        max: [profile.half_width, 0.1, 0.11],
                    },
                }
            };
            match fixture.mount {
                LightMount::Ceiling => ceiling_mount(level, current, bounds, snap_step),
                LightMount::Wall => wall_mount(level, current, bounds, snap_step),
            }
        }
        ObjectId::Room(_)
        | ObjectId::Wall(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => {
            Err("Surface mounting belongs to props and light fixtures".into())
        }
    }
}

fn model_bounds(
    path: Option<&str>,
    size: [f32; 3],
    assets: &mut PropAssets,
) -> Result<Aabb, String> {
    if let Some(model_path) = path {
        let asset = assets
            .resolve(model_path)
            .map_err(|error| format!("Cannot measure mounting model {model_path}: {error}"))?;
        let (min, max) = asset
            .model
            .bounds()
            .ok_or("Mounting model contains no finite geometry")?;
        Ok(Aabb { min, max })
    } else {
        Ok(Aabb {
            min: [-size[0] * 0.5, 0.0, -size[2] * 0.5],
            max: [size[0] * 0.5, size[1], size[2] * 0.5],
        })
    }
}

fn snapped(value: f32, step: Option<f32>) -> f32 {
    step.map_or(value, |spacing| (value / spacing).round() * spacing)
}

fn corners(bounds: Aabb) -> impl Iterator<Item = Vec3> {
    [bounds.min[0], bounds.max[0]]
        .into_iter()
        .flat_map(move |x| {
            [bounds.min[1], bounds.max[1]]
                .into_iter()
                .flat_map(move |y| {
                    [bounds.min[2], bounds.max[2]]
                        .into_iter()
                        .map(move |z| Vec3::new(x, y, z))
                })
        })
}

fn floor_mount(
    level: &LevelDef,
    mut transform: Transform,
    bounds: Aabb,
    snap_step: Option<f32>,
) -> Result<Transform, String> {
    let surfaces = LevelSurfaces::new(level);
    let x = snapped(transform.position[0], snap_step);
    let z = snapped(transform.position[2], snap_step);
    let source_base = surfaces
        .floor_y_at(x, z)
        .ok_or("Place the prop over a floor before mounting it")?;
    let reference_y = source_base + transform.position[1];
    let carrier = surfaces
        .rooms()
        .iter()
        .copied()
        .filter(|room| room.contains(x, z))
        .min_by(|first, second| {
            (first.floor_y + surfaces.walkable_offset_at(x, z) - reference_y)
                .abs()
                .total_cmp(
                    &(second.floor_y + surfaces.walkable_offset_at(x, z) - reference_y).abs(),
                )
        })
        .ok_or("Place the prop over a floor before mounting it")?;
    let floor = carrier.floor_y + surfaces.walkable_offset_at(x, z);
    let normal = surfaces.ramp_at(x, z).map_or(Vec3::Y, |ramp| {
        let (dx, dz) = match ramp.axis() {
            WallAxis::X => (ramp.rise / ramp.width, 0.0),
            WallAxis::Z => (0.0, ramp.rise / ramp.depth),
        };
        Vec3::new(-dx, 1.0, -dz).normalize()
    });
    let rotation = Quat::from_rotation_arc(Vec3::Y, normal)
        .mul_quat(Quat::from_rotation_y(transform.yaw_degrees.to_radians()));
    let (yaw, pitch, roll) = rotation.to_euler(EulerRot::YXZ);
    transform.yaw_degrees = yaw.to_degrees();
    transform.pitch_degrees = pitch.to_degrees();
    transform.roll_degrees = roll.to_degrees();
    transform.position = [x, 0.0, z];
    let mut lift = f32::NEG_INFINITY;
    for point in corners(bounds) {
        let relative = rotation.mul_vec3(Vec3::from_array(
            point
                .to_array()
                .map(|coordinate| coordinate * transform.size[0]),
        ));
        let world = Vec3::new(x + relative.x, floor + relative.y, z + relative.z);
        if !surfaces.rooms().iter().any(|room| {
            room.contains(world.x, world.z) && (room.floor_y - carrier.floor_y).abs() <= 0.0001
        }) {
            return Err("The model footprint extends beyond its supporting floor; adjust manually or move it inward".into());
        }
        let support = carrier.floor_y + surfaces.walkable_offset_at(world.x, world.z);
        let expected_support =
            floor - normal.x.mul_add(relative.x, normal.z * relative.z) / normal.y;
        if (support - expected_support).abs() > 0.002 {
            return Err("The model footprint crosses a floor step or slope boundary; move it onto one surface or adjust manually".into());
        }
        lift = lift.max(support - world.y);
    }
    transform.position[1] = floor - source_base + lift + CLEARANCE;
    Ok(transform)
}

fn ceiling_mount(
    level: &LevelDef,
    mut transform: Transform,
    bounds: Aabb,
    snap_step: Option<f32>,
) -> Result<Transform, String> {
    let x = snapped(transform.position[0], snap_step);
    let z = snapped(transform.position[2], snap_step);
    let room = level
        .rooms
        .iter()
        .filter(|room| room.contains(x, z))
        .min_by(|a, b| {
            (a.ceiling_y_at(x, z) - transform.position[1])
                .abs()
                .total_cmp(&(b.ceiling_y_at(x, z) - transform.position[1]).abs())
        })
        .ok_or("Place the fixture below a room ceiling before mounting it")?;
    if !matches!(room.ceiling, CeilingProfileDef::Flat) {
        return Err("This fixture needs a flat mounting ceiling; use manual placement for a gable or open roof".into());
    }
    let rotation = if crate::engine::lighting::fixture_is_turned(transform.yaw_degrees) {
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)
    } else {
        Quat::IDENTITY
    };
    let mut top = f32::NEG_INFINITY;
    for point in corners(bounds) {
        let relative = rotation.mul_vec3(point);
        if !room.contains(x + relative.x, z + relative.z) {
            return Err("The housing footprint extends beyond this ceiling; move it inward or adjust manually".into());
        }
        top = top.max(relative.y);
    }
    transform.position = [x, room.ceiling_y_at(x, z) - top - CLEARANCE, z];
    Ok(transform)
}

fn wall_mount(
    level: &LevelDef,
    mut transform: Transform,
    bounds: Aabb,
    snap_step: Option<f32>,
) -> Result<Transform, String> {
    let surfaces = LevelSurfaces::new(level);
    let origin = Vec3::from_array(transform.position);
    let half_width = (bounds.max[0] - bounds.min[0]) * 0.5;
    let half_height = (bounds.max[1] - bounds.min[1]) * 0.5;
    let mut best: Option<(f32, Vec3, f32)> = None;
    for wall in &level.walls {
        let (x, z) = wall.min_corner();
        for slice in wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &surfaces.wall_profile_breaks(wall),
        ) {
            let low = slice.start + half_width;
            let high = slice.end - half_width;
            let bottom = slice.bottom + half_height;
            let top = slice.top - half_height;
            if low > high || bottom > top {
                continue;
            }
            let requested_along = match wall.axis() {
                WallAxis::X => origin.x - x,
                WallAxis::Z => origin.z - z,
            };
            let along = snapped(requested_along, snap_step).clamp(low, high);
            let y = snapped(origin.y, snap_step).clamp(bottom, top);
            let faces = match wall.axis() {
                WallAxis::X => [
                    (Vec3::new(x + along, y, z), Vec3::NEG_Z, 180.0),
                    (Vec3::new(x + along, y, z + wall.depth.abs()), Vec3::Z, 0.0),
                ],
                WallAxis::Z => [
                    (Vec3::new(x, y, z + along), Vec3::NEG_X, 270.0),
                    (Vec3::new(x + wall.width.abs(), y, z + along), Vec3::X, 90.0),
                ],
            };
            for (face, normal, yaw) in faces {
                let offset = CLEARANCE - bounds.min[2];
                let position = Vec3::new(
                    normal.x.mul_add(offset, face.x),
                    normal.y.mul_add(offset, face.y),
                    normal.z.mul_add(offset, face.z),
                );
                let distance = origin.distance_squared(position);
                if best.is_none_or(|(prior, _, _)| distance < prior) {
                    best = Some((distance, position, yaw));
                }
            }
        }
    }
    let (_, position, yaw) =
        best.ok_or("No solid wall face has enough room for the fixture housing")?;
    transform.position = position.to_array();
    transform.yaw_degrees = yaw;
    Ok(transform)
}

#[cfg(test)]
#[path = "../tests/mounting.rs"]
mod tests;
