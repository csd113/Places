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
#[derive(Default, Debug)]
pub struct TextureCache {
    images: HashMap<String, Rc<RawImage>>,
    decodes: usize,
}

impl TextureCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a freshly decoded image, returning the shared handle.
    pub fn insert(&mut self, key: impl Into<String>, image: RawImage) -> Rc<RawImage> {
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
