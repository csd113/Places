//! Bounded Luau table conversion for the same serialized task schema.
use mlua::{Error, Value};
pub(super) fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, Error> {
    let json = match value {
        Value::String(raw) => {
            if raw.as_bytes().len() > 16_384 {
                return Err(Error::external("AI request exceeds 16 KiB"));
            }
            serde_json::from_slice(raw.as_bytes().as_ref()).map_err(Error::external)?
        }
        Value::Table(table) => convert(Value::Table(table), 0, &mut 0)?,
        Value::Nil
        | Value::Boolean(_)
        | Value::Integer(_)
        | Value::Number(_)
        | Value::LightUserData(_)
        | Value::Function(_)
        | Value::Thread(_)
        | Value::UserData(_)
        | Value::Vector(_)
        | Value::Buffer(_)
        | Value::Error(_)
        | Value::Other(_) => {
            return Err(Error::external(
                "AI request requires a table or bounded JSON string",
            ));
        }
    };
    serde_json::from_value(json).map_err(Error::external)
}
fn convert(value: Value, depth: u8, nodes: &mut u16) -> Result<serde_json::Value, Error> {
    *nodes = nodes.saturating_add(1);
    if depth > 12 || *nodes > 512 {
        return Err(Error::external(
            "AI table exceeds bounded depth/node budget",
        ));
    }
    match value {
        Value::Boolean(flag) => Ok(serde_json::Value::Bool(flag)),
        Value::Integer(number) => Ok(serde_json::Value::Number(number.into())),
        Value::Number(number) => serde_json::Number::from_f64(number)
            .map(serde_json::Value::Number)
            .ok_or_else(|| Error::external("AI numbers must be finite")),
        Value::String(text) => {
            if text.as_bytes().len() > 96 {
                return Err(Error::external("AI table string exceeds 96 bytes"));
            }
            Ok(serde_json::Value::String(text.to_str()?.to_string()))
        }
        Value::Table(table) => {
            if table.metatable().is_some() {
                return Err(Error::external("AI configuration cannot have a metatable"));
            }
            let length = table.raw_len();
            if length > 32 {
                return Err(Error::external("AI arrays exceed 32 entries"));
            }
            if length > 0 {
                let mut array = Vec::with_capacity(length);
                let mut entries = 0_usize;
                for pair in table.pairs::<Value, Value>() {
                    let (key, _value) = pair?;
                    let Value::Integer(index) = key else {
                        return Err(Error::external("Mixed AI arrays are unsupported"));
                    };
                    if usize::try_from(index)
                        .ok()
                        .is_none_or(|slot| slot == 0 || slot > length)
                    {
                        return Err(Error::external("Sparse AI arrays are unsupported"));
                    }
                    entries = entries.saturating_add(1);
                }
                if entries != length {
                    return Err(Error::external("Sparse AI arrays are unsupported"));
                }
                for index in 1..=length {
                    array.push(convert(
                        table.raw_get(index)?,
                        depth.saturating_add(1),
                        nodes,
                    )?);
                }
                Ok(serde_json::Value::Array(array))
            } else {
                let mut object = serde_json::Map::new();
                for pair in table.pairs::<mlua::LuaString, Value>() {
                    let (key, entry) = pair?;
                    if object.len() >= 32 || key.as_bytes().len() > 64 {
                        return Err(Error::external("AI object exceeds field/key budget"));
                    }
                    let field = key.to_str()?.to_string();
                    let empty_array = matches!(&entry,Value::Table(list) if list.is_empty())
                        && matches!(
                            field.as_str(),
                            "categories" | "objects" | "object_types" | "steps" | "points"
                        );
                    let converted = if empty_array {
                        serde_json::Value::Array(Vec::new())
                    } else {
                        convert(entry, depth.saturating_add(1), nodes)?
                    };
                    let _old = object.insert(field, converted);
                }
                Ok(serde_json::Value::Object(object))
            }
        }
        Value::Nil
        | Value::LightUserData(_)
        | Value::Function(_)
        | Value::Thread(_)
        | Value::UserData(_)
        | Value::Vector(_)
        | Value::Buffer(_)
        | Value::Error(_)
        | Value::Other(_) => Err(Error::external("Unsupported AI configuration value")),
    }
}
