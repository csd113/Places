//! Local source checks for stacked structural clearance. Called on demand by
//! authoring/export, never an all-pairs scan in the simulation frame loop.
use super::level::{LevelDef, MIN_STRUCTURE_M};

#[derive(Debug)]
pub struct Clearance {
    pub lower: usize,
    pub upper: usize,
    pub gap: f32,
    pub position: [f32; 3],
}

pub fn clearances(level: &LevelDef) -> Vec<Clearance> {
    let mut result = Vec::new();
    for (lower, room) in level
        .rooms
        .iter()
        .enumerate()
        .filter(|(_, r)| r.ceiling.is_flat())
    {
        let (x0, x1, z0, z1) = room.bounds();
        for (upper, next) in level.rooms.iter().enumerate() {
            if next.floor_y <= room.floor_y + 0.001 {
                continue;
            }
            let (a0, a1, b0, b1) = next.bounds();
            let (left, right, near, far) = (x0.max(a0), x1.min(a1), z0.max(b0), z1.min(b1));
            if right - left <= 0.001 || far - near <= 0.001 {
                continue;
            }
            let gap = next.floor_y - room.eave_y();
            // A taller enclosing hall may intentionally own the roof above
            // several storeys. A ceiling inside the upper room's occupied
            // height is a clearance problem even when the gap is negative.
            if gap < MIN_STRUCTURE_M - 0.0001
                && (gap >= -0.001 || room.eave_y() < next.eave_y() - 0.001)
            {
                result.push(Clearance {
                    lower,
                    upper,
                    gap,
                    position: [
                        f32::midpoint(left, right),
                        room.eave_y(),
                        f32::midpoint(near, far),
                    ],
                });
            }
        }
    }
    result
}
