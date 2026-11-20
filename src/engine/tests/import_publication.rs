//! A few bytes and real filesystem operations verify the fresh-publication
//! boundary directly. No race scheduling, map creation or decoding is needed.

use super::{ImportOutcome, ImportPlan, publish_import};
use crate::engine::package::archive;
use std::fs;

#[test]
fn late_fresh_import_conflict_preserves_destination_and_cleans_staging() -> Result<(), String> {
    let path = std::env::temp_dir().join("places-import-publication-test");
    let root = archive::create_temporary_directory(&path)?;
    let result = (|| {
        let staged = root.join("staged");
        let plan = ImportPlan {
            target: root.join("installed.placesmap"),
            outcome: ImportOutcome::InstalledOrUpdated,
            previous_hash: None,
        };
        fs::write(&staged, b"complete candidate").map_err(|e| e.to_string())?;
        fs::write(&plan.target, b"independent file").map_err(|e| e.to_string())?;
        assert!(
            publish_import(&staged, &plan).is_err(),
            "fresh publication must refuse the late file"
        );
        assert_eq!(
            fs::read(&plan.target).map_err(|e| e.to_string())?,
            b"independent file",
            "the late file must survive"
        );
        assert!(!staged.exists(), "normal conflicts must remove staging");

        fs::remove_file(&plan.target).map_err(|e| e.to_string())?;
        fs::write(&staged, b"complete candidate").map_err(|e| e.to_string())?;
        publish_import(&staged, &plan)?;
        assert_eq!(
            fs::read(&plan.target).map_err(|e| e.to_string())?,
            b"complete candidate",
            "fresh publication exposes the complete file"
        );
        assert!(!staged.exists(), "successful fresh rename consumes staging");
        Ok(())
    })();
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    result
}
