//! Two bounded CPU regressions protect the new context/ownership boundary:
//! full-footprint feature placement and atomic baseline/history persistence.
//! Tiny JSON/two-entry catalog fixtures; no shipped catalog, images, GPU, mesh or bake.
use super::{Document, MEMBER, OWNER, ObjectId, Parameters, Value, objects, record};
use crate::editor::document::Transform;
use crate::editor::reuse::array;
use crate::engine::level::{DecalSurface, LevelSurfaces};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn open() -> Result<(Self, Document), String> {
        let path = std::env::temp_dir().join(format!(
            "places-details-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut doc = Document::create(&path, "entry", "Detail regression")?;
        let initial = doc.raw().clone();
        let raw = json!({"schema":"places-level","format_version":1_u32,"id":"entry","name":"Details","spawn":{"x":3.0_f32,"z":3.0_f32},"rooms":[{"width":6.0_f32,"depth":6.0_f32,"height":3.0_f32}],"walls":[
            {"x":0.0_f32,"z":0.0_f32,"width":6.0_f32,"depth":0.2_f32,"height":3.0_f32,"openings":[{"offset":2.5_f32,"width":1.0_f32,"height":2.2_f32,"kind":"door"},{"offset":1.0_f32,"width":0.8_f32,"height":0.4_f32,"sill":2.2_f32,"kind":"vent"}]},
            {"x":0.0_f32,"z":5.8_f32,"width":6.0_f32,"depth":0.2_f32,"height":3.0_f32},
            {"x":0.0_f32,"z":0.0_f32,"width":0.2_f32,"depth":6.0_f32,"height":3.0_f32},
            {"x":5.8_f32,"z":0.0_f32,"width":0.2_f32,"depth":6.0_f32,"height":3.0_f32}],
            "decals":[{"x":4.0_f32,"y":0.0_f32,"z":3.0_f32,"width":0.4_f32,"height":0.4_f32,"material":"core:decal_arrow_01","surface":"floor","future_field":"preserve"}]});
        doc.apply_authoring_edit(&initial, raw)?;
        Ok((Self(path), doc))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}
fn parameters() -> Parameters {
    Parameters {
        preset: 0,
        density: 1.0,
        intensity: 0.35,
        scale: 1.0,
        variation: 0.2,
        seed: 12345,
    }
}
fn apply(doc: &mut Document, raw: Value) -> Result<(), String> {
    doc.apply_authoring_edit(&doc.raw().clone(), raw)
}

fn candidate(
    doc: &Document,
    selected: ObjectId,
    parameters: Parameters,
) -> Result<(Value, String, Option<ObjectId>), String> {
    super::candidate(
        doc,
        selected,
        parameters,
        &crate::engine::assets::AssetCatalog::builtin(),
    )
}

fn check_receivers(level: &crate::engine::level::LevelDef) -> Result<(), String> {
    for decal in level.decals.iter().skip(1) {
        assert!(
            (decal.width - decal.height).abs() < 0.00001,
            "fitted square artwork retains aspect"
        );
        if decal.surface == DecalSurface::Floor {
            assert!(
                crate::editor::document::signs::floor_fit(level, decal, ObjectId::Room(0)),
                "complete floor footprint remains supported"
            );
        } else {
            let mut fitted = decal.clone();
            crate::editor::document::signs::fit(
                level,
                &mut fitted,
                0,
                decal.surface,
                glam::Vec3::new(decal.x, decal.y, decal.z),
                None,
            )?;
            assert!(
                (fitted.x - decal.x).abs()
                    + (fitted.y - decal.y).abs()
                    + (fitted.z - decal.z).abs()
                    < 0.00001,
                "no decal spans an opening or requires relocation"
            );
        }
        let corners = crate::editor::document::signs::corners(decal).ok_or("quad")?;
        let normal = glam::Vec3::from_array(decal.surface.normal());
        assert!(
            (corners
                .first()
                .ok_or("corner")?
                .to_array()
                .into_iter()
                .zip([decal.x, decal.y, decal.z])
                .zip(normal.to_array())
                .map(|((coordinate, center), axis)| (coordinate - center) * axis)
                .sum::<f32>()
                - crate::engine::render::DECAL_SURFACE_OFFSET_M)
                .abs()
                < 0.00001,
            "one shared renderer bias only"
        );
    }
    Ok(())
}

fn check_structure(before: &Value, after: &Value) -> Result<(), String> {
    for key in ["walls", "spawn"] {
        assert_eq!(
            after.get(key),
            before.get(key),
            "contextual details leave structure unchanged"
        );
    }
    let mut room = after.pointer("/rooms/0").ok_or("room")?.clone();
    let fields = room.as_object_mut().ok_or("room fields")?;
    drop(fields.remove(OWNER));
    drop(fields.remove("editor_instance_id"));
    assert_eq!(
        Some(&room),
        before.pointer("/rooms/0"),
        "all room geometry and unknown fields remain exact"
    );
    Ok(())
}
fn check_manual_copy(doc: &mut Document) -> Result<(), String> {
    let id = doc.duplicate(ObjectId::Decal(1))?;
    let copied = record(doc.raw(), id)?;
    assert!(
        copied.get(MEMBER).is_none(),
        "ordinary duplicates never inherit generated ownership"
    );
    let ObjectId::Decal(index) = id else {
        return Err("decal copy".into());
    };
    assert!(
        crate::editor::document::signs::floor_fit(
            doc.level(),
            doc.level().decals.get(index).ok_or("copy")?,
            id
        ),
        "independent copies retain complete support"
    );
    Ok(())
}

fn check_cleared_scope_prefab(doc: &mut Document) -> Result<(), String> {
    let mut cleared = doc.raw().clone();
    *cleared.get_mut("decals").ok_or("decals")? = json!([]);
    apply(doc, cleared)?;
    let (captured, _, _) = crate::editor::reuse::prefabs::capture(doc, 0, "cleared_detail_room")?;
    apply(doc, captured)?;
    let (stamped, _, selection) =
        crate::editor::reuse::prefabs::stamp(doc, "cleared_detail_room", [12.0, 0.0], false)?;
    apply(doc, stamped)?;
    let copy = record(doc.raw(), selection.ok_or("stamped room")?)?;
    assert!(
        copy.get("editor_instance_id").is_none() && copy.get(OWNER).is_none(),
        "prefab copies cannot alias the cleared detail scope or its placement identity"
    );
    Ok(())
}

fn check_door_storey_context() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::open()?;
    let mut raw = doc.raw().clone();
    objects::array_mut(&mut raw, "rooms")?
        .push(json!({"width":6.0_f32,"depth":6.0_f32,"height":3.0_f32,"floor_y":4.0_f32}));
    objects::array_mut(&mut raw,"walls")?.push(json!({"x":0.0_f32,"y":4.0_f32,"z":0.0_f32,"width":6.0_f32,"depth":0.2_f32,"height":3.0_f32,"openings":[{"offset":2.5_f32,"width":1.0_f32,"height":2.2_f32,"kind":"door"}]}));
    let level = doc.validate_candidate(&raw)?;
    let floor = LevelSurfaces::new(&level)
        .floor_y_at(2.5, 0.1)
        .ok_or("door floor")?;
    objects::array_mut(&mut raw,"doors")?.push(json!({"id":"upstairs_door","x":2.5_f32,"z":0.1_f32,"y":4.0_f32-floor,"width":1.0_f32,"height":2.2_f32}));
    apply(&mut doc, raw)?;
    let mut controls = parameters();
    controls.preset = 3;
    let (generated, _, _) = candidate(&doc, ObjectId::Door(0), controls)?;
    let receiving = doc.validate_candidate(&generated)?;
    let marks: Vec<_> = receiving
        .decals
        .iter()
        .filter(|decal| decal.material.starts_with("core:detail_"))
        .collect();
    assert!(
        !marks.is_empty() && marks.iter().all(|decal| decal.y >= 4.0),
        "a selected upstairs door cannot decorate unrelated lower-storey openings at the same X/Z"
    );
    let mut terminal_raw = doc.raw().clone();
    objects::array_mut(&mut terminal_raw,"props")?.push(json!({"id":"elevated_terminal","model":"core:ceiling_diffuser_01","x":4.5_f32,"z":4.5_f32,"y":6.935_f32-floor,"solid":false}));
    apply(&mut doc, terminal_raw)?;
    controls.preset = 2;
    if let Ok((proposed, _, _)) = candidate(&doc, ObjectId::Prop(0), controls) {
        let terminal_receiving = doc.validate_candidate(&proposed)?;
        assert!(
            terminal_receiving
                .decals
                .iter()
                .filter(|decal| decal.material.starts_with("core:detail_"))
                .all(|decal| decal.y >= 4.0),
            "an elevated terminal must not decorate the unrelated lower floor"
        );
    }
    Ok(())
}

fn check_resolved_vent_context() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::open()?;
    let catalog=crate::engine::assets::AssetCatalog::from_json_str(&json!({"format_version":2_u32,"themes":[],"assets":[
        {"id":"fixture:tex","asset_class":"environment","asset_type":"texture","source":"file","model":"fixture.png"},
        {"id":"fixture:ceiling","asset_class":"environment","asset_type":"material","source":"definition","surface":"ceiling","texture":"fixture:tex","tile_metres":1.0_f32,"grid_metres":1.0_f32}
    ]}).to_string())?;
    let mut raw = doc.raw().clone();
    let room = objects::object_mut(&mut raw, ObjectId::Room(0))?;
    drop(room.insert("ceiling_material".into(), json!("fixture:ceiling")));
    drop(room.insert("ceiling_tile_rotation_degrees".into(), json!(90.0_f32)));
    objects::array_mut(&mut raw,"decals")?.push(json!({"x":1.08_f32,"z":1.12_f32,"width":0.6_f32,"height":0.6_f32,"material":"core:decal_ceiling_vent_01","surface":"ceiling","align":"ceiling_grid"}));
    apply(&mut doc, raw)?;
    let before = doc.raw().clone();
    let mut controls = parameters();
    controls.preset = 2;
    controls.variation = 0.0;
    let (proposed, _, _) = super::candidate(&doc, ObjectId::Decal(1), controls, &catalog)?;
    let receiving = doc.validate_candidate(&proposed)?;
    let mark = receiving
        .decals
        .iter()
        .find(|decal| decal.material == "core:detail_vent_stain_01")
        .ok_or("resolved vent mark")?;
    assert!(
        (mark.x - 1.5).abs() < 0.00001
            && (mark.z - 1.5).abs() < 0.00001
            && (mark.rotation_degrees + 90.0).abs() < 0.00001,
        "stain follows the actual gridded/rotated ceiling vent, not its unsnapped source pose"
    );
    assert_eq!(doc.raw(), &before, "context preparation is private");
    let mut banner_raw = before;
    objects::array_mut(&mut banner_raw, "props")?.push(
        json!({"model":"fixture:event_banner","x":1.5_f32,"y":2.0_f32,"z":0.21_f32,"solid":false}),
    );
    apply(&mut doc, banner_raw)?;
    assert!(
        candidate(&doc, ObjectId::Prop(0), controls).is_err(),
        "event-named objects are not mistaken for vents"
    );
    Ok(())
}

