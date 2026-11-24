//! Small deterministic boundary checks. Ordinary native package admission is
//! witnessed separately with existing compiled maps, so these need no bake.

use super::*;
use std::io::Write;

#[test]
fn map_zip_member_budget_charges_actual_bytes_across_failed_members() -> Result<(), String> {
    // Protects the expanded-stream cap independent of directory size claims.
    // Two short byte slices exercise exact EOF, an overrun and the shared cap.
    let mut remaining = 8;
    let mut output = Vec::new();
    let _copied = MemberRead::new(&b"abc"[..], 3, &mut remaining)?
        .read_to_end(&mut output)
        .map_err(|error| error.to_string())?;
    assert_eq!(output, b"abc");
    output.clear();
    let _overrun = MemberRead::new(&b"defghi"[..], 2, &mut remaining)?
        .read_to_end(&mut output)
        .map_err(|error| error.to_string())?;
    assert_eq!(output, b"def", "only one overrun probe byte may be read");
    assert_eq!(remaining, 2, "the rejected member's bytes must still count");
    assert!(MemberRead::new(io::empty(), 2, &mut remaining).is_err());
    Ok(())
}

#[test]
fn map_zip_directory_entries_count_and_no_map_does_not_install() -> Result<(), String> {
    // A two-entry ZIP covers folder metadata in the pre-index count and the
    // public no-map result without adding any compiled fixture or permanent file.
    let root = archive::create_temporary_directory(&std::env::temp_dir().join("places-zip-test"))?;
    let result = (|| {
        let path = root.join("notes.zip");
        let file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
        let mut writer = zip::ZipWriter::new(file);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer
            .add_directory("notes/", stored)
            .map_err(|error| error.to_string())?;
        writer
            .start_file("notes/readme.txt", stored)
            .map_err(|error| error.to_string())?;
        writer
            .write_all(b"ordinary notes")
            .map_err(|error| error.to_string())?;
        drop(writer.finish().map_err(|error| error.to_string())?);
        assert!(
            matches!(archive::open_map_zip(archive::open_file(&path)?, 1), Err(error) if error.contains("2 entries (limit 1)")),
            "explicit folders and unrelated files must count toward the limit"
        );
        let levels = root.join("levels");
        let mut manager =
            LevelManager::with_paths(root.join("bundled"), levels.clone(), root.join("import"));
        assert!(
            matches!(manager.import_zip(&path), Err(error) if error.contains("no compiled .placesmap"))
        );
        assert!(
            !levels.exists(),
            "no candidates must not create installation staging"
        );
        Ok(())
    })();
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    result
}
