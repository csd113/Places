//! The package manifest: the bounded, inspectable index of a compiled map.
//!
//! The manifest is small JSON. It names the level, lists the quality variants,
//! declares every archive entry with its size and SHA-256, and records the
//! content identities the package depends on. Decoders read the manifest
//! first and use it to bound every later read.

use serde::{Deserialize, Serialize};

use super::{FORMAT_VERSION, MAX_DEPENDENCIES, MAX_VARIANTS};

/// Runtime capabilities a package may require.
///
/// A package that requires a capability outside this list is rejected by name:
/// the player must never guess at a payload it does not understand.
pub const KNOWN_CAPABILITIES: [&str; 8] = [
    "geometry",
    "props",
    "lighting",
    "lightmaps-hdr",
    "irradiance-probes",
    "probes-rgba8",
    "collision",
    "navigation",
];

/// One archive entry as the manifest describes it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageEntry {
    /// Relative archive entry name (`blobs/<sha256>.<suffix>` or a named JSON
    /// record).
    pub name: String,
    /// What the entry holds: `mesh`, `props`, `lighting`, `lightmaps`,
    /// `lightmaps-meta`, `probes`, `probes-meta`, `collision`, `texture`.
    pub role: String,
    /// Uncompressed length in bytes.
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the uncompressed bytes.
    pub sha256: String,
}

/// What one dependency is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    /// A prop/animation model (GLB) read from the installed asset root.
    Model,
    /// A surface/decal/fixture texture (PNG) read from the installed asset
    /// root.
    Texture,
    /// A texture embedded in the package itself.
    Embedded,
}

/// One content dependency of a package.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageDependency {
    /// What kind of resource this is.
    pub kind: DependencyKind,
    /// Asset-root-relative path for model/texture dependencies, or the
    /// archive entry name for embedded dependencies.
    pub path: String,
    /// Lowercase hex SHA-256 of the resource bytes.
    pub sha256: String,
    /// Resource length in bytes.
    pub bytes: u64,
}

/// One prepared reflection probe payload for a face size.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbePayload {
    /// Probe face edge in texels (`48` or `64`).
    pub face_edge: u32,
    /// Mip levels each cubemap carries, base level first.
    ///
    /// A record written before offline prefiltering omits the field and is
    /// read as a single level.
    #[serde(default = "default_probe_levels")]
    pub levels: u32,
    /// Number of probes.
    pub count: u32,
    /// KTX2 cube entry names, one per probe, in routing order.
    pub cubemaps: Vec<String>,
    /// JSON entry with the probe positions.
    pub positions: String,
}

/// The mip level count of a probe payload that predates offline prefiltering.
#[must_use]
pub const fn default_probe_levels() -> u32 {
    1
}

/// One quality variant's prepared world entries.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariantEntries {
    /// Static geometry blob.
    pub mesh: String,
    /// Static prop batches blob.
    pub props: String,
    /// Baked lighting blob.
    pub lighting: String,
    /// Static collision blob.
    pub collision: String,
    /// Baked navigation grid blob.
    pub navigation: String,
    /// Prepared lightmap page array (absent for the vertex-lit variant).
    #[serde(default)]
    pub lightmaps: Option<String>,
    /// Lightmap chart/stat record matching [`Self::lightmaps`].
    #[serde(default)]
    pub lightmaps_meta: Option<String>,
    /// The prepared irradiance field moving objects sample (absent when the
    /// variant has none).
    #[serde(default)]
    pub irradiance: Option<String>,
    /// Prepared reflection probe payloads.
    #[serde(default)]
    pub probes: Vec<ProbePayload>,
}

/// One quality variant of the prepared world.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variant {
    /// Lightmap quality this variant answers: `off`, `medium` or `full`.
    pub lightmap_quality: String,
    /// Bake/plan profile this variant was prepared with: `low` or `full`.
    pub quality_profile: String,
    /// Why the variant fell back to vertex lighting, if it did.
    #[serde(default)]
    pub lightmap_failure: Option<String>,
    /// Prepared entries.
    pub entries: VariantEntries,
}

/// The package manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// Package format major version; must equal [`FORMAT_VERSION`].
    pub package_format: u32,
    /// Stable level id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Author, `""` when unnamed.
    #[serde(default)]
    pub author: String,
    /// Compiler name and version that produced the package.
    #[serde(default)]
    pub created_by: String,
    /// Developer rebuild fingerprint.
    ///
    /// This is *not* a runtime contract: a different fingerprint means the
    /// package was prepared from different sources or parameters, which the
    /// developer `verify` command reports as stale. A player never rejects a
    /// package for a fingerprint mismatch.
    #[serde(default)]
    pub compiler_fingerprint: String,
    /// Developer **stage** fingerprint of the illumination/geometry inputs.
    ///
    /// It excludes display name/author and navigation/AI components, while
    /// retaining catalogue and referenced-image/model identities. A display,
    /// encounter or AI tuning edit keeps the same lighting fingerprint and the compiler reuses
    /// the previous package's prepared geometry, lightmaps, probes and
    /// collision instead of rebaking illumination. `None` on a package built
    /// before the field existed (an explicit `--force` rebuild writes it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting_fingerprint: Option<String>,
    /// Capabilities the runtime must understand to interpret this package.
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    /// Content dependencies with their identities.
    #[serde(default)]
    pub dependencies: Vec<PackageDependency>,
    /// Every archive entry, with size and hash.
    #[serde(default)]
    pub entries: Vec<PackageEntry>,
    /// Quality variants, at most one per lightmap quality.
    pub variants: Vec<Variant>,
}

