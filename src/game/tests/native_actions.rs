//! The longer recording script must retain both its count bound and validation
//! of late actions. This pure parser fixture needs no window, assets or map bake.

#[test]
fn recording_input_remains_bounded_and_validated() {
    let step = r#"{"after":"start","delay_ms":16,"action":{"kind":"view","yaw":180,"pitch":0}}"#;
    let bounded = format!("[{}]", vec![step; super::MAX_ACTIONS].join(","));
    assert!(super::parse(bounded.as_bytes()).is_ok());
    let oversized = format!("[{}]", vec![step; super::MAX_ACTIONS + 1].join(","));
    assert!(super::parse(oversized.as_bytes()).is_err());
    let mut invalid_steps = vec![step; super::MAX_ACTIONS - 1];
    invalid_steps
        .push(r#"{"after":"start","delay_ms":16,"action":{"kind":"view","yaw":180,"pitch":86}}"#);
    let invalid = format!("[{}]", invalid_steps.join(","));
    assert!(super::parse(invalid.as_bytes()).is_err());
}
