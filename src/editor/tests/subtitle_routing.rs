//! One essential adapter regression: ordinary `ShowMessage` must not freeze Test,
//! whereas explicit hints/outcomes must remain modal. Exercises actual routing,
//! shared subtitle navigation/timing and reset using only tiny in-memory text;
//! no Game, renderer, device, asset, VM or map bake is constructed.
use super::Speech;
use sdl3::keyboard::Keycode;
use serde_json::json;

#[test]
fn ordinary_speech_is_nonmodal_but_requested_hint_and_outcome_pause_and_reset() {
    let mut speech = Speech::default();
    let lines = vec!["A short line.".to_owned()];
    assert!(
        !speech.show("NPC".into(), &lines, false),
        "ordinary speech must not pause Test"
    );
    assert!(
        speech.modal.is_empty(),
        "camera and NPC update must remain reachable"
    );
    speech.tick(0.1);
    let running = speech.subtitles.snapshot();
    assert_eq!(
        running.pointer("/pending"),
        Some(&json!(1_u32)),
        "shared queue receives speech"
    );
    assert!(
        speech.navigate(Keycode::PageDown, true),
        "reading keys work independently of player ownership"
    );
    assert!(
        !speech.subtitles.active(),
        "advance consumes the short page"
    );
    assert!(
        !speech.show("NPC".into(), &lines, false),
        "next ordinary line remains nonmodal"
    );
    speech.subtitles.guidance_requested = Some(());
    assert!(
        speech.show("Hint".into(), &lines, false),
        "explicit F1 response remains modal"
    );
    speech.finish_commands();
    let paused = speech.subtitles.snapshot();
    speech.tick(0.1);
    assert_eq!(
        speech.subtitles.snapshot(),
        paused,
        "modal reading does not spend subtitle time"
    );
    assert!(
        !speech.navigate(Keycode::Backspace, false),
        "modal owns input before subtitle controls"
    );
    speech.dismiss_hint();
    assert!(
        !speech.show("NPC".into(), &lines, false),
        "dismissed hint does not make later dialogue modal"
    );
    assert!(
        speech.show("Finished".into(), &lines, true),
        "outcome is intentionally modal"
    );
    assert!(
        !speech.subtitles.active(),
        "outcome clears independent speech"
    );
    assert!(
        !speech.show("Later failure".into(), &lines, true),
        "first outcome wins within a command batch"
    );
    assert_eq!(
        speech.modal.front().map(|message| message.title.as_str()),
        Some("Finished"),
        "later outcome cannot replace completion"
    );
    speech.clear();
    speech.subtitles.guidance_requested = Some(());
    speech.finish_commands();
    assert!(
        !speech.show("After silent hint".into(), &lines, false),
        "empty hint batch cannot capture later ordinary speech"
    );
    speech.clear();
    assert!(
        speech.modal.is_empty() && !speech.subtitles.active(),
        "Stop/repeat leaves no temporary speech"
    );
    assert!(
        !speech.show("NPC".into(), &lines, false),
        "new Test has no stale hint intent"
    );
}
