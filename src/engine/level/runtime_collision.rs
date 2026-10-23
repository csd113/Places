//! Owned runtime collision models and their bounded compiled codecs.
//! Authoring definitions remain in the parent; public level paths are reexported.

mod physics;

use super::{
    CeilingProfileDef, DEFAULT_WATER_MATERIAL, LevelDef, LevelSurfaces, MIN_STRUCTURE_M,
    ROOM_EDGE_EPS_M, RampSurface, StairSurface, WallAxis, WaterMotionDef, WaterShape,
    ceiling_y_for_volume,
};
use crate::engine::collision::WallAabb;
use crate::engine::package::binary::{Reader, Writer};
use crate::engine::package::collision::{
    MAX_COLLISION_LADDERS, MAX_COLLISION_WATER_MATERIAL_BYTES, MAX_COLLISION_WATER_VOLUMES,
};

/// One floor region resolved into the walkable surface model.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkableRegion {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    /// World Y of the walkable surface inside the region.
    y: f32,
}

/// One ramp resolved into the walkable surface model.
///
/// It owns the ramp's [`RampSurface`] and its room's floor plane and answers
/// with `floor_y + surface.offset_at(...)`, so the controller resolves the exact
/// arithmetic the renderer used.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkableRamp {
    surface: RampSurface,
    floor_y: f32,
}

impl WalkableRamp {
    /// World Y of the sloped surface at `(x, z)`.
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.offset_at(x, z)
    }
}

/// One staircase resolved into the walkable surface model.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkableStair {
    surface: StairSurface,
    floor_y: f32,
}

impl WalkableStair {
    /// World Y of the stepped surface at `(x, z)` (the rendered treads).
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.offset_at(x, z)
    }

    /// World Y of the surface the controller walks at `(x, z)`: the line
    /// through the nosings ([`StairSurface::pitch_offset_at`]).
    fn pitch_height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.pitch_offset_at(x, z)
    }

    fn tread_height(&self, index: u32) -> f32 {
        let (x0, x1, z0, z1) = self.surface.bounds();
        let (start, end) = self.surface.tread_span(index);
        match self.surface.axis() {
            WallAxis::X => self.height_at(f32::midpoint(start, end), f32::midpoint(z0, z1)),
            WallAxis::Z => self.height_at(f32::midpoint(x0, x1), f32::midpoint(start, end)),
        }
    }

    fn face(&self, run: (f32, f32), across: (f32, f32), y: (f32, f32)) -> WallAabb {
        let (x, z, width, depth) = match self.surface.axis() {
            WallAxis::X => (run.0, across.0, run.1 - run.0, across.1 - across.0),
            WallAxis::Z => (across.0, run.0, across.1 - across.0, run.1 - run.0),
        };
        WallAabb::with_y(x, y.0, z, width, y.1 - y.0, depth)
    }

    /// Exact closed tread sides, front risers and the flight's far end.
    /// Only the outward face blocks entry; legal step-up and inside exits stay
    /// governed by the same pitch line and step allowance as ordinary walking.
    fn for_each_body_barrier(
        &self,
        feet: f32,
        height: f32,
        step: f32,
        visit: &mut impl FnMut(&WallAabb, glam::Vec2),
    ) {
        let backing = self.floor_y + self.surface.base_offset().min(0.0);
        let head = feet + height - crate::engine::collision::CONTACT_EPS;
        if head <= backing {
            return;
        }
        let tolerance = if step > 0.0 {
            crate::engine::collision::STEP_EPS
        } else {
            crate::engine::collision::CONTACT_EPS
        };
        let threshold = feet + step + tolerance;
        let (x0, x1, z0, z1) = self.surface.bounds();
        let (across, front, back, low_side, high_side) = match self.surface.axis() {
            WallAxis::X => (
                (z0, z1),
                -glam::Vec2::X,
                glam::Vec2::X,
                -glam::Vec2::Y,
                glam::Vec2::Y,
            ),
            WallAxis::Z => (
                (x0, x1),
                -glam::Vec2::Y,
                glam::Vec2::Y,
                -glam::Vec2::X,
                glam::Vec2::X,
            ),
        };
        for index in 0..self.surface.step_count() {
            let top = self.tread_height(index);
            if top <= threshold {
                continue;
            }
            let (start, end) = self.surface.tread_span(index);
            let riser_bottom = if index == 0 {
                backing
            } else {
                self.tread_height(index.saturating_sub(1))
            };
            for (at, normal) in [(across.0, low_side), (across.1, high_side)] {
                visit(&self.face((start, end), (at, at), (backing, top)), normal);
            }
            // Grounded pitch-line walking may span multiple short treads with
            // its leading disc. Each legal riser is still an ordinary step;
            // airborne bodies must instead clear that riser's actual top.
            if riser_bottom < head
                && (step <= 0.0 || top - riser_bottom > step + crate::engine::collision::STEP_EPS)
            {
                visit(
                    &self.face((start, start), across, (riser_bottom, top)),
                    front,
                );
            }
            if index.saturating_add(1) == self.surface.step_count() {
                visit(&self.face((end, end), across, (backing, top)), back);
            }
        }
    }
}

/// One room of the walkable surface model.
#[derive(Debug, Clone, PartialEq)]
struct WalkableRoom {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    floor_y: f32,
    /// Ramps resolved against this room, in authored order (later wins).
    ramps: Vec<WalkableRamp>,
    /// Staircases resolved against this room, in authored order (later wins).
    stairs: Vec<WalkableStair>,
    /// Regions resolved against this room, in authored order (later wins).
    regions: Vec<WalkableRegion>,
}

/// Owned, allocation-light floor model the player controller samples while
/// walking.
///
/// It is built once per level from the same [`LevelSurfaces`] queries the mesh
/// and collision use, so the height the player stands on is by construction the
/// height that was rendered. The player position is the only per-frame input;
/// the lookup is a linear scan over the level's rooms (bounded at 500 by the
/// loader) followed by that room's regions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalkableFloor {
    rooms: Vec<WalkableRoom>,
}

/// Which vertical surface a staircase answers with.
///
/// The rendered treads and the controller's walking surface differ: the mesh,
/// the floor atlas and prop placement need the exact stepped geometry, while a
/// player must move continuously from one tread to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StairSampling {
    /// The rendered treads: the step the point is on.
    Stepped,
    /// The line through the flight's nosings, level on the top tread.
    PitchLine,
}

impl WalkableFloor {
    /// Retained room and nested surface storage for the prepared-world cache.
    #[must_use]
    pub(crate) fn retained_heap_bytes(&self) -> usize {
        self.rooms.iter().fold(
            self.rooms
                .capacity()
                .saturating_mul(std::mem::size_of::<WalkableRoom>()),
            |bytes, room| {
                bytes
                    .saturating_add(
                        room.ramps
                            .capacity()
                            .saturating_mul(std::mem::size_of::<WalkableRamp>()),
                    )
                    .saturating_add(
                        room.stairs
                            .capacity()
                            .saturating_mul(std::mem::size_of::<WalkableStair>()),
                    )
                    .saturating_add(
                        room.regions
                            .capacity()
                            .saturating_mul(std::mem::size_of::<WalkableRegion>()),
                    )
            },
        )
    }

