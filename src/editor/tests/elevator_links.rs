//! One CPU/source regression protects grouping, label order, refusal atomicity,
//! history and save/reopen without rendering or baking any map.
use super::links;
use crate::editor::document::Document;
use serde_json::json;
#[test]
fn linked_groups_are_height_independent_bounded_and_atomic() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-elevator-links-{}", std::process::id()));
    let mut doc = Document::create(&path, "lift_links", "Lift links")?;
    let expected = doc.raw().clone();
    doc.apply_authoring_edit(&expected,json!({"schema":"places-level","format_version":1_u32,"id":"lift_links","name":"Lift links","spawn":{"x":0.0_f32,"z":-1.0_f32},"rooms":[]}))?;
    for point in [
        [0.0_f32, 0.0, 0.0],
        [8.0, 0.0, 0.0],
        [8.0, 7.0, 0.0],
        [16.0, 0.0, 0.0],
        [16.0, 3.5, 0.0],
    ] {
        let raw = links::place(&doc, point, 0.0)?;
        let before = doc.raw().clone();
        doc.apply_authoring_edit(&before, raw)?;
    }
    for (first, second) in [(0_usize, 1_usize), (1, 2), (3, 4)] {
        let raw = links::connect(&doc, first, second)?;
        let before = doc.raw().clone();
        doc.apply_authoring_edit(&before, raw)?;
    }
    let before = doc.raw().clone();
    assert!(
        links::connect(&doc, 2, 3).is_err(),
        "merging into five stops is refused before mutation"
    );
    assert_eq!(
        doc.raw(),
        &before,
        "refusal preserves both independent groups"
    );
    let renamed = links::rename(&doc, 2, "8 / remote office")?;
    doc.apply_authoring_edit(&before, renamed)?;
    for (i, lift) in doc.level().elevators.iter().take(3).enumerate() {
        assert_eq!(
            lift.initial_stop, i,
            "button order follows placement, independent of height"
        );
        assert_eq!(lift.stops.len(), 3);
        assert_eq!(
            lift.stops.get(2).ok_or("Missing stop")?.label,
            "8 / remote office"
        );
    }
    assert!(
        doc.undo() && doc.raw() == &before,
        "rename and reverse labels undo together"
    );
    assert!(doc.redo());
    doc.save()?;
    let reopened = Document::open(&path)?;
    assert_eq!(
        reopened.raw(),
        doc.raw(),
        "all explicit group links survive source save/reopen"
    );
    assert_eq!(
        reopened
            .level()
            .elevators
            .get(3)
            .ok_or("Missing other group")?
            .stops
            .len(),
        2
    );
    std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    Ok(())
}
