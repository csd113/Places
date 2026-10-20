//! Exact, cross-quality sharing of prepared prop record bytes.
//!
//! PPS1 recipes retain every original PMP1 byte. They split each serialized
//! vertex into exact position, colour, UV and remaining lighting/frame planes,
//! then reference content-interned 65,520-byte chunks. Raw headers,
//! palettes and ordered indices use the same chunk store. Chunks are packed into
//! bounded segments; segment identities and the reconstructed record identity
//! are checked before the existing PMP1 decoder sees the result.
//!
//! Reconstruction allocates one output record and traverses borrowed segment
//! slices. It never concatenates whole vertex attribute planes. Version 1's
//! combined position/colour/UV prefix remains readable.

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

use super::binary::{Reader, Writer};
use super::{MAX_BINARY_BYTES, MAX_ENTRIES, MAX_PROP_BATCHES, MAX_TOTAL_BYTES, MAX_VARIANTS, hash};

/// Largest interned chunk, matching the exact factoring experiment.
pub const SHARED_PROPS_CHUNK_BYTES: usize = 65_520;
/// Largest physical segment; independently fetchable and below the archive entry cap.
pub const MAX_SHARED_PROPS_SEGMENT_BYTES: u64 = 16 * 1024 * 1024;
/// Largest recipe. Counts are additionally bounded by their actual wire storage.
pub const MAX_SHARED_PROPS_RECIPE_BYTES: u64 = 64 * 1024 * 1024;

const MAGIC: [u8; 4] = *b"PPS1";
/// Current exact attribute-plane recipe version; version 1 remains readable.
pub const SHARED_PROPS_RECIPE_VERSION: u16 = 4;
const VERSION: u16 = SHARED_PROPS_RECIPE_VERSION;
const PREFIX_VERSION: u16 = 1;
const ATTRIBUTE_VERSION: u16 = 2;
const SHUFFLED_VERSION: u16 = 3;
const SHUFFLE_VERTICES: usize = 65_536;
const PREVIOUS_SHUFFLE_VERTICES: usize = 256;
const PREFIX_BYTES: usize = 36;
const POSITION_BYTES: usize = 12;
const COLOR_BYTES: usize = 16;
const UV_BYTES: usize = 8;
const COLOR_END: usize = POSITION_BYTES + COLOR_BYTES;
const MAX_CHUNKS: usize = 1 << 20;
const MAX_PARTS: usize = MAX_PROP_BATCHES.saturating_mul(2).saturating_add(1);

/// Encoded recipes in input order and the physical shared segment payloads.
pub struct FactoredProps {
    /// One PPS1 record for each original PMP1 record.
    pub recipes: Vec<Vec<u8>>,
    /// Unique segment payloads, in deterministic first-occurrence order.
    pub segments: Vec<Vec<u8>>,
}

/// A segment required by one recipe, in the order expected by reconstruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedPropsSegment {
    /// Exact uncompressed length.
    pub bytes: u64,
    /// Lowercase SHA-256 of the complete physical segment.
    pub sha256: String,
}

impl SharedPropsSegment {
    /// Content-addressed archive name for this segment.
    #[must_use]
    pub fn blob_name(&self) -> String {
        format!("blobs/{}.props.shared", self.sha256)
    }
}

/// A structurally validated reconstruction recipe.
pub struct PropsRecipe {
    version: u16,
    expanded_bytes: u64,
    sha256: String,
    segments: Vec<SharedPropsSegment>,
    chunks: Vec<ChunkLocation>,
    parts: Vec<Part>,
}

impl PropsRecipe {
    /// Validated recipe revision, including version 1 records retained by reuse.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Original PMP1 record length, bounded by the prepared-record limit.
    #[must_use]
    pub const fn expanded_bytes(&self) -> u64 {
        self.expanded_bytes
    }

