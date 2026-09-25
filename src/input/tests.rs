//! Unit tests for the input handler.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::doc_markdown)]

use super::*;
use sdl3::keyboard::Mod;

const ALL_CONTROLS: [Control; 8] = [
    Control::MoveForward,
    Control::MoveBackward,
    Control::StrafeLeft,
    Control::StrafeRight,
    Control::LookUp,
    Control::LookDown,
    Control::LookLeft,
    Control::LookRight,
];

/// The first `KeyDown` SDL delivers for a key (OS auto-repeat comes later).
fn key_down(key: Keycode) -> Event {
    Event::KeyDown {
        timestamp: 0,
        window_id: 0,
        keycode: Some(key),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
        which: 0,
        raw: 0,
    }
}

fn key_up(key: Keycode) -> Event {
    Event::KeyUp {
        timestamp: 0,
        window_id: 0,
        keycode: Some(key),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
        which: 0,
        raw: 0,
    }
}

/// KeyDown carrying SDL's `repeat: true` flag, as sent while a key is held.
fn key_repeat(key: Keycode) -> Event {
    Event::KeyDown {
        timestamp: 0,
        window_id: 0,
        keycode: Some(key),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: true,
        which: 0,
        raw: 0,
    }
}

/// Presses `key` and asserts it engages exactly `control`, then releases it
/// and asserts no control is left held.
fn assert_key_drives_only(key: Keycode, control: Control, bindings: &KeyBindings) {
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(key), bindings);
    for other in ALL_CONTROLS {
        assert_eq!(
            handler.state().is_held(other),
            other == control,
            "{key:?} engaged {other:?}"
        );
    }
    handler.handle_gameplay_event(&key_up(key), bindings);
    assert_eq!(
        handler.state().held,
        0,
        "{key:?} release left a control held"
    );
}

#[test]
fn test_default_bindings_map_wasd_and_arrows() {
    let bindings = KeyBindings::default();
    assert_key_drives_only(Keycode::W, Control::MoveForward, &bindings);
    assert_key_drives_only(Keycode::S, Control::MoveBackward, &bindings);
    assert_key_drives_only(Keycode::A, Control::StrafeLeft, &bindings);
    assert_key_drives_only(Keycode::D, Control::StrafeRight, &bindings);
    assert_key_drives_only(Keycode::Left, Control::LookLeft, &bindings);
    assert_key_drives_only(Keycode::Right, Control::LookRight, &bindings);
    assert_key_drives_only(Keycode::Up, Control::LookUp, &bindings);
    assert_key_drives_only(Keycode::Down, Control::LookDown, &bindings);
}

#[test]
fn test_legacy_default_keys_no_longer_drive_controls() {
    let bindings = KeyBindings::default();

    // The pre-WASD layout: Z = backward plus O/./K/L for looking.
    for key in [
        Keycode::Z,
        Keycode::O,
        Keycode::K,
        Keycode::L,
        Keycode::Period,
    ] {
        let mut handler = InputHandler::new();
        handler.handle_gameplay_event(&key_down(key), &bindings);
        assert_eq!(
            handler.state().held,
            0,
            "{key:?} must not be a default binding"
        );
    }

    // S is backward now, and it must not also strafe right.
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(Keycode::S), &bindings);
    assert!(handler.state().is_held(Control::MoveBackward));
    assert!(!handler.state().is_held(Control::StrafeRight));
}

#[test]
fn test_held_keys_survive_repeat_events_and_release_individually() {
    let bindings = KeyBindings::default();
    let mut handler = InputHandler::new();

    handler.handle_gameplay_event(&key_down(Keycode::W), &bindings);
    handler.handle_gameplay_event(&key_repeat(Keycode::W), &bindings);
    assert!(handler.state().is_held(Control::MoveForward));

    // Looking with the arrow keys while W stays held.
    handler.handle_gameplay_event(&key_down(Keycode::Right), &bindings);
    assert!(handler.state().is_held(Control::LookRight));

    // Releasing W must not disturb the arrow look.
    handler.handle_gameplay_event(&key_up(Keycode::W), &bindings);
    assert!(!handler.state().is_held(Control::MoveForward));
    assert!(handler.state().is_held(Control::LookRight));

    handler.handle_gameplay_event(&key_up(Keycode::Right), &bindings);
    assert_eq!(handler.state().held, 0);
}

