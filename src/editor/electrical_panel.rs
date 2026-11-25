//! Basic wall-click fitting for the Maintenance cabinet.

use glam::Vec3;
use serde_json::{Value, json};

use super::{
    App,
    construction::Tool,
    document::{
        Document, ObjectId, ObjectKind, objects, signs::fit_wall_footprint,
        wall_edit::DoorwayTarget,
    },
    viewport::{Viewport, WallPicking},
};
use crate::engine::{
    electrical_panel::{MODEL, MOUNT_GAP, SIZE},
    level::{DecalSurface, LevelSurfaces, WallAxis},
    spatial::Aabb,
};

impl App {
    pub(super) fn start_electrical_panel(&mut self) -> Result<(), String> {
        if self.catalog.placeable(MODEL).is_none() {
            return Err("The Maintenance electrical panel is not installed".into());
        }
        self.start_placement(Tool::ElectricalPanel)
    }
}

pub(super) fn candidate(
    doc: &Document,
    viewport: &Viewport,
    cached: &mut Option<WallPicking>,
    point: [f32; 2],
    grid: Option<f32>,
) -> Result<(Value, ObjectId), String> {
    if cached.is_none() {
        *cached = viewport.wall_picking();
    }
    let ray = viewport.ray(point).ok_or("Point at a visible solid wall")?;
    let (index, target) = cached
        .as_ref()
        .ok_or("Wait for the wall preview to finish")?
        .target(doc.level(), ray)?;
    if !matches!(target, DoorwayTarget::WallFace(_)) {
        return Err("Point at solid wall next to the opening".into());
    }
    fit_candidate(doc, index, ray, grid)
}

pub(super) fn fit_candidate(
    doc: &Document,
    index: usize,
    (origin, direction): (Vec3, Vec3),
    grid: Option<f32>,
) -> Result<(Value, ObjectId), String> {
    let level = doc.level();
    let wall = level.walls.get(index).ok_or("Wall is no longer visible")?;
    let (x, z) = wall.min_corner();
    let (face, yaw, distance) = match wall.axis() {
        WallAxis::X if direction.z > 0.0 => (
            DecalSurface::WallNorth,
            180.0_f32,
            (z - origin.z) / direction.z,
        ),
        WallAxis::X => (
            DecalSurface::WallSouth,
            0.0,
            (z + wall.depth.abs() - origin.z) / direction.z,
        ),
        WallAxis::Z if direction.x > 0.0 => {
            (DecalSurface::WallWest, 270.0, (x - origin.x) / direction.x)
        }
        WallAxis::Z => (
            DecalSurface::WallEast,
            90.0,
            (x + wall.width.abs() - origin.x) / direction.x,
        ),
    };
    if !(0.0..=500.0).contains(&distance) {
        return Err("Point at the broad face of the wall".into());
    }
    let at = direction.mul_add(Vec3::splat(distance), origin);
    let [width, height, _] = SIZE;
    let fitted = fit_wall_footprint(level, index, face, at, grid, (width * 0.5, height * 0.5))
        .map_err(|_footprint| "Choose a solid wall area large enough for the electrical panel")?;
    // Never jump an aperture or to the other face just to find fit.
    if fitted.distance(at) > 0.32 {
        return Err("Choose a clear solid wall area closer to the cabinet centre".into());
    }
    let position = Vec3::from_array(face.normal()).mul_add(Vec3::splat(MOUNT_GAP), fitted);
    let surfaces = LevelSurfaces::new(level);
    let floor_y = surfaces
        .floor_y_at(position.x, position.z)
        .ok_or("Choose the room side of the wall")?;
    let matrix = glam::Mat4::from_rotation_translation(
        glam::Quat::from_rotation_y(yaw.to_radians()),
        position,
    );
    let box_at = crate::engine::electrical_panel::body_bounds(matrix);
    if level
        .props
        .iter()
        .filter(|prop| prop.electrical_panel.is_some())
        .any(|prop| {
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            intersects(box_at, prop.placement_bounds(base_y, SIZE))
        })
    {
        return Err("This cabinet would overlap an existing electrical panel".into());
    }
    let mut raw = doc.raw().clone();
    let id = objects::add(
        &mut raw,
        level,
        ObjectKind::Prop,
        [position.x, position.y - floor_y, position.z],
        Some(MODEL),
    )?;
    let _rotation =
        objects::object_mut(&mut raw, id)?.insert("rotation_degrees".into(), json!(yaw));
    objects::apply_mechanism_preset(&mut raw, id, MODEL)?;
    Ok((raw, id))
}

fn intersects(a: Aabb, b: Aabb) -> bool {
    a.min.iter().zip(&b.max).all(|(lo, hi)| lo < hi)
        && b.min.iter().zip(&a.max).all(|(lo, hi)| lo < hi)
}

pub(super) fn preview_message(
    level: &crate::engine::level::LevelDef,
    id: ObjectId,
) -> &'static str {
    let ObjectId::Prop(index) = id else {
        return "Click to mount the cabinet.";
    };
    let valid_ceiling = level.props.get(index).is_some_and(|prop| {
        crate::engine::electrical_panel::conduit_height(
            level,
            &crate::engine::level::WalkableFloor::from_level(level),
            &LevelSurfaces::new(level),
            prop,
        )
        .is_some()
    });
    if valid_ceiling {
        "Cabinet and ceiling conduits fit. Click to place."
    } else {
        "No compatible ceiling above. Click to place the cabinet without conduits."
    }
}
