//! A held prop must survive clicking a lift button, while ordinary clicks still
//! throw and focus guards still discard input. Pure edge ownership, no game I/O.
use super::{CarryInput, MouseContext};

// Protects the new tool/carry ownership boundary: a held or empty click cannot
// also throw, activate a lift or repeat while held. Pure edges cost microseconds.
#[test]
fn tool_claims_one_mouse_edge_before_carry_or_world_actions() {
    for held in [None, Some(9_u64)] {
        let mut input = CarryInput::default();
        let context = MouseContext {
            gameplay: true,
            captured: true,
            held,
        };
        input.mouse_button(true, context);
        assert!(input.take_tool_click(true));
        assert!(!input.take_tool_click(true));
        assert!(!input.take_world_click(true));
        assert!(!input.take_elevator_click(true));
        assert_eq!(input.take_throw(true), None);
        input.mouse_button(true, context);
        assert!(!input.take_tool_click(true), "held button never repeats");
        input.mouse_button(false, context);
        input.focus_changed(false);
        input.mouse_button(true, context);
        assert!(
            !input.take_tool_click(true),
            "focus-recapture click cannot attack"
        );
    }
}

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
