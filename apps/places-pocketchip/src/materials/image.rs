//! PNG decoding, encoding and the session texture cache.
//!
//! Everything here is about bytes: turning a PNG on disk into an 8-bit RGBA
//! buffer, drawing the one diagnostic pattern every resolution failure shares,
//! and decoding each logical texture exactly once per session.

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;

use crate::assets::MAX_TEXTURE_DIMENSION;

/// Edge length of the generated missing-texture pattern, in texels.
const MISSING_TEXTURE_SIZE: u32 = 64;

/// Bytes per RGBA texel.
const RGBA_CHANNELS: usize = 4;

/// Bytes in one row of the generated missing-texture pattern.
const MISSING_TEXTURE_ROW_BYTES: usize = (MISSING_TEXTURE_SIZE as usize) * RGBA_CHANNELS;

/// Bytes of the generated missing-texture pattern (`MISSING_TEXTURE_SIZE`
/// squared, RGBA8).
const MISSING_TEXTURE_BYTES: usize = {
    let size = MISSING_TEXTURE_SIZE as usize;
    size * size * RGBA_CHANNELS
};

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
/// Mirror of [`decode_png`], used by the `LIMINAL_CAPTURE` developer path so a
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
    let size = MISSING_TEXTURE_SIZE;
    let mut rgba = vec![0u8; MISSING_TEXTURE_BYTES];
    for (row_index, row) in rgba
        .as_chunks_mut::<MISSING_TEXTURE_ROW_BYTES>()
        .0
        .iter_mut()
        .enumerate()
    {
        for (column_index, texel) in row
            .as_chunks_mut::<RGBA_CHANNELS>()
            .0
            .iter_mut()
            .enumerate()
        {
            // Two 8-texel checker cells; a cell is magenta when its row and
            // column parities agree, matching the old `(x / 8 + y / 8) % 2`.
            let checker = (column_index / 8) % 2 == (row_index / 8) % 2;
            let colour: [u8; 4] = if checker {
                [255, 0, 255, 255]
            } else {
                [24, 24, 24, 255]
            };
            texel.copy_from_slice(&colour);
        }
    }
    RawImage::new(size, size, rgba)
}

/// Session cache of decoded images, keyed by logical texture id.
///
/// The cache is what guarantees "one decode per texture per session": a level
/// that uses a texture in twenty rooms decodes it once, and switching back to a
/// level never touches the disk again. GPU textures are owned separately by the
/// renderer, which uploads each distinct entry once per level.
///
/// The cache also owns the *resident* size of those decoded images. A source
/// sheet may legally be up to [`crate::assets::MAX_TEXTURE_DIMENSION`], but the
/// runtime profile only ever uploads a smaller one, and the decoded original
/// would otherwise stay in RAM for the whole session next to the GPU copy it
/// was fitted into. On a device without swap the difference is the whole
/// texture set: 25 authored 1024x1024 sheets are 100 MiB of resident pixels,
/// their 256x256 fitted forms are 6 MiB. [`Self::insert`] therefore fits an
/// image to the loaded budget as it is cached, once, and keeps only the fitted
/// pixels — the upload path then finds nothing left to rescale.
#[derive(Default, Debug)]
pub struct TextureCache {
    images: HashMap<String, Rc<RawImage>>,
    decodes: usize,
    sheet_budget: Option<u32>,
}

impl TextureCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A cache that keeps every decoded sheet within `max_edge` texels.
    ///
    /// Used by the runtime, where [`crate::quality::QualityProfile::budget`]
    /// supplies the edge. `max_edge == 0` means "no limit", which is what a
    /// freshly built cache and the test fixtures do.
    #[must_use]
    pub fn with_sheet_budget(max_edge: u32) -> Self {
        Self {
            sheet_budget: (max_edge > 0).then_some(max_edge),
            ..Self::default()
        }
    }

    /// Inserts a freshly decoded image, returning the shared handle.
    ///
    /// The image is fitted to the cache's sheet budget before it is stored, so
    /// the resident copy is never larger than what the GPU will receive.
    pub fn insert(&mut self, key: impl Into<String>, image: RawImage) -> Rc<RawImage> {
        let image = match self.sheet_budget {
            Some(budget) => image.downscaled_to(budget).unwrap_or(image),
            None => image,
        };
        let image = Rc::new(image);
        self.images.insert(key.into(), Rc::clone(&image));
        self.decodes = self.decodes.saturating_add(1);
        image
    }

    /// The cached image for a key, if it was decoded before.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Rc<RawImage>> {
        self.images.get(key).map(Rc::clone)
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
        self.decodes = 0;
    }
}

#[cfg(test)]
mod tests;
