//! Sub-100 KiB adversarial ZIP fixtures uniquely protect the method-93 window,
//! CRC, exact-output and single-frame contract, plus legacy and streamed paths.
//! No assets, maps, external compressor, renderer or timing campaign is used.

use std::io::{self, Cursor, Read, Seek, SeekFrom, Write as _};

use crate::engine::package::PackageReader;

const ENTRY: &str = "blobs/codec-fixture";

// The same tiny frame also protects valid one-byte reads. Header peeking must
// not confuse a short Read chunk with a truncated frame; no second fixture or
// bake is needed, and the bounded O(frame) cursor work takes milliseconds.
struct ShortReads<R>(R);
impl<R: Read> Read for ShortReads<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = buffer.len().min(1);
        self.0.read(
            buffer
                .get_mut(..length)
                .ok_or_else(|| io::Error::other("short-read fixture buffer"))?,
        )
    }
}
impl<R: Seek> Seek for ShortReads<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.0.seek(position)
    }
}

fn replace(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), String> {
    let end = offset
        .checked_add(value.len())
        .ok_or("fixture offset overflow")?;
    bytes
        .get_mut(offset..end)
        .ok_or("fixture header missing")?
        .copy_from_slice(value);
    Ok(())
}

fn raw_zstd(frame: &[u8], declared: u32, crc: u32) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            ENTRY,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .map_err(|error| error.to_string())?;
    writer.write_all(frame).map_err(|error| error.to_string())?;
    let mut bytes = writer
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner();
    let central = bytes
        .windows(4)
        .position(|part| part == b"PK\x01\x02")
        .ok_or("central header missing")?;
    for (base, method, checksum, length) in [
        (0_usize, 8_usize, 14_usize, 22_usize),
        (central, 10, 16, 24),
    ] {
        replace(
            &mut bytes,
            base.saturating_add(method),
            &93_u16.to_le_bytes(),
        )?;
        replace(
            &mut bytes,
            base.saturating_add(checksum),
            &crc.to_le_bytes(),
        )?;
        replace(
            &mut bytes,
            base.saturating_add(length),
            &declared.to_le_bytes(),
        )?;
    }
    Ok(bytes)
}

fn read(bytes: Vec<u8>, limit: u64) -> Result<Vec<u8>, String> {
    let mut reader = PackageReader::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    reader.read_entry(ENTRY, limit)
}

#[test]
fn zstd_and_legacy_records_keep_exact_bytes_through_read_and_stream() -> Result<(), String> {
    let payload: Vec<u8> = (0_u32..65_536)
        .flat_map(u32::to_le_bytes)
        .take(65_536)
        .collect();
    let length = u32::try_from(payload.len()).map_err(|error| error.to_string())?;
    let frame =
        zstd::stream::encode_all(Cursor::new(&payload), 9).map_err(|error| error.to_string())?;
    let archive = raw_zstd(&frame, length, crc32fast::hash(&payload))?;
    assert_eq!(
        read(archive.clone(), u64::from(length))?,
        payload,
        "bounded Zstandard preserves every byte"
    );
    let mut chunked = PackageReader::new(ShortReads(Cursor::new(archive.clone())))
        .map_err(|error| error.to_string())?;
    assert_eq!(
        chunked.read_entry(ENTRY, u64::from(length))?,
        payload,
        "a valid short-reading stream is not a truncated Zstandard frame"
    );
    let mut reader = PackageReader::new(Cursor::new(archive)).map_err(|error| error.to_string())?;
    let mut output = Vec::new();
    let copied = reader.copy_entry_to(ENTRY, u64::from(length), &mut output, &|| Ok(()))?;
    assert_eq!(
        output, payload,
        "project source streaming uses the same bounded decoder"
    );
    assert_eq!(copied.0, u64::from(length), "streamed length stays exact");
    // The explicit compact profile must keep bootstrap JSON at a valid
    // Deflate level while its blobs use Zstandard-19. Reuse the 64 KiB bytes.
    let mut compact = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("manifest.json", b"bootstrap".as_slice()),
        (ENTRY, payload.as_slice()),
    ] {
        compact
            .start_file(
                name,
                crate::engine::package::archive::file_options(name, 19),
            )
            .map_err(|error| error.to_string())?;
        compact
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
    }
    let encoded = compact
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner();
    let mut compact_reader =
        PackageReader::new(Cursor::new(encoded)).map_err(|error| error.to_string())?;
    assert_eq!(
        compact_reader.read_entry("manifest.json", 1024)?,
        b"bootstrap"
    );
    assert_eq!(
        compact_reader.read_entry(ENTRY, u64::from(length))?,
        payload
    );
    for method in [
        zip::CompressionMethod::Stored,
        zip::CompressionMethod::Deflated,
    ] {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                ENTRY,
                zip::write::SimpleFileOptions::default().compression_method(method),
            )
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&payload)
            .map_err(|error| error.to_string())?;
        let legacy = writer
            .finish()
            .map_err(|error| error.to_string())?
            .into_inner();
        assert_eq!(
            read(legacy, u64::from(length))?,
            payload,
            "existing codecs remain readable"
        );
    }
    Ok(())
}

