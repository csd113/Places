//! Vertical shutter rig and its static overhead clearance, isolated from doors.

#[cfg(test)]
#[path = "../tests/shutter.rs"]
mod tests;

use crate::engine::collision::{CONTACT_EPS, DoorCollider};
use crate::engine::gltf::{AnimationInterpolation, AnimationPath, PropAnimation, PropModel};
use crate::engine::level::{LevelDef, PhysicalPatch, WalkableCeiling, WalkableFloor};

pub(super) const MODEL: &str = "mall:security_shutter";
const CLIP: &str = "shutter_open";
const PANELS: [&str; 3] = [
    "Shutter_Grille_Lower",
    "Shutter_Grille_Middle",
    "Shutter_Grille_Upper",
];
const EPS: f32 = 1.0e-4;

pub(super) fn structure(level: &LevelDef) -> Vec<PhysicalPatch> {
    let mut floors = WalkableFloor::from_level(level).structural_patches();
    let ceilings = WalkableCeiling::from_level(level).physical_patches(&floors);
    floors.extend(ceilings);
    floors
}

/// Each panel is strictly linear, upright translation on a static XZ
/// footprint. This makes checking the total vertical envelope exact, even
/// between keys, without changing the permissive generic rigid-door format.
pub(super) fn motion<'a>(
    model: &'a PropModel,
    clip: &str,
    yaw: f32,
) -> Result<&'a PropAnimation, String> {
    if clip != CLIP || (yaw / 90.0 - (yaw / 90.0).round()).abs() > EPS {
        return Err("A security shutter requires shutter_open and a cardinal yaw".into());
    }
    let motion = model
        .animations
        .iter()
        .find(|candidate| candidate.name == CLIP)
        .ok_or("Missing shutter_open clip")?;
    if motion.channels.len() != PANELS.len() {
        return Err("Shutter requires one Y translation channel per grille bank".into());
    }
    for panel in PANELS {
        let channel = motion
            .channels
            .iter()
            .find(|channel| {
                model
                    .nodes
                    .get(usize::from(channel.node))
                    .is_some_and(|node| node.name == panel)
            })
            .ok_or("Missing shutter grille animation")?;
        let [x, _, z] = channel
            .values
            .first_chunk::<3>()
            .ok_or("Empty shutter grille animation")?;
        if channel.path != AnimationPath::Translation
            || channel.interpolation != AnimationInterpolation::Linear
            || !(2..=256).contains(&channel.times.len())
            || channel
                .values
                .as_chunks::<3>()
                .0
                .iter()
                .any(|[key_x, _, key_z]| (key_x - x).abs() > EPS || (key_z - z).abs() > EPS)
        {
            return Err(
                "Shutter grille banks must translate vertically with continuous linear keys".into(),
            );
        }
    }
    Ok(motion)
}

pub(super) fn include_pose(span: &mut DoorCollider, pose: &DoorCollider) -> Result<(), String> {
    let [x, z] = [span.dir_x.abs(), span.dir_z.abs()];
    if x.min(z) > EPS
        || (span.hinge_x - pose.hinge_x).abs() > EPS
        || (span.hinge_z - pose.hinge_z).abs() > EPS
    {
        return Err(
            "Shutter frame and grille collision must have fixed cardinal footprints".into(),
        );
    }
    let top = (span.hinge_y + span.height).max(pose.hinge_y + pose.height);
    span.hinge_y = span.hinge_y.min(pose.hinge_y);
    span.height = top - span.hinge_y;
    Ok(())
}

/// Slab/roof patches have rectangular plan bounds and a planar face. Test
/// the actual overlap rectangle, not the slab's unrelated high or low end.
pub(super) fn intersects(patch: &PhysicalPatch, collider: &DoorCollider) -> bool {
    let [origin, across, _, along] = patch.corners;
    let end_x = collider.dir_x.mul_add(collider.width, collider.hinge_x);
    let end_z = collider.dir_z.mul_add(collider.width, collider.hinge_z);
    let half_x = collider.dir_z.abs() * collider.thickness * 0.5;
    let half_z = collider.dir_x.abs() * collider.thickness * 0.5;
    let x0 = origin.x.max(collider.hinge_x.min(end_x) - half_x);
    let x1 = across.x.min(collider.hinge_x.max(end_x) + half_x);
    let z0 = origin.z.max(collider.hinge_z.min(end_z) - half_z);
    let z1 = along.z.min(collider.hinge_z.max(end_z) + half_z);
    if x1 - x0 <= CONTACT_EPS || z1 - z0 <= CONTACT_EPS {
        return false;
    }
    let slope_x = (across.y - origin.y) / (across.x - origin.x);
    let slope_z = (along.y - origin.y) / (along.z - origin.z);
    let surface =
        |x: f32, z: f32| slope_x.mul_add(x - origin.x, slope_z.mul_add(z - origin.z, origin.y));
    let (bottom, top) = if patch.ceiling {
        let x = if slope_x >= 0.0 { x0 } else { x1 };
        let z = if slope_z >= 0.0 { z0 } else { z1 };
        (surface(x, z), patch.backing)
    } else {
        let x = if slope_x >= 0.0 { x1 } else { x0 };
        let z = if slope_z >= 0.0 { z1 } else { z0 };
        (patch.backing, surface(x, z))
    };
    collider.overlaps_body_y(bottom, top - bottom)
}