    /// Original PMP1 content identity.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Only the physical segments this recipe uses, in reconstruction order.
    #[must_use]
    pub fn segments(&self) -> &[SharedPropsSegment] {
        &self.segments
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkLocation {
    segment: usize,
    offset: usize,
    length: usize,
}

enum Part {
    Raw {
        bytes: usize,
        chunks: Vec<u32>,
    },
    Vertices {
        count: usize,
        stride: usize,
        planes: VertexPlanes,
    },
}

struct VertexPlanes {
    widths: [usize; 4],
    streams: [Vec<u32>; 4],
}

impl Part {
    fn streams(&self) -> [&[u32]; 4] {
        match self {
            Self::Raw { chunks, bytes: _ } => [chunks, &[], &[], &[]],
            Self::Vertices {
                planes,
                count: _,
                stride: _,
            } => planes.streams.each_ref().map(Vec::as_slice),
        }
    }

    fn streams_mut(&mut self) -> [&mut [u32]; 4] {
        match self {
            Self::Raw { chunks, bytes: _ } => [chunks, &mut [], &mut [], &mut []],
            Self::Vertices {
                planes,
                count: _,
                stride: _,
            } => planes.streams.each_mut().map(Vec::as_mut_slice),
        }
    }

    fn bytes(&self) -> Result<usize, String> {
        match self {
            Self::Raw { bytes, chunks: _ } => Ok(*bytes),
            Self::Vertices {
                count,
                stride,
                planes: _,
            } => count
                .checked_mul(*stride)
                .ok_or_else(|| "shared prop vertex run size overflows".to_string()),
        }
    }
}

struct ChunkStore {
    segments: Vec<Vec<u8>>,
    chunks: Vec<ChunkLocation>,
    identities: HashMap<[u8; 32], u32>,
    bytes: u64,
}

impl ChunkStore {
    fn new() -> Self {
        Self {
            segments: Vec::new(),
            chunks: Vec::new(),
            identities: HashMap::new(),
            bytes: 0,
        }
    }

    fn intern(&mut self, bytes: &[u8]) -> Result<u32, String> {
        if bytes.is_empty() || bytes.len() > SHARED_PROPS_CHUNK_BYTES {
            return Err("shared prop chunk has an invalid length".to_string());
        }
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        if let Some(index) = self.identities.get(&digest) {
            let location = self
                .chunks
                .get(usize_from_u32(*index)?)
                .ok_or_else(|| "shared prop interning location is missing".to_string())?;
            if chunk_slice(&self.segments, *location)? != bytes {
                return Err("shared prop chunk SHA-256 collision".to_string());
            }
            return Ok(*index);
        }
        if self.chunks.len() >= MAX_CHUNKS {
            return Err("shared props exceed the unique chunk limit".to_string());
        }
        self.bytes = self
            .bytes
            .checked_add(u64_from_usize(bytes.len())?)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| "shared props exceed the archive aggregate limit".to_string())?;
        let segment_limit = usize_from_u64(MAX_SHARED_PROPS_SEGMENT_BYTES)?;
        if self
            .segments
            .last()
            .is_none_or(|segment| segment.len().saturating_add(bytes.len()) > segment_limit)
        {
            if self.segments.len() >= MAX_ENTRIES {
                return Err("shared props exceed the archive entry count limit".to_string());
            }
            self.segments.push(Vec::new());
        }
        let segment_index = self.segments.len().saturating_sub(1);
        let segment = self
            .segments
            .last_mut()
            .ok_or_else(|| "shared prop segment is missing".to_string())?;
        let location = ChunkLocation {
            segment: segment_index,
            offset: segment.len(),
            length: bytes.len(),
        };
        segment.extend_from_slice(bytes);
        let index = u32_from_usize(self.chunks.len())?;
        self.chunks.push(location);
        let _previous = self.identities.insert(digest, index);
        Ok(index)
    }

    fn raw(&mut self, bytes: &[u8]) -> Result<Vec<u32>, String> {
        bytes
            .chunks(SHARED_PROPS_CHUNK_BYTES)
            .map(|chunk| self.intern(chunk))
            .collect()
    }

    fn vertex_planes(
        &mut self,
        vertices: &[u8],
        stride: usize,
        shuffle: bool,
    ) -> Result<VertexPlanes, String> {
        Ok(VertexPlanes {
            widths: attribute_widths(stride),
            streams: [
                self.plane(vertices, stride, 0..POSITION_BYTES, shuffle)?,
                self.plane(vertices, stride, POSITION_BYTES..COLOR_END, shuffle)?,
                self.plane(vertices, stride, COLOR_END..PREFIX_BYTES, shuffle)?,
                self.plane(vertices, stride, PREFIX_BYTES..stride, shuffle)?,
            ],
        })
    }

    fn plane(
        &mut self,
        vertices: &[u8],
        stride: usize,
        range: std::ops::Range<usize>,
        shuffle: bool,
    ) -> Result<Vec<u32>, String> {
        let mut refs = Vec::new();
        let mut buffer = Vec::with_capacity(SHARED_PROPS_CHUNK_BYTES);
        let tile_bytes = stride
            .checked_mul(SHUFFLE_VERTICES)
            .ok_or("shared prop tile overflows")?;
        if shuffle {
            for tile in vertices.chunks(tile_bytes) {
                for lane in range.clone() {
                    for vertex in tile.chunks_exact(stride) {
                        buffer.push(*vertex.get(lane).ok_or("shared prop plane is truncated")?);
                        if buffer.len() == SHARED_PROPS_CHUNK_BYTES {
                            refs.push(self.intern(&buffer)?);
                            buffer.clear();
                        }
                    }
                }
            }
        } else {
            for vertex in vertices.chunks_exact(stride) {
                for byte in vertex
                    .get(range.clone())
                    .ok_or("shared prop plane is truncated")?
                {
                    buffer.push(*byte);
                    if buffer.len() == SHARED_PROPS_CHUNK_BYTES {
                        refs.push(self.intern(&buffer)?);
                        buffer.clear();
                    }
                }
            }
        }
        if !buffer.is_empty() {
            refs.push(self.intern(&buffer)?);
        }
        Ok(refs)
    }
}

/// Factor current encoded prop records together, preserving every original byte.
///
/// Only a single bounded chunk staging buffer is used for a vertex plane. The
/// caller retains the original records; the returned physical segment bytes are
/// the unique stored content, rather than another complete copy per quality.
/// The archive writer must additionally charge its other entries against the
/// existing aggregate and entry-count caps.
///
/// # Errors
/// Rejects invalid original PMP1 data, unsupported versions, too many qualities,
/// hash collisions, or physical/recipe storage beyond the package bounds.
pub fn factor_props(records: &[&[u8]]) -> Result<FactoredProps, String> {
    factor_props_with_version(records, VERSION)
}

pub(crate) fn factor_props_with_version(
    records: &[&[u8]],
    version: u16,
) -> Result<FactoredProps, String> {
    if !matches!(version, ATTRIBUTE_VERSION | VERSION) {
        return Err("Unsupported shared prop writer version".into());
    }
    if records.is_empty() || records.len() > MAX_VARIANTS {
        return Err("shared props require a bounded nonempty quality record set".to_string());
    }
    let mut store = ChunkStore::new();
    let mut recipes = Vec::with_capacity(records.len());
    for bytes in records {
        let regions = super::props::vertex_regions(bytes)?;
        let mut parts =
            Vec::with_capacity(regions.len().saturating_mul(2).saturating_add(1).min(4096));
        let mut cursor = 0;
        for region in regions {
            let raw = bytes
                .get(cursor..region.start)
                .ok_or_else(|| "shared prop raw region is out of bounds".to_string())?;
            if !raw.is_empty() {
                parts.push(Part::Raw {
                    bytes: raw.len(),
                    chunks: store.raw(raw)?,
                });
            }
            let vertices = bytes
                .get(region.start..region.end)
                .ok_or_else(|| "shared prop vertex region is out of bounds".to_string())?;
            parts.push(Part::Vertices {
                count: vertices
                    .len()
                    .checked_div(region.stride)
                    .ok_or_else(|| "shared prop vertex stride is zero".to_string())?,
                stride: region.stride,
                planes: store.vertex_planes(vertices, region.stride, version == VERSION)?,
            });
            cursor = region.end;
        }
        let tail = bytes
            .get(cursor..)
            .ok_or_else(|| "shared prop raw tail is out of bounds".to_string())?;
        if !tail.is_empty() {
            parts.push(Part::Raw {
                bytes: tail.len(),
                chunks: store.raw(tail)?,
            });
        }
        recipes.push(PropsRecipe {
            version,
            expanded_bytes: u64_from_usize(bytes.len())?,
            sha256: hash::sha256_hex(bytes),
            segments: Vec::new(),
            chunks: Vec::new(),
            parts,
        });
    }
    let identities: Vec<SharedPropsSegment> = store
        .segments
        .iter()
        .map(|segment| {
            Ok(SharedPropsSegment {
                bytes: u64_from_usize(segment.len())?,
                sha256: hash::sha256_hex(segment),
            })
        })
        .collect::<Result<_, String>>()?;
    let mut encoded = Vec::with_capacity(recipes.len());
    let mut physical_bytes = store.bytes;
    for mut recipe in recipes {
        localize_recipe(&mut recipe, &store.chunks, &identities)?;
        let bytes = write_recipe(&recipe)?;
        physical_bytes = physical_bytes
            .checked_add(u64_from_usize(bytes.len())?)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| "shared prop recipes exceed the archive aggregate limit".to_string())?;
        encoded.push(bytes);
    }
    if store.segments.len().saturating_add(encoded.len()) > MAX_ENTRIES {
        return Err("shared prop recipes exceed the archive entry count limit".to_string());
    }
    for segment in &mut store.segments {
        segment.shrink_to_fit();
    }
    Ok(FactoredProps {
        recipes: encoded,
        segments: store.segments,
    })
}

