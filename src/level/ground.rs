//! Material traction lookup over the same authored surfaces as the floor.
//!
//! Compiled collision keeps its established geometry format. These small
//! material descriptors are installed from the package's validated source and
//! resolved material table when a world loads, without rebuilding geometry.

use super::{LevelDef, ROOM_EDGE_EPS_M, RampSurface, StairSurface};
use crate::collision::CONTACT_EPS;
use crate::materials::{GroundSurface, MaterialTable};

#[derive(Debug)]
struct GroundRoom {
    bounds: (f32, f32, f32, f32),
    floor_y: f32,
    surface: GroundSurface,
}

#[derive(Debug)]
struct GroundRegion {
    bounds: (f32, f32, f32, f32),
    offset: f32,
    surface: Option<GroundSurface>,
}

#[derive(Debug)]
struct GroundPatch {
    bounds: (f32, f32, f32, f32),
    surface: GroundSurface,
}

/// Material-only floor descriptors. Ordinary levels retain an empty sampler.
#[derive(Debug, Default)]
pub struct GroundSurfaces {
    rooms: Vec<GroundRoom>,
    regions: Vec<GroundRegion>,
    patches: Vec<GroundPatch>,
    ramps: Vec<(RampSurface, GroundSurface)>,
    stairs: Vec<(StairSurface, GroundSurface)>,
}

fn contains(bounds: (f32, f32, f32, f32), x: f32, z: f32) -> bool {
    let (x0, x1, z0, z1) = bounds;
    x >= x0 && x <= x1 && z >= z0 && z <= z1
}

impl GroundSurfaces {
    /// Builds traction descriptors from a world's resolved material properties.
    #[must_use]
    pub fn from_level(level: &LevelDef, materials: &MaterialTable) -> Self {
        if !materials
            .entries()
            .iter()
            .any(|entry| entry.ground_surface == GroundSurface::Ice)
        {
            return Self::default();
        }
        let surface = |id: &str| {
            materials
                .entry_of(id)
                .map_or(GroundSurface::Normal, |entry| entry.ground_surface)
        };
        let default = surface(&level.defaults.floor);
        Self {
            rooms: level
                .room_iter()
                .map(|room| GroundRoom {
                    surface: room.material.as_deref().map_or(default, surface),
                    bounds: room.bounds(),
                    floor_y: room.floor_y,
                })
                .collect(),
            regions: level
                .floor_regions
                .iter()
                .map(|region| GroundRegion {
                    bounds: region.bounds(),
                    offset: region.offset(),
                    surface: region.material.as_deref().map(surface),
                })
                .collect(),
            patches: level
                .floor_patches
                .iter()
                .map(|patch| GroundPatch {
                    bounds: patch.bounds(),
                    surface: surface(&patch.material),
                })
                .collect(),
            ramps: level
                .ramps
                .iter()
                .map(|ramp| {
                    (
                        ramp.surface(),
                        ramp.material.as_deref().map_or(default, surface),
                    )
                })
                .collect(),
            stairs: level
                .stairs
                .iter()
                .map(|stair| {
                    (
                        stair.surface(),
                        stair.material.as_deref().map_or(default, surface),
                    )
                })
                .collect(),
        }
    }

    /// Traction at the feet on a currently supporting floor. A prop top,
    /// ceiling, different storey or airborne body never inherits ice below it.
    #[must_use]
    pub fn at(&self, x: f32, z: f32, feet: f32) -> GroundSurface {
        if !x.is_finite() || !z.is_finite() || !feet.is_finite() {
            return GroundSurface::Normal;
        }
        for ground in &self.rooms {
            let (x0, x1, z0, z1) = ground.bounds;
            if x < x0 - ROOM_EDGE_EPS_M
                || x > x1 + ROOM_EDGE_EPS_M
                || z < z0 - ROOM_EDGE_EPS_M
                || z > z1 + ROOM_EDGE_EPS_M
            {
                continue;
            }
            let room_x = x.clamp(x0, x1);
            let room_z = z.clamp(z0, z1);
            let (offset, surface) = self.in_room(ground.surface, room_x, room_z);
            if (ground.floor_y + offset - feet).abs() <= CONTACT_EPS {
                return surface;
            }
        }
        GroundSurface::Normal
    }

    fn in_room(&self, base: GroundSurface, x: f32, z: f32) -> (f32, GroundSurface) {
        if let Some((ramp, surface)) = self
            .ramps
            .iter()
            .rev()
            .find(|(ramp, _)| ramp.contains(x, z))
        {
            return (ramp.offset_at(x, z), *surface);
        }
        if let Some((stair, surface)) = self
            .stairs
            .iter()
            .rev()
            .find(|(stair, _)| stair.contains(x, z))
        {
            return (stair.pitch_offset_at(x, z), *surface);
        }
        let region = self
            .regions
            .iter()
            .rev()
            .find(|region| contains(region.bounds, x, z));
        let surface = region
            .and_then(|matched| matched.surface)
            .unwrap_or_else(|| {
                self.patches
                    .iter()
                    .rev()
                    .find(|patch| contains(patch.bounds, x, z))
                    .map_or(base, |patch| patch.surface)
            });
        (region.map_or(0.0, |matched| matched.offset), surface)
    }
}
