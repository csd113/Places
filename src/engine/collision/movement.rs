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
/// Squared displacement below which there is no meaningful sweep, in m².
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

/// Bounded horizontal recovery is distinct from incoming sweep contacts.
/// A residual multi-solid overlap is refused instead of escaping across a wall.
fn recover_start(world: &HorizontalWorld<'_>, from: Vec2, body: (f32, f32, f32)) -> Option<Vec2> {
    let (feet, height, radius) = body;
    let recovered = super::resolve_airborne_player_collision_with_doors(
        world.index,
        (from, from),
        radius,
        feet,
        height,
        world.walls,
        world.doors,
    );
    let mut clear = true;
    world.index.for_each_disc(
        recovered.x,
        recovered.y,
        radius - CONTACT_EPS,
        world.walls,
        |wall| {
            if wall.max_y > feet + CONTACT_EPS
                && wall.min_y < feet + height - CONTACT_EPS
                && wall.overlaps_disc(recovered.x, recovered.y, radius - CONTACT_EPS)
            {
                clear = false;
            }
        },
    );
    clear &= !world.doors.iter().any(|door| {
        door.overlaps_body_y(feet, height)
            && door.overlaps_disc(recovered.x, recovered.y, radius - CONTACT_EPS)
    });
    if recovered.distance(from) > radius + CONTACT_EPS || !clear {
        if world.debug {
            crate::engine::logging::warn(
                "[movement] recovery_rejected reason=bound_or_residual_overlap",
            );
        }
        return None;
    }
    if world.debug && recovered.distance_squared(from) > MOTION_EPS_SQUARED {
        crate::engine::logging::warn(format_args!(
            "[movement] small_depenetration initial={from:?} final={recovered:?} vector={:?}",
            Vec2::new(recovered.x - from.x, recovered.y - from.y)
        ));
    }
    Some(recovered)
}

/// Sweep and slide through boxes and oriented doors. Each new segment is
/// swept again, so a corner cannot resolve one wall by entering another.
pub fn sweep_horizontal(
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
    let Some(mut position) = recover_start(world, from, body) else {
        return from;
    };
    for _ in 0..CONTACT_ITERATIONS {
        if remaining.length_squared() <= MOTION_EPS_SQUARED {
            break;
        }
        let mut best = None;
        index.for_each_swept_disc(position, point_on_path(position, remaining, 1.0), radius, walls, |wall| {
            if wall.max_y <= feet + CONTACT_EPS
                || wall.min_y >= feet + height - CONTACT_EPS { return; }
            if let Some(hit) = cast_disc(position, remaining, radius, wall) {
                if *debug { crate::engine::logging::warn(format_args!("[movement] candidate collider=box identity={:?} bounds={wall:?} hit_time={} normal={:?}", walls.iter().position(|candidate| std::ptr::eq(candidate,wall)), hit.fraction,hit.normal)); }
                nearer(&mut best, hit, remaining);
            }
        });
        for door in *doors {
            if !door.overlaps_body_y(feet, height) {
                continue;
            }
            if let Some(hit) = cast_door(position, remaining, radius, door) {
                nearer(&mut best, hit, remaining);
            }
        }
        ceiling.for_each_body_barrier(feet, height, |wall| {
            if let Some(hit) = cast_disc(position, remaining, radius, wall) {
                nearer(&mut best, hit, remaining);
            }
        });
        floor.for_each_boundary(|wall, surface| {
            let Some(hit) = cast_disc(position, remaining, radius, wall) else {
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
                if let Some(hit) = cast_disc(position, remaining, radius, wall)
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

/// A top contacting any part of the feet disc is a real upward support.
/// Centre-only tests allowed the edge of a falling cylinder through solids.
pub fn body_support_top(
    index: &CollisionIndex,
    point: Vec2,
    radius: f32,
    max_top: f32,
    rim_max_top: f32,
    walls: &[WallAabb],
) -> Option<f32> {
    let mut highest: Option<f32> = None;
    index.for_each_disc(point.x, point.y, radius, walls, |wall| {
        if wall.max_y <= max_top
            && (wall.step_up <= 0.0 || wall.max_y <= rim_max_top)
            && wall.overlaps_disc(point.x, point.y, radius)
        {
            highest = Some(highest.map_or(wall.max_y, |height| height.max(wall.max_y)));
        }
    });
    highest
}
