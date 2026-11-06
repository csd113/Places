//! Architecture-owned bounded passive sound routes. No renderer muffling
//! fallback is evidence, and an obstructed source is localized only to portals.
use crate::engine::audio::{AudioCatalog, acoustics::AcousticScene};
use crate::engine::collision::DoorCollider;
use crate::engine::level::{LevelDef, LevelSurfaces, WallAxis};
use crate::engine::tall_guest::hearing::{Heard, Request};
use glam::Vec3;

pub(super) struct Portal {
    pub id: u32,
    pub center: Vec3,
    pub normal: Vec3,
    pub half_depth: f32,
}

#[derive(Default)]
pub(super) struct Layout {
    pub geometry: AcousticScene,
    pub portals: Vec<Portal>,
    pub transmissions: Vec<(String, f32)>,
    pub catalog: Option<AudioCatalog>,
}

impl Layout {
    pub fn build(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut portals = Vec::new();
        for wall in &level.walls {
            for opening in wall
                .openings
                .iter()
                .filter(|opening| opening.glass.is_none())
            {
                if portals.len() >= 256 {
                    break;
                }
                let (start_x, start_z) = wall.length_origin();
                let along = opening.width.mul_add(0.5, opening.offset);
                let (x, z, normal, depth) = match wall.axis() {
                    WallAxis::X => (
                        start_x + along,
                        wall.depth.mul_add(0.5, wall.z),
                        Vec3::Z,
                        wall.depth.abs(),
                    ),
                    WallAxis::Z => (
                        wall.width.mul_add(0.5, wall.x),
                        start_z + along,
                        Vec3::X,
                        wall.width.abs(),
                    ),
                };
                let y = surfaces.floor_y_at(x, z).unwrap_or(0.0)
                    + wall.y
                    + opening.sill
                    + (opening.height * 0.5).min(1.2);
                portals.push(Portal {
                    id: u32::try_from(portals.len()).unwrap_or(u32::MAX),
                    center: Vec3::new(x, y, z),
                    normal,
                    half_depth: depth.mul_add(0.5, 0.08),
                });
            }
        }
        let transmissions = level
            .doors
            .iter()
            .filter_map(|door| {
                let material = door.material.as_ref()?;
                let authored = level
                    .acoustic_materials
                    .iter()
                    .find(|entry| &entry.material == material)?;
                (authored.transmission > 0.0).then(|| (door.id.clone(), authored.transmission))
            })
            .collect();
        let catalog = crate::engine::assets::resolve_asset_root()
            .and_then(|root| AudioCatalog::load(&root).ok());
        Self {
            geometry: AcousticScene::build(level),
            portals,
            transmissions,
            catalog,
        }
    }
}

pub(super) struct Ambient {
    pub position: Vec3,
    pub amplitude: f32,
    pub attenuation: [f32; 2],
    pub spatial: bool,
}

impl Portal {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite side-of-plane and metre-space offsets select listener and source sides."
    )]
    pub fn sides(&self, listener: Vec3) -> (Vec3, Vec3) {
        let sign = if (listener - self.center).dot(self.normal) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let near = self.center + self.normal * (sign * self.half_depth);
        let far = self.center - self.normal * (sign * self.half_depth);
        (near, far)
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite segment normalization precedes the bounded oriented-leaf query."
)]
pub(super) fn crosses(from: Vec3, to: Vec3, leaf: &DoorCollider) -> bool {
    let delta = to - from;
    let distance = delta.length();
    distance > 0.001 && leaf.ray_entry(from, delta / distance, distance).is_some()
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite portal side offsets keep the body outside a closed leaf or beyond an open aperture."
)]
pub(super) fn portal_evidence(
    request: &Request,
    mut evidence: Heard,
    near: Vec3,
    far: Vec3,
    ids: Vec<u32>,
) -> Heard {
    // Every unconfirmed portal sound supplies a reachable listener-side
    // staging region, never the source's private coordinate. A later direct
    // echo may justify crossing toward an identified thrown body.
    evidence.estimated = near;
    let outward = (near - far).normalize_or_zero() * 1.2;
    evidence.goal = near + outward;
    evidence.goal.y = request.listener.y;
    evidence.uncertainty = evidence.uncertainty.max(1.2);
    evidence.portals = ids;
    evidence
}
