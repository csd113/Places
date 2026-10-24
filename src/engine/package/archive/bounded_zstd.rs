//! Independent ZIP Zstandard entries with explicit window and output bounds.
//!
//! ZIP's default Zstandard adapter has no window limit. Always enter method 93
//! through the raw entry and this decoder, including streamed project recovery.

use std::io::{self, BufRead as _, Read, Seek};

/// Eight MiB of history; compression levels 1..=19 stay within this contract.
const WINDOW_LOG_MAX: u32 = 23;
/// Conservative admission charge for history, codec state and input buffering.
const DECODER_RESERVATION_BYTES: usize = 16 * 1024 * 1024;
const INPUT_BUFFER_BYTES: usize = 64 * 1024;
const MAX_EXPANSION_RATIO: u64 = 65_536;
const RATIO_ALLOWANCE_BYTES: u64 = 64 * 1024;

pub(super) fn open<'a, R: Read + Seek>(
    archive: &'a mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<(u64, Box<dyn Read + 'a>), String> {
    let index = archive
        .index_for_name(name)
        .ok_or_else(|| format!("package entry '{name}' is missing"))?;
    let metadata = archive
        .by_index_data(index)
        .map_err(|error| error.to_string())?;
    let declared = metadata.size();
    if declared > limit {
        return Err(format!(
            "package entry '{name}' declares {declared} bytes (limit {limit})"
        ));
    }
    if metadata.compression() != zip::CompressionMethod::Zstd {
        return archive
            .by_index(index)
            .map(|entry| {
                let input: Box<dyn Read> = Box::new(entry);
                (declared, input)
            })
            .map_err(|error| format!("package entry '{name}': {error}"));
    }
    let compressed = metadata.compressed_size();
    let crc = metadata.crc32();
    if declared
        > compressed
            .saturating_mul(MAX_EXPANSION_RATIO)
            .saturating_add(RATIO_ALLOWANCE_BYTES)
    {
        return Err(format!(
            "package entry '{name}' exceeds the Zstandard expansion ratio bound"
        ));
    }
    let reservation = crate::engine::resources::allocation(DECODER_RESERVATION_BYTES)?;
    let mut raw = archive
        .by_index_raw(index)
        .map_err(|error| error.to_string())?;
    // Read permits short chunks. Validate an exact prefix, then replay those
    // four bytes to the decoder without seeking or allocating another input.
    let mut magic = [0_u8; 4];
    raw.read_exact(&mut magic)
        .map_err(|error| format!("Zstandard frame prefix: {error}"))?;
    if magic != [0x28_u8, 0xb5, 0x2f, 0xfd] {
        return Err("Zstandard entry must contain one ordinary frame".into());
    }
    let input = io::BufReader::with_capacity(INPUT_BUFFER_BYTES, io::Cursor::new(magic).chain(raw));
    let mut decoder = zstd::stream::read::Decoder::with_buffer(input)
        .map_err(|error| format!("Zstandard decoder: {error}"))?;
    decoder
        .window_log_max(WINDOW_LOG_MAX)
        .map_err(|error| format!("Zstandard window bound: {error}"))?;
    Ok((
        declared,
        Box::new(Entry {
            decoder: decoder.single_frame(),
            crc: crc32fast::Hasher::new(),
            expected_crc: crc,
            declared,
            produced: 0,
            finished: false,
            _reservation: reservation,
        }),
    ))
}

type FrameInput<'a, R> = io::BufReader<io::Chain<io::Cursor<[u8; 4]>, zip::read::ZipFile<'a, R>>>;

struct Entry<'a, R: Read> {
    decoder: zstd::stream::read::Decoder<'static, FrameInput<'a, R>>,
    crc: crc32fast::Hasher,
    expected_crc: u32,
    declared: u64,
    produced: u64,
    finished: bool,
    _reservation: crate::engine::resources::Reservation,
}

impl<R: Read> Read for Entry<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.finished {
            return Ok(0);
        }
        let count = self.decoder.read(buffer)?;
        if count == 0 {
            // Single-frame decoding stops at the frame boundary, even when the
            // ZIP record still has bytes. Reject concatenated/skippable frames
            // and arbitrary suffixes rather than hiding them behind output EOF.
            if !self.decoder.get_mut().fill_buf()?.is_empty() {
                return Err(io::Error::other("Zstandard entry has trailing frame bytes"));
            }
            if self.produced != self.declared {
                return Err(io::Error::other(
                    "Zstandard entry decoded length disagrees with ZIP",
                ));
            }
            if self.crc.clone().finalize() != self.expected_crc {
                return Err(io::Error::other("Zstandard entry CRC mismatch"));
            }
            self.finished = true;
        } else {
            self.produced = self
                .produced
                .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
            if self.produced > self.declared {
                return Err(io::Error::other(
                    "Zstandard entry exceeds its declared output bound",
                ));
            }
            self.crc.update(
                buffer
                    .get(..count)
                    .ok_or_else(|| io::Error::other("Zstandard read exceeds buffer"))?,
            );
        }
        Ok(count)
    }
}