    /// Builds the walkable surface model for a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut rooms = Vec::with_capacity(surfaces.rooms().len());
        for room in surfaces.rooms() {
            let (x0, x1, z0, z1) = room.bounds();
            let floor_y = if room.floor_y.is_finite() {
                room.floor_y
            } else {
                0.0
            };
            let regions = surfaces
                .regions_for_room(room)
                .into_iter()
                .map(|region| {
                    let (rx0, rx1, rz0, rz1) = region.bounds();
                    WalkableRegion {
                        x0: rx0,
                        x1: rx1,
                        z0: rz0,
                        z1: rz1,
                        y: floor_y + region.offset(),
                    }
                })
                .collect();
            let ramps = surfaces
                .ramps_for_room(room)
                .into_iter()
                .map(|ramp| WalkableRamp {
                    surface: ramp.surface(),
                    floor_y,
                })
                .collect();
            let stairs = surfaces
                .stairs_for_room(room)
                .into_iter()
                .map(|stair| WalkableStair {
                    surface: stair.surface(),
                    floor_y,
                })
                .collect();
            rooms.push(WalkableRoom {
                x0,
                x1,
                z0,
                z1,
                floor_y,
                ramps,
                stairs,
                regions,
            });
        }
        Self { rooms }
    }

    /// True when the level contains no rooms at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// World Y of the walkable floor at `(x, z)`, or `None` outside every room.
    ///
    /// This is the *rendered* surface: on a staircase it answers with the tread
    /// the point is on, exactly like [`LevelSurfaces::floor_y_at`]. Use
    /// [`Self::walk_height_at`] for the surface a walking player's feet follow,
    /// which is continuous across a flight.
    ///
    /// The first room in level order containing the point wins, matching
    /// [`LevelSurfaces::floor_y_at`]; inside it a ramp or staircase covering the
    /// point wins over the last authored floor region, exactly as the surface
    /// queries resolve it.
    #[must_use]
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        self.resolve_height_at(x, z, StairSampling::Stepped)
    }

    /// Rendered support of one room in level order, preserving its own ramp,
    /// stair and region ownership even under an overlapping upper storey.
    /// Diagnostics must not borrow the first room's slopes for every storey.
    #[must_use]
    pub(crate) fn height_in_room(&self, index: usize, x: f32, z: f32) -> Option<f32> {
        self.rooms
            .get(index)
            .and_then(|room| Self::room_height_at(room, x, z, StairSampling::Stepped))
    }

    /// World Y of the surface the player's feet follow while walking at
    /// `(x, z)`, or `None` outside every room.
    ///
    /// Resolves exactly like [`Self::height_at`] except on a staircase, where
    /// it answers with the line through the flight's nosings
    /// ([`StairSurface::pitch_offset_at`]). Sampling that line during movement
    /// makes the foot height rise and fall continuously from tread to tread
    /// instead of jumping one riser per boundary, while the rendered treads,
    /// collision rims and prop placement keep using the stepped surface.
    #[must_use]
    pub fn walk_height_at(&self, x: f32, z: f32) -> Option<f32> {
        self.resolve_height_at(x, z, StairSampling::PitchLine)
    }

    /// Closest continuous walking support within one permitted step of the
    /// current feet. Retains storey ownership for legacy scripted routes.
    #[must_use]
    pub fn walk_height_near(&self, x: f32, z: f32, feet: f32, step: f32) -> Option<f32> {
        if ![x, z, feet, step].iter().all(|value| value.is_finite()) || step < 0.0 {
            return None;
        }
        let mut nearest: Option<f32> = None;
        self.for_each_walk_surface(x, z, |height| {
            let delta = (height - feet).abs();
            if delta <= step + crate::engine::collision::STEP_EPS
                && nearest.is_none_or(|previous| delta < (previous - feet).abs())
            {
                nearest = Some(height);
            }
        });
        nearest
    }

    /// Visits every authored walking surface at a point, including stacked rooms.
    ///
    /// The compiler uses the same stair pitch line as physical movement. Room
    /// ownership is retained within each storey; duplicate adjoining surfaces
    /// are harmless and may be deduplicated by the caller.
    pub(crate) fn for_each_walk_surface(&self, x: f32, z: f32, mut visit: impl FnMut(f32)) {
        for room in &self.rooms {
            if let Some(height) = Self::room_height_at(room, x, z, StairSampling::PitchLine) {
                visit(height);
            }
        }
    }

    /// True when this exact storey's walking surface is a ramp or stair pitch.
    #[must_use]
    pub(crate) fn walk_surface_sloped(&self, x: f32, z: f32, y: f32) -> bool {
        self.rooms.iter().any(|room| {
            Self::room_height_at(room, x, z, StairSampling::PitchLine)
                .is_some_and(|height| (height - y).abs() <= crate::engine::collision::STEP_EPS)
                && (room.ramps.iter().any(|ramp| ramp.surface.contains(x, z))
                    || room.stairs.iter().any(|stair| stair.surface.contains(x, z)))
        })
    }

    /// Highest rendered floor no higher than `max_y`, across all storeys.
    ///
    /// Rendering retains authored room ownership. Physical contacts must also
    /// consider the other emitted floors in a stacked footprint: an unrelated
    /// upper floor must not hide a lower landing surface.
    #[must_use]
    pub fn height_below(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        self.resolve_height_below(x, z, max_y, StairSampling::Stepped)
    }

    /// Room and ramp-edge support includes the feet disc beyond its centre.
    /// Stair walking remains centre-sampled so its first riser occurs at the
    /// actual flight foot, followed by the continuous nosing pitch line.
    #[must_use]
    pub fn height_below_disc(&self, point: glam::Vec2, radius: f32, max_y: f32) -> Option<f32> {
        self.rooms
            .iter()
            .flat_map(|room| {
                let x = point.x.clamp(room.x0, room.x1);
                let z = point.y.clamp(room.z0, room.z1);
                let floor = (point.distance_squared(glam::Vec2::new(x, z)) < radius * radius)
                    .then(|| Self::room_height_at(room, x, z, StairSampling::Stepped))
                    .flatten()
                    .filter(|height| *height <= max_y);
                // An internal ramp edge is a landing surface too. Otherwise a
                // descending body's leading edge enters the wedge while its
                // centre still samples the lower room floor beside the ramp.
                let ramps = room.ramps.iter().filter_map(move |ramp| {
                    let (x0, x1, z0, z1) = ramp.surface.bounds();
                    let ramp_x = point.x.clamp(x0, x1);
                    let ramp_z = point.y.clamp(z0, z1);
                    let height = ramp.height_at(ramp_x, ramp_z);
                    (point.distance_squared(glam::Vec2::new(ramp_x, ramp_z)) < radius * radius
                        && height <= max_y)
                        .then_some(height)
                });
                floor.into_iter().chain(ramps)
            })
            .max_by(f32::total_cmp)
    }

    /// Highest support touched by a fixed square hull. Explicit ramps and
    /// treads are clipped to its whole rectangular footprint; room samples
    /// retain the existing ordered floor-region ownership.
    pub(crate) fn hull_support(
        &self,
        point: glam::Vec2,
        half: f32,
        max_y: f32,
    ) -> Option<(f32, glam::Vec3)> {
        let mut best: Option<(f32, glam::Vec3)> = None;
        let mut offer = |height: f32, normal: glam::Vec3| {
            if height <= max_y && best.is_none_or(|current| height >= current.0) {
                best = Some((height, normal));
            }
        };
        for room in &self.rooms {
            let low_x = (point.x - half).max(room.x0);
            let high_x = (point.x + half).min(room.x1);
            let low_z = (point.y - half).max(room.z0);
            let high_z = (point.y + half).min(room.z1);
            if low_x >= high_x || low_z >= high_z {
                continue;
            }
            for x in [low_x, point.x.clamp(low_x, high_x), high_x] {
                for z in [low_z, point.y.clamp(low_z, high_z), high_z] {
                    if let Some(height) = Self::room_height_at(room, x, z, StairSampling::Stepped) {
                        offer(height, glam::Vec3::Y);
                    }
                }
            }
            for ramp in &room.ramps {
                let (x0, x1, z0, z1) = ramp.surface.bounds();
                let a = (point.x - half).max(x0);
                let b = (point.x + half).min(x1);
                let c = (point.y - half).max(z0);
                let d = (point.y + half).min(z1);
                if a >= b || c >= d {
                    continue;
                }
                let top = [ramp.height_at(a, c), ramp.height_at(b, d)]
                    .into_iter()
                    .max_by(f32::total_cmp);
                let gradient = ramp.surface.rise()
                    / match ramp.surface.axis() {
                        WallAxis::X => x1 - x0,
                        WallAxis::Z => z1 - z0,
                    };
                let normal = match ramp.surface.axis() {
                    WallAxis::X => glam::Vec3::new(-gradient, 1.0, 0.0),
                    WallAxis::Z => glam::Vec3::new(0.0, 1.0, -gradient),
                }
                .normalize();
                if let Some(height) = top {
                    offer(height, normal);
                }
            }
            for stair in &room.stairs {
                let (x0, x1, z0, z1) = stair.surface.bounds();
                let a = (point.x - half).max(x0);
                let b = (point.x + half).min(x1);
                let c = (point.y - half).max(z0);
                let d = (point.y + half).min(z1);
                if a < b && c < d {
                    offer(
                        stair.height_at(a, c).max(stair.height_at(b, d)),
                        glam::Vec3::Y,
                    );
                }
            }
        }
        best
    }

    /// Exposed ramp/flight perimeters and stair risers in the live body band.
    ///
    /// Clip the sloping side at the exact feet plane (or grounded step reach),
    /// rather than making a whole-run box whose highest endpoint walls off the
    /// low end. Both signed rises share the rendered ramp's surface arithmetic.
    /// Ramp interiors stay heightfields; stair contacts use actual tread spans.
    pub(crate) fn for_each_terrain_barrier(
        &self,
        feet: f32,
        height: f32,
        step: f32,
        mut visit: impl FnMut(&WallAabb, glam::Vec2),
    ) {
        let threshold = feet + step + crate::engine::collision::CONTACT_EPS;
        for room in &self.rooms {
            for ramp in &room.ramps {
                let backing = room.floor_y + ramp.surface.low_offset().min(0.0);
                if feet + height <= backing + crate::engine::collision::CONTACT_EPS
                    || room.floor_y + ramp.surface.high_offset() <= threshold
                {
                    continue;
                }
                let (x0, x1, z0, z1) = ramp.surface.bounds();
                let (start, end) = match ramp.surface.axis() {
                    WallAxis::X => (x0, x1),
                    WallAxis::Z => (z0, z1),
                };
                let rise = ramp.surface.rise();
                let fraction = if rise.abs() > crate::engine::collision::CONTACT_EPS {
                    ((threshold - room.floor_y - ramp.surface.base_offset()) / rise).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let crossing = (end - start).mul_add(fraction, start);
                // An airborne body can enter the rising slope from *inside*
                // the footprint. Perimeter faces alone cannot catch it. The
                // contour at the feet plane closes that horizontal section of
                // the wedge; walking continues to use its legal step rule.
                if step <= 0.0 && fraction > 0.0 && fraction < 1.0 {
                    let outward = -rise.signum();
                    match ramp.surface.axis() {
                        WallAxis::X => visit(
                            &WallAabb::with_y(crossing, backing, z0, 0.0, 0.0, z1 - z0),
                            glam::Vec2::new(outward, 0.0),
                        ),
                        WallAxis::Z => visit(
                            &WallAabb::with_y(x0, backing, crossing, x1 - x0, 0.0, 0.0),
                            glam::Vec2::new(0.0, outward),
                        ),
                    }
                }
                let (low, high) = if rise > 0.0 {
                    (crossing, end)
                } else if rise < 0.0 {
                    (start, crossing)
                } else {
                    (start, end)
                };
                match ramp.surface.axis() {
                    WallAxis::X => {
                        for (z, normal) in [(z0, -glam::Vec2::Y), (z1, glam::Vec2::Y)] {
                            visit(
                                &WallAabb::with_y(low, backing, z, high - low, 0.0, 0.0),
                                normal,
                            );
                        }
                        for (x, normal) in [(x0, -glam::Vec2::X), (x1, glam::Vec2::X)] {
                            if ramp.height_at(x, z0) > threshold {
                                visit(&WallAabb::with_y(x, backing, z0, 0.0, 0.0, z1 - z0), normal);
                            }
                        }
                    }
                    WallAxis::Z => {
                        for (x, normal) in [(x0, -glam::Vec2::X), (x1, glam::Vec2::X)] {
                            visit(
                                &WallAabb::with_y(x, backing, low, 0.0, 0.0, high - low),
                                normal,
                            );
                        }
                        for (z, normal) in [(z0, -glam::Vec2::Y), (z1, glam::Vec2::Y)] {
                            if ramp.height_at(x0, z) > threshold {
                                visit(&WallAabb::with_y(x0, backing, z, x1 - x0, 0.0, 0.0), normal);
                            }
                        }
                    }
                }
            }
            for stair in &room.stairs {
                stair.for_each_body_barrier(feet, height, step, &mut visit);
            }
        }
    }

    /// A raised room's floor also blocks a head approaching from underneath.
    /// Regions in the player's own room are solid step volumes, handled by
    /// their rims; they are not an underside to clamp the player downward.
    #[must_use]
    pub fn lowest_room_underside(&self, point: glam::Vec2, radius: f32, feet: f32) -> Option<f32> {
        self.rooms
            .iter()
            .filter(|room| room.floor_y > feet + crate::engine::collision::CONTACT_EPS)
            .filter_map(|room| {
                let x = point.x.clamp(room.x0, room.x1);
                let z = point.y.clamp(room.z0, room.z1);
                if point.distance_squared(glam::Vec2::new(x, z)) >= radius * radius {
                    return None;
                }
                Self::room_height_at(room, x, z, StairSampling::Stepped)
                    .map(|top| {
                        let ramp_low = room
                            .ramps
                            .iter()
                            .find(|r| r.surface.contains(x, z))
                            .map_or(top, |r| r.floor_y + r.surface.low_offset());
                        top.min(room.floor_y).min(ramp_low) - MIN_STRUCTURE_M
                    })
                    .filter(|height| *height > feet + crate::engine::collision::CONTACT_EPS)
            })
            .min_by(f32::total_cmp)
    }

    /// Same authored surfaces queried by the fixed square player hull.
    pub(crate) fn lowest_room_underside_hull(
        &self,
        point: glam::Vec2,
        radius: f32,
        feet: f32,
    ) -> Option<f32> {
        self.rooms
            .iter()
            .filter(|room| room.floor_y > feet + crate::engine::collision::CONTACT_EPS)
            .filter_map(|room| {
                let x = point.x.clamp(room.x0, room.x1);
                let z = point.y.clamp(room.z0, room.z1);
                if (point.x - x).abs() >= radius || (point.y - z).abs() >= radius {
                    return None;
                }
                Self::room_height_at(room, x, z, StairSampling::Stepped)
                    .map(|top| {
                        let ramp_low = room
                            .ramps
                            .iter()
                            .find(|r| r.surface.contains(x, z))
                            .map_or(top, |r| r.floor_y + r.surface.low_offset());
                        top.min(room.floor_y).min(ramp_low) - MIN_STRUCTURE_M
                    })
                    .filter(|height| *height > feet + crate::engine::collision::CONTACT_EPS)
            })
            .min_by(f32::total_cmp)
    }

    /// Outer floor boundaries with a sample of the real surface at contact.
    /// Local regions, ramps and stairs determine the edge height, so a room's
    /// base floor cannot fabricate a slab through a recessed region.
    pub(crate) fn for_each_boundary(
        &self,
        mut visit: impl FnMut(&WallAabb, &dyn Fn(f32, f32) -> f32),
    ) {
        for room in &self.rooms {
            let bounds = WallAabb::with_y(
                room.x0,
                room.floor_y,
                room.z0,
                room.x1 - room.x0,
                0.0,
                room.z1 - room.z0,
            );
            let surface = |x: f32, z: f32| {
                Self::room_height_at(room, x, z, StairSampling::Stepped).unwrap_or(room.floor_y)
            };
            visit(&bounds, &surface);
        }
    }

    /// Highest walking surface within the supplied vertical reach.
    #[must_use]
    pub fn walk_height_below(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        // Reach is measured against the real tread, not the smoothing pitch
        // line. A legal first riser must remain climbable when that line has
        // already advanced partway toward the next nosing.
        self.rooms
            .iter()
            .filter_map(|room| {
                let tread = Self::room_height_at(room, x, z, StairSampling::Stepped)?;
                (tread <= max_y).then(|| {
                    (
                        tread,
                        Self::room_height_at(room, x, z, StairSampling::PitchLine),
                    )
                })
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .and_then(|(_, pitch)| pitch)
    }

    fn resolve_height_below(
        &self,
        x: f32,
        z: f32,
        max_y: f32,
        sampling: StairSampling,
    ) -> Option<f32> {
        self.rooms
            .iter()
            .filter_map(|room| Self::room_height_at(room, x, z, sampling))
            .filter(|height| *height <= max_y)
            .max_by(f32::total_cmp)
    }

    /// The height resolution shared by [`Self::height_at`] and
    /// [`Self::walk_height_at`]; `stair_sampling` selects the staircase's
    /// surface, everything else resolves identically.
    fn resolve_height_at(&self, x: f32, z: f32, stair_sampling: StairSampling) -> Option<f32> {
        self.rooms
            .iter()
            .find_map(|room| Self::room_height_at(room, x, z, stair_sampling))
    }

    fn room_height_at(
        room: &WalkableRoom,
        x: f32,
        z: f32,
        stair_sampling: StairSampling,
    ) -> Option<f32> {
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        if x < room.x0 - ROOM_EDGE_EPS_M
            || x > room.x1 + ROOM_EDGE_EPS_M
            || z < room.z0 - ROOM_EDGE_EPS_M
            || z > room.z1 + ROOM_EDGE_EPS_M
        {
            return None;
        }
        // Edge tolerance belongs to room selection. Clamp surface samples to
        // that room so a region ending at the shared edge cannot disappear
        // in the epsilon strip and briefly expose the lower base floor.
        let room_x = x.clamp(room.x0, room.x1);
        let room_z = z.clamp(room.z0, room.z1);
        if let Some(ramp) = room
            .ramps
            .iter()
            .rev()
            .find(|ramp| ramp.surface.contains(room_x, room_z))
        {
            return Some(ramp.height_at(room_x, room_z));
        }
        if let Some(stair) = room
            .stairs
            .iter()
            .rev()
            .find(|stair| stair.surface.contains(room_x, room_z))
        {
            return Some(match stair_sampling {
                StairSampling::Stepped => stair.height_at(room_x, room_z),
                StairSampling::PitchLine => stair.pitch_height_at(room_x, room_z),
            });
        }
        for region in room.regions.iter().rev() {
            if room_x >= region.x0
                && room_x <= region.x1
                && room_z >= region.z0
                && room_z <= region.z1
            {
                return Some(region.y);
            }
        }
        Some(room.floor_y)
    }

    /// Encodes this floor model into a compiled collision record: the room
    /// count, then each room's footprint, ramps, staircases and regions.
    ///
    /// # Errors
    ///
    /// Returns a named error when the model holds more rooms than
    /// [`crate::engine::package::MAX_COLLISION_BOXES`], or a room holds more ramps,
    /// stairs or regions than that same bound.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.rooms.len(),
            collision_box_limit(),
            "walkable floor room count",
        )?;
        for room in &self.rooms {
            write_walkable_room(writer, room)?;
        }
        Ok(())
    }

    /// Decodes a floor model from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a non-finite or inverted value.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let room_count = reader.count(collision_box_limit(), "walkable floor room count")?;
        let mut rooms = Vec::with_capacity(room_count.min(1024));
        for _ in 0..room_count {
            rooms.push(read_walkable_room(reader)?);
        }
        Ok(Self { rooms })
    }
}

