//! Map-local prefab sources and explicit three-way instance updates.
use super::{Document, ObjectId, array, inside, number, objects, room_indices};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const COLLECTIONS: [&str; 7] = [
    "rooms",
    "walls",
    "columns",
    "pillars",
    "props",
    "doors",
    "ceiling_lights",
];
const META: &str = "editor_reuse";
const MEMBER: &str = "editor_prefab";
const SOURCE: &str = "editor_prefab_source";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Item {
    collection: String,
    key: String,
    record: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Source {
    items: Vec<Item>,
    origin: [f32; 2],
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    source: String,
    destination: [f32; 2],
    baseline: Vec<Item>,
}

pub(super) fn names(raw: &Value) -> Vec<String> {
    raw.pointer("/editor_reuse/prefabs")
        .and_then(Value::as_object)
        .map_or_else(Vec::new, |m| m.keys().cloned().collect())
}
fn meta(raw: &mut Value) -> Result<&mut serde_json::Map<String, Value>, String> {
    raw.as_object_mut()
        .ok_or("Invalid scene")?
        .entry(META)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "Invalid editor reuse metadata".into())
}
fn table<'a>(
    raw: &'a mut Value,
    name: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    meta(raw)?
        .entry(name)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "Invalid reusable source table".into())
}
fn next(raw: &mut Value) -> Result<u64, String> {
    let previous = meta(raw)?
        .get("sequence")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let n = previous
        .checked_add(1)
        .ok_or("Prefab identity budget exhausted")?;
    drop(meta(raw)?.insert("sequence".into(), json!(n)));
    Ok(n)
}
fn clean(mut value: Value) -> Value {
    if let Some(m) = value.as_object_mut() {
        drop(m.remove(MEMBER));
        drop(m.remove(SOURCE));
        drop(m.remove("editor_variation"));
        drop(m.remove("editor_locked"));
        drop(m.remove("editor_flicker"));
    }
    value
}
fn source(raw: &Value, name: &str) -> Result<Source, String> {
    let value = raw.pointer("/editor_reuse/prefabs").and_then(|v|v.get(name))
        .ok_or("Prefab source is missing. Existing geometry is preserved; restore the source or detach this instance.")?;
    let s: Source =
        serde_json::from_value(value.clone()).map_err(|e| format!("Invalid prefab source: {e}"))?;
    if s.items.is_empty() || s.items.len() > 512 || !s.origin.iter().all(|n| n.is_finite()) {
        return Err("Prefab source needs 1..512 members and a finite origin".into());
    }
    let mut keys = BTreeSet::new();
    for item in &s.items {
        if !COLLECTIONS.contains(&item.collection.as_str())
            || !keys.insert(&item.key)
            || !item.record.is_object()
        {
            return Err("Invalid or duplicate prefab member identity".into());
        }
    }
    if s.items.iter().filter(|i| i.collection == "rooms").count() != 1 {
        return Err("A prefab must have exactly one room carrier".into());
    }
    Ok(s)
}
fn safe_room(doc: &Document, index: usize) -> Result<(), String> {
    let room = doc
        .level()
        .rooms
        .get(index)
        .ok_or("Select an existing room")?;
    for (i, other) in doc.level().rooms.iter().enumerate() {
        if i != index
            && (room.floor_y - other.floor_y).abs() < room.height.max(other.height)
            && room.x < other.x + other.width - 0.001
            && other.x < room.x + room.width - 0.001
            && room.z < other.z + other.depth - 0.001
            && other.z < room.z + room.depth - 0.001
        {
            return Err(
                "Overlapping room ownership is ambiguous. Capture a standalone room or corridor."
                    .into(),
            );
        }
    }
    for key in [
        "arc_walls",
        "ramps",
        "stairs",
        "guardrails",
        "half_walls",
        "archways",
        "thresholds",
        "baseboards",
        "decals",
        "floor_patches",
        "floor_regions",
        "water",
        "ladders",
        "sound_emitters",
        "volumes",
        "fog_regions",
        "ground_fog",
        "spawn_points",
        "effects",
    ] {
        if array(doc.raw(), key).iter().any(|r| {
            let point = r.get("min").and_then(Value::as_array);
            let x = point.and_then(|v| v.first()).map_or_else(
                || number(r, "x", 0.0),
                |v| number(&json!({"n":v}), "n", 0.0),
            );
            let z = point.and_then(|v| v.get(2)).map_or_else(
                || number(r, "z", 0.0),
                |v| number(&json!({"n":v}), "n", 0.0),
            );
            inside(room, x, z)
        }) {
            return Err(format!(
                "This room contains {key}. Room prefabs currently capture rooms, walls/openings/trim, pillars, fixed props, doors and lights; move unsupported content outside before capture."
            ));
        }
    }
    Ok(())
}

