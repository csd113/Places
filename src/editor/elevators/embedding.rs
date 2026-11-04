//! Undoable hollowing of the selected architectural host, using saved primitives.
use super::Document;
use crate::editor::document::ObjectId;
use crate::engine::elevator::ElevatorDef;
use crate::engine::level::{LevelSurfaces, WallDef};
use glam::Vec3;
use serde_json::{Value, json};

const EPS: f32 = 0.0005;
#[derive(Clone)]
struct Round {
    radius: f32,
    theta: f32,
    yaw: f32,
    segments: u32,
    polygon: Vec<(f32, f32)>,
}
#[derive(Clone)]
pub(super) struct Embedding {
    pub host: ObjectId,
    pub surface: WallDef,
    low: Vec3,
    high: Vec3,
    round: Option<Round>, // radius, cut half-angle, facing yaw
}
fn wall(value: Value) -> Result<WallDef, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}
impl Embedding {
    pub fn from_wall(doc: &Document, index: usize) -> Result<Self, String> {
        let surface = doc.level().walls.get(index).ok_or("Missing wall")?.clone();
        let (x, z) = surface.min_corner();
        let height = surface.height.unwrap_or_else(|| {
            LevelSurfaces::new(doc.level())
                .clear_ceiling_height_along(&surface, surface.length() * 0.5)
        });
        Ok(Self {
            host: ObjectId::Wall(index),
            low: Vec3::new(x, surface.y, z),
            high: Vec3::new(
                x + surface.width.abs(),
                surface.y + height,
                z + surface.depth.abs(),
            ),
            surface,
            round: None,
        })
    }
    pub fn contains(&self, def: &ElevatorDef, floor: f32) -> bool {
        if matches!(self.host, ObjectId::Wall(_)) {
            return true;
        }
        let sections = [
            (-1.45, 1.45, 0.018, 0.362, 2.57),
            (-1.28, 1.28, 0.24, 2.48, 2.63),
        ];
        sections
            .into_iter()
            .all(|(left, right, front, back, height)| {
                floor >= self.low.y - EPS
                    && floor + height <= self.high.y + EPS
                    && [left, right].into_iter().all(|x| {
                        [front, back].into_iter().all(|z| {
                            let p = def.point(x, floor, z);
                            self.round.as_ref().map_or(
                                p.x >= self.low.x - EPS
                                    && p.x <= self.high.x + EPS
                                    && p.z >= self.low.z - EPS
                                    && p.z <= self.high.z + EPS,
                                |round| {
                                    round
                                        .polygon
                                        .iter()
                                        .zip(round.polygon.iter().cycle().skip(1))
                                        .all(|(&(ax, az), &(bx, bz))| {
                                            let (dx, dz) = (bx - ax, bz - az);
                                            dx.mul_add(p.z - az, -dz * (p.x - ax)) / dx.hypot(dz)
                                                >= 0.12 - EPS
                                        })
                                },
                            )
                        })
                    })
            })
    }
    pub fn apply(
        &self,
        doc: &Document,
        raw: &mut Value,
        def: &ElevatorDef,
        floor: f32,
    ) -> Result<(), String> {
        if !self.contains(def, floor) {
            return Err("Not enough space: this pillar cannot contain the elevator cabin and sliding-door pockets.".into());
        }
        if let Some(round) = &self.round {
            self.hollow_round(doc, raw, round)?;
        } else {
            let (kind, index) = if let ObjectId::Wall(index) = self.host {
                ("walls", index)
            } else if let ObjectId::Column(index) = self.host {
                ("columns", index)
            } else {
                return Err("Unsupported elevator host".into());
            };
            let source = raw
                .get(kind)
                .and_then(Value::as_array)
                .and_then(|a| a.get(index))
                .cloned()
                .ok_or("Missing elevator host")?;
            let mut pieces = vec![(self.low, self.high)];
            // Preserve every existing bare wall opening before cutting the new void.
            if let ObjectId::Wall(i) = self.host {
                let original = doc.level().walls.get(i).ok_or("Missing wall")?;
                for opening in &original.openings {
                    if opening.recess.is_some()
                        || opening.glass.is_some()
                        || opening.surround.is_some()
                    {
                        return Err("Remove the host wall's pane, recess or surround before embedding an elevator.".into());
                    }
                    let (x, z) = original.min_corner();
                    let (lo, hi) = match original.axis() {
                        crate::engine::level::WallAxis::X => (
                            Vec3::new(x + opening.offset, opening.bottom(original.y), z),
                            Vec3::new(x + opening.end(), opening.top(original.y), self.high.z),
                        ),
                        crate::engine::level::WallAxis::Z => (
                            Vec3::new(x, opening.bottom(original.y), z + opening.offset),
                            Vec3::new(self.high.x, opening.top(original.y), z + opening.end()),
                        ),
                    };
                    pieces = cut(pieces, lo, hi);
                }
            }
            for (lo, hi) in cavity(def, floor) {
                pieces = cut(pieces, lo, hi);
            }
            let records = super::super::document::record_array_mut(raw, kind)?;
            drop(records.remove(index));
            for (lo, hi) in pieces {
                let mut record = source.clone();
                let object = record.as_object_mut().ok_or("Invalid host record")?;
                for (key, value) in [
                    ("x", lo.x),
                    ("y", lo.y),
                    ("z", lo.z),
                    ("width", hi.x - lo.x),
                    ("height", hi.y - lo.y),
                    ("depth", hi.z - lo.z),
                ] {
                    drop(object.insert(key.into(), json!(value)));
                }
                drop(object.remove("openings"));
                // Thin cut faces are internal to the retained elevator shell;
                // do not generate decorative trim on every decomposition seam.
                if let Some(trim) = object.get_mut("trim").and_then(Value::as_object_mut) {
                    let placement = trim
                        .get("placement")
                        .and_then(Value::as_str)
                        .unwrap_or("both");
                    let bottom = lo.y <= self.low.y + EPS && matches!(placement, "both" | "bottom");
                    let top = hi.y >= self.high.y - EPS && matches!(placement, "both" | "top");
                    drop(trim.insert(
                        "placement".into(),
                        json!(match (bottom, top) {
                            (true, true) => "both",
                            (true, false) => "bottom",
                            (false, true) => "top",
                            (false, false) => "none",
                        }),
                    ));
                }
                records.push(record);
            }
        }
        seat_deck(doc, raw, def, floor)
    }
    fn hollow_round(&self, doc: &Document, raw: &mut Value, round: &Round) -> Result<(), String> {
        let Round {
            radius,
            theta,
            yaw,
            segments,
            polygon: _,
        } = round;
        let ObjectId::Pillar(index) = self.host else {
            return Err("Missing round pillar".into());
        };
        let original = doc
            .level()
            .pillars
            .get(index)
            .ok_or("Missing round pillar")?;
        let mut arc = serde_json::to_value(original).map_err(|error| error.to_string())?;
        let object = arc.as_object_mut().ok_or("Invalid pillar")?;
        let sweep = (-2.0_f32).mul_add(*theta, 360.0);
        let cap_height = (self.high.y - self.low.y - 2.631).clamp(0.0, 0.02);
        // Retain the outer radius and authored segment budget on the remaining shell.
        for (key, value) in [
            ("radius", radius - 0.05),
            ("thickness", 0.1),
            ("start_degrees", (-yaw + theta).rem_euclid(360.0)),
            ("sweep_degrees", sweep),
            ("y", self.low.y),
            ("height", self.high.y - self.low.y - cap_height),
        ] {
            drop(object.insert(key.into(), json!(value)));
        }
        drop(object.insert("segments".into(), json!(segments)));
        super::super::document::record_array_mut(raw, "arc_walls")?.push(arc);
        let mut roof = serde_json::to_value(original).map_err(|error| error.to_string())?;
        let roof_object = roof.as_object_mut().ok_or("Invalid pillar cap")?;
        drop(roof_object.insert("y".into(), json!(self.high.y - cap_height)));
        drop(roof_object.insert("height".into(), json!(cap_height)));
        let pillars = super::super::document::record_array_mut(raw, "pillars")?;
        drop(pillars.remove(index));
        if cap_height > EPS {
            pillars.push(roof);
        }
        let facade = serde_json::to_value(&self.surface).map_err(|error| error.to_string())?;
        super::super::document::record_array_mut(raw, "walls")?.push(facade);
        // The facade is only 10 mm deep: the fixed frame covers its cut edge.
        let walls = super::super::document::record_array_mut(raw, "walls")?;
        let record = walls.last_mut().ok_or("Missing pillar facade")?;
        drop(record.as_object_mut().ok_or("Invalid facade")?.insert("openings".into(),json!([{"kind":"door","offset":self.surface.length().mul_add(0.5,-0.75),"width":1.5_f32,"height":2.25_f32}])));
        Ok(())
    }
}

