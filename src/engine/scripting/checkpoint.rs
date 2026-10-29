//! Explicit scalar closure state; no bytecode, callbacks, handles or API replay.
use super::{ScriptRuntime, deadline, diagnostic};
use mlua::{Function, Table, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    modules: BTreeMap<String, (bool, bool, ScriptValues)>,
}

// Luau numbers are doubles. Keep their precision instead of narrowing them to
// the engine's f32 entity-state component representation.
type ScriptValues = BTreeMap<String, Scalar>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Scalar {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

fn state_value(value: &Scalar) -> Result<(), String> {
    match value {
        Scalar::Bool(_) => Ok(()),
        Scalar::Int(number) if number.unsigned_abs() <= 2_147_483_647 => Ok(()),
        Scalar::Float(number) if number.is_finite() && number.abs() <= 1_000_000.0 => Ok(()),
        Scalar::Text(text) if text.len() <= 256 && !text.chars().any(char::is_control) => Ok(()),
        Scalar::Int(_) | Scalar::Float(_) | Scalar::Text(_) => {
            Err("Checkpoint script scalar exceeds its supported range".into())
        }
    }
}

impl ScriptRuntime {
    /// # Errors
    /// Every module must opt into both hooks, even if it has no mutable locals.
    pub fn checkpoint(&self) -> Result<Checkpoint, String> {
        if self.sources.len() != self.modules.len() {
            return Err("Checkpoint cannot retain failed script initialization".into());
        }
        let mut modules = BTreeMap::new();
        for module in &self.modules {
            let hook: Function =
                module
                    .exports
                    .raw_get("checkpoint_state")
                    .map_err(|_problem| {
                        format!(
                            "{} needs checkpoint_state and restore_state hooks",
                            module.path.as_str()
                        )
                    })?;
            let _: Function = module
                .exports
                .raw_get("restore_state")
                .map_err(|_problem| format!("{} needs restore_state hook", module.path.as_str()))?;
            deadline(&module.lua, std::time::Duration::from_millis(2));
            let result = hook.call::<Table>(());
            module.lua.remove_interrupt();
            let table = result.map_err(|problem| {
                diagnostic(module.path.as_str(), "checkpoint_state", &problem)
            })?;
            if table.metatable().is_some() {
                return Err("Checkpoint state must be a plain scalar table".into());
            }
            let mut values = ScriptValues::new();
            for entry in table.pairs::<Value, Value>() {
                let (incoming_key, incoming) = entry.map_err(|problem| problem.to_string())?;
                let Value::String(text_key) = incoming_key else {
                    return Err("Checkpoint state keys must be strings".into());
                };
                let key = text_key
                    .to_str()
                    .map_err(|problem| problem.to_string())?
                    .to_string();
                if values.len() >= 128
                    || key.is_empty()
                    || key.len() > 64
                    || key.chars().any(char::is_control)
                {
                    return Err("Checkpoint module state exceeds its key budget".into());
                }
                let scalar = match incoming {
                    Value::Boolean(value) => Scalar::Bool(value),
                    Value::Integer(value) => Scalar::Int(value),
                    Value::Number(value) => Scalar::Float(value),
                    Value::String(value) => Scalar::Text(
                        value
                            .to_str()
                            .map_err(|problem| problem.to_string())?
                            .to_string(),
                    ),
                    Value::Nil
                    | Value::LightUserData(_)
                    | Value::Table(_)
                    | Value::Function(_)
                    | Value::Thread(_)
                    | Value::UserData(_)
                    | Value::Vector(_)
                    | Value::Buffer(_)
                    | Value::Error(_)
                    | Value::Other(_) => {
                        return Err(
                            "Checkpoint module state accepts bool, number and text values only"
                                .into(),
                        );
                    }
                };
                state_value(&scalar)?;
                drop(values.insert(key, scalar));
            }
            drop(modules.insert(
                module.path.as_str().to_string(),
                (module.started, module.disabled, values),
            ));
        }
        Ok(Checkpoint { modules })
    }

    /// Call only on newly initialized VMs in a private world. No gameplay API is supplied.
    /// # Errors
    /// Rejects missing hooks, unknown modules, invalid scalar data or failing restore.
    pub fn restore_checkpoint(&mut self, checkpoint: &Checkpoint) -> Result<(), String> {
        if checkpoint.modules.len() != self.modules.len()
            || self.modules.len() != self.sources.len()
        {
            return Err("Checkpoint script membership mismatch".into());
        }
        for module in &mut self.modules {
            let (started, disabled, values) = checkpoint
                .modules
                .get(module.path.as_str())
                .ok_or("Checkpoint script identity mismatch")?;
            if values.len() > 128 {
                return Err("Checkpoint script key budget exceeded".into());
            }
            let table = module
                .lua
                .create_table()
                .map_err(|problem| problem.to_string())?;
            for (key, scalar) in values {
                if key.is_empty() || key.len() > 64 || key.chars().any(char::is_control) {
                    return Err("Invalid checkpoint script key".into());
                }
                state_value(scalar)?;
                let encoded = match scalar {
                    Scalar::Bool(value) => Value::Boolean(*value),
                    Scalar::Int(value) => Value::Integer(*value),
                    Scalar::Float(value) => Value::Number(*value),
                    Scalar::Text(value) => Value::String(
                        module
                            .lua
                            .create_string(value)
                            .map_err(|problem| problem.to_string())?,
                    ),
                };
                table
                    .raw_set(key.as_str(), encoded)
                    .map_err(|problem| problem.to_string())?;
            }
            table.set_readonly(true);
            let hook: Function = module
                .exports
                .raw_get("restore_state")
                .map_err(|problem| problem.to_string())?;
            deadline(&module.lua, std::time::Duration::from_millis(2));
            let result = hook.call::<()>(table);
            module.lua.remove_interrupt();
            result
                .map_err(|problem| diagnostic(module.path.as_str(), "restore_state", &problem))?;
            module.started = *started;
            module.disabled = *disabled;
        }
        Ok(())
    }
}
