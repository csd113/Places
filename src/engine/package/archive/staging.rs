//! Ownership boundary for compiler export and previous-member seed staging.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub struct OwnedStaging {
    path: PathBuf,
    #[cfg(unix)]
    owned: unix::Directory,
}

impl OwnedStaging {
    pub fn reserve(output: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            let (path, owned) = unix::Directory::reserve(output)?;
            Ok(Self { path, owned })
        }
        #[cfg(not(unix))]
        {
            let path = super::create_temporary_directory(output)?;
            Ok(Self { path })
        }
    }

    /// Runs a path-only operation after verifying ownership where the staging
    /// directory has a retained identity.
    pub fn with_path<T>(
        &self,
        operation: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, String> {
        #[cfg(unix)]
        self.check()?;
        operation(&self.path)
    }

    /// Keeps path-only compiler APIs inside the recorded private directory.
    #[cfg(unix)]
    pub fn path(&self) -> Result<&Path, String> {
        self.check()?;
        Ok(&self.path)
    }

    #[cfg(unix)]
    pub fn check(&self) -> Result<(), String> {
        self.owned.check_path(&self.path)
    }

    pub fn display(&self) -> std::path::Display<'_> {
        self.path.display()
    }

    #[cfg(unix)]
    pub const fn directory(&self) -> &std::fs::File {
        &self.owned.handle
    }

    /// Creates subdirectories for path-only writers. Unix records their
    /// identities so cleanup can refuse an unrecorded directory.
    pub fn create_directories(&self, relative: &Path) -> Result<(), String> {
        #[cfg(unix)]
        {
            self.owned.create_directories(relative)
        }
        #[cfg(not(unix))]
        {
            std::fs::create_dir_all(self.path.join(relative)).map_err(|error| error.to_string())
        }
    }

    /// Opens a completed ordinary build inside the owned root. Unix unlinks
    /// the staged file while the reader holds it, so only one member is staged.
    pub fn take_member(&self, name: &OsStr) -> Result<std::fs::File, String> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err("Invalid staged member filename".into());
        }
        #[cfg(unix)]
        {
            self.owned.take_member(name)
        }
        #[cfg(not(unix))]
        {
            std::fs::File::open(self.path.join(name)).map_err(|error| error.to_string())
        }
    }

    pub fn cleanup(&self) -> Result<(), String> {
        #[cfg(unix)]
        {
            self.owned.cleanup(&self.path).map_err(|error| {
                format!(
                    "Retained staging artifacts for {}: {error}",
                    self.path.display()
                )
            })
        }
        #[cfg(not(unix))]
        {
            // Restore the legacy export and seed teardown for platforms
            // without Unix staging handles. A completed job frees its name.
            std::fs::remove_dir_all(&self.path).map_err(|error| error.to_string())
        }
    }
}

#[cfg(unix)]
mod unix {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::{Component, Path, PathBuf};