fn localize_recipe(
    recipe: &mut PropsRecipe,
    shared_chunks: &[ChunkLocation],
    segments: &[SharedPropsSegment],
) -> Result<(), String> {
    let mut chunk_remap = HashMap::<u32, u32>::new();
    let mut segment_remap = HashMap::<usize, usize>::new();
    for part in &mut recipe.parts {
        for stream in part.streams_mut() {
            for reference in stream {
                if let Some(local) = chunk_remap.get(reference) {
                    *reference = *local;
                    continue;
                }
                let location = shared_chunks
                    .get(usize_from_u32(*reference)?)
                    .copied()
                    .ok_or_else(|| "shared prop chunk reference is missing".to_string())?;
                let local_segment = if let Some(local) = segment_remap.get(&location.segment) {
                    *local
                } else {
                    let local = recipe.segments.len();
                    recipe.segments.push(
                        segments
                            .get(location.segment)
                            .cloned()
                            .ok_or_else(|| "shared prop segment identity is missing".to_string())?,
                    );
                    let _previous = segment_remap.insert(location.segment, local);
                    local
                };
                let local_chunk = u32_from_usize(recipe.chunks.len())?;
                recipe.chunks.push(ChunkLocation {
                    segment: local_segment,
                    ..location
                });
                let _previous = chunk_remap.insert(*reference, local_chunk);
                *reference = local_chunk;
            }
        }
    }
    Ok(())
}

