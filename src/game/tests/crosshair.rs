//! Protects strict command parsing, map precedence and reload clearing. One
//! asset-free runtime value, no renderer, bake or wall-clock work (<1ms).
use crate::game::console::command::{self, Command, CommandKind};
use crate::game::gameplay_runtime::Runtime;

#[test]
fn crosshair_commands_and_map_override_lifetime() {
    for (text, expected) in [("/crosshair on", true), ("/crosshair off", false)] {
        assert_eq!(
            command::parse(text),
            Ok(Command::Crosshair(expected)),
            "explicit Boolean command"
        );
    }
    for text in ["/crosshair", "/crosshair yes", "/crosshair on extra"] {
        assert!(
            command::parse(text).is_err(),
            "malformed command cannot alter presentation"
        );
    }
    assert_eq!(
        command::parse("/help crosshair"),
        Ok(Command::Help(Some(CommandKind::Crosshair))),
        "help is registered"
    );
    let mut runtime = Runtime::default();
    assert!(runtime.crosshair_enabled(true), "old maps default on");
    assert!(!runtime.crosshair_enabled(false), "authored off applies");
    for enabled in [false, true] {
        runtime.crosshair_override = Some(enabled);
        assert_eq!(
            runtime.crosshair_enabled(!enabled),
            enabled,
            "console beats map default"
        );
        runtime.reset_map_presentation();
        assert_eq!(
            runtime.crosshair_override, None,
            "map commit clears override"
        );
        assert_eq!(
            runtime.crosshair_enabled(!enabled),
            !enabled,
            "new map default resumes"
        );
    }
}

// Odd-size letterboxing formerly puts the HUD midpoint half a pixel off the
// actual view centre. This cheap geometry check covers that unique boundary.
#[test]
fn crosshair_center_accounts_for_odd_drawable_letterboxing() {
    use crate::engine::render::DrawableSize;
    for (width, height) in [(960_u16, 544_u16), (1201, 801), (1921, 1081), (2880, 1632)] {
        let size = DrawableSize::new(u32::from(width), u32::from(height));
        let viewport = size.ui_viewport();
        let [x, y] = size.view_center_in_ui();
        let origin_x = f32::from(i16::try_from(viewport.x).unwrap_or(0));
        let origin_y = f32::from(i16::try_from(viewport.y).unwrap_or(0));
        let drawn_x = x.mul_add(
            f32::from(u16::try_from(viewport.width).unwrap_or(0)) / 480.0,
            origin_x,
        );
        let drawn_y = y.mul_add(
            f32::from(u16::try_from(viewport.height).unwrap_or(0)) / 272.0,
            origin_y,
        );
        assert!(
            f32::from(width).mul_add(-0.5, drawn_x).abs() < 0.001,
            "horizontal centre is exact"
        );
        assert!(
            f32::from(height).mul_add(-0.5, drawn_y).abs() < 0.001,
            "vertical centre is exact"
        );
    }
}
