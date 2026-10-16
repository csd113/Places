//! Exact prepared-prop factoring and compressed archive size selection.
//! Kept separate from validation, lighting preparation and package publication.

use std::collections::BTreeSet;

use super::{BlobMap, insert_blob};
use crate::engine::package::hash::blob_name;
use crate::engine::package::manifest::Variant;

/// Preserve the exact prepared record stream while sharing repeated model
/// data between placements and variants. Already-factored reusable records
/// retain their original identities.
pub(super) fn factor_prepared_props(
    variants: &mut [Variant],
    blobs: &mut BlobMap,
) -> Result<(), String> {
    let names: BTreeSet<String> = variants
        .iter()
        .flat_map(|variant| {
            std::iter::once(variant.entries.props.clone()).chain(
                variant
                    .entries
                    .static_light_off
                    .as_ref()
                    .map(|state| state.props.clone()),
            )
        })
        .collect();
    let source_names: Vec<String> = names
        .into_iter()
        .filter(|name| {
            blobs
                .get(name)
                .is_some_and(|(bytes, _)| bytes.starts_with(b"PLMP"))
        })
        .collect();
    if source_names.is_empty() {
        return Ok(());
    }
    let records: Vec<&[u8]> = source_names
        .iter()
        .map(|name| {
            blobs
                .get(name)
                .map(|(bytes, _)| bytes.as_slice())
                .ok_or_else(|| "prepared prop record is missing".to_string())
        })
        .collect::<Result<_, _>>()?;
    let original_bytes = records
        .iter()
        .fold(0_usize, |total, bytes| total.saturating_add(bytes.len()));
    let factored = crate::engine::package::shared_props::factor_props(&records)?;
    let mut recipes_fit = true;
    for recipe in &factored.recipes {
        if !crate::engine::package::shared_props::recipe_fits_runtime_budget(recipe)? {
            recipes_fit = false;
        }
    }
    if !recipes_fit {
        return Ok(());
    }
    let stored_bytes = factored
        .recipes
        .iter()
        .chain(&factored.segments)
        .fold(0_usize, |total, bytes| total.saturating_add(bytes.len()));
    // A tiny or nonrepeated model set need not pay for a dictionary.
    if stored_bytes >= original_bytes {
        return Ok(());
    }
    let (original_archive_bytes, shared_archive_bytes) =
        prepared_props_archive_sizes(&source_names, &records, &factored)?;
    if shared_archive_bytes >= original_archive_bytes {
        return Ok(());
    }
    for segment in factored.segments {
        let _name = insert_blob(blobs, segment, ".props.shared", "props-shared");
    }
    for (original, recipe) in source_names.iter().zip(factored.recipes) {
        let name = insert_blob(blobs, recipe, ".props.recipe", "props");
        for variant in variants.iter_mut() {
            if variant.entries.props == *original {
                variant.entries.props.clone_from(&name);
            }
            if let Some(state) = variant.entries.static_light_off.as_mut()
                && state.props == *original
            {
                state.props.clone_from(&name);
            }
        }
        drop(blobs.remove(original));
    }
    crate::engine::logging::info(format_args!(
        "[compiler] prepared props exact sharing: {original_bytes} -> {stored_bytes} raw bytes; {original_archive_bytes} -> {shared_archive_bytes} archive bytes including manifest allowance"
    ));
    Ok(())
}

/// Count both storage alternatives with the publication codec before choosing.
/// ZIP names, headers and directory entries are measured by the same writer;
/// a deliberately generous manifest allowance includes each replacement/new
/// entry plus variant references, the capability and JSON framing changes.
fn prepared_props_archive_sizes(
    source_names: &[String],
    records: &[&[u8]],
    factored: &crate::engine::package::shared_props::FactoredProps,
) -> Result<(u64, u64), String> {
    let original = deflated_entries_bytes(
        source_names
            .iter()
            .zip(records)
            .map(|(name, bytes)| (name.as_str(), *bytes)),
    )?;
    let shared_names: Vec<String> = factored
        .recipes
        .iter()
        .map(|bytes| blob_name(bytes, ".props.recipe"))
        .chain(
            factored
                .segments
                .iter()
                .map(|bytes| blob_name(bytes, ".props.shared")),
        )
        .collect();
    let shared = deflated_entries_bytes(
        shared_names
            .iter()
            .zip(factored.recipes.iter().chain(&factored.segments))
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    )?;
    let changed_entries = u64::try_from(shared_names.len())
        .map_err(|error| format!("shared prop manifest entry count is too large: {error}"))?;
    let manifest_allowance = changed_entries
        .checked_mul(1024)
        .and_then(|bytes| bytes.checked_add(1024))
        .ok_or_else(|| "shared prop manifest allowance overflows".to_string())?;
    let proposed = shared
        .checked_add(manifest_allowance)
        .ok_or_else(|| "shared prop archive size estimate overflows".to_string())?;
    Ok((original, proposed))
}

fn deflated_entries_bytes<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<u64, String> {
    use std::io::Write as _;

    let mut writer = zip::ZipWriter::new(ArchiveSizeCounter::default());
    for (name, bytes) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        writer
            .start_file(name, options)
            .map_err(|error| format!("could not estimate prop entry '{name}': {error}"))?;
        writer.write_all(bytes).map_err(|error| {
            format!("could not compress prop entry '{name}' for sizing: {error}")
        })?;
    }
    writer
        .finish()
        .map(|counter| counter.length)
        .map_err(|error| format!("could not finish prop archive size estimate: {error}"))
}

/// A ZIP-compatible seekable sink that retains no compressed payload bytes.
#[derive(Default)]
struct ArchiveSizeCounter {
    position: u64,
    length: u64,
}

impl std::io::Write for ArchiveSizeCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = u64::try_from(bytes.len()).map_err(std::io::Error::other)?;
        self.position = self
            .position
            .checked_add(count)
            .ok_or_else(|| std::io::Error::other("prop archive size counter overflows"))?;
        self.length = self.length.max(self.position);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::io::Seek for ArchiveSizeCounter {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            std::io::SeekFrom::Start(offset) => Some(offset),
            std::io::SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            std::io::SeekFrom::End(offset) => self.length.checked_add_signed(offset),
        };
        self.position = next.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "prop archive size counter seek is out of bounds",
            )
        })?;
        Ok(self.position)
    }
}
