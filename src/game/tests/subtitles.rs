//! Essential regression: consecutive speech was dropped by the modal panel.
//! Tiny strings and queue state verify order, retained long text, timed pages,
//! repeat suppression and reset; no assets, renderer, sleeps or map bake.
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
    for _ in 0..pages {
        speech.advance();
    }
    assert_eq!(
        speech.queue.front().map(|s| s.speaker.as_str()),
        Some("Second")
    );
    for _ in 0_u16..100 {
        speech.tick(0.1);
    }
    assert!(!speech.active());
    speech.push("Again".into(), &["Repeated visit.".into()]);
    speech.clear();
    assert!(!speech.active());
    Ok(())
}
