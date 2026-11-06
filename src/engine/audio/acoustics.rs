//! Game-thread acoustic scene. Queries never run on the audio device thread.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::door::DoorRuntime;
use crate::engine::level::{LevelDef, LevelSurfaces, WallAxis};

/// Material absorption in low/mid/high bands and sound transmitted by a solid.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AcousticMaterial {
    pub absorption: [f32; 3],
    pub transmission: f32,
}
impl Default for AcousticMaterial {
    fn default() -> Self {
        Self {
            absorption: [0.08, 0.16, 0.3],
            transmission: 0.16,
        }
    }
}
impl AcousticMaterial {
    /// Backward-compatible surface defaults; explicit material IDs override these.
    #[must_use]
    pub fn for_id(id: &str) -> Self {
        let name = id.to_ascii_lowercase();
        if name.contains("carpet") || name.contains("acoustic") || name.contains("ceiling_tile") {
            Self {
                absorption: [0.2, 0.65, 0.9],
                transmission: 0.1,
            }
        } else if name.contains("tile")
            || name.contains("concrete")
            || name.contains("glass")
            || name.contains("marble")
        {
            Self {
                absorption: [0.02, 0.04, 0.08],
                transmission: 0.2,
            }
        } else if name.contains("wood") {
            Self {
                absorption: [0.15, 0.12, 0.2],
                transmission: 0.24,
            }
        } else {
            Self::default()
        }
    }
    /// Reject invalid authored coefficients rather than hiding a malformed room.
    /// # Errors
    /// Returns a coefficient outside the finite unit interval.
    pub fn validate(self) -> Result<(), String> {
        if self
            .absorption
            .into_iter()
            .chain([self.transmission])
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            Ok(())
        } else {
            Err("acoustic material coefficients must be finite in 0..1".into())
        }
    }
}

/// Optional level material overrides, keyed by the existing material identity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AcousticMaterialDef {
    pub material: String,
    pub absorption: [f32; 3],
    pub transmission: f32,
}
impl AcousticMaterialDef {
    #[must_use]
    pub const fn properties(&self) -> AcousticMaterial {
        AcousticMaterial {
            absorption: self.absorption,
            transmission: self.transmission,
        }
    }
}
pub type AcousticMaterials = Vec<AcousticMaterialDef>;

/// Room authoring overrides; natural size/material response remains the default.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RoomAcoustics {
    pub wet: f32,
    pub decay_scale: f32,
}
impl Default for RoomAcoustics {
    fn default() -> Self {
        Self {
            wet: 0.18,
            decay_scale: 1.0,
        }
    }
}
impl RoomAcoustics {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    /// # Errors
    /// Returns invalid reverb level or decay scaling.
    pub fn validate(self) -> Result<(), String> {
        if self.wet.is_finite()
            && (0.0..=0.5).contains(&self.wet)
            && self.decay_scale.is_finite()
            && (0.25..=2.0).contains(&self.decay_scale)
        {
            Ok(())
        } else {
            Err("room acoustics require wet in 0..0.5 and decay_scale in 0.25..2".into())
        }
    }
}

