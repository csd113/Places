//! Three small CPU regressions protect contracts absent from primitive tests:
//! linked three-way updates/circuit identity/history; room-only style geometry;
//! seeded baseline replay and real standing-hull clearance. One tiny source per
//! test, no shipped map, asset load, mesh, GPU, image decode, light bake or ordinary-CI bake.
use super::*;
use std::{fs, path::PathBuf};
fn document(name: &str) -> Result<(Document, PathBuf), String> {
    let path = std::env::temp_dir().join(format!("places-reuse-{name}-{}", std::process::id()));
    let mut doc = Document::create(&path, name, "Reuse regression")?;
    let initial = doc.raw().clone();
    let raw = json!({"schema":"places-level","format_version":1_u32,"id":name,"name":"Reuse regression","spawn":{"x":1.0_f32,"z":1.0_f32},
        "rooms":[{"x":0.0_f32,"z":0.0_f32,"width":8.0_f32,"depth":6.0_f32,"height":3.0_f32,"ceiling_tile_origin":[1.0_f32,1.0_f32]}],
        "walls":[{"x":0.0_f32,"z":0.0_f32,"width":8.0_f32,"depth":0.2_f32,"height":3.0_f32,"trim":{"placement":"both","height":0.1_f32,"thickness":0.02_f32}},
            {"x":2.0_f32,"z":4.0_f32,"width":3.0_f32,"depth":0.2_f32,"height":3.0_f32,"trim":{"placement":"bottom","height":0.1_f32,"thickness":0.02_f32},"openings":[{"kind":"door","offset":1.0_f32,"width":1.1_f32,"height":2.2_f32}]}],
        "props":[{"id":"chair","model":"core:chair","x":3.0_f32,"z":2.0_f32,"size":[1.0_f32,1.0_f32,1.0_f32],"solid":true},
            {"id":"switch","model":"home:wall_switch","x":1.0_f32,"z":0.3_f32,"size":[0.1_f32,0.1_f32,0.05_f32],"solid":false}],
        "ceiling_lights":[{"id":"light","fixture":"backrooms:light_fluorescent_recessed","x":4.0_f32,"z":3.0_f32,"align":"none"}]});
    doc.apply_authoring_edit(&initial, raw)?;
    Ok((doc, path))
}
fn apply(doc: &mut Document, raw: Value) -> Result<(), String> {
    let expected = doc.raw().clone();
    doc.apply_authoring_edit(&expected, raw)
}

