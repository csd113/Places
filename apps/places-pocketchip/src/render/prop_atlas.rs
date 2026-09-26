//! The one-texture prop albedo atlas.
//!
//! The prop pass used to cost one texture upload and one material change per
//! model, and on the target device those submissions dominate the frame's CPU
//! time. This module packs every model's albedo into one 1024-texel sheet so a
//! whole level's prop geometry draws with a single texture — and, when every
//! model fits one buffer chunk, a single `draw_elements` (see `MeshPacker` and
//! `Renderer::pack_prop_batches`; a level whose props spill past the 16-bit
//! index space still issues one submission per buffer chunk).
//!
//! Layout
//! ------
//! * the sheet is 1024 x 1024 RGBA8;
//! * each cell is 144 texels on a side (`CELL_STRIDE`), so 7 x 7 = 49 cells
//!   fit (`7 * 144 = 1008 <= 1024`);
//! * the content area is 128 x 128 texels (`CONTENT_EDGE`), inset by an
//!   8-texel gutter (`GUTTER`) on every side;
//! * a source image is copied 1:1 into the top-left of the content area when it
//!   is smaller than 128, and box-filtered down to at most 128 when it is
//!   larger — the same filter the shipping profile applies to a prop sheet, and
//!   the content edge for every profile because the cell layout is fixed;
//! * before upload every texel of a cell — content, the unused remainder of the
//!   content area and the gutter — is filled with the nearest content texel, so
//!   a bilinear or mip sample that strays outside the image reads the cell's own
//!   edge colour rather than a neighbour's artwork (or transparent black).
//!
//! Mip safety
//! ----------
//! The atlas keeps its mip chain, so the profile's trilinear filtering still
//! applies, but it is sampled no deeper than level [`MAX_MIP_LEVEL`]. Mip level
//! `L` halves every distance, so a cell's 8-texel gutter is `8 / 2^L` texels
//! wide at level `L`: 8, 4, 2 and 1 texels at levels 0, 1, 2 and 3, and half a
//! texel at level 4. Bilinear sampling reaches at most one texel outside the
//! sampled point, so down to level 3 a sample can never leave the cell's own
//! edge-replicated border; at level 4 the gutter is narrower than the filter
//! footprint and a neighbour's content could bleed in. `GL_TEXTURE_MAX_LEVEL`
//! is set to 3 for exactly that reason.
//!
//! The `LIMINAL_PROP_ATLAS` switch
//! -------------------------------
//! `LIMINAL_PROP_ATLAS=0` (or `false`/`off`) disables the atlas for every
//! subsequent level build, keeping the historical per-model texture path.
//! Unset — the default — enables it. It is read once per level build, like the
//! other `LIMINAL_*` startup overrides, never per frame.

use crate::loader::RawImage;

/// Edge length of the atlas sheet, in texels.
pub const ATLAS_EDGE: u32 = 1024;

/// Distance from one cell's origin to the next, in texels.
pub const CELL_STRIDE: u32 = 144;

/// Cells along one atlas edge.
pub const CELL_COLUMNS: u32 = 7;

/// Cells the atlas holds: 7 x 7. The last cell's far gutter corner lands at
/// `6 * 144 + 8 + 128 = 1000`, inside [`ATLAS_EDGE`].
pub const CELL_COUNT: usize = (CELL_COLUMNS as usize) * (CELL_COLUMNS as usize);

/// Texels of edge-replicated padding on every side of a cell's content.
pub const GUTTER: u32 = 8;

/// Edge length of a cell's content area, in texels.
pub const CONTENT_EDGE: u32 = 128;

/// Deepest mip level the uploaded atlas may be sampled at.
///
/// See the module docs: the gutter is `GUTTER / 2^L` texels at level `L`, and
/// level 3 (gutter 1) is the last one where a bilinear footprint stays inside
/// the cell.
pub const MAX_MIP_LEVEL: u32 = 3;

/// Bytes per RGBA texel.
const RGBA_CHANNELS: usize = 4;

/// Environment switch that disables the atlas when set to a falsy value.
///
/// Read once per level build through [`enabled`]; unset keeps the atlas on.
pub const ENABLE_ENV: &str = "LIMINAL_PROP_ATLAS";

/// Whether this level build should atlas prop albedos.
///
/// Follows the shared `LIMINAL_*` rule: an unset variable is the default
/// (here: on), and an explicit empty, `0`, `false` or `off` value is off.
#[must_use]
pub fn enabled() -> bool {
    std::env::var(ENABLE_ENV).map_or(true, |value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off"
        )
    })
}

