//! Validated, bounded state reads and atomic writes; app payloads are read-only.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);

fn checked(path: &Path) -> io::Result<PathBuf> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
        || path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .any(|byte| *byte < 32 || *byte == 127)
    {
        return Err(io::Error::other("Invalid state path"));
    }
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    for part in path.ancestors() {
        match fs::symlink_metadata(part) {
            Ok(info) => {
                if info.file_type().is_symlink()
                    || (part != path && !info.is_dir())
                    || (part == path && !info.is_file())
                {
                    return Err(io::Error::other("Unsafe state path"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if (info.is_file() && info.nlink() != 1)
                        || (info.mode() & 0o022 != 0 && info.mode() & 0o1000 == 0)
                    {
                        return Err(io::Error::other("Unsafe state links or permissions"));
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(path)
}

pub fn read(path: &Path, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let path = checked(path)?;
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let info = file.metadata()?;
    if !info.is_file() || info.len() > u64::try_from(limit).map_err(io::Error::other)? {
        return Err(io::Error::other("State file exceeds bounds"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let current = fs::symlink_metadata(&path)?;
        if info.nlink() != 1 || info.ino() != current.ino() || info.dev() != current.dev() {
            return Err(io::Error::other("State file changed during read"));
        }
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(
            u64::try_from(limit)
                .map_err(io::Error::other)?
                .saturating_add(1),
        )
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other("State file grew beyond bounds"));
    }
    Ok(Some(bytes))
}

pub fn preserve_invalid(path: &Path) -> io::Result<PathBuf> {
    let path = checked(path)?;
    for index in 0..64 {
        let backup = path.with_extension(if index == 0 {
            "json.invalid".to_owned()
        } else {
            format!("json.invalid.{index}")
        });
        checked(&backup)?;
        if backup.try_exists()? {
            continue;
        }
        // The containing directories have already rejected other-user writes.
        // Keep earlier invalid files instead of replacing an existing backup.
        fs::rename(&path, &backup)?;
        File::open(
            path.parent()
                .ok_or_else(|| io::Error::other("Missing state parent"))?,
        )?
        .sync_all()?;
        return Ok(backup);
    }
    Err(io::Error::other("Invalid settings backup limit reached"))
}

pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_with(path, bytes, Write::write_all)
}

fn write_with(
    path: &Path,
    bytes: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let path = checked(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing state parent"))?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(parent)?;
    checked(&path)?;
    let temporary = parent.join(format!(
        ".places-state-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        write(&mut file, bytes)?;
        file.sync_all()?;
        checked(&path)?;
        fs::rename(&temporary, &path)?;
        File::open(parent)?
            .sync_all()
            .map_err(|error| io::Error::other(format!("State saved; storage sync failed: {error}")))
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_full_storage_write_preserves_previous_settings() -> io::Result<()> {
        let root = std::env::temp_dir().canonicalize()?.join(format!(
            "places-state-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&root)?;
        let path = root.join("settings.json");
        write(&path, b"previous settings")?;
        let result = write_with(&path, b"new settings", |file, bytes| {
            file.write_all(
                bytes
                    .get(..3)
                    .ok_or_else(|| io::Error::other("Missing test prefix"))?,
            )?;
            Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "injected ENOSPC",
            ))
        });
        assert!(result.is_err_and(|error| error.kind() == io::ErrorKind::StorageFull));
        assert_eq!(
            read(&path, 64)?.as_deref(),
            Some(b"previous settings".as_slice())
        );
        assert!(read(&path, 2).is_err());
        let prior = root.join("settings.json.invalid");
        write(&prior, b"earlier invalid settings")?;
        let preserved = preserve_invalid(&path)?;
        assert_eq!(
            read(&prior, 64)?.as_deref(),
            Some(b"earlier invalid settings".as_slice())
        );
        assert_eq!(
            read(&preserved, 64)?.as_deref(),
            Some(b"previous settings".as_slice())
        );
        write(&path, b"previous settings")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&root)?.permissions().mode() & 0o777, 0o700);
            assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
            let link = root.join("link.json");
            std::os::unix::fs::symlink(&path, &link)?;
            assert!(write(&link, b"changed").is_err());
            assert!(read(&link, 64).is_err());
            let hard = root.join("hard.json");
            fs::hard_link(&path, &hard)?;
            assert!(write(&path, b"changed").is_err());
            fs::remove_file(hard)?;
        }
        assert!(write(&root.join("new/../escape.json"), b"changed").is_err());
        assert!(!root.join("new").exists());
        fs::remove_dir_all(root)
    }
}
