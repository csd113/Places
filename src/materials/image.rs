//! PNG decoding, encoding and the session texture cache.
//!
//! Everything here is about bytes: turning a PNG on disk into an 8-bit RGBA
//! buffer, loading the diagnostic sheet every resolution failure shares,
//! and sharing decoded content within a level and bounded session retention.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use crate::assets::MAX_TEXTURE_DIMENSION;

/// Most CPUs one level's catalog-image prefetch may use.
///
/// Catalog PNGs are independent files, so decoding a few concurrently hides
/// most of the decode behind one worker. The cap keeps a level resolve inside
/// the shared CPU budget: the loading worker is already one thread, and the
/// main thread is presenting the preparation screen at the same time.
pub const MAX_LEVEL_DECODE_WORKERS: usize = 4;

/// Bytes per RGBA texel.
const RGBA_CHANNELS: usize = 4;

/// Expected byte length of an RGBA8 buffer of these dimensions.
///
/// `None` when the dimensions cannot describe a buffer this platform can
/// address (a 16-bit `usize` cannot hold the largest 8-bit PNG).
fn rgba_byte_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(RGBA_CHANNELS)
}

/// Decoded 8-bit RGBA image buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RawImage {
    #[must_use]
    pub const fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    /// The image's longest edge, in texels.
    #[must_use]
    pub const fn longest_edge(&self) -> u32 {
        if self.width > self.height {
            self.width
        } else {
            self.height
        }
    }

    /// Box-filters this image so both edges fit in `max_edge` texels.
    ///
    /// Deterministic: the scale factor is the smallest integer that fits, each
    /// output texel averages exactly the source texels whose centres fall under
    /// it, and channel values are integer-rounded. Power-of-two source edges
    /// scale by exact power-of-two factors, so a 1024x1024 sheet becomes an
    /// exact 4x4 average at 256 and a 2x2 average at 512.
    ///
    /// Returns `None` when the image already fits, so a caller can keep the
    /// decoded buffer it has instead of copying it. Callers downscale once, at
    /// load/upload time, and keep the result with the texture they uploaded;
    /// nothing here is meant to run per frame.
    #[must_use]
    pub fn downscaled_to(&self, max_edge: u32) -> Option<Self> {
        if max_edge == 0 || self.width == 0 || self.height == 0 {
            return None;
        }
        if self.longest_edge() <= max_edge {
            return None;
        }
        let factor = self.longest_edge().div_ceil(max_edge);
        let width = self.width.div_ceil(factor);
        let height = self.height.div_ceil(factor);
        let buffer_len = rgba_byte_len(width, height)?;
        let mut rgba = vec![0u8; buffer_len];
        for out_y in 0..height {
            let y0 = out_y.saturating_mul(factor);
            let y1 = y0.saturating_add(factor).min(self.height);
            for out_x in 0..width {
                let x0 = out_x.saturating_mul(factor);
                let x1 = x0.saturating_add(factor).min(self.width);
                let mut sums = [0u64; 4];
                let mut count = 0u64;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let Some(offset) = texel_offset(x, y, self.width) else {
                            continue;
                        };
                        let Some(texel) = self.rgba.get(offset..offset.saturating_add(4)) else {
                            continue;
                        };
                        for (sum, channel) in sums.iter_mut().zip(texel) {
                            *sum = sum.saturating_add(u64::from(*channel));
                        }
                        count = count.saturating_add(1);
                    }
                }
                if count == 0 {
                    continue;
                }
                let Some(offset) = texel_offset(out_x, out_y, width) else {
                    continue;
                };
                for (index, sum) in sums.iter().enumerate() {
                    let rounded = sum
                        .saturating_add(count / 2)
                        .checked_div(count)
                        .unwrap_or(0);
                    let value = u8::try_from(rounded.min(u64::from(u8::MAX))).unwrap_or(u8::MAX);
                    if let Some(slot) = rgba.get_mut(offset.saturating_add(index)) {
                        *slot = value;
                    }
                }
            }
        }
        Some(Self::new(width, height, rgba))
    }
}

