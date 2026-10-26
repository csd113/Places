//! Continuous circle/rectangle contact for the player's upright cylinder.
//!
//! Ordinary sweeps start clear and never perform an escape teleport. The
//! configuration-space rectangle has straight faces and circular corners;
//! expanding an AABB by the radius alone would invent square invisible walls.

use super::{CONTACT_EPS, DoorCollider, WallAabb};
use crate::engine::collision_index::CollisionIndex;
use glam::Vec2;

/// Distance kept before a hit to absorb f32 roundoff, in metres (20 microns).
const SWEEP_SKIN: f32 = 2e-5;
/// Squared displacement below which there is no meaningful sweep, in mÂ².
const MOTION_EPS_SQUARED: f32 = 1e-12;
/// Dot product tolerance for a grazing direction, in metres per sweep.
const GRAZING_EPS: f32 = 1e-8;
/// A bounded multi-contact solve; unused motion is discarded when exhausted.
const CONTACT_ITERATIONS: usize = 8;

#[derive(Debug, Clone, Copy)]
struct SweepHit {
    pub fraction: f32,
    pub normal: Vec2,
}

/// Borrowed physical geometry; the broad-phase index is shared with gameplay.
pub struct HorizontalWorld<'a> {
    pub index: &'a CollisionIndex,
    pub walls: &'a [WallAabb],
    pub doors: &'a [DoorCollider],
    pub ceiling: &'a crate::engine::level::WalkableCeiling,
    pub floor: &'a crate::engine::level::WalkableFloor,
    pub debug: bool,
}

const fn point_on_path(origin: Vec2, motion: Vec2, fraction: f32) -> Vec2 {
    Vec2::new(
        motion.x.mul_add(fraction, origin.x),
        motion.y.mul_add(fraction, origin.y),
    )
}

fn nearer(best: &mut Option<SweepHit>, candidate: SweepHit, motion: Vec2) {
    if candidate.fraction >= -GRAZING_EPS
        && candidate.fraction <= 1.0
        && motion.dot(candidate.normal) < -GRAZING_EPS
        && best.is_none_or(|hit| candidate.fraction < hit.fraction)
    {
        *best = Some(SweepHit {
            fraction: candidate.fraction.max(0.0),
            ..candidate
        });
    }
}

/// First incoming contact with the exact rounded rectangle.
fn cast_disc(from: Vec2, motion: Vec2, radius: f32, wall: &WallAabb) -> Option<SweepHit> {
    let mut best = None;
    for (origin, direction, plane, other, other_delta, lo, hi, normal) in [
        (
            from.x,
            motion.x,
            wall.min_x - radius,
            from.y,
            motion.y,
            wall.min_z,
            wall.max_z,
            -Vec2::X,
        ),
        (
            from.x,
            motion.x,
            wall.max_x + radius,
            from.y,
            motion.y,
            wall.min_z,
            wall.max_z,
            Vec2::X,
        ),
        (
            from.y,
            motion.y,
            wall.min_z - radius,
            from.x,
            motion.x,
            wall.min_x,
            wall.max_x,
            -Vec2::Y,
        ),
        (
            from.y,
            motion.y,
            wall.max_z + radius,
            from.x,
            motion.x,
            wall.min_x,
            wall.max_x,
            Vec2::Y,
        ),
    ] {
        if direction.abs() <= GRAZING_EPS {
            continue;
        }
        let fraction = (plane - origin) / direction;
        let tangent = other_delta.mul_add(fraction, other);
        if tangent >= lo && tangent <= hi {
            nearer(&mut best, SweepHit { fraction, normal }, motion);
        }
    }
    let a = motion.length_squared();
    if a <= MOTION_EPS_SQUARED {
        return best;
    }
    for (corner, quadrant) in [
        (Vec2::new(wall.min_x, wall.min_z), Vec2::new(-1.0, -1.0)),
        (Vec2::new(wall.min_x, wall.max_z), Vec2::new(-1.0, 1.0)),
        (Vec2::new(wall.max_x, wall.min_z), Vec2::new(1.0, -1.0)),
        (Vec2::new(wall.max_x, wall.max_z), Vec2::new(1.0, 1.0)),
    ] {
        let offset = Vec2::new(from.x - corner.x, from.y - corner.y);
        let b = offset.dot(motion);
        let c = radius.mul_add(-radius, offset.length_squared());
        let discriminant = b.mul_add(b, -(a * c));
        if discriminant < 0.0 {
            continue;
        }
        let fraction = (-b - discriminant.sqrt()) / a;
        let normal = point_on_path(offset, motion, fraction);
        if normal.x * quadrant.x >= -GRAZING_EPS && normal.y * quadrant.y >= -GRAZING_EPS {
            nearer(
                &mut best,
                SweepHit {
                    fraction,
                    normal: normal.normalize_or_zero(),
                },
                motion,
            );
        }
    }
    best
}