#[test]
fn contextual_details_fit_solid_receivers_preserve_geometry_and_refuse_bad_inputs()
-> Result<(), String> {
    check_door_storey_context()?;
    check_resolved_vent_context()?;
    let (_fixture, mut doc) = Fixture::open()?;
    let original = doc.raw().clone();
    let (raw, _, _) = candidate(&doc, ObjectId::Room(0), parameters())?;
    assert_eq!(
        doc.raw(),
        &original,
        "private generation has no source/history side effect"
    );
    check_structure(&original, &raw)?;
    assert_eq!(
        raw.pointer("/decals/0"),
        original.pointer("/decals/0"),
        "manual unknown data remains exact"
    );
    let level = doc.validate_candidate(&raw)?;
    for material in [
        "core:detail_floor_grime_01",
        "core:detail_vent_stain_01",
        "core:detail_door_wear_01",
    ] {
        assert!(
            level.decals.iter().any(|d| d.material == material),
            "every contextual feature must produce a detail"
        );
    }
    check_receivers(&level)?;
    let floor_detail = level
        .decals
        .iter()
        .find(|d| d.material == "core:detail_floor_grime_01")
        .ok_or("floor detail")?;
    let mut stepped = level.clone();
    stepped.floor_regions.push(serde_json::from_value(json!({"x":floor_detail.x-0.001_f32,"z":floor_detail.z-0.001_f32,"width":0.002_f32,"depth":0.002_f32,"offset_y":-0.2_f32})).map_err(|error|error.to_string())?);
    assert!(
        !crate::editor::document::signs::floor_fit(&stepped, floor_detail, ObjectId::Room(0)),
        "a tiny interior recess is rejected even when all four corners stay level"
    );
    apply(&mut doc, raw)?;
    let accepted = doc.raw().clone();
    assert!(
        doc.level().decals.iter().enumerate().all(|(index, _)| doc
            .objects()
            .iter()
            .any(|object| object.id == ObjectId::Decal(index))),
        "every floor and wall detail is selectable and lockable"
    );
    let mut invalid = parameters();
    invalid.scale = f32::NAN;
    assert!(
        candidate(&doc, ObjectId::Room(0), invalid).is_err(),
        "invalid controls fail before mutation"
    );
    let mut bad = accepted.clone();
    drop(
        objects::object_mut(&mut bad, ObjectId::Decal(0))?
            .insert("opacity".into(), json!(1.01_f32)),
    );
    assert!(
        apply(&mut doc, bad).is_err(),
        "opacity contract is enforced by shared source validation"
    );
    assert_eq!(
        doc.raw(),
        &accepted,
        "failed candidates preserve current document"
    );
    let surfaces = LevelSurfaces::new(doc.level());
    assert!(
        surfaces.floor_y_at(3.0, 3.0).is_some(),
        "receiving room remains intact"
    );
    Ok(())
}

