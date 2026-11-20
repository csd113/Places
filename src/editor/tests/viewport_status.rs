use super::{ViewportError, admission_error, clear_admission_error, status_text};

#[test]
fn superseding_edit_hides_its_refusal_and_preserves_test_failure() {
    let mut metrics = serde_json::json!({"admission_error": "Atlas limit", "generation": 7_u64});
    let saved_status = "Saved project";
    assert_eq!(
        status_text(
            saved_status,
            admission_error(&metrics).map(ViewportError::Request)
        ),
        "Viewport: Atlas limit",
        "the current Edit refusal is visible"
    );
    clear_admission_error(&mut metrics);
    assert_eq!(
        status_text(
            saved_status,
            admission_error(&metrics).map(ViewportError::Request)
        ),
        saved_status,
        "a new request retires the old refusal and reveals the prior action status"
    );
    assert_eq!(
        metrics,
        serde_json::json!({"generation": 7_u64}),
        "retiring the refusal preserves the other diagnostics"
    );
    assert_eq!(
        status_text(
            "Test stopped: Device lost",
            Some(ViewportError::Backend("Device lost"))
        ),
        "Test stopped: Device lost",
        "the shared renderer diagnostic must not obscure an explicit Test failure"
    );
    assert_eq!(
        status_text(
            "Test stopped: Atlas limit",
            Some(ViewportError::Request("Atlas limit"))
        ),
        "Viewport: Atlas limit",
        "a fresh Edit refusal can report the same reason as an earlier Test"
    );
}