/// Byte offset of texel `(x, y)` in a row-major RGBA8 buffer.
fn texel_offset(x: u32, y: u32, width: u32) -> Option<usize> {
    let column = usize::try_from(x).ok()?.checked_mul(RGBA_CHANNELS)?;
    let row = usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(width).ok()?)?;
    row.checked_mul(RGBA_CHANNELS)?.checked_add(column)
}

/// Encodes an 8-bit RGBA image as PNG bytes.
///
/// Mirror of [`decode_png`], used by the `PLACES_CAPTURE` developer path so a
/// rendered frame can be inspected on hardware without a screenshot tool.
/// # Errors
///
/// Returns a message when the image has a zero dimension or the PNG encoder
/// rejects the buffer.
pub fn encode_png(image: &RawImage) -> Result<Vec<u8>, String> {
    if image.width == 0 || image.height == 0 {
        return Err("cannot encode a zero-sized image".into());
    }
    if rgba_byte_len(image.width, image.height) != Some(image.rgba.len()) {
        return Err("image buffer length does not match its dimensions".into());
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("PNG header error: {error}"))?;
        writer
            .write_image_data(&image.rgba)
            .map_err(|error| format!("PNG encode error: {error}"))?;
    }
    Ok(out)
}

/// Decodes PNG bytes into an 8-bit RGBA raw image, validating the dimensions.
///
/// Any colour type the PNG specification allows is accepted: RGB, RGBA,
/// grayscale, grayscale+alpha and palette images (with or without `tRNS`) are
/// normalised to RGBA8, and 16-bit samples are stripped to 8 bits. Missing or
/// malformed data is an error, never a panic.
/// # Errors
///
/// Returns a message when the bytes are not a PNG, the image is empty, larger
/// than [`MAX_TEXTURE_DIMENSION`] on either edge, or the decoded buffer does
/// not match its declared size.
pub fn decode_png(bytes: &[u8]) -> Result<RawImage, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("not a PNG file (missing signature)".into());
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("invalid PNG: {e}"))?;
    let info = reader.info();
    let width = info.width;
    let height = info.height;

    if width == 0 || height == 0 {
        return Err("texture dimensions cannot be zero".into());
    }
    if width > MAX_TEXTURE_DIMENSION || height > MAX_TEXTURE_DIMENSION {
        return Err(format!(
            "texture dimensions {width}x{height} exceed the {MAX_TEXTURE_DIMENSION}x{MAX_TEXTURE_DIMENSION} limit"
        ));
    }

    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| "failed to size the PNG output buffer".to_string())?;
    let mut buf = vec![0; buf_size];
    let output_info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG decode error: {e}"))?;
    buf.truncate(output_info.buffer_size());

    let expected_len = rgba_byte_len(width, height);
    let rgba = match output_info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(expected_len.unwrap_or_default());
            for chunk in buf.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(expected_len.unwrap_or_default());
            for &g in &buf {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(expected_len.unwrap_or_default());
            for chunk in buf.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            rgba
        }
        png::ColorType::Indexed => {
            return Err("PNG palette was not expanded by the decoder".into());
        }
    };

    if Some(rgba.len()) != expected_len {
        return Err("decoded image buffer length does not match width * height * 4".into());
    }

    Ok(RawImage::new(width, height, rgba))
}

/// Reads and decodes a PNG below `root`, naming the file in every error.
/// # Errors
///
/// Returns a message when the file cannot be read or [`decode_png`] rejects it.
pub fn load_png_relative(root: &Path, relative: &str) -> Result<RawImage, String> {
    let path = root.join(relative);
    let bytes =
        fs::read(&path).map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    decode_png(&bytes).map_err(|error| format!("`{}`: {error}", path.display()))
}

/// The one conspicuous pattern a missing or corrupt texture resolves to.
///
/// A magenta/black checker is the classic "texture is broken" signal: it can
/// never be confused with authored content, so an authoring mistake is visible
/// in a capture instead of hidden behind a plausible-looking substitute.
#[must_use]
pub fn missing_texture() -> RawImage {
    super::BuiltinImage::MissingTexture.decode()
}