/// Exact straight body-disc sweep used by baked links and AI movement.
/// Step rims use the querying body's step height, rather than the player's.
#[expect(
    clippy::too_many_arguments,
    reason = "one physical body and indexed segment probe"
)]
pub fn disc_segment_blocked(
    from: Vec2,
    to: Vec2,
    radius: f32,
    feet: f32,
    height: f32,
    step_height: f32,
    index: &CollisionIndex,
    walls: &[WallAabb],
) -> bool {
    let motion = Vec2::new(to.x - from.x, to.y - from.y);
    let mut blocked = false;
    index.for_each_swept_disc(from, to, radius, walls, |wall| {
        if wall.max_y <= feet + wall.step_up.min(step_height) + super::STEP_EPS
            || wall.min_y + CONTACT_EPS >= feet + height
        {
            return;
        }
        if wall.overlaps_disc(from.x, from.y, radius)
            || wall.overlaps_disc(to.x, to.y, radius)
            || cast_disc(from, motion, radius, wall).is_some()
        {
            blocked = true;
        }
    });
    blocked
}

/// Dynamic bodies can settle into an actor after a drop. Permit a separating
/// sweep from that initial overlap, while retaining exact sweeps for clear starts.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite horizontal contact vectors"
)]
pub fn dynamic_disc_segment_blocked(
    from: Vec2,
    to: Vec2,
    radius: f32,
    feet: f32,
    height: f32,
    solids: &[WallAabb],
) -> bool {
    solids.iter().any(|solid| {
        if !solid.blocks_body(feet, height) {
            return false;
        }
        if solid.overlaps_disc(from.x, from.y, radius) {
            let nearest = from.clamp(
                Vec2::new(solid.min_x, solid.min_z),
                Vec2::new(solid.max_x, solid.max_z),
            );
            let offset = from - nearest;
            let normal = if offset.length_squared() > MOTION_EPS_SQUARED {
                offset
            } else {
                [
                    (from.x - solid.min_x, Vec2::NEG_X),
                    (solid.max_x - from.x, Vec2::X),
                    (from.y - solid.min_z, Vec2::NEG_Y),
                    (solid.max_z - from.y, Vec2::Y),
                ]
                .into_iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map_or(Vec2::ZERO, |(_, normal)| normal)
            };
            return (to - from).dot(normal) < -GRAZING_EPS;
        }
        solid.overlaps_disc(to.x, to.y, radius)
            || cast_disc(from, to - from, radius, solid).is_some()
    })
}

fn cast_door(from: Vec2, motion: Vec2, radius: f32, door: &DoorCollider) -> Option<SweepHit> {
    let (x, z) = door.local(from.x, from.y);
    let local_motion = Vec2::new(
        motion.x.mul_add(door.dir_x, motion.y * door.dir_z),
        motion.x.mul_add(-door.dir_z, motion.y * door.dir_x),
    );
    let local_box = WallAabb::with_y(
        0.0,
        door.hinge_y,
        -door.thickness * 0.5,
        door.width,
        door.height,
        door.thickness,
    );
    let hit = cast_disc(Vec2::new(x, z), local_motion, radius, &local_box)?;
    Some(SweepHit {
        fraction: hit.fraction,
        normal: Vec2::new(
            door.dir_x.mul_add(hit.normal.x, -door.dir_z * hit.normal.y),
            door.dir_z.mul_add(hit.normal.x, door.dir_x * hit.normal.y),
        ),
    })
}

/// Exact straight sweep against the current live door leaf poses.
pub fn door_segment_blocked(
    from: Vec2,
    to: Vec2,
    radius: f32,
    feet: f32,
    height: f32,
    doors: &[DoorCollider],
) -> bool {
    doors.iter().any(|door| {
        door.hinge_y + door.height > feet + super::STEP_EPS
            && door.hinge_y + CONTACT_EPS < feet + height
            && (door.overlaps_disc(from.x, from.y, radius)
                || door.overlaps_disc(to.x, to.y, radius)
                || cast_door(from, Vec2::new(to.x - from.x, to.y - from.y), radius, door).is_some())
    })
}

