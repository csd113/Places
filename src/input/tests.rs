//! Unit tests for the input handler.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::doc_markdown, clippy::expect_used, clippy::indexing_slicing)]

use super::*;
use crate::test_support::assert_exact;
use sdl3::keyboard::Mod;

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

/// One relative mouse-motion event, as SDL3 delivers it while captured.
fn mouse_motion(xrel: f32, yrel: f32) -> Event {
    Event::MouseMotion {
        timestamp: 0,
        window_id: 0,
        which: 0,
        mousestate: sdl3::mouse::MouseState::from_sdl_state(0),
        x: 0.0,
        y: 0.0,
        xrel,
        yrel,
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
    assert_key_drives_only(Keycode::Space, Control::Jump, &bindings);
    assert_key_drives_only(Keycode::C, Control::Crouch, &bindings);
    assert_key_drives_only(Keycode::E, Control::Interact, &bindings);
}

/// `E` is a gameplay binding, not a menu key: it never navigates a menu and
/// engages only Interact while held.
#[test]
fn e_is_a_gameplay_binding_not_a_menu_key() {
    assert_eq!(
        InputHandler::poll_menu_nav_event(&key_down(Keycode::E)),
        None
    );
    let bindings = KeyBindings::default();
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(Keycode::E), &bindings);
    assert!(handler.state().is_held(Control::Interact));
    handler.handle_gameplay_event(&key_up(Keycode::E), &bindings);
    assert!(!handler.state().is_held(Control::Interact));
}

/// `SPACE` is a gameplay binding, not a menu key: holding it in the Playing
/// state jumps while W/S/A/D keep driving both gameplay and the menus.
#[test]
fn space_is_a_gameplay_binding_not_a_menu_key() {
    assert_eq!(
        InputHandler::poll_menu_nav_event(&key_down(Keycode::Space)),
        None
    );
    let bindings = KeyBindings::default();
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(Keycode::W), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::Space), &bindings);
    assert!(handler.state().is_held(Control::MoveForward));
    assert!(handler.state().is_held(Control::Jump));
    handler.handle_gameplay_event(&key_up(Keycode::Space), &bindings);
    assert!(!handler.state().is_held(Control::Jump));
    assert!(handler.state().is_held(Control::MoveForward));
}

#[test]
fn old_non_wasd_keys_do_not_drive_controls() {
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

#[test]
fn mouse_motion_accumulates_and_is_consumed_exactly_once() {
    let bindings = KeyBindings::default();
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&mouse_motion(12.0, -4.5), &bindings);
    handler.handle_gameplay_event(&mouse_motion(3.5, 1.5), &bindings);
    let (dx, dy) = handler.state_mut().take_mouse_motion();
    assert_exact(dx, 15.5);
    assert_exact(dy, -3.0);
    // Consuming clears: a second take sees no motion at all.
    let (dx, dy) = handler.state_mut().take_mouse_motion();
    assert_exact(dx, 0.0);
    assert_exact(dy, 0.0);
}

#[test]
fn non_finite_mouse_motion_is_ignored() {
    let bindings = KeyBindings::default();
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&mouse_motion(f32::NAN, 4.0), &bindings);
    let (dx, dy) = handler.state_mut().take_mouse_motion();
    assert_exact(dx, 0.0);
    assert_exact(dy, 4.0);

    handler.handle_gameplay_event(&mouse_motion(f32::INFINITY, f32::NEG_INFINITY), &bindings);
    let (dx, dy) = handler.state_mut().take_mouse_motion();
    assert_exact(dx, 0.0);
    assert_exact(dy, 0.0);

    // An accumulation that would overflow to infinity is refused as well.
    handler
        .state_mut()
        .accumulate_mouse_motion(f32::MAX, f32::MAX);
    handler
        .state_mut()
        .accumulate_mouse_motion(f32::MAX, f32::MAX);
    let (dx, dy) = handler.state_mut().take_mouse_motion();
    assert!(dx.is_finite() && dy.is_finite(), "{dx} {dy}");
}

#[test]
fn clear_gameplay_inputs_discards_mouse_motion_too() {
    let bindings = KeyBindings::default();
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&mouse_motion(5.0, 5.0), &bindings);
    handler.handle_gameplay_event(&key_down(Keycode::W), &bindings);
    handler.clear_gameplay_inputs();
    assert_eq!(handler.state().held, 0);
    let (dx, dy) = handler.state_mut().take_mouse_motion();
    assert_exact(dx, 0.0);
    assert_exact(dy, 0.0);
}

/// A rebound interact key drives only Interact, OS auto-repeat never re-fires
/// the held bit, and releasing all gameplay inputs (pause, focus loss, level
/// load) drops a held Interact.
#[test]
fn interact_rebinding_repeat_suppression_and_release_all() {
    let mut bindings = KeyBindings::default();
    bindings
        .set_key("interact", "F")
        .expect("F is unused by default");
    assert_key_drives_only(Keycode::F, Control::Interact, &bindings);
    // E is no longer bound to anything after the rebind.
    let mut handler = InputHandler::new();
    handler.handle_gameplay_event(&key_down(Keycode::E), &bindings);
    assert_eq!(handler.state().held, 0, "the old key is unbound");

    handler.handle_gameplay_event(&key_down(Keycode::F), &bindings);
    handler.handle_gameplay_event(&key_repeat(Keycode::F), &bindings);
    assert!(handler.state().is_held(Control::Interact));
    handler.clear_gameplay_inputs();
    assert!(
        !handler.state().is_held(Control::Interact),
        "pause/focus/level-load release clears a held interact"
    );
}

