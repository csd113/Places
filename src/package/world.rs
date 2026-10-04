//! Player-side compiled package loading.
//!
//! The player opens a package, reads its manifest and validated semantics once
//! for discovery and material resolution, then reads exactly one quality
//! variant's prepared records for installation. Nothing here emits geometry,
//! bakes light, plans charts or captures reflections: the records are the
//! compiler's finished products, decoded under the package's own bounds.
//!
//! Reflection captures are required for a variant that renders reflections:
//! the player uploads the compiler's cubemaps instead of rendering its own.
//! A package built without the GPU capture step is refused with an actionable
//! error rather than silently rendering without reflections.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::level::LevelDef;
use crate::lighting::LevelLighting;
use crate::lighting::lightmap::LevelLightmaps;
use crate::materials::MaterialTable;
use crate::package::collision::{CompiledCollision, read_collision};
use crate::package::ktx2::{self, Ktx2Rgba8};
use crate::package::lighting::read_lighting;
use crate::package::lightmaps::read_lightmaps;
use crate::package::manifest::{Manifest, ProbePayload, Variant};
use crate::package::mesh::read_mesh;
use crate::package::props::read_props;
use crate::package::{
    MAX_BINARY_BYTES, MAX_COLLISION_BYTES, MAX_ENTRY_BYTES, MAX_LIGHTING_BYTES,
    MAX_LIGHTMAP_ATLAS_BYTES, MAX_LIGHTMAP_METADATA_BYTES, MAX_MATERIALS_BYTES,
    MAX_NAVIGATION_BYTES, MAX_SEMANTICS_BYTES, PackageReader,
};
use crate::quality::LightmapQuality;
use crate::render::{LevelMesh, PropMeshBatch};

/// The part of a package a discovery or material pass needs: the manifest and
/// the validated semantic level.
#[derive(Clone, Debug)]
pub struct OpenedPackage {
    /// Package manifest.
    pub manifest: Manifest,
    /// Validated, authoring-prepared level semantics.
    pub level: LevelDef,
}

/// One prepared quality variant, decoded and ready for installation.
pub struct LoadedVariant {
    /// The lightmap quality this variant answers.
    pub lightmap_quality: LightmapQuality,
    /// Prepared static geometry.
    pub mesh: LevelMesh,
    /// Prepared static prop batches, with textures attached from the catalog
    /// models the manifest identifies.
    pub props: Vec<PropMeshBatch>,
    /// The compiler's baked lighting.
    pub lighting: LevelLighting,
    /// Prepared lightmap atlas, absent for the vertex-lit variant or a
    /// recorded fill fallback.
    pub lightmaps: Option<Arc<LevelLightmaps>>,
    /// The prepared irradiance field for moving objects, when the variant has
    /// one.
    pub irradiance: Option<Arc<crate::lighting::probes::ProbeField>>,
    /// Compiled static collision.
    pub collision: CompiledCollision,
    /// The baked navigation grid this variant was compiled with.
    pub navigation: crate::package::navigation::NavGrid,
    /// Prepared reflection probe captures, keyed by face size.
    pub probes: ProbeCaptures,
}

/// Reflection probe captures for both runtime face sizes.
#[derive(Clone, Debug, Default)]
pub struct ProbeCaptures {
    /// 48-texel captures (Reflections Medium).
    pub medium: Option<ProbeCapture>,
    /// 64-texel captures (Reflections Full).
    pub full: Option<ProbeCapture>,
}

impl ProbeCaptures {
    /// True when both runtime face sizes are present.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.medium.is_some() && self.full.is_some()
    }

    /// The capture for one face size.
    #[must_use]
    pub const fn for_face_size(&self, face_edge: u32) -> Option<&ProbeCapture> {
        match face_edge {
            crate::render::PROBE_FACE_SIZE_MEDIUM => self.medium.as_ref(),
            crate::render::PROBE_FACE_SIZE_FULL => self.full.as_ref(),
            _ => None,
        }
    }
}

/// One face-size set of probe captures.
#[derive(Clone, Debug)]
pub struct ProbeCapture {
    /// Base face edge in texels.
    pub face_edge: u32,
    /// Mip levels every probe chain carries, base level first.
    pub levels: u32,
    /// Probe positions, in routing order.
    pub points: Vec<[f32; 3]>,
    /// One mip chain per probe, in routing order: level 0 first, six RGBA8
    /// faces per level, each level's faces sized `(face_edge >> level)`.
    pub chains: Vec<Vec<[Vec<u8>; 6]>>,
}

