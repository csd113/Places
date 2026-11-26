//! One tiny SDL-event regression protects lost short taps, repeat-triggered
//! actions/rebinds and cleared focus/menu state. No assets or native device.
use crate::game::{
    AppState, console,
    input::{Control, InputHandler, MenuNavEvent, pressed_control, pressed_key},
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
    assert_eq!(
        pressed_key(&key_event(Keycode::F1, true, false)),
        Some(Keycode::F1),
        "a fresh F1 press requests guidance"
    );
    assert!(
        pressed_key(&key_event(Keycode::F1, true, true)).is_none(),
        "a held F1 key cannot request another hint"
    );
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

/// Synthetic SDL events cover the binding lookup used by both respawn paths
/// when SDL Return and the saved binding have different names. No native setup.
#[test]
fn encounter_respawn_uses_the_active_interact_binding() -> Result<(), String> {
    let mut bindings = KeyBindings::default();
    let enter = key_event(Keycode::Return, true, false);
    let default = key_event(Keycode::E, true, false);
    assert_eq!(
        pressed_control(&default, &bindings),
        Some(Control::Interact)
    );
    assert_eq!(pressed_control(&enter, &bindings), None);

    bindings.set_key("interact", "ENTER")?;
    assert_eq!(pressed_control(&enter, &bindings), Some(Control::Interact));
    for event in [
        default,
        key_event(Keycode::Q, true, false),
        key_event(Keycode::Return, true, true),
        key_event(Keycode::Return, false, false),
    ] {
        assert_eq!(pressed_control(&event, &bindings), None);
    }
    Ok(())
}

// Synthetic SDL key/text events cover the same Return key in the menu,
// gameplay and the open console without booting a window or map.
#[test]
fn console_bound_to_enter_yields_to_menu_navigation_and_its_own_editor() -> Result<(), String> {
    let mut console = console::Console::default();
    let mut bindings = KeyBindings::default();
    bindings.set_key("console", "ENTER")?;
    let enter = key_event(Keycode::Return, true, false);
    for state in [
        AppState::MainMenu,
        AppState::Paused,
        AppState::Settings,
        AppState::PauseSettings,
    ] {
        assert!(
            !console.wants_toggle(&enter, &bindings.console, state, true),
            "menus own Enter"
        );
    }
    assert_eq!(
        InputHandler::poll_menu_nav_event(&enter),
        Some(MenuNavEvent::Activate)
    );
    assert!(
        console.wants_toggle(&enter, &bindings.console, AppState::Playing, true),
        "Enter opens the gameplay console"
    );
    console.toggle(true, true);
    let _command = console.handle_event(&Event::TextInput {
        timestamp: 0,
        window_id: 1,
        text: "/help".into(),
    });
    assert!(
        !console.wants_toggle(&enter, &bindings.console, AppState::Playing, true),
        "the open console owns text input"
    );
    assert!(
        console
            .handle_event(&enter)
            .is_some_and(|command| command.is_ok()),
        "Enter submits"
    );
    assert!(console.is_open(), "submission does not close the console");
    assert!(
        console::escape_key(&key_event(Keycode::Escape, true, false)),
        "Escape can still close"
    );
    Ok(())
}