fn seat_deck(doc: &Document, raw: &mut Value, def: &ElevatorDef, floor: f32) -> Result<(), String> {
    // In-room cabins must not share a plane with the room's floor mesh.
    let a = def.point(-1.2, floor, 0.0);
    let b = def.point(1.2, floor, 2.4);
    let (x0, x1, z0, z1) = (a.x.min(b.x), a.x.max(b.x), a.z.min(b.z), a.z.max(b.z));
    let has_support = doc
        .level()
        .rooms
        .iter()
        .any(|r| (r.floor_y - floor).abs() < 0.001 && r.contains(x0, z0) && r.contains(x1, z1));
    let other_storey = doc.level().rooms.iter().any(|r| {
        let (left, right, front, back) = r.bounds();
        x0 < right && x1 > left && z0 < back && z1 > front && (r.floor_y - floor).abs() > 0.001
    });
    if has_support && !other_storey {
        super::super::document::record_array_mut(raw, "floor_regions")?
            .push(json!({"x":x0,"z":z0,"width":x1-x0,"depth":z1-z0,"offset_y":-0.012_f32}));
    }
    Ok(())
}
pub(super) fn cavity(def: &ElevatorDef, floor: f32) -> Vec<(Vec3, Vec3)> {
    [
        (-0.75, 0.75, -0.002, 0.364, 2.25),
        (-1.451, 1.451, 0.018, 0.364, 2.571),
        (-1.281, 1.281, 0.24, 2.482, 2.631),
    ]
    .into_iter()
    .map(|(x0, x1, z0, z1, h)| {
        let a = def.point(x0, floor, z0);
        let b = def.point(x1, floor + h, z1);
        (a.min(b), a.max(b))
    })
    .collect()
}
fn cut(boxes: Vec<(Vec3, Vec3)>, low: Vec3, high: Vec3) -> Vec<(Vec3, Vec3)> {
    boxes
        .into_iter()
        .flat_map(|(a, b)| {
            let lo = a.max(low);
            let hi = b.min(high);
            if (hi.x - lo.x).min(hi.y - lo.y).min(hi.z - lo.z) <= EPS {
                return vec![(a, b)];
            }
            [
                (a, Vec3::new(lo.x, b.y, b.z)),
                (Vec3::new(hi.x, a.y, a.z), b),
                (Vec3::new(lo.x, a.y, a.z), Vec3::new(hi.x, lo.y, b.z)),
                (Vec3::new(lo.x, hi.y, a.z), Vec3::new(hi.x, b.y, b.z)),
                (Vec3::new(lo.x, lo.y, a.z), Vec3::new(hi.x, hi.y, lo.z)),
                (Vec3::new(lo.x, lo.y, hi.z), Vec3::new(hi.x, hi.y, b.z)),
            ]
            .into_iter()
            .filter(|(l, h)| (h.x - l.x).min(h.y - l.y).min(h.z - l.z) > EPS)
            .collect()
        })
        .collect()
}

