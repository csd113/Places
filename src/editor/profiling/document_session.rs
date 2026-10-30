//! Explicit authoring trace; isolated project copies only, never tests or CI.
use std::{path::Path, time::Instant};

use serde_json::{Value, json};

use super::super::document::{Document, ObjectId, Transform};

fn elapsed(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0_f64
}

fn timed<T>(work: impl FnOnce() -> Result<T, String>) -> Result<(T, f64), String> {
    let started = Instant::now();
    let result = work()?;
    Ok((result, elapsed(started)))
}

fn restore(document: &mut Document, redo: bool) -> Result<(), String> {
    if if redo {
        document.redo()
    } else {
        document.undo()
    } {
        Ok(())
    } else {
        Err("Expected history revision was unavailable".into())
    }
}

fn candidate(document: &Document, kind: &str) -> Result<(ObjectId, Transform), String> {
    for object in document.objects() {
        let eligible = matches!(
            (kind, object.id),
            ("wall_move" | "wall_resize", ObjectId::Wall(_))
                | ("door_move", ObjectId::Door(_))
                | ("light_move", ObjectId::Light(_))
        );
        if !eligible {
            continue;
        }
        let mut pose = document.transform(object.id)?;
        if kind == "wall_resize" {
            let axis = if pose.size[0] >= pose.size[2] { 0 } else { 2 };
            if let Some(size) = pose.size.get_mut(axis) {
                *size += 0.05_f32;
            }
        } else {
            pose.position[0] += 0.05_f32;
        }
        if document.preview_transform(object.id, pose).is_ok() {
            return Ok((object.id, pose));
        }
    }
    Err(format!("No valid {kind} candidate in authored map"))
}

pub(super) fn run(path: &Path, level: &str) -> Result<Value, String> {
    let (mut document, open_ms) = timed(|| Document::open(path))?;
    let ((), select_ms) = timed(|| document.select_level(level))?;
    let original = document.raw().clone();
    let mut samples = Vec::new();
    let mut cases = Vec::new();
    for kind in ["wall_move", "wall_resize", "door_move", "light_move"] {
        let (id, pose) = match candidate(&document, kind) {
            Ok(candidate) => candidate,
            Err(reason) => {
                cases.push(json!({"kind":kind,"not_run":reason}));
                continue;
            }
        };
        cases.push(json!({"kind":kind,"object":format!("{id:?}"),"pose":format!("{pose:?}")}));
        for repeat in 0_u16..24 {
            let ((), edit_ms) = timed(|| document.set_transform(id, pose))?;
            let ((), undo_ms) = timed(|| restore(&mut document, false))?;
            let ((), redo_ms) = timed(|| restore(&mut document, true))?;
            let ((), restore_ms) = timed(|| restore(&mut document, false))?;
            if document.raw() != &original || document.selected_is_dirty() {
                return Err(format!(
                    "{kind}: undo changed the original source/savepoint"
                ));
            }
            samples.push(json!({"kind":kind,"repeat":repeat,"edit_ms":edit_ms,"undo_ms":undo_ms,"redo_ms":redo_ms,"restore_ms":restore_ms}));
        }
    }
    let catalog = crate::engine::assets::AssetCatalog::load_default();
    let material = catalog
        .entries()
        .iter()
        .find(|entry| entry.asset_type.as_str() == "material")
        .map(|entry| entry.id.clone())
        .ok_or("No material in catalog")?;
    for repeat in 0_u16..24 {
        let ((), edit_ms) = timed(|| document.set_material(ObjectId::Room(0), &material))?;
        let ((), undo_ms) = timed(|| restore(&mut document, false))?;
        if document.raw() != &original {
            return Err("Material undo lost source data".into());
        }
        samples
            .push(json!({"kind":"material","repeat":repeat,"edit_ms":edit_ms,"undo_ms":undo_ms}));
    }
    let light = candidate(&document, "light_move")?.0;
    let mut pose = document.transform(light)?;
    let mut rapid = Vec::new();
    for repeat in 0_u16..64 {
        pose.position[0] += if repeat % 2 == 0 { 0.05_f32 } else { -0.05_f32 };
        let ((), edit_ms) = timed(|| document.set_transform(light, pose))?;
        rapid.push(edit_ms);
    }
    let retained = document.resource_snapshot();
    let rapid_source = document.raw().clone();
    for _ in 0_u16..64 {
        restore(&mut document, false)?;
    }
    if document.raw() != &original {
        return Err("64 undos failed exact source restoration".into());
    }
    for _ in 0_u16..64 {
        restore(&mut document, true)?;
    }
    if document.raw() != &rapid_source {
        return Err("64 redos failed exact source restoration".into());
    }
    let ((), save_ms) = timed(|| document.save())?;
    let mut reopened = Document::open(path)?;
    reopened.select_level(level)?;
    if reopened.raw() != document.raw() || reopened.is_dirty() {
        return Err("Save/reopen did not preserve the authoritative JSON".into());
    }
    let loaded = document.resource_snapshot();
    let ids: Vec<_> = document.level_ids().map(str::to_owned).collect();
    let mut switches = Vec::new();
    for id in ids.iter().take(2) {
        let ((), ms) = timed(|| document.select_level(id))?;
        switches.push(json!({"id":id,"ms":ms}));
    }
    document.select_level(level)?;
    let cached_switch_ms = if ids.len() > 1 {
        let ((), ms) = timed(|| {
            document.select_level(ids.first().ok_or("No entry")?)?;
            document.select_level(level)
        })?;
        Some(ms)
    } else {
        None
    };
    let objects = document.objects().len();
    drop(reopened);
    drop(document);
    Ok(
        json!({"kind":"document_session","project":path,"level":level,"objects":objects,"open_ms":open_ms,"select_ms":select_ms,"cases":cases,"samples":samples,"rapid_ms":rapid,"retained":retained,"after_save":loaded,"save_ms":save_ms,"switches":switches,"cached_switch_ms":cached_switch_ms,"exact_history":true,"exact_save_reopen":true}),
    )
}