    use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, Stat};

    const NOFOLLOW: AtFlags = AtFlags::SYMLINK_NOFOLLOW;
    const OPEN_DIRECTORY: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    const PRIVATE: Mode = Mode::RUSR.union(Mode::WUSR).union(Mode::XUSR);

    pub(super) struct Directory {
        pub(super) handle: File,
        parent: File,
        name: OsString,
        owner: libc::uid_t,
        created_dirs: RefCell<BTreeMap<PathBuf, Stat>>,
    }

    impl Directory {
        pub(super) fn reserve(output: &Path) -> Result<(PathBuf, Self), String> {
            // SAFETY: geteuid returns the calling process's effective user ID.
            let owner = unsafe { libc::geteuid() };
            let output_parent = output
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            // Preserve same-volume staging when its namespace is trusted.
            // Shared non-sticky output folders use a trusted system temp root.
            let (parent_path, parent) = prepare_parent(output_parent, owner)
                .or_else(|_| prepare_parent(&std::env::temp_dir(), owner))?;
            let base = parent_path.join(output.file_name().ok_or("Missing staging filename")?);
            for attempt in 0_u8..16 {
                let path = super::super::temporary_path(&base, attempt);
                let name = path.file_name().ok_or("Missing staging filename")?;
                match fs::mkdirat(&parent, name, PRIVATE) {
                    Ok(()) => {}
                    Err(rustix::io::Errno::EXIST) => continue,
                    Err(error) => {
                        return Err(format!("Could not reserve {}: {error}", path.display()));
                    }
                }
                // The opened parent permits only trusted principals to move a
                // directory we just created. The private handle stays attached
                // to this object even if its public path is later replaced.
                let handle = open_directory(&parent, name).map_err(|error| {
                    format!(
                        "Could not open owned staging {}; retained artifacts: {error}",
                        path.display()
                    )
                })?;
                let mode = fs::fstat(&handle).map_err(|error| error.to_string())?;
                if mode.st_uid != owner || permissions(mode.st_mode) != PRIVATE {
                    return Err(format!(
                        "Could not establish private ownership of {}; retained artifacts",
                        path.display()
                    ));
                }
                let reserved = Self {
                    handle,
                    parent,
                    name: name.into(),
                    owner,
                    created_dirs: RefCell::new(BTreeMap::new()),
                };
                return Ok((path, reserved));
            }
            Err(format!(
                "Could not reserve project staging for {}",
                output.display()
            ))
        }

        pub(super) fn check_path(&self, path: &Path) -> Result<(), String> {
            let parent_path = path.parent().ok_or("Missing staging parent")?;
            let current_parent = trusted_directory(parent_path, self.owner)?;
            let parent_stat = fs::fstat(&self.parent).map_err(|error| error.to_string())?;
            check_identity(
                &parent_stat,
                &fs::fstat(current_parent).map_err(|error| error.to_string())?,
            )?;
            let recorded = fs::fstat(&self.handle).map_err(|error| error.to_string())?;
            if recorded.st_uid != self.owner || permissions(recorded.st_mode) != PRIVATE {
                return Err("Project staging is no longer private; retained artifacts".into());
            }
            check_identity(
                &recorded,
                &fs::statat(&self.parent, &self.name, NOFOLLOW).map_err(|error| {
                    format!("Project staging unavailable; retained artifacts: {error}")
                })?,
            )
        }

        pub(super) fn create_directories(&self, relative: &Path) -> Result<(), String> {
            let mut parent = self.handle.try_clone().map_err(|error| error.to_string())?;
            let mut below = PathBuf::new();
            let mut created = self.created_dirs.borrow_mut();
            for component in relative.components() {
                let Component::Normal(name) = component else {
                    return Err("Staged subdirectory must use ordinary relative components".into());
                };
                below.push(name);
                parent = if let Some(expected) = created.get(&below) {
                    let existing =
                        open_directory(&parent, name).map_err(|error| error.to_string())?;
                    check_identity(
                        expected,
                        &fs::fstat(&existing).map_err(|error| error.to_string())?,
                    )?;
                    existing
                } else {
                    fs::mkdirat(&parent, name, PRIVATE).map_err(|error| {
                        format!(
                            "Could not reserve staged subdirectory {}: {error}",
                            below.display()
                        )
                    })?;
                    let new = open_directory(&parent, name).map_err(|error| error.to_string())?;
                    let stat = fs::fstat(&new).map_err(|error| error.to_string())?;
                    if stat.st_uid != self.owner || permissions(stat.st_mode) != PRIVATE {
                        return Err("Staged subdirectory is not privately owned".into());
                    }
                    let _previous = created.insert(below.clone(), stat);
                    new
                };
            }
            Ok(())
        }

        pub(super) fn take_member(&self, name: &OsStr) -> Result<File, String> {
            let member = fs::openat(
                &self.handle,
                name,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map(File::from)
            .map_err(|error| error.to_string())?;
            let stat = fs::fstat(&member).map_err(|error| error.to_string())?;
            if stat.st_uid != self.owner
                || stat.st_nlink != 1
                || FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            {
                return Err(
                    "Unexpected staged member ownership or type; retained artifacts".into(),
                );
            }
            check_identity(
                &stat,
                &fs::statat(&self.handle, name, NOFOLLOW).map_err(|error| error.to_string())?,
            )?;
            fs::unlinkat(&self.handle, name, AtFlags::empty())
                .map_err(|error| error.to_string())?;
            Ok(member)
        }

        pub(super) fn cleanup(&self, path: &Path) -> Result<(), String> {
            self.check_path(path)?;
            let root = fs::fstat(&self.handle).map_err(|error| error.to_string())?;
            remove_contents(
                &self.handle,
                &root,
                self.owner,
                Path::new(""),
                &self.created_dirs.borrow(),
            )?;
            // Only the empty directory entry is removed by name. All
            // descendant operations used the original directory handle.
            self.check_path(path)?;
            fs::unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR)
                .map_err(|error| format!("Retained staging directory: {error}"))
        }
    }

    fn permissions(raw: rustix::fs::RawMode) -> Mode {
        Mode::from_raw_mode(raw) & (Mode::RWXU | Mode::RWXG | Mode::RWXO)
    }

    fn prepare_parent(path: &Path, owner: libc::uid_t) -> Result<(PathBuf, File), String> {
        let canonical = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
        let directory = trusted_directory(&canonical, owner)?;
        Ok((canonical, directory))
    }

    fn trusted_directory(path: &Path, owner: libc::uid_t) -> Result<File, String> {
        let mut parts = path.components();
        if parts.next() != Some(Component::RootDir) {
            return Err("Project staging parent must be absolute".into());
        }
        let mut directory = File::open("/").map_err(|error| error.to_string())?;
        for component in parts {
            let Component::Normal(name) = component else {
                return Err("Project staging parent must use ordinary path components".into());
            };
            directory = open_directory(&directory, name).map_err(|error| error.to_string())?;
            let stat = fs::fstat(&directory).map_err(|error| error.to_string())?;
            let mode = Mode::from_raw_mode(stat.st_mode);
            if (stat.st_uid != 0 && stat.st_uid != owner)
                || (mode.intersects(Mode::WGRP | Mode::WOTH) && !mode.contains(Mode::SVTX))
            {
                return Err(format!(
                    "Project staging parent is not private or sticky: {}",
                    path.display()
                ));
            }
        }
        Ok(directory)
    }

    fn open_directory(parent: &File, name: &OsStr) -> rustix::io::Result<File> {
        fs::openat(parent, name, OPEN_DIRECTORY, Mode::empty()).map(File::from)
    }

    fn check_identity(expected: &Stat, current: &Stat) -> Result<(), String> {
        if expected.st_dev != current.st_dev || expected.st_ino != current.st_ino {
            return Err("Project staging identity changed; retained artifacts".into());
        }
        Ok(())
    }

    fn remove_contents(
        directory: &File,
        root: &Stat,
        owner: libc::uid_t,
        relative: &Path,
        created_dirs: &BTreeMap<PathBuf, Stat>,
    ) -> Result<(), String> {
        for item in Dir::read_from(directory).map_err(|error| error.to_string())? {
            let entry = item.map_err(|error| error.to_string())?;
            let name = entry.file_name();
            if name == c"." || name == c".." {
                continue;
            }
            let stat = fs::statat(directory, name, NOFOLLOW).map_err(|error| error.to_string())?;
            if stat.st_uid != owner || stat.st_dev != root.st_dev {
                return Err(
                    "Unexpected ownership or filesystem inside project staging; retained artifacts"
                        .into(),
                );
            }
            let kind = FileType::from_raw_mode(stat.st_mode);
            if kind == FileType::Directory {
                let child_path = relative.join(OsStr::from_bytes(name.to_bytes()));
                let expected = created_dirs
                    .get(&child_path)
                    .ok_or("Unrecorded staged subdirectory")?;
                check_identity(expected, &stat)?;
                let child = fs::openat(directory, name, OPEN_DIRECTORY, Mode::empty())
                    .map(File::from)
                    .map_err(|error| error.to_string())?;
                check_identity(
                    &stat,
                    &fs::fstat(&child).map_err(|error| error.to_string())?,
                )?;
                remove_contents(&child, root, owner, &child_path, created_dirs)?;
                check_identity(
                    &stat,
                    &fs::statat(directory, name, NOFOLLOW).map_err(|error| error.to_string())?,
                )?;
                fs::unlinkat(directory, name, AtFlags::REMOVEDIR)
                    .map_err(|error| error.to_string())?;
            } else if kind == FileType::RegularFile && stat.st_nlink == 1 {
                fs::unlinkat(directory, name, AtFlags::empty())
                    .map_err(|error| error.to_string())?;
            } else {
                return Err(
                    "Unexpected file type or link inside project staging; retained artifacts"
                        .into(),
                );
            }
        }
        Ok(())
    }
}