/// The collision-record entry bound as a `u64` reader limit.
fn collision_box_limit() -> u64 {
    u64::try_from(crate::engine::package::MAX_COLLISION_BOXES).unwrap_or(u64::MAX)
}

/// True when every value is finite.
fn collision_values_finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Writes one counted section header, rejecting counts over `limit`.
fn write_collision_count(
    writer: &mut Writer,
    length: usize,
    limit: u64,
    what: &str,
) -> Result<(), String> {
    if u64::try_from(length).unwrap_or(u64::MAX) > limit {
        return Err(format!("{what} {length} exceeds the limit {limit}"));
    }
    writer.u32(u32::try_from(length).map_err(|error| format!("{what} is too large: {error}"))?);
    Ok(())
}

/// Encodes one walkable room: its footprint and floor, then its ramps,
/// staircases and regions.
fn write_walkable_room(writer: &mut Writer, room: &WalkableRoom) -> Result<(), String> {
    writer.f32(room.x0);
    writer.f32(room.x1);
    writer.f32(room.z0);
    writer.f32(room.z1);
    writer.f32(room.floor_y);
    write_collision_count(
        writer,
        room.ramps.len(),
        collision_box_limit(),
        "walkable ramp count",
    )?;
    for ramp in &room.ramps {
        write_walkable_ramp(writer, ramp);
    }
    write_collision_count(
        writer,
        room.stairs.len(),
        collision_box_limit(),
        "walkable stair count",
    )?;
    for stair in &room.stairs {
        write_walkable_stair(writer, stair);
    }
    write_collision_count(
        writer,
        room.regions.len(),
        collision_box_limit(),
        "walkable region count",
    )?;
    for region in &room.regions {
        write_walkable_region(writer, region);
    }
    Ok(())
}