// Swept separating-axis intervals for an axis-aligned player box against an
// oriented rectangle. The four face axes give the exact Minkowski obstacle;
// rotating the camera never rotates the player's hull.
fn square_cast(
    from: Vec2,
    motion: Vec2,
    radius: f32,
    center: Vec2,
    half: Vec2,
    direction: Vec2,
) -> Option<SweepHit> {
    let cross = Vec2::new(-direction.y, direction.x);
    let offset = Vec2::new(from.x - center.x, from.y - center.y);
    let mut enter = f32::NEG_INFINITY;
    let mut leave = f32::INFINITY;
    let mut normal = Vec2::ZERO;
    let mut contact_depth = f32::INFINITY;
    let mut contact_normal = Vec2::ZERO;
    for axis in [Vec2::X, Vec2::Y, direction, cross] {
        let extent = half.y.mul_add(
            cross.dot(axis).abs(),
            half.x.mul_add(
                direction.dot(axis).abs(),
                radius * (axis.x.abs() + axis.y.abs()),
            ),
        );
        let point = offset.dot(axis);
        // Match the radius tolerance in square_start_clear. A hull accepted
        // inside that contact band must still block incoming motion; treating
        // its negative entry time as a miss can bury it on the next substep.
        let depth = (extent - point.abs()) / (axis.x.abs() + axis.y.abs());
        if depth < contact_depth {
            contact_depth = depth;
            contact_normal = if point >= 0.0 {
                axis
            } else {
                Vec2::new(-axis.x, -axis.y)
            };
        }
        let speed = motion.dot(axis);
        if speed.abs() < GRAZING_EPS {
            if point.abs() >= extent {
                return None;
            }
            continue;
        }
        let first = (-extent - point) / speed;
        let last = (extent - point) / speed;
        let near = first.min(last);
        if near > enter {
            enter = near;
            normal = if speed > 0.0 {
                Vec2::new(-axis.x, -axis.y)
            } else {
                axis
            };
        }
        leave = leave.min(first.max(last));
        if enter > leave {
            return None;
        }
    }
    if (0.0..=CONTACT_EPS).contains(&contact_depth) {
        return (motion.dot(contact_normal) < -GRAZING_EPS).then_some(SweepHit {
            fraction: 0.0,
            normal: contact_normal,
        });
    }
    ((-GRAZING_EPS..=1.0).contains(&enter) && leave >= 0.0).then_some(SweepHit {
        fraction: enter.max(0.0),
        normal,
    })
}

fn square_wall(from: Vec2, motion: Vec2, radius: f32, wall: &WallAabb) -> Option<SweepHit> {
    square_cast(
        from,
        motion,
        radius,
        Vec2::new(
            f32::midpoint(wall.min_x, wall.max_x),
            f32::midpoint(wall.min_z, wall.max_z),
        ),
        Vec2::new(
            (wall.max_x - wall.min_x) * 0.5,
            (wall.max_z - wall.min_z) * 0.5,
        ),
        Vec2::X,
    )
}

fn square_door(from: Vec2, motion: Vec2, radius: f32, door: &DoorCollider) -> Option<SweepHit> {
    square_cast(
        from,
        motion,
        radius,
        Vec2::new(
            door.dir_x.mul_add(door.width * 0.5, door.hinge_x),
            door.dir_z.mul_add(door.width * 0.5, door.hinge_z),
        ),
        Vec2::new(door.width * 0.5, door.thickness * 0.5),
        Vec2::new(door.dir_x, door.dir_z),
    )
}

pub fn square_overlaps_wall(point: Vec2, radius: f32, wall: &WallAabb) -> bool {
    point.x + radius > wall.min_x
        && point.x - radius < wall.max_x
        && point.y + radius > wall.min_z
        && point.y - radius < wall.max_z
}

pub fn square_overlaps_door(point: Vec2, radius: f32, door: &DoorCollider) -> bool {
    let direction = Vec2::new(door.dir_x, door.dir_z);
    let cross = Vec2::new(-direction.y, direction.x);
    let center = Vec2::new(
        door.dir_x.mul_add(door.width * 0.5, door.hinge_x),
        door.dir_z.mul_add(door.width * 0.5, door.hinge_z),
    );
    let offset = Vec2::new(point.x - center.x, point.y - center.y);
    [Vec2::X, Vec2::Y, direction, cross]
        .into_iter()
        .all(|axis| {
            offset.dot(axis).abs()
                < (door.thickness * 0.5).mul_add(
                    cross.dot(axis).abs(),
                    (door.width * 0.5).mul_add(
                        direction.dot(axis).abs(),
                        radius * (axis.x.abs() + axis.y.abs()),
                    ),
                )
        })
}

fn square_start_clear(world: &HorizontalWorld<'_>, point: Vec2, body: (f32, f32, f32)) -> bool {
    let (feet, height, radius) = body;
    let mut clear = true;
    world.index.for_each_disc(
        point.x,
        point.y,
        radius * std::f32::consts::SQRT_2,
        world.walls,
        |wall| {
            if wall.max_y > feet + CONTACT_EPS
                && wall.min_y < feet + height - CONTACT_EPS
                && square_overlaps_wall(point, radius - CONTACT_EPS, wall)
            {
                clear = false;
            }
        },
    );
    clear
        && !world.doors.iter().any(|door| {
            door.overlaps_body_y(feet, height)
                && square_overlaps_door(point, radius - CONTACT_EPS, door)
        })
}

