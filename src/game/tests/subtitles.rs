//! Essential regression: consecutive speech was dropped by the modal panel.
//! Tiny strings and queue state verify order, retained long text, timed pages,
//! repeat suppression and reset. Zero-delta pause, cinematic-only forward controls and the
//! minimum reading time protect cinematic interruption without a real clock;
//! no assets, renderer, sleeps or map bake.
use super::*;
#[test]
fn speech_retains_pages_and_consecutive_messages_without_blocking_gameplay() -> Result<(), String> {
    let mut speech = Subtitles::default();
    let lines = vec!["word ".repeat(60), "é".repeat(120)];
    speech.push("First".into(), &lines);
    speech.push("First".into(), &lines);
    speech.push("Second".into(), &["Still here.".into()]);
    assert_eq!(speech.queue.len(), 2);
    let first = speech.queue.front().ok_or("missing first speech")?;
    assert!(first.rows.iter().all(|s| s.chars().count() <= 54));
    assert_eq!(
        first
            .rows
            .concat()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
        lines
            .concat()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    );
    let pages = first.rows.len().div_ceil(2);
    speech.tick(0.1);
    let paused = speech.snapshot();
    for _ in 0_u8..10 {
        speech.tick(0.0);
    }
    assert_eq!(
        speech.snapshot(),
        paused,
        "pause preserves the reading clock"
    );
    assert!(
        !speech.navigate(sdl3::keyboard::Keycode::Escape, false),
        "ordinary Esc must remain available to pause or leave Test"
    );
    assert!(speech.navigate(sdl3::keyboard::Keycode::Return, true));
    speech.tick(0.1);
    let cinematic_page = speech.snapshot();
    for key in [
        sdl3::keyboard::Keycode::PageUp,
        sdl3::keyboard::Keycode::Backspace,
        sdl3::keyboard::Keycode::Escape,
    ] {
        assert!(!speech.navigate(key, true));
    }
    assert_eq!(
        speech.snapshot(),
        cinematic_page,
        "only forward changes cinematic dialogue"
    );
    assert!(speech.navigate(sdl3::keyboard::Keycode::PageUp, false));
    let reread = speech.queue.front().ok_or("missing reread speech")?;
    assert_eq!(reread.page, 0, "previous returns to the retained page");
    assert!(
        reread.elapsed.abs() < f32::EPSILON,
        "rereading gets fresh time"
    );
    for _ in 0..pages {
        speech.advance();
    }
    assert_eq!(
        speech.queue.front().map(|s| s.speaker.as_str()),
        Some("Second")
    );
    for _ in 0_u8..34 {
        speech.tick(0.1);
    }
    assert!(speech.active(), "short speech gets at least 3.5 seconds");
    for _ in 0_u8..2 {
        speech.tick(0.1);
    }
    assert!(!speech.active());
    speech.push("Again".into(), &["Repeated visit.".into()]);
    speech.clear();
    assert!(!speech.active());
    Ok(())
}
