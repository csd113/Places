//! Two essential editor regressions cover previously unavailable multi-event
//! authoring and reference-safe identity/collision edits. They use only a tiny
//! source project, a one-part catalog shape and ordinary JSON validation;
//! no renderer, audio device, VM, model load or map bake runs.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::editor::document::components::{Field, Kind, Target};
use crate::editor::document::{Document, ObjectKind};
use crate::engine::assets::AssetCatalog;
use crate::engine::level::EventKindName;
use crate::engine::physics::config::{BodyMotion, ShapeDef};
use serde_json::json;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

// Protects the actual map-owned inventory document boundary: independent health
// opt-in, rejected tuning without edits, history and save/reopen. One tiny source
// project, a few JSON fields, no assets or package bake (milliseconds).
#[test]
fn map_tool_inventory_and_tuning_survive_editor_history_and_reopen() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    let id = crate::editor::document::ObjectId::Prop(0);
    assert!(!doc.level().tools.crowbar && !doc.level().health.enabled);
    doc.set_component_field(
        id,
        Field::new(Target::Tools, "crowbar", "Crowbar", Kind::Bool, "false"),
        "true",
    )?;
    for (key, value, kind) in [
        ("damage_hp", "3", Kind::WholeNumber),
        ("reach_m", "2.25", Kind::Number),
        ("swings_per_second", "2.5", Kind::Number),
    ] {
        doc.set_component_field(
            id,
            Field::new(Target::Tools, key, "Tuning", kind, "1"),
            value,
        )?;
    }
    let accepted = doc.raw().clone();
    assert!(
        doc.set_component_field(
            id,
            Field::new(Target::Tools, "reach_m", "Reach", Kind::Number, "1.75"),
            "5"
        )
        .is_err()
    );
    assert_eq!(doc.raw(), &accepted, "invalid tuning cannot mutate history");
    assert!(doc.undo() && doc.redo());
    assert_eq!(doc.raw(), &accepted);
    assert!(
        !doc.level().health.enabled,
        "granting a tool cannot opt into health"
    );
    doc.save_all()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(reopened.raw(), &accepted);
    assert_eq!(reopened.level().tools.damage_hp, 3);
    Ok(())
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<(Self, Document), String> {
        let path = std::env::temp_dir().join(format!(
            "places-component-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let doc = Document::create(&path, "components", "Tiny components")?;
        Ok((Self(path), doc))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _removed = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn selected_event_and_state_conditions_survive_undo_and_reopen() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    let prop = doc.add(ObjectKind::Prop, [1.0, 0.0, 1.0], Some("core:chair"))?;
    doc.rename_instance(prop, "switch")?;
    doc.add_state(prop, "ready")?;
    let trigger = doc.add(ObjectKind::Trigger, [1.0, 0.0, 1.0], None)?;
    doc.add_binding_action(
        trigger,
        0,
        EventKindName::EnterVolume,
        json!({"action":"set_state","target":"switch","name":"ready","value":true}),
    )?;
    doc.add_binding_action(
        trigger,
        1,
        EventKindName::ExitVolume,
        json!({"action":"set_state","target":"switch","name":"ready","value":false}),
    )?;
    let first = doc
        .raw()
        .pointer("/volumes/0/bindings/0")
        .cloned()
        .ok_or("Missing first event")?;
    doc.set_component_field(
        trigger,
        Field::new(
            Target::Binding(1),
            "cooldown_seconds",
            "Cooldown",
            Kind::Number,
            "0.0",
        ),
        "0.25",
    )?;
    doc.add_binding_condition(
        trigger,
        1,
        json!({"check":"state","target":"switch","name":"ready","equals":true}),
    )?;
    assert_eq!(
        doc.raw().pointer("/volumes/0/bindings/0"),
        Some(&first),
        "second-event forms cannot edit the first event"
    );
    doc.set_component_field(
        prop,
        Field::new(Target::State(0), "name", "State name", Kind::Text, "\"\""),
        "complete",
    )?;
    assert_eq!(
        doc.raw().pointer("/volumes/0/bindings/1/when/0/name"),
        Some(&json!("complete")),
        "state rename updates every typed condition/action reference"
    );
    assert_eq!(
        doc.raw().pointer("/volumes/0/bindings/0/actions/0/name"),
        Some(&json!("complete")),
        "state rename updates every typed condition/action reference"
    );
    let wired = doc.raw().clone();
    assert!(
        doc.add_binding_condition(
            trigger,
            1,
            json!({"check":"state","target":"missing","name":"ready","equals":true})
        )
        .is_err(),
        "an absent condition owner is refused"
    );
    assert_eq!(
        doc.raw(),
        &wired,
        "invalid references never consume source/history"
    );
    assert!(doc.undo(), "the state-reference edit is undoable");
    assert!(doc.redo(), "the state-reference edit is redoable");
    assert_eq!(doc.raw(), &wired, "redo restores exact source");
    doc.save_all()?;
    assert_eq!(
        Document::open(&fixture.0)?.raw(),
        &wired,
        "save/reopen preserves all selected-event edits"
    );
    Ok(())
}

#[test]
fn identity_filter_and_catalog_fixed_shape_edits_preserve_their_contracts() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::new()?;
    let prop = doc.add(ObjectKind::Prop, [1.0, 0.0, 1.0], Some("core:rack"))?;
    doc.rename_instance(prop, "rack")?;
    let catalog = AssetCatalog::from_json_str(
        r#"{"format_version":2,"assets":[{"id":"core:rack","display_name":"Rack","asset_class":"core","asset_type":"prop","source":"file","model":"shared/rack.glb","size":[1,1,1],"solid":true}]}"#,
    )?;
    let mut catalog_with_shape = catalog;
    catalog_with_shape.apply_gameplay_json(r#"{"format_version":1,"assets":{"core:rack":{"physics":{"motion":"fixed","shape":{"kind":"compound","parts":[{"offset":[0,0.1,0],"shape":{"kind":"cuboid","half_extents":[0.5,0.1,0.5]}}]}}}}}"#)?;
    doc.use_catalog_fixed_shape(prop, &catalog_with_shape)?;
    let profile = doc
        .level()
        .props
        .first()
        .and_then(|record| record.physics.as_ref())
        .ok_or("Missing fitted shape")?;
    assert_eq!(
        profile.motion,
        BodyMotion::Fixed,
        "catalog adoption keeps fitted support fixed"
    );
    assert!(
        !profile.pickup,
        "a fixed support cannot become a carried item"
    );
    assert!(
        matches!(profile.shape,Some(ShapeDef::Compound{ref parts}) if parts.len()==1),
        "the installed compound shape is retained"
    );
    let trigger = doc.add(ObjectKind::Trigger, [1.0, 0.0, 1.0], None)?;
    // A physical receipt requires a movable item; the rack remains its fitted support.
    let item = doc.add(ObjectKind::Prop, [2.0, 0.0, 1.0], Some("core:rack"))?;
    doc.rename_instance(item, "item")?;
    doc.set_gameplay_value(
        item,
        crate::editor::document::GameplaySetting::Physics,
        json!(true),
        &catalog_with_shape,
    )?;
    doc.set_trigger_filter(
        trigger,
        json!({"kind":"item","instance":"item","held":false}),
    )?;
    doc.set_trigger_filter(trigger, json!({"kind":"item","instance":"item"}))?;
    assert_eq!(
        doc.raw().pointer("/volumes/0/filter/held"),
        Some(&json!(false)),
        "identity picking retains the independent held policy"
    );
    let _refusal = doc
        .add_binding_action(
            trigger,
            0,
            EventKindName::EnterVolume,
            json!({"action":"toggle_label","target":"rack"}),
        )
        .err()
        .ok_or("Label target without capability should be rejected")?;
    doc.add_component(prop, "interactable")?;
    doc.add_binding_action(
        trigger,
        0,
        EventKindName::EnterVolume,
        json!({"action":"toggle_label","target":"rack"}),
    )?;
    doc.rename_instance(prop, "fitted_rack")?;
    assert_eq!(
        doc.raw().pointer("/volumes/0/bindings/0/actions/0/target"),
        Some(&json!("fitted_rack")),
        "instance rename updates action references"
    );
    let before = doc.raw().clone();
    assert!(
        doc.rename_instance(prop, "item").is_err(),
        "an existing identity cannot be reused"
    );
    assert_eq!(
        doc.raw(),
        &before,
        "duplicate identities are rejected atomically"
    );
    Ok(())
}

// A single tiny project protects the user-facing toggle's undo and raw-source
// custody. No assets, compiler, native window or full map preparation is needed.
#[test]
fn shadow_only_is_independent_of_collision_and_roundtrips() -> Result<(), String> {
    use crate::editor::document::GameplaySetting;
    let (fixture, mut doc) = Fixture::new()?;
    let catalog = AssetCatalog::default();
    let prop = doc.add(ObjectKind::Entity, [1.0, 0.0, 1.0], Some("mannequin"))?;
    let original = doc.level().props.first().ok_or("missing actor")?.solid;
    doc.set_gameplay_value(prop, GameplaySetting::ShadowOnly, json!(true), &catalog)?;
    assert!(
        doc.level()
            .props
            .first()
            .ok_or("missing actor")?
            .shadow_only
    );
    assert_eq!(
        doc.level().props.first().ok_or("missing actor")?.solid,
        original
    );
    assert!(doc.undo());
    assert!(
        !doc.level()
            .props
            .first()
            .ok_or("missing actor")?
            .shadow_only
    );
    assert!(doc.redo());
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert!(
        reopened
            .level()
            .props
            .first()
            .ok_or("missing reopened actor")?
            .shadow_only
    );
    Ok(())
}

/// Protects independent health switches, rejection without history mutation,
/// and persistence through the actual editor document workflow. One tiny source
/// project is sufficient; no map compilation or native renderer is involved.
#[test]
fn health_rules_and_kit_action_survive_editor_history_and_reopen() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    assert_eq!(
        doc.level().health,
        crate::engine::health::HealthSettings::default()
    );
    let initial = doc.raw().clone();
    let id = crate::editor::document::ObjectId::Prop(0);
    for (key, value) in [
        ("enabled", "true"),
        ("regeneration", "true"),
        ("health_kits", "false"),
        ("show_player_health", "false"),
    ] {
        doc.set_component_field(
            id,
            Field::new(Target::Health, key, "Health", Kind::Bool, "false"),
            value,
        )?;
    }
    for (key, value) in [
        ("regeneration_hp_per_second", "0.25"),
        ("regeneration_delay_seconds", "3.0"),
        ("damage_cooldown_seconds", "0.5"),
    ] {
        let field = Field::new(Target::Health, key, "Regeneration", Kind::Number, "0.0");
        doc.set_component_field(id, field, value)?;
        let valid = doc.raw().clone();
        for invalid in ["-1", "NaN", "inf"] {
            assert!(doc.set_component_field(id, field, invalid).is_err());
            assert_eq!(doc.raw(), &valid, "invalid health edits must be atomic");
        }
    }
    for (key, value) in [("player_max_hp", "10"), ("player_starting_hp", "8")] {
        doc.set_component_field(
            id,
            Field::new(Target::Health, key, "Health", Kind::WholeNumber, "6"),
            value,
        )?;
    }
    let edited = doc.raw().clone();
    for _ in 0..9_u8 {
        assert!(doc.undo());
    }
    assert_eq!(doc.raw(), &initial);
    for _ in 0..9_u8 {
        assert!(doc.redo());
    }
    assert_eq!(doc.raw(), &edited);
    assert!(doc.level().health.regeneration);
    assert!(!doc.level().health.health_kits);
    assert!(!doc.level().health.show_player_health);
    let prop = doc.add(ObjectKind::Prop, [1.0, 0.0, 1.0], Some("core:chair"))?;
    doc.add_binding_action(
        prop,
        0,
        EventKindName::Interact,
        json!({"action":"health","target":{"kind":"player"},"change":{"kind":"health_kit","hp":2_u16}}),
    )?;
    doc.rename_instance(prop, "kit")?;
    doc.add_binding_action(prop, 0, EventKindName::Interact,
        json!({"action":"health","target":{"kind":"entity","id":"kit"},"change":{"kind":"heal","hp":1_u16}}))?;
    doc.rename_instance(prop, "health_station")?;
    assert_eq!(
        doc.raw().pointer("/props/0/bindings/0/actions/1/target/id"),
        Some(&json!("health_station"))
    );
    let complete = doc.raw().clone();
    assert!(doc.undo());
    assert!(doc.redo());
    assert_eq!(doc.raw(), &complete);
    assert!(doc.add_binding_action(prop, 0, EventKindName::Interact,
        json!({"action":"health","target":{"kind":"entity","id":"missing"},"change":{"kind":"heal","hp":2_u16}})).is_err());
    assert_eq!(doc.raw(), &complete);
    doc.save_all()?;
    assert_eq!(Document::open(&fixture.0)?.raw(), &complete);
    Ok(())
}

/// Covers per-instance HP overrides and bounded whole-number authoring with one
/// tiny source project, including atomic rejection and normal history/persistence.
#[test]
fn individual_health_limits_are_validated_and_saved() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    let npc = doc.add(ObjectKind::Entity, [1.0, 0.0, 1.0], Some("mannequin"))?;
    doc.add_component(npc, "health")?;
    let maximum = Field::new(
        Target::Component("health"),
        "max_hp",
        "Maximum HP",
        Kind::WholeNumber,
        "6",
    );
    let starting = Field::new(
        Target::Component("health"),
        "starting_hp",
        "Starting HP",
        Kind::WholeNumber,
        "6",
    );
    doc.set_component_field(npc, maximum, "12")?;
    doc.set_component_field(npc, starting, "0")?;
    assert_eq!(doc.component_value(npc, starting), json!(0_u16));
    let valid = doc.raw().clone();
    for value in ["0", "-1", "65536", "2.5"] {
        assert!(doc.set_component_field(npc, maximum, value).is_err());
        assert_eq!(doc.raw(), &valid);
    }
    assert!(doc.set_component_field(npc, starting, "13").is_err());
    let player_maximum = Field::new(
        Target::Health,
        "player_max_hp",
        "Maximum HP",
        Kind::WholeNumber,
        "6",
    );
    assert!(doc.set_component_field(npc, player_maximum, "0").is_err());
    assert!(doc.set_component_field(npc, player_maximum, "5").is_err());
    assert_eq!(doc.raw(), &valid);
    assert!(doc.undo());
    assert!(doc.redo());
    assert_eq!(doc.raw(), &valid);
    doc.save_all()?;
    assert_eq!(Document::open(&fixture.0)?.raw(), &valid);
    Ok(())
}

