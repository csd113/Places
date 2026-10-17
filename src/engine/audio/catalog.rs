use std::fs;
use std::io::Read;
use std::path::{Component, Path};

use serde::Deserialize;

const MAX_CATALOG_BYTES: u64 = 262_144;
pub(super) const MAX_CUES: usize = 64;
pub(super) const MAX_FILES: usize = 128;
pub(super) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Validated authored cue metadata. Loading it performs no audio-device work.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCatalog {
    format_version: u32,
    pub(super) cues: Vec<Cue>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cue {
    pub id: String,
    pub files: Vec<String>,
    #[serde(default = "unit_gain")]
    pub gain: f32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default = "spatial_default")]
    pub spatial: bool,
    #[serde(default = "unit_gain")]
    pub min_distance: f32,
    #[serde(default = "max_distance_default")]
    pub max_distance: f32,
}

const fn unit_gain() -> f32 {
    1.0
}
const fn spatial_default() -> bool {
    true
}
const fn max_distance_default() -> f32 {
    30.0
}

impl AudioCatalog {
    /// Installed cue identities for authoring pickers; no audio device needed.
    pub fn cue_ids(&self) -> impl Iterator<Item = &str> {
        self.cues.iter().map(|cue| cue.id.as_str())
    }
    /// Reads and validates `asset_root/audio/catalog.json` within fixed limits.
    ///
    /// # Errors
    /// Returns malformed metadata, unsafe paths, duplicate IDs or exceeded bounds.
    pub fn load(asset_root: &Path) -> Result<Self, String> {
        let path = asset_root.join("audio/catalog.json");
        let file = fs::File::open(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut bytes = Vec::new();
        let _read = file
            .take(MAX_CATALOG_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CATALOG_BYTES {
            return Err("audio catalog exceeds 256 KiB".to_owned());
        }
        let catalog = Self::from_json(&bytes)?;
        catalog.validate_files(asset_root)?;
        Ok(catalog)
    }

    /// Checks every declared file exists and is a bounded regular file confined
    /// to `asset_root/audio`, including after resolving symbolic links.
    ///
    /// # Errors
    /// Returns a missing/nonregular file, escaped path or file-size violation.
    pub fn validate_files(&self, asset_root: &Path) -> Result<(), String> {
        let root = fs::canonicalize(asset_root.join("audio")).map_err(|error| error.to_string())?;
        for cue in &self.cues {
            for file in &cue.files {
                let path = fs::canonicalize(root.join(file))
                    .map_err(|error| format!("{file}: {error}"))?;
                if !path.starts_with(&root) {
                    return Err(format!("audio file escapes asset root: {file}"));
                }
                let metadata = fs::metadata(&path).map_err(|error| format!("{file}: {error}"))?;
                if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_FILE_BYTES {
                    return Err(format!(
                        "audio file must be a nonempty regular WAV of at most 16 MiB: {file}"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validates catalog JSON without opening files or initializing SDL.
    ///
    /// # Errors
    /// Returns invalid JSON, fields, IDs, paths or catalog resource bounds.
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CATALOG_BYTES {
            return Err("audio catalog exceeds 256 KiB".to_owned());
        }
        let catalog: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if catalog.format_version != 1 || catalog.cues.len() > MAX_CUES {
            return Err("audio catalog requires format_version 1 and at most 64 cues".to_owned());
        }
        let mut files = 0_usize;
        for (index, cue) in catalog.cues.iter().enumerate() {
            if cue.id.is_empty()
                || cue.id.len() > 96
                || !cue
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            {
                return Err(format!("invalid audio cue id `{}`", cue.id));
            }
            if catalog
                .cues
                .iter()
                .take(index)
                .any(|previous| previous.id == cue.id)
            {
                return Err(format!("duplicate audio cue `{}`", cue.id));
            }
            if cue.files.is_empty()
                || cue.files.len() > 8
                || !valid_gain(cue.gain)
                || !cue.min_distance.is_finite()
                || !cue.max_distance.is_finite()
                || cue.min_distance < 0.01
                || cue.max_distance <= cue.min_distance
                || cue.max_distance > 1000.0
            {
                return Err(format!("invalid audio cue bounds for `{}`", cue.id));
            }
            for file in &cue.files {
                let path = Path::new(file);
                if file.len() > 192
                    || file.contains('\\')
                    || !path
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
                    || !path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
                {
                    return Err(format!("unsafe/non-WAV audio path `{file}`"));
                }
            }
            files = files.saturating_add(cue.files.len());
            if files > MAX_FILES {
                return Err("audio catalog exceeds 128 file references".to_owned());
            }
        }
        Ok(catalog)
    }

    /// True when the manifest declares this exact logical cue ID.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.cues.iter().any(|cue| cue.id == id)
    }
}

pub(super) fn valid_gain(gain: f32) -> bool {
    gain.is_finite() && (0.0..=1.0).contains(&gain)
}
