//! Short-lived scoped capabilities. Retaining `api` in a Luau closure does not
//! retain a Rust world reference: mlua invalidates its functions after the call.

use mlua::{FromLuaMulti, Function, IntoLuaMulti, Lua, Scope, Table, Value};
use std::cell::Cell;
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

// Counts keep Luau's original integer precision instead of coordinate f32 rounding.
struct BloodCount(u8);
impl mlua::FromLua for BloodCount {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        let count = match value {
            Value::Integer(number) => u8::try_from(number).ok(),
            Value::Number(number)
                if number.is_finite()
                    && (0.0_f64..=12.0_f64).contains(&number)
                    && number.fract() == 0.0_f64 =>
            {
                number.abs().to_string().parse::<u8>().ok()
            }
            Value::Nil
            | Value::Boolean(_)
            | Value::LightUserData(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Table(_)
            | Value::Function(_)
            | Value::Thread(_)
            | Value::UserData(_)
            | Value::Vector(_)
            | Value::Buffer(_)
            | Value::Error(_)
            | Value::Other(_) => None,
        };
        count
            .filter(|bounded| *bounded <= 12)
            .map(Self)
            .ok_or_else(|| error("Blood droplets require an actual numeric integer in 0..12"))
    }
}

struct Coordinate(f32);
impl mlua::FromLua for Coordinate {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        // mlua's ordinary float conversion also parses numeric strings. Host
        // calls accept actual bounded numbers, never a large string-to-number
        // conversion whose native cost precedes the capability's work guard.
        let number = if let Value::Number(number) = value {
            number
        } else if let Value::Integer(number) = value {
            number
                .to_string()
                .parse()
                .map_err(|_problem| error("Invalid coordinate"))?
        } else {
            return Err(error("Coordinate requires a number"));
        };
        if !number.is_finite() || number.abs() > 100_000.0_f64 {
            return Err(error("Coordinate requires a finite number within ±100000"));
        }
        Ok(Self(
            number
                .to_string()
                .parse()
                .map_err(|_problem| error("Invalid coordinate"))?,
        ))
    }
}

use super::{MAX_NATIVE_CALLS, error};
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