/// Decodes one walkable room.
fn read_walkable_room(reader: &mut Reader<'_>) -> Result<WalkableRoom, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let floor_y = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, floor_y]) {
        return Err("walkable room has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("walkable room bounds are inverted".to_string());
    }
    let ramp_count = reader.count(collision_box_limit(), "walkable ramp count")?;
    let mut ramps = Vec::with_capacity(ramp_count.min(1024));
    for _ in 0..ramp_count {
        ramps.push(read_walkable_ramp(reader)?);
    }
    let stair_count = reader.count(collision_box_limit(), "walkable stair count")?;
    let mut stairs = Vec::with_capacity(stair_count.min(1024));
    for _ in 0..stair_count {
        stairs.push(read_walkable_stair(reader)?);
    }
    let region_count = reader.count(collision_box_limit(), "walkable region count")?;
    let mut regions = Vec::with_capacity(region_count.min(1024));
    for _ in 0..region_count {
        regions.push(read_walkable_region(reader)?);
    }
    Ok(WalkableRoom {
        x0,
        x1,
        z0,
        z1,
        floor_y,
        ramps,
        stairs,
        regions,
    })
}

/// Encodes one ramp's detached surface and the floor plane it sits on.
fn write_walkable_ramp(writer: &mut Writer, ramp: &WalkableRamp) {
    writer.f32(ramp.surface.x);
    writer.f32(ramp.surface.z);
    writer.f32(ramp.surface.width);
    writer.f32(ramp.surface.depth);
    writer.f32(ramp.surface.offset_y);
    writer.f32(ramp.surface.rise);
    writer.f32(ramp.floor_y);
}

/// Decodes one walkable ramp, rejecting malformed surfaces.
fn read_walkable_ramp(reader: &mut Reader<'_>) -> Result<WalkableRamp, String> {
    let x = reader.f32()?;
    let z = reader.f32()?;
    let width = reader.f32()?;
    let depth = reader.f32()?;
    let offset_y = reader.f32()?;
    let rise = reader.f32()?;
    let floor_y = reader.f32()?;
    if !collision_values_finite(&[x, z, width, depth, offset_y, rise, floor_y]) {
        return Err("walkable ramp has a non-finite value".to_string());
    }
    if width == 0.0 || depth == 0.0 {
        return Err("walkable ramp has a zero width or depth".to_string());
    }
    Ok(WalkableRamp {
        surface: RampSurface {
            x,
            z,
            width,
            depth,
            offset_y,
            rise,
        },
        floor_y,
    })
}

/// Encodes one staircase's detached surface and the floor plane it sits on.
fn write_walkable_stair(writer: &mut Writer, stair: &WalkableStair) {
    writer.f32(stair.surface.x);
    writer.f32(stair.surface.z);
    writer.f32(stair.surface.width);
    writer.f32(stair.surface.depth);
    writer.f32(stair.surface.offset_y);
    writer.f32(stair.surface.rise);
    writer.u32(stair.surface.steps);
    writer.f32(stair.floor_y);
}

/// Decodes one walkable staircase, rejecting malformed surfaces.
fn read_walkable_stair(reader: &mut Reader<'_>) -> Result<WalkableStair, String> {
    let x = reader.f32()?;
    let z = reader.f32()?;
    let width = reader.f32()?;
    let depth = reader.f32()?;
    let offset_y = reader.f32()?;
    let rise = reader.f32()?;
    // `steps` is a `u32`, so a decoded count can never be negative.
    let steps = reader.u32()?;
    let floor_y = reader.f32()?;
    if !collision_values_finite(&[x, z, width, depth, offset_y, rise, floor_y]) {
        return Err("walkable stair has a non-finite value".to_string());
    }
    if width == 0.0 || depth == 0.0 {
        return Err("walkable stair has a zero width or depth".to_string());
    }
    Ok(WalkableStair {
        surface: StairSurface {
            x,
            z,
            width,
            depth,
            offset_y,
            rise,
            steps,
        },
        floor_y,
    })
}

/// Encodes one floor region.
fn write_walkable_region(writer: &mut Writer, region: &WalkableRegion) {
    writer.f32(region.x0);
    writer.f32(region.x1);
    writer.f32(region.z0);
    writer.f32(region.z1);
    writer.f32(region.y);
}

/// Decodes one floor region.
fn read_walkable_region(reader: &mut Reader<'_>) -> Result<WalkableRegion, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let y = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, y]) {
        return Err("walkable region has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("walkable region bounds are inverted".to_string());
    }
    Ok(WalkableRegion { x0, x1, z0, z1, y })
}

/// One room of the walkable ceiling model.
///
/// The room's profile maths stay in [`ceiling_y_for_volume`], so the ceiling a
/// jumping player bumps against is by construction the same surface the mesh
/// and the fixtures resolved.
#[derive(Debug, Clone, PartialEq)]
struct WalkableCeilingRoom {
    /// Room footprint `(x0, x1, z0, z1)`, normalised.
    bounds: (f32, f32, f32, f32),
    floor_y: f32,
    height: f32,
    profile: CeilingProfileDef,
}

