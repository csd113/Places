//! The compiled Places map package (`.placesmap`).
//!
//! A package is a ZIP archive carrying one validated semantic level record,
//! one or more quality variants of the prepared static world, prepared
//! lighting and lightmap payloads, reference data for dynamic objects, and the
//! dependency identities the player verifies before use.
//!
//! ```text
//! manifest.json                     small JSON record: format, identity, entries, dependencies
//! semantics.json                    the validated LevelDef (gameplay semantics only)
//! materials.json                    the resolved material/fixture-sheet records
//! blobs/<sha256>.mesh               static geometry, binary
//! blobs/<sha256>.props              transformed static prop batches, binary
//! blobs/<sha256>.lighting           baked LevelLighting, binary
//! blobs/<sha256>.lightmaps.ktx2     prepared HDR atlas pages (KTX2 2D array, RGBA16F)
//! blobs/<sha256>.lightmaps.json     atlas chart/stat/switchable record
//! blobs/<sha256>.probe.ktx2        one prepared reflection probe cubemap (KTX2 cube, RGBA8)
//! blobs/<sha256>.probes.json        probe positions and face size for one payload
//! blobs/<sha256>.collision          compile-time static collision record, binary
//! blobs/<sha256>.navigation         compile-time baked navigation grid, binary
//! blobs/<sha256>.png                embedded texture pixels (community packages)
//! ```
//!
//! Blobs are content-addressed: the entry name carries the SHA-256 of the
//! entry's uncompressed bytes, and readers verify it before decoding. Variants
//! share blobs by identity, so a payload that does not differ between quality
//! levels is stored once.
//!
//! # Contract rules
//!
//! * **Data only.** Nothing in a package is executed. There are no scripts,
//!   no tool invocations and no auto-compilation of authoring sources.
//! * **Bounded before use.** Every archive entry, count, dimension, hash and
//!   aggregate is bounded from the manifest and from hard per-subsystem caps
//!   before an allocation or a decode happens. A package that exceeds a bound
//!   is rejected with a named error, never truncated or skipped.
//! * **One current version.** [`FORMAT_VERSION`] is the only accepted major
//!   version. An unsupported required capability is a named error; there is no
//!   legacy reader and no compatibility mode. A newer *compiler fingerprint*
//!   is not a format problem: developer rebuild identity and runtime format
//!   validity are separate concerns.
//! * **No ambient state.** Paths inside a package are relative entry names.
//!   Absolute paths, drive letters, UNC names, `..` traversal, links and
//!   duplicate normalized names are rejected at open.
//!
//! The writer lives in the same module tree so the compiler and the player can
//! never drift: the writer produces exactly what the reader accepts, and both
//! are covered by the malformed-input suite in `src/package/tests.rs`.

pub mod archive;
pub mod binary;
pub mod collision;
pub mod hash;
pub mod ktx2;
pub mod lighting;
pub mod lightmaps;
pub mod manifest;
pub mod mesh;
pub mod navigation;
pub mod props;
pub mod world;

#[cfg(test)]
mod tests;

use std::path::Path;

pub use archive::{PackageReader, normalize_entry_name};
pub use manifest::{
    DependencyKind, Manifest, PackageDependency, PackageEntry, Variant, VariantEntries,
};

/// The one accepted package format major version.
///
/// A package whose `package_format` differs is rejected by name; there is no
/// read path for a superseded development version. Increase this only for a
/// contract change the current player cannot interpret at all.
pub const FORMAT_VERSION: u32 = 1;

/// Largest accepted `manifest.json`, in bytes.
pub const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

/// Largest accepted `semantics.json`, in bytes.
pub const MAX_SEMANTICS_BYTES: u64 = 64 * 1024 * 1024;

/// Largest accepted `materials.json`, in bytes.
pub const MAX_MATERIALS_BYTES: u64 = 16 * 1024 * 1024;

/// Largest accepted chart metadata record, in bytes. Static model receivers
/// carry many more charts than the small material catalog; their read remains
/// bounded independently from material metadata and the aggregate archive cap.
pub const MAX_LIGHTMAP_METADATA_BYTES: u64 = 64 * 1024 * 1024;

/// Largest accepted archive entry, in bytes (decompressed).
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// Largest accepted sum of decompressed archive entry bytes.
pub const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

/// Largest accepted number of archive entries.
pub const MAX_ENTRIES: usize = 512;