/// Session cache of decoded images, keyed by encoded content and source identity.
///
/// The cache shares a decode across a level and retains bounded prior images: a level
/// that uses a texture in twenty rooms decodes unchanged bytes once. Resolution
/// rereads files to detect content changes without relying on timestamps. GPU textures are owned separately by the
/// renderer, which uploads each distinct entry once per level.
#[derive(Default, Debug)]
pub struct TextureCache {
    images: HashMap<String, Arc<RawImage>>,
    decodes: usize,
    revisions: HashMap<String, String>,
}

impl TextureCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bounds optional retention before a new level resolves its images.
    /// No eviction happens during that level's material/fixture resolution.
    pub fn begin_level(&mut self) {
        self.trim_retained(256, 256 * 1024 * 1024);
    }

    fn trim_retained(&mut self, max_entries: usize, max_bytes: usize) {
        let mut bytes = self.images.values().fold(0_usize, |total, image| {
            total.saturating_add(image.rgba.capacity())
        });
        if self.images.len() <= max_entries && bytes <= max_bytes {
            return;
        }
        // Retire the largest optional allocations first, with stable ties.
        let mut candidates: Vec<_> = self
            .images
            .iter()
            .map(|(key, image)| (key.clone(), image.rgba.capacity()))
            .collect();
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (key, size) in candidates {
            if self.images.len() <= max_entries && bytes <= max_bytes {
                break;
            }
            drop(self.images.remove(&key));
            bytes = bytes.saturating_sub(size);
        }
        self.revisions
            .retain(|_, key| self.images.contains_key(key));
    }

    /// Inserts a freshly decoded image, returning the shared handle.
    pub fn insert(&mut self, key: impl Into<String>, image: RawImage) -> Arc<RawImage> {
        let shared_image = Arc::new(image);
        drop(self.images.insert(key.into(), Arc::clone(&shared_image)));
        self.decodes = self.decodes.saturating_add(1);
        shared_image
    }

    /// The cached image for a key, if it was decoded before.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Arc<RawImage>> {
        self.images
            .get(key)
            .or_else(|| {
                self.revisions
                    .get(key)
                    .and_then(|current| self.images.get(current))
            })
            .map(Arc::clone)
    }

    /// Reads the current encoded bytes before consulting the decoded cache.
    /// Resolution calls this per referenced image, never per rendered instance.
    /// # Errors
    /// Returns a read or PNG decode error, without substituting an older image.
    pub fn load_relative(
        &mut self,
        root: &Path,
        relative: &str,
        logical: &str,
    ) -> Result<(Arc<RawImage>, String), String> {
        let path = root.join(relative);
        let bytes = fs::read(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        self.decode_encoded(logical, &bytes)
            .map_err(|error| format!("`{}`: {error}", path.display()))
    }

    /// Reuses only identical encoded content and retires older cached revisions.
    /// Live tables keep their own Arc, so refreshing cannot mutate an old world.
    /// # Errors
    /// Returns the PNG decoder error for the current bytes.
    pub fn decode_encoded(
        &mut self,
        logical: &str,
        bytes: &[u8],
    ) -> Result<(Arc<RawImage>, String), String> {
        let key = texture_content_key(logical, bytes);
        if self
            .revisions
            .get(logical)
            .is_some_and(|previous| previous != &key)
            && let Some(previous) = self.revisions.remove(logical)
        {
            drop(self.images.remove(&previous));
        }
        if let Some(image) = self.images.get(&key) {
            return Ok((Arc::clone(image), key));
        }
        let decoded = decode_png(bytes)?;
        let image = self.insert(key.clone(), decoded);
        drop(self.revisions.insert(logical.to_string(), key.clone()));
        Ok((image, key))
    }

    /// Decodes a level's catalog images concurrently and inserts them in order.
    ///
    /// Every reference is read and content-keyed exactly like
    /// [`Self::load_relative`], so the later serial resolution pass re-reads
    /// the same bytes and finds the decoded image in the cache; an image whose
    /// file changed in between is decoded again by that pass, so a prefetch can
    /// never serve stale pixels. A reference that fails to read or decode
    /// inserts nothing: the serial resolver still reports the error with its
    /// own material context.
    ///
    /// `references` are `(logical key, path relative to `root`)` pairs in
    /// first-use order; duplicate logical keys decode once. Results are
    /// inserted in that order, so the cache state is deterministic regardless
    /// of which worker finishes first.
    pub fn prefetch_catalog(&mut self, root: &Path, references: &[(String, String)]) {
        let mut seen = HashSet::new();
        let jobs: Vec<&(String, String)> = references
            .iter()
            .filter(|(logical, _)| seen.insert(logical.as_str()))
            .collect();
        if jobs.is_empty() {
            return;
        }
        let workers = level_decode_workers(jobs.len());
        self.decode_references(root, &jobs, workers);
    }

    /// Decodes `jobs` with exactly `workers` threads (1 = serial reference) and
    /// inserts the results in job order.
    ///
    /// Each worker fills its own map; the parent then inserts every result in
    /// reference order, so the shared cache never depends on completion order.
    fn decode_references(&mut self, root: &Path, jobs: &[&(String, String)], workers: usize) {
        if workers <= 1 {
            for (logical, relative) in jobs {
                if let Ok((key, image)) = decode_relative_image(root, logical, relative) {
                    self.insert_prefetched(logical, key, image);
                }
            }
            return;
        }
        let chunk = jobs.len().div_ceil(workers);
        let mut decoded: HashMap<&str, Result<(String, RawImage), String>> = HashMap::new();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for chunk_jobs in jobs.chunks(chunk) {
                handles.push(scope.spawn(move || {
                    chunk_jobs
                        .iter()
                        .map(|(logical, relative)| {
                            (
                                logical.as_str(),
                                decode_relative_image(root, logical, relative),
                            )
                        })
                        .collect::<Vec<_>>()
                }));
            }
            for handle in handles {
                if let Ok(part) = handle.join() {
                    decoded.extend(part);
                }
            }
        });
        for (logical, _) in jobs {
            // A worker that panicked leaves its references out; the serial
            // resolver decodes them exactly as it would have without a
            // prefetch.
            if let Some(Ok((key, image))) = decoded.remove(logical.as_str()) {
                self.insert_prefetched(logical, key, image);
            }
        }
    }

    /// Inserts an already-decoded image under the same revision rules as
    /// [`Self::decode_encoded`].
    fn insert_prefetched(&mut self, logical: &str, key: String, image: RawImage) {
        if self
            .revisions
            .get(logical)
            .is_some_and(|previous| previous != &key)
            && let Some(previous) = self.revisions.remove(logical)
        {
            drop(self.images.remove(&previous));
        }
        if self.images.contains_key(&key) {
            return;
        }
        drop(self.insert(key.clone(), image));
        drop(self.revisions.insert(logical.to_string(), key));
    }

    /// Number of successful decodes this session (tests and diagnostics).
    #[must_use]
    pub const fn decoded_count(&self) -> usize {
        self.decodes
    }

    /// Number of cached images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// True when nothing has been decoded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Drops every cached image (developer tooling and tests).
    pub fn clear(&mut self) {
        self.images.clear();
        self.revisions.clear();
        self.decodes = 0;
    }
}

