//! Short-lived scoped capabilities. Retaining `api` in a Luau closure does not
//! retain a Rust world reference: mlua invalidates its functions after the call.

use mlua::{Lua, Scope, Table, Value};
use std::collections::BTreeSet;
use std::sync::Mutex;

struct HostString(mlua::LuaString);
impl mlua::FromLua for HostString {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        if let Value::String(string) = value {
            Ok(Self(string))
        } else {
            Err(error("Expected a string"))
        }
    }
}
struct Flag(bool);
impl mlua::FromLua for Flag {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        if let Value::Boolean(flag) = value {
            Ok(Self(flag))
        } else {
            Err(error("Expected a boolean"))
        }
    }
}

use super::error;
use crate::engine::entities::EntityWorld;
use crate::engine::entities::components::StateValue;
use crate::engine::level::ActionDef;

type Queue = Mutex<Vec<ActionDef>>;

fn text(value: HostString, maximum: usize) -> mlua::Result<String> {
    let string = value.0;
    if string.as_bytes().len() > maximum {
        return Err(error("Host string exceeds its byte limit"));
    }
    let content = string.to_str()?;
    if content.is_empty() || content.chars().any(char::is_control) {
        return Err(error("Host string must be nonempty printable UTF-8"));
    }
    Ok(content.to_string())
}

fn entity(world: &EntityWorld, value: HostString) -> mlua::Result<String> {
    let id = text(value, 96)?;
    if world.handle_of(&id).is_none() {
        return Err(error(format!("Unknown live entity '{id}'")));
    }
    Ok(id)
}

