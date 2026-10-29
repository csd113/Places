//! Small, named forms expose existing engine capabilities without raw JSON panels.

use crate::editor::document::components::{Field as F, Kind as K, Target as T};

pub(super) fn identity() -> Vec<F> {
    vec![
        F::new(T::Identity, "id", "Stable identity", K::Text, "\"\""),
        F::new(T::Object, "display_name", "Display name", K::Text, "\"\""),
        F::new(
            T::Component("interactable"),
            "prompt",
            "Interaction prompt",
            K::Text,
            "\"Interact\"",
        ),
        F::new(
            T::Component("interactable"),
            "label",
            "World label",
            K::Text,
            "\"\"",
        ),
        F::new(
            T::Component("interactable"),
            "enabled",
            "Interaction enabled",
            K::Bool,
            "true",
        ),
        F::new(
            T::Component("interactable"),
            "reach",
            "Interaction reach m",
            K::Number,
            "2.5",
        ),
    ]
}
pub(super) fn lighting(fixture: bool) -> Vec<F> {
    if fixture {
        return vec![
            F::new(T::Object, "switch_group", "Shared circuit", K::Text, "\"\""),
            F::new(
                T::Object,
                "hold_in_static",
                "Hold authored Static",
                K::Bool,
                "false",
            ),
            F::new(
                T::Object,
                "static_switch",
                "Static off preparation",
                K::Choice(&["disabled", "prepared"]),
                "\"disabled\"",
            ),
        ];
    }
    vec![
        F::new(T::Object, "switch_group", "Shared circuit", K::Text, "\"\""),
        F::new(
            T::Component("light"),
            "enabled",
            "Light enabled",
            K::Bool,
            "true",
        ),
        F::new(
            T::Component("light"),
            "switchable",
            "Switchable",
            K::Bool,
            "false",
        ),
        F::new(
            T::Component("light"),
            "emission_scale",
            "Emission scale",
            K::Number,
            "1.0",
        ),
    ]
}
pub(super) fn carry() -> Vec<F> {
    vec![
        F::new(
            T::Component("carry"),
            "item",
            "Requested item identity",
            K::Text,
            "\"\"",
        ),
        F::new(
            T::Component("carry"),
            "socket",
            "Animated hand node",
            K::Text,
            "\"hand.R\"",
        ),
        F::new(
            T::Component("carry"),
            "offset",
            "Hand local offset metres",
            K::Vector,
            "[0,0,0]",
        ),
        F::new(
            T::Component("carry"),
            "rotation_degrees",
            "Hand local XYZ degrees",
            K::Vector,
            "[0,0,0]",
        ),
        F::new(
            T::Component("carry"),
            "idle_clip",
            "Carry idle clip",
            K::Text,
            "\"carry_idle\"",
        ),
        F::new(
            T::Component("carry"),
            "walk_clip",
            "Carry walk clip",
            K::Text,
            "\"carry_walk\"",
        ),
    ]
}

pub(super) fn physics() -> Vec<F> {
    vec![
        F::new(T::Physics, "reset_group", "Reset group", K::Text, "\"\""),
        F::new(
            T::Object,
            "size",
            "Fixed collision [W,H,D]",
            K::Vector,
            "[0.6,0.9,0.6]",
        ),
        F::new(T::Physics, "friction", "Friction", K::Number, "0.6"),
        F::new(T::Physics, "restitution", "Bounce", K::Number, "0.05"),
        F::new(
            T::Physics,
            "linear_damping",
            "Linear damping",
            K::Number,
            "0.1",
        ),
        F::new(
            T::Physics,
            "angular_damping",
            "Angular damping",
            K::Number,
            "0.2",
        ),
    ]
}
pub(super) fn ai() -> Vec<F> {
    let mut fields = vec![
        F::new(
            T::Component("ai"),
            "walk_speed",
            "Walk m/s",
            K::Number,
            "1.0",
        ),
        F::new(T::Component("ai"), "run_speed", "Run m/s", K::Number, "2.0"),
        F::new(
            T::Component("ai"),
            "sight_range",
            "Sight range m",
            K::Number,
            "8.0",
        ),
        F::new(
            T::Component("ai"),
            "catch_radius",
            "Catch radius m",
            K::Number,
            "0.45",
        ),
        F::new(
            T::Component("ai"),
            "catch_height",
            "Catch height m",
            K::Number,
            "0.8",
        ),
        F::new(
            T::Component("ai"),
            "hearing_range",
            "Hearing range m",
            K::Number,
            "6.0",
        ),
        F::new(
            T::Component("ai"),
            "can_open_doors",
            "AI may open doors",
            K::Bool,
            "false",
        ),
        F::new(
            T::Component("light_response"),
            "reveal_only",
            "Visible only in beam",
            K::Bool,
            "false",
        ),
        F::new(
            T::Component("light_response"),
            "enter_intensity",
            "Beam enter intensity",
            K::Number,
            "0.035",
        ),
        F::new(
            T::Component("light_response"),
            "exit_intensity",
            "Beam exit intensity",
            K::Number,
            "0.005",
        ),
        F::new(
            T::Component("light_response"),
            "release_seconds",
            "Beam release seconds",
            K::Number,
            "0.2",
        ),
    ];
    fields.extend(nav_body());
    fields
}
fn nav_body() -> Vec<F> {
    vec![
        F::new(
            T::Component("nav_agent"),
            "speed_mps",
            "Navigation speed m/s",
            K::Number,
            "1.2",
        ),
        F::new(
            T::Component("nav_agent"),
            "radius",
            "Body radius m",
            K::Number,
            "0.3",
        ),
        F::new(
            T::Component("nav_agent"),
            "height",
            "Body height m",
            K::Number,
            "1.8",
        ),
        F::new(
            T::Component("nav_agent"),
            "step_height",
            "Step height m",
            K::Number,
            "0.4",
        ),
        F::new(
            T::Component("nav_agent"),
            "max_slope",
            "Maximum walk slope",
            K::Number,
            "2.6666667",
        ),
    ]
}

