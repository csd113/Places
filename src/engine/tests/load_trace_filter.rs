//! One allocation-free fixture guards quiet-player lifecycle evidence while
//! letting expensive ignored world snapshots return before construction.
use super::LoadTrace;

#[test]
fn quiet_trace_keeps_lifecycle_evidence_and_rejects_unconsumed_snapshots() {
    let trace = LoadTrace {
        started: std::time::Instant::now(),
        output: None,
    };
    for event in [
        "world_committed",
        "runtime_context",
        "load_cancelled",
        "load_recovered",
    ] {
        assert!(
            trace.will_record(event),
            "persistent lifecycle evidence must remain enabled"
        );
    }
    for event in ["world_unloaded", "item_picked_up", "flashlight_toggled"] {
        assert!(
            !trace.will_record(event),
            "an unconsumed detailed snapshot must be skipped in a quiet run"
        );
    }
}
