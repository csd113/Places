//! Verified owned ZIP snapshots: decode once for validation, copy the original
//! compressed stream at publication. No trust is placed in mutable source files.
use std::cell::RefCell;
use std::fs::File;
use std::io::{self, Read, Seek, Write};

use sha2::{Digest as _, Sha256};

use super::{PackageReader, SpoolEntry, copy_bounded, hash, open_spool_file};

const RECORD: &str = "record";

#[derive(Clone)]
pub struct CompressedRecord {
    pub(super) file_bytes: u64,
    bytes: u64,
    sha256: String,
    method: zip::CompressionMethod,
    crc32: u32,
}

impl<R: Read + Seek> PackageReader<R> {
    pub(crate) fn snapshot_compressed_entry(
        &mut self,
        name: &str,
        mut file: File,
        check: &impl Fn() -> Result<(), String>,
    ) -> Result<(u64, String, CompressedRecord), String> {
        check()?;
        let actual = self
            .view
            .as_ref()
            .and_then(|view| view.get(name))
            .map_or(name, String::as_str);
        let index = self
            .archive
            .index_for_name(actual)
            .ok_or("Missing compressed record")?;
        if self
            .archive
            .by_index_data(index)
            .map_err(|error| error.to_string())?
            .size()
            > crate::engine::package::MAX_BINARY_BYTES
        {
            return Err("Compressed record exceeds binary limit".into());
        }
        {
            let mut writer = zip::ZipWriter::new(Checked {
                inner: &mut file,
                check,
                receipt: None,
            });
            writer
                .raw_copy_file_rename(
                    self.archive
                        .by_index_raw(index)
                        .map_err(|error| error.to_string())?,
                    RECORD,
                )
                .map_err(|error| error.to_string())?;
            let _finished = writer.finish().map_err(|error| error.to_string())?;
        }
        file.rewind().map_err(|error| error.to_string())?;
        // Validate the exact owned snapshot through the ordinary bounded decoder:
        // history/ratio, exact output, CRC, frame termination and cancellation.
        let mut snapshot = PackageReader::new(&mut file).map_err(|error| error.to_string())?;
        let (bytes, digest) = snapshot.copy_entry_to(
            RECORD,
            crate::engine::package::MAX_BINARY_BYTES,
            &mut io::sink(),
            check,
        )?;
        if hash::sha256_from_blob_name(actual).is_some_and(|expected| expected != digest) {
            return Err(format!("project record hash mismatch: {actual}"));
        }
        let raw = snapshot
            .archive
            .by_index_raw(0)
            .map_err(|error| error.to_string())?;
        let compressed_bytes = raw.compressed_size();
        let method = raw.compression();
        let crc32 = raw.crc32();
        let (_, compressed_digest) = copy_bounded(
            raw,
            &mut io::sink(),
            compressed_bytes,
            super::MAX_TRANSPORT_BYTES,
            check,
        )?;
        drop(snapshot);
        Ok((
            bytes,
            digest,
            CompressedRecord {
                file_bytes: file.metadata().map_err(|error| error.to_string())?.len(),
                bytes: compressed_bytes,
                sha256: compressed_digest,
                method,
                crc32,
            },
        ))
    }
}

pub(super) fn write(
    writer: &mut zip::ZipWriter<File>,
    entry: &SpoolEntry,
    record: &CompressedRecord,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let input = open_spool_file(entry)?;
    let receipt = RefCell::new(None);
    let mut snapshot = PackageReader::new(Checked {
        inner: input,
        check,
        receipt: Some(&receipt),
    })
    .map_err(|error| error.to_string())?;
    if snapshot.names() != [RECORD] {
        return Err("Compressed spool must contain exactly one record".into());
    }
    let raw = snapshot
        .archive
        .by_index_raw(0)
        .map_err(|error| error.to_string())?;
    if raw.size() != entry.bytes
        || raw.compressed_size() != record.bytes
        || raw.crc32() != record.crc32
        || raw.compression() != record.method
    {
        return Err("Compressed spool metadata changed".into());
    }
    // Hash precisely the compressed payload read by raw_copy, not a separate
    // preflight read. A same-length spool edit cannot race validation/publication.
    *receipt.borrow_mut() = Some((Sha256::new(), 0_u64));
    writer
        .raw_copy_file_rename(raw, &entry.name)
        .map_err(|error| error.to_string())?;
    drop(snapshot);
    let (hasher, count) = receipt
        .into_inner()
        .ok_or("Missing compressed copy receipt")?;
    if count != record.bytes || hash::digest_hex(&hasher.finalize()) != record.sha256 {
        return Err("Compressed spool payload changed".into());
    }
    check()
}

type Receipt = RefCell<Option<(Sha256, u64)>>;
struct Checked<'a, T, F> {
    inner: T,
    check: &'a F,
    receipt: Option<&'a Receipt>,
}
impl<T: Read, F: Fn() -> Result<(), String>> Read for Checked<'_, T, F> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        (self.check)().map_err(io::Error::other)?;
        let count = self.inner.read(buffer)?;
        if let Some(receipt) = self.receipt
            && let Some((hasher, bytes)) = receipt.borrow_mut().as_mut()
        {
            hasher.update(
                buffer
                    .get(..count)
                    .ok_or_else(|| io::Error::other("Invalid copy count"))?,
            );
            *bytes = bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        }
        Ok(count)
    }
}
impl<T: Write, F: Fn() -> Result<(), String>> Write for Checked<'_, T, F> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        (self.check)().map_err(io::Error::other)?;
        self.inner.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<T: Seek, F> Seek for Checked<'_, T, F> {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}