/// Largest accepted number of quality variants in one package.
pub const MAX_VARIANTS: usize = 4;

/// Largest accepted number of declared dependencies.
pub const MAX_DEPENDENCIES: usize = 4096;

/// Largest accepted embedded texture payload, in bytes.
pub const MAX_TEXTURE_BYTES: u64 = 64 * 1024 * 1024;

/// Largest accepted lightmap page edge, in texels.
pub const MAX_LIGHTMAP_PAGE_EDGE: u32 = 4096;

/// Largest accepted number of lightmap pages.
pub const MAX_LIGHTMAP_PAGES: usize = 64;

/// Largest accepted number of switchable-light contribution layer groups.
///
/// Each group costs two RGBA16F planes per page, so this is the bound that
/// keeps one prepared variant's atlas memory predictable. The shipped demo has
/// one switchable fixture; a level that authors more than this fails over to
/// vertex lighting by name.
pub const MAX_SWITCHABLE_LIGHTS: usize = 4;

/// Largest accepted probe face edge, in texels.
pub const MAX_PROBE_FACE_EDGE: u32 = 256;

/// Largest accepted probe count in one payload.
pub const MAX_PROBES: usize = 32;

/// Largest accepted static mesh vertex count.
///
/// Mirrors the level format's own vertex budget so a compiled package can
/// never admit more geometry than the authoring pipeline can describe.
pub const MAX_MESH_VERTICES: u64 = crate::level::MAX_LEVEL_VERTICES;

/// Largest accepted static mesh index count.
///
/// Six indices per quad over the level's geometry budget.
pub const MAX_MESH_INDICES: u64 = 6 * MAX_MESH_VERTICES;

/// Largest accepted material index in one mesh range.
///
/// Mirrors the level's own explicit material budget
/// ([`crate::level::MAX_LEVEL_MATERIALS`]) so a package can never name a
/// material the level format would have refused. The sentinel
/// [`crate::render::MATERIAL_NONE`] is always accepted.
///
/// The literal is kept in sync with [`crate::level::MAX_LEVEL_MATERIALS`] by
/// `src/package/tests.rs::the_mesh_material_budget_mirrors_the_level_budget`:
/// `TryFrom` is not const-stable, so the equality cannot be expressed in the
/// constant itself.
pub const MAX_MESH_MATERIALS: u32 = 131_072;

/// Largest accepted number of mesh ranges.
pub const MAX_MESH_RANGES: usize = 1 << 20;

/// Largest accepted number of prop batches.
pub const MAX_PROP_BATCHES: usize = 1 << 20;

/// Largest accepted number of prop submeshes in one batch.
pub const MAX_PROP_SUBMESHES: usize = 4096;

/// Largest accepted number of vertices in one prop batch.
pub const MAX_PROP_BATCH_VERTICES: u64 = 1 << 24;

/// Largest accepted binary payload (mesh, props, lighting, collision), in bytes.
pub const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;

/// Largest accepted serialized lighting record, in bytes.
pub const MAX_LIGHTING_BYTES: u64 = 256 * 1024 * 1024;

/// Largest accepted collision record, in bytes.
pub const MAX_COLLISION_BYTES: u64 = 128 * 1024 * 1024;

/// Largest accepted number of collision boxes in one record.
pub const MAX_COLLISION_BOXES: usize = 1 << 22;

/// Largest accepted serialized navigation record, in bytes.
pub const MAX_NAVIGATION_BYTES: u64 = 128 * 1024 * 1024;

/// Largest accepted navigation grid cell count.
pub const MAX_NAV_CELLS: usize = 1 << 21;

/// Largest accepted baked agent class count in one navigation record.
pub const MAX_NAV_CLASSES: usize = 8;

/// Largest accepted door portal count in one navigation record.
pub const MAX_NAV_PORTALS: usize = 256;

/// A package open/read error: the archive could not be opened at all.
#[derive(Debug)]
pub enum OpenError {
    /// The file could not be read.
    Io(String),
    /// The bytes are not a ZIP archive.
    NotArchive(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "could not read map package: {message}"),
            Self::NotArchive(message) => {
                write!(formatter, "file is not a valid map package: {message}")
            }
        }
    }
}

/// The extension of a compiled map package.
pub const PACKAGE_EXTENSION: &str = "placesmap";

/// True when `path` names a compiled map package.
#[must_use]
pub fn is_package_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == PACKAGE_EXTENSION)
}
