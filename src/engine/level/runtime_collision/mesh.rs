//! Bulk floor and ceiling clearance for posed meshes.
use glam::{Vec2, Vec3};

use super::{ROOM_EDGE_EPS_M, StairSampling, WalkableCeiling, WalkableFloor};
use crate::engine::collision::CONTACT_EPS;

pub struct MeshClearance {
    pub(crate) disc_radius: f32,
    pub(crate) head_margin: f32,
    pub(crate) ground_slop: f32,
    pub(crate) ground_reach: f32,
}

impl MeshClearance {
    pub(crate) fn is_clear(
        &self,
        floor: &WalkableFloor,
        ceiling: &WalkableCeiling,
        vertices: &[Vec3],
        feet: f32,
    ) -> bool {
        let Some(bounds) = Footprint::of(vertices) else {
            return self.clear_vertex_by_vertex(floor, ceiling, vertices, feet);
        };
        let ground_limit = feet + self.ground_reach;
        ceiling
            .rooms
            .iter()
            .filter(|room| bounds.reaches_disc(room.bounds, self.disc_radius))
            .all(|room| {
                vertices.iter().all(|point| {
                    room.height_above_disc(Vec2::new(point.x, point.z), self.disc_radius, feet)
                        .is_none_or(|height| point.y + self.head_margin < height)
                })
            })
            && floor.rooms.iter().all(|room| {
                let room_bounds = (room.x0, room.x1, room.z0, room.z1);
                let check_ground = bounds.reaches_floor(room_bounds);
                let check_underside = room.floor_y > feet + CONTACT_EPS
                    && bounds.reaches_disc(room_bounds, self.disc_radius);
                (!check_ground && !check_underside)
                    || vertices.iter().all(|point| {
                        (!check_ground
                            || WalkableFloor::room_height_at(
                                room,
                                point.x,
                                point.z,
                                StairSampling::Stepped,
                            )
                            .filter(|height| *height <= ground_limit)
                            .is_none_or(|height| point.y + self.ground_slop >= height))
                            && (!check_underside
                                || WalkableFloor::room_underside_disc(
                                    room,
                                    Vec2::new(point.x, point.z),
                                    self.disc_radius,
                                    feet,
                                )
                                .is_none_or(|height| point.y + self.head_margin < height))
                    })
            })
    }

    fn clear_vertex_by_vertex(
        &self,
        floor: &WalkableFloor,
        ceiling: &WalkableCeiling,
        vertices: &[Vec3],
        feet: f32,
    ) -> bool {
        vertices.iter().all(|point| {
            ceiling
                .ceiling_above_disc(point.x, point.z, self.disc_radius, feet)
                .is_none_or(|height| point.y + self.head_margin < height)
                && floor
                    .lowest_room_underside(Vec2::new(point.x, point.z), self.disc_radius, feet)
                    .is_none_or(|height| point.y + self.head_margin < height)
                && floor
                    .height_below(point.x, point.z, feet + self.ground_reach)
                    .is_none_or(|height| point.y + self.ground_slop >= height)
        })
    }
}

struct Footprint {
    min: Vec2,
    max: Vec2,
}

impl Footprint {
    fn of(vertices: &[Vec3]) -> Option<Self> {
        let empty = Self {
            min: Vec2::splat(f32::INFINITY),
            max: Vec2::splat(f32::NEG_INFINITY),
        };
        vertices.iter().try_fold(empty, |bounds, point| {
            let xz = Vec2::new(point.x, point.z);
            point.is_finite().then_some(Self {
                min: bounds.min.min(xz),
                max: bounds.max.max(xz),
            })
        })
    }

    fn reaches_floor(&self, (x0, x1, z0, z1): (f32, f32, f32, f32)) -> bool {
        self.max.x >= x0 - ROOM_EDGE_EPS_M
            && self.min.x <= x1 + ROOM_EDGE_EPS_M
            && self.max.y >= z0 - ROOM_EDGE_EPS_M
            && self.min.y <= z1 + ROOM_EDGE_EPS_M
    }

    // Reject only separated axes. Each remaining vertex still uses the exact
    // disc query, including the diagonal and the pitched surface extrema.
    fn reaches_disc(&self, (x0, x1, z0, z1): (f32, f32, f32, f32), radius: f32) -> bool {
        x0 - self.max.x < radius
            && self.min.x - x1 < radius
            && z0 - self.max.y < radius
            && self.min.y - z1 < radius
    }
}