#[test]
fn zstd_refuses_window_output_ratio_crc_and_frame_hazards() -> Result<(), String> {
    let payload = b"bounded payload";
    let length = u32::try_from(payload.len()).map_err(|error| error.to_string())?;
    let crc = crc32fast::hash(payload);
    let frame =
        zstd::stream::encode_all(Cursor::new(payload), 9).map_err(|error| error.to_string())?;
    for end in 0..frame.len() {
        let truncated = frame.get(..end).ok_or("truncation range")?;
        assert!(
            read(raw_zstd(truncated, length, crc)?, 1024).is_err(),
            "every truncated frame must fail, at {end}"
        );
    }
    let mut trailing = frame.clone();
    trailing.extend_from_slice(&frame);
    assert!(
        read(raw_zstd(&trailing, length, crc)?, 1024).is_err(),
        "concatenated frames must fail"
    );
    assert!(
        read(raw_zstd(&frame, length, crc.wrapping_add(1))?, 1024).is_err(),
        "raw ZIP CRC remains checked"
    );
    assert!(
        read(raw_zstd(&frame, length.saturating_sub(1), crc)?, 1024).is_err(),
        "decoder output cannot exceed declaration"
    );
    assert!(
        read(raw_zstd(&frame, length.saturating_add(1), crc)?, 1024).is_err(),
        "decoder output cannot fall short of declaration"
    );
    assert!(
        read(raw_zstd(&frame, length, crc)?, 1).is_err(),
        "per-entry limit applies before decoding"
    );
    let ratio_error = read(raw_zstd(&frame, 256 * 1024 * 1024, crc)?, 256 * 1024 * 1024)
        .err()
        .ok_or("unbounded ratio accepted")?;
    assert!(
        ratio_error.contains("ratio"),
        "ratio is refused before reserving output: {ratio_error}"
    );
    let skippable = [0x50_u8, 0x2a, 0x4d, 0x18, 0, 0, 0, 0];
    assert!(
        read(raw_zstd(&skippable, 0, 0)?, 1).is_err(),
        "skippable frames cannot hide bytes behind an empty decoded identity"
    );
    // Standard frame, no content-size field, 16 MiB window, last empty block.
    // This requests a larger history even though its decoded output is empty.
    let oversized_window = [0x28_u8, 0xb5, 0x2f, 0xfd, 0, 112, 1, 0, 0];
    let window_error = read(raw_zstd(&oversized_window, 0, 0)?, 1)
        .err()
        .ok_or("oversized window accepted")?;
    assert!(
        window_error.contains("memory"),
        "explicit history bound rejects tiny hostile frame: {window_error}"
    );
    Ok(())
}
