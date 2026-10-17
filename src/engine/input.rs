//! Stable names for native input keys shared by application bindings.

use sdl3::keyboard::Keycode;

/// Keys whose binding name is not SDL's own key name.
///
/// Every other key falls back to `Keycode::name()` uppercased; that fallback is
/// the identity written to `settings.json` and the one `is_reserved_key`
/// compares against. The table is explicit (rather than a wildcard match over
/// SDL3's extensible `Keycode` enum) so a future SDL3 key constant can never be
/// mistaken for one of these spellings.
const KEY_NAME_OVERRIDES: [(Keycode, &str); 17] = [
    (Keycode::Grave, "`"),
    (Keycode::Period, "."),
    (Keycode::Minus, "-"),
    (Keycode::Equals, "="),
    (Keycode::Comma, ","),
    (Keycode::Slash, "/"),
    (Keycode::Backslash, "\\"),
    (Keycode::Semicolon, ";"),
    (Keycode::Return, "ENTER"),
    (Keycode::Escape, "ESC"),
    (Keycode::Space, "SPACE"),
    (Keycode::Tab, "TAB"),
    (Keycode::Backspace, "BACKSPACE"),
    (Keycode::Up, "UP"),
    (Keycode::Down, "DOWN"),
    (Keycode::Left, "LEFT"),
    (Keycode::Right, "RIGHT"),
];

/// Converts an SDL Keycode into a normalized string representation.
#[must_use]
pub fn keycode_to_str(key: Keycode) -> String {
    KEY_NAME_OVERRIDES
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map_or_else(
            || key.name().to_uppercase(),
            |(_, name)| (*name).to_string(),
        )
}

/// Resolves persisted/native key names to the identity emitted by input events.
/// Unknown names cannot create a binding that native input will never trigger.
#[must_use]
pub fn canonical_key_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    // Retain the historical keypad-minus spelling from settings files.
    let key = if trimmed.eq_ignore_ascii_case("KP_MINUS") {
        Some(Keycode::KpMinus)
    } else {
        KEY_NAME_OVERRIDES
            .iter()
            .find(|(_, candidate)| candidate.eq_ignore_ascii_case(trimmed))
            .map(|(key, _)| *key)
            .or_else(|| Keycode::from_name(trimmed))
    }?;
    Some(keycode_to_str(key))
}