pub(super) fn capture(
    doc: &Document,
    index: usize,
    name: &str,
) -> Result<(Value, String, Option<ObjectId>), String> {
    if name.is_empty()
        || name.len() > 48
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err("Source name: 1..48 letters, digits, underscores or dashes".into());
    }
    safe_room(doc, index)?;
    if names(doc.raw()).len() >= 32 && !names(doc.raw()).iter().any(|n| n == name) {
        return Err("This map already has 32 reusable room sources".into());
    }
    let room = doc.level().rooms.get(index).ok_or("Missing room")?;
    let mut raw = doc.raw().clone();
    let ids = [
        doc.level().prop_instance_ids(),
        doc.level().door_instance_ids(),
        doc.level().light_instance_ids(),
    ];
    let items = capture_items(doc, index, name, &mut raw, &ids)?;
    if items.len() > 512 {
        return Err("Prefab member limit is 512".into());
    }
    // Circuit ownership cannot reach across the room capture boundary.
    let captured: BTreeSet<_> = items
        .iter()
        .filter_map(|i| i.record.get("id").and_then(Value::as_str))
        .collect();
    let all: BTreeSet<_> = ids.iter().flatten().map(String::as_str).collect();
    for item in &items {
        check_references(&item.record, &captured, &all)?;
    }
    for item in &items {
        if let Some(group) = item
            .record
            .get("switch_group")
            .or_else(|| item.record.get("light_switch"))
            .and_then(Value::as_str)
        {
            for (key, field) in [
                ("ceiling_lights", "switch_group"),
                ("props", "light_switch"),
            ] {
                if array(doc.raw(), key).iter().any(|v| {
                    v.get(field).and_then(Value::as_str) == Some(group)
                        && !v
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| captured.contains(id))
                }) {
                    return Err("Circuit crosses this room boundary. Keep its switch and all members in the captured room.".into());
                }
            }
        }
    }
    let count = items.len();
    let s = Source {
        items,
        origin: [room.x, room.z],
    };
    drop(table(&mut raw, "prefabs")?.insert(
        name.into(),
        serde_json::to_value(s).map_err(|e| e.to_string())?,
    ));
    Ok((
        raw,
        format!(
            "Capture {name}: {count} members. Existing linked instances update only when requested."
        ),
        None,
    ))
}
fn capture_items(
    doc: &Document,
    index: usize,
    name: &str,
    raw: &mut Value,
    ids: &[Vec<String>; 3],
) -> Result<Vec<Item>, String> {
    let room = doc.level().rooms.get(index).ok_or("Missing room")?;
    let mut items = Vec::new();
    for key in COLLECTIONS {
        let indices = if key == "rooms" {
            vec![index]
        } else {
            room_indices(doc, index, key)?
        };
        for i in indices {
            let original = array(doc.raw(), key).get(i).ok_or("Member disappeared")?;
            if original.get("editor_locked").and_then(Value::as_bool) == Some(true) {
                return Err(format!(
                    "Unlock {key} member {} before capture",
                    i.saturating_add(1)
                ));
            }
            if key == "props" {
                let prop = doc.level().props.get(i).ok_or("Missing prop")?;
                if prop.physics.is_some()
                    || prop.float.is_some()
                    || prop.span.is_some()
                    || prop.components.iter().any(|c| {
                        !matches!(
                            c,
                            crate::engine::level::ComponentDef::Interactable {
                                prompt: _,
                                reach: _,
                                enabled: _,
                                label: _
                            }
                        )
                    })
                    || prop.animated_door.is_some()
                {
                    return Err("Prefab props must be fixed scenery or a circuit switch without scripted components".into());
                }
            }
            if original
                .pointer("/editor_flicker/settings/enabled")
                .and_then(Value::as_bool)
                == Some(true)
                || contains_script(original)
            {
                return Err("A captured member owns scripted behavior/flicker. Disable its generated flicker and remove script bindings before capture; embedded script identities cannot be safely cloned.".into());
            }
            if matches!(key, "walls" | "columns")
                && !inside(
                    room,
                    number(original, "x", 0.0) + number(original, "width", 0.0),
                    number(original, "z", 0.0) + number(original, "depth", 0.0),
                )
            {
                return Err("A wall or pillar crosses the room boundary. Capture the complete carrier instead.".into());
            }
            let old_key = original
                .get(SOURCE)
                .and_then(|s| s.get("key"))
                .and_then(Value::as_str);
            let member_key = if let Some(k) = old_key {
                k.to_owned()
            } else {
                format!("m{}", next(raw)?)
            };
            let record = objects::array_mut(raw, key)?
                .get_mut(i)
                .ok_or("Member disappeared")?
                .as_object_mut()
                .ok_or("Invalid member")?;
            drop(record.insert(SOURCE.into(), json!({"source":name,"key":member_key})));
            let identities = match key {
                "props" => ids.first(),
                "doors" => ids.get(1),
                "ceiling_lights" => ids.get(2),
                _ => None,
            };
            if let Some(id) = identities.and_then(|a| a.get(i)) {
                drop(record.insert("id".into(), json!(id)));
            }
            let record_value = Value::Object(record.clone());
            items.push(Item {
                collection: key.into(),
                key: member_key,
                record: clean(record_value),
            });
        }
    }
    Ok(items)
}