/// Positions record of one probe payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbePositions {
    /// Record version.
    pub record_version: u16,
    /// Face edge this payload was captured at.
    pub face_edge: u32,
    /// Mip levels every probe chain in this payload carries.
    pub levels: u32,
    /// Probe positions in routing order.
    pub points: Vec<[f32; 3]>,
}

/// Version of the probe positions record.
///
/// Version 2 added the prefiltered mip chain; there is no reader for an older
/// record: a version-1 package is superseded and must be rebuilt.
pub const PROBE_POSITIONS_VERSION: u16 = 2;

/// Opens a package and reads its manifest and semantics.
///
/// # Errors
///
/// Returns an error when the archive, manifest, semantics or level validation
/// fails, or the semantics entry's hash does not match the manifest.
pub fn open(path: &Path) -> Result<OpenedPackage, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    open_reader(PackageReader::new(file).map_err(|error| error.to_string())?)
}

/// Opens an in-memory package, used by the embedded fallback demo.
///
/// # Errors
///
/// Returns the same errors as [`open`].
pub fn open_bytes(bytes: &[u8]) -> Result<OpenedPackage, String> {
    open_reader(package_reader(bytes)?)
}

fn package_reader(bytes: &[u8]) -> Result<PackageReader<std::io::Cursor<&[u8]>>, String> {
    PackageReader::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())
}

fn open_reader<R: std::io::Read + std::io::Seek>(
    mut reader: PackageReader<R>,
) -> Result<OpenedPackage, String> {
    let manifest_bytes = reader.read_entry("manifest.json", crate::package::MAX_MANIFEST_BYTES)?;
    let names = reader.names().to_vec();
    let manifest = Manifest::from_json(&manifest_bytes, &names)?;
    let semantics = read_named_entry(
        &mut reader,
        &manifest,
        "semantics.json",
        MAX_SEMANTICS_BYTES,
    )?;
    let text = std::str::from_utf8(&semantics)
        .map_err(|error| format!("semantics.json is not UTF-8: {error}"))?;
    let level = LevelDef::from_json(text)
        .map_err(|error| format!("package semantics are not valid: {error}"))?;
    crate::loader::validate_level(&level)?;
    check_dependencies(&manifest)?;
    Ok(OpenedPackage { manifest, level })
}

/// Enforces the declared dependency identities before a package is used.
///
/// With an installed asset bundle, a declared model or texture must exist with
/// exactly the recorded size; a substituted or resized resource is a load
/// error naming the dependency. A truly asset-less install (the embedded
/// fallback boot) has nothing to check against and logs once instead.
fn check_dependencies(manifest: &Manifest) -> Result<(), String> {
    let Some(root) = crate::assets::resolve_asset_root() else {
        crate::logging::warn_once(
            "package-dependencies-no-root",
            "[levels] no asset root: package dependency identities cannot be checked",
        );
        return Ok(());
    };
    for dependency in &manifest.dependencies {
        if dependency.kind == crate::package::DependencyKind::Embedded {
            continue;
        }
        let path = root.join(&dependency.path);
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() == dependency.bytes => {}
            Ok(metadata) => {
                return Err(format!(
                    "package dependency '{}' is {} bytes, recorded {}",
                    dependency.path,
                    metadata.len(),
                    dependency.bytes
                ));
            }
            Err(_) => {
                return Err(format!(
                    "package dependency '{}' is missing",
                    dependency.path
                ));
            }
        }
    }
    Ok(())
}

/// Reads only the manifest of a package, verifying its structure and hashes.
///
/// # Errors
///
/// Returns an error when the archive cannot be opened or the manifest is
/// malformed.
pub fn inspect(path: &Path) -> Result<Manifest, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let mut reader = PackageReader::new(file).map_err(|error| error.to_string())?;
    let bytes = reader.read_entry("manifest.json", crate::package::MAX_MANIFEST_BYTES)?;
    let names = reader.names().to_vec();
    Manifest::from_json(&bytes, &names)
}