fn write_recipe(recipe: &PropsRecipe) -> Result<Vec<u8>, String> {
    let mut writer = Writer::new();
    writer.bytes(&MAGIC);
    writer.u16(recipe.version);
    writer.u64(recipe.expanded_bytes);
    writer.bytes(recipe.sha256.as_bytes());
    writer.u32(u32_from_usize(recipe.segments.len())?);
    for segment in &recipe.segments {
        writer.u64(segment.bytes);
        writer.bytes(segment.sha256.as_bytes());
    }
    writer.u32(u32_from_usize(recipe.chunks.len())?);
    for location in &recipe.chunks {
        writer.u16(
            u16::try_from(location.segment)
                .map_err(|error| format!("shared prop segment index is too large: {error}"))?,
        );
        writer.u32(u32_from_usize(location.offset)?);
        writer.u32(u32_from_usize(location.length)?);
    }
    writer.u32(u32_from_usize(recipe.parts.len())?);
    for part in &recipe.parts {
        match part {
            Part::Raw { bytes, chunks } => {
                writer.u8(0);
                writer.u64(u64_from_usize(*bytes)?);
                write_references(&mut writer, chunks);
            }
            Part::Vertices {
                count,
                stride,
                planes,
            } => {
                if planes.widths != attribute_widths(*stride) {
                    return Err(
                        "shared prop writer requires version 2 attribute planes".to_string()
                    );
                }
                writer.u8(1);
                writer.u8(u8::try_from(*stride)
                    .map_err(|error| format!("shared prop stride is too large: {error}"))?);
                writer.u32(u32_from_usize(*count)?);
                for stream in &planes.streams {
                    write_references(&mut writer, stream);
                }
            }
        }
        check_recipe_size(writer.len())?;
    }
    let mut bytes = writer.into_bytes();
    bytes.shrink_to_fit();
    Ok(bytes)
}

fn write_references(writer: &mut Writer, references: &[u32]) {
    for reference in references {
        writer.u32(*reference);
    }
}

/// Parse and fully bound a PPS1 recipe before fetching or allocating expanded data.
///
/// Segment identities are canonical lowercase SHA-256 strings. Every location,
/// stream length, reference count and expanded length must agree exactly; unused
/// chunks/segments and duplicate locations are rejected.
///
/// # Errors
/// Rejects truncated, oversized, unsupported or internally inconsistent recipes.
pub fn read_recipe(bytes: &[u8]) -> Result<PropsRecipe, String> {
    read_recipe_checked(bytes).map_err(RecipeReadError::into_message)
}

/// Validate compiler-emitted recipes, distinguishing a runtime budget limit
/// (which can retain literal storage) from malformed data (which remains an error).
pub(crate) fn recipe_fits_runtime_budget(bytes: &[u8]) -> Result<bool, String> {
    match read_recipe_checked(bytes) {
        Ok(_recipe) => Ok(true),
        Err(RecipeReadError::WorkingSetLimit) => Ok(false),
        Err(RecipeReadError::Malformed(error)) => Err(error),
    }
}

enum RecipeReadError {
    WorkingSetLimit,
    Malformed(String),
}

impl From<String> for RecipeReadError {
    fn from(error: String) -> Self {
        Self::Malformed(error)
    }
}

impl RecipeReadError {
    fn into_message(self) -> String {
        match self {
            Self::WorkingSetLimit => {
                "shared props reconstruction working set exceeds 1 GiB".to_string()
            }
            Self::Malformed(error) => error,
        }
    }
}

