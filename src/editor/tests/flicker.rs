//! Tiny source-only scenes protect ownership, circuit scope and executable Luau.
//! No bake, GPU, audio or shipped-map dependency; each case costs milliseconds.
use crate::editor::document::{Document, ObjectId, flicker::Flicker};
use crate::engine::{
    collision_index::CollisionIndex,
    entities::{EntityWorld, WorldContext},
    level::{ActionDef, LevelDef, WalkableFloor},
    project::ProjectPath,
    scripting::ScriptSource,
};
use glam::Vec3;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<(Self, Document), String> {
        let root = std::env::temp_dir().join(format!(
            "places-flicker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let mut doc = Document::create(&root.join("project"), "flicker_test", "Flicker test")?;
        let old = doc.raw().clone();
        let mut raw = old.clone();
        *raw.get_mut("ceiling_lights").ok_or("Missing fixtures")? = json!([
            {"id":"solo","fixture":"core:fluorescent_panel_01","x":1.0_f32,"z":1.0_f32},
            {"id":"pair_a","fixture":"core:fluorescent_panel_01","x":3.0_f32,"z":1.0_f32,"switch_group":"pair","switchable":true},
            {"id":"pair_b","fixture":"core:fluorescent_panel_01","x":5.0_f32,"z":1.0_f32,"switch_group":"pair","switchable":true}]);
        doc.apply_authoring_edit(&old, raw)?;
        Ok((Self(root), doc))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn flicker_scope_glow_and_undo_survive_save_reopen() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    let before = doc.raw().clone();
    let solo = Flicker {
        enabled: true,
        ..Flicker::default()
    };
    doc.apply_flicker(ObjectId::Light(0), &solo, false)?;
    assert!(
        doc.level()
            .ceiling_lights
            .first()
            .ok_or("No solo")?
            .switchable
    );
    assert_eq!(
        doc.raw().pointer("/ceiling_lights/1"),
        before.pointer("/ceiling_lights/1"),
        "same asset is not same fixture"
    );
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    assert!(doc.redo());
    let circuit = Flicker {
        enabled: true,
        circuit: true,
        ..Flicker::default()
    };
    doc.apply_flicker(ObjectId::Light(1), &circuit, false)?;
    assert_eq!(
        doc.flicker_script(ObjectId::Light(1))?,
        doc.flicker_script(ObjectId::Light(2))?
    );
    assert_ne!(
        doc.flicker_script(ObjectId::Light(0))?,
        doc.flicker_script(ObjectId::Light(1))?
    );
    let script = doc.flicker_script(ObjectId::Light(0))?;
    doc.apply_glow("core:fluorescent_panel_01", true, 7.5, 0.6, 0.0)?;
    assert_eq!(
        doc.level().animated_emissions.len(),
        1,
        "one global asset effect"
    );
    assert_eq!(
        doc.flicker_script(ObjectId::Light(0))?,
        script,
        "effects do not replace one another"
    );
    doc.check_scripts()?;
    doc.save_all()?;
    let reopened = Document::open(&fixture.0.join("project"))?;
    assert_eq!(doc.raw(), reopened.raw());
    Ok(())
}

#[test]
fn flicker_custom_regeneration_and_invalid_scope_are_atomic() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    let settings = Flicker {
        enabled: true,
        circuit: true,
        ..Flicker::default()
    };
    doc.apply_flicker(ObjectId::Light(1), &settings, false)?;
    let path = doc
        .flicker_script(ObjectId::Light(1))?
        .ok_or("No controller")?;
    let custom = format!(
        "{}\n-- user-owned timing\n",
        doc.script_text(&path).ok_or("No buffer")?
    );
    doc.set_script_text(&path, custom.clone())?;
    let before = doc.raw().clone();
    assert!(
        doc.apply_flicker(ObjectId::Light(2), &settings, false)
            .is_err()
    );
    assert_eq!(doc.raw(), &before);
    doc.apply_flicker(ObjectId::Light(2), &settings, true)?;
    assert_eq!(doc.script_text(&path), Some(custom.as_str()));
    assert!(!doc.script_paths().contains(&path));
    let accepted = doc.raw().clone();
    let mut changed = accepted.clone();
    drop(
        changed
            .pointer_mut("/ceiling_lights/0")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("Missing solo")?
            .insert("switch_group".into(), json!("pair")),
    );
    assert!(
        doc.apply_authoring_edit(&accepted, changed).is_err(),
        "cannot silently expand script scope"
    );
    assert_eq!(doc.raw(), &accepted);
    let invalid = Flicker {
        dark: f32::NAN,
        ..settings.clone()
    };
    assert!(
        doc.apply_flicker(ObjectId::Light(2), &invalid, false)
            .is_err()
    );
    let off = Flicker {
        enabled: false,
        ..settings
    };
    let before_link = doc.raw().clone();
    let mut linked = before_link.clone();
    let module = doc
        .flicker_script(ObjectId::Light(2))?
        .ok_or("No generated module")?;
    drop(linked.as_object_mut().ok_or("No object")?.insert("volumes".into(),json!([{"id":"zone","x":0.0_f32,"z":0.0_f32,"width":1.0_f32,"depth":1.0_f32,"bindings":[{"on":"enter_volume","actions":[{"action":"run_script","script":module,"handler":"mute"}]}]}])));
    doc.apply_authoring_edit(&before_link, linked.clone())?;
    assert!(
        doc.apply_flicker(ObjectId::Light(1), &off, true).is_err(),
        "explicit takeover cannot orphan another event"
    );
    assert_eq!(doc.raw(), &linked);
    doc.apply_authoring_edit(&linked, before_link)?;

    doc.apply_flicker(ObjectId::Light(1), &off, false)?;
    assert!(doc.flicker_script(ObjectId::Light(2))?.is_none());
    doc.save_all()?;
    assert_eq!(
        fs::read_to_string(fixture.0.join("project").join(path)).map_err(|e| e.to_string())?,
        custom
    );
    Ok(())
}

fn light(world: &EntityWorld, id: &str) -> Result<bool, String> {
    world
        .handle_of(id)
        .and_then(|h| world.components().lights.get(h))
        .map(|l| l.enabled)
        .ok_or_else(|| format!("Missing light {id}"))
}
fn tick(world: &mut EntityWorld, level: &LevelDef, seconds: f32) {
    let index = CollisionIndex::build(&[]);
    let floor = WalkableFloor::from_level(level);
    let _tick = world.tick(&WorldContext {
        delta_seconds: seconds,
        feet_from: Vec3::splat(3.0),
        feet: Vec3::splat(3.0),
        eye: Vec3::splat(3.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    });
}
fn invoke(world: &mut EntityWorld, path: &ProjectPath, handler: &str) {
    let _report = world.dispatch_on(
        "pair_a",
        &[ActionDef::RunScript {
            script: path.clone(),
            handler: handler.into(),
        }],
    );
}

#[test]
fn flicker_generated_handlers_use_real_circuit_dispatch_and_reset() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::new()?;
    let settings = Flicker {
        enabled: true,
        circuit: true,
        steady: 0.2,
        dark: 0.2,
        gap: 0.1,
        ..Flicker::default()
    };
    doc.apply_flicker(ObjectId::Light(1), &settings, false)?;
    let name = doc
        .flicker_script(ObjectId::Light(1))?
        .ok_or("Missing script")?;
    let path = ProjectPath::new(name.clone())?;
    let mut level = doc.level().clone();
    level.script_sources.push(ScriptSource {
        path: path.clone(),
        source: doc.script_text(&name).ok_or("Missing source")?.into(),
    });
    crate::engine::scripting::check_level(&level)?;
    let mut world = EntityWorld::from_level(&level);
    for _ in 0_u8..15 {
        tick(&mut world, &level, 0.02);
    }
    assert!(!light(&world, "pair_a")?);
    assert!(!light(&world, "pair_b")?);
    assert!(
        light(&world, "solo")?,
        "same asset outside circuit stays on"
    );
    invoke(&mut world, &path, "pause");
    for _ in 0_u8..30 {
        tick(&mut world, &level, 0.02);
    }
    assert!(!light(&world, "pair_a")?, "pause holds current phase/state");
    invoke(&mut world, &path, "resume");
    for _ in 0_u8..8 {
        tick(&mut world, &level, 0.02);
    }
    assert!(light(&world, "pair_a")?);
    assert!(light(&world, "pair_b")?);
    invoke(&mut world, &path, "mute");
    for _ in 0_u8..40 {
        tick(&mut world, &level, 0.02);
    }
    assert!(!light(&world, "pair_a")?);
    invoke(&mut world, &path, "reset");
    assert!(light(&world, "pair_a")?);
    invoke(&mut world, &path, "mute");
    world.reset_runtime();
    assert!(
        light(&world, "pair_a")?,
        "world reset restores authored state and cancels closure state"
    );
    for _ in 0_u8..15 {
        tick(&mut world, &level, 0.02);
    }
    assert!(
        !light(&world, "pair_a")?,
        "fresh VM starts the pattern again"
    );
    Ok(())
}

// Adds the authored-off/paused-start boundary to the real-dispatch test: two
// metadata-only worlds, without baking or renderer work.
#[test]
fn flicker_paused_start_and_reset_preserve_authored_off() -> Result<(), String> {
    for authored_on in [false, true] {
        let (_fixture, mut doc) = Fixture::new()?;
        let before = doc.raw().clone();
        let mut raw = before.clone();
        for index in [1_usize, 2] {
            drop(
                raw.get_mut("ceiling_lights")
                    .and_then(serde_json::Value::as_array_mut)
                    .and_then(|lights| lights.get_mut(index))
                    .and_then(serde_json::Value::as_object_mut)
                    .ok_or("Missing pair")?
                    .insert("enabled".into(), json!(authored_on)),
            );
        }
        doc.apply_authoring_edit(&before, raw)?;
        doc.apply_flicker(
            ObjectId::Light(1),
            &Flicker {
                enabled: true,
                circuit: true,
                autostart: false,
                ..Flicker::default()
            },
            false,
        )?;
        let name = doc
            .flicker_script(ObjectId::Light(1))?
            .ok_or("Missing script")?;
        let path = ProjectPath::new(name.clone())?;
        let mut level = doc.level().clone();
        level.script_sources.push(ScriptSource {
            path: path.clone(),
            source: doc.script_text(&name).ok_or("No source")?.into(),
        });
        let mut world = EntityWorld::from_level(&level);
        for _ in 0_u8..30 {
            tick(&mut world, &level, 0.02);
        }
        assert_eq!(light(&world, "pair_a")?, authored_on);
        invoke(&mut world, &path, "steady");
        assert!(light(&world, "pair_b")?);
        invoke(&mut world, &path, "reset");
        assert_eq!(light(&world, "pair_a")?, authored_on);
        invoke(&mut world, &path, "resume");
        assert!(
            light(&world, "pair_a")?,
            "explicit resume unmutes and starts the pattern"
        );
    }
    Ok(())
}