/// Content identity of the package a level entry names.
///
/// For an installed package this hashes the canonical manifest record — the
/// package's own statement of what it contains — rather than the file bytes, so
/// the worker can coalesce requests and key its cache without reading every
/// prepared record again. The embedded fallback hashes its embedded bytes once
/// per process.
///
/// # Errors
///
/// Returns an error when the package cannot be opened or its manifest is
/// malformed.
pub fn package_identity(entry: &crate::loader::LevelEntry) -> Result<String, String> {
    match entry.source_type {
        crate::loader::LevelSourceType::Embedded => {
            static EMBEDDED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
            Ok(EMBEDDED
                .get_or_init(|| {
                    crate::package::hash::sha256_hex(crate::loader::embedded_demo_package())
                })
                .clone())
        }
        crate::loader::LevelSourceType::Bundled | crate::loader::LevelSourceType::Installed => {
            let manifest = inspect(&entry.path)?;
            let canonical = manifest.to_json()?;
            Ok(crate::package::hash::sha256_hex(&canonical))
        }
    }
}

/// Reads one quality variant and attaches its prop textures from the catalog.
///
/// # Errors
///
/// Returns an error when the variant is not declared, any record fails to
/// decode or validate, a declared probe payload is missing, or the variant's
/// reflection captures are incomplete.
pub fn load_variant(
    path: &Path,
    manifest: &Manifest,
    quality: LightmapQuality,
    assets: &mut crate::props::PropAssets,
) -> Result<LoadedVariant, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let mut reader = PackageReader::new(file).map_err(|error| error.to_string())?;
    decode_named_variant(&mut reader, manifest, quality, assets)
}

/// Reads one quality variant of an in-memory package.
///
/// # Errors
///
/// Returns the same errors as [`load_variant`].
pub fn load_variant_bytes(
    bytes: &[u8],
    manifest: &Manifest,
    quality: LightmapQuality,
    assets: &mut crate::props::PropAssets,
) -> Result<LoadedVariant, String> {
    let mut reader = package_reader(bytes)?;
    decode_named_variant(&mut reader, manifest, quality, assets)
}

fn decode_named_variant<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    quality: LightmapQuality,
    assets: &mut crate::props::PropAssets,
) -> Result<LoadedVariant, String> {
    let name = quality.name();
    let variant = manifest
        .variant(name)
        .ok_or_else(|| format!("package '{}' has no '{name}' variant", manifest.id))?;
    decode_variant(reader, variant, quality, assets)
}

fn decode_variant<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    variant: &Variant,
    quality: LightmapQuality,
    assets: &mut crate::props::PropAssets,
) -> Result<LoadedVariant, String> {
    let mesh_bytes = reader.read_blob(&variant.entries.mesh, MAX_BINARY_BYTES)?;
    let mesh = read_mesh(&mesh_bytes)?;
    let props_bytes = reader.read_blob(&variant.entries.props, MAX_BINARY_BYTES)?;
    let mut props = read_props(&props_bytes)?;
    for batch in &mut props {
        match assets.resolve(&batch.model) {
            Ok(asset) => {
                batch.textures = asset.model.textures.iter().cloned().map(Arc::new).collect();
            }
            Err(error) => {
                // A missing model degrades to the diagnostic sheet, exactly
                // like a missing surface texture: the world still boots (the
                // empty-install recovery path has no asset tree at all), and
                // the failure is reported once per model.
                crate::logging::warn_once(
                    format!("package-model:{}", batch.model),
                    format!(
                        "[props] compiled prop model '{}' is unavailable ({error}); \
                         drawing the diagnostic sheet",
                        batch.model
                    ),
                );
                let slots = batch
                    .submeshes
                    .iter()
                    .filter_map(|submesh| submesh.texture)
                    .max()
                    .map_or(1_usize, |slot| usize::from(slot).saturating_add(1));
                let diagnostic = Arc::new(crate::materials::missing_texture());
                batch.textures = std::iter::repeat_n(diagnostic, slots).collect();
            }
        }
    }
    let lighting_bytes = reader.read_blob(&variant.entries.lighting, MAX_LIGHTING_BYTES)?;
    let lighting = read_lighting(&lighting_bytes)?;
    let collision_bytes = reader.read_blob(&variant.entries.collision, MAX_COLLISION_BYTES)?;
    let collision = read_collision(&collision_bytes)?;
    let navigation_bytes = reader.read_blob(&variant.entries.navigation, MAX_NAVIGATION_BYTES)?;
    let navigation = crate::package::navigation::read_navigation(&navigation_bytes)?;
    let lightmaps = match (&variant.entries.lightmaps, &variant.entries.lightmaps_meta) {
        (Some(pages), Some(meta)) => {
            let page_bytes = reader.read_blob(pages, MAX_LIGHTMAP_ATLAS_BYTES)?;
            let meta_bytes = read_entry_checked(reader, meta, MAX_LIGHTMAP_METADATA_BYTES)?;
            Some(Arc::new(read_lightmaps(&meta_bytes, &page_bytes)?))
        }
        _ => None,
    };
    let irradiance = match &variant.entries.irradiance {
        Some(entry) => {
            let bytes = reader.read_blob(entry, MAX_ENTRY_BYTES)?;
            let field = crate::lighting::probes::ProbeField::read(&bytes)?;
            validate_probe_rooms(&field, lighting.rooms().len())?;
            Some(Arc::new(field))
        }
        None => None,
    };
    let probes = read_probes(reader, variant)?;
    Ok(LoadedVariant {
        lightmap_quality: quality,
        mesh,
        props,
        lighting,
        lightmaps,
        irradiance,
        collision,
        navigation,
        probes,
    })
}