impl WalkableCeilingRoom {
    /// True when `(x, z)` lies inside the footprint, with the same edge
    /// tolerance every other surface query uses.
    fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds;
        x >= x0 - ROOM_EDGE_EPS_M
            && x <= x1 + ROOM_EDGE_EPS_M
            && z >= z0 - ROOM_EDGE_EPS_M
            && z <= z1 + ROOM_EDGE_EPS_M
    }

    /// World Y of this room's ceiling at `(x, z)`.
    fn ceiling_y_at(&self, x: f32, z: f32) -> f32 {
        ceiling_y_for_volume(self.bounds, self.floor_y, self.height, self.profile, x, z)
    }

    /// Exact height extrema where a disc meets this flat or gabled patch.
    fn disc_height_range(&self, point: glam::Vec2, radius: f32) -> Option<[f32; 2]> {
        self.shape_height_range(point, radius, false)
    }

    fn hull_height_range(&self, point: glam::Vec2, radius: f32) -> Option<[f32; 2]> {
        self.shape_height_range(point, radius, true)
    }

    fn shape_height_range(&self, point: glam::Vec2, radius: f32, square: bool) -> Option<[f32; 2]> {
        let (x0, x1, z0, z1) = self.bounds;
        let x = point.x.clamp(x0, x1);
        let z = point.y.clamp(z0, z1);
        let dx = point.x - x;
        let dz = point.y - z;
        if if square {
            dx.abs() >= radius || dz.abs() >= radius
        } else {
            dx.mul_add(dx, dz * dz) >= radius * radius
        } {
            return None;
        }
        if self.profile == CeilingProfileDef::Flat {
            let height = self.floor_y + self.height;
            return Some([height, height]);
        }
        let reach_x = if square {
            radius
        } else {
            (radius.mul_add(radius, -(dz * dz))).max(0.0).sqrt()
        };
        let reach_z = if square {
            radius
        } else {
            (radius.mul_add(radius, -(dx * dx))).max(0.0).sqrt()
        };
        let lo_x = (point.x - reach_x).clamp(x0, x1);
        let hi_x = (point.x + reach_x).clamp(x0, x1);
        let lo_z = (point.y - reach_z).clamp(z0, z1);
        let hi_z = (point.y + reach_z).clamp(z0, z1);
        // A gable is constant along its ridge. The perpendicular endpoints
        // give its minimum; the ridge, when touched, gives its maximum.
        let heights = [
            self.ceiling_y_at(lo_x, z),
            self.ceiling_y_at(hi_x, z),
            self.ceiling_y_at(x, lo_z),
            self.ceiling_y_at(x, hi_z),
            self.ceiling_y_at(f32::midpoint(x0, x1).clamp(lo_x, hi_x), z),
            self.ceiling_y_at(x, f32::midpoint(z0, z1).clamp(lo_z, hi_z)),
        ];
        Some(
            heights
                .into_iter()
                .fold([f32::INFINITY, f32::NEG_INFINITY], |[low, high], value| {
                    [low.min(value), high.max(value)]
                }),
        )
    }
}

/// Owned, allocation-light ceiling model a controller samples while jumping.
///
/// It is built once per level from the same rooms and profiles
/// [`LevelSurfaces::ceiling_y_at`] resolves against, including a gable's
/// ridge, but borrows nothing from the definition. Unlike the borrowing query a
/// point outside every room answers `None`: an off-room spawn or a walkable
/// void has no ceiling to clamp against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalkableCeiling {
    rooms: Vec<WalkableCeilingRoom>,
}

impl WalkableCeiling {
    /// Retained room storage for the prepared-world cache.
    #[must_use]
    pub(crate) const fn retained_heap_bytes(&self) -> usize {
        self.rooms
            .capacity()
            .saturating_mul(std::mem::size_of::<WalkableCeilingRoom>())
    }

    /// Builds the ceiling model for a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let rooms = level
            .room_iter()
            // An open-ceiling room has no surface to clamp a jumping player
            // against; it contributes nothing to this model.
            .filter(|room| !room.ceiling.is_open())
            .map(|room| WalkableCeilingRoom {
                bounds: room.bounds(),
                floor_y: if room.floor_y.is_finite() {
                    room.floor_y
                } else {
                    0.0
                },
                height: room.height,
                profile: room.ceiling,
            })
            .collect();
        Self { rooms }
    }

    /// True when the level contains no rooms at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// World Y of the ceiling over `(x, z)`, or `None` outside every room.
    ///
    /// The first room in level order containing the point wins, matching the
    /// ownership rule of [`LevelSurfaces::ceiling_y_at`].
    #[must_use]
    pub fn ceiling_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.rooms
            .iter()
            .find(|room| room.contains(x, z))
            .map(|room| room.ceiling_y_at(x, z))
    }

    /// Lowest ceiling touched by the body disc and above its feet.
    ///
    /// The vertical reference prevents a lower storey's ceiling from pulling
    /// an airborne player down. Sampling the whole disc prevents head clipping
    /// at room edges and on pitched ceilings. Surfaces below the feet can
    /// instead receive a descending player through [`Self::surface_below_disc`].
    #[must_use]
    pub fn ceiling_above_disc(&self, x: f32, z: f32, radius: f32, feet: f32) -> Option<f32> {
        self.rooms
            .iter()
            .filter_map(|room| {
                let [height, _] = room.disc_height_range(glam::Vec2::new(x, z), radius)?;
                (height > feet + crate::engine::collision::CONTACT_EPS).then_some(height)
            })
            .min_by(f32::total_cmp)
    }

    /// Same authored surfaces queried by the fixed square player hull.
    pub(crate) fn ceiling_above_hull(&self, x: f32, z: f32, radius: f32, feet: f32) -> Option<f32> {
        self.rooms
            .iter()
            .filter_map(|room| {
                let [height, _] = room.hull_height_range(glam::Vec2::new(x, z), radius)?;
                (height > feet + crate::engine::collision::CONTACT_EPS).then_some(height)
            })
            .min_by(f32::total_cmp)
    }

    /// Highest emitted ceiling plane beneath a descending player's disc.
    ///
    /// A ceiling is also a solid platform from above; selecting it as an
    /// underside while already above it was the stacked-balcony teleport.
    #[must_use]
    pub fn surface_below_disc(&self, point: glam::Vec2, radius: f32, max_y: f32) -> Option<f32> {
        self.rooms
            .iter()
            .filter_map(|room| room.disc_height_range(point, radius).map(|[_, high]| high))
            .filter(|height| *height <= max_y)
            .max_by(f32::total_cmp)
    }

    /// Same authored surfaces queried by the fixed square player hull.
    pub(crate) fn surface_below_hull(
        &self,
        point: glam::Vec2,
        radius: f32,
        max_y: f32,
    ) -> Option<f32> {
        self.rooms
            .iter()
            .filter_map(|room| room.hull_height_range(point, radius).map(|[_, high]| high))
            .filter(|height| *height <= max_y)
            .max_by(f32::total_cmp)
    }

    /// Ceiling patches crossing a body, including a pitched plane crossing its feet.
    pub(crate) fn for_each_body_contact(
        &self,
        point: glam::Vec2,
        radius: f32,
        feet: f32,
        height: f32,
        mut visit: impl FnMut([f32; 2]),
    ) {
        for room in &self.rooms {
            if let Some([low, high]) = room.disc_height_range(point, radius)
                && low < feet + height - crate::engine::collision::CONTACT_EPS
                && high > feet + crate::engine::collision::CONTACT_EPS
            {
                visit([low, high]);
            }
        }
    }

    /// Same authored surfaces queried by the fixed square player hull.
    pub(crate) fn for_each_hull_contact(
        &self,
        point: glam::Vec2,
        radius: f32,
        feet: f32,
        height: f32,
        mut visit: impl FnMut([f32; 2]),
    ) {
        for room in &self.rooms {
            if let Some([low, high]) = room.hull_height_range(point, radius)
                && low < feet + height - crate::engine::collision::CONTACT_EPS
                && high > feet + crate::engine::collision::CONTACT_EPS
            {
                visit([low, high]);
            }
        }
    }

    /// Horizontal footprints of ceiling patches crossing the live body band.
    /// A jump beside a low room must hit the edge of its ceiling as a solid;
    /// a later downward clamp cannot repair that lateral penetration.
    pub(crate) fn for_each_body_barrier(
        &self,
        feet: f32,
        height: f32,
        mut visit: impl FnMut(&WallAabb),
    ) {
        let low = feet + crate::engine::collision::CONTACT_EPS;
        let high = feet + height - crate::engine::collision::CONTACT_EPS;
        for room in &self.rooms {
            let (x0, x1, z0, z1) = room.bounds;
            let eave = room.floor_y + room.height;
            match room.profile {
                CeilingProfileDef::Gable { ridge, ridge_rise } if ridge_rise > 0.0 => {
                    if high <= eave || low >= eave + ridge_rise {
                        continue;
                    }
                    let inner = 1.0 - ((high - eave) / ridge_rise).clamp(0.0, 1.0);
                    let outer = 1.0 - ((low - eave) / ridge_rise).clamp(0.0, 1.0);
                    let (a, b) = match ridge {
                        WallAxis::X => (z0, z1),
                        WallAxis::Z => (x0, x1),
                    };
                    let centre = f32::midpoint(a, b);
                    let half = (b - a) * 0.5;
                    for (start, end) in [
                        (half.mul_add(-outer, centre), half.mul_add(-inner, centre)),
                        (half.mul_add(inner, centre), half.mul_add(outer, centre)),
                    ] {
                        let wall = match ridge {
                            WallAxis::X => {
                                WallAabb::with_y(x0, low, start, x1 - x0, height, end - start)
                            }
                            WallAxis::Z => {
                                WallAabb::with_y(start, low, z0, end - start, height, z1 - z0)
                            }
                        };
                        visit(&wall);
                    }
                }
                CeilingProfileDef::Flat
                | CeilingProfileDef::Gable {
                    ridge: _,
                    ridge_rise: _,
                } if eave > low && eave < high => {
                    visit(&WallAabb::with_y(x0, eave, z0, x1 - x0, 0.0, z1 - z0));
                }
                CeilingProfileDef::Flat
                | CeilingProfileDef::Gable {
                    ridge: _,
                    ridge_rise: _,
                }
                | CeilingProfileDef::Open => {}
            }
        }
    }

    /// Encodes this ceiling model into a compiled collision record: the room
    /// count, then each room's footprint, eave and profile.
    ///
    /// # Errors
    ///
    /// Returns a named error when the model holds more rooms than
    /// [`crate::engine::package::MAX_COLLISION_BOXES`].
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.rooms.len(),
            collision_box_limit(),
            "walkable ceiling room count",
        )?;
        for room in &self.rooms {
            write_ceiling_room(writer, room);
        }
        Ok(())
    }

    /// Decodes a ceiling model from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a malformed room or profile.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let room_count = reader.count(collision_box_limit(), "walkable ceiling room count")?;
        let mut rooms = Vec::with_capacity(room_count.min(1024));
        for _ in 0..room_count {
            rooms.push(read_ceiling_room(reader)?);
        }
        Ok(Self { rooms })
    }
}