/// Queries also consume a hard native-work budget: they do not enqueue actions,
/// and a tight read-only loop must not rely solely on cooperative VM interrupts.
fn capability<'scope, A, R, F>(
    scope: &'scope Scope<'scope, '_>,
    calls: &'scope Cell<usize>,
    callback: F,
) -> mlua::Result<Function>
where
    A: FromLuaMulti,
    R: IntoLuaMulti,
    F: Fn(&Lua, A) -> mlua::Result<R> + 'scope,
{
    scope.create_function(move |vm, arguments| {
        let used = calls.get();
        if used >= MAX_NATIVE_CALLS {
            return Err(error("Host native-call budget exhausted"));
        }
        calls.set(used.saturating_add(1));
        callback(vm, arguments)
    })
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
    calls: &'scope Cell<usize>,
    completion: &'scope Cell<bool>,
    limit: usize,
) -> mlua::Result<Table> {
    let api = lua.create_table()?;
    api.set(
        "completed",
        capability(scope, calls, move |_, value: HostString| {
            let id = text(value, 96)?;
            world
                .campaign
                .lock()
                .map(|progress| progress.completed(&id))
                .map_err(|_problem| error("Campaign state unavailable"))
        })?,
    )?;
    api.set(
        "complete",
        capability(scope, calls, move |_, ()| {
            super::campaign::validate_id(&world.campaign_level).map_err(error)?;
            completion.set(true);
            Ok(())
        })?,
    )?;
    api.set(
        "message",
        capability(
            scope,
            calls,
            move |_, (value, lines): (HostString, Table)| {
                if lines.metatable().is_some() || lines.raw_len() == 0 || lines.raw_len() > 6 {
                    return Err(error("Guidance requires 1..=6 plain text lines"));
                }
                let count = lines.raw_len();
                let mut keys = 0usize;
                for pair in lines.pairs::<Value, Value>().take(7) {
                    let (key, _) = pair?;
                    let Value::Integer(index) = key else {
                        return Err(error("Guidance lines must be a dense array"));
                    };
                    if index < 1 || !usize::try_from(index).is_ok_and(|ordinal| ordinal <= count) {
                        return Err(error("Guidance lines must be a dense array"));
                    }
                    keys = keys.saturating_add(1);
                }
                if keys != count {
                    return Err(error("Guidance lines must be a dense array"));
                }
                let title = text(value, 80)?;
                let mut content = Vec::new();
                for line in lines.sequence_values::<HostString>() {
                    content.push(text(line?, 72)?);
                }
                crate::engine::level::validate_message(&title, &content).map_err(error)?;
                push(
                    queue,
                    limit,
                    ActionDef::ShowMessage {
                        title,
                        lines: content,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "player",
        capability(scope, calls, move |vm, ()| {
            let result = vm.create_table()?;
            result.set("x", world.script_player.x)?;
            result.set("y", world.script_player.y)?;
            result.set("z", world.script_player.z)?;
            result.set("form", world.script_form.clone())?;
            result.set_readonly(true);
            Ok(result)
        })?,
    )?;
    api.set(
        "held_item",
        capability(scope, calls, move |_, ()| {
            Ok(world.script_held_item.clone())
        })?,
    )?;
    for (name, retiring) in [("handoff", false), ("retire_hidden", true)] {
        api.set(
            name,
            capability(scope, calls, move |_, value: HostString| {
                let id = entity(world, value)?;
                if !world
                    .handle_of(&id)
                    .is_some_and(|h| world.components().carries.contains(h))
                {
                    return Err(error("Target requires an authored carry component"));
                }
                push(
                    queue,
                    limit,
                    if retiring {
                        ActionDef::RetireHidden { target: id }
                    } else {
                        ActionDef::HandoffItem { target: id }
                    },
                )
            })?,
        )?;
    }
    api.set(
        "release_item",
        capability(scope, calls, move |_, value: HostString| {
            let id = entity(world, value)?;
            if world
                .handle_of(&id)
                .and_then(|handle| world.components().carries.get(handle))
                .is_none()
            {
                return Err(error("Item release requires a carry component"));
            }
            push(queue, limit, ActionDef::ReleaseItem { target: id })
        })?,
    )?;
    api.set(
        "actor",
        capability(scope, calls, move |vm, value: HostString| {
            let id = entity(world, value)?;
            let handle = world.handle_of(&id).ok_or_else(|| error("Unknown actor"))?;
            let pose = world
                .components()
                .transforms
                .get(handle)
                .ok_or_else(|| error("Actor has no transform"))?;
            let response = world.components().light_responses.get(handle);
            let result = vm.create_table()?;
            result.set("x", pose.position.x)?;
            result.set("y", pose.position.y)?;
            result.set("z", pose.position.z)?;
            let body = world.components().nav_agents.get(handle);
            result.set("radius", body.map(|agent| agent.radius))?;
            result.set("height", body.map(|agent| agent.height))?;
            let physical = world
                .script_physical
                .iter()
                .find(|(physical_handle, _, _, _)| *physical_handle == handle);
            result.set("physical", physical.is_some())?;
            result.set("up_y", physical.map(|(_, up_y, _, _)| *up_y))?;
            result.set("held", physical.is_some_and(|(_, _, held, _)| *held))?;
            result.set(
                "sleeping",
                physical.is_some_and(|(_, _, _, sleeping)| *sleeping),
            )?;
            result.set("illuminated", response.is_some_and(|r| r.active))?;
            result.set("intensity", response.map_or(0.0, |r| r.intensity))?;
            result.set("player_visible", world.script_visible.contains(&handle))?;
            let carry = world.components().carries.get(handle);
            result.set(
                "carried_item",
                carry.and_then(|ownership| ownership.item.as_deref()),
            )?;
            result.set("retired", carry.is_some_and(|ownership| ownership.retired))?;
            if let Some(agent) = world.ai().agent(&id) {
                result.set(
                    "ai_phase",
                    agent
                        .controller
                        .active
                        .and_then(|token| {
                            agent.controller.runs.iter().find(|run| run.token == token)
                        })
                        .map(|run| run.progress.status),
                )?;
                result.set("ai_owner", agent.controller.owner.name())?;
                result.set("ai_result", agent.controller.last_result.clone())?;
                result.set(
                    "ai_task",
                    agent
                        .controller
                        .active
                        .and_then(|token| {
                            agent.controller.runs.iter().find(|run| run.token == token)
                        })
                        .map(|run| run.request.key.clone()),
                )?;
            }
            result.set(
                "shadow_only",
                world
                    .components()
                    .renderables
                    .get(handle)
                    .is_some_and(|renderable| renderable.shadow_only),
            )?;
            result.set_readonly(true);
            Ok(result)
        })?,
    )?;
    api.set(
        "toggle_animation",
        capability(
            scope,
            calls,
            move |_, (value, clip): (HostString, HostString)| {
                let id = entity(world, value)?;
                let name = text(clip, 96)?;
                if world
                    .handle_of(&id)
                    .and_then(|handle| world.components().animations.get(handle))
                    .is_none()
                {
                    return Err(error("Animation requires an animation component"));
                }
                push(
                    queue,
                    limit,
                    ActionDef::ToggleAnimation {
                        target: Some(id),
                        clip: Some(name),
                    },
                )
            },
        )?,
    )?;
    api.set(
        "animate",
        capability(
            scope,
            calls,
            move |_, (value, clip, looped): (HostString, HostString, Flag)| {
                let id = entity(world, value)?;
                let name = text(clip, 96)?;
                if world
                    .handle_of(&id)
                    .and_then(|handle| world.components().animations.get(handle))
                    .is_none()
                {
                    return Err(error("Animation requires an animation component"));
                }
                push(
                    queue,
                    limit,
                    ActionDef::PlayAnimation {
                        target: Some(id),
                        clip: Some(name),
                        looped: looped.0,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "ai_pause",
        capability(
            scope,
            calls,
            move |_, (raw_target, raw_key, paused): (HostString, HostString, Flag)| {
                push(
                    queue,
                    limit,
                    ActionDef::AiPause {
                        target: entity(world, raw_target)?,
                        key: text(raw_key, 96)?,
                        paused: paused.0,
                    },
                )
            },
        )?,
    )?;
    api.set("ai_senses",capability(scope,calls,move |_,(raw_target,payload):(HostString,Value)|{
        let rules:Vec<crate::engine::ai::authoring::SenseRule>=if matches!(&payload,Value::Table(table) if table.is_empty()){Vec::new()}else{super::task_value::decode(payload)?};
        if rules.len()>16 || !rules.iter().all(crate::engine::ai::authoring::SenseRule::is_valid){return Err(error("Invalid bounded perception rules"));}
        push(queue,limit,ActionDef::AiSenses{target:entity(world,raw_target)?,rules})
    })?)?;
    api.set(
        "ai_task",
        capability(
            scope,
            calls,
            move |_, (raw_target, raw_key, payload): (HostString, HostString, Option<Value>)| {
                let target = entity(world, raw_target)?;
                let key = text(raw_key, 96)?;
                let request = payload
                    .map(super::task_value::decode::<crate::engine::ai::authoring::TaskRequest>)
                    .transpose()?
                    .map(Box::new);
                if request
                    .as_deref()
                    .is_some_and(|submitted| submitted.key != key || !submitted.is_valid())
                {
                    return Err(error("Invalid bounded shared AI request"));
                }
                push(
                    queue,
                    limit,
                    ActionDef::AiTask {
                        target,
                        key,
                        request,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "navigate",
        capability(
            scope,
            calls,
            move |_,
                  (value, x, y, z, speed): (
                HostString,
                Coordinate,
                Coordinate,
                Coordinate,
                Option<Coordinate>,
            )| {
                let id = entity(world, value)?;
                let speed_mps = speed.map(|requested| requested.0);
                if speed_mps.is_some_and(|requested| !(0.05..=8.0).contains(&requested)) {
                    return Err(error("Navigation speed requires 0.05..=8 m/s"));
                }
                let position = [x.0, y.0, z.0];
                if world
                    .handle_of(&id)
                    .and_then(|h| world.components().ais.get(h))
                    .is_none()
                {
                    return Err(error(
                        "Navigation requires an AI body and a finite bounded goal",
                    ));
                }
                push(
                    queue,
                    limit,
                    ActionDef::Navigate {
                        target: id,
                        position,
                        speed_mps,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "finish",
        capability(
            scope,
            calls,
            move |_, (won, raw_title, raw_detail): (Flag, HostString, HostString)| {
                let title = text(raw_title, 80)?;
                let detail = text(raw_detail, 160)?;
                crate::engine::level::validate_run_feedback(&title, &detail).map_err(error)?;
                push(
                    queue,
                    limit,
                    ActionDef::EndRun {
                        won: won.0,
                        title,
                        detail,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "change_level",
        capability(scope, calls, move |_, value: HostString| {
            let level = text(value, 96)?;
            if !world.permits_transition(&level) {
                return Err(error("Undeclared project transition"));
            }
            push(queue, limit, ActionDef::ChangeLevel { level })
        })?,
    )?;
    api.set(
        "reset",
        capability(scope, calls, move |_, ()| {
            push(queue, limit, ActionDef::ResetToStart)
        })?,
    )?;
    api.set(
        "exists",
        capability(scope, calls, move |_, value: HostString| {
            Ok(world.handle_of(&text(value, 96)?).is_some())
        })?,
    )?;
    api.set(
        "enable",
        capability(
            scope,
            calls,
            move |_, (value, enabled): (HostString, Flag)| {
                let id = entity(world, value)?;
                push(
                    queue,
                    limit,
                    if enabled.0 {
                        ActionDef::Enable { target: Some(id) }
                    } else {
                        ActionDef::Disable { target: Some(id) }
                    },
                )
            },
        )?,
    )?;
    api.set(
        "door_status",
        capability(scope, calls, move |vm, value: HostString| {
            let id = entity(world, value)?;
            let door = world
                .doors()
                .index_of(&id)
                .and_then(|index| world.doors().get(index))
                .ok_or_else(|| error("Target is not a hinged door"))?;
            let result = vm.create_table()?;
            result.set("phase", door.phase().name())?;
            result.set("locked", door.is_locked())?;
            result.set("obstructed", door.obstructed)?;
            result.set("angle", door.angle())?;
            result.set_readonly(true);
            Ok(result)
        })?,
    )?;
    api.set(
        "prompt",
        capability(
            scope,
            calls,
            move |_, (value, prompt): (HostString, HostString)| {
                let id = entity(world, value)?;
                if world
                    .handle_of(&id)
                    .and_then(|handle| world.components().interactables.get(handle))
                    .is_none()
                {
                    return Err(error("Prompt requires an interactable entity"));
                }
                push(
                    queue,
                    limit,
                    ActionDef::SetPrompt {
                        target: Some(id),
                        prompt: text(prompt, 96)?,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "door",
        capability(
            scope,
            calls,
            move |_, (value, operation): (HostString, HostString)| {
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
            },
        )?,
    )?;
    api.set(
        "light",
        capability(scope, calls, move |_, (value, on): (HostString, Flag)| {
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
        capability(
            scope,
            calls,
            move |_, (value, playing): (HostString, Flag)| {
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
            },
        )?,
    )?;
    api.set(
        "blood",
        capability(
            scope,
            calls,
            move |_,
                  (x, y, z, dx, dy, dz, droplets): (
                Coordinate,
                Coordinate,
                Coordinate,
                Coordinate,
                Coordinate,
                Coordinate,
                Option<BloodCount>,
            )| {
                let emission = crate::engine::render::BloodEmission {
                    origin: [x.0, y.0, z.0],
                    direction: [dx.0, dy.0, dz.0],
                    droplets: droplets.map_or(8, |count| count.0),
                };
                emission.validate().map_err(error)?;
                push(queue, limit, ActionDef::Blood { emission })
            },
        )?,
    )?;
    api.set(
        "become",
        capability(
            scope,
            calls,
            move |_, (raw_model, raw_scale): (Option<HostString>, Option<Coordinate>)| {
                let model = raw_model.map(|value| text(value, 96)).transpose()?;
                let scale = raw_scale.map_or(1.0, |value| value.0);
                if !(0.1..=4.0).contains(&scale)
                    || model
                        .as_ref()
                        .is_some_and(|id| !crate::game::representation::is_eligible(id))
                {
                    return Err(error("Form requires an eligible model and scale 0.1..=4"));
                }
                push(queue, limit, ActionDef::PlayerForm { model, scale })
            },
        )?,
    )?;
    api.set(
        "ragdoll",
        capability(
            scope,
            calls,
            move |_,
                  (value, enabled, x, y, z): (
                HostString,
                Flag,
                Option<Coordinate>,
                Option<Coordinate>,
                Option<Coordinate>,
            )| {
                let id = entity(world, value)?;
                let velocity = [
                    x.map_or(0.0, |coordinate| coordinate.0),
                    y.map_or(0.0, |coordinate| coordinate.0),
                    z.map_or(0.0, |coordinate| coordinate.0),
                ];
                let speed = glam::Vec3::from_array(velocity);
                if !speed.is_finite() || speed.length() > 15.0 {
                    return Err(error("Ragdoll velocity must be finite and at most 15 m/s"));
                }
                push(
                    queue,
                    limit,
                    ActionDef::Ragdoll {
                        target: Some(id),
                        enabled: enabled.0,
                        velocity,
                    },
                )
            },
        )?,
    )?;
    api.set(
        "physics",
        capability(
            scope,
            calls,
            move |_, (value, activate): (HostString, Flag)| {
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
            },
        )?,
    )?;
    api.set(
        "spawn",
        capability(
            scope,
            calls,
            move |_, (value, raw_name): (HostString, Option<HostString>)| {
                let point = text(value, 96)?;
                let name = raw_name.map(|requested| text(requested, 96)).transpose()?;
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
                        name: name.clone(),
                    },
                )?;
                Ok(name)
            },
        )?,
    )?;
    api.set(
        "despawn",
        capability(scope, calls, move |_, value: HostString| {
            let id = text(value, 96)?;
            if !world.is_runtime_spawn(&id) {
                return Err(error("Despawn requires a live runtime spawn"));
            }
            push(queue, limit, ActionDef::DespawnEntity { target: id })
        })?,
    )?;
    api.set(
        "state",
        capability(
            scope,
            calls,
            move |vm, (value, key): (HostString, HostString)| {
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
                    StateValue::Int(_) => {
                        Err(error("Declared integer is outside the scripting range"))
                    }
                    StateValue::Float(v) => Ok(Value::Number(f64::from(*v))),
                    StateValue::Text(v) => Ok(Value::String(vm.create_string(v)?)),
                }
            },
        )?,
    )?;
    api.set(
        "set_state",
        capability(
            scope,
            calls,
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
        capability(scope, calls, move |_, value: HostString| {
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
