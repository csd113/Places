//! Regression value: complete native draft transactions must preserve unrelated
//! source/history and refuse competing owners. One tiny project, no bake/GPU;
//! ordinary admission loads only the two existing creature rigs.
use super::{Draft, Tool, creatures, form_fields};
use crate::editor::document::Document;
use serde_json::{Value, json};

fn draft(doc: &Document, tool: Tool, index: usize) -> Result<Draft, String> {
    let record = doc
        .raw()
        .get("props")
        .and_then(|v| v.get(index))
        .cloned()
        .ok_or("Missing prop")?;
    let fields = form_fields(tool, Some(&record))
        .into_iter()
        .map(|f| {
            let v = record.pointer(&f.path).map_or_else(
                || f.default.into(),
                |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
            );
            (f, v)
        })
        .collect();
    Ok(Draft {
        owner: None,
        tool,
        index,
        fields,
        native_cursor: 0,
        native_group: "patrol",
        active: None,
        page: 0,
        preview: false,
        slice_z: false,
        environment_before: false,
        expected: doc.raw().clone(),
        original: Some(record),
    })
}
#[test]
fn native_draft_history_roundtrip_and_owner_refusal() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-native-draft-{}", std::process::id()));
    let mut doc = Document::create(&path, "native_draft", "Native draft")?;
    let mut raw = doc.raw().clone();
    drop(raw.as_object_mut().ok_or("object")?.insert("props".into(),serde_json::from_str::<Value>(r#"[{"id":"friend","model":"feltling","x":2,"z":2,"solid":false,"feltling":{"mode":"wait","routes":[{"id":"guide","points":[[2,0,2],[3,0,2]]}],"guide_recovery":{"stop_metres":20,"resume_metres":20}}},{"id":"guest","model":"tall-guest","x":4,"z":4,"solid":false,"tall_guest":{"patrol":[[4,0,4]],"prey_targets":["friend"],"passages":[{"id":"refuge","entry":[1,0,1],"exit":[1,0,2],"crouch_only":true,"protected":{"min":[0,0,0],"max":[2,2,3]}}]}}]"#).map_err(|e|e.to_string())?));
    drop(
        raw.as_object_mut()
            .ok_or("object")?
            .insert("future_editor_metadata".into(), json!({"untouched":"kept"})),
    );
    doc.apply_authoring_edit(&doc.raw().clone(), raw.clone())?;
    let mut d = draft(&doc, Tool::Feltling, 0)?;
    let mode = d
        .fields
        .iter_mut()
        .find(|(f, _)| f.path == "/feltling/mode")
        .ok_or("Missing mode")?;
    mode.1 = "guide".into();
    let candidate = d.candidate(&doc)?;
    assert_eq!(doc.raw(), &raw, "draft/preview does not mutate source");
    doc.apply_authoring_edit(&d.expected, candidate.clone())?;
    assert!(doc.undo() && doc.raw() == &raw);
    assert!(doc.redo() && doc.raw() == &candidate);
    doc.save()?;
    let reopened = Document::open(&path)?;
    assert_eq!(reopened.raw(), &candidate);
    let mut actors = crate::engine::feltling::load(reopened.level())?;
    let actor = actors.first_mut().ok_or("Missing native actor")?;
    assert_eq!(actor.state.policy.recovery.stop_metres, 20.0);
    assert_eq!(actor.state.policy.recovery.resume_metres, 20.0);
    actor.state.policy.begun = true;
    actor.state.policy.advance(0.1, 400.0, false);
    assert!(!actor.state.policy.waiting);
    actor.state.policy.advance(0.1, 400.01, false);
    assert!(actor.state.policy.waiting);
    let guest = draft(&reopened, Tool::TallGuest, 1)?;
    assert_eq!(
        guest.candidate(&reopened)?,
        candidate,
        "all patrol/prey/passage/refuge data retained"
    );
    for (key, value) in [
        ("physics", json!({})),
        ("scale", json!(0.5_f32)),
        ("solid", json!(true)),
        ("pitch_degrees", json!(1_f32)),
        (
            "components",
            json!([{"component":"nav_agent","radius":0.4_f32,"height":0.8_f32,"step_height":0.18_f32,"max_slope":2_f32,"speed_mps":1_f32}]),
        ),
    ] {
        let mut bad = candidate.clone();
        drop(
            bad.pointer_mut("/props/0")
                .and_then(Value::as_object_mut)
                .ok_or("prop")?
                .insert(key.into(), value),
        );
        assert!(
            doc.apply_authoring_edit(&candidate, bad).is_err(),
            "must refuse {key}"
        );
        assert_eq!(doc.raw(), &candidate);
    }
    let mut list = draft(&doc, Tool::Feltling, 0)?;
    creatures::edit_list(&doc, &mut list, "add")?;
    assert_eq!(
        list.record()?
            .pointer("/feltling/routes")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(2)
    );
    creatures::edit_list(&doc, &mut list, "point_add")?;
    creatures::edit_list(&doc, &mut list, "up")?;
    assert_eq!(
        doc.raw(),
        &candidate,
        "list drafts retain atomic Apply boundary"
    );
    drop(reopened);
    drop(doc);
    std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    Ok(())
}