/// Listener-independent path result, suitable for later perception consumers.
/// This contains no mixer/device handle and does not emit or implement AI.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct AcousticPath {
    pub transmission: f32,
    pub cutoff_hz: f32,
    pub budget_exhausted: bool,
}
impl Default for AcousticPath {
    fn default() -> Self {
        Self {
            transmission: 1.0,
            cutoff_hz: 20_000.0,
            budget_exhausted: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq)]
pub struct RoomResponse {
    pub wet: f32,
    pub decay_seconds: f32,
    pub damping: f32,
    pub size: [f32; 3],
}
struct Surface {
    bounds: WallAabb,
    material: AcousticMaterial,
    // Exact sloped prism face, y = x*slope.x + z*slope.z + slope.y.
    // The bool selects solid above (ceiling) or below (floor), inside bounds.
    plane: Option<(Vec3, bool)>,
}
struct Node {
    bounds: WallAabb,
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}
struct Room {
    bounds: WallAabb,
    response: RoomResponse,
    volume: f32,
    absorption_area: f32,
    scale: f32,
}

/// Immutable BVH plus bounded dynamic door poses, rebuilt only on map install.
pub struct AcousticScene {
    surfaces: Vec<Surface>,
    nodes: Vec<Node>,
    doors: Vec<DoorCollider>,
    elevators: Vec<DoorCollider>,
    rooms: Vec<Room>,
    door_closed: Vec<f32>,
    pub revision: u64,
}
impl Default for AcousticScene {
    fn default() -> Self {
        Self {
            surfaces: Vec::new(),
            nodes: Vec::new(),
            doors: Vec::new(),
            elevators: Vec::new(),
            rooms: Vec::new(),
            door_closed: Vec::new(),
            revision: 1,
        }
    }
}
impl AcousticScene {
    /// Build once from the same sliced wall solids used by collision.
    #[must_use]
    pub fn build(level: &LevelDef) -> Self {
        let mut scene = Self::default();
        let surfaces = LevelSurfaces::new(level);
        let material = |id: &str| {
            level
                .acoustic_materials
                .iter()
                .find(|entry| entry.material == id)
                .map_or_else(
                    || AcousticMaterial::for_id(id),
                    AcousticMaterialDef::properties,
                )
        };
        for wall in &level.walls {
            let (origin_x, origin_z) = wall.length_origin();
            let mat = wall_material(wall, &level.defaults.wall, &material);
            for slice in surfaces.wall_solid_slices(wall) {
                let bounds = match wall.axis() {
                    WallAxis::X => WallAabb::with_y(
                        origin_x + slice.start,
                        slice.bottom,
                        wall.z.min(wall.z + wall.depth),
                        slice.end - slice.start,
                        slice.top - slice.bottom,
                        wall.depth.abs(),
                    ),
                    WallAxis::Z => WallAabb::with_y(
                        wall.x.min(wall.x + wall.width),
                        slice.bottom,
                        origin_z + slice.start,
                        wall.width.abs(),
                        slice.top - slice.bottom,
                        slice.end - slice.start,
                    ),
                };
                scene.surfaces.push(Surface {
                    bounds,
                    material: mat,
                    plane: None,
                });
            }
            for opening in &wall.openings {
                if let Some(glass) = opening.glass_material()
                    && let Some(bounds) = crate::engine::level::opening_glass_bounds(wall, opening)
                {
                    scene.surfaces.push(Surface {
                        bounds,
                        material: material(glass),
                        plane: None,
                    });
                }
            }
        }
        // Reuse canonical cutout-aware architecture and authored solid-prop
        // proxies, without duplicating the material-bearing wall slices above.
        let mut known: std::collections::BTreeSet<_> = scene
            .surfaces
            .iter()
            .map(|surface| box_key(surface.bounds))
            .collect();
        for bounds in level.item_collision_aabbs() {
            if known.insert(box_key(bounds)) {
                scene.surfaces.push(Surface {
                    bounds,
                    material: AcousticMaterial::default(),
                    plane: None,
                });
            }
        }
        append_horizontals(&mut scene.surfaces, level, &surfaces, &material);
        if !scene.surfaces.is_empty() {
            let _root = build_node(&mut scene.nodes, &mut scene.surfaces, 0);
        }
        scene.doors = level
            .doors
            .iter()
            .map(|def| DoorRuntime::new(def, level).collider())
            .collect();
        scene.door_closed = level
            .doors
            .iter()
            .map(|def| {
                1.0 - DoorRuntime::new(def, level)
                    .angle()
                    .to_radians()
                    .sin()
                    .abs()
            })
            .collect();
        let elevators: Vec<_> = level
            .elevators
            .iter()
            .map(crate::engine::elevator::Elevator::new)
            .collect();
        scene.elevators.reserve(192); // Authored maximum: eight landings, 24 shared boxes each.
        scene.update_elevators(&elevators);
        scene.rooms = build_rooms(level, &material);
        scene
    }
    /// Updates exact oriented leaves and invalidates cached paths on a pose change.
    pub fn update_doors(&mut self, doors: &crate::engine::door::Doors) {
        for ((saved, closed), door) in self
            .doors
            .iter_mut()
            .zip(&mut self.door_closed)
            .zip(doors.iter())
        {
            *closed = 1.0 - door.angle().to_radians().sin().abs();
            let pose = door.collider();
            if (saved.dir_x - pose.dir_x).abs() + (saved.dir_z - pose.dir_z).abs() > 0.002 {
                *saved = pose;
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }
    /// Reuses fixed-capacity storage and the collision visitor, with no per-voice geometry.
    pub fn update_elevators(&mut self, elevators: &[crate::engine::elevator::Elevator]) {
        let mut index = 0_usize;
        let mut changed = false;
        for elevator in elevators {
            elevator.visit_colliders(|pose| {
                if let Some(saved) = self.elevators.get_mut(index) {
                    changed |= *saved != pose;
                    *saved = pose;
                } else {
                    self.elevators.push(pose);
                    changed = true;
                }
                index = index.saturating_add(1);
            });
        }
        changed |= self.elevators.len() != index;
        self.elevators.truncate(index);
        if changed {
            self.revision = self.revision.wrapping_add(1).max(1);
        }
    }
    #[must_use]
    pub const fn counts(&self) -> [usize; 3] {
        [self.surfaces.len(), self.nodes.len(), self.doors.len()]
    }
    /// Strict static geometry visibility for gameplay evidence. Includes sliced
    /// walls, glazing, fixed props, floors and ceilings. It applies no default
    /// transmission, and budget exhaustion blocks evidence. Live leaves remain
    /// the caller's responsibility.
    #[must_use]
    pub fn static_clear(&self, from: Vec3, to: Vec3) -> bool {
        if !from.is_finite() || !to.is_finite() {
            return false;
        }
        let mut stack = [0_usize; 64];
        let mut len = usize::from(!self.nodes.is_empty());
        let mut work = 0_usize;
        while len > 0 && work < 256 {
            len = len.saturating_sub(1);
            work = work.saturating_add(1);
            let Some(node) = stack.get(len).and_then(|index| self.nodes.get(*index)) else {
                return false;
            };
            if !segment_box(from, to, node.bounds) {
                continue;
            }
            if let Some(children) = node.children {
                for child in children {
                    let Some(slot) = stack.get_mut(len) else {
                        return false;
                    };
                    *slot = child;
                    len = len.saturating_add(1);
                }
            } else if self
                .surfaces
                .get(node.range.clone())
                .unwrap_or_default()
                .iter()
                .any(|surface| surface_span(from, to, surface).is_some())
            {
                return false;
            }
        }
        len == 0
    }

    /// At most 256 BVH nodes, 256 hinged leaves and 192 shared elevator boxes.
    /// Fixed hit storage merges touching solids; queries allocate no memory.
    /// Exhaustion returns conservative muffling; it never grows a backlog.
    #[must_use]
    pub fn path(&self, from: Vec3, to: Vec3) -> AcousticPath {
        let mut path = AcousticPath::default();
        let mut hits = Hits::default();
        let mut stack = [0_usize; 64];
        let mut len = usize::from(!self.nodes.is_empty());
        let mut work = 0_usize;
        while len > 0 && work < 256 {
            len = len.saturating_sub(1);
            work = work.saturating_add(1);
            let Some(node) = stack.get(len).and_then(|index| self.nodes.get(*index)) else {
                break;
            };
            if !segment_box(from, to, node.bounds) {
                continue;
            }
            if let Some(children) = node.children {
                for child in children {
                    if let Some(slot) = stack.get_mut(len) {
                        *slot = child;
                        len = len.saturating_add(1);
                    }
                }
            } else {
                for surface in self.surfaces.get(node.range.clone()).unwrap_or_default() {
                    if let Some(span) = surface_span(from, to, surface) {
                        hits.insert(span, surface.material);
                    }
                }
            }
        }
        if len > 0 && work >= 256 {
            path.budget_exhausted = true;
            block(&mut path, AcousticMaterial::default());
        }
        for (door, material) in self
            .doors
            .iter()
            .take(256)
            .map(|door| {
                (
                    door,
                    AcousticMaterial {
                        absorption: [0.15, 0.3, 0.55],
                        transmission: 0.23,
                    },
                )
            })
            .chain(
                self.elevators
                    .iter()
                    .take(192)
                    .map(|door| (door, AcousticMaterial::default())),
            )
        {
            if let Some(span) = door_span(from, to, door) {
                hits.insert(span, material);
            }
        }
        for hit in hits.entries.into_iter().flatten() {
            block(&mut path, hit.material);
        }
        if hits.overflow {
            path.budget_exhausted = true;
            path.transmission = 0.025;
            path.cutoff_hz = path.cutoff_hz.min(450.0);
        }
        path
    }
    /// Size/shape/material Sabine approximation with aperture leakage.
    #[must_use]
    pub fn room_at(&self, position: Vec3) -> RoomResponse {
        let Some(room) = self
            .rooms
            .iter()
            .rev()
            .find(|room| contains(room.bounds, position))
        else {
            return RoomResponse::default();
        };
        let mut area = room.absorption_area;
        for (door, closed) in self.doors.iter().zip(&self.door_closed) {
            if contains(
                room.bounds,
                Vec3::new(door.hinge_x, door.hinge_y + 0.1, door.hinge_z),
            ) {
                // Closed door removes the corresponding open aperture energy.
                area = f32::mul_add(door.width * door.height * closed, -0.85, area);
            }
        }
        RoomResponse {
            decay_seconds: (0.161 * room.volume / area.max(1.0) * room.scale).clamp(0.12, 3.0),
            ..room.response
        }
    }
}
fn block(path: &mut AcousticPath, material: AcousticMaterial) {
    path.transmission = (path.transmission * material.transmission).max(0.025);
    path.cutoff_hz = path.cutoff_hz.min(
        (1.0 - material.absorption.get(2).copied().unwrap_or_default()).mul_add(2_500.0, 450.0),
    );
}
fn contains(b: WallAabb, p: Vec3) -> bool {
    p.x >= b.min_x - 0.02
        && p.x <= b.max_x + 0.02
        && p.y >= b.min_y - 0.02
        && p.y <= b.max_y + 0.02
        && p.z >= b.min_z - 0.02
        && p.z <= b.max_z + 0.02
}
fn segment_box(from: Vec3, to: Vec3, b: WallAabb) -> bool {
    segment_span(from, to, b).is_some()
}
fn segment_span(from: Vec3, to: Vec3, b: WallAabb) -> Option<(f32, f32)> {
    let mut enter = 0.001_f32;
    let mut leave = 0.999_f32;
    for (origin, end, low, high) in [
        (from.x, to.x, b.min_x, b.max_x),
        (from.y, to.y, b.min_y, b.max_y),
        (from.z, to.z, b.min_z, b.max_z),
    ] {
        let delta = end - origin;
        if delta.abs() < 1.0e-6 {
            if origin < low || origin > high {
                return None;
            }
        } else {
            let a = (low - origin) / delta;
            let c = (high - origin) / delta;
            enter = enter.max(a.min(c));
            leave = leave.min(a.max(c));
            if enter > leave {
                return None;
            }
        }
    }
    Some((enter, leave))
}
fn build_node(nodes: &mut Vec<Node>, surfaces: &mut [Surface], base: usize) -> usize {
    let bounds = surfaces.iter().fold(
        WallAabb {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            min_z: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
            max_z: f32::NEG_INFINITY,
            step_up: 0.0,
        },
        |mut b, s| {
            b.min_x = b.min_x.min(s.bounds.min_x);
            b.min_y = b.min_y.min(s.bounds.min_y);
            b.min_z = b.min_z.min(s.bounds.min_z);
            b.max_x = b.max_x.max(s.bounds.max_x);
            b.max_y = b.max_y.max(s.bounds.max_y);
            b.max_z = b.max_z.max(s.bounds.max_z);
            b
        },
    );
    let index = nodes.len();
    nodes.push(Node {
        bounds,
        range: base..base.saturating_add(surfaces.len()),
        children: None,
    });
    if surfaces.len() > 4 {
        let extent = Vec3::new(
            bounds.max_x - bounds.min_x,
            bounds.max_y - bounds.min_y,
            bounds.max_z - bounds.min_z,
        );
        let axis: u8 = if extent.y > extent.x.max(extent.z) {
            1
        } else if extent.x >= extent.z {
            0
        } else {
            2
        };
        surfaces.sort_unstable_by(|a, b| {
            let center = |s: &Surface| match axis {
                0 => s.bounds.min_x + s.bounds.max_x,
                1 => s.bounds.min_y + s.bounds.max_y,
                _ => s.bounds.min_z + s.bounds.max_z,
            };
            center(a).total_cmp(&center(b))
        });
        let half = surfaces.len() / 2;
        let (left, right) = surfaces.split_at_mut(half);
        let children = [
            build_node(nodes, left, base),
            build_node(nodes, right, base.saturating_add(half)),
        ];
        if let Some(node) = nodes.get_mut(index) {
            node.children = Some(children);
        }
    }
    index
}

fn build_rooms(level: &LevelDef, material: &impl Fn(&str) -> AcousticMaterial) -> Vec<Room> {
    let mut rooms = Vec::with_capacity(level.rooms.len());
    let surfaces = LevelSurfaces::new(level);
    for room in surfaces.rooms() {
        let (x0, x1, z0, z1) = room.bounds();
        let w = x1 - x0;
        let d = z1 - z0;
        let h = f32::mul_add(room.ceiling.ridge_rise_m(), 0.5, room.height);
        let floor = material(room.material.as_deref().unwrap_or(&level.defaults.floor));
        let ceiling = material(
            room.ceiling_material
                .as_deref()
                .unwrap_or(&level.defaults.ceiling),
        );
        let wall = material(&level.defaults.wall);
        let mut weighted = [0.0_f32; 3];
        let floor_area = w * d;
        let wall_area = 2.0 * (w + d) * h;
        for (i, total) in weighted.iter_mut().enumerate() {
            *total = f32::mul_add(
                wall_area,
                wall.absorption.get(i).copied().unwrap_or_default(),
                floor_area
                    * (floor.absorption.get(i).copied().unwrap_or_default()
                        + ceiling.absorption.get(i).copied().unwrap_or_default()),
            );
        }
        apply_material_overrides(level, room, &surfaces, material, &mut weighted, floor, wall);
        let mut aperture = 0.0;
        for wall_def in &level.walls {
            for opening in &wall_def.openings {
                if opening.glass_material().is_none() {
                    aperture += wall_room_area(
                        room,
                        wall_def,
                        opening.offset,
                        opening.end(),
                        opening.bottom(wall_def.y),
                        opening.top(wall_def.y),
                    );
                }
            }
        }
        if room.ceiling.is_open() {
            aperture += floor_area;
        }
        let absorption_area = weighted
            .iter()
            .zip([0.2_f32, 0.6, 0.2])
            .map(|(area, weight)| area * weight)
            .sum::<f32>()
            + aperture;
        let area = 2.0f32.mul_add(floor_area, wall_area);
        let volume = w * d * h;
        rooms.push(Room {
            bounds: WallAabb::with_y(x0, room.floor_y, z0, w, h, d),
            response: RoomResponse {
                wet: if room.ceiling.is_open() {
                    0.0
                } else {
                    room.acoustics.wet
                        * (1.0 - weighted.get(1).copied().unwrap_or_default() / area.max(1.0))
                            .clamp(0.1, 1.0)
                },
                decay_seconds: 0.0,
                damping: (weighted.get(2).copied().unwrap_or_default() / area.max(1.0))
                    .clamp(0.05, 0.95),
                size: [w, h, d],
            },
            volume,
            absorption_area,
            scale: room.acoustics.decay_scale,
        });
    }
    rooms
}

fn apply_material_overrides(
    level: &LevelDef,
    room: &crate::engine::level::RoomDef,
    surfaces: &LevelSurfaces<'_>,
    material: &impl Fn(&str) -> AcousticMaterial,
    weighted: &mut [f32; 3],
    floor: AcousticMaterial,
    wall: AcousticMaterial,
) {
    let (x0, x1, z0, z1) = room.bounds();
    // Only the solid face area actually inside this room replaces its default.
    for wall_def in &level.walls {
        let surface_area = surfaces
            .wall_solid_slices(wall_def)
            .iter()
            .map(|slice| {
                wall_room_area(
                    room,
                    wall_def,
                    slice.start,
                    slice.end,
                    slice.bottom,
                    slice.top,
                )
            })
            .sum::<f32>();
        let authored = wall_material(wall_def, &level.defaults.wall, material);
        for (i, total) in weighted.iter_mut().enumerate() {
            *total = f32::mul_add(
                surface_area,
                authored.absorption.get(i).copied().unwrap_or_default()
                    - wall.absorption.get(i).copied().unwrap_or_default(),
                *total,
            );
        }
    }
    for patch in &level.floor_patches {
        // Like the floor emitter, ignore malformed non-positive patch extents.
        let area = ((patch.x + patch.width).min(x1) - patch.x.max(x0)).max(0.0)
            * ((patch.z + patch.depth).min(z1) - patch.z.max(z0)).max(0.0);
        let authored = material(&patch.material);
        for (i, total) in weighted.iter_mut().enumerate() {
            *total = f32::mul_add(
                area,
                authored.absorption.get(i).copied().unwrap_or_default()
                    - floor.absorption.get(i).copied().unwrap_or_default(),
                *total,
            );
        }
    }
}

fn box_key(bounds: WallAabb) -> [u32; 6] {
    [
        bounds.min_x,
        bounds.max_x,
        bounds.min_y,
        bounds.max_y,
        bounds.min_z,
        bounds.max_z,
    ]
    .map(f32::to_bits)
}

fn wall_material(
    wall: &crate::engine::level::WallDef,
    default: &str,
    material: &impl Fn(&str) -> AcousticMaterial,
) -> AcousticMaterial {
    let names = match wall.axis() {
        WallAxis::X => ["north", "south"],
        WallAxis::Z => ["west", "east"],
    };
    let [front, back] = names.map(|name| {
        material(
            wall.face_ref(name)
                .map_or(default, |reference| reference.id),
        )
    });
    AcousticMaterial {
        absorption: std::array::from_fn(|i| {
            f32::midpoint(
                front.absorption.get(i).copied().unwrap_or_default(),
                back.absorption.get(i).copied().unwrap_or_default(),
            )
        }),
        transmission: f32::midpoint(front.transmission, back.transmission),
    }
}

/// Use the same convex terrain patches as rendered backing and rigid bodies.
/// Material-only cuts subdivide these prisms without changing their geometry.
fn append_horizontals(
    output: &mut Vec<Surface>,
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    material: &impl Fn(&str) -> AcousticMaterial,
) {
    use crate::engine::level::{WalkableCeiling, WalkableFloor};
    let floors = WalkableFloor::from_level(level).structural_patches();
    let ceilings = WalkableCeiling::from_level(level).physical_patches(&floors);
    let grids: Vec<_> = surfaces
        .rooms()
        .iter()
        .map(|room| surfaces.floor_grid(room))
        .collect();
    for patch in floors.iter().chain(&ceilings) {
        let [origin, along_x, opposite, along_z] = patch.corners;
        let (centre_x, centre_z) = (
            f32::midpoint(origin.x, opposite.x),
            f32::midpoint(origin.z, opposite.z),
        );
        let Some((owner, room)) = surfaces.rooms().iter().enumerate().find(|(_, room)| {
            (room.floor_y - patch.room_floor_y).abs() < 1.0e-4
                && room.contains(centre_x, centre_z)
                && (!patch.ceiling
                    || (room.ceiling_y_at(centre_x, centre_z)
                        - f32::midpoint(origin.y, opposite.y))
                    .abs()
                        < 1.0e-4)
        }) else {
            continue;
        };
        let sx = (along_x.y - origin.y) / (along_x.x - origin.x);
        let sz = (along_z.y - origin.y) / (along_z.z - origin.z);
        let height = |(px, pz)| sx.mul_add(px - origin.x, sz.mul_add(pz - origin.z, origin.y));
        if patch.ceiling {
            push_plane(
                output,
                (origin.x, opposite.x, origin.z, opposite.z),
                height,
                patch.backing,
                true,
                material(
                    room.ceiling_material
                        .as_deref()
                        .unwrap_or(&level.defaults.ceiling),
                ),
            );
            continue;
        }
        let Some(grid) = grids.get(owner) else {
            continue;
        };
        for z_span in grid.zs.windows(2) {
            let &[z0, z1] = z_span else {
                continue;
            };
            for x_span in grid.xs.windows(2) {
                let &[x0, x1] = x_span else {
                    continue;
                };
                let bounds = (
                    x0.max(origin.x),
                    x1.min(opposite.x),
                    z0.max(origin.z),
                    z1.min(opposite.z),
                );
                if bounds.0 >= bounds.1 || bounds.2 >= bounds.3 {
                    continue;
                }
                let centre = (
                    f32::midpoint(bounds.0, bounds.1),
                    f32::midpoint(bounds.2, bounds.3),
                );
                let id = floor_material(level, surfaces, room, centre);
                push_plane(output, bounds, height, patch.backing, false, material(id));
            }
        }
    }
}

fn floor_material<'a>(
    level: &'a LevelDef,
    surfaces: &LevelSurfaces<'a>,
    room: &'a crate::engine::level::RoomDef,
    (x, z): (f32, f32),
) -> &'a str {
    let base = room.material.as_deref().unwrap_or(&level.defaults.floor);
    if let Some(ramp) = surfaces.ramp_at(x, z) {
        return ramp.material.as_deref().unwrap_or(base);
    }
    if let Some(stair) = surfaces.stair_at(x, z) {
        return stair.material.as_deref().unwrap_or(base);
    }
    surfaces
        .region_at(x, z)
        .and_then(|region| region.material.as_deref())
        .or_else(|| {
            level
                .floor_patches
                .iter()
                .rev()
                .find(|patch| {
                    x >= patch.x
                        && x <= patch.x + patch.width
                        && z >= patch.z
                        && z <= patch.z + patch.depth
                })
                .map(|patch| patch.material.as_str())
        })
        .unwrap_or(base)
}

fn push_plane(
    output: &mut Vec<Surface>,
    (x0, x1, z0, z1): (f32, f32, f32, f32),
    y_at: impl Fn((f32, f32)) -> f32,
    backing: f32,
    ceiling: bool,
    material: AcousticMaterial,
) {
    if x1 <= x0 || z1 <= z0 {
        return;
    }
    let y = y_at((x0, z0));
    let sx = (y_at((x1, z0)) - y) / (x1 - x0);
    let sz = (y_at((x0, z1)) - y) / (z1 - z0);
    let plane = Vec3::new(sx, sz.mul_add(-z0, sx.mul_add(-x0, y)), sz);
    let heights = [y, y_at((x1, z0)), y_at((x0, z1)), y_at((x1, z1))];
    let mut bounds = WallAabb::with_y(
        x0,
        heights.into_iter().fold(f32::INFINITY, f32::min),
        z0,
        x1 - x0,
        0.0,
        z1 - z0,
    );
    bounds.max_y = heights.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if ceiling {
        bounds.max_y = backing;
    } else {
        bounds.min_y = backing;
    }
    // Remove grid subdivisions only when their exact material and plane agree.
    // Each room owns a small build-time list; queries never merge or allocate.
    loop {
        let adjacent = output.iter().position(|saved| {
            saved.material == material
                && saved.plane == Some((plane, ceiling))
                && if ceiling {
                    saved.bounds.max_y.to_bits() == backing.to_bits()
                } else {
                    saved.bounds.min_y.to_bits() == backing.to_bits()
                }
                && ((saved.bounds.min_x.to_bits() == bounds.min_x.to_bits()
                    && saved.bounds.max_x.to_bits() == bounds.max_x.to_bits()
                    && (saved.bounds.max_z.to_bits() == bounds.min_z.to_bits()
                        || saved.bounds.min_z.to_bits() == bounds.max_z.to_bits()))
                    || (saved.bounds.min_z.to_bits() == bounds.min_z.to_bits()
                        && saved.bounds.max_z.to_bits() == bounds.max_z.to_bits()
                        && (saved.bounds.max_x.to_bits() == bounds.min_x.to_bits()
                            || saved.bounds.min_x.to_bits() == bounds.max_x.to_bits())))
        });
        let Some(index) = adjacent else { break };
        let saved = output.swap_remove(index).bounds;
        bounds.min_x = bounds.min_x.min(saved.min_x);
        bounds.max_x = bounds.max_x.max(saved.max_x);
        bounds.min_y = bounds.min_y.min(saved.min_y);
        bounds.max_y = bounds.max_y.max(saved.max_y);
        bounds.min_z = bounds.min_z.min(saved.min_z);
        bounds.max_z = bounds.max_z.max(saved.max_z);
    }
    output.push(Surface {
        bounds,
        material,
        plane: Some((plane, ceiling)),
    });
}

fn surface_span(from: Vec3, to: Vec3, surface: &Surface) -> Option<(f32, f32)> {
    let (mut enter, mut leave) = segment_span(from, to, surface.bounds)?;
    let Some((plane, ceiling)) = surface.plane else {
        return Some((enter, leave));
    };
    let sign = if ceiling { 1.0 } else { -1.0 };
    let distance = |p: Vec3| sign * (p.y - plane.x.mul_add(p.x, plane.z.mul_add(p.z, plane.y)));
    let start = distance(from);
    let delta = distance(to) - start;
    if delta.abs() < 1.0e-6 {
        return (start >= 0.0).then_some((enter, leave));
    }
    let crossing = -start / delta;
    if delta > 0.0 {
        enter = enter.max(crossing);
    } else {
        leave = leave.min(crossing);
    }
    (enter <= leave).then_some((enter, leave))
}

fn door_span(from: Vec3, to: Vec3, door: &DoorCollider) -> Option<(f32, f32)> {
    let local = |v: Vec3| {
        Vec3::new(
            (v.x - door.hinge_x).mul_add(door.dir_x, (v.z - door.hinge_z) * door.dir_z),
            v.y,
            (v.z - door.hinge_z).mul_add(door.dir_x, -(v.x - door.hinge_x) * door.dir_z),
        )
    };
    segment_span(
        local(from),
        local(to),
        WallAabb::with_y(
            0.0,
            door.hinge_y,
            -door.thickness * 0.5,
            door.width,
            door.height,
            door.thickness,
        ),
    )
}

#[derive(Clone, Copy)]
struct Hit {
    span: (f32, f32),
    material: AcousticMaterial,
}
#[derive(Default)]
struct Hits {
    entries: [Option<Hit>; 32],
    overflow: bool,
}
impl Hits {
    fn insert(&mut self, mut span: (f32, f32), mut material: AcousticMaterial) {
        // Touching leaves, coincident carrier cells and slabs are one barrier.
        // Merge intervals transitively, independently of BVH traversal order.
        loop {
            let overlap = self.entries.iter().position(|entry| {
                entry.is_some_and(|hit| {
                    hit.span.0 <= span.1 + 1.0e-6 && hit.span.1 >= span.0 - 1.0e-6
                })
            });
            let Some(index) = overlap else { break };
            if let Some(hit) = self.entries.get_mut(index).and_then(Option::take) {
                span = (span.0.min(hit.span.0), span.1.max(hit.span.1));
                material.transmission = material.transmission.min(hit.material.transmission);
                for (value, other) in material.absorption.iter_mut().zip(hit.material.absorption) {
                    *value = value.max(other);
                }
            }
        }
        if let Some(slot) = self.entries.iter_mut().find(|entry| entry.is_none()) {
            *slot = Some(Hit { span, material });
        } else {
            self.overflow = true;
        }
    }
}

/// Project one actual wall face/opening onto this room, clipping both length
/// and height. Normalized footprints permit walls beginning outside the room.
fn wall_room_area(
    room: &crate::engine::level::RoomDef,
    wall: &crate::engine::level::WallDef,
    start: f32,
    end: f32,
    bottom: f32,
    top: f32,
) -> f32 {
    let (x0, x1, z0, z1) = room.bounds();
    let (wx, wz) = wall.min_corner();
    if wx + wall.width.abs() < x0 || wx > x1 || wz + wall.depth.abs() < z0 || wz > z1 {
        return 0.0;
    }
    let (origin, low, high, across) = match wall.axis() {
        WallAxis::X => (wx, x0, x1, wall.depth.abs().mul_add(0.5, wz).clamp(z0, z1)),
        WallAxis::Z => (wz, z0, z1, wall.width.abs().mul_add(0.5, wx).clamp(x0, x1)),
    };
    let a = (origin + start.min(end)).max(low).max(origin);
    let b = (origin + start.max(end))
        .min(high)
        .min(origin + wall.length());
    let clipped_bottom = bottom.max(room.floor_y).max(wall.y);
    let clipped_top = wall.height.map_or(top, |height| top.min(wall.y + height));
    if a >= b || clipped_bottom >= clipped_top {
        return 0.0;
    }
    let ceiling_at = |along| match wall.axis() {
        WallAxis::X => room.ceiling_y_at(along, across),
        WallAxis::Z => room.ceiling_y_at(across, along),
    };
    let ridge = room
        .ridge_across()
        .filter(|_| {
            room.ceiling
                .ridge_axis()
                .is_some_and(|axis| axis != wall.axis())
        })
        .unwrap_or(a)
        .clamp(a, b);
    [(a, ridge), (ridge, b)]
        .into_iter()
        .map(|(left, right)| {
            let (y0, y1) = (ceiling_at(left), ceiling_at(right));
            let mut cuts = [left, right, left, right];
            if (y1 - y0).abs() > 1.0e-6 {
                for (slot, height) in cuts.iter_mut().skip(2).zip([clipped_bottom, clipped_top]) {
                    *slot = ((height - y0) / (y1 - y0))
                        .clamp(0.0, 1.0)
                        .mul_add(right - left, left);
                }
            }
            cuts.sort_unstable_by(f32::total_cmp);
            cuts.windows(2)
                .map(|span| {
                    let l = span.first().copied().unwrap_or(left);
                    let r = span.get(1).copied().unwrap_or(right);
                    (r - l)
                        * f32::midpoint(
                            (ceiling_at(l).min(clipped_top) - clipped_bottom).max(0.0),
                            (ceiling_at(r).min(clipped_top) - clipped_bottom).max(0.0),
                        )
                })
                .sum::<f32>()
        })
        .sum()
}
