//! Material regions independent of geometry, sampled at real support contacts.

use glam::Vec3;
use rapier3d::prelude::{ContactModificationContext, PhysicsHooks};

use crate::engine::level::{LevelDef, RampSurface, StairSurface};
use crate::engine::materials::MaterialTable;

#[derive(Debug, Clone, Copy)]
struct Rect {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
}

impl Rect {
    const fn new(bounds: (f32, f32, f32, f32)) -> Self {
        Self {
            x0: bounds.0,
            x1: bounds.1,
            z0: bounds.2,
            z1: bounds.3,
        }
    }

    const fn contains(self, x: f32, z: f32) -> bool {
        x >= self.x0 && x <= self.x1 && z >= self.z0 && z <= self.z1
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let overlap = Self {
            x0: self.x0.max(other.x0),
            x1: self.x1.min(other.x1),
            z0: self.z0.max(other.z0),
            z1: self.z1.min(other.z1),
        };
        (overlap.x1 > overlap.x0 && overlap.z1 > overlap.z0).then_some(overlap)
    }
}

struct RoomSurface {
    bounds: Rect,
    floor_y: f32,
    friction: f32,
}

struct Override {
    bounds: Rect,
    friction: Option<f32>,
}

pub(super) struct FloorMaterial {
    pub base_friction: f32,
    patches: Vec<(Rect, f32)>,
}

impl FloorMaterial {
    fn friction_at(&self, x: f32, z: f32) -> f32 {
        self.patches
            .iter()
            .find(|(bounds, _)| bounds.contains(x, z))
            .map_or(self.base_friction, |(_, friction)| *friction)
    }
}

// Item collider user_data encodes a u64 key plus one. Static material tags
// occupy a disjoint namespace and retain O(1) lookup without a per-step map.
const FLOOR_MATERIAL_TAG: u128 = 1_u128 << 127;

#[derive(Default)]
pub(super) struct FloorContactHooks {
    materials: Vec<FloorMaterial>,
}

impl FloorContactHooks {
    pub fn register(&mut self, material: FloorMaterial) -> Option<u128> {
        if material.patches.is_empty() {
            return None;
        }
        let index = u128::try_from(self.materials.len()).ok()?;
        self.materials.push(material);
        Some(FLOOR_MATERIAL_TAG | index)
    }

    fn material(&self, tag: u128) -> Option<&FloorMaterial> {
        if tag & FLOOR_MATERIAL_TAG == 0 {
            return None;
        }
        let index = usize::try_from(tag ^ FLOOR_MATERIAL_TAG).ok()?;
        self.materials.get(index)
    }
}

impl PhysicsHooks for FloorContactHooks {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext<'_>) {
        let Some(first) = context.colliders.get(context.collider1) else {
            return;
        };
        let Some(second) = context.colliders.get(context.collider2) else {
            return;
        };
        let (material, floor_first, item_friction) =
            if let Some(material) = self.material(first.user_data) {
                (material, true, second.friction())
            } else if let Some(material) = self.material(second.user_data) {
                (material, false, first.friction())
            } else {
                return;
            };
        let Some(rigid) = context.rigid_mut() else {
            return;
        };
        let mut coefficient_sum = 0.0_f32;
        let mut contact_count = 0.0_f32;
        for contact in rigid.solver_contacts.iter() {
            let anchor = if floor_first {
                contact.anchor1
            } else {
                contact.anchor2
            };
            coefficient_sum += material.friction_at(anchor.x, anchor.z);
            contact_count += 1.0;
        }
        if contact_count > 0.0 {
            // Rapier 0.36 stores friction per manifold. Sampling its actual
            // world-space support anchors gives a gradual footprint transition,
            // while compound subshapes retain independent contact manifolds.
            *rigid.friction = (coefficient_sum / contact_count) * item_friction;
        }
    }
}

#[derive(Default)]
pub(super) struct FloorSurfaces {
    rooms: Vec<RoomSurface>,
    regions: Vec<Override>,
    patches: Vec<Override>,
    ramps: Vec<(RampSurface, f32)>,
    stairs: Vec<(StairSurface, f32)>,
    fallback: Option<f32>,
}

impl FloorSurfaces {
    pub fn from_level(level: &LevelDef, materials: &MaterialTable) -> Self {
        let friction = |id: &str| materials.ground_friction(id);
        let default = friction(&level.defaults.floor);
        Self {
            rooms: level
                .room_iter()
                .map(|room| RoomSurface {
                    bounds: Rect::new(room.bounds()),
                    floor_y: room.floor_y,
                    friction: room.material.as_deref().map_or(default, friction),
                })
                .collect(),
            regions: level
                .floor_regions
                .iter()
                .map(|region| Override {
                    bounds: Rect::new(region.bounds()),
                    friction: region.material.as_deref().map(friction),
                })
                .collect(),
            patches: level
                .floor_patches
                .iter()
                .map(|patch| Override {
                    bounds: Rect::new((
                        patch.x.min(patch.x + patch.width),
                        patch.x.max(patch.x + patch.width),
                        patch.z.min(patch.z + patch.depth),
                        patch.z.max(patch.z + patch.depth),
                    )),
                    friction: Some(friction(&patch.material)),
                })
                .collect(),
            ramps: level
                .ramps
                .iter()
                .map(|ramp| {
                    (
                        ramp.surface(),
                        ramp.material.as_deref().map_or(default, friction),
                    )
                })
                .collect(),
            stairs: level
                .stairs
                .iter()
                .map(|stair| {
                    (
                        stair.surface(),
                        stair.material.as_deref().map_or(default, friction),
                    )
                })
                .collect(),
            fallback: Some(default),
        }
    }

    /// Geometry already partitions ramp/stair/region ownership. Material-only
    /// boundaries stay descriptors: cutting a backing slab introduces ghost
    /// vertical faces that CCD can hit while sliding on a coplanar floor.
    pub fn material(&self, corners: [Vec3; 4], floor_y: f32) -> FloorMaterial {
        let [a, b, c, _d] = corners;
        let bounds = Rect::new((a.x, b.x, a.z, c.z));
        let sample_x = f32::midpoint(bounds.x0, bounds.x1);
        let sample_z = f32::midpoint(bounds.z0, bounds.z1);
        let base = self
            .rooms
            .iter()
            .find(|room| {
                room.bounds.contains(sample_x, sample_z) && (room.floor_y - floor_y).abs() < 1.0e-4
            })
            .map_or_else(|| self.fallback.unwrap_or(0.7), |room| room.friction);
        let higher = self
            .ramps
            .iter()
            .rev()
            .find(|(ramp, _)| ramp.contains(sample_x, sample_z))
            .map(|(_, coefficient)| *coefficient)
            .or_else(|| {
                self.stairs
                    .iter()
                    .rev()
                    .find(|(stair, _)| stair.contains(sample_x, sample_z))
                    .map(|(_, coefficient)| *coefficient)
            })
            .or_else(|| {
                self.regions
                    .iter()
                    .rev()
                    .find(|region| region.bounds.contains(sample_x, sample_z))
                    .and_then(|region| region.friction)
            });
        if let Some(friction) = higher {
            return FloorMaterial {
                base_friction: friction,
                patches: Vec::new(),
            };
        }
        FloorMaterial {
            base_friction: base,
            patches: self
                .patches
                .iter()
                .rev()
                .filter_map(|patch| Some((bounds.intersection(patch.bounds)?, patch.friction?)))
                .collect(),
        }
    }
}
