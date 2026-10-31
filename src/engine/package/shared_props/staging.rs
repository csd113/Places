//! Bounded compiler candidate storage. The factoring algorithm and record
//! order are shared with the memory writer; only unique segment ownership moves.

use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use super::{
    ChunkLocation, MAX_ENTRIES, MAX_SHARED_PROPS_SEGMENT_BYTES, SHARED_PROPS_CHUNK_BYTES,
    SharedPropsSegment, chunk_slice, hash, usize_from_u64,
};

pub(super) enum SegmentStorage {
    Memory(Vec<Vec<u8>>),
    Staged(StagedSegments),
}

impl SegmentStorage {
    pub(super) const fn len(&self) -> usize {
        match self {
            Self::Memory(segments) => segments.len(),
            Self::Staged(stage) => stage.segments.len(),
        }
    }

    pub(super) fn matches(
        &mut self,
        location: ChunkLocation,
        bytes: &[u8],
    ) -> Result<bool, String> {
        match self {
            Self::Memory(segments) => Ok(chunk_slice(segments, location)? == bytes),
            Self::Staged(stage) => {
                if location.segment == stage.segments.len() {
                    let end = location
                        .offset
                        .checked_add(location.length)
                        .ok_or("Prop chunk offset overflows")?;
                    return Ok(stage
                        .active
                        .get(location.offset..end)
                        .ok_or("Staged prop chunk is missing")?
                        == bytes);
                }
                let segment = stage
                    .segments
                    .get(location.segment)
                    .ok_or("Staged prop segment is missing")?;
                let mut file = File::open(&segment.path).map_err(|e| e.to_string())?;
                let _position = file
                    .seek(std::io::SeekFrom::Start(
                        u64::try_from(location.offset).map_err(|e| e.to_string())?,
                    ))
                    .map_err(|e| e.to_string())?;
                stage.scratch.resize(location.length, 0);
                file.read_exact(&mut stage.scratch)
                    .map_err(|e| e.to_string())?;
                Ok(stage.scratch == bytes)
            }
        }
    }

    pub(super) fn append(
        &mut self,
        bytes: &[u8],
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<ChunkLocation, String> {
        let limit = usize_from_u64(MAX_SHARED_PROPS_SEGMENT_BYTES)?;
        match self {
            Self::Memory(segments) => {
                if segments
                    .last()
                    .is_none_or(|segment| segment.len().saturating_add(bytes.len()) > limit)
                {
                    if segments.len() >= MAX_ENTRIES {
                        return Err("shared props exceed the archive entry count limit".into());
                    }
                    segments.push(Vec::new());
                }
                let index = segments.len().saturating_sub(1);
                let segment = segments
                    .last_mut()
                    .ok_or("shared prop segment is missing")?;
                let location = ChunkLocation {
                    segment: index,
                    offset: segment.len(),
                    length: bytes.len(),
                };
                segment.extend_from_slice(bytes);
                Ok(location)
            }
            Self::Staged(stage) => {
                if stage.active.len().saturating_add(bytes.len()) > limit {
                    stage.flush(check)?;
                }
                if stage.segments.len() >= MAX_ENTRIES {
                    return Err("shared props exceed the archive entry count limit".into());
                }
                let location = ChunkLocation {
                    segment: stage.segments.len(),
                    offset: stage.active.len(),
                    length: bytes.len(),
                };
                stage.active.extend_from_slice(bytes);
                Ok(location)
            }
        }
    }

    pub(super) fn identities(
        &mut self,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Vec<SharedPropsSegment>, String> {
        match self {
            Self::Memory(segments) => segments
                .iter()
                .map(|segment| {
                    Ok(SharedPropsSegment {
                        bytes: u64::try_from(segment.len()).map_err(|e| e.to_string())?,
                        sha256: hash::sha256_hex(segment),
                    })
                })
                .collect(),
            Self::Staged(stage) => {
                stage.flush(check)?;
                Ok(stage
                    .segments
                    .iter()
                    .map(|segment| segment.identity.clone())
                    .collect())
            }
        }
    }
}

/// One owned, identity-checked compiler candidate segment.
pub struct StagedSegment {
    pub(crate) path: PathBuf,
    identity: SharedPropsSegment,
}

impl StagedSegment {
    pub(crate) fn name(&self) -> String {
        self.identity.blob_name()
    }

    pub(crate) fn read_verified(&self) -> Result<Vec<u8>, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let length = usize_from_u64(self.identity.bytes)?;
        if file.metadata().map_err(|e| e.to_string())?.len() != self.identity.bytes
            || self.identity.bytes > MAX_SHARED_PROPS_SEGMENT_BYTES
        {
            return Err("Staged prop segment length changed".into());
        }
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        if hash::sha256_hex(&bytes) != self.identity.sha256 {
            return Err("Staged prop segment identity changed".into());
        }
        Ok(bytes)
    }
}

pub(super) struct StagedSegments {
    root: Option<PathBuf>,
    active: Vec<u8>,
    scratch: Vec<u8>,
    segments: Vec<StagedSegment>,
}

impl StagedSegments {
    fn new(destination: &Path) -> Result<Self, String> {
        let root = super::super::archive::create_temporary_directory(
            &destination.with_extension("props-staging"),
        )?;
        Ok(Self {
            root: Some(root),
            active: Vec::with_capacity(usize_from_u64(MAX_SHARED_PROPS_SEGMENT_BYTES)?),
            scratch: Vec::new(),
            segments: Vec::new(),
        })
    }

