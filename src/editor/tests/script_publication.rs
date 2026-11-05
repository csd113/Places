//! One tiny five-map fixture reproduces the 64-buffer publication bug and its
//! shared template path. Also probes a publication collision and locked edit
//! before file creation. No assets, renderer, compiler or bake.
use super::Document;
use serde_json::json;
use std::fs;

#[test]
fn script_capacity_and_publication_failures_leave_source_history_and_files_untouched()
-> Result<(), String> {
    let path =
        std::env::temp_dir().join(format!("places-script-publication-{}", std::process::id()));
    let mut doc = Document::create(&path, "first", "Scripts")?;
    let result = (|| {
        for map in 0_u8..4 {
            if map > 0 {
                doc.add_level(&format!("map{map}"))?;
            }
            for _ in 0_u8..16 {
                let _name = doc.create_script()?;
            }
        }
        doc.save_all()?;
        doc.add_level("fifth")?;
        let before = doc.raw().clone();
        let files = fs::read_dir(path.join("scripts"))
            .map_err(|e| e.to_string())?
            .count();
        assert!(doc.create_script().is_err());
        assert!(
            doc.create_script_edit(&before, "return {}", |raw, _| {
                *raw.get_mut("name").ok_or("Missing name")? = json!("Template edit");
                Ok(())
            })
            .is_err()
        );
        assert_eq!(doc.raw(), &before);
        assert!(!doc.is_dirty() && !doc.can_undo());
        assert_eq!(
            fs::read_dir(path.join("scripts"))
                .map_err(|e| e.to_string())?
                .count(),
            files
        );
        let key = doc.scripts.keys().next().cloned().ok_or("Missing buffer")?;
        drop(doc.scripts.remove(&key));
        // Simulate another author winning publication after our name preflight.
        let collision = path.join("scripts/fifth_1.luau");
        assert!(
            doc.create_script_edit(&before, "return {}", |_, _| {
                fs::write(&collision, "-- other author").map_err(|e| e.to_string())
            })
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(&collision).map_err(|e| e.to_string())?,
            "-- other author"
        );
        assert_eq!(doc.raw(), &before);
        assert!(!doc.is_dirty() && !doc.can_undo());
        let name = doc.create_script()?;
        assert_eq!(name, "scripts/fifth_2.luau");
        assert!(doc.script_text(&name).is_some());
        assert!(doc.undo());
        assert_eq!(doc.raw(), &before);
        assert!(
            path.join(&name).is_file(),
            "successful creation retains its file after Undo"
        );
        // A template edit to a locked object must fail before publication too.
        drop(doc.scripts.remove(&name));
        let mut locking = doc.raw().clone();
        drop(
            locking
                .pointer_mut("/rooms/0")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or("Missing room")?
                .insert("editor_locked".into(), json!(true)),
        );
        doc.accept_edit(locking)?;
        let locked = doc.raw().clone();
        assert!(
            doc.create_script_edit(&locked, "return {}", |raw, _| {
                *raw.pointer_mut("/rooms/0/width").ok_or("Missing width")? = json!(7.0_f32);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(doc.raw(), &locked);
        assert!(!path.join("scripts/fifth_3.luau").exists());
        #[cfg(unix)]
        {
            fs::rename(path.join("scripts"), path.join("saved-scripts"))
                .map_err(|e| e.to_string())?;
            std::os::unix::fs::symlink("saved-scripts", path.join("scripts"))
                .map_err(|e| e.to_string())?;
            assert!(
                doc.create_script().is_err(),
                "a later Open would refuse a symlinked directory"
            );
            assert_eq!(doc.raw(), &locked);
            assert!(!path.join("saved-scripts/fifth_3.luau").exists());
            fs::remove_file(path.join("scripts")).map_err(|e| e.to_string())?;
            fs::rename(path.join("saved-scripts"), path.join("scripts"))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    drop(doc);
    fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    result
}