fn validate_probe_rooms(
    field: &crate::lighting::probes::ProbeField,
    rooms: usize,
) -> Result<(), String> {
    for probe in &field.probes {
        if probe.room >= 0_i32 && usize::try_from(probe.room).map_or(true, |room| room >= rooms) {
            return Err(format!(
                "irradiance probe references missing room {}",
                probe.room
            ));
        }
    }
    Ok(())
}

fn read_probes<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    variant: &Variant,
) -> Result<ProbeCaptures, String> {
    let mut captures = ProbeCaptures::default();
    for payload in &variant.entries.probes {
        let positions_bytes = read_entry_checked(reader, &payload.positions, MAX_MATERIALS_BYTES)?;
        let positions: ProbePositions = serde_json::from_slice(&positions_bytes)
            .map_err(|error| format!("probe positions record is not valid JSON: {error}"))?;
        validate_positions_record(&positions, payload)?;
        let mut chains = Vec::with_capacity(payload.cubemaps.len());
        for cubemap in &payload.cubemaps {
            let cube_bytes = reader.read_blob(cubemap, MAX_ENTRY_BYTES)?;
            chains.push(split_cube(&cube_bytes, payload.face_edge, payload.levels)?);
        }
        let capture = ProbeCapture {
            face_edge: payload.face_edge,
            levels: payload.levels,
            points: positions.points,
            chains,
        };
        match payload.face_edge {
            48 => captures.medium = Some(capture),
            64 => captures.full = Some(capture),
            other => {
                return Err(format!("unsupported probe face edge {other}"));
            }
        }
    }
    Ok(captures)
}

/// Validates one positions record against the manifest payload it belongs to.
///
/// The reader accepts the current record version only: a package prepared
/// before the offline mip chain carries version 1 and is superseded, not
/// adapted.
fn validate_positions_record(
    positions: &ProbePositions,
    payload: &ProbePayload,
) -> Result<(), String> {
    if positions.record_version != PROBE_POSITIONS_VERSION {
        return Err(format!(
            "probe positions record version {} is not supported",
            positions.record_version
        ));
    }
    if positions.face_edge != payload.face_edge || positions.levels != payload.levels {
        return Err("probe positions record does not match its payload".to_string());
    }
    if positions.points.is_empty() {
        return Err("probe positions record has no points".to_string());
    }
    if u32::try_from(positions.points.len()).unwrap_or(u32::MAX) != payload.count {
        return Err("probe positions count does not match the manifest".to_string());
    }
    for point in &positions.points {
        if !point.iter().all(|value| value.is_finite()) {
            return Err("probe position is non-finite".to_string());
        }
    }
    Ok(())
}

/// Splits a decoded cube KTX2 into its full mip chain of six face buffers.
fn split_cube(bytes: &[u8], face_edge: u32, levels: u32) -> Result<Vec<[Vec<u8>; 6]>, String> {
    let image: Ktx2Rgba8 = ktx2::read_rgba8(bytes)?;
    if image.faces != 6 || image.layers != 0 || image.edge != face_edge {
        return Err("probe cubemap shape does not match its payload".to_string());
    }
    split_cube_levels(&image, face_edge, levels)
}