#[test]
fn regeneration_preserves_customization_deletion_history_and_reopen() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::open()?;
    let initial = doc.raw().clone();
    let (generated, _, _) = candidate(&doc, ObjectId::Room(0), parameters())?;
    apply(&mut doc, generated)?;
    let accepted = doc.raw().clone();
    assert!(
        doc.undo() && doc.raw() == &initial,
        "one undo reverses all generated records and ownership"
    );
    assert!(
        doc.redo() && doc.raw() == &accepted,
        "redo restores the exact generation"
    );
    let (repeat, _, _) = candidate(&doc, ObjectId::Room(0), parameters())?;
    assert_eq!(
        repeat, accepted,
        "repeated application cannot accumulate duplicates"
    );
    let mut changed = parameters();
    changed.intensity = 0.7;
    changed.seed = 42;
    let (different, _, _) = candidate(&doc, ObjectId::Room(0), changed)?;
    apply(&mut doc, different)?;
    let (replay, _, _) = candidate(&doc, ObjectId::Room(0), parameters())?;
    assert_eq!(
        replay, accepted,
        "A/B/A parameters regenerate from the same contextual baseline"
    );
    apply(&mut doc, replay)?;
    let id = ObjectId::Decal(1);
    let mut pose: Transform = doc.transform(id)?;
    pose.position[0] += 0.02;
    doc.set_transform(id, pose)?;
    let custom = record(doc.raw(), id)?.clone();
    let mut deleted = doc.raw().clone();
    let removed = objects::array_mut(&mut deleted, "decals")?.remove(2);
    apply(&mut doc, deleted)?;
    let (regenerated, _, _) = candidate(&doc, ObjectId::Room(0), changed)?;
    apply(&mut doc, regenerated)?;
    assert!(
        array(doc.raw(), "decals").contains(&custom),
        "manual transforms remain exact on regeneration"
    );
    assert!(
        !array(doc.raw(), "decals")
            .iter()
            .any(|d| d.get(MEMBER) == removed.get(MEMBER)),
        "manual deletion becomes a persistent tombstone"
    );
    let before_save = doc.raw().clone();
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        &before_save,
        "ordinary save/reopen preserves source, overrides and tombstones"
    );
    let (again, _, _) = candidate(&reopened, ObjectId::Room(0), changed)?;
    assert_eq!(again, before_save, "reopened regeneration is stable");
    check_manual_copy(&mut doc)?;
    check_cleared_scope_prefab(&mut doc)?;
    Ok(())
}
