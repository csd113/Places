//! Installed dependency identities without unbounded reads or root escapes.

use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::{DependencyKind, PackageDependency, archive, hash};

fn limit(kind: DependencyKind) -> Result<u64, String> {
    match kind {
        DependencyKind::Model => {
            u64::try_from(crate::engine::gltf::MAX_ENCODED_GLB_BYTES).map_err(|e| e.to_string())
        }
        DependencyKind::Texture => u64::try_from(crate::engine::materials::MAX_ENCODED_PNG_BYTES)
            .map_err(|e| e.to_string()),
        DependencyKind::Embedded => {
            Err("Embedded dependencies are archive records, not installed files".into())
        }
    }
}

/// Hash one confined regular installed resource using an 8 KiB buffer and the
/// same encoded byte limit its GLB/PNG consumer applies, including growth.
pub fn identity(root: &Path, path: &str, kind: DependencyKind) -> Result<(u64, String), String> {
    if super::normalize_entry_name(path).as_deref() != Some(path) {
        return Err(format!("Unsafe installed dependency path '{path}'"));
    }
    let canonical_root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let resolved = std::fs::canonicalize(canonical_root.join(path))
        .map_err(|e| format!("dependency '{path}' is missing/unreadable: {e}"))?;
    if !resolved.starts_with(&canonical_root) {
        return Err(format!(
            "dependency '{path}' escapes the installed asset root"
        ));
    }
    let limit = limit(kind)?;
    let mut file = archive::open_file(&resolved)?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err(format!(
            "dependency '{path}' exceeds its {limit}-byte encoded resource limit"
        ));
    }
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0; 8192];
    let mut bounded = file.by_ref().take(limit.saturating_add(1));
    loop {
        crate::engine::resources::checkpoint()?;
        let count = bounded.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if total > limit {
            return Err(format!(
                "dependency '{path}' grew beyond its encoded resource limit"
            ));
        }
        hasher.update(
            buffer
                .get(..count)
                .ok_or("Dependency hash exceeded its buffer")?,
        );
    }
    Ok((total, hash::digest_hex(&hasher.finalize())))
}

pub fn validate(root: &Path, dependency: &PackageDependency) -> Result<(), String> {
    if dependency.bytes > limit(dependency.kind)? {
        return Err(format!(
            "dependency '{}' declares more than its encoded resource limit",
            dependency.path
        ));
    }
    let (bytes, sha256) = identity(root, &dependency.path, dependency.kind).map_err(|error| {
        format!(
            "dependency '{}' could not be verified: {error}",
            dependency.path
        )
    })?;
    if bytes != dependency.bytes || sha256 != dependency.sha256 {
        return Err(format!(
            "dependency '{}' content/size changed (recorded {} bytes); rebuild against the installed assets",
            dependency.path, dependency.bytes
        ));
    }
    Ok(())
}
