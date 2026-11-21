//! Serializes manifest mutations by cooperating Places editors.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

pub(super) fn lock(root: &Path) -> Result<File, String> {
    if fs::canonicalize(root).map_err(|error| error.to_string())? != root {
        return Err("Project root changed; reopen before editing membership".into());
    }
    // Keep the lock name stable across writers. Removing it on release would
    // let another process open a different object while a waiter still uses it.
    let path = root.join(".places-editor-project.lock");
    let file = confined_open()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| {
            format!(
                "Could not open project writer lock {}: {error}",
                path.display()
            )
        })?;
    check_regular(&file, &path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => {
            Err("Another editor is publishing this project; try again".into())
        }
        Err(TryLockError::Error(error)) => {
            Err(format!("Could not lock the project for writing: {error}"))
        }
    }
}

fn confined_open() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let _flags = options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // Permit readers and the created writer handle. Keep the checked
        // pathname from being renamed or deleted until the commit finishes.
        const SHARE_READ_WRITE: u32 = 3;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let _flags = options
            .share_mode(SHARE_READ_WRITE)
            .custom_flags(OPEN_REPARSE_POINT);
    }
    options
}

fn check_regular(file: &File, path: &Path) -> Result<(), String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(format!(
                "Project file {} must not be a reparse point",
                path.display()
            ));
        }
    }
    if !metadata.is_file() {
        return Err(format!(
            "Expected a regular project file at {}",
            path.display()
        ));
    }
    Ok(())
}
