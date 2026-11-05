//! Regression value/cost: square-corner steps and semantic footstep ownership
//! previously disagreed with collision/backend playback. Tiny CPU geometry and
//! bounded commands, no assets, audio device, renderer or full-map bake.
use crate::engine::{
    ai::{AiStimulus, perception::PlayerMovement},
    audio::{PerceptionOwner, SoundEvent},
    collision::WallAabb,
    level::LevelDef,
    physics::ItemWorld,
};
use crate::game::{
    AppState, CollisionWorld, Game, Settings,
    audio::WorldAudio,
    input::{Control, InputState, ScriptedHold},
};
use glam::Vec3;

fn fixture() -> Result<(Game, ItemWorld), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"step_noise","name":"Step noise","spawn":{"x":0,"z":0},"rooms":[{"x":-20,"z":-20,"width":40,"depth":40,"height":10,"ceiling":{"kind":"open"}}]}"#).map_err(|error|error.to_string())?;
    let mut game = Game::new(
        Vec3::new(0.0, 1.6, 0.0),
        std::f32::consts::FRAC_PI_2,
        CollisionWorld::from_level(&level),
    );
    game.set_app_state(AppState::Playing);
    Ok((game, ItemWorld::from_level(&level, &[])))
}
fn frame(game: &mut Game, controls: &[Control], delta: f32) {
    let mut input = InputState::default();
    input.apply_move_script(
        &controls
            .iter()
            .map(|control| ScriptedHold {
                control: *control,
                first_seconds: 0.0,
                last_seconds: 1.0,
            })
            .collect::<Vec<_>>(),
        0.5,
    );
    input.clear_presses();
    game.set_benchmark_simulation_delta(delta);
    game.update_player_movement(&mut input, &Settings::default());
}
fn step(
    game: &mut Game,
    items: &ItemWorld,
    audio: &mut WorldAudio,
) -> Result<crate::game::audio::PlayerFootstep, String> {
    audio.reset_movement();
    let _initial = audio.footstep(game, items);
    for _ in 0_u8..3 {
        game.player_position.x += 0.5;
        if let Some(step) = audio.footstep(game, items) {
            return Ok(step);
        }
    }
    Err("Expected real travel to make a footstep".into())
}
#[test]
fn low_solid_steps_use_square_corners_without_raising_step_or_headroom_limits() -> Result<(), String>
{
    for hz in [20_u16, 30, 60, 120, 144, 240] {
        for crouched in [false, true] {
            for z in [0.35_f32, -1.35, -0.5] {
                for (height, roof) in [(0.3_f32, false), (0.44, false), (0.46, false), (0.3, true)]
                {
                    let (mut game, _) = fixture()?;
                    if crouched {
                        for _ in 0_u8..30 {
                            frame(&mut game, &[Control::Crouch], 0.015);
                        }
                    }
                    let mut walls = vec![WallAabb::with_y(1.0, 0.0, z, 1.0, height, 1.0)];
                    if roof {
                        walls.push(WallAabb::with_y(
                            -1.0,
                            game.body_height() + 0.1,
                            -2.0,
                            4.0,
                            0.2,
                            4.0,
                        ));
                    }
                    game.set_walls(walls);
                    let mut max_feet = 0.0_f32;
                    for _ in 0..hz.saturating_mul(2) {
                        let controls = if crouched {
                            vec![Control::MoveForward, Control::Crouch]
                        } else {
                            vec![Control::MoveForward]
                        };
                        frame(&mut game, &controls, 1.0 / f32::from(hz));
                        max_feet = max_feet.max(game.feet_y());
                        assert!(
                            game.body_fits_at(Vec3::new(
                                game.player_position.x,
                                game.feet_y(),
                                game.player_position.z
                            )),
                            "step may not enter solid geometry"
                        );
                    }
                    if height < 0.45 && !roof {
                        assert!(
                            game.player_position.x > 2.0,
                            "low square corner must traverse: {hz} {z} {height} {crouched}"
                        );
                        assert!(
                            (max_feet - height).abs() < 0.001,
                            "rise must come from step support"
                        );
                    } else {
                        assert!(
                            game.player_position.x < 0.601,
                            "height and roof rejection remain intact"
                        );
                        assert!(max_feet.abs() < 0.001, "rejected steps cannot grant height");
                    }
                    assert_eq!(
                        game.reset_count(),
                        0,
                        "no recovery may hide a collision failure"
                    );
                }
            }
        }
    }
    Ok(())
}
#[test]
fn footstep_center_owner_and_blocked_stance_survive_playback_echo_and_rest() -> Result<(), String> {
    let (mut game, items) = fixture()?;
    let mut audio = WorldAudio::default();
    let walk = step(&mut game, &items, &mut audio)?;
    assert_eq!(
        walk.movement,
        PlayerMovement::Walking,
        "standing collision stance owns the event"
    );
    assert!(
        (walk.origin.y - 0.9).abs() < 0.0001,
        "standing center differs from feet and camera"
    );
    let before = game.entities().stimuli().len();
    let request = walk.request();
    assert!(request.gain > 0.0, "walking output remains audible");
    assert_eq!(
        request.perception,
        PerceptionOwner::Gameplay { event: walk.serial },
        "semantic event owns playback"
    );
    game.entities_mut().observe_audio_start(SoundEvent {
        serial: 1,
        source: None,
        // The reproduced backend echo used the camera, separated from the
        // body center; provenance must work without the proximity heuristic.
        position: [walk.origin.x, walk.origin.y + 0.7, walk.origin.z],
        gain: request.gain,
        max_distance: 24.0,
        looping: false,
        perception: request.perception,
    });
    assert_eq!(
        game.entities().stimuli().len(),
        before,
        "backend acceptance cannot duplicate the footstep"
    );
    for _ in 0_u8..30 {
        frame(&mut game, &[Control::Crouch], 0.015);
    }
    let crouch = step(&mut game, &items, &mut audio)?;
    assert_eq!(
        crouch.movement,
        PlayerMovement::Crouched,
        "crouch uses its collision stance"
    );
    assert!(
        (crouch.origin.y - 0.45).abs() < 0.0001,
        "crouching center differs from camera and feet"
    );
    assert!(
        crouch.request().gain > 0.0 && crouch.request().gain < request.gain,
        "both outputs audible and normal walking louder"
    );
    game.set_walls(vec![WallAabb::with_y(-10.0, 1.1, -10.0, 20.0, 0.2, 20.0)]);
    for _ in 0_u8..30 {
        frame(&mut game, &[], 0.015);
    }
    assert!(
        game.is_crouched(),
        "blocked released duck must retain its collision stance"
    );
    assert_eq!(
        step(&mut game, &items, &mut audio)?.movement,
        PlayerMovement::Crouched,
        "blocked release cannot publish a 15m event"
    );
    let resting = game.entities().stimuli().len();
    for _ in 0_u8..50 {
        assert!(
            audio.footstep(&mut game, &items).is_none(),
            "rest cannot create a new sound"
        );
    }
    assert_eq!(
        game.entities().stimuli().len(),
        resting,
        "stopping preserves prior events without creating new ones"
    );
    let old_walk = game
        .entities()
        .stimuli()
        .into_iter()
        .find(|noise| noise.player_movement == Some(PlayerMovement::Walking))
        .ok_or("missing past walking event")?;
    assert!(
        old_walk.heard_at(walk.origin + Vec3::X * 15.0, 30.0),
        "past walking step retains its emitted stance and bound"
    );
    Ok(())
}
#[test]
fn unrelated_backend_noise_and_recovery_keep_their_lifecycle_policy() -> Result<(), String> {
    let (mut game, items) = fixture()?;
    let mut audio = WorldAudio::default();
    let emission = step(&mut game, &items, &mut audio)?;
    // A concurrent ordinary sound at the same center is a distinct event;
    // footstep ownership must never suppress it through spatial deduplication.
    game.entities_mut().observe_audio_start(SoundEvent {
        serial: 2,
        source: None,
        position: emission.origin.to_array(),
        gain: 0.5,
        max_distance: 24.0,
        looping: false,
        perception: PerceptionOwner::Playback,
    });
    assert!(
        game.entities()
            .stimuli()
            .iter()
            .any(|noise| noise.category == "sound" && noise.player_movement.is_none()),
        "unrelated backend sounds retain hearing"
    );
    game.entities_mut()
        .push_stimulus(AiStimulus::player_footstep(
            Vec3::ZERO,
            PlayerMovement::Crouched,
            1,
        ));
    game.reset_to_spawn();
    assert!(
        game.entities().stimuli().is_empty(),
        "recovery cannot replay a stale movement event"
    );
    Ok(())
}