// Protects independent nested zone tuning and ordinary-map opt-in through the
// actual editor history/save boundary. One tiny project; no assets or map bake.
#[test]
fn spawn_zones_keep_independent_tuning_across_history_and_save() -> Result<(), String> {
    use crate::editor::document::GameplaySetting as S;
    let (fixture, mut doc) = Fixture::new()?;
    let catalog = AssetCatalog::from_json_str(r#"{"format_version":2,"assets":[]}"#)?;
    let left = doc.add(ObjectKind::SpawnPoint, [1.0, 0.0, 1.0], Some("mannequin"))?;
    let right = doc.add(ObjectKind::SpawnPoint, [3.0, 0.0, 1.0], Some("mannequin"))?;
    for id in [left, right] {
        doc.set_gameplay_value(id, S::SpawnZone, json!(true), &catalog)?;
    }
    doc.set_gameplay_value(left, S::ZoneCooldown, json!(2.5_f32), &catalog)?;
    doc.set_gameplay_value(left, S::ZoneCount, json!(7.0_f64), &catalog)?;
    let accepted = doc.raw().clone();
    assert!(
        doc.set_gameplay_value(left, S::ZoneCount, json!(1.5_f64), &catalog)
            .is_err()
    );
    assert_eq!(
        doc.raw(),
        &accepted,
        "fractional counts cannot mutate the source"
    );
    assert_eq!(doc.gameplay_value(right, S::ZoneCooldown), json!(1.0_f32));
    assert_eq!(doc.gameplay_value(left, S::ZoneCount), json!(7_u16));
    assert!(doc.undo() && doc.redo());
    assert_eq!(doc.raw(), &accepted);
    doc.save_all()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(reopened.raw(), &accepted);
    assert!(
        !reopened.level().health.enabled && reopened.level().encounter.is_none(),
        "placing a zone never silently enables combat"
    );
    Ok(())
}