/// Checks the decoded chain's level count and per-level byte length and splits
/// every level into six face buffers.
fn split_cube_levels(
    image: &Ktx2Rgba8,
    face_edge: u32,
    levels: u32,
) -> Result<Vec<[Vec<u8>; 6]>, String> {
    if u32::try_from(image.levels.len()).unwrap_or(u32::MAX) != levels {
        return Err(format!(
            "probe cubemap has {} mip levels; the record says {levels}",
            image.levels.len()
        ));
    }
    let mut chain = Vec::with_capacity(image.levels.len());
    for (level, data) in image.levels.iter().enumerate() {
        let level_index = u32::try_from(level)
            .map_err(|error| format!("probe mip level is too large: {error}"))?;
        let level_edge = face_edge
            .checked_shr(level_index)
            .filter(|edge| *edge > 0)
            .ok_or_else(|| format!("probe cubemap has no mip level {level}"))?;
        let face_bytes = usize::try_from(level_edge)
            .map_err(|error| format!("probe face edge is too large: {error}"))?
            .checked_mul(
                usize::try_from(level_edge)
                    .map_err(|error| format!("probe face edge is too large: {error}"))?,
            )
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| "probe face size overflows".to_string())?;
        if data.len() != face_bytes.saturating_mul(6) {
            return Err(format!(
                "probe cubemap level {level} holds {} bytes, expected {}",
                data.len(),
                face_bytes.saturating_mul(6)
            ));
        }
        let mut faces: Vec<Vec<u8>> = Vec::with_capacity(6);
        for face in data.chunks_exact(face_bytes) {
            faces.push(face.to_vec());
        }
        chain.push(faces.try_into().map_err(|incomplete_faces: Vec<Vec<u8>>| {
            format!(
                "probe cubemap holds {} faces instead of six",
                incomplete_faces.len()
            )
        })?);
    }
    Ok(chain)
}

/// Validates packaged captures against the emitted geometry and materials.
///
/// The player never captures probes, so a mismatched or incomplete payload is
/// a load error: installing the world without its reflections would silently
/// change what the level looks like.
///
/// # Errors
///
/// Returns an error when the routing expects probes and either face size is
/// absent, or the captured points do not match the routing's probe points.
pub fn validate_probe_captures(
    mesh: &LevelMesh,
    materials: &MaterialTable,
    probes: &ProbeCaptures,
) -> Result<(), String> {
    let state = crate::render::MaterialRenderState::from_table(materials);
    let routing =
        crate::render::routing_from_mesh(mesh, &state.reflections, materials.entries().len());
    if routing.probe_points.is_empty() {
        return Ok(());
    }
    if !probes.complete() {
        return Err(format!(
            "package is missing its reflection captures ({} probe point(s)); \
             rebuild it with the probe capture step",
            routing.probe_points.len()
        ));
    }
    for (face_edge, capture) in [
        (
            crate::render::PROBE_FACE_SIZE_MEDIUM,
            probes.medium.as_ref(),
        ),
        (crate::render::PROBE_FACE_SIZE_FULL, probes.full.as_ref()),
    ] {
        let Some(resident_capture) = capture else {
            return Err("package is missing a reflection capture face size".to_string());
        };
        if resident_capture.face_edge != face_edge
            || resident_capture.points.len() != routing.probe_points.len()
        {
            return Err("reflection capture shape does not match the emitted geometry".to_string());
        }
        for (captured, expected) in resident_capture.points.iter().zip(&routing.probe_points) {
            let close = captured
                .iter()
                .zip(expected.iter())
                .all(|(a, b)| (a - b).abs() <= 1.0e-3);
            if !close {
                return Err(
                    "reflection capture positions do not match the emitted geometry".to_string(),
                );
            }
        }
    }
    Ok(())
}

fn read_named_entry<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = manifest
        .entry(name)
        .ok_or_else(|| format!("manifest does not declare '{name}'"))?;
    let bytes = reader.read_entry(name, limit)?;
    let actual = crate::package::hash::sha256_hex(&bytes);
    if actual != entry.sha256 {
        return Err(format!(
            "package entry '{name}' has hash {actual}, manifest says {}",
            entry.sha256
        ));
    }
    Ok(bytes)
}