/// Exact fixed-orientation player hull; the disc API remains available to AI.
pub fn sweep_player_hull(
    world: &HorizontalWorld<'_>,
    path: (Vec2, Vec2),
    body: (f32, f32, f32),
    allow_steps: bool,
) -> Vec2 {
    sweep_shape(world, path, body, allow_steps)
}

fn sweep_shape(
    world: &HorizontalWorld<'_>,
    path: (Vec2, Vec2),
    body: (f32, f32, f32),
    allow_steps: bool,
) -> Vec2 {
    let HorizontalWorld {
        index,
        walls,
        doors,
        ceiling,
        floor,
        debug,
    } = world;
    let (from, to) = path;
    let (feet, height, radius) = body;
    let mut remaining = Vec2::new(to.x - from.x, to.y - from.y);
    let cast_wall = square_wall;
    let cast_leaf = square_door;
    if !square_start_clear(world, from, body) {
        return from;
    }
    let mut position = from;
    for _ in 0..CONTACT_ITERATIONS {
        if remaining.length_squared() <= MOTION_EPS_SQUARED {
            break;
        }
        let mut best = None;
        index.for_each_swept_disc(position, point_on_path(position, remaining, 1.0), radius * std::f32::consts::SQRT_2, walls, |wall| {
            if wall.max_y <= feet + CONTACT_EPS
                || wall.min_y >= feet + height - CONTACT_EPS { return; }
            if let Some(hit) = cast_wall(position, remaining, radius, wall) {
                if *debug { crate::engine::logging::warn(format_args!("[movement] candidate collider=box identity={:?} bounds={wall:?} hit_time={} normal={:?}", walls.iter().position(|candidate| std::ptr::eq(candidate,wall)), hit.fraction,hit.normal)); }
                nearer(&mut best, hit, remaining);
            }
        });
        for door in *doors {
            if !door.overlaps_body_y(feet, height) {
                continue;
            }
            if let Some(hit) = cast_leaf(position, remaining, radius, door) {
                nearer(&mut best, hit, remaining);
            }
        }
        ceiling.for_each_body_barrier(feet, height, |wall| {
            if let Some(hit) = cast_wall(position, remaining, radius, wall) {
                nearer(&mut best, hit, remaining);
            }
        });
        floor.for_each_boundary(|wall, surface| {
            let Some(hit) = cast_wall(position, remaining, radius, wall) else {
                return;
            };
            let contact = point_on_path(position, remaining, hit.fraction);
            let top = surface(
                contact.x.clamp(wall.min_x, wall.max_x),
                contact.y.clamp(wall.min_z, wall.max_z),
            );
            let step = if allow_steps {
                super::PLAYER_STEP_HEIGHT
            } else {
                0.0
            };
            if top > feet + step + CONTACT_EPS && top < feet + height - CONTACT_EPS {
                nearer(&mut best, hit, remaining);
            }
        });
        floor.for_each_terrain_barrier(
            feet,
            height,
            if allow_steps {
                super::PLAYER_STEP_HEIGHT
            } else {
                0.0
            },
            |wall, outward| {
                if let Some(hit) = cast_wall(position, remaining, radius, wall)
                    && hit.normal.dot(outward) > GRAZING_EPS
                {
                    // Terrain faces block entry from outside. A body following
                    // the ramp/stair walking surface can leave from inside,
                    // including when its leading disc reaches a higher end.
                    nearer(&mut best, hit, remaining);
                }
            },
        );
        let Some(hit) = best else {
            position = point_on_path(position, remaining, 1.0);
            break;
        };
        let distance = remaining.length();
        // Keep at least one representable coordinate increment before contact.
        // A fixed micron margin disappears at large world coordinates.
        let skin = SWEEP_SKIN.max(position.abs().max_element().max(1.0) * f32::EPSILON);
        let advance = (hit.fraction - skin / distance).max(0.0);
        if *debug {
            crate::engine::logging::warn(format_args!(
                "[movement] sweep from={position:?} requested={remaining:?} time={} contact={:?} normal={:?} classification=wall",
                hit.fraction,
                point_on_path(position, remaining, hit.fraction),
                hit.normal
            ));
        }
        position = point_on_path(position, remaining, advance);
        remaining = Vec2::new(remaining.x * (1.0 - advance), remaining.y * (1.0 - advance));
        let incoming = remaining.dot(hit.normal).min(0.0);
        remaining = point_on_path(remaining, hit.normal, -incoming);
    }
    position
}