fn read_recipe_checked(bytes: &[u8]) -> Result<PropsRecipe, RecipeReadError> {
    check_recipe_size(bytes.len())?;
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != MAGIC {
        return Err("shared props recipe has an unsupported header"
            .to_string()
            .into());
    }
    let version = reader.u16()?;
    if !matches!(
        version,
        PREFIX_VERSION | ATTRIBUTE_VERSION | SHUFFLED_VERSION | VERSION
    ) {
        return Err("shared props recipe has an unsupported version"
            .to_string()
            .into());
    }
    let expanded_bytes = reader.u64()?;
    if !(10..=MAX_BINARY_BYTES).contains(&expanded_bytes) {
        return Err("shared props expanded record length exceeds its bounds"
            .to_string()
            .into());
    }
    let sha256 = read_hash(&mut reader)?;
    let segments = read_segments(&mut reader)?;
    // Bound simultaneous output, fetched segments and conservative decoded
    // recipe overhead before allocating chunk/part tables or fetching pixels.
    let working_bytes = segments
        .iter()
        .try_fold(expanded_bytes, |total, segment| {
            total.checked_add(segment.bytes)
        })
        .and_then(|total| {
            let tile = if version == SHUFFLED_VERSION {
                PREVIOUS_SHUFFLE_VERTICES
            } else if version == VERSION {
                SHUFFLE_VERTICES
            } else {
                0
            };
            total.checked_add(u64_from_usize(tile.saturating_mul(45)).ok()?)
        })
        .and_then(|total| {
            u64::try_from(bytes.len())
                .ok()
                .and_then(|wire| wire.checked_mul(16))
                .and_then(|metadata| total.checked_add(metadata))
        });
    if working_bytes.is_none_or(|total| total > MAX_TOTAL_BYTES) {
        return Err(RecipeReadError::WorkingSetLimit);
    }
    let chunks = read_chunks(&mut reader, &segments)?;
    let part_count = bounded_count(&mut reader, MAX_PARTS, 6, "shared prop part")?;
    if part_count == 0 {
        return Err("shared props recipe has no parts".to_string().into());
    }
    let mut parts = Vec::with_capacity(part_count.min(4096));
    let mut expanded = 0_u64;
    let mut used = vec![false; chunks.len()];
    for _ in 0..part_count {
        let part = read_part(&mut reader, &chunks, version)?;
        expanded = expanded
            .checked_add(u64_from_usize(part.bytes()?)?)
            .filter(|total| *total <= expanded_bytes)
            .ok_or_else(|| "shared props parts exceed their expanded record length".to_string())?;
        for stream in part.streams() {
            for reference in stream {
                let flag = used
                    .get_mut(usize_from_u32(*reference)?)
                    .ok_or_else(|| "shared prop chunk reference is out of bounds".to_string())?;
                *flag = true;
            }
        }
        parts.push(part);
    }
    if expanded != expanded_bytes || !reader.is_empty() || used.iter().any(|flag| !*flag) {
        return Err(
            "shared props recipe has a length mismatch or unused/trailing data"
                .to_string()
                .into(),
        );
    }
    Ok(PropsRecipe {
        version,
        expanded_bytes,
        sha256,
        segments,
        chunks,
        parts,
    })
}

fn read_segments(reader: &mut Reader<'_>) -> Result<Vec<SharedPropsSegment>, String> {
    let count = bounded_count(reader, MAX_ENTRIES, 72, "shared prop segment")?;
    if count == 0 {
        return Err("shared props recipe has no segments".to_string());
    }
    let mut segments = Vec::with_capacity(count);
    let mut identities = HashSet::new();
    let mut total = 0_u64;
    for _ in 0..count {
        let bytes = reader.u64()?;
        if !(1..=MAX_SHARED_PROPS_SEGMENT_BYTES).contains(&bytes) {
            return Err("shared prop segment length exceeds its bounds".to_string());
        }
        total = total
            .checked_add(bytes)
            .filter(|sum| *sum <= MAX_TOTAL_BYTES)
            .ok_or_else(|| "shared prop segments exceed the archive aggregate limit".to_string())?;
        let sha256 = read_hash(reader)?;
        if !identities.insert(sha256.clone()) {
            return Err("shared props recipe repeats a segment identity".to_string());
        }
        segments.push(SharedPropsSegment { bytes, sha256 });
    }
    Ok(segments)
}