#[cfg(test)]
mod tests;

/// Widest useful parallel decode for `jobs` catalog images.
///
/// Bounded by the process-wide preparation budget when one is set (the offline
/// compiler stores its `--workers` there, so `--workers 1` stays serial), then
/// by [`MAX_LEVEL_DECODE_WORKERS`] and the job count. Tests stay serial: a unit
/// test asserts values, not throughput.
fn level_decode_workers(jobs: usize) -> usize {
    if cfg!(test) {
        return 1;
    }
    let budget = crate::perf::prepare_workers().unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .saturating_sub(1)
            .max(1)
    });
    jobs.clamp(1, MAX_LEVEL_DECODE_WORKERS).min(budget.max(1))
}

/// Reads and decodes one catalog PNG without touching a cache.
///
/// Returns the content key [`TextureCache::decode_encoded`] computes for the
/// same bytes, so the prefetched image is found by whichever resolution call
/// next reads those bytes.
fn decode_relative_image(
    root: &Path,
    logical: &str,
    relative: &str,
) -> Result<(String, RawImage), String> {
    let path = root.join(relative);
    let bytes =
        fs::read(&path).map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let key = texture_content_key(logical, &bytes);
    let image = decode_png(&bytes)?;
    Ok((key, image))
}

/// Encoded-content identity shared by CPU decoding and GPU texture lookup.
pub(super) fn texture_content_key(logical: &str, bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{logical}#png-v1-{hash:016x}")
}

