//! Bound native standard-library operations whose work is independent of Lua
//! heap size. Sparse negative table indices must not bypass VM interrupts.

use mlua::{Function, Lua, Table, Value};

use super::error;

const MAX_TABLE_MOVE_ELEMENTS: i64 = 8192;

fn index(value: &Value) -> mlua::Result<i32> {
    let invalid = || error("Table move indices require integral 32-bit numbers");
    if let Value::Integer(number) = value {
        i32::try_from(*number).map_err(|_problem| invalid())
    } else if let Value::Number(number) = value {
        // Formatting an actual number is bounded; never coerce arbitrary Lua
        // strings before checking the native work range.
        number.to_string().parse().map_err(|_problem| invalid())
    } else {
        Err(invalid())
    }
}

pub(super) fn bound(lua: &Lua) -> mlua::Result<()> {
    let tables: Table = lua.globals().get("table")?;
    let original: Function = tables.get("move")?;
    tables.set(
        "move",
        lua.create_function(
            move |_,
                  (source, first_value, last_value, destination_value, target): (
                Table,
                Value,
                Value,
                Value,
                Option<Table>,
            )| {
                let first = index(&first_value)?;
                let last = index(&last_value)?;
                let destination = index(&destination_value)?;
                let count = i64::from(last)
                    .saturating_sub(i64::from(first))
                    .saturating_add(1);
                if count > MAX_TABLE_MOVE_ELEMENTS {
                    return Err(error("Table move range exceeds 8192 elements"));
                }
                original.call::<Table>((source, first, last, destination, target))
            },
        )?,
    )
}
