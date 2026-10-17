//! One small project protects fog-specific raw transform/profile preservation,
//! independent copy identities, failed-edit history and save/reopen. Engine
//! tests cover extinction/budgets; this exercises only editor publication and
//! adds a handful of tiny local writes, no asset lookup, bake or GPU work.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::editor::document::{Document, ObjectKind};
use crate::engine::render::LevelFog;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fog_box_profile_and_ground_layer_roundtrip_without_losing_raw_data() -> Result<(), String> {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "places-editor-fog-{}-{sequence}",
        std::process::id()
    ));
    let mut document = Document::create(&path, "fog", "Fog fixture")?;
    let fixture = Fixture(path);
    let boxed = document.add(ObjectKind::FogRegion, [0.0, 3.0, 0.0], None)?;
    let ground = document.add(ObjectKind::GroundFog, [0.0, 3.0, 0.0], None)?;
    let before_unknown = document.raw().clone();
    let mut unknown = before_unknown.clone();
    let raw_fog = unknown
        .pointer_mut("/fog_regions/0")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("missing box record")?;
    drop(raw_fog.insert(
        "future_mist".into(),
        serde_json::json!({"preserve":[true,3_u32,"mist"]}),
    ));
    document.apply_checked_edit(&before_unknown, unknown)?;
    document.set_fog_density(boxed, 0.25)?;
    document.set_fog_color(ground, Some([0.2, 0.4, 0.6]))?;
    document.set_fog_falloff(ground, 0.2)?;
    let mut moved = document.transform(boxed)?;
    moved.position[1] = 7.0;
    moved.size[1] = 2.0;
    document.set_transform(boxed, moved)?;
    let profile = document.level().fog_regions.first().ok_or("missing box")?;
    assert_eq!(
        profile.ground_y,
        Some(9.0),
        "full box density follows the resized top"
    );
    assert!(
        profile
            .top_y
            .is_some_and(|height| (height - 9.0_f32).abs() < 1.0e-6_f32),
        "the top follows box resizing"
    );
    let resolved_fog = LevelFog::from_level(document.level());
    assert!(
        resolved_fog
            .regions
            .first()
            .ok_or("missing resolved box")?
            .density_at([2.0_f32, 4.0_f32, 2.0_f32])
            .abs()
            < 1.0e-6_f32,
        "moving the box to another storey removes its previous contribution"
    );
    let copy = document.duplicate(ground)?;
    assert_ne!(copy, ground, "copy selection addresses a new record");
    let first = document
        .level()
        .ground_fog
        .first()
        .ok_or("missing original ground")?;
    let second = document.level().ground_fog.get(1).ok_or("missing copy")?;
    assert_ne!(
        first.id, second.id,
        "copied layers have independent stable identities"
    );
    assert!(
        (second.min[0] - first.min[0] - 0.5).abs() < 1.0e-6,
        "copy offsets actual fog bounds"
    );
    let accepted = document.raw().clone();
    assert!(
        document.set_fog_falloff(ground, f32::NAN).is_err(),
        "NaN cannot silently remove optional softness"
    );
    assert!(
        document.set_fog_density(boxed, 0.51).is_err(),
        "density above the engine bound is refused"
    );
    assert_eq!(
        document.raw(),
        &accepted,
        "failed numeric edits cannot mutate optional fields"
    );
    assert!(
        document.undo(),
        "the failed edits do not consume copy's undo entry"
    );
    assert_eq!(
        document.level().ground_fog.len(),
        1,
        "undo removes only the duplicate"
    );
    assert!(document.redo(), "the valid copy remains redoable");
    assert_eq!(document.raw(), &accepted, "redo restores the exact source");
    document.save()?;
    assert_eq!(
        Document::open(&fixture.0)?.raw(),
        &accepted,
        "both fog types and unknown records roundtrip exactly"
    );
    Ok(())
}