/// Pick all four rectangular post faces, or a fitted flat chord on a round post.
pub(super) fn under_ray(
    doc: &Document,
    eye: Vec3,
    ray: Vec3,
    visible: impl Fn(ObjectId) -> bool,
) -> Option<(Embedding, f32, [f32; 3])> {
    column_hits(doc, eye, ray)
        .into_iter()
        .chain(round_hits(doc, eye, ray))
        .filter(|(host, _, _)| visible(host.host))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}
type Hit = (Embedding, f32, [f32; 3]);
fn column_hits(doc: &Document, eye: Vec3, ray: Vec3) -> Vec<Hit> {
    let surfaces = LevelSurfaces::new(doc.level());
    let mut candidates = Vec::new();
    for (index, column) in doc.level().columns.iter().enumerate() {
        let (x0, x1, z0, z1) = column.bounds();
        let base = column.base_y(&surfaces);
        let top = column.top_y(&surfaces);
        for (axis, face, positive) in [
            (0_u8, x0, true),
            (0, x1, false),
            (2, z0, true),
            (2, z1, false),
        ] {
            let delta = if axis == 0 { ray.x } else { ray.z };
            let source = if axis == 0 { eye.x } else { eye.z };
            let t = (face - source) / delta;
            if !t.is_finite() || !(0.0..=500.0).contains(&t) {
                continue;
            }
            let p = ray.mul_add(Vec3::splat(t), eye);
            if p.x < x0 - EPS
                || p.x > x1 + EPS
                || p.z < z0 - EPS
                || p.z > z1 + EPS
                || p.y < base
                || p.y > top
            {
                continue;
            }
            let value = if axis == 2 {
                json!({"x":x0,"z":if positive{face}else{face-0.01},"y":base,"width":x1-x0,"depth":0.01_f32,"height":top-base})
            } else {
                json!({"x":if positive{face}else{face-0.01},"z":z0,"y":base,"width":0.01_f32,"depth":z1-z0,"height":top-base})
            };
            if let Ok(mut surface) = wall(value) {
                surface.material.clone_from(&column.material);
                surface.shine = column.shine;
                candidates.push((
                    Embedding {
                        host: ObjectId::Column(index),
                        surface,
                        low: Vec3::new(x0, base, z0),
                        high: Vec3::new(x1, top, z1),
                        round: None,
                    },
                    t,
                    [p.x, base, p.z],
                ));
            }
        }
    }
    candidates
}
fn round_hits(doc: &Document, eye: Vec3, ray: Vec3) -> Vec<Hit> {
    let surfaces = LevelSurfaces::new(doc.level());
    let mut candidates = Vec::new();
    for (index, pillar) in doc.level().pillars.iter().enumerate() {
        let base = pillar.base_y(&surfaces);
        let top = pillar.top_y(&surfaces);
        let radius = pillar.radius;

        let dx = eye.x - pillar.x;
        let dz = eye.z - pillar.z;
        let yaw = if dz.abs() >= dx.abs() {
            if dz < 0.0 { 0.0 } else { 180.0 }
        } else if dx < 0.0 {
            90.0
        } else {
            270.0
        };
        let count = f32::from(u16::try_from(pillar.resolved_segments()).unwrap_or(128));
        let step = 360.0 / count;
        let cut_segments = (1..pillar.resolved_segments())
            .find(|segment| {
                radius
                    * (f32::from(u16::try_from(*segment).unwrap_or(128)) * step)
                        .to_radians()
                        .sin()
                    >= 1.55
            })
            .unwrap_or_else(|| pillar.resolved_segments());
        let angle = f32::from(u16::try_from(cut_segments).unwrap_or(128)) * step;
        let theta = if angle >= 90.0 { 0.0 } else { angle };
        let depth = radius * theta.to_radians().cos();
        let half = if theta == 0.0 {
            radius
        } else {
            radius * theta.to_radians().sin()
        };
        let def = ElevatorDef {
            x: pillar.x,
            z: pillar.z,
            rotation_degrees: yaw,
            id: String::new(),
            initial_stop: 0,
            speed_mps: 1.5,
            stops: Vec::new(),
        };
        let centre = def.point(0.0, base, -depth);
        let edge_start = def.point(-half, base, -depth);
        let edge_end = def.point(half, base, -depth + 0.01);
        let (x0, x1, z0, z1) = (
            edge_start.x.min(edge_end.x),
            edge_start.x.max(edge_end.x),
            edge_start.z.min(edge_end.z),
            edge_start.z.max(edge_end.z),
        );
        let normal = Vec3::new(yaw.to_radians().sin(), 0.0, yaw.to_radians().cos());
        let t = Vec3::new(centre.x - eye.x, centre.y - eye.y, centre.z - eye.z).dot(normal)
            / ray.dot(normal);
        let p = ray.mul_add(Vec3::splat(t), eye);
        if !t.is_finite()
            || !(0.0..=500.0).contains(&t)
            || p.y < base
            || p.y > top
            || Vec3::new(p.x - centre.x, 0.0, p.z - centre.z).length() > half
        {
            continue;
        }
        if let Ok(mut surface) =
            wall(json!({"x":x0,"z":z0,"y":base,"width":x1-x0,"depth":z1-z0,"height":top-base}))
        {
            surface.material.clone_from(&pillar.material);
            surface.shine = pillar.shine;
            surface.openings=serde_json::from_value(json!([{"kind":"door","offset":surface.length().mul_add(0.5,-0.75),"width":1.5_f32,"height":2.25_f32}])).unwrap_or_default();
            candidates.push((
                Embedding {
                    host: ObjectId::Pillar(index),
                    surface,
                    low: Vec3::new(pillar.x - radius, base, pillar.z - radius),
                    high: Vec3::new(pillar.x + radius, top, pillar.z + radius),
                    round: Some(Round {
                        radius,
                        theta,
                        yaw,
                        segments: pillar
                            .resolved_segments()
                            .saturating_sub(cut_segments.saturating_mul(2)),
                        polygon: pillar.polygon_points(),
                    }),
                },
                t,
                [centre.x, base, centre.z],
            ));
        }
    }
    candidates
}