#[test]
fn prefab_links_preserve_overrides_deletions_circuits_and_atomic_persistence() -> Result<(), String>
{
    let (mut doc, path) = document("prefab_regression")?;
    let result = (|| {
        doc.assign_circuit(&[0], 1)?;
        let (captured, _, _) = prefabs::capture(&doc, 0, "furnished")?;
        apply(&mut doc, captured)?;
        source_names_keep_member_identity(&mut doc)?;
        let initial = doc.raw().clone();
        let (stamped, _, selection) = prefabs::stamp(&doc, "furnished", [12.0, 0.0], true)?;
        apply(&mut doc, stamped)?;
        let stamped_raw = doc.raw().clone();
        assert!(
            doc.undo() && doc.raw() == &initial,
            "one undo removes complete stamp and metadata"
        );
        assert!(
            doc.redo() && doc.raw() == &stamped_raw,
            "redo restores exact membership and links"
        );
        assert_ne!(
            doc.level()
                .ceiling_lights
                .first()
                .and_then(|l| l.switch_group.as_ref()),
            doc.level()
                .ceiling_lights
                .get(1)
                .and_then(|l| l.switch_group.as_ref()),
            "stamps own independent real circuit groups"
        );
        let mut overrides = doc.raw().clone();
        drop(
            objects::object_mut(&mut overrides, ObjectId::Prop(2))?
                .insert("display_name".into(), json!("My override")),
        );
        drop(
            objects::object_mut(&mut overrides, ObjectId::Prop(0))?
                .insert("scale".into(), json!(0.8_f32)),
        );
        apply(&mut doc, overrides)?;
        let (updated_source, _, _) = prefabs::capture(&doc, 0, "furnished")?;
        apply(&mut doc, updated_source)?;
        let (updated, _, _) = prefabs::update(&doc, selection.ok_or("room selection")?)?;
        apply(&mut doc, updated)?;
        assert_eq!(
            doc.level().props.get(2).map(|p| p.scale),
            Some(0.8_f32),
            "unmodified pose follows source"
        );
        assert_eq!(
            array(doc.raw(), "props")
                .get(2)
                .and_then(|p| p.get("display_name"))
                .and_then(Value::as_str),
            Some("My override"),
            "instance field override survives source change"
        );
        let previous = doc.raw().clone();
        let (repeat, _, _) = prefabs::update(&doc, ObjectId::Room(1))?;
        assert_eq!(repeat, previous, "repeated update is idempotent");
        let mut deleted = doc.raw().clone();
        drop(objects::array_mut(&mut deleted, "props")?.remove(2));
        apply(&mut doc, deleted)?;
        let (after_deletion, _, _) = prefabs::update(&doc, ObjectId::Room(1))?;
        apply(&mut doc, after_deletion)?;
        assert_eq!(
            doc.level().props.len(),
            3,
            "deleted instance member remains a tombstone"
        );
        manual_copy_is_independent(&mut doc)?;
        let before = doc.raw().clone();
        doc.save()?;
        let reopened = Document::open(&path)?;
        assert_eq!(
            reopened.raw(),
            &before,
            "source definitions, links, baselines and overrides persist exactly"
        );
        source_and_carrier_refusals(&mut doc)?;
        Ok(())
    })();
    drop(doc);
    let _cleanup = fs::remove_dir_all(path);
    result
}
#[test]
fn room_style_preserves_geometry_openings_power_and_neighbor_scope() -> Result<(), String> {
    let (mut doc, path) = document("style_regression")?;
    let result = (|| {
        let before = doc.raw().clone();
        let (style, _, _) = styles::candidate(
            &doc,
            0,
            1,
            [true; 5],
            &crate::engine::assets::AssetCatalog::load_default(),
        )?;
        assert_eq!(doc.raw(), &before, "style preview leaves source unchanged");
        assert_eq!(
            style.pointer("/walls/1/openings"),
            before.pointer("/walls/1/openings"),
            "opening geometry is unrelated to style"
        );
        assert_eq!(
            style.get("props"),
            before.get("props"),
            "style does not restyle furniture"
        );
        assert_eq!(
            style.pointer("/walls/0/faces/north"),
            None,
            "exterior face remains outside the room scope"
        );
        assert_eq!(
            style.pointer("/walls/0/trim"),
            before.pointer("/walls/0/trim"),
            "shared boundary trim remains untouched"
        );
        let collision = doc.level().collision_aabbs();
        apply(&mut doc, style)?;
        assert_eq!(
            doc.level().collision_aabbs(),
            collision,
            "style cannot alter geometry or collision"
        );
        assert!(doc.undo() && doc.raw() == &before, "style is one undo step");
        Ok(())
    })();
    drop(doc);
    let _cleanup = fs::remove_dir_all(path);
    result
}
#[test]
fn variation_seed_replay_does_not_accumulate_and_protects_standing_clearance() -> Result<(), String>
{
    let (mut doc, path) = document("variation_regression")?;
    let result = (|| {
        let before = doc.raw().clone();
        let (varied, _, _) =
            variation::candidate(&doc, ObjectId::Prop(0), 42, [30.0, 0.4, 0.1], false, false)?;
        apply(&mut doc, varied)?;
        let accepted = doc.raw().clone();
        let (repeat, _, _) =
            variation::candidate(&doc, ObjectId::Prop(0), 42, [30.0, 0.4, 0.1], false, false)?;
        assert_eq!(
            repeat, accepted,
            "same seed/ranges replay original baseline without drift"
        );
        assert!(
            doc.undo() && doc.raw() == &before,
            "variation metadata and pose undo together"
        );
        let mut refused = false;
        for seed in 0_u64..16 {
            let (candidate, summary, _) =
                variation::candidate(&doc, ObjectId::Prop(0), seed, [90.0, 0.5, 0.0], true, false)?;
            if candidate == before {
                refused = true;
            }
            assert!(
                summary.contains("standing samples"),
                "protection reports actual standing-hull samples"
            );
            drop(doc.validate_candidate(&candidate)?);
        }
        assert!(
            refused,
            "at least one enlarged/rotated collision footprint is refused"
        );
        let (seed_a, _, _) =
            variation::candidate(&doc, ObjectId::Prop(0), 9, [40.0, 0.4, 0.05], true, false)?;
        apply(&mut doc, seed_a.clone())?;
        let (seed_b, _, _) =
            variation::candidate(&doc, ObjectId::Prop(0), 12, [40.0, 0.4, 0.05], true, false)?;
        apply(&mut doc, seed_b)?;
        let (seed_a_again, _, _) =
            variation::candidate(&doc, ObjectId::Prop(0), 9, [40.0, 0.4, 0.05], true, false)?;
        assert_eq!(
            seed_a_again, seed_a,
            "protected seed A/B/A replay uses the same common baseline"
        );
        apply(&mut doc, accepted)?;
        doc.save()?;
        let reopened = Document::open(&path)?;
        let (replayed, _, _) = variation::candidate(
            &reopened,
            ObjectId::Prop(0),
            42,
            [30.0, 0.4, 0.1],
            false,
            false,
        )?;
        assert_eq!(
            &replayed,
            reopened.raw(),
            "seed baseline survives save/reopen"
        );
        Ok(())
    })();
    drop(doc);
    let _cleanup = fs::remove_dir_all(path);
    result
}

