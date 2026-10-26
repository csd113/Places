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

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RoomResponse {
    pub wet: f32,
    pub decay_seconds: f32,
    pub damping: f32,
    pub size: [f32; 3],
}
struct Surface {
    bounds: WallAabb,
    material: AcousticMaterial,
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
                });
            }
            for opening in &wall.openings {
                if let Some(glass) = opening.glass_material()
                    && let Some(bounds) = crate::engine::level::opening_glass_bounds(wall, opening)
                {
                    scene.surfaces.push(Surface {
                        bounds,
                        material: material(glass),
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
                });
            }
        }
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
    #[must_use]
    pub const fn counts(&self) -> [usize; 3] {
        [self.surfaces.len(), self.nodes.len(), self.doors.len()]
    }
    /// At most 256 BVH nodes and 256 door tests, no allocations or queued work.
    /// Exhaustion returns conservative muffling; it never grows a backlog.
    #[must_use]
    pub fn path(&self, from: Vec3, to: Vec3) -> AcousticPath {
        let mut path = AcousticPath::default();
        let mut stack = [0_usize; 64];
        let mut len = usize::from(!self.nodes.is_empty());
        let mut work = 0_usize;
        while len > 0 && work < 256 && path.transmission > 0.025 {
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
                    if segment_box(from, to, surface.bounds) {
                        block(&mut path, surface.material);
                    }
                }
            }
        }
        if len > 0 && work >= 256 {
            path.budget_exhausted = true;
            block(&mut path, AcousticMaterial::default());
        }
        for door in self.doors.iter().take(256) {
            let local = |v: Vec3| {
                Vec3::new(
                    (v.x - door.hinge_x).mul_add(door.dir_x, (v.z - door.hinge_z) * door.dir_z),
                    v.y,
                    (v.z - door.hinge_z).mul_add(door.dir_x, -(v.x - door.hinge_x) * door.dir_z),
                )
            };
            if segment_box(
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
            ) {
                block(
                    &mut path,
                    AcousticMaterial {
                        absorption: [0.15, 0.3, 0.55],
                        transmission: 0.23,
                    },
                );
            }
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
                return false;
            }
        } else {
            let a = (low - origin) / delta;
            let c = (high - origin) / delta;
            enter = enter.max(a.min(c));
            leave = leave.min(a.max(c));
            if enter > leave {
                return false;
            }
        }
    }
    true
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
        let x_axis = bounds.max_x - bounds.min_x >= bounds.max_z - bounds.min_z;
        surfaces.sort_unstable_by(|a, b| {
            let center = |s: &Surface| {
                if x_axis {
                    s.bounds.min_x + s.bounds.max_x
                } else {
                    s.bounds.min_z + s.bounds.max_z
                }
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
    for room in &level.rooms {
        let w = room.width.abs();
        let d = room.depth.abs();
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
        apply_material_overrides(level, room, material, &mut weighted, floor, wall);
        let mut aperture = 0.0;
        for wall_def in &level.walls {
            if wall_def.x >= room.x - 0.5
                && wall_def.x <= room.x + w + 0.5
                && wall_def.z >= room.z - 0.5
                && wall_def.z <= room.z + d + 0.5
            {
                for opening in &wall_def.openings {
                    if opening.glass.is_none() {
                        aperture = f32::mul_add(opening.width, opening.height, aperture);
                    }
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
            bounds: WallAabb::with_y(room.x, room.floor_y, room.z, w, h, d),
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
    material: &impl Fn(&str) -> AcousticMaterial,
    weighted: &mut [f32; 3],
    floor: AcousticMaterial,
    wall: AcousticMaterial,
) {
    let w = room.width.abs();
    let d = room.depth.abs();
    let h = room.height;
    // Openings leak energy; closed leaves restore absorption dynamically.
    for wall_def in &level.walls {
        let overlap_x = (wall_def.x + wall_def.width).min(room.x + w) - wall_def.x.max(room.x);
        let overlap_z = (wall_def.z + wall_def.depth).min(room.z + d) - wall_def.z.max(room.z);
        if overlap_x >= -0.1 && overlap_z >= -0.1 {
            let surface_area = (overlap_x.max(0.0) + overlap_z.max(0.0)) * h;
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
    }
    for patch in &level.floor_patches {
        let area = ((patch.x + patch.width).min(room.x + w) - patch.x.max(room.x)).max(0.0)
            * ((patch.z + patch.depth).min(room.z + d) - patch.z.max(room.z)).max(0.0);
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