/// Profile byte code of a flat ceiling.
const CEILING_PROFILE_FLAT: u8 = 0;

/// Profile byte code of a gable ceiling.
const CEILING_PROFILE_GABLE: u8 = 1;

/// Ridge-axis byte code of a gable running along X.
const CEILING_RIDGE_X: u8 = 0;

/// Ridge-axis byte code of a gable running along Z.
const CEILING_RIDGE_Z: u8 = 1;

/// Encodes one walkable ceiling room: its footprint, eave and profile.
fn write_ceiling_room(writer: &mut Writer, room: &WalkableCeilingRoom) {
    let (x0, x1, z0, z1) = room.bounds;
    writer.f32(x0);
    writer.f32(x1);
    writer.f32(z0);
    writer.f32(z1);
    writer.f32(room.floor_y);
    writer.f32(room.height);
    match room.profile {
        // An open room never reaches the record: `WalkableCeiling::from_level`
        // filters it. A stray one degrades to its eave (flat), never a panic.
        CeilingProfileDef::Flat | CeilingProfileDef::Open => writer.u8(CEILING_PROFILE_FLAT),
        CeilingProfileDef::Gable { ridge, ridge_rise } => {
            writer.u8(CEILING_PROFILE_GABLE);
            writer.u8(match ridge {
                WallAxis::X => CEILING_RIDGE_X,
                WallAxis::Z => CEILING_RIDGE_Z,
            });
            writer.f32(ridge_rise);
        }
    }
}

/// Decodes one walkable ceiling room.
fn read_ceiling_room(reader: &mut Reader<'_>) -> Result<WalkableCeilingRoom, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let floor_y = reader.f32()?;
    let height = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, floor_y, height]) {
        return Err("walkable ceiling room has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("walkable ceiling room bounds are inverted".to_string());
    }
    let profile = match reader.u8()? {
        CEILING_PROFILE_FLAT => CeilingProfileDef::Flat,
        CEILING_PROFILE_GABLE => {
            let ridge = match reader.u8()? {
                CEILING_RIDGE_X => WallAxis::X,
                CEILING_RIDGE_Z => WallAxis::Z,
                other => return Err(format!("unknown gable ridge axis code {other}")),
            };
            let ridge_rise = reader.f32()?;
            if !ridge_rise.is_finite() {
                return Err("walkable gable has a non-finite ridge rise".to_string());
            }
            CeilingProfileDef::Gable { ridge, ridge_rise }
        }
        other => return Err(format!("unknown ceiling profile code {other}")),
    };
    Ok(WalkableCeilingRoom {
        bounds: (x0, x1, z0, z1),
        floor_y,
        height,
        profile,
    })
}

/// One water volume resolved against the level's floors: a footprint, a
/// horizontal surface, a bottom and the author's material/opacity/swimming
/// contract.
///
/// The bottom is resolved once, at level load: an authored `bottom_y` is kept,
/// and an omitted one becomes the lowest walkable floor under the footprint
/// (so a basin's water is as deep as the basin) with a `surface_y - 2.0`
/// fallback for a volume that floats over the void. Physics always stands on
/// the level's own walkable floor; the resolved bottom exists so authors and
/// renders can reason about the body as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterVolume {
    /// Footprint shape: the rectangle `x0..x1` × `z0..z1`, or the disc of
    /// [`WaterVolume::radius`] inscribed in that bounding box.
    pub shape: WaterShape,
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    /// Circle radius in metres; `0.0` for a rectangle.
    pub radius: f32,
    /// World Y of the free surface.
    pub surface_y: f32,
    /// World Y of the resolved bottom, always below `surface_y`.
    pub bottom_y: f32,
    /// Authored surface material id, or `None` for [`DEFAULT_WATER_MATERIAL`].
    pub material: Option<String>,
    /// Effective opacity of the surface, `0.0..=1.0`.
    pub opacity: f32,
    /// Whether the controller swims in this volume.
    pub swimming: bool,
    /// Whether the volume participates in the water sampling at all.
    ///
    /// A `disable` action clears this: the surface still draws (it is baked
    /// geometry) but the controller walks or falls through the volume.
    pub enabled: bool,
    /// Source controls restored beside compiled collision geometry.
    pub motion: WaterMotionDef,
}

impl WaterVolume {
    /// Footprint centre `(x, z)`: the bounding box midpoint for a rectangle
    /// and the disc's centre for a circle.
    #[must_use]
    pub const fn center(&self) -> (f32, f32) {
        (
            f32::midpoint(self.x0, self.x1),
            f32::midpoint(self.z0, self.z1),
        )
    }

    /// True when `(x, z)` lies inside the footprint: the rectangle, or the
    /// disc.
    ///
    /// A circle's rim is the wall line and is **dry** (strictly inside the
    /// radius), so its bounding box's axis extremes are not water.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        match self.shape {
            WaterShape::Rect => x >= self.x0 && x <= self.x1 && z >= self.z0 && z <= self.z1,
            WaterShape::Circle => {
                let (cx, cz) = self.center();
                let dx = x - cx;
                let dz = z - cz;
                dx.mul_add(dx, dz * dz) < self.radius * self.radius
            }
        }
    }

    /// True when the whole disc of `radius` around `(x, z)` lies inside this
    /// volume's own footprint.
    ///
    /// A rectangle keeps the historical box containment; a circle requires the
    /// disc to fit inside the circle, so a floating prop can never poke past
    /// the rim of a round pool.
    #[must_use]
    pub fn contains_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        if !x.is_finite() || !z.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        match self.shape {
            WaterShape::Rect => {
                x - radius >= self.x0
                    && x + radius <= self.x1
                    && z - radius >= self.z0
                    && z + radius <= self.z1
            }
            WaterShape::Circle => {
                let (cx, cz) = self.center();
                let dx = x - cx;
                let dz = z - cz;
                let clearance = self.radius - radius;
                clearance >= 0.0 && dx.mul_add(dx, dz * dz) <= clearance * clearance
            }
        }
    }

    /// Resolved depth below the surface, in metres; always positive.
    #[must_use]
    pub fn depth(&self) -> f32 {
        (self.surface_y - self.bottom_y).max(0.0)
    }

    /// Surface material id: the authored one, or [`DEFAULT_WATER_MATERIAL`].
    #[must_use]
    pub fn material_id(&self) -> &str {
        self.material.as_deref().unwrap_or(DEFAULT_WATER_MATERIAL)
    }
}

/// What one point inside a water volume reports to the controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterSample {
    /// World Y of the free surface.
    pub surface_y: f32,
    /// World Y of the volume's resolved bottom.
    pub bottom_y: f32,
    /// Surface-to-bottom depth, in metres.
    pub depth: f32,
    /// Horizontal fluid velocity in world XZ metres per second.
    pub flow: [f32; 2],
    /// Whether the volume allows swimming.
    pub swimming: bool,
}