impl Manifest {
    /// Parses a manifest and validates its structure against `archive_names`,
    /// the normalized names the archive actually carries.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn from_json(bytes: &[u8], archive_names: &[String]) -> Result<Self, String> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("manifest is not valid JSON: {error}"))?;
        manifest.validate(archive_names)?;
        Ok(manifest)
    }

    /// Structural validation, independent of payload decoding.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    #[allow(clippy::too_many_lines)] // one cohesive structural validation pass
    pub fn validate(&self, archive_names: &[String]) -> Result<(), String> {
        if self.package_format != FORMAT_VERSION {
            return Err(format!(
                "package format {} is not supported (this player reads format {FORMAT_VERSION})",
                self.package_format
            ));
        }
        validate_identity("id", &self.id, 128)?;
        validate_identity("name", &self.name, 256)?;
        validate_optional_identity("author", &self.author, 256)?;
        for capability in &self.required_capabilities {
            if !KNOWN_CAPABILITIES.contains(&capability.as_str()) {
                return Err(format!(
                    "package requires the unsupported capability '{capability}'"
                ));
            }
        }
        if self.variants.is_empty() {
            return Err("package has no quality variants".to_string());
        }
        if self.variants.len() > MAX_VARIANTS {
            return Err(format!(
                "package has {} variants (limit {MAX_VARIANTS})",
                self.variants.len()
            ));
        }
        if self.dependencies.len() > MAX_DEPENDENCIES {
            return Err(format!(
                "package declares {} dependencies (limit {MAX_DEPENDENCIES})",
                self.dependencies.len()
            ));
        }
        if self.entries.is_empty() {
            return Err("package manifest lists no entries".to_string());
        }
        // Every declared entry must exist in the archive exactly once, and
        // every archive entry must be declared: an undeclared entry is either
        // a mistake or data the reader must not trust.
        for name in archive_names {
            if name == "manifest.json" {
                continue;
            }
            if !self.entries.iter().any(|entry| entry.name == *name) {
                return Err(format!(
                    "archive entry '{name}' is not declared in the manifest"
                ));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.entries {
            if super::normalize_entry_name(&entry.name).as_deref() != Some(entry.name.as_str()) {
                return Err(format!("manifest entry name '{}' is not safe", entry.name));
            }
            if !seen.insert(entry.name.as_str()) {
                return Err(format!("manifest declares entry '{}' twice", entry.name));
            }
            if archive_names
                .binary_search_by(|name| name.as_str().cmp(entry.name.as_str()))
                .is_err()
            {
                return Err(format!(
                    "manifest entry '{}' is missing from the archive",
                    entry.name
                ));
            }
            validate_sha256("entry", &entry.name, &entry.sha256)?;
            if !entry.role.is_empty() && entry.role.len() > 32 {
                return Err(format!("entry '{}' has an invalid role", entry.name));
            }
        }
        let mut qualities = std::collections::BTreeSet::new();
        for variant in &self.variants {
            if !matches!(variant.lightmap_quality.as_str(), "off" | "medium" | "full") {
                return Err(format!(
                    "variant lightmap quality '{}' is not off/medium/full",
                    variant.lightmap_quality
                ));
            }
            if !qualities.insert(variant.lightmap_quality.as_str()) {
                return Err(format!(
                    "package declares two '{}' variants",
                    variant.lightmap_quality
                ));
            }
            if !matches!(variant.quality_profile.as_str(), "low" | "full") {
                return Err(format!(
                    "variant quality profile '{}' is not low/full",
                    variant.quality_profile
                ));
            }
            let entries = &variant.entries;
            for (role, name) in [
                ("mesh", &entries.mesh),
                ("props", &entries.props),
                ("lighting", &entries.lighting),
                ("collision", &entries.collision),
                ("navigation", &entries.navigation),
            ] {
                self.validate_variant_entry(role, name)?;
            }
            match (&entries.lightmaps, &entries.lightmaps_meta) {
                (Some(pages), Some(meta)) => {
                    self.validate_variant_entry("lightmaps", pages)?;
                    self.validate_variant_entry("lightmaps-meta", meta)?;
                }
                (None, None) => {
                    if variant.lightmap_quality != "off" && variant.lightmap_failure.is_none() {
                        return Err(format!(
                            "variant '{}' has no lightmaps and no recorded failure",
                            variant.lightmap_quality
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "variant '{}' names only one of lightmaps/lightmaps_meta",
                        variant.lightmap_quality
                    ));
                }
            }
            if let Some(irradiance) = &entries.irradiance {
                self.validate_variant_entry("irradiance", irradiance)?;
            }
            let mut edges = std::collections::BTreeSet::new();
            for probe in &entries.probes {
                if probe.face_edge == 0 || probe.face_edge > super::MAX_PROBE_FACE_EDGE {
                    return Err(format!(
                        "probe payload edge {} is out of range",
                        probe.face_edge
                    ));
                }
                if !edges.insert(probe.face_edge) {
                    return Err(format!(
                        "variant declares two {}-texel probe payloads",
                        probe.face_edge
                    ));
                }
                validate_probe_levels(probe.face_edge, probe.levels)?;
                if probe.count > u32::try_from(super::MAX_PROBES).unwrap_or(u32::MAX) {
                    return Err(format!("probe payload has {} probes", probe.count));
                }
                if u32::try_from(probe.cubemaps.len()).unwrap_or(u32::MAX) != probe.count {
                    return Err(format!(
                        "probe payload declares {} probes but lists {} cubemaps",
                        probe.count,
                        probe.cubemaps.len()
                    ));
                }
                for cubemap in &probe.cubemaps {
                    self.validate_variant_entry("probes", cubemap)?;
                }
                self.validate_variant_entry("probes-meta", &probe.positions)?;
            }
        }
        for dependency in &self.dependencies {
            if super::normalize_entry_name(&dependency.path).as_deref()
                != Some(dependency.path.as_str())
            {
                return Err(format!(
                    "dependency path '{}' is not a safe relative path",
                    dependency.path
                ));
            }
            validate_sha256("dependency", &dependency.path, &dependency.sha256)?;
        }
        Ok(())
    }

    /// The declared entry with `name`, if any.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&PackageEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// The variant for a lightmap quality name, if present.
    #[must_use]
    pub fn variant(&self, lightmap_quality: &str) -> Option<&Variant> {
        self.variants
            .iter()
            .find(|variant| variant.lightmap_quality == lightmap_quality)
    }

    /// Serializes the manifest as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// Returns an error when the input is malformed, out of bounds or unsupported.
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("could not serialize manifest: {error}"))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    fn validate_variant_entry(&self, role: &str, name: &str) -> Result<(), String> {
        let Some(entry) = self.entry(name) else {
            return Err(format!("variant {role} entry '{name}' is not declared"));
        };
        if !entry.role.is_empty() && entry.role != role {
            return Err(format!(
                "entry '{name}' has role '{}' but is referenced as {role}",
                entry.role
            ));
        }
        Ok(())
    }
}

/// Validates one probe payload's mip level count: at least the base level, at
/// most the renderer's packaging cap, and never past one texel at the declared
/// face edge.
fn validate_probe_levels(face_edge: u32, levels: u32) -> Result<(), String> {
    if face_edge == 0 {
        return Err("probe payload face edge is zero".to_string());
    }
    let max_levels = crate::render::ProbeFaceReadback::packaged_mip_levels(face_edge);
    if levels == 0
        || levels > crate::render::ProbeFaceReadback::MAX_MIP_LEVELS
        || levels > max_levels
    {
        return Err(format!(
            "probe payload declares {levels} mip level(s) for {face_edge}-texel faces \
             (allowed 1..={max_levels})"
        ));
    }
    Ok(())
}

/// A bounded, printable, optional identity string (an unnamed author is
/// ordinary content, not malformed).
fn validate_optional_identity(what: &str, value: &str, max: usize) -> Result<(), String> {
    if value.len() > max {
        return Err(format!("package {what} is longer than {max} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("package {what} contains control characters"));
    }
    Ok(())
}

/// A bounded, printable level identity string.
fn validate_identity(what: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("package {what} is empty"));
    }
    if value.len() > max {
        return Err(format!("package {what} is longer than {max} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("package {what} contains control characters"));
    }
    Ok(())
}

/// A lowercase 64-character hex SHA-256.
fn validate_sha256(what: &str, name: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{what} '{name}' has an invalid SHA-256"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::render::ProbeFaceReadback;

    #[test]
    fn an_absent_probe_level_field_reads_as_one_level() {
        let payload: ProbePayload = serde_json::from_str(
            r#"{
                "face_edge": 64,
                "count": 1,
                "cubemaps": ["blobs/aa.probe.ktx2"],
                "positions": "blobs/bb.probes.json"
            }"#,
        )
        .expect("payload parses");
        assert_eq!(payload.levels, default_probe_levels());
        assert_eq!(payload.levels, 1);
    }

    #[test]
    fn probe_levels_are_bounded_by_the_edge_and_the_packaging_cap() {
        validate_probe_levels(64, 7).expect("the full 64-texel chain");
        validate_probe_levels(48, 6).expect("the full 48-texel chain");
        validate_probe_levels(64, 1).expect("a single level");
        assert!(validate_probe_levels(0, 1).is_err(), "no edge");
        assert!(validate_probe_levels(64, 0).is_err(), "no base level");
        assert!(
            validate_probe_levels(64, ProbeFaceReadback::MAX_MIP_LEVELS + 1).is_err(),
            "past the packaging cap"
        );
        assert!(
            validate_probe_levels(48, 7).is_err(),
            "past the 48-texel chain"
        );
    }
}