#[test]
fn test_diagonal_movement_combinations() {
    let bindings = KeyBindings::default();

    // W+A then W+D.
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(Keycode::W), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::A), &bindings);
    assert!(handler.state().is_held(Control::MoveForward));
    assert!(handler.state().is_held(Control::StrafeLeft));
    handler.handle_gameplay_event(&key_down(Keycode::D), &bindings);
    assert!(handler.state().is_held(Control::StrafeRight));
    assert!(handler.state().is_held(Control::StrafeLeft));
    handler.handle_gameplay_event(&key_up(Keycode::A), &bindings);
    assert!(!handler.state().is_held(Control::StrafeLeft));
    assert!(handler.state().is_held(Control::MoveForward));
    assert!(handler.state().is_held(Control::StrafeRight));

    // Backward diagonals S+A and S+D.
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(Keycode::S), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::A), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::D), &bindings);
    assert!(handler.state().is_held(Control::MoveBackward));
    assert!(handler.state().is_held(Control::StrafeLeft));
    assert!(handler.state().is_held(Control::StrafeRight));
    assert!(!handler.state().is_held(Control::MoveForward));
    handler.handle_gameplay_event(&key_up(Keycode::S), &bindings);
    assert!(!handler.state().is_held(Control::MoveBackward));
    assert!(handler.state().is_held(Control::StrafeLeft));
    assert!(handler.state().is_held(Control::StrafeRight));
}

#[test]
fn test_simultaneous_movement_and_looking() {
    let mut handler = InputHandler::new();
    let bindings = KeyBindings::default();

    // Press W (forward), Left (look left) and Up (look up) simultaneously.
    handler.handle_gameplay_event(&key_down(Keycode::W), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::Left), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::Up), &bindings);

    assert!(handler.state().is_held(Control::MoveForward));
    assert!(handler.state().is_held(Control::LookLeft));
    assert!(handler.state().is_held(Control::LookUp));
    assert!(!handler.state().is_held(Control::MoveBackward));

    // Releasing W should not stop looking
    handler.handle_gameplay_event(&key_up(Keycode::W), &bindings);
    assert!(!handler.state().is_held(Control::MoveForward));
    assert!(handler.state().is_held(Control::LookLeft));
    assert!(handler.state().is_held(Control::LookUp));

    // Releasing the arrows clears them both.
    handler.handle_gameplay_event(&key_up(Keycode::Left), &bindings);
    handler.handle_gameplay_event(&key_up(Keycode::Up), &bindings);
    assert_eq!(handler.state().held, 0);
}

#[test]
fn test_menu_nav_uses_wasd_and_arrows() {
    for (key, expected) in [
        (Keycode::W, MenuNavEvent::Up),
        (Keycode::Up, MenuNavEvent::Up),
        (Keycode::S, MenuNavEvent::Down),
        (Keycode::Down, MenuNavEvent::Down),
        (Keycode::A, MenuNavEvent::Left),
        (Keycode::Left, MenuNavEvent::Left),
        (Keycode::D, MenuNavEvent::Right),
        (Keycode::Right, MenuNavEvent::Right),
        (Keycode::Return, MenuNavEvent::Activate),
        (Keycode::Escape, MenuNavEvent::Back),
    ] {
        assert_eq!(
            InputHandler::poll_menu_nav_event(&key_down(key)),
            Some(expected),
            "{key:?} menu navigation"
        );
    }

    // The old Z menu key is gone; menus use W/S plus the arrow keys.
    assert_eq!(
        InputHandler::poll_menu_nav_event(&key_down(Keycode::Z)),
        None
    );
    // Key releases never navigate.
    assert_eq!(InputHandler::poll_menu_nav_event(&key_up(Keycode::W)), None);
    // OS auto-repeat does not navigate repeatedly.
    assert_eq!(
        InputHandler::poll_menu_nav_event(&key_repeat(Keycode::W)),
        None
    );
}

#[test]
fn reserved_keys_are_not_gameplay_bindings() {
    // `-` toggles the performance overlay in every app state, so it must never
    // be claimed by a gameplay binding; ESC pauses and cancels rebinds.
    let mut settings = crate::settings::Settings::default();
    for key in ["-", "KP_MINUS", "ESC"] {
        assert!(
            crate::settings::is_reserved_key(key),
            "{key} should be reserved"
        );
    }
    assert!(settings.bindings.set_key("forward", "-").is_err());
    assert!(settings.bindings.set_key("forward", "ESC").is_err());
    assert!(settings.bindings.set_key("forward", "Z").is_ok());
}
