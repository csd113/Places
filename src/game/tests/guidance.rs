use crate::game::ui::{GuidanceNavigation, guidance_navigation};
use sdl3::keyboard::Keycode;
// Prose previously disappeared behind ellipses. A few ASCII/UTF-8 strings
// uniquely protect exact-width rows, long-word splitting and retained content.
// This costs only small string allocations; no renderer or map fixture.
#[test]
fn guidance_wraps_without_truncation_and_retains_dismissal() {
    let lines = vec![
        "A short paragraph.".into(),
        "word ".repeat(50),
        "é".repeat(120),
        String::new(),
    ];
    let wrapped = crate::game::ui::wrap_guidance(&lines);
    assert!(wrapped.iter().all(|line| line.chars().count() <= 54));
    assert_eq!(
        wrapped.first().map(String::as_str),
        Some("A short paragraph.")
    );
    assert_eq!(
        wrapped
            .iter()
            .flat_map(|line| line.chars())
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
        lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    );
    assert!(
        wrapped.len() > 6,
        "long messages need multiple six-line pages"
    );
    assert_eq!(wrapped.last().map(String::as_str), Some(""));
    assert_eq!(
        guidance_navigation(Keycode::Return, 0, wrapped.len()),
        GuidanceNavigation::Dismiss,
        "Beam's existing hint dismissal must not consume a new page instead"
    );
    assert_eq!(
        guidance_navigation(Keycode::PageDown, 0, 12),
        GuidanceNavigation::Page(1)
    );
    assert_eq!(
        guidance_navigation(Keycode::PageDown, 1, 12),
        GuidanceNavigation::Page(1)
    );
    assert_eq!(
        guidance_navigation(Keycode::PageUp, 0, 12),
        GuidanceNavigation::Page(0)
    );
}
