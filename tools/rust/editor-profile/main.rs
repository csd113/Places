//! Explicit developer latency probe. No rendering, assets, baking or CI hook.
use places::editor::document::{Document, ObjectId};
use std::{hint::black_box, io::Write as _, path::Path, time::Instant};
fn measure(mut work: impl FnMut() -> Result<(), String>, count: u16) -> Result<f64, String> {
    let start = Instant::now();
    for _ in 0..count {
        work()?;
    }
    Ok(start.elapsed().as_secs_f64() * 1000.0 / f64::from(count))
}
fn main() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() == Some("--preview-audit") {
        let path = std::env::args()
            .nth(2)
            .ok_or("Pass an isolated project copy")?;
        let level = std::env::args().nth(3).ok_or("Pass a level id")?;
        let result = places::editor::profiling::preview_audit(Path::new(&path), &level)?;
        return writeln!(std::io::stdout(), "{result}").map_err(|error| error.to_string());
    }

    if std::env::args().nth(1).as_deref() == Some("--responsiveness") {
        let path = std::env::args()
            .nth(2)
            .ok_or("Pass an isolated project copy")?;
        let result = places::editor::profiling::responsiveness(Path::new(&path))?;
        return writeln!(std::io::stdout(), "{result}").map_err(|error| error.to_string());
    }
    let argument = std::env::args()
        .nth(1)
        .ok_or("Pass an isolated project copy")?;
    let path = Path::new(&argument);
    let opening = measure(
        || {
            let _opened = black_box(Document::open(path)?);
            Ok(())
        },
        10,
    )?;
    let mut doc = Document::open(path)?;
    let ids: Vec<_> = doc.level_ids().map(str::to_owned).collect();
    let switches = measure(
        || {
            for id in &ids {
                doc.select_level(id)?;
            }
            Ok(())
        },
        20,
    )? / f64::from(u16::try_from(ids.len()).map_err(|e| e.to_string())?);
    doc.select_level(ids.get(1).ok_or("Needs multiple maps")?)?;
    let objects = measure(
        || {
            let _objects = black_box(doc.objects());
            Ok(())
        },
        1000,
    )?;
    let dirty = measure(
        || {
            let _dirty = black_box(doc.is_dirty());
            Ok(())
        },
        1000,
    )?;
    let pose = doc.transform(ObjectId::Prop(0))?;
    let edits = measure(
        || {
            let mut next = pose;
            next.yaw_degrees += 5.0;
            doc.set_transform(ObjectId::Prop(0), next)?;
            let _undo = doc.undo();
            Ok(())
        },
        20,
    )?;
    let history = measure(
        || {
            let _redo = doc.redo();
            let _undo = doc.undo();
            Ok(())
        },
        20,
    )?;
    let save = measure(|| doc.save_all(), 5)?;
    writeln!(std::io::stdout(),
        "{}",
        serde_json::json!({"kind":"cpu_document_latency_ms","open":opening,"switch_cached":switches,"scene_list":objects,"dirty_check":dirty,"edit_and_undo":edits,"redo_and_undo":history,"save_clean":save,"object_count":doc.objects().len()})
    ).map_err(|error|error.to_string())?;
    Ok(())
}