fn read_chunks(
    reader: &mut Reader<'_>,
    segments: &[SharedPropsSegment],
) -> Result<Vec<ChunkLocation>, String> {
    let count = bounded_count(reader, MAX_CHUNKS, 10, "shared prop chunk")?;
    if count == 0 {
        return Err("shared props recipe has no chunks".to_string());
    }
    let mut chunks = Vec::with_capacity(count.min(4096));
    let mut locations = HashSet::new();
    let mut used_segments = vec![false; segments.len()];
    for _ in 0..count {
        let location = ChunkLocation {
            segment: usize::from(reader.u16()?),
            offset: usize_from_u32(reader.u32()?)?,
            length: usize_from_u32(reader.u32()?)?,
        };
        let segment = segments
            .get(location.segment)
            .ok_or_else(|| "shared prop chunk segment is out of bounds".to_string())?;
        let end = location
            .offset
            .checked_add(location.length)
            .ok_or_else(|| "shared prop chunk offset overflows".to_string())?;
        if location.length == 0
            || location.length > SHARED_PROPS_CHUNK_BYTES
            || u64_from_usize(end)? > segment.bytes
            || !locations.insert(location)
        {
            return Err("shared prop chunk location is invalid or repeated".to_string());
        }
        let used = used_segments
            .get_mut(location.segment)
            .ok_or_else(|| "shared prop chunk segment is out of bounds".to_string())?;
        *used = true;
        chunks.push(location);
    }
    if used_segments.iter().any(|flag| !*flag) {
        return Err("shared props recipe declares an unused segment".to_string());
    }
    Ok(chunks)
}

const fn attribute_widths(stride: usize) -> [usize; 4] {
    [
        POSITION_BYTES,
        COLOR_BYTES,
        UV_BYTES,
        stride.saturating_sub(PREFIX_BYTES),
    ]
}

fn read_part(
    reader: &mut Reader<'_>,
    chunks: &[ChunkLocation],
    version: u16,
) -> Result<Part, String> {
    match reader.u8()? {
        0 => {
            let bytes = reader.u64()?;
            if bytes == 0 || bytes > MAX_BINARY_BYTES {
                return Err("shared prop raw part length exceeds its bounds".to_string());
            }
            let length = usize_from_u64(bytes)?;
            Ok(Part::Raw {
                bytes: length,
                chunks: read_references(reader, chunks, length)?,
            })
        }
        1 => {
            let stride = usize::from(reader.u8()?);
            let count = usize_from_u32(reader.u32()?)?;
            if !matches!(stride, 43 | 45 | 69 | 55 | 57 | 81)
                || count == 0
                || u64_from_usize(count)? > super::MAX_PROP_BATCH_VERTICES
            {
                return Err("shared prop vertex part has an invalid stride/count".to_string());
            }
            let run_bytes = count
                .checked_mul(stride)
                .ok_or_else(|| "shared prop vertex run size overflows".to_string())?;
            if u64_from_usize(run_bytes)? > MAX_BINARY_BYTES {
                return Err("shared prop vertex part exceeds the expanded record limit".to_string());
            }
            let widths = if version == PREFIX_VERSION {
                [PREFIX_BYTES, 0, 0, stride.saturating_sub(PREFIX_BYTES)]
            } else {
                attribute_widths(stride)
            };
            let mut streams: [Vec<u32>; 4] = std::array::from_fn(|_| Vec::new());
            for (width, stream) in widths.iter().zip(&mut streams) {
                let bytes = count
                    .checked_mul(*width)
                    .ok_or_else(|| "shared prop vertex plane size overflows".to_string())?;
                *stream = read_references(reader, chunks, bytes)?;
            }
            Ok(Part::Vertices {
                count,
                stride,
                planes: VertexPlanes { widths, streams },
            })
        }
        other => Err(format!("shared prop part has unknown kind {other}")),
    }
}

fn read_references(
    reader: &mut Reader<'_>,
    chunks: &[ChunkLocation],
    bytes: usize,
) -> Result<Vec<u32>, String> {
    let count = bytes.div_ceil(SHARED_PROPS_CHUNK_BYTES);
    if count > reader.remaining().checked_div(4).unwrap_or(0) {
        return Err("shared prop reference stream is truncated".to_string());
    }
    let mut references = Vec::with_capacity(count);
    let mut remaining = bytes;
    for _ in 0..count {
        let reference = reader.u32()?;
        let location = chunks
            .get(usize_from_u32(reference)?)
            .ok_or_else(|| "shared prop chunk reference is out of bounds".to_string())?;
        let expected = remaining.min(SHARED_PROPS_CHUNK_BYTES);
        if location.length != expected {
            return Err(
                "shared prop reference stream has an inconsistent chunk length".to_string(),
            );
        }
        remaining = remaining.saturating_sub(expected);
        references.push(reference);
    }
    Ok(references)
}

