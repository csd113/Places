//! Essential regression value: a stale package must never be called a successful
//! subprocess build without matching structured success AND exit status.
//!
//! A second tiny byte-stream fixture proves output stays bounded under malformed tools.
//! Both are deterministic in-memory checks; no native process or map bake cost.

use std::path::Path;
use std::sync::Mutex;

use super::child::{Capture, read_stream};
use super::{BuildOutcome, completed_build};
use crate::map_compiler::BuildReport;
use crate::map_compiler::events::{Event, Outcome, PROTOCOL};

#[test]
fn subprocess_result_requires_matching_terminal_event_and_exit_status() -> Result<(), String> {
    let output = Path::new("/chosen/project/levels/entry.placesmap");
    let report = BuildReport {
        source: "project".into(),
        out: output.display().to_string(),
        rebuilt: true,
        reuse_reason: String::new(),
        variants: vec!["off".into()],
        bytes: 1,
        millis: 1.0,
        warnings: Vec::new(),
        fingerprint: "fresh".into(),
        variant_stats: Vec::new(),
        phases: Vec::new(),
        cache_decisions: Vec::new(),
    };
    let mut bytes = serde_json::to_vec(&Event::Started {
        protocol: PROTOCOL,
        source: "project".into(),
        output: output.display().to_string(),
    })
    .map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    let state = Mutex::new(Capture::default());
    read_stream(bytes.as_slice(), true, &state);
    let capture = state.into_inner().map_err(|error| error.to_string())?;
    assert!(
        matches!(
            completed_build(true, output, &capture),
            BuildOutcome::Failed(_)
        ),
        "a successful exit without a result could reuse stale output"
    );
    let success_bytes = serde_json::to_vec(&Event::Finished {
        outcome: Outcome::Success,
        report: Some(Box::new(report)),
        message: None,
    })
    .map_err(|error| error.to_string())?;
    let with_success = Mutex::new(capture);
    read_stream(success_bytes.as_slice(), true, &with_success);
    let mut completed_capture = with_success
        .into_inner()
        .map_err(|error| error.to_string())?;
    assert!(
        matches!(
            completed_build(true, output, &completed_capture),
            BuildOutcome::Succeeded(_)
        ),
        "matching result and exit should succeed"
    );
    assert!(
        matches!(
            completed_build(false, output, &completed_capture),
            BuildOutcome::Failed(_)
        ),
        "a success event cannot override process failure"
    );
    assert!(
        matches!(
            completed_build(true, Path::new("/other.placesmap"), &completed_capture),
            BuildOutcome::Failed(_)
        ),
        "a success event for another output must be refused"
    );
    completed_capture.terminal = Some((Outcome::Cancelled, None, None));
    assert!(
        matches!(
            completed_build(false, output, &completed_capture),
            BuildOutcome::Cancelled
        ),
        "an explicit cancelled result must remain recoverable"
    );
    Ok(())
}

// Prevents equal source filenames in different projects from overwriting an
// unrelated installed map/profile. Three tiny paths, no filesystem cost.
#[test]
fn play_profile_is_stable_per_project_package_and_selected_level() {
    let cache = Path::new("/user/cache");
    let first = Path::new("/projects/first/levels/entry.placesmap");
    let second = Path::new("/projects/second/levels/entry.placesmap");
    let root = super::play_state_root(cache, first, "entry");
    assert!(
        root.starts_with(cache.join("editor-play")),
        "play belongs only in editor cache"
    );
    assert_eq!(
        root,
        super::play_state_root(cache, first, "entry"),
        "repeated play must reuse its isolated profile"
    );
    assert_ne!(
        root,
        super::play_state_root(cache, second, "entry"),
        "projects sharing an output filename must not share installed maps"
    );
    assert_ne!(
        root,
        super::play_state_root(cache, first, "other"),
        "different selected levels need separate profiles"
    );
}

#[test]
fn malformed_feedback_is_bounded_and_invalidates_success() -> Result<(), String> {
    let state = Mutex::new(Capture::default());
    let noisy = vec![b'x'; 80 * 1024];
    read_stream(noisy.as_slice(), false, &state);
    read_stream(b"not-json\n".as_slice(), true, &state);
    let capture = state.into_inner().map_err(|error| error.to_string())?;
    assert!(
        capture.diagnostics.len() <= 16 * 1024,
        "a noisy subprocess must not grow editor memory without limit"
    );
    assert!(
        capture.protocol_error.is_some(),
        "malformed protocol must be reported"
    );
    assert!(
        matches!(
            completed_build(true, Path::new("/map.placesmap"), &capture),
            BuildOutcome::Failed(_)
        ),
        "malformed structured output must never be accepted as a build success"
    );
    Ok(())
}