pub(super) fn audio() -> Vec<F> {
    vec![
        F::new(
            T::Component("audio"),
            "sound",
            "Installed sound cue",
            K::Text,
            "\"switch_click\"",
        ),
        F::new(T::Component("audio"), "gain", "Gain", K::Number, "1.0"),
        F::new(T::Component("audio"), "looped", "Loop", K::Bool, "false"),
        F::new(
            T::Component("audio"),
            "playing",
            "Play on load",
            K::Bool,
            "false",
        ),
        F::new(
            T::Component("audio"),
            "enabled",
            "Audio enabled",
            K::Bool,
            "true",
        ),
    ]
}
pub(super) fn source(index: usize) -> Vec<F> {
    let t = T::Light(index);
    vec![
        F::new(
            t,
            "offset",
            "Local offset [X,Y,Z]",
            K::Vector,
            "[0.0,1.0,0.0]",
        ),
        F::new(t, "intensity", "Intensity", K::Number, "1.0"),
        F::new(t, "range", "Range m", K::Number, "5.0"),
        F::new(
            t,
            "color",
            "Linear colour [R,G,B]",
            K::Vector,
            "[1.0,1.0,1.0]",
        ),
        F::new(t, "enabled", "Source enabled", K::Bool, "true"),
        F::new(
            t,
            "rotation_degrees",
            "Source yaw degrees",
            K::Number,
            "0.0",
        ),
    ]
}
pub(super) fn map() -> Vec<F> {
    vec![
        F::new(T::Map, "saving", "Gameplay saves", K::Bool, "false"),
        F::new(T::Spawn, "x", "Player spawn X", K::Number, "0.0"),
        F::new(T::Spawn, "z", "Player spawn Z", K::Number, "0.0"),
        F::new(
            T::Spawn,
            "yaw_degrees",
            "Player facing degrees",
            K::Number,
            "0.0",
        ),
        F::new(
            T::Defaults,
            "floor",
            "Default surface material",
            K::Text,
            "\"core:carpet_beige_01\"",
        ),
        F::new(
            T::Defaults,
            "wall",
            "Default wall material",
            K::Text,
            "\"core:wallpaper_yellow_01\"",
        ),
        F::new(
            T::Defaults,
            "ceiling",
            "Default ceiling material",
            K::Text,
            "\"core:ceiling_panel_01\"",
        ),
        F::new(
            T::Map,
            "requires_full_lighting",
            "Require Full lighting",
            K::Bool,
            "false",
        ),
        F::new(T::Presentation, "exposure", "Exposure", K::Number, "1.0"),
        F::new(
            T::Presentation,
            "tone_knee",
            "Tone shoulder",
            K::Number,
            "0.75",
        ),
        F::new(
            T::Presentation,
            "saturation",
            "Saturation",
            K::Number,
            "1.03",
        ),
        F::new(T::Presentation, "contrast", "Contrast", K::Number, "1.02"),
        F::new(
            T::Fog,
            "color",
            "Fog colour [R,G,B]",
            K::Vector,
            "[0.6,0.63,0.68]",
        ),
        F::new(T::Fog, "density", "Fog density", K::Number, "0.0095"),
        F::new(T::Fog, "reference_y", "Fog reference Y", K::Number, "2.0"),
        F::new(T::Fog, "height_gain", "Fog height gain", K::Number, "0.045"),
    ]
}
