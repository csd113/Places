//! Authored content releases and automatic pre-release runtime admission.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Numeric, canonical `major.minor.patch`; content releases never order as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ReleaseVersion([u32; 3]);

impl Default for ReleaseVersion {
    fn default() -> Self {
        Self([1, 0, 0])
    }
}

impl TryFrom<String> for ReleaseVersion {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut numbers = value.split('.');
        let mut parts = [0_u32; 3];
        for part in &mut parts {
            let text = numbers
                .next()
                .ok_or("Package version requires major.minor.patch")?;
            if text.is_empty()
                || (text.len() > 1 && text.starts_with('0'))
                || !text.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("Package version requires canonical numeric major.minor.patch".into());
            }
            *part = text
                .parse()
                .map_err(|error| format!("Invalid package version: {error}"))?;
        }
        if numbers.next().is_some() {
            return Err("Package version requires exactly major.minor.patch".into());
        }
        Ok(Self(parts))
    }
}

impl From<ReleaseVersion> for String {
    fn from(value: ReleaseVersion) -> Self {
        value.to_string()
    }
}

impl std::fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [major, minor, patch] = self.0;
        write!(formatter, "{major}.{minor}.{patch}")
    }
}

/// Automatic compiler/player identity, independent of content and ZIP revisions.
pub const PLACES_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Pre-release builds promise compatibility only with this exact Places version.
/// Schema, capability and record admission remain additional requirements.
/// # Errors
/// Rejects missing, older and newer targets with actionable diagnostics.
pub fn admit_places_version(target: &str) -> Result<(), String> {
    if target != PLACES_VERSION {
        return Err(format!(
            "Package was built for Places {target}; this is Places {PLACES_VERSION}. Pre-release packages require an exact game version match. Install the matching Places version or recompile the map with this version."
        ));
    }
    Ok(())
}

/// One distributable's matching identity. Member IDs survive release updates.
/// The hash is the existing canonical manifest/index identity, never ZIP bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackageDescriptor {
    pub package_id: String,
    pub package_version: ReleaseVersion,
    pub places_version: String,
    pub project: bool,
    pub member_ids: Vec<String>,
    pub content_hash: String,
}

/// An equal release must name identical content; versions never prove equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateAdmission {
    Unchanged,
    Upgrade,
}

impl PackageDescriptor {
    /// Decides an update after runtime admission, before any installed write.
    /// # Errors
    /// Refuses identity changes, downgrades and conflicting release labels.
    pub fn admit_update(&self, incoming: &Self) -> Result<UpdateAdmission, String> {
        admit_places_version(&incoming.places_version)?;
        if self.package_id != incoming.package_id
            || self.project != incoming.project
            || (!self.project && self.member_ids != incoming.member_ids)
        {
            return Err("Cannot replace a different package identity/kind".into());
        }
        match incoming.package_version.cmp(&self.package_version) {
            std::cmp::Ordering::Less => Err(format!(
                "Package '{}' downgrade refused: installed {}, incoming {}",
                self.package_id, self.package_version, incoming.package_version
            )),
            std::cmp::Ordering::Equal if self == incoming => Ok(UpdateAdmission::Unchanged),
            std::cmp::Ordering::Equal => Err(format!(
                "Package '{}' release {} has different content; publish a higher package version",
                self.package_id, self.package_version
            )),
            std::cmp::Ordering::Greater => Ok(UpdateAdmission::Upgrade),
        }
    }
}

/// Reads admitted metadata, including every bundle member, without world preparation.
/// # Errors
/// Returns archive, version, metadata disagreement or integrity errors.
pub fn inspect(path: &Path) -> Result<PackageDescriptor, String> {
    let descriptor = inspect_installed(path)?;
    admit_places_version(&descriptor.places_version)?;
    Ok(descriptor)
}

/// Current-contract metadata may describe an older game target for update
/// comparison. This never authorizes runtime loading or legacy format conversion.
pub(crate) fn inspect_installed(path: &Path) -> Result<PackageDescriptor, String> {
    let mut reader = super::PackageReader::new(super::archive::open_file(path)?)
        .map_err(|error| error.to_string())?;
    if let Some(index) = super::project::read_index_metadata(&mut reader)? {
        return index.descriptor();
    }
    let bytes = reader.read_entry("manifest.json", super::MAX_MANIFEST_BYTES)?;
    super::Manifest::from_json_metadata(&bytes, reader.names())?.descriptor()
}
