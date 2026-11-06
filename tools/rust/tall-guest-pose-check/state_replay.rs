//! Tiny native controller persistence/delivery campaign, separate from the GPU
//! and device campaigns. Uses the production world and native imported asset.
use glam::Vec3;
use places::engine::collision_index::CollisionIndex;
use places::engine::entities::{EntityWorld, WorldContext, checkpoint::Snapshot};
use places::engine::level::{ActionDef, LevelDef, WalkableFloor};
use places::engine::tall_guest::audio_fence::Fence;
use serde_json::{Value, json};
use std::path::Path;

fn state(world: &EntityWorld) -> Result<Value, String> {
    world
        .guest_diagnostics()
        .pointer("/0/state")
        .cloned()
        .ok_or_else(|| "Guest absent".into())
}
fn tick(world: &mut EntityWorld, context: &WorldContext<'_>) {
    let _tick = world.tick(context);
    let _commands = world.take_commands();
    let _sounds = world.take_guest_sounds();
}
fn require(value: bool, description: &str) -> Result<Value, String> {
    if !value {
        return Err(format!("Native state replay failed: {description}"));
    }
    Ok(json!({"check":description,"status":"passed"}))
}
fn round_trip(level: &LevelDef, ctx: &WorldContext<'_>) -> Result<Vec<Value>, String> {
    let mut world = EntityWorld::from_level(level);
    let _initial = world.take_commands();
    for _ in 0_u8..87 {
        tick(&mut world, ctx);
    }
    let snapshot = world.checkpoint()?;
    let encoded = serde_json::to_value(&snapshot).map_err(|e| e.to_string())?;
    let mut restored = EntityWorld::from_level(level);
    restored.restore_checkpoint(&snapshot)?;
    restored.validate_restored_guests(ctx)?;
    let _commands = restored.take_commands();
    let mut checks = vec![require(
        state(&world)? == state(&restored)?,
        "exact current semantic state survives private restore",
    )?];
    for _ in 0_u8..120 {
        tick(&mut world, ctx);
        tick(&mut restored, ctx);
        if state(&world)? != state(&restored)? {
            return Err(format!(
                "Restored native continuation diverged: original={} restored={}",
                state(&world)?,
                state(&restored)?
            ));
        }
    }
    checks.push(require(
        true,
        "120 native ticks retain original evidence, clocks, markers and decisions",
    )?);
    let negative = [
        (
            "unknown record field",
            "/guests/0/unrecognized",
            json!(true),
        ),
        (
            "future sampled return",
            "/guests/0/pending/0/observation/sampled",
            json!(1000.0_f64),
        ),
        (
            "invalid confidence",
            "/guests/0/pending/0/observation/confidence",
            json!(1.1_f64),
        ),
        (
            "wrong correlated body",
            "/guests/0/pending/0/observation/track",
            json!(2_u64),
        ),
        (
            "mismatched pulse",
            "/guests/0/pending/0/observation/pulse",
            json!(0_u64),
        ),
        ("physical node count", "/guests/0/physical_pose", json!([])),
        (
            "physical scale override",
            "/guests/0/physical_pose/0/scale",
            json!([0.9_f32, 0.9_f32, 0.9_f32]),
        ),
        (
            "physical translation bound",
            "/guests/0/physical_pose/0/translation",
            json!([0.0_f32, 11.0_f32, 0.0_f32]),
        ),
    ];
    for (name, pointer, value) in negative {
        let mut invalid = encoded.clone();
        if name == "unknown record field" {
            let _previous = invalid
                .pointer_mut("/guests/0")
                .and_then(Value::as_object_mut)
                .ok_or("Missing guest record")?
                .insert("unrecognized".into(), value);
        } else {
            *invalid
                .pointer_mut(pointer)
                .ok_or("Fixture must save a pending body echo")? = value;
        }
        let refused = serde_json::from_value::<Snapshot>(invalid).map_or(true, |bad| {
            EntityWorld::from_level(level)
                .restore_checkpoint(&bad)
                .is_err()
        });
        checks.push(require(refused, name)?);
    }
    checks.extend(lifecycle(&mut world, ctx)?);
    Ok(checks)
}
fn lifecycle(world: &mut EntityWorld, ctx: &WorldContext<'_>) -> Result<Vec<Value>, String> {
    let _disabled = world.dispatch_on("guest", &[ActionDef::Disable { target: None }]);
    let _commands = world.take_commands();
    tick(world, ctx);
    let off = state(world)?;
    let mut checks = vec![require(
        off.get("life") == Some(&json!("disabled"))
            && off.get("pending") == Some(&json!([]))
            && off.get("voice") == Some(&Value::Null),
        "disable clears pending echoes and logical head voice",
    )?];
    let _enabled = world.dispatch_on("guest", &[ActionDef::Enable { target: None }]);
    tick(world, ctx);
    checks.push(require(
        state(world)?.get("life") == Some(&json!("active")),
        "clear retained pose re-enables without teleport",
    )?);
    let _removed = world.dispatch_on(
        "guest",
        &[ActionDef::DespawnEntity {
            target: "guest".into(),
        }],
    );
    checks.push(require(
        world.handle_of("guest").is_none()
            && world
                .guest_diagnostics()
                .as_array()
                .is_some_and(Vec::is_empty),
        "despawn removes guest ownership",
    )?);
    Ok(checks)
}
fn cosmetic(level: &LevelDef, ctx: &WorldContext<'_>) -> Result<Vec<Value>, String> {
    let mut world = EntityWorld::from_level(level);
    let mut sounds = Vec::new();
    for _ in 0_u8..90 {
        let _tick = world.tick(ctx);
        sounds.extend(world.take_guest_sounds());
    }
    let (key, mut sound) = sounds
        .into_iter()
        .find(|(_, s)| s.forward.is_some() && !s.stop)
        .ok_or("Missing native head sound")?;
    let epoch = Some(sound.epoch);
    let mut fence = Fence::default();
    sound.sequence = 101;
    sound.update = true;
    sound.cursor = 0.4_f64;
    let mut checks = vec![require(
        fence.admit(key, epoch, &sound),
        "desired update admitted after dropped start",
    )?];
    sound.sequence = 100;
    sound.update = false;
    checks.push(require(
        !fence.admit(key, epoch, &sound),
        "older reordered start rejected",
    )?);
    sound.sequence = 102;
    sound.stop = true;
    checks.push(require(
        fence.admit(key, epoch, &sound),
        "current generation stop admitted",
    )?);
    sound.sequence = 103;
    sound.stop = false;
    sound.update = true;
    checks.push(require(
        !fence.admit(key, epoch, &sound),
        "stopped generation cannot revive",
    )?);
    sound.sequence = 104;
    sound.generation = sound.generation.saturating_add(1);
    checks.push(require(
        fence.admit(key, epoch, &sound),
        "new logical generation admitted",
    )?);
    checks.push(require(
        !fence.admit(key, Some(sound.epoch.saturating_add(1)), &sound),
        "old world epoch rejected",
    )?);
    Ok(checks)
}
pub fn run(fixture: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err("Use a new state replay output".into());
    }
    let bytes = super::replay::bounded(fixture)?;
    let level: LevelDef = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let walls = level.item_collision_aabbs();
    let index = CollisionIndex::build(&walls);
    let floor = WalkableFloor::from_level(&level);
    let feet = Vec3::new(0.0, 0.0, 8.0);
    let ctx = WorldContext {
        delta_seconds: 1.0 / 120.0,
        player_walk_speed: 3.0,
        feet_from: feet,
        feet,
        eye: Vec3::new(0.0, 1.6, 8.0),
        body_height: 1.8,
        body_half: 0.4,
        form_model: None,
        walls: &walls,
        index: &index,
        floor: &floor,
        nav: None,
    };
    let started = std::time::Instant::now();
    let mut checks = round_trip(&level, &ctx)?;
    checks.extend(cosmetic(&level, &ctx)?);
    let receipt = json!({"scope":"Native CPU controller, imported posed model, serialization and cosmetic delivery fence; actual GPU/device lifecycle evidence is separate","seconds":started.elapsed().as_secs_f64(),"fixture_sha256":places::engine::package::hash::sha256_hex(&bytes),"checks":checks});
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
