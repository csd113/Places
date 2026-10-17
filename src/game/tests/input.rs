//! One tiny SDL-event regression protects lost short taps, repeat-triggered
//! actions/rebinds and cleared focus/menu state. No assets or native device.
use crate::game::{
    console,
    input::{Control, InputHandler, MenuNavEvent, pressed_key},
    settings::KeyBindings,
};
use sdl3::{
    event::Event,
    keyboard::{Keycode, Mod},
};

fn key_event(key: Keycode, down: bool, repeat: bool) -> Event {
    if down {
        Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            keycode: Some(key),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat,
            which: 0,
            raw: 0,
        }
    } else {
        Event::KeyUp {
            timestamp: 0,
            window_id: 1,
            keycode: Some(key),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat,
            which: 0,
            raw: 0,
        }
    }
}

#[test]
fn taps_repeats_and_screen_resets_share_one_press_contract() {
    let bindings = KeyBindings::default();
    let mut input = InputHandler::new();
    for (key, control) in [
        (Keycode::E, Control::Interact),
        (Keycode::F, Control::Flashlight),
        (Keycode::Space, Control::Jump),
    ] {
        input.handle_gameplay_event(&key_event(key, true, false), &bindings);
        input.handle_gameplay_event(&key_event(key, false, false), &bindings);
        assert!(
            input.state().was_pressed(control),
            "short taps must survive release before a frame"
        );
        assert!(
            !input.state().is_held(control),
            "released taps must not remain held"
        );
        input.state_mut().clear_presses();
        input.handle_gameplay_event(&key_event(key, true, true), &bindings);
        assert!(
            !input.state().was_pressed(control),
            "OS repeat must not fabricate a fresh press"
        );
    }
    let repeated = key_event(Keycode::Return, true, true);
    assert!(
        pressed_key(&repeated).is_none(),
        "a held menu activation must not become a rebind"
    );
    assert!(
        InputHandler::poll_menu_nav_event(&repeated).is_none(),
        "repeat must not navigate"
    );
    assert_eq!(
        InputHandler::poll_menu_nav_event(&key_event(Keycode::Return, true, false)),
        Some(MenuNavEvent::Activate),
        "fresh activation must still work"
    );
    assert!(
        !console::toggle_key(&key_event(Keycode::Grave, true, false), "G"),
        "old console key must release ownership after rebinding"
    );
    assert!(
        console::toggle_key(&key_event(Keycode::G, true, false), "G"),
        "console must use configured key"
    );
    assert!(
        !console::toggle_key(&key_event(Keycode::G, true, true), "G"),
        "console repeats must not toggle"
    );
    input.handle_gameplay_event(&key_event(Keycode::W, true, false), &bindings);
    input.state_mut().accumulate_mouse_motion(13.0, -4.0);
    input.clear_gameplay_inputs();
    assert!(
        !input.state().is_held(Control::MoveForward),
        "focus/menu transitions must release held control"
    );
    assert!(
        !input.state().was_pressed(Control::MoveForward),
        "transitions must discard pending actions"
    );
    assert_eq!(
        input.state_mut().take_mouse_motion(),
        (0.0, 0.0),
        "transitions must discard stale look motion"
    );
}
