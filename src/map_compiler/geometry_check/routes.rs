//! Bounded construction-route samples use the engine's own floor, collision,
//! opening and authored door pose. These are local evidence, never a nav proof.
use super::{Checker, Severity};
use crate::engine::collision::{
    CROUCH_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, PLAYER_STEP_HEIGHT, WallAabb,
};
use crate::engine::level::{LevelSurfaces, WalkableFloor, WallAxis};

pub(super) fn check(checker: &mut Checker<'_>) {
    let level = checker.level;
    let surfaces = LevelSurfaces::new(level);
    let boxes = level.collision_aabbs();
    openings(checker, &boxes);
    for (index, ramp) in level.ramps.iter().enumerate() {
        let Some((_, room)) = surfaces.ramp_room(ramp) else {
            continue;
        };
        let route = Route {
            label: format!("ramp {index}"),
            bounds: ramp.bounds(),
            axis: ramp.axis(),
            base: room.floor_y,
            offset: ramp.offset_y,
            rise: ramp.rise,
            steps: false,
        };
        route.check(checker, &surfaces, &boxes, |x, z| {
            ramp.surface().offset_at(x, z)
        });
    }
    for (index, stair) in level.stairs.iter().enumerate() {
        let (x0, x1, z0, z1) = stair.bounds();
        let base = surfaces
            .room_floor_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1))
            .unwrap_or(0.0);
        let route = Route {
            label: format!("stairs {index}"),
            bounds: stair.bounds(),
            axis: stair.axis(),
            base,
            offset: stair.offset_y,
            rise: stair.rise,
            steps: true,
        };
        route.check(checker, &surfaces, &boxes, |x, z| {
            stair.surface().offset_at(x, z)
        });
    }
    let x = level.spawn.x;
    let z = level.spawn.z;
    if let Some(foot) = surfaces.floor_y_at(x, z) {
        let _blocked = clearance(checker, "spawn", [x, foot, z], &boxes);
    }
}
fn openings(checker: &mut Checker<'_>, boxes: &[WallAabb]) {
    let level = checker.level;
    let floor = WalkableFloor::from_level(level);
    for (index, wall) in level.walls.iter().enumerate() {
        let (x, z) = wall.length_origin();
        for (opening_index, opening) in wall
            .openings
            .iter()
            .enumerate()
            .filter(|(_, o)| o.reaches_floor() && !(o.glass.is_some() && o.solid))
        {
            let along = opening.width.mul_add(0.5, opening.offset);
            let (px, pz, dx, dz) = match wall.axis() {
                WallAxis::X => (
                    x + along,
                    wall.depth.abs().mul_add(0.5, z),
                    0.0,
                    wall.depth.abs().mul_add(0.5, PLAYER_RADIUS),
                ),
                WallAxis::Z => (
                    wall.width.abs().mul_add(0.5, x),
                    z + along,
                    wall.width.abs().mul_add(0.5, PLAYER_RADIUS),
                    0.0,
                ),
            };
            let id = format!("wall {index} / opening {opening_index}");
            if opening.width < PLAYER_RADIUS.mul_add(2.0, -0.001) {
                checker.push("opening-player-width",Severity::Warning,&id,format!("Opening is {:.3} m wide; the player needs {:.3} m in both standing and crouched stances. Decorative vents and windows can be intentional. No aperture is widened automatically.",opening.width,PLAYER_RADIUS*2.0),[px,wall.y,pz]);
            }
            if opening.height < PLAYER_HEIGHT - 0.001 {
                checker.push("opening-headroom",Severity::Warning,&id,format!("Opening height {:.3} m: {} Standing height is {PLAYER_HEIGHT:.2} m and crouched height is {CROUCH_HEIGHT:.2} m. Review the route; no ceiling or doorway is enlarged automatically.",opening.height,if opening.height<CROUCH_HEIGHT{"Even a crouched player cannot fit."}else{"Crouching is required; this may be intentional."}),[px,wall.y+opening.height,pz]);
            }
            let first = floor.height_below(px - dx, pz - dz, wall.y + PLAYER_STEP_HEIGHT);
            let second = floor.height_below(px + dx, pz + dz, wall.y + PLAYER_STEP_HEIGHT);
            if first
                .zip(second)
                .is_some_and(|(a, b)| (a - b).abs() > PLAYER_STEP_HEIGHT + 0.001)
            {
                checker.push("opening-connection-step",Severity::Warning,&id,"The sampled floors across this aperture differ by more than a player step. Review the landing or ramp; decorative/open-drop openings are valid and are never filled automatically.",[px,wall.y,pz]);
            }
        }
    }
    // Sample inside the leaf, excluding the hinge/latch boundary where a frame
    // legitimately touches. Thin oriented leaf checks avoid broad AABB guesses.
    for (index, door) in level.doors.iter().enumerate() {
        let leaf = crate::engine::door::DoorRuntime::new(door, level).collider();
        let intrudes = [0.15_f32, 0.5, 0.85].into_iter().any(|fraction| {
            let x = leaf.dir_x.mul_add(leaf.width * fraction, leaf.hinge_x);
            let z = leaf.dir_z.mul_add(leaf.width * fraction, leaf.hinge_z);
            boxes.iter().any(|b| {
                b.overlaps_disc(x, z, 0.005)
                    && b.min_y < leaf.hinge_y + leaf.height - 0.02
                    && b.max_y > leaf.hinge_y + 0.02
            })
        });
        if intrudes {
            checker.push("door-solid-intrusion",Severity::Warning,format!("door {index}"),"Interior samples of this leaf's authored starting pose enter static collision. Review its fitted opening, hinge and opening angle. Door frames and endpoint contacts are excluded; no door or wall is moved automatically.",[(leaf.dir_x*leaf.width).mul_add(0.5,leaf.hinge_x),leaf.height.mul_add(0.5,leaf.hinge_y),(leaf.dir_z*leaf.width).mul_add(0.5,leaf.hinge_z)]);
        }
    }
}
struct Route {
    label: String,
    bounds: (f32, f32, f32, f32),
    axis: WallAxis,
    base: f32,
    offset: f32,
    rise: f32,
    steps: bool,
}
impl Route {
    fn point(&self, t: f32) -> (f32, f32) {
        let (x0, x1, z0, z1) = self.bounds;
        match self.axis {
            WallAxis::X => (t.mul_add(x1 - x0, x0), f32::midpoint(z0, z1)),
            WallAxis::Z => (f32::midpoint(x0, x1), t.mul_add(z1 - z0, z0)),
        }
    }
    fn check(
        &self,
        checker: &mut Checker<'_>,
        _surfaces: &LevelSurfaces<'_>,
        boxes: &[WallAabb],
        height: impl Fn(f32, f32) -> f32,
    ) {
        let (x0, x1, z0, z1) = self.bounds;
        let (length, width) = match self.axis {
            WallAxis::X => (x1 - x0, z1 - z0),
            WallAxis::Z => (z1 - z0, x1 - x0),
        };
        let (cx, cz) = self.point(0.5);
        if width < PLAYER_RADIUS.mul_add(2.0, -0.001) {
            checker.push("route-player-width",Severity::Warning,&self.label,format!("This route is {width:.3} m wide; player diameter is {:.3} m. Crouching changes height, not width. Review side clearance in Play.",PLAYER_RADIUS*2.0),[cx,self.base+self.offset,cz]);
        }
        if !self.steps && self.rise.abs() / length > 1.0 {
            checker.push("ramp-steep-review",Severity::Warning,&self.label,format!("Ramp rise/run is {:.3}. The engine supports up to {:.1}; this steep authored slope merits a Play check, not automatic flattening.",self.rise.abs()/length,crate::engine::level::MAX_RAMP_SLOPE),[cx,self.base+self.offset,cz]);
        }
        for sample in 0_u16..=12 {
            let (x, z) = self.point(f32::from(sample) / 12.0);
            if clearance(
                checker,
                &self.label,
                [x, self.base + height(x, z), z],
                boxes,
            ) {
                break;
            }
        }
        let floor = WalkableFloor::from_level(checker.level);
        for (fraction, expected) in [
            (-PLAYER_RADIUS / length, self.base + self.offset),
            (
                1.0 + PLAYER_RADIUS / length,
                self.base + self.offset + self.rise,
            ),
        ] {
            let (x, z) = self.point(fraction);
            let landing = floor.height_below(x, z, expected + PLAYER_STEP_HEIGHT);
            if landing.is_none_or(|y| (y - expected).abs() > PLAYER_STEP_HEIGHT + 0.001) {
                checker.push("route-landing",Severity::Warning,&self.label,"A player-radius sample beyond this route endpoint has no floor within one step of its end height. Review the landing, gap or intended drop. No floor is added and no opening is sealed automatically.",[x,expected,z]);
            }
        }
    }
}
fn clearance(
    checker: &mut Checker<'_>,
    id: &str,
    [x, foot, z]: [f32; 3],
    boxes: &[WallAabb],
) -> bool {
    let level = checker.level;
    let mut headroom = f32::INFINITY;
    for room in level.rooms.iter().filter(|r| r.contains(x, z)) {
        if !room.ceiling.is_open() {
            let ceiling = room.ceiling_y_at(x, z);
            if ceiling >= foot - 0.001 {
                headroom = headroom.min(ceiling - foot);
            }
        }
        if room.floor_y > foot + 0.001 {
            headroom = headroom.min(room.floor_y - foot);
        }
    }
    for b in boxes.iter().filter(|b| {
        b.overlaps_disc(x, z, PLAYER_RADIUS - 0.01) && b.max_y > foot + PLAYER_STEP_HEIGHT + 0.001
    }) {
        if b.min_y > foot + 0.001 {
            headroom = headroom.min(b.min_y - foot);
        } else {
            headroom = 0.0;
        }
    }
    if headroom >= PLAYER_HEIGHT - 0.001 {
        return false;
    }
    checker.push("route-headroom",Severity::Warning,id,format!("At this player-sized sample clearance is {headroom:.3} m; standing needs {PLAYER_HEIGHT:.2} m and crouching needs {CROUCH_HEIGHT:.2} m. {} Samples do not prove complete traversability; use Play for the full route.",if headroom<CROUCH_HEIGHT{"Crouching is also blocked."}else{"Crouching can fit vertically; it may be intended."}),[x,foot,z]);
    true
}
