//! Deterministic bounded pose variation with real standing-hull witnesses.
use super::{Document, LevelDef, ObjectId, Value, array, inside, json, objects, room_indices};
use crate::engine::{
    collision::{DoorCollider, PLAYER_HEIGHT, PLAYER_RADIUS, WallAabb},
    collision_index::CollisionIndex,
    level::{LevelSurfaces, PropDef},
};
use sha2::{Digest, Sha256};

const STEP: f32 = 0.2;
const MAX_SAMPLES: usize = 65_536;
struct Clearance {
    points: Vec<[f32; 3]>,
    doors: Vec<DoorCollider>,
}
impl Clearance {
    fn new(level: &LevelDef, room_index: usize) -> Result<Self, String> {
        let room = level
            .rooms
            .get(room_index)
            .ok_or("The prop needs a supporting room")?;
        if room.width > 50.0 || room.depth > 50.0 {
            return Err(
                "Protect clearance supports rooms up to 50 by 50 m; split the authoring scope"
                    .into(),
            );
        }
        let doors = level
            .doors
            .iter()
            .map(|d| crate::engine::door::DoorRuntime::new(d, level).collider())
            .collect();
        let mut witnesses = Self {
            points: Vec::new(),
            doors,
        };
        let collision = level.collision_aabbs();
        let index = CollisionIndex::build(&collision);
        let surfaces = LevelSurfaces::new(level);
        for ix in 0_u16..=250 {
            let x = f32::from(ix).mul_add(STEP, room.x);
            if x > room.x + room.width {
                break;
            }
            for iz in 0_u16..=250 {
                let z = f32::from(iz).mul_add(STEP, room.z);
                if z > room.z + room.depth {
                    break;
                }
                let Some(y) = surfaces.floor_y_at(x, z) else {
                    continue;
                };

                let p = [x, y, z];
                if clear(p, &collision, &index, &witnesses.doors)
                    && surfaces.ceiling_y_at(x, z) - y >= PLAYER_HEIGHT
                {
                    witnesses.points.push(p);
                }
                if witnesses.points.len() > MAX_SAMPLES {
                    return Err("Clearance sample budget reached; use a smaller room".into());
                }
            }
        }
        // Exact spawn is a witness even between grid points.
        if inside(room, level.spawn.x, level.spawn.z)
            && let Some(y) = surfaces.floor_y_at(level.spawn.x, level.spawn.z)
        {
            let p = [level.spawn.x, y, level.spawn.z];
            if clear(p, &collision, &index, &witnesses.doors) {
                witnesses.points.push(p);
            }
        }
        Ok(witnesses)
    }
    fn accepts(&self, level: &LevelDef) -> bool {
        let collision = level.collision_aabbs();
        let index = CollisionIndex::build(&collision);
        self.points
            .iter()
            .all(|p| clear(*p, &collision, &index, &self.doors))
    }
}
fn clear(
    [x, y, z]: [f32; 3],
    collision: &[WallAabb],
    index: &CollisionIndex,
    doors: &[DoorCollider],
) -> bool {
    let mut blocked = false;
    index.for_each_disc(x, z, PLAYER_RADIUS, collision, |wall| {
        blocked |= wall.blocks_body(y, PLAYER_HEIGHT) && wall.overlaps_disc(x, z, PLAYER_RADIUS);
    });
    !blocked
        && !doors.iter().any(|door| {
            door.overlaps_body_y(y, PLAYER_HEIGHT) && door.overlaps_disc(x, z, PLAYER_RADIUS)
        })
}
const fn eligible(prop: &PropDef) -> bool {
    prop.physics.is_none()
        && prop.float.is_none()
        && prop.span.is_none()
        && prop.animated_door.is_none()
        && prop.components.is_empty()
        && prop.light_switch.is_none()
        && prop.bindings.is_empty()
}
fn pose(record: &Value) -> Value {
    json!({"rotation_degrees":super::number(record,"rotation_degrees",0.0),"scale":super::number(record,"scale",1.0),"y":super::number(record,"y",0.0)})
}
fn write_pose(raw: &mut Value, index: usize, pose: &Value) -> Result<(), String> {
    let object = objects::object_mut(raw, ObjectId::Prop(index))?;
    for key in ["rotation_degrees", "scale", "y"] {
        drop(object.insert(
            key.into(),
            pose.get(key).ok_or("Missing saved pose")?.clone(),
        ));
    }
    Ok(())
}
fn samples(seed: u64, key: &str) -> [f32; 3] {
    let digest = Sha256::digest([seed.to_le_bytes().as_slice(), key.as_bytes()].concat());
    let mut result = [0.0_f32; 3];
    for (out, bytes) in result.iter_mut().zip(digest.as_chunks::<2>().0) {
        if let (Some(a), Some(b)) = (bytes.first(), bytes.get(1)) {
            *out = f32::from(u16::from_le_bytes([*a, *b])) / 65535.0;
        }
    }
    result
}
fn scope(doc: &Document, selected: ObjectId) -> Result<(Vec<usize>, usize), String> {
    Ok(match selected {
        ObjectId::Room(i) => (room_indices(doc, i, "props")?, i),
        ObjectId::Prop(i) => {
            let p = doc.level().props.get(i).ok_or("Prop disappeared")?;
            let surfaces = LevelSurfaces::new(doc.level());
            let owner = surfaces
                .room_at(p.x, p.z)
                .ok_or("Prop has no supporting room")?;
            let room = doc
                .level()
                .rooms
                .iter()
                .position(|r| std::ptr::eq(r, owner))
                .ok_or("Prop room disappeared")?;
            (vec![i], room)
        }
        ObjectId::Decal(_)
        | ObjectId::Column(_)
        | ObjectId::Pillar(_)
        | ObjectId::ArcWall(_)
        | ObjectId::Wall(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => {
            return Err("Select one room or one fixed scenery prop for variation".into());
        }
    })
}

pub(super) fn candidate(
    doc: &Document,
    selected: ObjectId,
    seed: u64,
    ranges: [f32; 3],
    protect: bool,
    reset: bool,
) -> Result<(Value, String, Option<ObjectId>), String> {
    let [yaw, scale, sink] = ranges;
    if !(0.0..=180.0).contains(&yaw)
        || !(0.0..=0.5).contains(&scale)
        || !(0.0..=0.25).contains(&sink)
    {
        return Err("Yaw 0..180 degrees; scale fraction 0..0.5; sinking 0..0.25 m".into());
    }
    let (indices, room_index) = scope(doc, selected)?;
    if indices.len() > 128 {
        return Err("Variation scope is limited to 128 props per preview".into());
    }
    let (mut raw, restored) = baseline_scope(doc, &indices)?;
    if reset {
        return Ok((
            raw,
            format!("Restored {restored} saved poses; unrelated fields retained."),
            None,
        ));
    }
    let baseline_level = doc.validate_candidate(&raw)?;
    let witnesses = if protect {
        Some(Clearance::new(&baseline_level, room_index)?)
    } else {
        None
    };
    let mut varied = 0_usize;
    let mut skipped = 0_usize;
    let mut protected = 0_usize;
    for i in indices {
        let prop = doc.level().props.get(i).ok_or("Prop disappeared")?;
        if !eligible(prop) {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let original = array(doc.raw(), "props").get(i).ok_or("Prop disappeared")?;
        if original.get("editor_locked").and_then(Value::as_bool) == Some(true) {
            return Err("Unlock every affected prop before previewing this scope".into());
        }
        let current = pose(original);
        let saved = original.get("editor_variation");
        let base = if let Some(meta) = saved {
            if meta.get("last") != Some(&current) {
                return Err("A varied pose was edited manually. Restore its saved baseline before changing variation settings.".into());
            }
            meta.get("base")
                .ok_or("Invalid variation baseline")?
                .clone()
        } else {
            current
        };
        let key = saved
            .and_then(|m| m.get("key"))
            .and_then(Value::as_str)
            .map_or_else(
                || {
                    let bytes = serde_json::to_vec(original).unwrap_or_default();
                    crate::engine::package::hash::digest_hex(&Sha256::digest(bytes))
                },
                str::to_owned,
            );
        let [a, b, c] = samples(seed, &key);
        let proposed = json!({"rotation_degrees":a.mul_add(2.0,-1.0).mul_add(yaw,super::number(&base,"rotation_degrees",0.0)).rem_euclid(360.0),
            "scale":super::number(&base,"scale",1.0)*b.mul_add(2.0,-1.0).mul_add(scale,1.0),"y":c.mul_add(-sink,super::number(&base,"y",0.0))});
        let previous = raw.clone();
        write_pose(&mut raw, i, &proposed)?;
        let level = doc.validate_candidate(&raw)?;
        if witnesses.as_ref().is_some_and(|w| !w.accepts(&level)) {
            raw = previous;
            protected = protected.saturating_add(1);
            continue;
        }
        drop(objects::object_mut(&mut raw,ObjectId::Prop(i))?.insert("editor_variation".into(),json!({"key":key,"base":base,"base_fields":base_fields(original)?,"last":proposed,"seed":seed,"ranges":ranges,"protect":protect})));
        varied = varied.saturating_add(1);
    }
    let samples = witnesses.as_ref().map_or(0, |w| w.points.len());
    Ok((
        raw,
        format!(
            "{varied} poses; {protected} clearance refusals; {skipped} active/fitted skipped; {samples} standing samples."
        ),
        None,
    ))
}

fn base_fields(original: &Value) -> Result<Value, String> {
    if let Some(meta) = original.get("editor_variation") {
        return meta.get("base_fields").filter(|v|v.is_object()).cloned().ok_or_else(||"Saved variation fields are malformed. Keep current base to start a new seed baseline.".into());
    }
    let mut fields = serde_json::Map::new();
    for key in ["rotation_degrees", "scale", "y"] {
        if let Some(v) = original.get(key) {
            drop(fields.insert(key.into(), v.clone()));
        }
    }
    Ok(Value::Object(fields))
}
fn baseline_scope(doc: &Document, indices: &[usize]) -> Result<(Value, usize), String> {
    let mut raw = doc.raw().clone();
    let mut restored = 0_usize;
    for i in indices {
        let prop = doc.level().props.get(*i).ok_or("Missing prop")?;
        if !eligible(prop) {
            continue;
        }
        let original = array(doc.raw(), "props").get(*i).ok_or("Missing prop")?;
        if let Some(saved) = original.get("editor_variation") {
            if saved.get("last") != Some(&pose(original)) {
                return Err("A varied pose was edited manually. Choose Keep current base to adopt it, or undo that pose edit; nothing was overwritten.".into());
            }
            let fields = base_fields(original)?;
            let record = objects::object_mut(&mut raw, ObjectId::Prop(*i))?;
            for key in ["rotation_degrees", "scale", "y"] {
                drop(record.remove(key));
                if let Some(v) = fields.get(key) {
                    drop(record.insert(key.into(), v.clone()));
                }
            }
            drop(record.remove("editor_variation"));
            restored = restored.saturating_add(1);
        }
    }
    Ok((raw, restored))
}

pub(super) fn forget(
    doc: &Document,
    selected: ObjectId,
) -> Result<(Value, String, Option<ObjectId>), String> {
    let (indices, _room) = scope(doc, selected)?;
    let mut raw = doc.raw().clone();
    let mut count = 0_usize;
    for i in indices {
        if objects::object_mut(&mut raw, ObjectId::Prop(i))?
            .remove("editor_variation")
            .is_some()
        {
            count = count.saturating_add(1);
        }
    }
    Ok((
        raw,
        format!("Keep current poses; forget {count} seed baselines. Next variation starts here."),
        None,
    ))
}