// Unique regression: shrinking the stride on an in-place duck released a
// buffered walking distance as a phantom step. Thirty CPU commands, no assets.
#[test]
fn stationary_duck_cannot_release_a_buffered_step_without_new_travel() -> Result<(), String> {
    let (mut game, items) = fixture()?;
    let mut audio = WorldAudio::default();
    assert!(
        audio.footstep(&mut game, &items).is_none(),
        "initial pose is silent"
    );
    for _ in 0_u8..2 {
        game.player_position.x += 0.5;
        assert!(
            audio.footstep(&mut game, &items).is_none(),
            "partial standing stride is buffered"
        );
    }
    let before = glam::Vec2::new(game.player_position.x, game.player_position.z);
    for _ in 0_u8..30 {
        frame(&mut game, &[Control::Crouch], 0.015);
        assert!(
            audio.footstep(&mut game, &items).is_none(),
            "changing stance at rest cannot emit a footstep"
        );
    }
    assert!(
        game.is_crouched(),
        "actual collision stance changes during the witness"
    );
    assert_eq!(
        before,
        glam::Vec2::new(game.player_position.x, game.player_position.z),
        "the witness has no planar motion"
    );
    assert!(
        game.entities().stimuli().is_empty(),
        "no new movement stimulus at rest"
    );
    game.player_position.x += 0.01;
    let resumed = audio
        .footstep(&mut game, &items)
        .ok_or("buffered cadence should resume with real travel")?;
    assert_eq!(
        resumed.movement,
        PlayerMovement::Crouched,
        "resumed event samples current stance"
    );
    Ok(())
}
