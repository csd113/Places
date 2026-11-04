//! A held prop must survive clicking a lift button, while ordinary clicks still
//! throw and focus guards still discard input. Pure edge ownership, no game I/O.
use super::{CarryInput, MouseContext};

#[test]
fn held_button_click_is_consumed_once_without_throwing() {
    let mut input = CarryInput::default();
    let context = MouseContext {
        gameplay: true,
        captured: true,
        held: Some(9),
    };
    input.mouse_button(true, context);
    assert!(input.take_elevator_click(true));
    assert!(!input.take_elevator_click(true));
    assert_eq!(input.take_throw(true), None);
    input.mouse_button(false, context);
    input.mouse_button(true, context);
    assert!(!input.take_elevator_click(false));
    assert_eq!(input.take_throw(true), Some(9));
    input.focus_changed(false);
    input.mouse_button(true, context);
    assert!(!input.take_elevator_click(true));
    assert_eq!(input.take_throw(true), None);
}