/// The single albedo slot a model can be baked into one cell through.
///
/// A model shares one cell, so it must sample exactly one albedo image. This
/// answers `None` — meaning "leave the model on its own per-model texture
/// path" — when a submesh:
///
/// * carries an emissive mask: the mask is bound as its own sampler per draw,
///   so it cannot share the atlas draw's surface state;
/// * samples a different albedo slot than a sibling: two cells would be needed
///   and one vertex stream cannot carry two remaps;
/// * samples no albedo at all: the draw binds the shared white sheet, which the
///   atlas texture must not silently replace (it would tint the material).
#[must_use]
pub fn albedo_slot(model: &crate::gltf::PropModel) -> Option<u16> {
    let mut slot: Option<u16> = None;
    for submesh in &model.submeshes {
        if submesh.emission.mask.is_some() {
            return None;
        }
        let texture = submesh.texture?;
        match slot {
            None => slot = Some(texture),
            Some(existing) if existing == texture => {}
            Some(_) => return None,
        }
    }
    slot
}

/// Where one model's fitted albedo sits inside the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasPlacement {
    /// Atlas texel column of the content's top-left corner, gutter included.
    pub origin_x: u32,
    /// Atlas texel row of the content's top-left corner, gutter included.
    pub origin_y: u32,
    /// Fitted content width, in texels.
    pub width: u32,
    /// Fitted content height, in texels.
    pub height: u32,
}

impl AtlasPlacement {
    /// Maps a model UV into this placement's slice of the atlas.
    ///
    /// The source UV is clamped to the unit square first: the prop reader
    /// accepts a hair outside `0..1` for floating-point noise, and an
    /// out-of-range value must sample the edge texel (what `CLAMP_TO_EDGE` did
    /// for the model's own texture) rather than a neighbouring cell. Small
    /// images map onto their real fitted size, not the full 128-texel content
    /// area.
    #[must_use]
    pub fn remap_uv(self, uv: [f32; 2]) -> [f32; 2] {
        let u = uv[0]
            .clamp(0.0, 1.0)
            .mul_add(pixel_to_f32(self.width), pixel_to_f32(self.origin_x));
        let v = uv[1]
            .clamp(0.0, 1.0)
            .mul_add(pixel_to_f32(self.height), pixel_to_f32(self.origin_y));
        [u / pixel_to_f32(ATLAS_EDGE), v / pixel_to_f32(ATLAS_EDGE)]
    }
}

/// One level's prop albedo atlas: the pixels and the placements baked into it.
///
/// The renderer uploads this once per level and drops the CPU pixels with the
/// level's prop batches, so a 4 MiB staging buffer is never kept resident next
/// to the GPU copy it was uploaded from.
pub struct PropAtlas {
    /// Row-major RGBA8 pixels, [`ATLAS_EDGE`] squared. Empty until the first
    /// placement, so a level with nothing to atlas never allocates it.
    pixels: Vec<u8>,
    /// Every filled cell, in placement order.
    placements: Vec<AtlasPlacement>,
    /// Wall-clock cost of fitting and copying the placed images, in ms.
    build_millis: f64,
}

