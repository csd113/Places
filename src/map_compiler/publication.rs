//! Shared editable-input and destination guards, checked again after encoding.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::package::{hash, project};
use crate::engine::project::{ProjectFolder, ProjectPath};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FileIdentity {
    canonical: PathBuf,
    bytes: u64,
    modified: std::time::SystemTime,
    hash: String,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

pub(super) fn output_identity(path: &Path) -> Result<Option<FileIdentity>, String> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            #[cfg(unix)]
            use std::os::unix::fs::MetadataExt as _;
            Ok(Some(FileIdentity {
                canonical: fs::canonicalize(path).map_err(|e| e.to_string())?,
                bytes: metadata.len(),
                modified: metadata.modified().map_err(|e| e.to_string())?,
                hash: hash::sha256_file(path)?,
                #[cfg(unix)]
                device: metadata.dev(),
                #[cfg(unix)]
                inode: metadata.ino(),
            }))
        }
        Ok(_) => Err("Package destination must be a regular file, never a symlink".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn destination_path(path: &Path) -> Result<PathBuf, String> {
    if path
        .extension()
        .is_none_or(|extension| extension != "placesmap")
    {
        return Err("Compiler output requires a .placesmap filename".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(fs::canonicalize(parent)
        .map_err(|e| e.to_string())?
        .join(path.file_name().ok_or("Output has no filename")?))
}

enum Inputs {
    Project(PathBuf, ProjectFolder, BTreeMap<ProjectPath, Vec<u8>>),
    Standalone(PathBuf, FileIdentity),
}

pub(super) struct Guard {
    inputs: Inputs,
    out: PathBuf,
    destination: PathBuf,
    original: Option<FileIdentity>,
}

impl Guard {
    pub(super) fn new(source: &Path, out: &Path) -> Result<Self, String> {
        let destination = destination_path(out)?;
        let original = output_identity(out)?;
        let inputs = if super::project::is_project(source) {
            let folder = ProjectFolder::open(source)?;
            let sources = project::snapshot(&folder, &super::progress::check_cancelled)?;
            for path in sources.keys() {
                reject_alias(&folder.resolve(path)?, &destination, original.as_ref())?;
            }
            Inputs::Project(source.into(), folder, sources)
        } else {
            let canonical = fs::canonicalize(source).map_err(|e| e.to_string())?;
            reject_alias(&canonical, &destination, original.as_ref())?;
            let identity = output_identity(&canonical)?.ok_or("Source disappeared")?;
            Inputs::Standalone(source.into(), identity)
        };
        Ok(Self {
            inputs,
            out: out.into(),
            destination,
            original,
        })
    }

    pub(super) fn check(&self) -> Result<(), String> {
        super::progress::check_cancelled()?;
        if destination_path(&self.out)? != self.destination
            || output_identity(&self.out)? != self.original
        {
            return Err(
                "Compiler destination changed during compilation; external work retained".into(),
            );
        }
        let unchanged = match &self.inputs {
            Inputs::Project(input, folder, sources) => {
                let current = ProjectFolder::open(input)?;
                current.root() == folder.root()
                    && project::snapshot(&current, &super::progress::check_cancelled)? == *sources
            }
            Inputs::Standalone(path, original) => {
                output_identity(&fs::canonicalize(path).map_err(|e| e.to_string())?)?.as_ref()
                    == Some(original)
            }
        };
        if !unchanged {
            return Err(
                "Editable input changed during compilation; previous output retained".into(),
            );
        }
        super::progress::check_cancelled()
    }
}

fn reject_alias(
    source: &Path,
    destination: &Path,
    output: Option<&FileIdentity>,
) -> Result<(), String> {
    if source == destination || same_output_file(source, output)? {
        return Err("Compiler output aliases an editable input; choose another filename".into());
    }
    Ok(())
}

fn same_output_file(source: &Path, output: Option<&FileIdentity>) -> Result<bool, String> {
    let Some(identity) = output else {
        return Ok(false);
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let metadata = fs::metadata(source).map_err(|e| e.to_string())?;
        Ok(metadata.dev() == identity.device && metadata.ino() == identity.inode)
    }
    #[cfg(not(unix))]
    {
        // Portable conservative fallback when std exposes no stable file ID.
        // An editable JSON/script/image cannot also be a valid compiled ZIP;
        // refusing identical copies is safe as well as protecting hard links.
        Ok(hash::sha256_file(source)? == identity.hash)
    }
}

pub(super) fn check_map_destination(out: &Path, id: &str, package_id: &str) -> Result<(), String> {
    if out.exists() {
        let previous = super::inspect(out)?;
        if crate::engine::package::project::inspect(out)?.is_some()
            || previous.id != id
            || previous.package_id != package_id
        {
            return Err(
                "Build would overwrite another map/project; choose another filename".into(),
            );
        }
    }
    Ok(())
}
