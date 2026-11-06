//! Explicit native controller replay. Real contact descriptors are replayed
//! through `EntityWorld`; this does not claim GPU, solver or audio-device output.
use glam::Vec3;
use places::engine::collision_index::CollisionIndex;
use places::engine::entities::{EntityWorld, WorldContext};
use places::engine::level::{LevelDef, WalkableFloor};
use places::engine::physics::ImpactEvent;
use std::io::Read;
use std::path::Path;

pub fn bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Replay inputs must be regular files".into());
    }
    let mut bytes = Vec::new();
    let _read = file
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 65_536 {
        return Err("Replay input exceeds 64KiB".into());
    }
    Ok(bytes)
}

pub fn run(fixture: &Path, contacts: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err("Replay output must be a new file".into());
    }
    let started = std::time::Instant::now();
    let source = bounded(fixture)?;
    let contact_bytes = bounded(contacts)?;
    let level: LevelDef = serde_json::from_slice(&source).map_err(|error| error.to_string())?;
    let reports: Vec<ImpactEvent> =
        serde_json::from_slice(&contact_bytes).map_err(|error| error.to_string())?;
    if reports.len() != 6
        || reports.iter().any(|report| {
            reports.first().is_none_or(|first| {
                first.key != report.key
                    || first.causal_source != report.causal_source
                    || first.incident != report.incident
                    || first.release != report.release
            })
        })
    {
        return Err("Replay requires six reports from one causal release".into());
    }
    let result = execute(&level, &reports)?;
    let receipt = serde_json::json!({"scope":"Native Places controller/asset replay; six recorded collision descriptors in0.5simulationseconds. Rendering, actual solver contacts and audible output are covered by separately labeled native game runs.",
        "fixture_sha256":places::engine::package::hash::sha256_hex(&source),
        "contacts_sha256":places::engine::package::hash::sha256_hex(&contact_bytes),
        "seconds":started.elapsed().as_secs_f64(),"result":result});
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn execute(level: &LevelDef, reports: &[ImpactEvent]) -> Result<serde_json::Value, String> {
    let mut world = EntityWorld::from_level(level);
    let walls = level.item_collision_aabbs();
    let index = CollisionIndex::build(&walls);
    let floor = WalkableFloor::from_level(level);
    let feet = Vec3::new(-6.0, 0.0, -8.0);
    let context = WorldContext {
        delta_seconds: 1.0 / 120.0,
        player_walk_speed: 3.0,
        feet_from: feet,
        feet,
        eye: Vec3::new(-6.0, 1.6, -8.0),
        body_height: 1.8,
        body_half: 0.4,
        form_model: None,
        walls: &walls,
        index: &index,
        floor: &floor,
        nav: None,
    };
    let mut starts = 0_usize;
    let mut submitted = Vec::new();
    for frame in 0_u16..240 {
        if let Some(slot) = [96_u16, 108, 120, 132, 144, 156]
            .iter()
            .position(|at| *at == frame)
        {
            let report = reports.get(slot).copied().ok_or("Missing contact")?;
            world.note_guest_impact(report);
            submitted.push(f32::from(frame) / 120.0);
        }
        let _tick = world.tick(&context);
        for (_, sound) in world.take_guest_sounds() {
            if sound.cue == "tg_pursuit_shriek_loop"
                && sound.looping
                && !sound.stop
                && !sound.update
            {
                starts = starts.saturating_add(1);
            }
        }
    }
    let state = world.guest_diagnostics();
    let guest = state
        .as_array()
        .and_then(|guests| guests.first())
        .ok_or("Native guest did not load")?;
    let accepted = guest
        .get("events")
        .and_then(serde_json::Value::as_array)
        .ok_or("Missing events")?
        .iter()
        .filter(|event| {
            event.get("type").and_then(serde_json::Value::as_str) == Some("stimulus_heard")
        })
        .count();
    let incidents = guest
        .pointer("/state/incidents")
        .and_then(serde_json::Value::as_array)
        .ok_or("Missing incidents")?
        .len();
    if starts != 1 || accepted != 1 || incidents != 1 {
        return Err(format!(
            "Causal replay failed: starts={starts}, accepted={accepted}, incidents={incidents}"
        ));
    }
    Ok(
        serde_json::json!({"submitted_at":submitted,"accepted_incidents":incidents,"accepted_landings":accepted,"pursuit_loop_starts":starts,"guest":guest}),
    )
}