    fn flush(&mut self, check: &dyn Fn() -> Result<(), String>) -> Result<(), String> {
        if self.active.is_empty() {
            return Ok(());
        }
        let root = self.root.as_ref().ok_or("Prop stage already removed")?;
        let path = root.join(format!("{}.segment", self.segments.len()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        for bytes in self.active.chunks(SHARED_PROPS_CHUNK_BYTES) {
            check()?;
            file.write_all(bytes).map_err(|e| e.to_string())?;
        }
        drop(file);
        let identity = SharedPropsSegment {
            bytes: u64::try_from(self.active.len()).map_err(|e| e.to_string())?,
            sha256: hash::sha256_hex(&self.active),
        };
        self.segments.push(StagedSegment { path, identity });
        self.active.clear();
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), String> {
        if let Some(root) = &self.root {
            fs::remove_dir_all(root)
                .map_err(|e| format!("Could not remove prop stage {}: {e}", root.display()))?;
            self.root = None;
        }
        Ok(())
    }
}

impl Drop for StagedSegments {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            crate::engine::logging::warn(format_args!("{error}"));
        }
    }
}

/// One candidate with bounded segment heap storage and owned disk files.
pub struct StagedProps {
    pub(crate) recipes: Vec<Vec<u8>>,
    storage: StagedSegments,
}

impl StagedProps {
    pub(crate) fn segments(&self) -> &[StagedSegment] {
        &self.storage.segments
    }
    pub(crate) fn segment_bytes(&self) -> usize {
        self.storage
            .segments
            .iter()
            .fold(0_usize, |total, segment| {
                total.saturating_add(usize::try_from(segment.identity.bytes).unwrap_or(usize::MAX))
            })
    }
    pub(crate) fn cleanup(mut self) -> Result<(), String> {
        self.storage.cleanup()
    }
}

/// Factor exact bytes into an exclusively owned bounded candidate stage.
///
/// # Errors
/// Rejects invalid records, package limits, cancellation and staging I/O failures.
pub fn factor_props_staged(
    records: &[&[u8]],
    version: u16,
    destination: &Path,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<StagedProps, String> {
    check()?;
    let backing = SegmentStorage::Staged(StagedSegments::new(destination)?);
    let (recipes, completed) = super::factor_props_into(records, version, backing, check)?;
    match completed {
        SegmentStorage::Staged(mut staged) => {
            // No active bytes remain after identities were finalized.
            staged.active = Vec::new();
            staged.scratch = Vec::new();
            Ok(StagedProps {
                recipes,
                storage: staged,
            })
        }
        SegmentStorage::Memory(_) => Err("Unexpected memory prop result".into()),
    }
}

#[cfg(test)]
#[path = "../../tests/shared_props_staging.rs"]
mod tests;
