//! Typed form parsing and raw JSON addressing, shared by all capability sections.

use super::{Field, Kind, Target};
use serde_json::{Map, Value};

pub(super) fn read(source: Option<&Value>, field: Field) -> Value {
    let Some(object) = source else {
        return field.fallback();
    };
    let record = match field.target {
        Target::Identity
        | Target::Object
        | Target::Map
        | Target::Spawn
        | Target::Defaults
        | Target::Presentation
        | Target::Fog => Some(object),
        Target::NewState => None,
        Target::Physics => object.get("physics"),
        Target::Filter => object.get("filter"),
        Target::Component(kind) => {
            object
                .get("components")
                .and_then(Value::as_array)
                .and_then(|entries| {
                    entries
                        .iter()
                        .find(|entry| entry.get("component").and_then(Value::as_str) == Some(kind))
                })
        }
        Target::State(index) => object
            .get("components")
            .and_then(|entries| entries.get(index)),
        Target::Light(index) => object.get("lights").and_then(|entries| entries.get(index)),
        Target::Binding(index) => object
            .get("bindings")
            .and_then(|entries| entries.get(index)),
        Target::Condition(binding, index) => object
            .get("bindings")
            .and_then(|entries| entries.get(binding))
            .and_then(|entry| entry.get("when"))
            .and_then(|entries| entries.get(index)),
    };
    let value = record
        .and_then(|entry| entry.get(field.key))
        .cloned()
        .unwrap_or_else(|| field.fallback());
    if field.target == Target::Filter && field.key == "held" {
        return value.as_bool().map_or_else(
            || "any".into(),
            |held| if held { "held".into() } else { "unheld".into() },
        );
    }
    value
}

pub(super) fn parse(field: Field, text: &str) -> Result<Value, String> {
    match field.kind {
        Kind::Text => Ok(text.into()),
        Kind::Number => {
            let number: f32 = text.parse().map_err(|_error| "Enter a finite number")?;
            if !number.is_finite() {
                return Err("Enter a finite number".into());
            }
            Ok(Value::from(number))
        }
        Kind::Bool => text
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|_error| "Choose true or false".into()),
        Kind::Choice(choices) => {
            if !choices.contains(&text) {
                return Err("Choose a listed option".into());
            }
            if field.target == Target::Filter && field.key == "held" {
                return Ok(match text {
                    "held" => Value::Bool(true),
                    "unheld" => Value::Bool(false),
                    _ => Value::Null,
                });
            }
            Ok(text.into())
        }
        Kind::Vector => {
            let vector: [f32; 3] =
                serde_json::from_str(text).map_err(|_error| "Enter three numbers: [x, y, z]")?;
            if vector.iter().any(|value| !value.is_finite()) {
                return Err("Vector must contain finite numbers".into());
            }
            serde_json::to_value(vector).map_err(|error| error.to_string())
        }
        Kind::State => {
            let state: crate::engine::entities::components::StateValue = serde_json::from_str(text)
                .map_err(|_error| "Enter true, false, an integer, a decimal or quoted text")?;
            serde_json::to_value(state).map_err(|error| error.to_string())
        }
    }
}

pub(super) fn record_mut(
    object: &mut Map<String, Value>,
    target: Target,
) -> Result<&mut Map<String, Value>, String> {
    let record = match target {
        Target::Identity
        | Target::Object
        | Target::Map
        | Target::Spawn
        | Target::Defaults
        | Target::Presentation
        | Target::Fog => return Ok(object),
        Target::NewState => None,
        Target::Physics => object.get_mut("physics"),
        Target::Filter => object.get_mut("filter"),
        Target::Component(kind) => object
            .get_mut("components")
            .and_then(Value::as_array_mut)
            .and_then(|entries| {
                entries
                    .iter_mut()
                    .find(|entry| entry.get("component").and_then(Value::as_str) == Some(kind))
            }),
        Target::State(index) => object
            .get_mut("components")
            .and_then(|entries| entries.get_mut(index)),
        Target::Light(index) => object
            .get_mut("lights")
            .and_then(|entries| entries.get_mut(index)),
        Target::Binding(index) => object
            .get_mut("bindings")
            .and_then(|entries| entries.get_mut(index)),
        Target::Condition(binding, index) => object
            .get_mut("bindings")
            .and_then(|entries| entries.get_mut(binding))
            .and_then(|entry| entry.get_mut("when"))
            .and_then(|entries| entries.get_mut(index)),
    };
    record
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "This capability or event no longer exists; select it again".into())
}