#[cfg(test)]
mod content_revision_tests {
    use super::{RawImage, TextureCache, encode_png, texture_content_key};
    use std::sync::Arc;

    #[test]
    fn encoded_changes_replace_cache_revision_without_mutating_old_owner() -> Result<(), String> {
        let first = encode_png(&RawImage::new(1, 1, vec![1, 2, 3, 255]))?;
        let second = encode_png(&RawImage::new(1, 1, vec![7, 8, 9, 255]))?;
        let mut cache = TextureCache::new();
        let (old, old_key) = cache.decode_encoded("same", &first)?;
        let (reused, same_key) = cache.decode_encoded("same", &first)?;
        assert!(Arc::ptr_eq(&old, &reused));
        assert_eq!(old_key, same_key);
        let (current, new_key) = cache.decode_encoded("same", &second)?;
        assert_ne!(old_key, new_key);
        assert_eq!(new_key, texture_content_key("same", &second));
        assert_eq!(current.rgba, [7, 8, 9, 255]);
        assert_eq!(old.rgba, [1, 2, 3, 255]);
        assert!(cache.get(&old_key).is_none());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.decoded_count(), 2);
        assert!(cache.decode_encoded("same", b"broken PNG").is_err());
        assert!(cache.get("same").is_none());
        assert_eq!(current.rgba, [7, 8, 9, 255]);
        Ok(())
    }

    #[test]
    fn same_pack_path_with_new_bytes_uses_new_cpu_and_gpu_identity() -> Result<(), String> {
        let first = encode_png(&RawImage::new(1, 1, vec![10, 20, 30, 255]))?;
        let second = encode_png(&RawImage::new(1, 1, vec![30, 20, 10, 255]))?;
        let pack = |bytes: Vec<u8>| {
            crate::materials::PackMaterials::new(
                "same.zip",
                None,
                std::collections::HashMap::from([(
                    "textures/wall.png".to_string(),
                    Arc::<[u8]>::from(bytes),
                )]),
            )
        };
        let original = pack(first);
        let updated = pack(second);
        let mut cache = TextureCache::new();
        let old = original.decode_texture(&mut cache, "textures/wall.png")?;
        let new = updated.decode_texture(&mut cache, "textures/wall.png")?;
        assert_ne!(
            original.cache_key("textures/wall.png"),
            updated.cache_key("textures/wall.png")
        );
        assert_ne!(old.rgba, new.rgba);
        assert_eq!(cache.len(), 1);
        Ok(())
    }

    #[test]
    fn replacing_file_bytes_refreshes_without_timestamp_identity() -> Result<(), String> {
        let root =
            std::env::temp_dir().join(format!("places-texture-content-{}", std::process::id()));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join("source.png");
        let run = (|| {
            let first = encode_png(&RawImage::new(1, 1, vec![1, 2, 3, 255]))?;
            let second = encode_png(&RawImage::new(1, 1, vec![7, 8, 9, 255]))?;
            std::fs::write(&path, &first).map_err(|error| error.to_string())?;
            let modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .map_err(|error| error.to_string())?;
            let mut cache = TextureCache::new();
            let (_, before) = cache.load_relative(&root, "source.png", "logical")?;
            std::fs::write(&path, &second).map_err(|error| error.to_string())?;
            std::fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|file| file.set_times(std::fs::FileTimes::new().set_modified(modified)))
                .map_err(|error| error.to_string())?;
            let (image, after) = cache.load_relative(&root, "source.png", "logical")?;
            assert_ne!(before, after);
            assert_eq!(image.rgba, [7, 8, 9, 255]);
            Ok(())
        })();
        crate::test_support::remove_dir_if_present(root);
        run
    }
}