fn read_entry_checked<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let bytes = reader.read_entry(name, limit)?;
    if name.starts_with("blobs/") {
        let expected = crate::package::hash::sha256_from_blob_name(name)
            .ok_or_else(|| format!("package blob '{name}' has no content hash"))?;
        let actual = crate::package::hash::sha256_hex(&bytes);
        if expected != actual {
            return Err(format!(
                "package blob '{name}' has hash {actual}, its name says {expected}"
            ));
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    #[test]
    fn irradiance_labels_must_reference_a_packaged_room() {
        use crate::lighting::probes::{ProbeField, ProbeSample};
        let mut field = ProbeField {
            min: [0.0; 3],
            cell_m: 1.0,
            dims: [1; 3],
            probes: vec![ProbeSample {
                room: -1,
                ..ProbeSample::default()
            }],
        };
        assert!(validate_probe_rooms(&field, 0).is_ok());
        field.probes[0].room = 0_i32;
        assert!(validate_probe_rooms(&field, 1).is_ok());
        assert!(validate_probe_rooms(&field, 0).is_err());
        field.probes[0].room = 1_i32;
        assert!(validate_probe_rooms(&field, 1).is_err());
    }

    /// A cube of solid `edge`-texel faces.
    fn solid_faces(edge: u32, colour: [u8; 4]) -> [Vec<u8>; 6] {
        let bytes = usize::try_from(edge).unwrap() * usize::try_from(edge).unwrap() * 4;
        std::array::from_fn(|_| {
            let mut face = Vec::with_capacity(bytes);
            for _ in 0..bytes / 4 {
                face.extend_from_slice(&colour);
            }
            face
        })
    }

    fn payload(face_edge: u32, levels: u32, count: u32) -> ProbePayload {
        ProbePayload {
            face_edge,
            levels,
            count,
            cubemaps: (0..count)
                .map(|i| format!("blobs/{i}.probe.ktx2"))
                .collect(),
            positions: "blobs/positions.probes.json".to_string(),
        }
    }

    fn positions(face_edge: u32, levels: u32, points: Vec<[f32; 3]>) -> ProbePositions {
        ProbePositions {
            record_version: PROBE_POSITIONS_VERSION,
            face_edge,
            levels,
            points,
        }
    }

    #[test]
    fn a_multi_level_cube_splits_into_its_whole_chain() {
        let chain = vec![
            solid_faces(8, [1, 2, 3, 255]),
            solid_faces(4, [4, 5, 6, 255]),
        ];
        let bytes = ktx2::write_rgba8_cube_with_mips(8, &chain).expect("encode");
        let split = split_cube(&bytes, 8, 2).expect("split");
        assert_eq!(split.len(), 2);
        assert_eq!(split[0], chain[0]);
        assert_eq!(split[1], chain[1]);
    }

    #[test]
    fn a_level_count_mismatch_is_refused() {
        let chain = vec![
            solid_faces(8, [1, 2, 3, 255]),
            solid_faces(4, [4, 5, 6, 255]),
        ];
        let bytes = ktx2::write_rgba8_cube_with_mips(8, &chain).expect("encode");
        // The payload claims a chain the cube does not carry.
        assert!(split_cube(&bytes, 8, 1).is_err(), "too few levels claimed");
        assert!(
            split_cube(&bytes, 8, 3).is_err(),
            "more levels claimed than the cube carries"
        );
    }

    #[test]
    fn a_wrong_level_byte_length_is_refused() {
        let image = Ktx2Rgba8 {
            edge: 4,
            layers: 0,
            faces: 6,
            levels: vec![vec![0; 4 * 4 * 4 * 6], vec![0; 3]],
        };
        let error = split_cube_levels(&image, 4, 2).expect_err("level 1 is too short");
        assert!(
            error.contains("level 1"),
            "the message names the level: {error}"
        );
    }

    #[test]
    fn a_version_one_positions_record_is_refused() {
        let payload = payload(64, 7, 1);
        let mut record = positions(64, 7, vec![[0.0, 0.0, 0.0]]);
        record.record_version = 1;
        let error = validate_positions_record(&record, &payload).expect_err("version 1 is gone");
        assert!(error.contains("version 1"), "the message names it: {error}");
    }

    #[test]
    fn a_positions_record_must_match_its_payload() {
        let payload = payload(64, 7, 1);
        validate_positions_record(&positions(64, 7, vec![[1.0, 2.0, 3.0]]), &payload)
            .expect("a matching record");
        // A different level count, edge, count or non-finite point is refused.
        assert!(
            validate_positions_record(&positions(64, 6, vec![[1.0, 2.0, 3.0]]), &payload).is_err()
        );
        assert!(
            validate_positions_record(&positions(48, 7, vec![[1.0, 2.0, 3.0]]), &payload).is_err()
        );
        assert!(validate_positions_record(&positions(64, 7, Vec::new()), &payload).is_err());
        assert!(
            validate_positions_record(&positions(64, 7, vec![[f32::NAN, 0.0, 0.0]]), &payload)
                .is_err()
        );
    }
}