/// Reconstruct one exact PMP1 record from fetched segments in recipe order.
///
/// All fetched lengths and segment SHA-256 identities are checked before the
/// single output allocation. The final PMP1 SHA-256 is verified before return;
/// callers then use the existing semantic PMP1 decoder and its allocation cap.
///
/// # Errors
/// Rejects missing/extra segments, wrong lengths or hashes, failed allocation,
/// out-of-range storage, or a reconstructed length/content mismatch.
pub fn reconstruct_props(recipe: &PropsRecipe, segments: &[&[u8]]) -> Result<Vec<u8>, String> {
    if segments.len() != recipe.segments.len() {
        return Err("shared props fetched segment count disagrees with its recipe".to_string());
    }
    for (bytes, descriptor) in segments.iter().zip(&recipe.segments) {
        if u64_from_usize(bytes.len())? != descriptor.bytes
            || hash::sha256_hex(bytes) != descriptor.sha256
        {
            return Err("shared prop segment length or SHA-256 mismatch".to_string());
        }
    }
    let length = usize_from_u64(recipe.expanded_bytes)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|error| format!("could not allocate bounded shared prop record: {error}"))?;
    for part in &recipe.parts {
        match part {
            Part::Raw { bytes, chunks } => {
                ChunkCursor::new(recipe, segments, chunks).append(&mut output, *bytes)?;
            }
            Part::Vertices {
                count,
                stride,
                planes,
            } => {
                let mut cursors = planes
                    .streams
                    .each_ref()
                    .map(|stream| ChunkCursor::new(recipe, segments, stream));
                if recipe.version >= SHUFFLED_VERSION {
                    restore_shuffled(
                        &mut output,
                        &mut cursors,
                        *count,
                        *stride,
                        planes.widths,
                        if recipe.version == SHUFFLED_VERSION {
                            PREVIOUS_SHUFFLE_VERTICES
                        } else {
                            SHUFFLE_VERTICES
                        },
                    )?;
                } else {
                    for _ in 0..*count {
                        for (cursor, width) in cursors.iter_mut().zip(planes.widths) {
                            cursor.append(&mut output, width)?;
                        }
                    }
                }
            }
        }
    }
    if output.len() != length || hash::sha256_hex(&output) != recipe.sha256 {
        return Err("shared props expanded record length or SHA-256 mismatch".to_string());
    }
    Ok(output)
}

fn restore_shuffled(
    output: &mut Vec<u8>,
    cursors: &mut [ChunkCursor<'_>; 4],
    vertex_count: usize,
    stride: usize,
    widths: [usize; 4],
    tile_vertices: usize,
) -> Result<(), String> {
    let mut plane = Vec::new();
    let mut remaining = vertex_count;
    while remaining > 0 {
        let count = remaining.min(tile_vertices);
        let start = output.len();
        let end = start
            .checked_add(
                count
                    .checked_mul(stride)
                    .ok_or("shared prop tile overflows")?,
            )
            .ok_or("shared prop output overflows")?;
        output.resize(end, 0);
        let restored = output
            .get_mut(start..end)
            .ok_or("shared prop tile is truncated")?;
        let mut offset = 0_usize;
        for (cursor, width) in cursors.iter_mut().zip(widths) {
            plane.clear();
            cursor.append(
                &mut plane,
                count
                    .checked_mul(width)
                    .ok_or("shared prop plane overflows")?,
            )?;
            for (index, vertex) in restored.chunks_exact_mut(stride).enumerate() {
                for (lane, byte) in vertex
                    .get_mut(offset..offset.saturating_add(width))
                    .ok_or("shared prop plane exceeds stride")?
                    .iter_mut()
                    .enumerate()
                {
                    *byte = *plane
                        .get(lane.saturating_mul(count).saturating_add(index))
                        .ok_or("shared prop plane is truncated")?;
                }
            }
            offset = offset.saturating_add(width);
        }
        remaining = remaining.saturating_sub(count);
    }
    Ok(())
}

struct ChunkCursor<'a> {
    recipe: &'a PropsRecipe,
    segments: &'a [&'a [u8]],
    references: &'a [u32],
    reference: usize,
    offset: usize,
}