fn manual_copy_is_independent(doc: &mut Document) -> Result<(), String> {
    let before = doc.raw().clone();
    let copy = doc.duplicate(ObjectId::Prop(0))?;
    let (collection, index) = copy.collection();
    let record = array(doc.raw(), collection)
        .get(index)
        .ok_or("Copy disappeared")?;
    assert!(
        record.get("editor_prefab").is_none()
            && record.get("editor_prefab_source").is_none()
            && record.get("editor_variation").is_none(),
        "manual duplication cannot alias prefab membership or a saved variation baseline"
    );
    assert!(
        doc.undo() && doc.raw() == &before,
        "manual copy and fresh identity undo atomically"
    );
    Ok(())
}

fn source_and_carrier_refusals(doc: &mut Document) -> Result<(), String> {
    let before = doc.raw().clone();
    let mut resized = before.clone();
    drop(
        objects::object_mut(&mut resized, ObjectId::Room(1))?
            .insert("width".into(), json!(9.0_f32)),
    );
    apply(doc, resized)?;
    assert!(
        prefabs::update(doc, ObjectId::Room(1)).is_err(),
        "carrier geometry overrides refuse rather than placing source children against the old carrier"
    );
    assert!(
        doc.undo() && doc.raw() == &before,
        "refused update leaves the carrier edit as the only history step"
    );
    let mut missing = doc.raw().clone();
    drop(
        missing
            .pointer_mut("/editor_reuse/prefabs")
            .and_then(Value::as_object_mut)
            .ok_or("sources")?
            .remove("furnished"),
    );
    apply(doc, missing)?;
    assert!(
        prefabs::update(doc, ObjectId::Room(1)).is_err(),
        "missing source cannot remove or change existing geometry"
    );
    let (detached, _, _) = prefabs::detach(doc, ObjectId::Room(1))?;
    apply(doc, detached)?;
    assert!(
        prefabs::update(doc, ObjectId::Room(1)).is_err(),
        "detached geometry has no automatic update link"
    );
    Ok(())
}

fn source_names_keep_member_identity(doc: &mut Document) -> Result<(), String> {
    let captured = std::collections::BTreeSet::from(["inside"]);
    let known = std::collections::BTreeSet::new();
    assert!(
        prefabs::check_references(
            &json!({"action":"play_sound","target":"external_audio"}),
            &captured,
            &known
        )
        .is_err(),
        "external action targets outside prop/door/light namespaces must not leak out of a room stamp"
    );

    let before = doc
        .raw()
        .pointer("/editor_reuse/prefabs/furnished/items")
        .cloned()
        .ok_or("Missing furnished source")?;
    let (other, _, _) = prefabs::capture(doc, 0, "alternate")?;
    apply(doc, other)?;
    let (again, _, _) = prefabs::capture(doc, 0, "furnished")?;
    apply(doc, again)?;
    assert_eq!(
        doc.raw().pointer("/editor_reuse/prefabs/furnished/items"),
        Some(&before),
        "capturing one room under another name cannot replace its stable source-member identities"
    );
    Ok(())
}