/// The level's water volumes, resolved once at load time.
///
/// Lookups are linear over the level's authored volumes (bounded by
/// [`MAX_LEVEL_WATER_VOLUMES`]) and allocation-free, exactly like the walkable
/// floor's room scan; a level with no `water` array is empty and every query
/// returns `None`, which is the historical dry level.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaterVolumes {
    volumes: Vec<WaterVolume>,
}

/// Points sampled per rectangle when resolving an omitted `bottom_y`: the
/// centre and the four footprint corners.
const WATER_BOTTOM_SAMPLES: [(f32, f32); 5] =
    [(0.5, 0.5), (0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

/// Points sampled per circle when resolving an omitted `bottom_y`: the centre
/// and four points at half the radius, on the diagonals. They lie strictly
/// inside the disc, so the resolution samples the circle's own footprint
/// rather than the bounding box corners a square would wrongly include.
const WATER_CIRCLE_SAMPLES: [(f32, f32); 5] = [
    (0.0, 0.0),
    (-0.353_553_4, -0.353_553_4),
    (0.353_553_4, -0.353_553_4),
    (0.353_553_4, 0.353_553_4),
    (-0.353_553_4, 0.353_553_4),
];

impl WaterVolumes {
    /// Retained volume and material-id storage for the prepared-world cache.
    #[must_use]
    pub(crate) fn retained_heap_bytes(&self) -> usize {
        self.volumes.iter().fold(
            self.volumes
                .capacity()
                .saturating_mul(std::mem::size_of::<WaterVolume>()),
            |bytes, volume| {
                bytes.saturating_add(volume.material.as_ref().map_or(0, String::capacity))
            },
        )
    }

    /// An empty set: every query misses.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            volumes: Vec::new(),
        }
    }

    /// Resolves every volume against the level's walkable floors.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut volumes = Vec::with_capacity(level.water.len());
        for def in &level.water {
            let (x0, x1, z0, z1) = def.bounds();
            if !x0.is_finite() || !x1.is_finite() || !z0.is_finite() || !z1.is_finite() {
                continue;
            }
            // A degenerate footprint is malformed input the loader rejects;
            // resolution skips it so it can never draw or answer a query.
            if x1 <= x0 || z1 <= z0 {
                continue;
            }
            let radius = match def.shape {
                WaterShape::Circle => match def.radius {
                    Some(radius) if radius.is_finite() && radius > 0.0 => radius,
                    _ => continue,
                },
                WaterShape::Rect => 0.0,
            };
            let surface_y = def.surface_y;
            if !surface_y.is_finite() {
                continue;
            }
            let bottom = def
                .bottom_y
                .filter(|value| value.is_finite())
                .unwrap_or_else(|| {
                    let mut lowest = f32::INFINITY;
                    match def.shape {
                        WaterShape::Rect => {
                            for (u, v) in WATER_BOTTOM_SAMPLES {
                                let x = (x1 - x0).mul_add(u, x0);
                                let z = (z1 - z0).mul_add(v, z0);
                                if let Some(floor) = surfaces.floor_y_at(x, z) {
                                    lowest = lowest.min(floor);
                                }
                            }
                        }
                        WaterShape::Circle => {
                            let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
                            for (du, dv) in WATER_CIRCLE_SAMPLES {
                                let x = radius.mul_add(du, cx);
                                let z = radius.mul_add(dv, cz);
                                if let Some(floor) = surfaces.floor_y_at(x, z) {
                                    lowest = lowest.min(floor);
                                }
                            }
                        }
                    }
                    if lowest.is_finite() {
                        lowest
                    } else {
                        surface_y - 2.0
                    }
                });
            // A bottom at or above the surface has no body to swim in; keep it
            // strictly below so depth() is always positive.
            let bottom_y = if bottom < surface_y {
                bottom
            } else {
                surface_y - 0.05
            };
            volumes.push(WaterVolume {
                shape: def.shape,
                x0,
                x1,
                z0,
                z1,
                radius,
                surface_y,
                bottom_y,
                material: def.material.clone(),
                opacity: if def.attenuation_per_metre > 0.0 {
                    (1.0 - def.opacity()).mul_add(
                        -(-def.attenuation_per_metre * (surface_y - bottom_y)).exp(),
                        1.0,
                    )
                } else {
                    def.opacity()
                },
                swimming: def.swimming,
                enabled: true,
                motion: def.motion,
            });
        }
        Self { volumes }
    }

    /// Restore source-only controls without changing the collision record layout.
    #[must_use]
    pub fn with_motion(mut self, level: &LevelDef) -> Self {
        for (volume, source) in self.volumes.iter_mut().zip(&level.water) {
            volume.motion = source.motion;
        }
        self
    }

    /// Enables or disables the volume at `index`.
    ///
    /// Returns whether the state changed; an out-of-range index returns false.
    /// A disabled volume is skipped by every query, so the controller treats
    /// its footprint as dry.
    pub fn set_enabled(&mut self, index: usize, enabled: bool) -> bool {
        let Some(volume) = self.volumes.get_mut(index) else {
            return false;
        };
        if volume.enabled == enabled {
            return false;
        }
        volume.enabled = enabled;
        true
    }

    /// True when the level defines no volumes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.volumes.is_empty()
    }

    /// Number of resolved volumes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.volumes.len()
    }

    /// Every resolved volume, in authored order.
    #[must_use]
    pub fn volumes(&self) -> &[WaterVolume] {
        &self.volumes
    }

    /// The water at `(x, z, y)`, or `None` when the point is outside every
    /// footprint or above its surface.
    ///
    /// Overlapping volumes resolve like overlapping floor regions: the last one
    /// authored wins.
    #[must_use]
    pub fn sample(&self, x: f32, z: f32, y: f32) -> Option<WaterSample> {
        if !y.is_finite() {
            return None;
        }
        for volume in self.volumes.iter().rev() {
            if !volume.enabled || !volume.contains(x, z) || y > volume.surface_y {
                continue;
            }
            return Some(WaterSample {
                surface_y: volume.surface_y,
                bottom_y: volume.bottom_y,
                depth: volume.depth(),
                flow: volume.motion.flow(),
                swimming: volume.swimming,
            });
        }
        None
    }

    /// World Y of the topmost surface covering `(x, z)`, ignoring height.
    #[must_use]
    pub fn surface_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.volumes
            .iter()
            .filter(|volume| volume.enabled && volume.contains(x, z))
            .map(|volume| volume.surface_y)
            .reduce(f32::max)
    }

    /// True when some volume contains the whole horizontal disc of `radius`
    /// around `(x, z)`.
    ///
    /// This is the containment guarantee behind a floating prop: the authored
    /// footprint (plus its heel excursion) must fit inside one basin, so the
    /// hull can never poke through a rim, whatever the phase. A circle
    /// requires the disc to fit inside the circle, not merely inside its
    /// bounding box.
    #[must_use]
    pub fn contains_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        if !x.is_finite() || !z.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        self.volumes
            .iter()
            .filter(|volume| volume.enabled)
            .any(|volume| volume.contains_disc(x, z, radius))
    }

    /// Encodes this volume set into a compiled collision record: the volume
    /// count, then each volume's footprint, surface, bottom and contract.
    ///
    /// # Errors
    ///
    /// Returns a named error when the set holds more volumes than the format
    /// admits or a material id is longer than the record bound.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.volumes.len(),
            MAX_COLLISION_WATER_VOLUMES,
            "water volume count",
        )?;
        for volume in &self.volumes {
            write_water_volume(writer, volume)?;
        }
        Ok(())
    }

    /// Decodes a volume set from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a malformed volume.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let volume_count = reader.count(MAX_COLLISION_WATER_VOLUMES, "water volume count")?;
        let mut volumes = Vec::with_capacity(volume_count.min(256));
        for _ in 0..volume_count {
            volumes.push(read_water_volume(reader)?);
        }
        Ok(Self { volumes })
    }
}

/// Shape code of a rectangular compiled water volume.
const WATER_SHAPE_RECT: u8 = 0;
/// Shape code of a circular compiled water volume.
const WATER_SHAPE_CIRCLE: u8 = 1;

