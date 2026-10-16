//! Small, deterministic contracts: no package loading, GPU or map bake.
use super::*;
use crate::engine::level::LevelDef;
use crate::game::{CollisionWorld, EYE_HEIGHT, Game};
use command::{Command, parse};
use glam::Vec3;

#[test]
fn registry_parses_only_complete_typed_commands() {
    assert_eq!(parse(" /help "), Ok(Command::Help));
    assert_eq!(parse("/clear"), Ok(Command::Clear));
    assert_eq!(parse("/noclip"), Ok(Command::Noclip(None)));
    assert_eq!(parse("/noclip off"), Ok(Command::Noclip(Some(false))));
    assert_eq!(
        parse("/give core:rubber_duck"),
        Ok(Command::Give("core:rubber_duck".to_string()))
    );
    for malformed in [
        "",
        "help",
        "/",
        "/unknown",
        "/help extra",
        "/clear extra",
        "/noclip maybe",
        "/give",
        "/give a b",
        "/give ../../file",
        "/give $(shell)",
    ] {
        assert!(parse(malformed).is_err(), "must reject {malformed:?}");
    }
}

#[test]
fn enable_gate_bounds_history_and_restores_draft() {
    let mut console = Console::default();
    console.toggle(false, true);
    assert!(!console.is_open(), "disabled console cannot open");
    console.toggle(true, false);
    assert!(!console.is_open(), "menus cannot open console");
    console.toggle(true, true);
    console.append_text(&"w".repeat(1_000));
    assert_eq!(console.input.len(), MAX_INPUT, "input has a hard bound");
    console.input.clear();
    for index in 0_usize..40 {
        console.input = format!("/unknown{index}");
        let event = Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Return),
            scancode: None,
            keymod: sdl3::keyboard::Mod::NOMOD,
            repeat: false,
            which: 0,
            raw: 0,
        };
        assert!(
            console.handle_event(&event).is_some(),
            "submission returns a typed result"
        );
    }
    assert_eq!(
        console.history.len(),
        MAX_HISTORY,
        "oldest commands are evicted"
    );
    console.append_text("draft");
    console.recall(true);
    assert_eq!(console.input, "/unknown39");
    console.recall(false);
    assert_eq!(console.input, "draft", "Down restores unfinished input");
    for _ in 0_usize..200 {
        console.write("x".repeat(1_000));
    }
    assert_eq!(console.output.len(), MAX_OUTPUT, "output remains bounded");
    assert!(console.output.iter().all(|line| line.len() <= MAX_MESSAGE));
    console.scroll(true);
    assert!(console.scroll > 0, "wrapped output scrolls");
    let vertices = console.vertices(false);
    assert!(
        vertices
            .iter()
            .all(|vertex| (0.0..=480.0).contains(&vertex.pos[0])
                && (0.0..=272.0).contains(&vertex.pos[1])),
        "overflow stays inside the native UI"
    );
    console.close();
    assert_eq!(console.input, "", "closing discards partial input");
}

fn fixture_game() -> Result<Game, String> {
    let level = LevelDef::from_json(
        r#"{
        "format_version":3,"id":"console_control","name":"Console control",
        "spawn":{"x":2,"z":2},
        "rooms":[{"x":0,"z":0,"width":10,"depth":10,"height":4}, {"x":0,"z":0,"width":10,"depth":10,"floor_y":4.5,"height":4}],
        "walls":[{"x":5,"z":0,"width":1,"depth":10,"height":4}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    Ok(Game::new(
        Vec3::new(2.0, EYE_HEIGHT, 2.0),
        0.0,
        CollisionWorld::from_level(&level),
    ))
}

#[test]
fn noclip_exit_recovers_geometry_and_void_and_reset_clears_mode() -> Result<(), String> {
    let mut game = fixture_game()?;
    let start = game.player_position;
    let _message_1 = game.set_noclip(true)?;
    game.player_position = Vec3::new(5.5, EYE_HEIGHT, 2.0);
    let _message_2 = game.set_noclip(false)?;
    assert!(
        game.player_position.distance(start) < 0.001,
        "solid exit restores safe entry"
    );
    let _message_3 = game.set_noclip(true)?;
    game.player_position = Vec3::new(100.0, EYE_HEIGHT, 100.0);
    let _message_4 = game.set_noclip(false)?;
    assert!(
        game.player_position.distance(start) < 0.001,
        "void exit is immediate, not a grace-period fall"
    );
    game.set_app_state(crate::game::AppState::Playing);
    let _flight = game.set_noclip(true)?;
    game.player_yaw = std::f32::consts::FRAC_PI_2;
    let mut input =
        crate::game::input::InputState::holding(&[crate::game::input::Control::MoveForward]);
    for _ in 0_usize..15 {
        game.set_benchmark_simulation_delta(0.1);
        game.update_player_movement(&mut input, &crate::game::settings::Settings::default());
    }
    assert!(
        game.player_position.x > 6.0,
        "flight crosses the fixture wall"
    );
    assert!(
        (game.player_position.y - EYE_HEIGHT).abs() < 0.001,
        "flight has no gravity"
    );
    let _land = game.set_noclip(false)?;
    assert!(
        game.player_position.x > 6.0,
        "clear in-map flight exit keeps its position"
    );
    let _message_5 = game.set_noclip(true)?;
    game.reset_to_spawn();
    assert!(!game.noclip_enabled(), "reset invalidates flight mode");
    assert!(
        game.developer_prop_position(Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1))
            .is_some(),
        "small prop gets a clear flat floor"
    );
    game.set_benchmark_player_position(Vec3::new(2.0, 6.1, 2.0), 0.0, 0.0);
    let upper = game.developer_prop_position(Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1));
    assert!(
        upper.is_some_and(|position| (position.y - 4.5).abs() < 0.001),
        "give selects the player's storey in a stacked footprint"
    );

    Ok(())
}

#[test]
fn dispatcher_gates_runtime_and_surfaces_results() {
    #[derive(Default)]
    struct Target {
        flight: bool,
        give_calls: usize,
    }
    impl runtime::CommandTarget for Target {
        fn set_noclip(&mut self, request: Option<bool>) -> Result<String, String> {
            self.flight = request.unwrap_or(!self.flight);
            Ok(format!("Noclip {}", self.flight))
        }
        fn give(&mut self, id: &str) -> Result<String, String> {
            self.give_calls = self.give_calls.saturating_add(1);
            Err(format!("Unsupported item {id}"))
        }
    }
    let mut target = Target::default();
    let mut console = Console::default();
    runtime::dispatch(parse("/noclip"), &mut console, &mut target);
    assert!(!target.flight, "closed console cannot call runtime");
    console.toggle(true, true);
    runtime::dispatch(parse("/noclip"), &mut console, &mut target);
    assert!(target.flight, "typed mode request reaches runtime once");
    runtime::dispatch(parse("/give core:crate"), &mut console, &mut target);
    assert_eq!(target.give_calls, 1, "give uses the typed integration seam");
    assert!(
        console
            .latest_message()
            .is_some_and(|message| message.starts_with("Error: Unsupported")),
        "runtime refusal is visible"
    );
    runtime::dispatch(parse("/help"), &mut console, &mut target);
    assert!(
        console
            .output
            .iter()
            .any(|message| message.starts_with("/give")),
        "help uses the same registry"
    );
    runtime::dispatch(parse("/clear"), &mut console, &mut target);
    assert_eq!(console.output.len(), 0, "clear removes output");
}
