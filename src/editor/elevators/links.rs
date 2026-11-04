//! Transactional source edits for independent groups of linked cabins.
use super::{Document, ElevatorDef, title};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn lifts(raw: &mut Value) -> Result<&mut Vec<Value>, String> {
    super::super::document::record_array_mut(raw, "elevators")
}
fn encode(lift: &ElevatorDef) -> Result<Value, String> {
    serde_json::to_value(lift).map_err(|e| e.to_string())
}
pub(super) fn place(doc: &Document, at: [f32; 3], rotation: f32) -> Result<Value, String> {
    if doc.level().elevators.len() >= 8 {
        return Err("At most eight elevators per level".into());
    }
    let candidate = ElevatorDef {
        id: String::new(),
        x: at[0],
        z: at[2],
        rotation_degrees: rotation,
        initial_stop: 0,
        speed_mps: 1.5,
        stops: Vec::new(),
    };
    let bounds = footprint(&candidate, at[1], at[1]);
    if doc.level().elevators.iter().any(|other| {
        let low = other
            .stops
            .iter()
            .filter(|s| s.is_landing())
            .map(|s| s.y)
            .fold(f32::INFINITY, f32::min);
        let high = other
            .stops
            .iter()
            .filter(|s| s.is_landing())
            .map(|s| s.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let occupied = footprint(other, low, high);
        bounds.iter().zip(occupied).all(|(range, other_range)| {
            range.0 < other_range.1 + 0.01 && range.1 > other_range.0 - 0.01
        })
    }) {
        return Err("Cabins overlap. Move this elevator clear of the other cabin or shaft.".into());
    }
    let mut number = 1_usize;
    let id = loop {
        let id = format!("lift_{number}");
        if doc.level().elevators.iter().all(|lift| lift.id != id) {
            break id;
        }
        number = number.saturating_add(1);
    };
    let mut raw = doc.raw().clone();
    lifts(&mut raw)?.push(json!({"id":id,"x":at[0],"z":at[2],"rotation_degrees":rotation,"stops":[{"label":format!("Elevator {number}"),"y":at[1]}]}));
    Ok(raw)
}
pub(super) fn connect(doc: &Document, first: usize, second: usize) -> Result<Value, String> {
    let all = &doc.level().elevators;
    let source = all.get(first).ok_or("Select the first elevator")?;
    let target = all.get(second).ok_or("Select the second elevator")?;
    if first == second {
        return Err("Choose a different elevator to link".into());
    }
    let mut group = BTreeSet::from([source.id.clone(), target.id.clone()]);
    loop {
        let before = group.len();
        for lift in all {
            for stop in &lift.stops {
                if stop.level.is_none()
                    && let Some(id) = &stop.arrival_elevator
                    && (group.contains(&lift.id) || group.contains(id))
                {
                    let _a = group.insert(lift.id.clone());
                    let _b = group.insert(id.clone());
                }
            }
        }
        if group.len() == before {
            break;
        }
    }
    if group.len() > crate::engine::elevator::MAX_LINKED_STOPS {
        return Err("A group supports three stops for now (two destination buttons per cabin). Keep independent groups separate.".into());
    }
    let mut raw = doc.raw().clone();
    for (index, lift) in all
        .iter()
        .enumerate()
        .filter(|(_, lift)| group.contains(&lift.id))
    {
        let mut updated = lift.clone();
        if lift.stops.iter().filter(|stop| stop.is_landing()).count() == 1 {
            let landing = lift
                .stops
                .get(lift.initial_stop)
                .cloned()
                .ok_or("Missing landing")?;
            updated.stops.clear();
            for arrival in all.iter().filter(|other| group.contains(&other.id)) {
                if arrival.id == lift.id {
                    updated.initial_stop = updated.stops.len();
                    updated.stops.push(landing.clone());
                } else {
                    updated.stops.push(crate::engine::elevator::Stop {
                        label: title(arrival).into(),
                        y: arrival
                            .stops
                            .get(arrival.initial_stop)
                            .ok_or("Missing landing")?
                            .y,
                        level: None,
                        arrival_elevator: Some(arrival.id.clone()),
                    });
                }
            }
            updated.stops.extend(
                lift.stops
                    .iter()
                    .filter(|stop| stop.level.is_some())
                    .cloned(),
            );
        } else {
            for arrival in all
                .iter()
                .filter(|other| other.id != lift.id && group.contains(&other.id))
            {
                if updated.stops.iter().any(|stop| {
                    stop.level.is_none() && stop.arrival_elevator.as_deref() == Some(&arrival.id)
                }) {
                    continue;
                }
                updated.stops.push(crate::engine::elevator::Stop {
                    label: title(arrival).into(),
                    y: arrival
                        .stops
                        .get(arrival.initial_stop)
                        .ok_or("Missing landing")?
                        .y,
                    level: None,
                    arrival_elevator: Some(arrival.id.clone()),
                });
            }
        }
        if updated.stops.len() > crate::engine::elevator::MAX_LINKED_STOPS {
            return Err("Each cabin supports two destination buttons for now. This group or level link would exceed that limit.".into());
        }
        *lifts(&mut raw)?.get_mut(index).ok_or("Elevator removed")? = encode(&updated)?;
    }
    Ok(raw)
}
pub(super) fn external(
    doc: &Document,
    index: usize,
    level: &str,
    target: &ElevatorDef,
) -> Result<Value, String> {
    let mut lift = doc
        .level()
        .elevators
        .get(index)
        .cloned()
        .ok_or("Select an elevator")?;
    if lift.stops.iter().any(|s| {
        s.level.as_deref() == Some(level) && s.arrival_elevator.as_deref() == Some(&target.id)
    }) {
        return Err("That destination is already linked".into());
    }
    if lift.stops.len() >= crate::engine::elevator::MAX_LINKED_STOPS {
        return Err("Each cabin supports two destination buttons for now. This level link would exceed that limit.".into());
    }
    lift.stops.push(crate::engine::elevator::Stop {
        label: title(target).into(),
        y: 0.0,
        level: Some(level.into()),
        arrival_elevator: Some(target.id.clone()),
    });
    let mut raw = doc.raw().clone();
    *lifts(&mut raw)?.get_mut(index).ok_or("Elevator removed")? = encode(&lift)?;
    let object = raw.as_object_mut().ok_or("Invalid level")?;
    let links = object
        .entry("transitions")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Invalid transitions")?;
    if !links.iter().any(|id| id.as_str() == Some(level)) {
        links.push(Value::String(level.into()));
    }
    Ok(raw)
}
pub(super) fn rename(doc: &Document, index: usize, name: &str) -> Result<Value, String> {
    let text = name.trim();
    if text.is_empty() || text.len() > 48 || text.chars().any(char::is_control) {
        return Err("Use a name of 1..48 printable bytes".into());
    }
    let mut updated = doc
        .level()
        .elevators
        .get(index)
        .cloned()
        .ok_or("Select an elevator")?;
    updated
        .stops
        .get_mut(updated.initial_stop)
        .ok_or("Missing local landing")?
        .label = text.into();
    let mut raw = doc.raw().clone();
    *lifts(&mut raw)?.get_mut(index).ok_or("Elevator removed")? = encode(&updated)?;
    for (i, lift) in doc
        .level()
        .elevators
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
    {
        let mut other = lift.clone();
        for stop in &mut other.stops {
            if stop.level.is_none() && stop.arrival_elevator.as_deref() == Some(&updated.id) {
                stop.label = text.into();
            }
        }
        *lifts(&mut raw)?.get_mut(i).ok_or("Elevator removed")? = encode(&other)?;
    }
    Ok(raw)
}

fn footprint(def: &ElevatorDef, low: f32, high: f32) -> [(f32, f32); 3] {
    let centre = def.point(0.0, 0.0, 1.155);
    let (sin, cos) = def.rotation_degrees.to_radians().sin_cos();
    let x = cos.abs().mul_add(1.45, sin.abs() * 1.325);
    let z = sin.abs().mul_add(1.45, cos.abs() * 1.325);
    [
        (centre.x - x, centre.x + x),
        (low - 0.08, high + 2.64),
        (centre.z - z, centre.z + z),
    ]
}

pub(super) fn remove(doc: &Document, index: usize) -> Result<Value, String> {
    let removed = doc
        .level()
        .elevators
        .get(index)
        .ok_or("Select an elevator")?;
    for id in doc.level_ids().filter(|id| *id != doc.level_id()) {
        let (_, _, level) = doc.project().read_level(Some(id))?;
        if level.elevators.iter().any(|lift| {
            lift.stops.iter().any(|stop| {
                stop.level.as_deref() == Some(doc.level_id())
                    && stop.arrival_elevator.as_deref().unwrap_or(&lift.id) == removed.id
            })
        }) {
            return Err(format!(
                "{id} still loads into this elevator. Change that destination before deleting it."
            ));
        }
    }
    let mut raw = doc.raw().clone();
    let mut retained = Vec::new();
    for lift in doc
        .level()
        .elevators
        .iter()
        .filter(|lift| lift.id != removed.id)
    {
        let mut changed = lift.clone();
        let mut stops = Vec::new();
        for (stop_index, stop) in lift.stops.iter().enumerate() {
            if stop.level.is_none() && stop.arrival_elevator.as_deref() == Some(&removed.id) {
                continue;
            }
            if stop_index == lift.initial_stop {
                changed.initial_stop = stops.len();
            }
            stops.push(stop.clone());
        }
        changed.stops = stops;
        retained.push(encode(&changed)?);
    }
    *lifts(&mut raw)? = retained;
    Ok(raw)
}