/// Encodes one water volume: footprint, shape, surface, bottom, contract and
/// material.
///
/// The current record includes the shape byte and circle radius after
/// `opacity`; superseded records would misread the shape byte as `swimming`,
/// so it is refused by name.
fn write_water_volume(writer: &mut Writer, volume: &WaterVolume) -> Result<(), String> {
    writer.f32(volume.x0);
    writer.f32(volume.x1);
    writer.f32(volume.z0);
    writer.f32(volume.z1);
    writer.f32(volume.surface_y);
    writer.f32(volume.bottom_y);
    writer.f32(volume.opacity);
    match volume.shape {
        WaterShape::Rect => {
            writer.u8(WATER_SHAPE_RECT);
            writer.f32(0.0);
        }
        WaterShape::Circle => {
            writer.u8(WATER_SHAPE_CIRCLE);
            writer.f32(volume.radius);
        }
    }
    writer.bool(volume.swimming);
    match volume.material.as_deref() {
        None => writer.u8(0),
        Some(material) => {
            if u64::try_from(material.len()).unwrap_or(u64::MAX)
                > MAX_COLLISION_WATER_MATERIAL_BYTES
            {
                return Err(format!(
                    "water volume material id is {} bytes \
                     (limit {MAX_COLLISION_WATER_MATERIAL_BYTES})",
                    material.len()
                ));
            }
            writer.u8(1);
            writer.str(material)?;
        }
    }
    Ok(())
}

/// Decodes one water volume, rejecting a malformed shape/radius combination.
fn read_water_volume(reader: &mut Reader<'_>) -> Result<WaterVolume, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let surface_y = reader.f32()?;
    let bottom_y = reader.f32()?;
    let opacity = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, surface_y, bottom_y, opacity]) {
        return Err("water volume has a non-finite value".to_string());
    }
    if x1 <= x0 || z1 <= z0 {
        return Err("water volume bounds are inverted or empty".to_string());
    }
    let shape = match reader.u8()? {
        WATER_SHAPE_RECT => WaterShape::Rect,
        WATER_SHAPE_CIRCLE => WaterShape::Circle,
        other => return Err(format!("unknown water volume shape code {other}")),
    };
    let radius = reader.f32()?;
    if !radius.is_finite() {
        return Err("water volume has a non-finite radius".to_string());
    }
    match shape {
        WaterShape::Rect => {
            if radius != 0.0 {
                return Err("rectangular water volume carries a radius".to_string());
            }
        }
        WaterShape::Circle => {
            if radius <= 0.0 {
                return Err("circular water volume radius must be positive".to_string());
            }
            // The bounding box and the radius describe one disc; a record
            // whose box does not match a radius would make the drawn surface
            // and the membership test disagree.
            let diameter = 2.0 * radius;
            if (x1 - x0 - diameter).abs() > 1.0e-3 || (z1 - z0 - diameter).abs() > 1.0e-3 {
                return Err("circular water volume bounds do not match its radius".to_string());
            }
        }
    }
    let swimming = reader.bool()?;
    let material = match reader.u8()? {
        0 => None,
        1 => Some(reader.str(MAX_COLLISION_WATER_MATERIAL_BYTES)?),
        other => return Err(format!("invalid water material marker {other}")),
    };
    Ok(WaterVolume {
        shape,
        x0,
        x1,
        z0,
        z1,
        radius,
        surface_y,
        bottom_y,
        material,
        opacity,
        swimming,
        enabled: true,
        motion: WaterMotionDef::default(),
    })
}

/// One ladder resolved against the level, ready for the controller.
///
/// The facing vector is the direction the climber moves while climbing, in the
/// same convention as player movement: `(sin(yaw), -cos(yaw))` in `(x, z)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ladder {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    pub bottom_y: f32,
    pub top_y: f32,
    pub facing_x: f32,
    pub facing_z: f32,
}

impl Ladder {
    /// The footprint's centre `(x, z)`.
    #[must_use]
    pub const fn center(&self) -> (f32, f32) {
        (
            f32::midpoint(self.x0, self.x1),
            f32::midpoint(self.z0, self.z1),
        )
    }

    /// True when the player's disc at `(x, z)` touches the footprint.
    #[must_use]
    pub fn overlaps_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        let closest_x = x.clamp(self.x0, self.x1);
        let closest_z = z.clamp(self.z0, self.z1);
        let dx = x - closest_x;
        let dz = z - closest_z;
        dx.mul_add(dx, dz * dz) < radius * radius
    }

    /// True when the player is on the approach side of the ladder: the side the
    /// climber comes from, opposite the climb direction.
    ///
    /// Incidental contact from the exit side never attaches: a player standing
    /// on the deck beyond the ladder is past its centre along `facing`.
    #[must_use]
    pub fn approach_side(&self, x: f32, z: f32) -> bool {
        let (cx, cz) = self.center();
        (x - cx).mul_add(self.facing_x, (z - cz) * self.facing_z) <= 0.0
    }

    /// True when the body spanning `[eye - body_height, eye]` overlaps the
    /// ladder's authored vertical reach.
    #[must_use]
    pub fn overlaps_body_y(&self, eye: f32, body_height: f32) -> bool {
        eye > self.bottom_y && eye - body_height < self.top_y
    }

    /// The component of a movement direction along the climb direction.
    ///
    /// The direction need not be normalised; the caller normalises once. A
    /// positive value means "climb up", negative "climb down".
    #[must_use]
    pub fn climb_intent(&self, dir_x: f32, dir_z: f32) -> f32 {
        dir_x.mul_add(self.facing_x, dir_z * self.facing_z)
    }
}

/// The level's ladders, resolved once at load time.
///
/// Lookups are linear over the authored list, like the water volumes; a level
/// with no `ladders` array is empty and every query misses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ladders {
    ladders: Vec<Ladder>,
}

impl Ladders {
    /// Retained ladder storage for the prepared-world cache.
    #[must_use]
    pub(crate) const fn retained_heap_bytes(&self) -> usize {
        self.ladders
            .capacity()
            .saturating_mul(std::mem::size_of::<Ladder>())
    }

    /// An empty set: every query misses.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ladders: Vec::new(),
        }
    }

    /// Resolves every authored ladder.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let mut ladders = Vec::with_capacity(level.ladders.len());
        for def in &level.ladders {
            let (x0, x1, z0, z1) = def.bounds();
            if !x0.is_finite()
                || !x1.is_finite()
                || !z0.is_finite()
                || !z1.is_finite()
                || !def.bottom_y.is_finite()
                || !def.top_y.is_finite()
                || def.top_y <= def.bottom_y
            {
                continue;
            }
            let yaw = if def.facing_degrees.is_finite() {
                def.facing_degrees.to_radians()
            } else {
                0.0
            };
            ladders.push(Ladder {
                x0,
                x1,
                z0,
                z1,
                bottom_y: def.bottom_y,
                top_y: def.top_y,
                facing_x: yaw.sin(),
                facing_z: -yaw.cos(),
            });
        }
        Self { ladders }
    }

    /// True when the level defines no ladders.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ladders.is_empty()
    }

    /// Number of resolved ladders.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.ladders.len()
    }

    /// The ladder at `index`, if it exists.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Ladder> {
        self.ladders.get(index)
    }

    /// The first ladder whose footprint the disc at `(x, z)` touches.
    #[must_use]
    pub fn overlapping(&self, x: f32, z: f32, radius: f32) -> Option<usize> {
        self.ladders
            .iter()
            .position(|ladder| ladder.overlaps_disc(x, z, radius))
    }

    /// Encodes this ladder set into a compiled collision record: the count,
    /// then each ladder's footprint, reach and facing.
    ///
    /// # Errors
    ///
    /// Returns a named error when the set holds more ladders than the format
    /// admits.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.ladders.len(),
            MAX_COLLISION_LADDERS,
            "ladder count",
        )?;
        for ladder in &self.ladders {
            write_ladder(writer, ladder);
        }
        Ok(())
    }

    /// Decodes a ladder set from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a malformed ladder.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let count = reader.count(MAX_COLLISION_LADDERS, "ladder count")?;
        let mut ladders = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            ladders.push(read_ladder(reader)?);
        }
        Ok(Self { ladders })
    }
}

/// Encodes one ladder volume.
fn write_ladder(writer: &mut Writer, ladder: &Ladder) {
    writer.f32(ladder.x0);
    writer.f32(ladder.x1);
    writer.f32(ladder.z0);
    writer.f32(ladder.z1);
    writer.f32(ladder.bottom_y);
    writer.f32(ladder.top_y);
    writer.f32(ladder.facing_x);
    writer.f32(ladder.facing_z);
}

/// Decodes one ladder volume.
fn read_ladder(reader: &mut Reader<'_>) -> Result<Ladder, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let bottom_y = reader.f32()?;
    let top_y = reader.f32()?;
    let facing_x = reader.f32()?;
    let facing_z = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, bottom_y, top_y, facing_x, facing_z]) {
        return Err("ladder has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("ladder bounds are inverted".to_string());
    }
    Ok(Ladder {
        x0,
        x1,
        z0,
        z1,
        bottom_y,
        top_y,
        facing_x,
        facing_z,
    })
}