fn contains_script(v: &Value) -> bool {
    match v {
        Value::Object(m) => {
            m.get("action").and_then(Value::as_str) == Some("run_script")
                || m.values().any(contains_script)
        }
        Value::Array(a) => a.iter().any(contains_script),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}
pub(super) fn check_references(
    v: &Value,
    captured: &BTreeSet<&str>,
    all: &BTreeSet<&str>,
) -> Result<(), String> {
    match v {
        Value::Object(m) => {
            for (k, value) in m {
                if value.as_str().is_some_and(|identity| {
                    !captured.contains(identity)
                        && (k == "target" || k == "value" && all.contains(identity))
                }) {
                    return Err("A binding targets an object outside the captured room; capture a self-contained room".into());
                }
                check_references(value, captured, all)?;
            }
        }
        Value::Array(a) => {
            for value in a {
                check_references(value, captured, all)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}
fn shift(record: &mut Value, dx: f32, dz: f32) -> Result<(), String> {
    let x = number(record, "x", 0.0) + dx;
    let z = number(record, "z", 0.0) + dz;
    let m = record.as_object_mut().ok_or("Invalid prefab record")?;
    drop(m.insert("x".into(), json!(x)));
    drop(m.insert("z".into(), json!(z)));
    if let Some(a) = m
        .get_mut("ceiling_tile_origin")
        .and_then(Value::as_array_mut)
    {
        if let Some(v) = a.first_mut() {
            *v = json!(number(&json!({"n":v}), "n", 0.0) + dx);
        }
        if let Some(v) = a.get_mut(1) {
            *v = json!(number(&json!({"n":v}), "n", 0.0) + dz);
        }
    }
    Ok(())
}
fn replacements(s: &Source, id: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for item in &s.items {
        if let Some(old) = item.record.get("id").and_then(Value::as_str) {
            drop(map.insert(old.into(), format!("{id}_{}", item.key)));
        }
        for field in ["switch_group", "light_switch"] {
            if let Some(old) = item.record.get(field).and_then(Value::as_str)
                && !map.contains_key(old)
            {
                drop(map.insert(
                    old.into(),
                    format!(
                                "{id}_g{}",
                                crate::engine::package::hash::sha256_hex(old.as_bytes())
                                    .chars()
                                    .take(16)
                                    .collect::<String>()
                            ),
                ));
            }
        }
    }
    map
}
fn remap(v: &mut Value, map: &BTreeMap<String, String>) {
    match v {
        Value::String(s) => {
            if let Some(new) = map.get(s) {
                *s = new.clone();
            }
        }
        Value::Object(m) => {
            for (k, value) in m {
                if !matches!(
                    k.as_str(),
                    "model" | "fixture" | "material" | "display_name" | "prompt"
                ) {
                    remap(value, map);
                }
            }
        }
        Value::Array(a) => {
            for value in a {
                remap(value, map);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
fn instantiated(
    s: &Source,
    id: &str,
    destination: [f32; 2],
    linked: bool,
) -> Result<Vec<Item>, String> {
    let map = replacements(s, id);
    let mut items = s.items.clone();
    for item in &mut items {
        shift(
            &mut item.record,
            destination[0] - s.origin[0],
            destination[1] - s.origin[1],
        )?;
        remap(&mut item.record, &map);
        if linked {
            drop(
                item.record
                    .as_object_mut()
                    .ok_or("Invalid member")?
                    .insert(MEMBER.into(), json!({"instance":id,"key":item.key})),
            );
        }
    }
    Ok(items)
}
fn disjoint(doc: &Document, items: &[Item], ignore: Option<&str>) -> Result<(), String> {
    let room = items
        .iter()
        .find(|i| i.collection == "rooms")
        .ok_or("Prefab has no room")?;
    let r: crate::engine::level::RoomDef =
        serde_json::from_value(room.record.clone()).map_err(|e| e.to_string())?;
    for (i, other) in doc.level().rooms.iter().enumerate() {
        if ignore.is_some_and(|id| {
            array(doc.raw(), "rooms")
                .get(i)
                .and_then(|v| v.pointer("/editor_prefab/instance"))
                .and_then(Value::as_str)
                == Some(id)
        }) {
            continue;
        }
        if r.x < other.x + other.width - 0.001
            && other.x < r.x + r.width - 0.001
            && r.z < other.z + other.depth - 0.001
            && other.z < r.z + r.depth - 0.001
            && r.floor_y < other.eave_y()
            && other.floor_y < r.eave_y()
        {
            return Err("Stamp overlaps an existing room/storey. Choose an empty destination; adjoining edges are allowed.".into());
        }
    }
    Ok(())
}
pub(super) fn stamp(
    doc: &Document,
    name: &str,
    destination: [f32; 2],
    linked: bool,
) -> Result<(Value, String, Option<ObjectId>), String> {
    let s = source(doc.raw(), name)?;
    let mut raw = doc.raw().clone();
    if table(&mut raw, "instances")?.len() >= 128 {
        return Err("This map already has 128 linked instances".into());
    }
    let id = format!("pf{}", next(&mut raw)?);
    let items = instantiated(&s, &id, destination, linked)?;
    disjoint(doc, &items, None)?;
    let room_index = array(&raw, "rooms").len();
    for item in &items {
        objects::array_mut(&mut raw, &item.collection)?.push(item.record.clone());
    }
    if items.iter().any(|i| i.record.get("switch_group").is_some()) {
        drop(
            raw.as_object_mut()
                .ok_or("Invalid level")?
                .insert("requires_full_lighting".into(), json!(true)),
        );
    }
    if linked {
        drop(
            table(&mut raw, "instances")?.insert(
                id,
                serde_json::to_value(Instance {
                    source: name.into(),
                    destination,
                    baseline: items.clone(),
                })
                .map_err(|e| e.to_string())?,
            ),
        );
    }
    Ok((
        raw,
        format!(
            "Stamp {name}: {} members, {}. Select its room to reuse or update.",
            items.len(),
            if linked { "linked" } else { "independent" }
        ),
        Some(ObjectId::Room(room_index)),
    ))
}
fn selected_instance(doc: &Document, selected: ObjectId) -> Result<(String, Instance), String> {
    let (key, i) = selected.collection();
    let id = array(doc.raw(), key)
        .get(i)
        .and_then(|v| v.pointer("/editor_prefab/instance"))
        .and_then(Value::as_str)
        .ok_or("Select a linked stamp member. Independent copies have no update link.")?;
    let v = doc
        .raw()
        .pointer("/editor_reuse/instances")
        .and_then(|v| v.get(id))
        .ok_or("Instance link metadata is missing; existing geometry was preserved")?;
    let instance: Instance = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok((id.into(), instance))
}
fn member<'a>(raw: &'a Value, collection: &str, id: &str, key: &str) -> Option<(usize, &'a Value)> {
    array(raw, collection).iter().enumerate().find(|(_, v)| {
        v.pointer("/editor_prefab/instance").and_then(Value::as_str) == Some(id)
            && v.pointer("/editor_prefab/key").and_then(Value::as_str) == Some(key)
    })
}
// Three-way object merge. Arrays are one coherent override (openings/bindings).
// Missing fields, deleted members, and explicit nulls are all meaningful.
fn merge(base: Option<&Value>, current: Option<&Value>, new: Option<&Value>) -> Option<Value> {
    if current == base {
        return new.cloned();
    }
    if let (Some(Value::Object(b)), Some(Value::Object(c)), Some(Value::Object(n))) =
        (base, current, new)
    {
        let keys: BTreeSet<_> = b.keys().chain(c.keys()).chain(n.keys()).collect();
        let mut result = serde_json::Map::new();
        for k in keys {
            if let Some(v) = merge(b.get(k), c.get(k), n.get(k)) {
                drop(result.insert(k.clone(), v));
            }
        }
        Some(Value::Object(result))
    } else {
        current.cloned()
    }
}
fn ensure_unique_members(raw: &Value, id: &str) -> Result<(), String> {
    let mut identities = BTreeSet::new();
    for collection in COLLECTIONS {
        for record in array(raw, collection) {
            if record
                .pointer("/editor_prefab/instance")
                .and_then(Value::as_str)
                == Some(id)
            {
                let key = record
                    .pointer("/editor_prefab/key")
                    .and_then(Value::as_str)
                    .ok_or("Linked member has no identity")?;
                if !identities.insert((collection, key)) {
                    return Err("Duplicate linked member identities. Detach the instance before updating; geometry was preserved.".into());
                }
            }
        }
    }
    Ok(())
}
fn ensure_carrier_override(doc: &Document, id: &str, instance: &Instance) -> Result<(), String> {
    let base = instance
        .baseline
        .iter()
        .find(|i| i.collection == "rooms")
        .ok_or("Missing prefab room baseline")?;
    let (_index, current) = member(doc.raw(), "rooms", id, &base.key)
        .ok_or("This instance's carrier was deleted; detach remaining members instead")?;
    let normalized = |v: &Value| -> Result<Value, String> {
        let room: crate::engine::level::RoomDef =
            serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        serde_json::to_value(room).map_err(|e| e.to_string())
    };
    let old = normalized(&base.record)?;
    let now = normalized(current)?;
    if [
        "x", "z", "width", "depth", "height", "floor_y", "ceiling", "tunnels",
    ]
    .iter()
    .any(|k| old.get(*k) != now.get(*k))
    {
        return Err("This linked room has a carrier geometry override. Restore its room geometry or detach before updating; surface and furnishing overrides remain supported.".into());
    }
    Ok(())
}

pub(super) fn update(
    doc: &Document,
    selected: ObjectId,
) -> Result<(Value, String, Option<ObjectId>), String> {
    let (id, instance) = selected_instance(doc, selected)?;
    ensure_unique_members(doc.raw(), &id)?;
    ensure_carrier_override(doc, &id, &instance)?;
    let s = source(doc.raw(), &instance.source)?;
    let fresh = instantiated(&s, &id, instance.destination, true)?;
    let mut raw = doc.raw().clone();
    let mut overrides = 0_usize;
    for key in COLLECTIONS {
        let previous: Vec<_> = instance
            .baseline
            .iter()
            .filter(|i| i.collection == key)
            .collect();
        for old in previous {
            let next_item = fresh
                .iter()
                .find(|i| i.key == old.key && i.collection == key);
            let current = member(&raw, key, &id, &old.key).map(|(i, v)| (i, v.clone()));
            let Some((i, record)) = current else {
                overrides = overrides.saturating_add(1);
                continue;
            };
            if record != old.record {
                overrides = overrides.saturating_add(1);
            }
            let result = merge(
                Some(&old.record),
                Some(&record),
                next_item.map(|n| &n.record),
            );
            let a = objects::array_mut(&mut raw, key)?;
            if let Some(mut v) = result {
                if next_item.is_none() {
                    drop(
                        v.as_object_mut()
                            .ok_or("Invalid retained override")?
                            .remove(MEMBER),
                    );
                }
                *a.get_mut(i).ok_or("Member disappeared")? = v;
            } else {
                drop(a.remove(i));
            }
        }
        for new in fresh.iter().filter(|n| {
            n.collection == key
                && !instance
                    .baseline
                    .iter()
                    .any(|i| i.key == n.key && i.collection == key)
        }) {
            objects::array_mut(&mut raw, key)?.push(new.record.clone());
        }
    }
    // Keep deleted-member tombstones in the baseline until the instance detaches.
    let mut baseline = fresh;
    for old in &instance.baseline {
        if !baseline
            .iter()
            .any(|i| i.key == old.key && i.collection == old.collection)
        {
            baseline.push(old.clone());
        }
    }
    let room = baseline
        .iter()
        .find(|i| i.collection == "rooms")
        .ok_or("Missing room baseline")?;
    let current_room = member(&raw, "rooms", &id, &room.key)
        .ok_or("This instance's carrier was deleted; detach remaining members instead")?;
    let room_index = current_room.0;
    let current_items = vec![Item {
        collection: "rooms".into(),
        key: room.key.clone(),
        record: current_room.1.clone(),
    }];
    disjoint(doc, &current_items, Some(&id))?;
    drop(
        table(&mut raw, "instances")?.insert(
            id,
            serde_json::to_value(Instance {
                source: instance.source,
                destination: instance.destination,
                baseline,
            })
            .map_err(|e| e.to_string())?,
        ),
    );
    Ok((
        raw,
        format!(
            "Linked update: {overrides} overridden/deleted members preserved. Arrays remain coherent overrides."
        ),
        Some(ObjectId::Room(room_index)),
    ))
}
pub(super) fn detach(
    doc: &Document,
    selected: ObjectId,
) -> Result<(Value, String, Option<ObjectId>), String> {
    let (id, _instance) = selected_instance(doc, selected)?;
    let mut raw = doc.raw().clone();
    for key in COLLECTIONS {
        for record in objects::array_mut(&mut raw, key)? {
            if record
                .pointer("/editor_prefab/instance")
                .and_then(Value::as_str)
                == Some(id.as_str())
            {
                drop(
                    record
                        .as_object_mut()
                        .ok_or("Invalid member")?
                        .remove(MEMBER),
                );
            }
        }
    }
    drop(table(&mut raw, "instances")?.remove(&id));
    Ok((
        raw,
        "Detach link: current geometry and all overrides remain editable.".into(),
        None,
    ))
}