impl<'a> ChunkCursor<'a> {
    const fn new(recipe: &'a PropsRecipe, segments: &'a [&'a [u8]], references: &'a [u32]) -> Self {
        Self {
            recipe,
            segments,
            references,
            reference: 0,
            offset: 0,
        }
    }

    fn append(&mut self, output: &mut Vec<u8>, mut bytes: usize) -> Result<(), String> {
        let output_end = output
            .len()
            .checked_add(bytes)
            .ok_or_else(|| "shared props output length overflows".to_string())?;
        if u64_from_usize(output_end)? > self.recipe.expanded_bytes {
            return Err("shared props output exceeds its expanded record length".to_string());
        }
        while bytes != 0 {
            let reference = self
                .references
                .get(self.reference)
                .ok_or_else(|| "shared prop chunk stream is truncated".to_string())?;
            let location = self
                .recipe
                .chunks
                .get(usize_from_u32(*reference)?)
                .ok_or_else(|| "shared prop chunk reference is out of bounds".to_string())?;
            let segment = self
                .segments
                .get(location.segment)
                .ok_or_else(|| "shared prop chunk segment is missing".to_string())?;
            let count = bytes.min(location.length.saturating_sub(self.offset));
            if count == 0 {
                return Err("shared prop chunk stream has an invalid cursor".to_string());
            }
            let start = location
                .offset
                .checked_add(self.offset)
                .ok_or_else(|| "shared prop chunk cursor overflows".to_string())?;
            let end = start
                .checked_add(count)
                .ok_or_else(|| "shared prop chunk cursor overflows".to_string())?;
            output.extend_from_slice(
                segment
                    .get(start..end)
                    .ok_or_else(|| "shared prop chunk bytes are truncated".to_string())?,
            );
            self.offset = self.offset.saturating_add(count);
            bytes = bytes.saturating_sub(count);
            if self.offset == location.length {
                self.reference = self.reference.saturating_add(1);
                self.offset = 0;
            }
        }
        Ok(())
    }
}

fn chunk_slice(segments: &[Vec<u8>], location: ChunkLocation) -> Result<&[u8], String> {
    let end = location
        .offset
        .checked_add(location.length)
        .ok_or_else(|| "shared prop chunk offset overflows".to_string())?;
    segments
        .get(location.segment)
        .and_then(|bytes| bytes.get(location.offset..end))
        .ok_or_else(|| "shared prop chunk is outside its segment".to_string())
}

fn bounded_count(
    reader: &mut Reader<'_>,
    max: usize,
    width: usize,
    label: &str,
) -> Result<usize, String> {
    let count = usize_from_u32(reader.u32()?)?;
    if count > max || count > reader.remaining().checked_div(width).unwrap_or(0) {
        return Err(format!(
            "{label} count exceeds its limit or available storage"
        ));
    }
    Ok(count)
}

fn read_hash(reader: &mut Reader<'_>) -> Result<String, String> {
    let bytes = reader.bytes(64)?;
    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err("shared props SHA-256 identity is not canonical lowercase hex".to_string());
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("shared props SHA-256 is invalid: {error}"))
}

fn check_recipe_size(bytes: usize) -> Result<(), String> {
    if u64_from_usize(bytes)? > MAX_SHARED_PROPS_RECIPE_BYTES {
        return Err("shared props recipe exceeds its storage limit".to_string());
    }
    Ok(())
}

fn u32_from_usize(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|error| format!("shared prop count/offset is too large: {error}"))
}

fn u64_from_usize(value: usize) -> Result<u64, String> {
    u64::try_from(value).map_err(|error| format!("shared prop size is too large: {error}"))
}

fn usize_from_u32(value: u32) -> Result<usize, String> {
    usize::try_from(value)
        .map_err(|error| format!("shared prop count/offset is too large: {error}"))
}

fn usize_from_u64(value: u64) -> Result<usize, String> {
    usize::try_from(value).map_err(|error| format!("shared prop size is too large: {error}"))
}

/// Loads a prepared props record, reconstructing shared payloads only when
/// the declared recipe magic is present. Current literal prop records are also supported.
///
/// # Errors
/// Rejects malformed recipes, missing/truncated segments, content mismatches,
/// and reconstructed records outside the established allocation bounds.
pub fn read_record<R: std::io::Read + std::io::Seek>(
    reader: &mut super::PackageReader<R>,
    name: &str,
) -> Result<Vec<u8>, String> {
    let bytes = reader.read_blob(name, MAX_BINARY_BYTES)?;
    if !bytes.starts_with(&MAGIC) {
        return Ok(bytes);
    }
    let recipe = read_recipe(&bytes)?;
    let mut segments = Vec::with_capacity(recipe.segments().len());
    for segment in recipe.segments() {
        let payload = reader.read_blob(&segment.blob_name(), segment.bytes)?;
        if u64_from_usize(payload.len())? != segment.bytes {
            return Err("shared prop segment length disagrees with its recipe".to_string());
        }
        segments.push(payload);
    }
    let views: Vec<&[u8]> = segments.iter().map(Vec::as_slice).collect();
    reconstruct_props(&recipe, &views)
}

#[cfg(test)]
#[path = "../tests/shared_props_attributes.rs"]
mod tests;