fn push(queue: &Queue, limit: usize, action: ActionDef) -> mlua::Result<()> {
    let mut actions = queue
        .lock()
        .map_err(|_problem| error("Host queue unavailable"))?;
    if actions.len() >= limit {
        return Err(error("Host action budget exhausted"));
    }
    actions.push(action);
    drop(actions);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "One explicit capability table keeps the sandbox surface auditable; arguments are borrowed bounded world/queue views"
)]
pub(super) fn host<'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, '_>,
    world: &'scope EntityWorld,
    queue: &'scope Queue,
    notices: &'scope Mutex<Vec<String>>,
    points: &'scope BTreeSet<String>,
    groups: &'scope BTreeSet<String>,
    limit: usize,
) -> mlua::Result<Table> {
    let api = lua.create_table()?;
    api.set(
        "exists",
        scope.create_function(move |_, value: HostString| {
            Ok(world.handle_of(&text(value, 96)?).is_some())
        })?,
    )?;
    api.set(
        "door",
        scope.create_function(move |_, (value, operation): (HostString, HostString)| {
            let id = entity(world, value)?;
            if world.doors().index_of(&id).is_none() {
                return Err(error("Target is not a door"));
            }
            let target = Some(id);
            let action = match text(operation, 16)?.as_str() {
                "open" => ActionDef::Open { target },
                "close" => ActionDef::Close { target },
                "toggle" => ActionDef::Toggle { target },
                "lock" => ActionDef::Lock { target },
                "unlock" => ActionDef::Unlock { target },
                _ => {
                    return Err(error(
                        "Door operation requires open, close, toggle, lock or unlock",
                    ));
                }
            };
            push(queue, limit, action)
        })?,
    )?;
    api.set(
        "light",
        scope.create_function(move |_, (value, on): (HostString, Flag)| {
            let id = entity(world, value)?;
            if world
                .handle_of(&id)
                .and_then(|h| world.components().lights.get(h))
                .is_none_or(|light| !light.switchable)
            {
                return Err(error("Target is not a controllable light"));
            }
            push(
                queue,
                limit,
                ActionDef::SetLight {
                    target: Some(id),
                    on: on.0,
                },
            )
        })?,
    )?;
    api.set(
        "sound",
        scope.create_function(move |_, (value, playing): (HostString, Flag)| {
            let id = entity(world, value)?;
            let emitter = world
                .handle_of(&id)
                .and_then(|h| world.components().audio.get(h))
                .ok_or_else(|| error("Target is not a sound emitter"))?;
            push(
                queue,
                limit,
                if playing.0 {
                    ActionDef::PlaySound {
                        target: Some(id),
                        sound: None,
                        looped: emitter.looped,
                    }
                } else {
                    ActionDef::StopSound { target: Some(id) }
                },
            )
        })?,
    )?;
    api.set(
        "physics",
        scope.create_function(move |_, (value, activate): (HostString, Flag)| {
            let group = text(value, 96)?;
            if !groups.contains(&group) {
                return Err(error("Unknown authored physics group"));
            }
            push(
                queue,
                limit,
                if activate.0 {
                    ActionDef::ActivatePhysicsGroup { group }
                } else {
                    ActionDef::ResetPhysicsGroup { group }
                },
            )
        })?,
    )?;
    api.set(
        "spawn",
        scope.create_function(move |_, value: HostString| {
            let point = text(value, 96)?;
            if !points.contains(&point) {
                return Err(error("Unknown spawn point"));
            }
            push(
                queue,
                limit,
                ActionDef::SpawnEntity {
                    point: Some(point),
                    template: None,
                    group: None,
                    name: None,
                },
            )
        })?,
    )?;
    api.set(
        "state",
        scope.create_function(move |vm, (value, key): (HostString, HostString)| {
            let id = entity(world, value)?;
            let name = text(key, 96)?;
            let state = world
                .handle_of(&id)
                .and_then(|h| world.components().states.get(h))
                .and_then(|s| s.get(&name))
                .ok_or_else(|| error("Unknown declared state"))?;
            match state {
                StateValue::Bool(v) => Ok(Value::Boolean(*v)),
                StateValue::Int(v) if (-2_147_483_647..=2_147_483_647).contains(v) => {
                    Ok(Value::Number(
                        v.to_string()
                            .parse::<f64>()
                            .map_err(|_problem| error("Invalid state number"))?,
                    ))
                }
                StateValue::Int(_) => Err(error("Declared integer is outside the scripting range")),
                StateValue::Float(v) => Ok(Value::Number(f64::from(*v))),
                StateValue::Text(v) => Ok(Value::String(vm.create_string(v)?)),
            }
        })?,
    )?;
    api.set(
        "set_state",
        scope.create_function(
            move |_, (value, key, next): (HostString, HostString, Value)| {
                let id = entity(world, value)?;
                let name = text(key, 96)?;
                let previous = world
                    .handle_of(&id)
                    .and_then(|h| world.components().states.get(h))
                    .and_then(|s| s.get(&name))
                    .ok_or_else(|| error("State must be declared in the level"))?;
                let state_value = match (previous, next) {
                    (StateValue::Bool(_), Value::Boolean(v)) => StateValue::Bool(v),
                    (StateValue::Text(_), Value::String(v)) => {
                        StateValue::Text(text(HostString(v), 256)?)
                    }
                    (StateValue::Int(_), Value::Integer(v))
                        if (-2_147_483_647..=2_147_483_647).contains(&v) =>
                    {
                        StateValue::Int(v)
                    }
                    (StateValue::Int(_), Value::Number(v))
                        if v.is_finite()
                            && v.fract().abs() < f64::EPSILON
                            && v.abs() <= 2_147_483_647.0_f64 =>
                    {
                        StateValue::Int(
                            v.to_string()
                                .parse()
                                .map_err(|_problem| error("Invalid integer"))?,
                        )
                    }
                    (StateValue::Float(_), Value::Number(v))
                        if v.is_finite() && v.abs() <= 1_000_000.0_f64 =>
                    {
                        StateValue::Float(
                            v.to_string()
                                .parse()
                                .map_err(|_problem| error("Invalid number"))?,
                        )
                    }
                    _ => {
                        return Err(error(
                            "State value has wrong type or is outside supported finite range",
                        ));
                    }
                };
                push(
                    queue,
                    limit,
                    ActionDef::SetState {
                        target: Some(id),
                        name,
                        value: state_value,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "notice",
        scope.create_function(move |_, value: HostString| {
            let message = text(value, 256)?;
            let mut messages = notices
                .lock()
                .map_err(|_problem| error("Host diagnostics unavailable"))?;
            if messages.len() >= 4 {
                return Err(error("Host diagnostic budget exhausted"));
            }
            messages.push(message);
            drop(messages);
            Ok(())
        })?,
    )?;
    api.set_readonly(true);
    Ok(api)
}
