//! Regression value: a first numeric cabin must retain the ordinary source
//! admission, atomic history and save/reopen contract without a prior selected
//! elevator. One tiny CPU document; no renderer, asset load or bake.
use super::{Draft, Tool, form_fields};
use crate::editor::document::Document;
use serde_json::{Value, json};

fn draft(doc: &Document, record: Value) -> Draft {
    let fields = form_fields(Tool::Elevator, Some(&record))
        .into_iter()
        .map(|field| {
            let value = record.pointer(&field.path).map_or_else(
                || field.default.into(),
                |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
            );
            (field, value)
        })
        .collect();
    Draft {
        tool: Tool::Elevator,
        owner: None,
        index: 0,
        fields,
        active: None,
        page: 0,
        preview: false,
        slice_z: false,
        environment_before: false,
        native_cursor: 0,
        native_group: "patrol",
        expected: doc.raw().clone(),
        original: Some(record),
    }
}

#[test]
fn first_numeric_cabin_preserves_admission_history_and_reopen() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-numeric-lift-{}", std::process::id()));
    let mut doc = Document::create(&path, "numeric_lift", "Numeric lift")?;
    let mut raw = doc.raw().clone();
    *raw.get_mut("rooms").ok_or("Missing rooms")? =
        json!([{"width":12_i32,"depth":12_i32,"height":3.8_f32}]);
    drop(
        raw.as_object_mut()
            .ok_or("Missing scene")?
            .insert("future".into(), json!({"retained":true})),
    );
    doc.apply_authoring_edit(&doc.raw().clone(), raw.clone())?;
    assert_eq!(doc.level().elevators.len(), 0);
    let record = json!({"id":"first","x":4_i32,"z":4_i32,"rotation_degrees":0_i32,"stops":[{"label":"G","y":0_i32}],"speed_mps":2_i32});
    let form = draft(&doc, record.clone());
    let candidate = form.candidate(&doc)?;
    let _validated = doc.validate_candidate(&candidate)?;
    assert_eq!(doc.raw(), &raw, "Preview/Cancel cannot mutate source");
    doc.apply_authoring_edit(&form.expected, candidate.clone())?;
    assert_eq!(doc.raw().pointer("/future"), raw.get("future"));
    assert_eq!(
        doc.raw().pointer("/elevators/0/speed_mps"),
        record.get("speed_mps")
    );
    assert!(doc.undo());
    assert_eq!(doc.raw(), &raw, "one undo removes the complete first cabin");
    assert!(doc.redo());
    assert_eq!(doc.raw(), &candidate);
    doc.save()?;
    let reopened = Document::open(&path)?;
    assert_eq!(reopened.raw(), &candidate);

    let mut blocked = candidate.clone();
    *blocked.get_mut("walls").ok_or("Missing walls")? =
        json!([{"x":3_i32,"z":5_i32,"width":2_i32,"depth":0.2_f32,"height":3.8_f32}]);
    assert!(
        doc.validate_candidate(&blocked).is_err(),
        "static cabin obstruction is refused"
    );
    blocked = candidate.clone();
    *blocked
        .pointer_mut("/elevators/0/stops/0/y")
        .ok_or("Missing landing")? = json!(2_i32);
    assert!(
        doc.validate_candidate(&blocked).is_err(),
        "ceiling crossing is refused"
    );
    blocked = record;
    blocked
        .get_mut("stops")
        .ok_or("Missing fixture stops")?
        .as_array_mut()
        .ok_or("Missing fixture stops")?
        .push(json!({"label":"Missing","y":0_i32,"level":"missing","arrival_elevator":"absent"}));
    assert!(
        draft(&doc, blocked).candidate(&doc).is_err(),
        "unresolved destination is refused"
    );
    assert_eq!(
        doc.raw(),
        &candidate,
        "failed admission leaves the saved cabin intact"
    );
    std::fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    Ok(())
}