impl Default for PropAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl PropAtlas {
    /// An empty atlas: no pixels allocated until [`Self::place`] succeeds.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pixels: Vec::new(),
            placements: Vec::new(),
            build_millis: 0.0,
        }
    }

    /// Places one model's image in the next free cell.
    ///
    /// Returns `None` when the atlas is full or the image cannot be made
    /// usable (zero-sized, or a buffer that does not match its dimensions),
    /// which makes the caller keep the model on its own texture. A malformed
    /// image is never placed, so a remapped UV can never point at nothing.
    pub fn place(&mut self, image: &RawImage) -> Option<AtlasPlacement> {
        if self.placements.len() >= CELL_COUNT {
            return None;
        }
        if image.width == 0 || image.height == 0 {
            return None;
        }
        // Fit the source to the content edge. `downscaled_to(128)` is the same
        // box filter the shipping profile applies to a prop sheet, so under
        // `Low` the atlas holds exactly the pixels the per-model texture
        // would have; an image that already fits is copied 1:1 and its UVs are
        // remapped by its real size instead of the content edge.
        let fitted;
        let source = if image.longest_edge() > CONTENT_EDGE {
            fitted = image.downscaled_to(CONTENT_EDGE)?;
            &fitted
        } else {
            image
        };
        let width = source.width;
        let height = source.height;
        if width == 0 || height == 0 || width > CONTENT_EDGE || height > CONTENT_EDGE {
            return None;
        }
        let row_bytes = usize::try_from(width).ok()?.checked_mul(RGBA_CHANNELS)?;
        let pixel_bytes = usize::try_from(height).ok()?.checked_mul(row_bytes)?;
        if source.rgba.len() != pixel_bytes {
            return None;
        }

        let cell = self.placements.len();
        let column = u32::try_from(cell).ok()? % CELL_COLUMNS;
        let row = u32::try_from(cell).ok()? / CELL_COLUMNS;
        let origin_x = column.checked_mul(CELL_STRIDE)?.checked_add(GUTTER)?;
        let origin_y = row.checked_mul(CELL_STRIDE)?.checked_add(GUTTER)?;

        if self.pixels.is_empty() {
            self.pixels = vec![0; ATLAS_BYTES];
        }
        for image_row in 0..height {
            let source_start = image_byte_offset(0, image_row, width)?;
            let source_row = source
                .rgba
                .get(source_start..source_start.checked_add(row_bytes)?)?;
            let destination_start = texel_byte_offset(origin_x, origin_y.checked_add(image_row)?)?;
            let destination = self
                .pixels
                .get_mut(destination_start..destination_start.checked_add(row_bytes)?)?;
            destination.copy_from_slice(source_row);
        }

        // Replicate the image's edge over the whole cell: the 8-texel gutter,
        // and (for an image smaller than 128) the remainder of the content
        // area. Every texel a mip chain can average or a filter can reach then
        // holds this cell's own edge colour.
        let cell_x = column.checked_mul(CELL_STRIDE)?;
        let cell_y = row.checked_mul(CELL_STRIDE)?;
        let last_source_x = width.saturating_sub(1);
        let last_source_y = height.saturating_sub(1);
        for y in cell_y..cell_y.saturating_add(CELL_STRIDE) {
            let source_y = y.saturating_sub(origin_y).min(last_source_y);
            for x in cell_x..cell_x.saturating_add(CELL_STRIDE) {
                let source_x = x.saturating_sub(origin_x).min(last_source_x);
                let source_start = image_byte_offset(source_x, source_y, width)?;
                let source_pixel = source
                    .rgba
                    .get(source_start..source_start.checked_add(RGBA_CHANNELS)?)?;
                let destination_start = texel_byte_offset(x, y)?;
                let destination = self
                    .pixels
                    .get_mut(destination_start..destination_start.checked_add(RGBA_CHANNELS)?)?;
                destination.copy_from_slice(source_pixel);
            }
        }

        let placement = AtlasPlacement {
            origin_x,
            origin_y,
            width,
            height,
        };
        self.placements.push(placement);
        Some(placement)
    }

    /// The RGBA8 pixels, [`ATLAS_EDGE`] squared; empty before the first
    /// placement.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Every filled cell, in placement order.
    #[must_use]
    pub fn placements(&self) -> &[AtlasPlacement] {
        &self.placements
    }

    /// Cells filled so far.
    #[must_use]
    pub const fn cells_used(&self) -> usize {
        self.placements.len()
    }

    /// Records the wall-clock cost of building the atlas, for the level log.
    pub const fn record_build_millis(&mut self, millis: f64) {
        self.build_millis = millis;
    }

    /// Wall-clock cost of fitting and copying every placed image, in ms.
    #[must_use]
    pub const fn build_millis(&self) -> f64 {
        self.build_millis
    }
}

// Printing the pixel buffer would flood a log line with four million bytes, so
// the summary reports its size instead.
impl std::fmt::Debug for PropAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PropAtlas")
            .field("pixels", &format_args!("{} byte(s)", self.pixels.len()))
            .field("placements", &self.placements.len())
            .field("build_millis", &self.build_millis)
            .finish()
    }
}

/// Bytes of the full [`ATLAS_EDGE`] square RGBA8 sheet.
const ATLAS_BYTES: usize = (ATLAS_EDGE as usize) * (ATLAS_EDGE as usize) * RGBA_CHANNELS;

/// Byte offset of texel `(x, y)` in a `width`-wide RGBA8 image.
fn image_byte_offset(x: u32, y: u32, width: u32) -> Option<usize> {
    usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(width).ok()?)?
        .checked_add(usize::try_from(x).ok()?)?
        .checked_mul(RGBA_CHANNELS)
}

/// Byte offset of texel `(x, y)` in the atlas sheet.
fn texel_byte_offset(x: u32, y: u32) -> Option<usize> {
    image_byte_offset(x, y, ATLAS_EDGE)
}

/// Converts a bounded atlas coordinate to `f32`.
///
/// Every value converted here is at most [`ATLAS_EDGE`] (1024), far below
/// 2^24 where `f32` is exact; clippy cannot see that bound.
#[allow(clippy::cast_precision_loss)]
const fn pixel_to_f32(value: u32) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests;
