//! Small runtime-parser regressions: early count refusal, nested typed and raw
//! JSON allocation admission, and precision/default preservation; no map bake.
use super::*;

#[test]
fn semantic_count_refuses_before_invalid_elements_are_constructed() {
    let text = format!("{{\"props\":[{}]}}", vec!["null"; 100_001].join(","));
    let error = parse(&text).err().unwrap_or_default();
    assert!(error.contains("too many props"), "{error}");
}

#[test]
fn bounded_adapter_preserves_nested_values_and_level_defaults() -> Result<(), String> {
    let text = r#"{"schema":"places-level","format_version":1,"id":"small","name":"small","spawn":{"x":0.123456789,"z":0},"props":[],"transitions":["other"]}"#;
    let expected = LevelDef::from_json(text).map_err(|e| e.to_string())?;
    let actual = parse(text)?;
    assert_eq!(
        serde_json::to_value(actual).map_err(|e| e.to_string())?,
        serde_json::to_value(expected).map_err(|e| e.to_string())?
    );
    let action = r#"{"action":"show_message","title":"Title","lines":["one","two"]}"#;
    assert_eq!(
        serde_json::to_value(decode::<crate::engine::level::ActionDef>(action)?)
            .map_err(|e| e.to_string())?,
        serde_json::from_str::<serde_json::Value>(action).map_err(|e| e.to_string())?
    );
    let nested =
        r#"{"nested":[[1,2],{"escaped":"a\\b\"c","values":[true,null,3.141592653589793]}]}"#;
    assert_eq!(
        decode::<serde_json::Value>(nested)?,
        serde_json::from_str::<serde_json::Value>(nested).map_err(|e| e.to_string())?
    );
    Ok(())
}
