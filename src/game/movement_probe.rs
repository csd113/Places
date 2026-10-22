//! Explicit developer replay: no renderer, rigid simulation, assets or map bake.
use super::input::{Control, InputState, ScriptedHold};
use super::{CollisionWorld, EYE_HEIGHT, Game, LevelDef, Settings, VERTICAL_SUBSTEP};
use glam::{Vec2, Vec3};
use std::io::Write;
use std::path::Path;

/// Save deterministic per-command trajectories through an ordinary editor map.
/// # Errors
/// Returns source-validation or output I/O failures without modifying the map.
pub fn run(source: &Path, out: &Path) -> Result<(), String> {
    let level = LevelDef::from_json(&std::fs::read_to_string(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let cases = [
        ("acceleration-stop", 4.0, 25.0, 0.0, 0.0, 0_u8),
        ("reversal", 4.0, 25.0, 0.0, 0.0, 1),
        ("diagonal", 4.0, 25.0, 0.0, 0.0, 2),
        ("stationary-jump", 4.0, 25.0, 0.0, 0.0, 3),
        ("running-jump", 4.0, 25.0, 0.0, 0.0, 4),
        ("held-jump", 4.0, 25.0, 0.0, 0.0, 5),
        ("landing-hop", 4.0, 25.0, 0.0, 0.0, 6),
        ("crouch-jump", 4.0, 25.0, 0.0, 0.0, 7),
        ("air-left", 4.0, 25.0, 0.0, 0.0, 8),
        ("air-right", 4.0, 25.0, 0.0, 0.0, 9),
        ("stairs-jump", 15.0, 3.0, 0.0, 180.0, 10),
        ("ramp-jump", 19.0, 3.0, 0.0, 180.0, 10),
        ("low-ceiling", 6.0, 35.4, 0.0, 0.0, 11),
        ("step-below", 15.0, 17.0, 0.0, 180.0, 12),
        ("step-above", 19.0, 17.0, 0.0, 180.0, 12),
        ("steep-slide", 23.0, 6.0, 4.6, 0.0, 13),
        ("fast-corner", 25.0, 28.0, 0.0, 90.0, 14),
    ];
    for (name, x, z, feet, yaw, kind) in cases {
        let mut game = Game::new(
            Vec3::new(x, feet + EYE_HEIGHT, z),
            f32::to_radians(yaw),
            CollisionWorld::from_level(&level),
        );
        let mut file =
            std::fs::File::create(out.join(format!("{name}.jsonl"))).map_err(|e| e.to_string())?;
        let mut command = InputState::default();
        if kind == 14 {
            game.horizontal_velocity = Vec2::new(100.0, 100.0);
        }
        for tick in 0_u16..240 {
            let walk = match kind {
                0 | 1 => tick < 80,
                3 | 13 | 14 => false,
                _ => tick < 180,
            };
            let jump = match kind {
                3 => tick == 0,
                4 | 7..=10 => tick == 45,
                5 => tick >= 45,
                6 => tick == 45 || (tick > 46 && game.grounded),
                _ => false,
            };
            let duck = kind == 7 && (46..100).contains(&tick) || kind == 11 && tick < 100;
            let right = kind == 2 || kind == 9 && tick > 45;
            let left = kind == 8 && tick > 45;
            let back = kind == 1 && (80..160).contains(&tick);
            let script: Vec<_> = [
                (Control::MoveForward, walk),
                (Control::MoveBackward, back),
                (Control::Jump, jump),
                (Control::Crouch, duck),
                (Control::StrafeLeft, left),
                (Control::StrafeRight, right),
            ]
            .into_iter()
            .map(|(control, on)| ScriptedHold {
                control,
                first_seconds: if on { 0.0 } else { 2.0 },
                last_seconds: 3.0,
            })
            .collect();
            command.apply_move_script(&script, 1.0);
            game.sim_delta_seconds = VERTICAL_SUBSTEP;
            let _origin = game.update_playing_frame(&command, &Settings::default(), (0.0, 0.0));
            command.clear_presses();
            let row = serde_json::json!({"tick":tick,"seconds":f32::from(tick.saturating_add(1))*VERTICAL_SUBSTEP,"eye":game.player_position.to_array(),"feet":game.feet_y,"hull_top":game.feet_y+game.body_height(),"hull_height":game.body_height(),"velocity":[game.horizontal_velocity.x,game.vertical_velocity,game.horizontal_velocity.y],"grounded":game.grounded,"crouched":game.is_crouched()});
            writeln!(file, "{row}").map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