#[test]
fn a_complete_tap_retains_one_press_until_consumed_or_cleared() {
    let bindings = KeyBindings::default();
    for (key, control) in [
        (Keycode::C, Control::Crouch),
        (Keycode::E, Control::Interact),
        (Keycode::Space, Control::Jump),
    ] {
        let mut handler = InputHandler::new();
        handler.handle_gameplay_event(&key_down(key), &bindings);
        handler.handle_gameplay_event(&key_up(key), &bindings);
        assert!(!handler.state().is_held(control));
        assert!(handler.state().was_pressed(control));
        handler.state_mut().clear_presses();
        assert!(!handler.state().was_pressed(control));
        handler.handle_gameplay_event(&key_repeat(key), &bindings);
        assert!(!handler.state().was_pressed(control));
        handler.handle_gameplay_event(&key_down(key), &bindings);
        handler.clear_gameplay_inputs();
        assert!(!handler.state().was_pressed(control));
    }
}

/// The `PLACES_MOVE_SCRIPT` parser accepts the documented grammar and names
/// every rejected entry instead of silently dropping it.
#[test]
fn move_script_parses_ranges_and_rejects_malformed_entries() {
    let (script, rejected) = parse_move_script(
        "forward@0-6.5, jump@0.2-0.6 ,jump@3-3.4,look_right@4-5.5,crouch@0.1-0.2",
    );
    assert!(rejected.is_empty(), "a valid script has no rejections");
    assert_eq!(script.len(), 5);
    assert_eq!(script[0].control, Control::MoveForward);
    assert_exact(script[0].first_seconds, 0.0);
    assert_exact(script[0].last_seconds, 6.5);
    assert_eq!(script[1].control, Control::Jump);
    assert_exact(script[1].last_seconds, 0.6);
    assert_eq!(script[2].control, Control::Jump);
    assert_eq!(script[3].control, Control::LookRight);
    assert_eq!(script[4].control, Control::Crouch);

    let (script, rejected) =
        parse_move_script("nope@1-2,forward,forward@5,x@1-2,forward@-1-2,forward@9-3,forward@a-b");
    assert!(script.is_empty(), "no entry survives: {script:?}");
    assert_eq!(
        rejected,
        [
            "nope@1-2",
            "forward",
            "forward@5",
            "x@1-2",
            "forward@-1-2",
            "forward@9-3",
            "forward@a-b",
        ],
        "every malformed entry is named, in authored order"
    );
}

/// A scripted hold is a real held control with a real rising edge: the jump
/// presses once per range, releases between ranges, and a second range presses
/// again; controls outside the script keep whatever the player holds.
#[test]
fn move_script_holds_controls_with_one_press_edge_per_range() {
    let (script, rejected) = parse_move_script("jump@0.2-0.3,jump@1.0-1.0,forward@0.2-0.4");
    assert!(rejected.is_empty());
    let mut state = InputState::default();
    for seconds in [0.0_f32, 0.1, 0.2, 0.25, 0.3, 0.35, 0.4, 0.5, 1.0] {
        state.apply_move_script(&script, seconds);
        let expected_jump = (0.2..=0.3).contains(&seconds) || (seconds - 1.0).abs() < 1e-6;
        assert_eq!(
            state.is_held(Control::Jump),
            expected_jump,
            "seconds {seconds}"
        );
        assert_eq!(
            state.is_held(Control::MoveForward),
            (0.2..=0.4).contains(&seconds),
            "seconds {seconds}"
        );
        state.clear_presses();
    }
    let mut state = InputState::default();
    state.apply_move_script(&script, 0.2);
    assert!(
        state.was_pressed(Control::Jump),
        "the first held frame presses"
    );
    state.clear_presses();
    state.apply_move_script(&script, 0.25);
    assert!(
        !state.was_pressed(Control::Jump),
        "a held frame never re-presses"
    );
    state.apply_move_script(&script, 0.5);
    assert!(!state.is_held(Control::Jump), "the range releases");
    state.apply_move_script(&script, 1.0);
    assert!(
        state.was_pressed(Control::Jump),
        "a second range presses again"
    );
}

/// A control the script never names is untouched: a real held key survives a
/// scripted frame.
#[test]
fn move_script_leaves_unscripted_controls_alone() {
    let (script, _) = parse_move_script("jump@0-0.1");
    let mut state = InputState::holding(&[Control::MoveForward]);
    state.apply_move_script(&script, 0.05);
    assert!(state.is_held(Control::MoveForward));
    assert!(state.is_held(Control::Jump));
    state.apply_move_script(&script, 0.2);
    assert!(state.is_held(Control::MoveForward));
    assert!(!state.is_held(Control::Jump));
}
